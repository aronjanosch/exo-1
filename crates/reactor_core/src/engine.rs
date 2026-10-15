//! The engines' throttle and thrust (r38), after Barotrauma's `Engine.cs` (the shared source in
//! `exo-research/barotrauma`): the throttle is the crew's target, the force follows it, the power
//! draw follows the force, and the thrust is what the ship feels.
//!
//! - Force follows the target at 10 per second of the gap (`Engine.cs:136`, `Lerp(force, target,
//!   dt * 10)`). Our step caps the fraction at 1, so a long step cannot overshoot.
//! - Below the minimum voltage 0.5 the force goes to 0 and the thrust is 0 (`Engine.cs:136` gates on
//!   `Voltage < MinVoltage`; `Powered.cs:125`, `Powered.cs:198`).
//! - Demand: the full demand times `(|force| / 100)^1.5` (`Engine.cs:32` the exponent, `:190` the
//!   power). Barotrauma takes the power from the target, we take it from the force: the draw then
//!   follows the thrust the ship really gets, as the force lerps.
//! - Thrust (our number, %): `|force| * min(voltage, 1) * condition factor`. The condition factor
//!   is Barotrauma's `Lerp(0.5, 2.0, condition)` (`Engine.cs:160`) with the top cut to 1.0, so a
//!   sound engine gives its full force and a worn one half. TODO(initiator): the factor's floor 0.5.
//! - Not modelled: Barotrauma's `MaxForce` (500, it scales the push in newtons), the talents, the
//!   helm's skill, and the over-voltage boost (`Engine.cs:139`). There is no flight in the slice.
//!   TODO(initiator): when flight reads the thrust.
//!
//! Every number is a starting value (`TODO(initiator)`) where it is ours.

#[cfg(test)]
mod tests;

/// The lowest voltage the engine runs at (Barotrauma's `MinVoltage` of the engine, `Powered.cs:125`).
pub const MIN_VOLTAGE: f64 = 0.5;
/// How fast the force follows the target, per second: Barotrauma's `deltaTime * 10`.
pub const FORCE_RATE: f64 = 10.0;
/// The power draw's exponent on the force (`Engine.cs:32`, 3/2).
pub const POWER_EXPONENT: f64 = 1.5;

/// One engine, the values in percent (-100..100 for the throttle and the force, 0..100 for the thrust).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Engine {
    target: f64,
    force: f64,
    /// The engine's condition, 0..1 (its share of the max; a worn engine gives less thrust).
    condition: f64,
}

impl Default for Engine {
    fn default() -> Engine {
        Engine::new()
    }
}

impl Engine {
    /// A running engine at full throttle, in full condition: the room's start (the grid's demand at
    /// full throttle, as before the throttle existed).
    pub fn new() -> Engine {
        Engine { target: 100.0, force: 100.0, condition: 1.0 }
    }

    /// The crew's throttle, -100..100 (the host clamps it too).
    pub fn set_target(&mut self, pct: f64) {
        self.target = pct.clamp(-100.0, 100.0);
    }

    pub fn target(&self) -> f64 {
        self.target
    }

    /// The force the engine has now, -100..100: it follows the target.
    pub fn force(&self) -> f64 {
        self.force
    }

    pub fn set_condition(&mut self, c: f64) {
        self.condition = c.clamp(0.0, 1.0);
    }

    pub fn condition(&self) -> f64 {
        self.condition
    }

    /// Advances the force by `dt` seconds toward the target, or toward 0 below `MIN_VOLTAGE`.
    /// `voltage` is the grid's voltage (0..1, the junction box's), taken as it stood at the step's start.
    pub fn step(&mut self, dt: f64, voltage: f64) {
        let goal = if voltage < MIN_VOLTAGE { 0.0 } else { self.target };
        self.force += (goal - self.force) * (FORCE_RATE * dt).min(1.0);
    }

    /// The power the engine asks for, kW: `full_kw` (the grid's engine demand) at full force, by the
    /// force's power curve.
    pub fn demand_kw(&self, full_kw: f64) -> f64 {
        full_kw * (self.force.abs() / 100.0).powf(POWER_EXPONENT)
    }

    /// The thrust the ship feels, %: 0 below `MIN_VOLTAGE`, else `|force|` times the voltage (at most
    /// 1) times the condition factor.
    pub fn thrust(&self, voltage: f64) -> f64 {
        if voltage < MIN_VOLTAGE {
            return 0.0;
        }
        let condition_factor = 0.5 + 0.5 * self.condition;
        self.force.abs() * voltage.min(1.0) * condition_factor
    }
}
