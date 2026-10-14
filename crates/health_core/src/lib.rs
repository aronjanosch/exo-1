//! health_core: a character's health (#295, from the dev-area round #254, #287), without engine
//! types, in f64.
//!
//! Vitality is the maximum minus what the afflictions take (no HP pool). Afflictions (burn, oxygen
//! low, radiation, headache) each have a strength; its strength band sets how much vitality it
//! takes and how fast the strength heals by itself, per tick. Damage over time comes in ticks of
//! `tick_s` (1 s): a source adds its amount once per tick, so the bar drops in steps.
//!
//! - Heat: standing on a hot plate adds a little burn per tick, and fills the burning buildup
//!   (0..1). A fire touching the body fills the buildup too, and adds burn per tick by its rate
//!   (`Env::fire`), not the plate's. Full, the body is on fire: a bigger burn per tick, and the fire drains out over
//!   `burning.drain_s` after the heat is left.
//! - Radiation: builds per tick in the glow, decays by its band, and from 25 up burns per tick by
//!   a table. From 50 up it makes nausea (a derived strength), which costs no vitality but slows
//!   the walk (`Mobility`).
//! - Oxygen low: bad air adds per tick, good air removes; from 50 it takes vitality by band.
//! - Headache: oxygen low above `headache.oxygen_above` for `headache.after_s` in a row gives a
//!   headache (strength 100). It costs no vitality but lowers the maximum (`max_share`).
//! - A treatment (a bandage, a deglow) reduces one affliction per use, scaled by the treater's
//!   skill, with a success roll. Death below `dead_at` x the maximum; respawn clears the body.
//!
//! Taken from Barotrauma: vitality as the maximum minus the afflictions' banded vitality loss,
//! unconscious at zero and dead below zero, the burn and oxygen-low bands, the oxygen and burn
//! rules, and the skill-based success roll.
//!
//! - `HealthTuning`: everything from `content/gameplay/health/health.json` (strict loader).
//! - `Health`: `add`, `step`, `treat`, `respawn`; `HealthEvent`s out of `step` and `treat`.
//! - `Rng`: a seeded xorshift64 for the treatment rolls (no crate dependency).
use std::collections::BTreeMap;

use content_core::{parse_strict, File};
use serde::{Deserialize, Serialize};

pub const HEALTH_PATH: &str = "health/health.json";

/// One strength band of an affliction: from the previous band's `to` (0 for the first) to `to`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Band {
    pub to: f64,
    /// Vitality lost at the band's start and end, as a share of the maximum.
    pub vitality: [f64; 2],
    /// The strength's own change per tick (negative heals).
    pub change_per_tick: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AfflictionKind {
    pub id: String,
    pub max: f64,
    pub bands: Vec<Band>,
}

impl AfflictionKind {
    /// The band a strength falls in, with the strength where that band starts.
    pub fn band(&self, s: f64) -> (f64, &Band) {
        let mut from = 0.0;
        for b in &self.bands {
            if s <= b.to {
                return (from, b);
            }
            from = b.to;
        }
        let last = self.bands.last().expect("checked: at least one band");
        (self.bands.len().checked_sub(2).map(|i| self.bands[i].to).unwrap_or(0.0), last)
    }

    /// Vitality lost at strength `s` (share of the maximum).
    pub fn vitality_share(&self, s: f64) -> f64 {
        let (from, b) = self.band(s);
        let t = if b.to > from { ((s - from) / (b.to - from)).clamp(0.0, 1.0) } else { 1.0 };
        b.vitality[0] + (b.vitality[1] - b.vitality[0]) * t
    }

    /// The band index a strength falls in (0 is the mildest).
    pub fn band_index(&self, s: f64) -> usize {
        self.bands.iter().position(|b| s <= b.to).unwrap_or(self.bands.len().saturating_sub(1))
    }
}

/// What a treatment reduces per use.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Treatment {
    /// An item kind (`bandage`, `deglow`).
    pub id: String,
    /// At this skill it always works; each point below loses 1 % chance.
    pub required_skill: f64,
    /// A failure multiplies the reduction by this.
    pub fail_factor: f64,
    /// Affliction id to strength removed per use.
    pub reduce: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OxygenTuning {
    /// Air below this fraction is bad.
    pub low_below: f64,
    /// Oxygen low grows by this per tick in bad air (conscious or not).
    pub fall_per_tick: f64,
    /// And falls by this per tick in good air.
    pub rise_per_tick: f64,
    /// Unconscious in good air, it grows by this per tick.
    pub unconscious_per_tick: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TreatTuning {
    /// Reduction factor at skill 0 and at skill 100.
    pub skill_low: f64,
    pub skill_high: f64,
}

/// Standing on the hot plate: burn per tick from the first tick. A fire's burn (the rate per
/// second that `fire_core` gives a person) becomes burn per tick at `fire_per_unit` of it.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeatTuning {
    pub burn_per_tick: f64,
    /// Burn per tick per unit of the fire's burn rate (per second).
    pub fire_per_unit: f64,
}

/// The burning buildup (0..1) and the fire.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BurningTuning {
    /// In heat, the buildup fills in this many seconds; full, the body is on fire.
    pub fill_s: f64,
    /// Out of heat, the buildup cools in this many seconds.
    pub cool_s: f64,
    /// The fire goes out over this many seconds after the heat is left.
    pub drain_s: f64,
    /// Burn per tick while on fire.
    pub burn_per_tick: f64,
}

/// A radiation band from the previous band's `to` (0 for the first): burn per tick there.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RadiationBurnBand {
    pub to: f64,
    pub burn_per_tick: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RadiationTuning {
    /// Radiation built per tick in the glow.
    pub glow_per_tick: f64,
    /// Burn per tick by radiation band (a strength below `to` takes that band's rate).
    pub burn_bands: Vec<RadiationBurnBand>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeadacheTuning {
    /// Seconds in a row that oxygen low must stay above `oxygen_above`.
    pub after_s: f64,
    pub oxygen_above: f64,
    /// The maximum is lowered by this share at headache strength 100 (scaled by strength).
    pub max_share: f64,
}

/// The walk from the nausea: the first band whose `above` the nausea exceeds applies.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MobilityBand {
    pub above: f64,
    pub walk: f64,
    pub sprint: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NauseaTuning {
    /// Radiation at which the nausea starts; it grows to `max` at the radiation's maximum.
    pub from_radiation: f64,
    pub max: f64,
    pub mobility: Vec<MobilityBand>,
}

/// Every health number, from `health/health.json`. TODO(initiator): all values are starting points.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HealthTuning {
    pub max_vitality: f64,
    /// Dead at this share of the maximum (negative).
    pub dead_at: f64,
    /// Seconds a dead player waits before the respawn.
    pub respawn_s: f64,
    /// Damage over time comes in ticks of this many seconds.
    pub tick_s: f64,
    /// How far the aim reaches for another player to treat them (m).
    pub use_reach: f64,
    pub oxygen: OxygenTuning,
    pub treat: TreatTuning,
    pub heat: HeatTuning,
    pub burning: BurningTuning,
    pub radiation: RadiationTuning,
    pub headache: HeadacheTuning,
    pub nausea: NauseaTuning,
    pub afflictions: Vec<AfflictionKind>,
    pub treatments: Vec<Treatment>,
}

impl HealthTuning {
    /// Loads and checks `health/health.json` from `files`. Errors name the file and the field.
    pub fn load(files: &[File]) -> Result<HealthTuning, Vec<String>> {
        let p = HEALTH_PATH;
        let Some(f) = files.iter().find(|f| f.path == p) else { return Err(vec![format!("{p}: missing")]) };
        let t = parse_strict::<HealthTuning>(p, &f.text).map_err(|m| vec![m])?;
        let mut e = Vec::new();
        t.check(&mut e);
        if e.is_empty() { Ok(t) } else { Err(e) }
    }

    fn check(&self, e: &mut Vec<String>) {
        let p = HEALTH_PATH;
        let mut need = |ok: bool, field: String, rule: &str| {
            if !ok {
                e.push(format!("{p}: {field}: {rule}"));
            }
        };
        need(self.max_vitality > 0.0, "max_vitality".into(), "must be > 0");
        need(self.dead_at < 0.0, "dead_at".into(), "must be below 0");
        need(self.respawn_s >= 0.0, "respawn_s".into(), "must be >= 0");
        need(self.tick_s > 0.0, "tick_s".into(), "must be > 0");
        need((0.0..=1.0).contains(&self.oxygen.low_below), "oxygen.low_below".into(), "must be in 0..1");
        need(self.treat.skill_low > 0.0 && self.treat.skill_high >= self.treat.skill_low, "treat.skill_high".into(), "must be at least skill_low > 0");
        need(self.heat.burn_per_tick >= 0.0, "heat.burn_per_tick".into(), "must be >= 0");
        need(self.heat.fire_per_unit >= 0.0, "heat.fire_per_unit".into(), "must be >= 0");
        need(self.use_reach > 0.0, "use_reach".into(), "must be > 0");
        need(self.burning.fill_s > 0.0 && self.burning.cool_s > 0.0 && self.burning.drain_s > 0.0, "burning".into(), "fill_s, cool_s and drain_s must be > 0");
        need(self.burning.burn_per_tick >= 0.0, "burning.burn_per_tick".into(), "must be >= 0");
        need(self.radiation.glow_per_tick >= 0.0, "radiation.glow_per_tick".into(), "must be >= 0");
        let rad_max = self.affliction("radiation").map_or(0.0, |k| k.max);
        let mut from = 0.0;
        for (j, b) in self.radiation.burn_bands.iter().enumerate() {
            need(b.to > from, format!("radiation.burn_bands[{j}].to"), "must be above the band before");
            need(b.burn_per_tick >= 0.0, format!("radiation.burn_bands[{j}].burn_per_tick"), "must be >= 0");
            from = b.to;
        }
        need((from - rad_max).abs() < 1e-9, "radiation.burn_bands".into(), "the last band must end at the radiation's max");
        need(self.headache.after_s >= 0.0, "headache.after_s".into(), "must be >= 0");
        need((0.0..=1.0).contains(&self.headache.max_share), "headache.max_share".into(), "must be in 0..1");
        need(self.nausea.max > 0.0 && self.nausea.from_radiation >= 0.0 && self.nausea.from_radiation < rad_max, "nausea".into(), "from_radiation must be below the radiation's max, max > 0");
        need(!self.nausea.mobility.is_empty(), "nausea.mobility".into(), "needs a band");
        need(self.nausea.mobility.iter().all(|m| m.walk > 0.0 && m.walk <= 1.0), "nausea.mobility".into(), "walk must be in (0..1]");
        let mut seen = std::collections::HashSet::new();
        for (i, a) in self.afflictions.iter().enumerate() {
            need(seen.insert(a.id.clone()), format!("afflictions[{i}].id"), "used twice");
            need(a.max > 0.0, format!("afflictions[{i}].max"), "must be > 0");
            need(!a.bands.is_empty(), format!("afflictions[{i}].bands"), "needs a band");
            let mut from = 0.0;
            for (j, b) in a.bands.iter().enumerate() {
                need(b.to > from, format!("afflictions[{i}].bands[{j}].to"), "must be above the band before");
                need(b.vitality[0] <= b.vitality[1], format!("afflictions[{i}].bands[{j}].vitality"), "start must not be above end");
                from = b.to;
            }
            need((from - a.max).abs() < 1e-9, format!("afflictions[{i}].bands"), "the last band must end at max");
        }
        for (i, t) in self.treatments.iter().enumerate() {
            need((0.0..=1.0).contains(&t.fail_factor), format!("treatments[{i}].fail_factor"), "must be in 0..1");
            for (k, v) in &t.reduce {
                need(self.affliction(k).is_some(), format!("treatments[{i}].reduce"), &format!("unknown affliction '{k}'"));
                need(*v > 0.0, format!("treatments[{i}].reduce.{k}"), "must be > 0");
            }
        }
        for id in ["burn", "oxygen_low", "radiation", "headache"] {
            need(self.affliction(id).is_some(), "afflictions".into(), &format!("needs '{id}'"));
        }
    }

    pub fn affliction(&self, id: &str) -> Option<&AfflictionKind> {
        self.afflictions.iter().find(|a| a.id == id)
    }

    pub fn treatment(&self, id: &str) -> Option<&Treatment> {
        self.treatments.iter().find(|t| t.id == id)
    }

    /// Whether a treatment reduces this affliction (its `reduce` names it with more than zero): the
    /// medic view highlights the items that suit an injury someone has (#300).
    pub fn suits(&self, treatment: &str, affliction: &str) -> bool {
        self.treatment(treatment).is_some_and(|t| t.reduce.get(affliction).is_some_and(|r| *r > 0.0))
    }

    /// The reduction factor of a treater's skill (0..100).
    pub fn skill_factor(&self, skill: f64) -> f64 {
        let k = (skill / 100.0).clamp(0.0, 1.0);
        self.treat.skill_low + (self.treat.skill_high - self.treat.skill_low) * k
    }

    /// The nausea at a radiation strength: 0 up to `from_radiation`, `max` at the radiation's max.
    pub fn nausea_of(&self, radiation: f64) -> f64 {
        let Some(k) = self.affliction("radiation") else { return 0.0 };
        let span = k.max - self.nausea.from_radiation;
        ((radiation - self.nausea.from_radiation).max(0.0) / span * self.nausea.max).min(self.nausea.max)
    }
}

impl RadiationTuning {
    /// Burn per tick at a radiation strength: the band it falls in (`s < to`), the last one at most.
    pub fn burn_for(&self, s: f64) -> f64 {
        self.burn_bands.iter().find(|b| s < b.to).or(self.burn_bands.last()).map_or(0.0, |b| b.burn_per_tick)
    }
}

/// A seeded xorshift64: enough for the treatment rolls, no crate needed.
#[derive(Clone, Debug, PartialEq)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }

    /// The next number in 0..1.
    pub fn next_f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }

    /// True with probability `p` (0..1).
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Life {
    Conscious,
    Unconscious,
    Dead,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HealthEvent {
    Unconscious,
    Woke,
    /// The affliction that took the most vitality.
    Died { cause: String },
}

/// The surroundings for one step: the air's oxygen fraction (0..1), whether the body stands on
/// the hot plate and whether it is in the glow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Env {
    pub oxygen: f64,
    pub hot: bool,
    pub glow: bool,
    /// The fire's burn rate on the body, per second (0 when no fire touches it).
    pub fire: f64,
}

/// How the body walks: a share of the walking speed and whether it may sprint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mobility {
    pub walk: f64,
    pub sprint: bool,
}

/// What a treatment did.
#[derive(Clone, Debug, PartialEq)]
pub struct TreatResult {
    pub success: bool,
    /// Strength removed in total.
    pub reduced: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Health {
    /// Affliction id to strength; only those above zero are kept.
    afflictions: BTreeMap<String, f64>,
    vitality: f64,
    max: f64,
    life: Life,
    rng: Rng,
    /// The burning buildup, 0..1.
    buildup: f64,
    /// The fire, 0..1: 1 while the buildup is full in heat, draining after.
    fire: f64,
    /// Seconds into the tick, not yet a whole tick.
    tick_acc: f64,
    /// Seconds in a row the oxygen low has been above the headache's line.
    oxygen_high_s: f64,
}

impl Health {
    /// A healthy character; `seed` for its rolls.
    pub fn new(t: &HealthTuning, seed: u64) -> Health {
        Health {
            afflictions: BTreeMap::new(),
            vitality: t.max_vitality,
            max: t.max_vitality,
            life: Life::Conscious,
            rng: Rng::new(seed),
            buildup: 0.0,
            fire: 0.0,
            tick_acc: 0.0,
            oxygen_high_s: 0.0,
        }
    }

    pub fn vitality(&self) -> f64 {
        self.vitality
    }
    /// The maximum before any headache.
    pub fn max_vitality(&self) -> f64 {
        self.max
    }
    pub fn state(&self) -> Life {
        self.life
    }

    /// Strength of an affliction (0 if none).
    pub fn strength(&self, id: &str) -> f64 {
        self.afflictions.get(id).copied().unwrap_or(0.0)
    }

    /// The burning buildup, 0..1.
    pub fn buildup(&self) -> f64 {
        self.buildup
    }

    /// The fire, 0..1 (draining after the heat).
    pub fn fire(&self) -> f64 {
        self.fire
    }

    /// Whether the body is on fire.
    pub fn burning(&self) -> bool {
        self.fire > 0.0
    }

    /// The burn's level 1 to 3 by its band (0 without burn).
    pub fn burn_level(&self, t: &HealthTuning) -> u8 {
        let s = self.strength("burn");
        t.affliction("burn").map_or(0, |k| if s > 0.0 { (k.band_index(s) + 1) as u8 } else { 0 })
    }

    /// The maximum now, as a share of the maximum before any headache (1 without).
    pub fn max_share(&self, t: &HealthTuning) -> f64 {
        let full = t.affliction("headache").map_or(100.0, |k| k.max);
        1.0 - t.headache.max_share * (self.strength("headache") / full).clamp(0.0, 1.0)
    }

    /// The nausea from the radiation (0 without), no vitality.
    pub fn nausea(&self, t: &HealthTuning) -> f64 {
        t.nausea_of(self.strength("radiation"))
    }

    /// How the body walks now: the nausea's bands, the healthy walk without nausea.
    pub fn mobility(&self, t: &HealthTuning) -> Mobility {
        let n = self.nausea(t);
        let mut m = Mobility { walk: 1.0, sprint: true };
        for b in &t.nausea.mobility {
            if n > b.above {
                m = Mobility { walk: b.walk, sprint: b.sprint };
            }
        }
        m
    }

    /// The vitality an injury costs now, in points: its band's share of the maximum at its
    /// strength (`burn`, `oxygen_low`), or the headache's part of the maximum it takes (`headache`).
    /// 0 for anything else, and for an injury that is not there.
    pub fn cost(&self, t: &HealthTuning, id: &str) -> f64 {
        if id == "headache" {
            return t.max_vitality * (1.0 - self.max_share(t));
        }
        let s = self.strength(id);
        match t.affliction(id) {
            Some(k) if s > 0.0 => k.vitality_share(s) * t.max_vitality,
            _ => 0.0,
        }
    }

    /// Adds strength to an affliction, capped at its max. False for an unknown id. The dead take
    /// nothing.
    pub fn add(&mut self, t: &HealthTuning, id: &str, amount: f64) -> bool {
        if t.affliction(id).is_none() {
            return false;
        }
        if self.life == Life::Dead || amount <= 0.0 {
            return true;
        }
        self.change(t, id, amount);
        self.recalc(t);
        true
    }

    /// Strength change, capped at the affliction's max, never below zero.
    fn change(&mut self, t: &HealthTuning, id: &str, delta: f64) {
        let Some(k) = t.affliction(id) else { return };
        if delta > 0.0 {
            let s = self.afflictions.entry(id.to_string()).or_insert(0.0);
            *s = (*s + delta).min(k.max);
        } else if let Some(s) = self.afflictions.get_mut(id) {
            *s = (*s + delta).max(0.0);
        }
    }

    fn recalc(&mut self, t: &HealthTuning) {
        self.afflictions.retain(|_, s| *s > 0.0);
        let lost: f64 = self.afflictions.iter().filter_map(|(id, s)| t.affliction(id).map(|k| k.vitality_share(*s))).sum();
        self.vitality = self.max * (self.max_share(t) - lost);
    }

    /// The affliction that takes the most vitality.
    pub fn worst(&self, t: &HealthTuning) -> Option<&str> {
        self.afflictions
            .iter()
            .max_by(|a, b| {
                let v = |id: &str, s: f64| t.affliction(id).map(|k| k.vitality_share(s)).unwrap_or(0.0);
                v(a.0, *a.1).total_cmp(&v(b.0, *b.1))
            })
            .map(|(id, _)| id.as_str())
    }

    /// One step: the buildup and the fire (continuous), the headache's count, every whole tick
    /// that the step completes, then vitality and the state.
    pub fn step(&mut self, t: &HealthTuning, dt: f64, env: Env) -> Vec<HealthEvent> {
        if self.life == Life::Dead {
            return Vec::new();
        }
        let b = &t.burning;
        // A fire touching the body fills the buildup like the plate does; only the plate adds its
        // own burn per tick (`tick`), the fire adds its rate.
        let heating = env.hot || env.fire > 0.0;
        self.buildup = if heating { (self.buildup + dt / b.fill_s).min(1.0) } else { (self.buildup - dt / b.cool_s).max(0.0) };
        if heating && self.buildup >= 1.0 - 1e-9 {
            self.fire = 1.0;
        } else if self.fire > 0.0 {
            self.fire = (self.fire - dt / b.drain_s).max(0.0);
        }
        if self.strength("oxygen_low") > t.headache.oxygen_above {
            self.oxygen_high_s += dt;
        } else {
            self.oxygen_high_s = 0.0;
        }
        if self.oxygen_high_s >= t.headache.after_s - 1e-9 {
            self.oxygen_high_s = 0.0;
            if let Some(k) = t.affliction("headache") {
                self.afflictions.insert("headache".to_string(), k.max);
            }
        }
        self.tick_acc += dt;
        while self.tick_acc >= t.tick_s - 1e-9 {
            self.tick_acc -= t.tick_s;
            self.tick(t, env);
        }
        self.recalc(t);
        self.update_life(t)
    }

    /// One tick: the sources add their amounts, then every strength heals or falls by its band.
    fn tick(&mut self, t: &HealthTuning, env: Env) {
        let o = &t.oxygen;
        // Bad air takes its rate whether awake or not; unconscious in good air still loses some.
        let air = match self.life {
            _ if env.oxygen < o.low_below => o.fall_per_tick,
            Life::Unconscious => o.unconscious_per_tick,
            _ => -o.rise_per_tick,
        };
        self.change(t, "oxygen_low", air);
        if env.hot {
            self.change(t, "burn", t.heat.burn_per_tick);
        }
        if env.fire > 0.0 {
            self.change(t, "burn", env.fire * t.heat.fire_per_unit * t.tick_s);
        }
        if env.glow {
            self.change(t, "radiation", t.radiation.glow_per_tick);
        }
        let from_radiation = t.radiation.burn_for(self.strength("radiation"));
        if from_radiation > 0.0 {
            self.change(t, "burn", from_radiation);
        }
        if self.fire > 0.0 {
            self.change(t, "burn", t.burning.burn_per_tick);
        }
        for (id, s) in self.afflictions.iter_mut() {
            if let Some(k) = t.affliction(id) {
                let (_, b) = k.band(*s);
                *s = (*s + b.change_per_tick).clamp(0.0, k.max);
            }
        }
    }

    fn update_life(&mut self, t: &HealthTuning) -> Vec<HealthEvent> {
        let mut ev = Vec::new();
        let dead_line = t.dead_at * self.max;
        if self.vitality <= 0.0 && self.life == Life::Conscious {
            self.life = Life::Unconscious;
            ev.push(HealthEvent::Unconscious);
        }
        if self.vitality <= dead_line + 1e-9 && self.life != Life::Dead {
            self.life = Life::Dead;
            let cause = self.worst(t).unwrap_or_default().to_string();
            ev.push(HealthEvent::Died { cause });
        } else if self.vitality > 0.0 && self.life == Life::Unconscious {
            self.life = Life::Conscious;
            ev.push(HealthEvent::Woke);
        }
        ev
    }

    /// Applies a treatment by its id: each listed affliction loses its amount x skill factor (x
    /// the failure factor on a failed roll). None for an unknown treatment. The dead take nothing.
    pub fn treat(&mut self, t: &HealthTuning, id: &str, skill: f64) -> Option<TreatResult> {
        let tr = t.treatment(id)?.clone();
        if self.life == Life::Dead {
            return Some(TreatResult { success: false, reduced: 0.0 });
        }
        let p = (1.0 + (skill - tr.required_skill) / 100.0).clamp(0.0, 1.0);
        let success = self.rng.chance(p);
        let factor = t.skill_factor(skill) * if success { 1.0 } else { tr.fail_factor };
        let mut reduced = 0.0;
        for (aff, amount) in &tr.reduce {
            if let Some(s) = self.afflictions.get_mut(aff) {
                let r = (amount * factor).min(*s);
                *s -= r;
                reduced += r;
            }
        }
        self.recalc(t);
        self.update_life(t);
        Some(TreatResult { success, reduced })
    }

    /// Back to full health, conscious, cold and sober.
    pub fn respawn(&mut self, t: &HealthTuning) {
        self.afflictions.clear();
        self.life = Life::Conscious;
        self.buildup = 0.0;
        self.fire = 0.0;
        self.tick_acc = 0.0;
        self.oxygen_high_s = 0.0;
        self.recalc(t);
    }
}

/// Where a ray from `origin` along `dir` (unit) first meets a capsule that stands on `feet` and
/// reaches `height` up `up` (unit), with `radius`: the distance along the ray, within `reach`.
/// Sampled every 5 cm (a rough hit test for the aim; the glue passes plain numbers).
pub fn capsule_hit(origin: [f64; 3], dir: [f64; 3], reach: f64, feet: [f64; 3], up: [f64; 3], height: f64, radius: f64) -> Option<f64> {
    const STEP: f64 = 0.05;
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let steps = (reach / STEP).ceil().max(0.0) as usize;
    for i in 0..=steps {
        let t = (i as f64 * STEP).min(reach);
        let p = [origin[0] + dir[0] * t, origin[1] + dir[1] * t, origin[2] + dir[2] * t];
        let rel = [p[0] - feet[0], p[1] - feet[1], p[2] - feet[2]];
        let s = dot(rel, up).clamp(0.0, height);
        let c = [feet[0] + up[0] * s, feet[1] + up[1] * s, feet[2] + up[2] * s];
        let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        if dot(d, d) <= radius * radius {
            return Some(t);
        }
    }
    None
}
