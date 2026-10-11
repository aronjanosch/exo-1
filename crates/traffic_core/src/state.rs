//! Airspace, speeding, the tower's answers and clearances (#226). Reads `Hailed`, `TimePassed`,
//! `HeatStars` and `Landed`; raises the airspace, clearance, violation and grade events and the
//! wallet fines. Everything random comes from the seeded generator in the state.
use std::collections::BTreeMap;

use gameplay_core::content::WALLET;
use gameplay_core::notice::{Notice, NoticeKind};
use gameplay_core::rng::Rng;
use gameplay_core::save::{Envelope, SaveError};
use gameplay_core::{ClientId, Content, Event, LocationId, TextKey, TownId, TrackId, ViolationKind, WorldEvent};
use serde::{Deserialize, Serialize};

use crate::town::{Town, TrafficContent, town_of_pad};

pub const SECTION: &str = "traffic";
pub const SECTION_VERSION: u32 = 1;

pub enum Outcome {
    /// A domain event for every system; the host gives it an id and applies it.
    Emit(WorldEvent),
    Notice(Notice),
    /// A radio line of the town's tower (a key of its voice pool; the glue picks the line).
    Say { town: TownId, key: TextKey },
}

/// A clearance: the pad to land on and the time left in the window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clearance {
    pub town: TownId,
    pub pad: LocationId,
    pub time_left_s: f64,
    pub inspection: bool,
}

/// An answer the tower owes: due in `due_s`. `second`: the answer after a holding pattern, which
/// always grants.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub town: TownId,
    pub due_s: f64,
    pub second: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Pilot {
    inside: Option<TownId>,
    /// Seconds above the speed limit in the current airspace.
    over_s: f64,
    /// Seconds until another speeding violation may be raised.
    cooldown_s: f64,
    clearance: Option<Clearance>,
    pending: Option<Pending>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Traffic {
    rng: Rng,
    /// The crew's heat stars, the last seen (0..=5).
    stars: u8,
    pilots: BTreeMap<ClientId, Pilot>,
}

impl Traffic {
    pub fn new(seed: u64) -> Traffic {
        Traffic { rng: Rng::new(seed), stars: 0, pilots: BTreeMap::new() }
    }

    /// The glue's call every step: `inside` is the town whose airspace the ship is in now.
    /// Raises the airspace changes, and speeding after the grace time (at most once per cooldown),
    /// never in a free port.
    pub fn observe(&mut self, tc: &TrafficContent, player: ClientId, inside: Option<TownId>, speed_ms: f64, dt: f64) -> Vec<Outcome> {
        let tuning = &tc.tuning.record;
        let p = self.pilots.entry(player).or_default();
        let mut out = Vec::new();
        if p.inside != inside {
            if let Some(old) = p.inside.take() {
                out.push(Outcome::Emit(WorldEvent::AirspaceLeft { town: old }));
            }
            if let Some(new) = &inside {
                out.push(Outcome::Emit(WorldEvent::AirspaceEntered { town: new.clone() }));
            }
            p.inside = inside.clone();
            p.over_s = 0.0;
        }
        p.cooldown_s = (p.cooldown_s - dt).max(0.0);
        // No speeding rules in a free port, nor outside every town's airspace.
        let limit = inside.as_ref().and_then(|t| tc.towns.get(t)).filter(|t| !t.record.free_port);
        match limit {
            Some(town) if speed_ms > town.record.speed_limit_ms => {
                let id = &town.record.id;
                p.over_s += dt;
                if p.over_s > tuning.speeding_grace_s && p.cooldown_s <= 0.0 {
                    p.cooldown_s = tuning.speeding_cooldown_s;
                    out.push(Outcome::Emit(WorldEvent::Violation { town: id.clone(), kind: ViolationKind::Speeding }));
                    out.push(Outcome::Emit(wallet(-tuning.fine_speeding)));
                    out.push(say(id, &town.record, "violation.speeding"));
                }
            }
            _ => p.over_s = 0.0,
        }
        out
    }

    /// Applies a world event. Only `Hailed`, `TimePassed`, `HeatStars` and `Landed` matter here.
    pub fn apply_world(&mut self, tc: &TrafficContent, _kernel: &Content, ev: &Event<WorldEvent>) -> Vec<Outcome> {
        let player = ev.sender();
        match &ev.payload {
            WorldEvent::Hailed { town } => self.hail(tc, player, town),
            WorldEvent::TimePassed { dt } => self.time(tc, *dt),
            WorldEvent::HeatStars { stars } => {
                self.stars = *stars;
                Vec::new()
            }
            WorldEvent::Landed { at: Some(at), speed } => self.landed(tc, player, at, *speed),
            _ => Vec::new(),
        }
    }

    fn hail(&mut self, tc: &TrafficContent, player: ClientId, town: &TownId) -> Vec<Outcome> {
        let Some(t) = tc.towns.get(town).map(|t| &t.record) else { return Vec::new() };
        let p = self.pilots.entry(player).or_default();
        if p.inside.as_ref() != Some(town) {
            return vec![notice("notice.traffic.out_of_range"), say(town, t, "out_of_range")];
        }
        if t.free_port {
            return vec![notice("notice.traffic.no_tower"), say(town, t, "no_tower")];
        }
        if let Some(c) = p.clearance.as_ref().filter(|c| &c.town == town) {
            // Hailing again while cleared repeats the clearance as it stands.
            let line = if c.inspection { "inspection" } else { "granted" };
            let repeat = WorldEvent::ClearanceGranted { town: town.clone(), pad: c.pad.clone(), window_s: c.time_left_s, inspection: c.inspection };
            return vec![Outcome::Emit(repeat), say(town, t, line)];
        }
        p.pending = Some(Pending { town: town.clone(), due_s: tc.tuning.record.answer_delay_s, second: false });
        Vec::new()
    }

    fn time(&mut self, tc: &TrafficContent, dt: f64) -> Vec<Outcome> {
        let mut out = Vec::new();
        for p in self.pilots.values_mut() {
            // The window runs down; a clearance granted in this step starts counting next step.
            if let Some(c) = p.clearance.as_mut() {
                c.time_left_s -= dt;
            }
            if let Some(c) = p.clearance.take_if(|c| c.time_left_s <= 0.0) {
                out.push(Outcome::Emit(WorldEvent::ClearanceExpired { town: c.town.clone() }));
                out.extend(tc.towns.get(&c.town).map(|t| say(&c.town, &t.record, "expired")));
            }
            // The tower's answer: taken out, counted down, and given when it is due.
            if let Some(mut pend) = p.pending.take() {
                pend.due_s -= dt;
                if pend.due_s > 0.0 {
                    p.pending = Some(pend);
                } else {
                    answer(tc, &mut self.rng, self.stars, p, pend, &mut out);
                }
            }
        }
        out
    }

    fn landed(&mut self, tc: &TrafficContent, player: ClientId, at: &LocationId, speed: f64) -> Vec<Outcome> {
        let Some(town) = town_of_pad(tc, at).cloned() else { return Vec::new() };
        let Some(t) = tc.towns.get(&town).map(|t| &t.record) else { return Vec::new() };
        if t.free_port {
            return Vec::new();
        }
        let tuning = &tc.tuning.record;
        let grade = grade_of(speed, tuning.grade_speeds);
        let p = self.pilots.entry(player).or_default();
        let cleared = p.clearance.as_ref().filter(|c| c.town == town).map(|c| c.pad.clone());
        match cleared {
            Some(pad) if &pad == at => {
                p.clearance = None;
                vec![
                    Outcome::Emit(WorldEvent::LandingGraded { town: town.clone(), at: at.clone(), grade, on_time: true }),
                    say(&town, t, &format!("graded.{grade}")),
                ]
            }
            Some(_) => vec![
                Outcome::Emit(WorldEvent::Violation { town: town.clone(), kind: ViolationKind::WrongPad }),
                Outcome::Emit(wallet(-tuning.fine_wrong_pad)),
                Outcome::Emit(WorldEvent::LandingGraded { town: town.clone(), at: at.clone(), grade, on_time: false }),
                say(&town, t, "violation.wrong_pad"),
            ],
            None => vec![
                Outcome::Emit(WorldEvent::Violation { town: town.clone(), kind: ViolationKind::NoClearance }),
                Outcome::Emit(wallet(-tuning.fine_no_clearance)),
                Outcome::Emit(WorldEvent::LandingGraded { town: town.clone(), at: at.clone(), grade, on_time: false }),
                say(&town, t, "violation.no_clearance"),
            ],
        }
    }

    /// The clearance of a pilot, if any.
    pub fn clearance(&self, player: ClientId) -> Option<&Clearance> {
        self.pilots.get(&player).and_then(|p| p.clearance.as_ref())
    }

    /// The tower's answer a pilot is waiting for, if any.
    pub fn pending(&self, player: ClientId) -> Option<&Pending> {
        self.pilots.get(&player).and_then(|p| p.pending.as_ref())
    }

    /// The town whose airspace a pilot is in, as last observed.
    pub fn inside(&self, player: ClientId) -> Option<&TownId> {
        self.pilots.get(&player).and_then(|p| p.inside.as_ref())
    }

    /// The crew's last seen heat stars.
    pub fn heat_stars(&self) -> u8 {
        self.stars
    }

    pub fn save(&self, env: &mut Envelope) {
        env.put(SECTION, SECTION_VERSION, self);
    }

    /// None if the save has no traffic section; an error for a version this build does not read.
    pub fn load(env: &Envelope) -> Result<Option<Traffic>, SaveError> {
        env.get(SECTION, SECTION_VERSION)
    }
}

/// The tower's answer when it is due: a hold (once) or a grant.
fn answer(tc: &TrafficContent, rng: &mut Rng, stars: u8, p: &mut Pilot, pend: Pending, out: &mut Vec<Outcome>) {
    let Some(t) = tc.towns.get(&pend.town).map(|t| &t.record) else { return };
    let tuning = &tc.tuning.record;
    if !pend.second && rng.unit() < t.hold_chance {
        out.push(Outcome::Emit(WorldEvent::ClearanceHeld { town: pend.town.clone(), wait_s: tuning.hold_s }));
        out.push(say(&pend.town, t, "held"));
        p.pending = Some(Pending { town: pend.town, due_s: tuning.hold_s, second: true });
        return;
    }
    let inspection = stars >= t.inspection_stars && t.inspection_pad.is_some();
    let pad = match (&t.inspection_pad, inspection) {
        (Some(insp), true) => insp.clone(),
        _ => {
            let normal: Vec<&LocationId> = t.pads.iter().filter(|x| Some(*x) != t.inspection_pad.as_ref()).collect();
            normal[rng.below(normal.len())].clone()
        }
    };
    let line = if inspection { "inspection" } else { "granted" };
    out.push(Outcome::Emit(WorldEvent::ClearanceGranted { town: pend.town.clone(), pad: pad.clone(), window_s: tuning.window_s, inspection }));
    out.push(say(&pend.town, t, line));
    p.clearance = Some(Clearance { town: pend.town, pad, time_left_s: tuning.window_s, inspection });
}

/// 3, 2 or 1 for a touchdown at or under the band's speed; 0 for faster.
fn grade_of(speed: f64, bands: [f64; 3]) -> u8 {
    if speed <= bands[0] {
        3
    } else if speed <= bands[1] {
        2
    } else if speed <= bands[2] {
        1
    } else {
        0
    }
}

fn wallet(delta: i64) -> WorldEvent {
    WorldEvent::TrackChanged { track: TrackId::new(WALLET), delta, player: None }
}

fn say(town: &TownId, t: &Town, ending: &str) -> Outcome {
    Outcome::Say { town: town.clone(), key: TextKey::new(format!("{}.{ending}", t.voice)) }
}

fn notice(key: &str) -> Outcome {
    Outcome::Notice(Notice::new(NoticeKind::Warning, key))
}
