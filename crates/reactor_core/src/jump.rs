//! The jump drive's charge (#252): PULSAR's warp charge stages, ported as a state machine.
//! Sources: the PULSAR manual's warp section (`manual_plain.txt`, "The Warp Drive Screen": the
//! engineer starts the charge and can pause it at any time; charging causes "a noticeable spike"
//! in the EM signature) and the concept repo's `docs/research/pulsar/pulsar-reference.md` section 3 (the
//! stages COLD_START, PREPPING, PAUSED, READY, ACTIVE). The concept repo's
//! `docs/research/pulsar/pulsar-mech-conduits.md` gives the Engineering limit: less power, a longer charge.
//!
//! Our rule for the charge: the drive draws the JUMP group's demand while it is armed, so the
//! grid's factor (0..1) is its power. Prepping counts `prep_s` of power, then the charge grows
//! by `factor / charge_s` a second. At factor 0 it is PAUSED and resumes where it stopped.
//! A full charge is READY. A jump empties the charge and runs ACTIVE for `active_s`, then the
//! drive is COLD_START again with the charge empty (the emergency jump, #252).
//!
//! The values are in `content/gameplay/bridge/bridge.json` under "jump"; TODO(initiator): all
//! three, and whether the prep counts power too (here it does).
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JumpTuning {
    /// Seconds of full power before the charge starts (PULSAR's PREPPING). TODO(initiator): 5.
    pub prep_s: f64,
    /// Seconds to a full charge at full power. TODO(initiator): 60.
    pub charge_s: f64,
    /// Seconds the drive stays ACTIVE after a jump. TODO(initiator): 3.
    pub active_s: f64,
}

impl Default for JumpTuning {
    fn default() -> JumpTuning {
        JumpTuning { prep_s: 5.0, charge_s: 60.0, active_s: 3.0 }
    }
}

/// PULSAR's warp charge stages (`pulsar-reference.md` section 3).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JumpStage {
    ColdStart,
    /// Armed and charging (the prep first, then the charge).
    Prepping,
    /// Armed, with no power: the charge waits where it is.
    Paused,
    /// Charged in full: the pilot may jump.
    Ready,
    /// Jumped: the drive is out of its charge and runs down.
    Active,
}

/// The jump drive's state as a save holds it (the tuning comes back from the values).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JumpSave {
    pub stage: JumpStage,
    pub charge: f64,
    pub prep_left: f64,
    pub active_left: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JumpDrive {
    tuning: JumpTuning,
    pub stage: JumpStage,
    /// 0..1 of a full charge.
    pub charge: f64,
    /// Seconds of power the prep still needs.
    prep_left: f64,
    /// Seconds the ACTIVE stage still runs.
    active_left: f64,
}

impl JumpDrive {
    pub fn new(tuning: JumpTuning) -> JumpDrive {
        JumpDrive { tuning, stage: JumpStage::ColdStart, charge: 0.0, prep_left: tuning.prep_s, active_left: 0.0 }
    }

    /// The whole ship cold (`EngineRoom::cold_ship`): back to COLD_START with the charge empty.
    pub fn cold(&mut self) {
        self.stage = JumpStage::ColdStart;
        self.charge = 0.0;
        self.prep_left = self.tuning.prep_s;
        self.active_left = 0.0;
    }

    pub fn to_save(&self) -> JumpSave {
        JumpSave { stage: self.stage, charge: self.charge, prep_left: self.prep_left, active_left: self.active_left }
    }

    pub fn from_save(tuning: JumpTuning, s: &JumpSave) -> JumpDrive {
        JumpDrive { tuning, stage: s.stage, charge: s.charge, prep_left: s.prep_left, active_left: s.active_left }
    }

    /// The engineer's start: COLD_START goes to PREPPING. Refused (false) in any other stage.
    pub fn arm(&mut self) -> bool {
        if self.stage != JumpStage::ColdStart {
            return false;
        }
        self.stage = JumpStage::Prepping;
        self.prep_left = self.tuning.prep_s;
        true
    }

    /// Advances `dt` seconds at the grid's power factor (0..1, clamped). Armed, the charge grows
    /// with the factor; at 0 the stage is PAUSED. A full charge is READY; an ACTIVE drive runs
    /// down to COLD_START with the charge empty.
    pub fn step(&mut self, dt: f64, power_factor: f64) {
        let f = power_factor.clamp(0.0, 1.0);
        match self.stage {
            JumpStage::ColdStart | JumpStage::Ready => {}
            JumpStage::Prepping | JumpStage::Paused => {
                if f <= 0.0 {
                    self.stage = JumpStage::Paused;
                    return;
                }
                self.stage = JumpStage::Prepping;
                if self.prep_left > 0.0 {
                    self.prep_left = (self.prep_left - dt * f).max(0.0);
                } else {
                    self.charge = (self.charge + dt * f / self.tuning.charge_s).min(1.0);
                    if self.charge >= 1.0 {
                        self.stage = JumpStage::Ready;
                    }
                }
            }
            JumpStage::Active => {
                self.active_left -= dt;
                if self.active_left <= 0.0 {
                    self.stage = JumpStage::ColdStart;
                    self.charge = 0.0;
                    self.active_left = 0.0;
                }
            }
        }
    }

    /// The pilot's jump: only at READY. Empties the charge and runs ACTIVE. True when it jumped.
    pub fn jump(&mut self) -> bool {
        if self.stage != JumpStage::Ready {
            return false;
        }
        self.stage = JumpStage::Active;
        self.charge = 0.0;
        self.active_left = self.tuning.active_s;
        true
    }

    /// The dev fault JUMP FULL: the charge is full and the drive READY (#252's refill). An ACTIVE
    /// drive keeps its stage; the charge still refills.
    pub fn fill(&mut self) {
        self.charge = 1.0;
        if self.stage != JumpStage::Active {
            self.stage = JumpStage::Ready;
        }
    }

    /// The drive is charging: armed and not yet READY. It draws power then, and only then.
    pub fn charging(&self) -> bool {
        matches!(self.stage, JumpStage::Prepping | JumpStage::Paused)
    }

    /// The EM spike PULSAR's manual describes: true while the charge runs (not while paused).
    pub fn em_spike(&self) -> bool {
        self.stage == JumpStage::Prepping
    }

    /// The kW the JUMP group asks for: its full demand while charging, 0 otherwise.
    pub fn demand_kw(&self, full_kw: f64) -> f64 {
        if self.charging() { full_kw } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive() -> JumpDrive {
        JumpDrive::new(JumpTuning::default())
    }

    /// Steps of 0.1 s at `f` until READY; the seconds it took.
    fn time_to_ready(f: f64) -> f64 {
        let mut d = drive();
        assert!(d.arm());
        let mut t = 0.0;
        while d.stage != JumpStage::Ready {
            d.step(0.1, f);
            t += 0.1;
            assert!(t < 1000.0, "never ready at factor {f}");
        }
        t
    }

    #[test]
    fn charge_time_scales_with_power() {
        let full = time_to_ready(1.0);
        let half = time_to_ready(0.5);
        // Prep 5 s and charge 60 s at full power: 65 s; at half power both take twice as long.
        assert!((full - 65.0).abs() < 0.2, "full power took {full} s");
        assert!((half - 130.0).abs() < 0.2, "half power took {half} s");
    }

    #[test]
    fn jump_is_refused_unless_ready() {
        let mut d = drive();
        assert!(!d.jump(), "cold");
        assert_eq!(d.stage, JumpStage::ColdStart);
        assert!(d.arm());
        assert!(!d.jump(), "prepping");
        d.step(1.0, 0.0);
        assert!(!d.jump(), "paused");
        assert_eq!(d.stage, JumpStage::Paused);
        assert!(!d.arm(), "arming twice is refused");
    }

    #[test]
    fn jump_empties_the_charge_and_runs_down() {
        let mut d = drive();
        d.fill();
        assert_eq!(d.stage, JumpStage::Ready);
        assert!(d.jump());
        assert_eq!(d.stage, JumpStage::Active);
        assert_eq!(d.charge, 0.0);
        d.step(1.0, 1.0);
        assert_eq!(d.stage, JumpStage::Active, "ACTIVE lasts 3 s");
        d.step(2.5, 1.0);
        assert_eq!(d.stage, JumpStage::ColdStart);
        assert_eq!(d.charge, 0.0);
    }

    #[test]
    fn no_power_pauses_and_resumes_where_it_stopped() {
        let mut d = drive();
        assert!(d.arm());
        for _ in 0..100 {
            d.step(0.1, 1.0);
        }
        let charge = d.charge;
        assert!(charge > 0.0);
        d.step(1.0, 0.0);
        assert_eq!(d.stage, JumpStage::Paused);
        assert_eq!(d.charge, charge, "the charge waits");
        assert!(!d.em_spike(), "no spike while paused");
        d.step(0.1, 1.0);
        assert_eq!(d.stage, JumpStage::Prepping);
    }

    #[test]
    fn the_dev_refill_sets_ready() {
        let mut d = drive();
        d.fill();
        assert_eq!(d.stage, JumpStage::Ready);
        assert_eq!(d.charge, 1.0);
    }

    #[test]
    fn the_drive_asks_power_only_while_charging() {
        let mut d = drive();
        assert_eq!(d.demand_kw(400.0), 0.0, "cold");
        d.arm();
        assert_eq!(d.demand_kw(400.0), 400.0, "prepping");
        assert!(d.em_spike());
        d.fill();
        assert_eq!(d.demand_kw(400.0), 0.0, "ready");
        assert!(!d.em_spike());
    }
}
