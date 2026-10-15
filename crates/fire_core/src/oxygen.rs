//! The oxygen generator of a room: the air it makes per second from the power it gets.
//!
//! Source: Barotrauma's OxygenGenerator
//! (`BarotraumaShared/SharedSource/Items/Components/Machines/OxygenGenerator.cs:40-61`):
//! `CurrFlow = min(Voltage, MaxOverVoltageFactor) * generatedAmount * 100 * condition²`, and
//! nothing while the device has no power. The minimum voltage is `Powered.cs:122-128` (0.5 by
//! default; `MinVoltage`). The breathing side, `Hull.cs:124` (`OxygenConsumptionSpeed` 700 per
//! character per second), is not in the core: no characters breathe here yet.
//!
//! Ours: the voltage is the grid's life support factor (`reactor_core::grid`,
//! `Grid::factor(Group::LifeSupport)`), capped at 1 because our grid never over-volts the group.
//! The rate is m³ per second at full power (`room.oxygen_generator_rate` in `reactor.json`), so
//! the Barotrauma scale of 100 is folded into it. The condition is 1 for now: the generator does
//! not wear yet. TODO(initiator): should it wear like the pump?
//! TODO(initiator): the minimum voltage 0.5 is Barotrauma's default, kept as it is.

/// Below this voltage the generator makes nothing (Barotrauma `MinVoltage` default).
pub const MIN_VOLTAGE: f64 = 0.5;

/// m³ of air per second: `min(voltage, 1) * rate * condition²`, 0 below `MIN_VOLTAGE` (and for a
/// NaN voltage). `rate` is m³ per second at full power; `condition` is 0..1.
pub fn generator_flow(voltage: f64, rate: f64, condition: f64) -> f64 {
    // `!(>=)` also catches NaN, which would otherwise pass the minimum.
    if !(voltage >= MIN_VOLTAGE) {
        return 0.0;
    }
    voltage.min(1.0) * rate.max(0.0) * condition.clamp(0.0, 1.0).powi(2)
}
