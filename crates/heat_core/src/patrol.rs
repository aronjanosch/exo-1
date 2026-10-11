//! Air patrols (#231). Inside a town's official airspace, a hot ship (stars from
//! `patrol_from_stars`) may be hailed: "Halt for inspection!". Slowing below the halt speed in
//! time gives a scan in the air (the glue raises `ScanStarted` from `Halted`, as for a pad);
//! running gives `Fled`. No patrol in the free port, and a cooldown after every order.
//!
//! Engine-free, seeded through `gameplay_core::rng::Rng`. The glue calls `step` once per tick,
//! and `scan_over` when `ScanResolved` arrives. The shared kernel events are not used here.
use gameplay_core::rng::Rng;
use gameplay_core::save::{Envelope, SaveError};
use gameplay_core::LocationId;
use serde::{Deserialize, Serialize};

use crate::tuning::Tuning;

pub const SECTION: &str = "patrol";
pub const SECTION_VERSION: u32 = 1;
/// The highest chance of one roll, however many stars. TODO(initiator)
pub const CHANCE_CAP: f64 = 0.9;

/// What a `step` tells the glue.
#[derive(Clone, Debug, PartialEq)]
pub enum PatrolOut {
    /// A patrol hails the ship. `at`: the airspace's inspection pad.
    Ordered { at: LocationId },
    /// The ship got below the halt speed at `at`: the glue raises `ScanStarted` there.
    Halted { at: LocationId },
    /// The ship ran (time out, or it left the airspace): the glue raises `Violation{FledScan}`.
    Fled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Phase {
    /// Counting the seconds inside, toward the next roll.
    Idle { acc_s: f64 },
    /// Ordered to halt at `at`; `left_s` to get below the halt speed.
    Ordered { at: LocationId, left_s: f64 },
    /// Halted; the scan is in progress until `scan_over`.
    Scanning,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Patrol {
    rng: Rng,
    phase: Phase,
    /// Seconds until a new order may come after one ended.
    cooldown_left_s: f64,
}

impl Patrol {
    pub fn new(seed: u64) -> Patrol {
        Patrol { rng: Rng::new(seed), phase: Phase::Idle { acc_s: 0.0 }, cooldown_left_s: 0.0 }
    }

    /// The chance of one roll at `stars`: stars x `patrol_chance_per_star`, at most `CHANCE_CAP`.
    pub fn chance(t: &Tuning, stars: u8) -> f64 {
        (f64::from(stars) * t.patrol_chance_per_star).min(CHANCE_CAP)
    }

    /// Advances the patrol by `dt` seconds. `in_official_airspace`: the airspace's inspection pad
    /// when the ship is in a non-free-port airspace, None otherwise. `speed_ms`: the ship's speed.
    pub fn step(&mut self, t: &Tuning, stars: u8, in_official_airspace: Option<LocationId>, speed_ms: f64, dt: f64) -> Vec<PatrolOut> {
        if !(dt > 0.0) {
            return Vec::new();
        }
        self.cooldown_left_s = (self.cooldown_left_s - dt).max(0.0);
        let mut out = Vec::new();
        match std::mem::replace(&mut self.phase, Phase::Scanning) {
            Phase::Idle { acc_s } => {
                let counting = stars >= t.patrol_from_stars && self.cooldown_left_s <= 0.0;
                self.phase = match in_official_airspace {
                    Some(at) if counting => {
                        let mut acc = acc_s + dt;
                        let mut hit = false;
                        while acc >= t.patrol_check_s && !hit {
                            acc -= t.patrol_check_s;
                            hit = self.rng.unit() < Patrol::chance(t, stars);
                        }
                        if hit {
                            out.push(PatrolOut::Ordered { at: at.clone() });
                            Phase::Ordered { at, left_s: t.patrol_halt_s }
                        } else {
                            Phase::Idle { acc_s: acc }
                        }
                    }
                    // Outside, too cool, or on cooldown: the time before a roll starts again.
                    _ => Phase::Idle { acc_s: 0.0 },
                };
            }
            Phase::Ordered { at, left_s } => {
                if in_official_airspace.is_none() {
                    out.push(PatrolOut::Fled);
                    self.end_order(t);
                } else if speed_ms <= t.patrol_halt_speed_ms {
                    out.push(PatrolOut::Halted { at });
                    self.phase = Phase::Scanning;
                } else {
                    let left = left_s - dt;
                    if left <= 0.0 {
                        out.push(PatrolOut::Fled);
                        self.end_order(t);
                    } else {
                        self.phase = Phase::Ordered { at, left_s: left };
                    }
                }
            }
            Phase::Scanning => self.phase = Phase::Scanning,
        }
        out
    }

    /// The scan that followed `Halted` is over: the cooldown starts. Does nothing in any other
    /// state, since the glue also sees the scans of the pads.
    pub fn scan_over(&mut self, t: &Tuning) {
        if matches!(self.phase, Phase::Scanning) {
            self.end_order(t);
        }
    }

    fn end_order(&mut self, t: &Tuning) {
        self.phase = Phase::Idle { acc_s: 0.0 };
        self.cooldown_left_s = t.patrol_cooldown_s;
    }

    /// Seconds left to get below the halt speed, while an order is open (for the HUD).
    pub fn ordered_left(&self) -> Option<f64> {
        match &self.phase {
            Phase::Ordered { left_s, .. } => Some(*left_s),
            _ => None,
        }
    }

    /// Whether a halted ship is in its scan now.
    pub fn scanning(&self) -> bool {
        matches!(self.phase, Phase::Scanning)
    }

    /// Seconds until a new order may come (0 when none is blocked).
    pub fn cooldown_left(&self) -> f64 {
        self.cooldown_left_s
    }

    pub fn save(&self, env: &mut Envelope) {
        env.put(SECTION, SECTION_VERSION, self);
    }

    /// None if the save has no patrol section; an error for a version this build does not read.
    pub fn load(env: &Envelope) -> Result<Option<Patrol>, SaveError> {
        env.get(SECTION, SECTION_VERSION)
    }
}
