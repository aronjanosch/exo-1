//! The engine room's battery, ported from Barotrauma's PowerContainer (PowerContainer.cs:63-127
//! for the values, :199-263 for the recharge and the output cap, the charge update in
//! GridResolved: `charge += kW × dt / 60 × efficiency`). It sits behind the reactor: it gives
//! what the grid lacks, and takes what the grid leaves at the player's recharge setting.
//! Units: charge in kW·min (a minute at 1 kW), power in kW, time in s.
//! Scaled from Barotrauma's 5000 kW reactor to our 2000 kW (× 0.4); the values are in
//! `content/gameplay/reactor/reactor.json` under "battery", TODO(initiator).
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BatteryTuning {
    /// Capacity, kW·min (Barotrauma 2000).
    pub capacity_kwmin: f64,
    /// Output when full, kW (Barotrauma 1000).
    pub max_output_kw: f64,
    /// Charge speed at recharge 1, kW (Barotrauma 500).
    pub max_recharge_kw: f64,
    /// Share of the taken power that is stored (Barotrauma 0.95).
    pub efficiency: f64,
}

impl Default for BatteryTuning {
    fn default() -> BatteryTuning {
        BatteryTuning { capacity_kwmin: 800.0, max_output_kw: 400.0, max_recharge_kw: 200.0, efficiency: 0.95 }
    }
}

/// The battery's state as a save holds it (the tuning comes back from the values).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatterySave {
    pub charge_kwmin: f64,
    pub recharge: f64,
    pub output_kw: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Battery {
    tuning: BatteryTuning,
    charge_kwmin: f64,
    /// 0..1 of `max_recharge_kw` the player set.
    recharge: f64,
    /// kW it gave in the last step.
    output_kw: f64,
}

impl Battery {
    /// An empty battery at recharge 0 (Barotrauma starts empty).
    pub fn new(tuning: BatteryTuning) -> Battery {
        Battery { tuning, charge_kwmin: 0.0, recharge: 0.0, output_kw: 0.0 }
    }

    pub fn to_save(&self) -> BatterySave {
        BatterySave { charge_kwmin: self.charge_kwmin, recharge: self.recharge, output_kw: self.output_kw }
    }

    pub fn from_save(tuning: BatteryTuning, s: &BatterySave) -> Battery {
        Battery { tuning, charge_kwmin: s.charge_kwmin, recharge: s.recharge, output_kw: s.output_kw }
    }

    pub fn tuning(&self) -> &BatteryTuning {
        &self.tuning
    }

    pub fn charge_kwmin(&self) -> f64 {
        self.charge_kwmin
    }

    /// Charge over capacity, 0..1.
    pub fn fraction(&self) -> f64 {
        self.charge_kwmin / self.tuning.capacity_kwmin
    }

    /// Sets the charge (kW·min), clamped to 0..capacity.
    pub fn set_charge_kwmin(&mut self, kwmin: f64) {
        self.charge_kwmin = kwmin.clamp(0.0, self.tuning.capacity_kwmin);
    }

    pub fn recharge(&self) -> f64 {
        self.recharge
    }

    /// The player's recharge setting, 0..1 (clamped): the share of `max_recharge_kw` taken from the surplus.
    pub fn set_recharge(&mut self, r: f64) {
        self.recharge = r.clamp(0.0, 1.0);
    }

    /// kW the battery gave in the last step (`supply`).
    pub fn output_kw(&self) -> f64 {
        self.output_kw
    }

    /// Most kW it can give for `dt` seconds: max output, cut to 0..1 of it below 10 % charge
    /// (Barotrauma PowerContainer.cs:252-254), and limited by the charge left.
    pub fn max_output_kw(&self, dt: f64) -> f64 {
        let ratio = self.fraction();
        let cap = if ratio < 0.1 { ratio.max(0.0) * 10.0 * self.tuning.max_output_kw } else { self.tuning.max_output_kw };
        if dt > 0.0 { cap.min(self.charge_kwmin * 60.0 / dt) } else { 0.0 }
    }

    /// Gives up to `want_kw` for `dt` seconds; returns the kW given. Empty gives nothing.
    pub fn supply(&mut self, want_kw: f64, dt: f64) -> f64 {
        let kw = want_kw.max(0.0).min(self.max_output_kw(dt));
        self.charge_kwmin = (self.charge_kwmin - kw * dt / 60.0).max(0.0);
        self.output_kw = kw;
        kw
    }

    /// Takes up to `recharge × max_recharge_kw` from `surplus_kw` for `dt` seconds; returns the
    /// kW taken. Only `efficiency` of it is stored, and the charge stops at capacity.
    pub fn absorb(&mut self, surplus_kw: f64, dt: f64) -> f64 {
        if dt <= 0.0 {
            return 0.0;
        }
        let eff = self.tuning.efficiency;
        let room_kw = (self.tuning.capacity_kwmin - self.charge_kwmin) * 60.0 / (dt * eff);
        let take = (self.recharge * self.tuning.max_recharge_kw).min(surplus_kw.max(0.0)).min(room_kw.max(0.0));
        self.charge_kwmin = (self.charge_kwmin + take * dt / 60.0 * eff).min(self.tuning.capacity_kwmin);
        take
    }
}
