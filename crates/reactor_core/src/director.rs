//! The incident director (r38): breaks one device of the engine room at random intervals, and
//! breaks none while the room is already in trouble. A port of two Barotrauma rules, simplified.
//!
//! Sources (`/home/agent/exo-research/barotrauma/source/Barotrauma/BarotraumaShared/SharedSource/Events/`):
//! - Which items, how picked: `MalfunctionEvent.cs:44-50` takes items that still work (Condition > 0)
//!   and picks at random; `:65-67` breaks them at once when the duration is 0 (ours: all faults are
//!   at once). Here the candidates are the room's six `Fault`s (`CANDIDATES`); the two outside hits
//!   (r41) are always open.
//! - Intensity, the terms: `EventManager.cs:1119-1124` (a fire of any size is 20 %, the hull's holes)
//!   and `:1139-1143`: the three terms averaged, plus fire at half weight, clamped to 0..1. Ours: the
//!   broken machines, the bad air and the critical temperature band for the three, `FIRE_SHARE` per fire.
//! - Rise and fall: `EventManager.cs:38` (update every 5 s) and `:1145-1157` (0.04 per second up,
//!   0.0025 per second down; `rise_per_s`, `fall_per_s`).
//! - Waiting: `EventManager.cs:919-920`, an event starts only when the cooldown is over and the
//!   intensity is at or below the threshold. The threshold starts at `DefaultEventThreshold` 0.2
//!   (`EventManager.cs:60`, the Easiest row of `content/EventManagerSettings.xml`).
//! - Cooldown: `EventCooldown` 60 s between events (the same XML). Ours: the interval is random
//!   between `min_interval_s` and `max_interval_s`, a seeded pick.
//!
//! Not ported: the threshold's drift (`EventThresholdIncrease`, `EventManager.cs:899-900`) and its
//! reset after each event (`:926-934`); the threshold is one value. TODO(initiator).
use gameplay_core::rng::Rng;
use serde::{Deserialize, Serialize};

use crate::engine_room::{EngineRoom, Fault};

/// The faults the director picks from (the room's existing faults; Barotrauma's coolant leak is the
/// pump's damage, so it is PumpBreak here, not a fifth entry). The two outside hits (r41) are picks too:
/// a light and a heavy hit on the wall, as the dev faults. TODO(initiator): weights are equal.
pub const CANDIDATES: [Fault; 6] = [Fault::PumpBreak, Fault::Surge, Fault::OxygenGeneratorBreak, Fault::HullBreach, Fault::HitLight, Fault::HitHeavy];

/// Barotrauma's fire share: any fire is 20 % (`EventManager.cs:1124`), and the fire term counts at half
/// weight (`:1141`).
const FIRE_SHARE: f64 = 0.2;
const FIRE_WEIGHT: f64 = 0.5;
/// Oxygen at or below this share is bad air: the status monitor's line (display_core monitor, Barotrauma
/// 30 %). The bad-air term ramps from 0 at this line to 1 at no oxygen.
const BAD_AIR: f64 = 0.3;
/// The machines the broken term counts: reactor, pump, junction box, oxygen generator.
const MACHINES: f64 = 4.0;

/// `director` in incidents/director.json. Start values, TODO(initiator): see the module doc for their sources.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorTuning {
    /// On at the start (the dev faults screen's DIRECTOR toggle sets it in play).
    pub enabled: bool,
    pub min_interval_s: f64,
    pub max_interval_s: f64,
    /// The intensity at or below which a fault may fire.
    pub threshold: f64,
    /// The intensity's climb and fall, per second (`EventManager.cs:1145-1157`).
    pub rise_per_s: f64,
    pub fall_per_s: f64,
}

impl Default for DirectorTuning {
    fn default() -> DirectorTuning {
        DirectorTuning { enabled: false, min_interval_s: 60.0, max_interval_s: 180.0, threshold: 0.2, rise_per_s: 0.04, fall_per_s: 0.0025 }
    }
}

/// What the room reports, as plain numbers and the faults it can take now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Situation {
    /// Fires burning in the rooms the crew is in (the engine room and the corridor).
    pub fires: u32,
    /// Broken machines of the `MACHINES` (see its doc).
    pub broken: u32,
    /// The engine room's oxygen share, 0..1.
    pub oxygen: f64,
    /// The reactor's temperature is outside its allowed band (power on): the critical lamps.
    pub temp_critical: bool,
    /// Per `CANDIDATES` entry: the fault can happen now (a broken device, or a breach already open,
    /// is not a candidate: Barotrauma picks among items with Condition > 0, `MalfunctionEvent.cs:44`).
    pub can_break: [bool; 6],
}

impl Situation {
    /// The room's report, with the fires and the oxygen the host keeps (its air).
    pub fn of_room(room: &EngineRoom, fires: u32, oxygen: f64) -> Situation {
        let working = [room.reactor().condition().functioning(), room.pump().condition().functioning(), room.junction().condition().functioning(), room.oxygen_generator().condition().functioning()];
        let w = room.warnings();
        Situation {
            fires,
            broken: working.iter().filter(|ok| !**ok).count() as u32,
            oxygen,
            temp_critical: w.critical_heat || w.cold,
            can_break: [room.pump().condition().functioning(), room.junction().condition().functioning(), room.oxygen_generator().condition().functioning(), room.hull().intact(), true, true],
        }
    }
}

/// The director's state as a save holds it: the switch, the timer, the intensity, the rng (the
/// tuning comes back from the values).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirectorSave {
    pub enabled: bool,
    pub timer: f64,
    pub intensity: f64,
    pub rng: Rng,
}

#[derive(Debug)]
pub struct Director {
    cfg: DirectorTuning,
    rng: Rng,
    enabled: bool,
    /// Seconds until the next fault may fire (it waits at or below 0 while the intensity is high).
    timer: f64,
    intensity: f64,
}

impl Director {
    pub fn new(cfg: &DirectorTuning, seed: u64) -> Director {
        let mut d = Director { cfg: cfg.clone(), rng: Rng::new(seed), enabled: cfg.enabled, timer: 0.0, intensity: 0.0 };
        d.timer = d.next_interval();
        d
    }

    pub fn to_save(&self) -> DirectorSave {
        DirectorSave { enabled: self.enabled, timer: self.timer, intensity: self.intensity, rng: self.rng.clone() }
    }

    pub fn from_save(cfg: &DirectorTuning, s: &DirectorSave) -> Director {
        Director { cfg: cfg.clone(), rng: s.rng.clone(), enabled: s.enabled, timer: s.timer, intensity: s.intensity }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
    }

    /// 0..1: the room's trouble, following its target at the rise and fall rates.
    pub fn intensity(&self) -> f64 {
        self.intensity
    }

    /// The intensity this report asks for (`EventManager.cs:1139-1143`, our terms).
    pub fn target(s: &Situation) -> f64 {
        let broken = f64::from(s.broken) / MACHINES;
        let bad_air = ((BAD_AIR - s.oxygen) / BAD_AIR).clamp(0.0, 1.0);
        let temp = if s.temp_critical { 1.0 } else { 0.0 };
        let fire = if s.fires > 0 { (FIRE_SHARE * f64::from(s.fires)).min(1.0) } else { 0.0 };
        ((broken + bad_air + temp) / 3.0 + fire * FIRE_WEIGHT).clamp(0.0, 1.0)
    }

    /// Steps `dt` seconds on the report. Off: nothing moves. Returns the fault that fires now, if any.
    pub fn step(&mut self, dt: f64, s: &Situation) -> Option<Fault> {
        if !self.enabled {
            return None;
        }
        let target = Director::target(s);
        self.intensity = if target > self.intensity {
            (self.intensity + self.cfg.rise_per_s * dt).min(target)
        } else {
            (self.intensity - self.cfg.fall_per_s * dt).max(target)
        };
        self.timer -= dt;
        if self.timer > 0.0 || self.intensity > self.cfg.threshold {
            return None;
        }
        self.timer = self.next_interval();
        let open: Vec<Fault> = CANDIDATES.iter().zip(s.can_break).filter(|(_, ok)| *ok).map(|(f, _)| *f).collect();
        if open.is_empty() {
            return None;
        }
        Some(open[self.rng.below(open.len())])
    }

    fn next_interval(&mut self) -> f64 {
        let u = self.rng.unit();
        self.cfg.min_interval_s + (self.cfg.max_interval_s - self.cfg.min_interval_s) * u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(enabled: bool) -> DirectorTuning {
        DirectorTuning { enabled, min_interval_s: 60.0, max_interval_s: 120.0, threshold: 0.2, rise_per_s: 0.04, fall_per_s: 0.0025 }
    }

    fn calm() -> Situation {
        Situation { fires: 0, broken: 0, oxygen: 1.0, temp_critical: false, can_break: [true; 6] }
    }

    fn bad() -> Situation {
        Situation { fires: 5, broken: 4, oxygen: 0.0, temp_critical: true, can_break: [true; 6] }
    }

    /// Steps `secs` in 0.1 s steps; the faults with the time they fired at.
    fn run(d: &mut Director, s: &Situation, secs: f64) -> Vec<(f64, Fault)> {
        let mut out = Vec::new();
        let mut t = 0.0;
        while t < secs {
            t += 0.1;
            if let Some(f) = d.step(0.1, s) {
                out.push((t, f));
            }
        }
        out
    }

    #[test]
    fn off_does_nothing() {
        let mut d = Director::new(&cfg(false), 1);
        assert!(run(&mut d, &calm(), 1000.0).is_empty());
        assert_eq!(d.intensity(), 0.0);
    }

    #[test]
    fn on_fires_within_the_max_interval() {
        let mut d = Director::new(&cfg(true), 1);
        let got = run(&mut d, &calm(), 121.0);
        assert!(!got.is_empty() && got[0].0 <= 120.2, "{got:?}");
    }

    #[test]
    fn high_intensity_waits() {
        let mut d = Director::new(&cfg(true), 1);
        assert!(run(&mut d, &bad(), 600.0).is_empty());
        assert!(d.intensity() > 0.2);
    }

    #[test]
    fn only_open_candidates_fire() {
        let only_hull = Situation { can_break: [false, false, false, true, false, false], ..calm() };
        let got = run(&mut Director::new(&cfg(true), 3), &only_hull, 1000.0);
        assert!(!got.is_empty() && got.iter().all(|(_, f)| *f == Fault::HullBreach), "{got:?}");
    }

    #[test]
    fn the_same_seed_gives_the_same_faults() {
        let a = run(&mut Director::new(&cfg(true), 7), &calm(), 3000.0);
        let b = run(&mut Director::new(&cfg(true), 7), &calm(), 3000.0);
        let c = run(&mut Director::new(&cfg(true), 8), &calm(), 3000.0);
        assert_eq!(a, b);
        assert!(a.len() >= 10 && a != c, "{a:?}");
    }
}
