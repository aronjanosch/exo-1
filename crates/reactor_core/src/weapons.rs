//! The weapons' power side: one capacitor that the WEAPONS grid group recharges, and a shot that
//! takes one shot's energy and starts the reload. No aiming and no enemies yet: the FIRE button on
//! GN-01 stands in for the tactical officer.
//!
//! Source: Barotrauma's coilgun (content/Items/Weapons/coilgun.xml:16) has `powerconsumption="1000.0"`
//! kW and `reload="0.25"` s. The turret checks the draw before a shot (Turret.cs:650-655
//! `HasPowerToShoot`) and takes it from the batteries (Turret.cs:776-789), then waits the reload
//! (Turret.cs:876 `Launch`). Our energy per shot is the kW held for the reload: 1000 kW × 0.25 s,
//! scaled by 0.4 (our 2000 kW reactor vs their 5000, as the battery) to 400 kW × 0.25 s = 100 kW·s.
//! Continuous fire then draws 100 kW·s / 0.25 s = 400 kW, the group's demand.
//! Units: energy kW·s, power kW, time s. TODO(initiator): the capacity, the recharge and the reading
//! of Barotrauma's draw (instant check or energy per shot) are open; see `content/gameplay/reactor/reactor.json`.
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponsTuning {
    /// Shots the capacitor holds when full (`TODO(initiator)`: 6).
    pub capacity_shots: u8,
    /// kW a shot draws over its reload (Barotrauma coilgun 1000 × 0.4 = 400).
    pub shot_kw: f64,
    /// Reload after a shot, s (Barotrauma coilgun 0.25).
    pub reload_s: f64,
    /// Recharge at full power, kW: the WEAPONS group's demand while the capacitor is not full.
    pub recharge_kw: f64,
}

impl Default for WeaponsTuning {
    fn default() -> WeaponsTuning {
        WeaponsTuning { capacity_shots: 6, shot_kw: 400.0, reload_s: 0.25, recharge_kw: 400.0 }
    }
}

/// Below this energy (kW·s) the capacitor counts as empty: the float sums of the steps.
const EPS: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq)]
pub struct Weapons {
    tuning: WeaponsTuning,
    /// Energy in the capacitor, kW·s.
    stored: f64,
    /// Seconds of reload left (0 when ready).
    reload_left: f64,
}

impl Weapons {
    /// An empty capacitor, ready: the guns start with no power in them.
    pub fn new(tuning: WeaponsTuning) -> Weapons {
        Weapons { tuning, stored: 0.0, reload_left: 0.0 }
    }

    pub fn tuning(&self) -> &WeaponsTuning {
        &self.tuning
    }

    /// Energy one shot takes, kW·s (`shot_kw × reload_s`).
    pub fn shot_kws(&self) -> f64 {
        self.tuning.shot_kw * self.tuning.reload_s
    }

    /// The capacitor's energy when full, kW·s.
    pub fn capacity_kws(&self) -> f64 {
        f64::from(self.tuning.capacity_shots) * self.shot_kws()
    }

    pub fn stored_kws(&self) -> f64 {
        self.stored
    }

    /// Whole shots in the capacitor, 0..=capacity_shots.
    pub fn shots(&self) -> u8 {
        let n = ((self.stored + EPS) / self.shot_kws()).floor();
        (n.max(0.0) as u8).min(self.tuning.capacity_shots)
    }

    /// Energy over capacity, 0..1.
    pub fn charge(&self) -> f64 {
        (self.stored / self.capacity_kws()).clamp(0.0, 1.0)
    }

    /// Seconds of reload left (0 when ready).
    pub fn reload_left(&self) -> f64 {
        self.reload_left
    }

    /// The group's demand share: 1 while the capacitor is not full, else 0 (the room sets the
    /// WEAPONS demand to `recharge_kw` times this, so a full capacitor asks for nothing).
    pub fn demand_share(&self) -> f64 {
        if self.stored + EPS >= self.capacity_kws() { 0.0 } else { 1.0 }
    }

    /// Advances `dt` seconds: the reload runs down, and the capacitor takes `recharge_kw × power_factor`
    /// (the group's delivered share, 0..1) while it is not full. No power, no recharge.
    pub fn step(&mut self, dt: f64, power_factor: f64) {
        self.reload_left = (self.reload_left - dt).max(0.0);
        if self.demand_share() > 0.0 {
            let kw = self.tuning.recharge_kw * power_factor.clamp(0.0, 1.0);
            self.stored = (self.stored + kw * dt).min(self.capacity_kws());
        }
    }

    /// Fires a shot: refused while reloading or without one shot's energy. An accepted shot takes
    /// `shot_kws` and starts the reload.
    pub fn fire(&mut self) -> bool {
        if self.reload_left > 0.0 || self.stored + EPS < self.shot_kws() {
            return false;
        }
        self.stored = (self.stored - self.shot_kws()).max(0.0);
        self.reload_left = self.tuning.reload_s;
        true
    }
}
