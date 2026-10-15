//! The ship's shield: a generator fed by the SHIELDS grid group (PULSAR's `PLShieldGenerator`).
//!
//! Sources: `pulsar-mech-boarding-shield.md` section 2 (the generator's fields `m_Current`, `m_Max`,
//! `m_ChargeRateCurrent/Max`, `m_RecoveryRate`, `m_Deflection`, `m_MinIntegrityPercentForQuantumShield`);
//! manual `manual_plain.txt:134-137` (the quantum shield is commonly at 75 %; below it, the total
//! shielding must drop more than 25 % to disable the quantum shield, and it stays offline for a
//! specific period even when the shield regenerates); manual `:321-322` (the Shields conduit sets the
//! regeneration). PULSAR gives no numbers: every value in `ShieldTuning` is a TODO(initiator) start.
//!
//! Hits drain the shield first, then the hull (`absorb` returns what passes on). Regen starts only
//! after the recovery delay since the last hit, and runs at the charge rate times the Shields group's
//! power factor. The quantum shield (the boarding gate) drops when a hit takes the shield below
//! `quantum_min`, and stays down for `quantum_offline_s` from that drop, even while the shield refills.
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShieldTuning {
    /// The shield's capacity, points. TODO(initiator): 1000 is a placeholder.
    pub max: f64,
    /// Regen at full power, points a second: 50 fills the shield from 0 in 20 s. TODO(initiator).
    pub charge_rate_max: f64,
    /// Seconds after a hit before the regen restarts. TODO(initiator): 3 s is a placeholder.
    pub recovery_delay_s: f64,
    /// The share of each hit the shield ignores, 0..1 (PULSAR's `m_Deflection`). TODO(initiator): 0.
    pub deflection: f64,
    /// The quantum shield's line, 0..1 of max (PULSAR's `m_MinIntegrityPercentForQuantumShield`,
    /// the manual's commonly 75 %).
    pub quantum_min: f64,
    /// Seconds the quantum shield stays down after it drops. TODO(initiator): 30 s is a placeholder.
    pub quantum_offline_s: f64,
    /// The Shields group's demand at full shield, as a share of its demand: the upkeep of a full
    /// shield, 0..1. TODO(initiator): 0.1.
    pub upkeep: f64,
}

impl Default for ShieldTuning {
    fn default() -> ShieldTuning {
        ShieldTuning { max: 1000.0, charge_rate_max: 50.0, recovery_delay_s: 3.0, deflection: 0.0, quantum_min: 0.75, quantum_offline_s: 30.0, upkeep: 0.1 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shield {
    tuning: ShieldTuning,
    current: f64,
    /// Seconds since the last hit (the recovery delay runs on it).
    since_hit_s: f64,
    /// Regen of the last step, points a second (0 inside the recovery delay).
    regen: f64,
    /// Seconds the quantum shield stays down (0 when the timer has run out).
    offline_left_s: f64,
}

impl Shield {
    /// A full shield with the quantum shield up.
    pub fn new(tuning: ShieldTuning) -> Shield {
        Shield { tuning, current: tuning.max, since_hit_s: tuning.recovery_delay_s, regen: 0.0, offline_left_s: 0.0 }
    }

    pub fn tuning(&self) -> &ShieldTuning {
        &self.tuning
    }

    /// Points left, 0..max.
    pub fn current(&self) -> f64 {
        self.current
    }

    /// Charge over max, 0..1.
    pub fn fraction(&self) -> f64 {
        self.current / self.tuning.max.max(1e-9)
    }

    /// The regen of the last step, points a second (0 inside the recovery delay).
    pub fn regen_per_s(&self) -> f64 {
        self.regen
    }

    /// Advances `dt` seconds. `power_factor` is the Shields group's factor (0..1, delivered over demand).
    pub fn step(&mut self, dt: f64, power_factor: f64) {
        let dt = dt.max(0.0);
        self.since_hit_s += dt;
        self.offline_left_s = (self.offline_left_s - dt).max(0.0);
        self.regen = if self.since_hit_s >= self.tuning.recovery_delay_s { self.tuning.charge_rate_max * power_factor.clamp(0.0, 1.0) } else { 0.0 };
        self.current = (self.current + self.regen * dt).min(self.tuning.max);
    }

    /// A hit of `damage`: the share the shield deflects is ignored, the rest drains the shield, and
    /// the overflow is returned for the hull. A hit restarts the recovery delay. A hit that takes the
    /// shield below the quantum line starts the offline time.
    pub fn absorb(&mut self, damage: f64) -> f64 {
        let damage = damage.max(0.0);
        if damage <= 0.0 {
            return 0.0;
        }
        let was_up = self.quantum_up();
        let through = damage * (1.0 - self.tuning.deflection.clamp(0.0, 1.0));
        let taken = through.min(self.current);
        self.current -= taken;
        self.since_hit_s = 0.0;
        self.regen = 0.0;
        if was_up && self.fraction() < self.tuning.quantum_min {
            self.offline_left_s = self.tuning.quantum_offline_s;
        }
        through - taken
    }

    /// The quantum shield is up: the offline time has run out and the shield is at its line or above.
    pub fn quantum_up(&self) -> bool {
        self.offline_left_s <= 0.0 && self.fraction() >= self.tuning.quantum_min
    }

    /// The Shields demand's share, 0..1: 1 while the shield is below max, the upkeep at max.
    pub fn demand_share(&self) -> f64 {
        if self.current >= self.tuning.max { self.tuning.upkeep } else { 1.0 }
    }
}
