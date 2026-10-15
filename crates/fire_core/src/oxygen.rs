//! The oxygen generator of a room: the air it makes per second from the power it gets.
//!
//! Source: Barotrauma's OxygenGenerator
//! (`BarotraumaShared/SharedSource/Items/Components/Machines/OxygenGenerator.cs:40-61`):
//! `CurrFlow = min(Voltage, MaxOverVoltageFactor) * generatedAmount * 100 * condition²`, and
//! nothing while the device has no power. The minimum voltage is `Powered.cs:122-128` (0.5 by
//! default; `MinVoltage`). The breathing side (`Hull.cs:124`, `Deck::breathe`) is the other end of
//! the same room: the glue drains it per living player (`dev_area/engine_room.rs`, `breath_step`).
//!
//! Ours: the voltage is the grid's life support factor (`reactor_core::grid`,
//! `Grid::factor(Group::LifeSupport)`), capped at 1 because our grid never over-volts the group.
//! The rate is m³ per second at full power (`life_support.oxygen_generator_rate` in `hull/hull.json`), so
//! the Barotrauma scale of 100 is folded into it. The condition is 1 for now: the generator does
//! not wear yet. TODO(initiator): should it wear like the pump?
//! TODO(initiator): the minimum voltage 0.5 is Barotrauma's default, kept as it is.
//!
//! The vents (r38): Barotrauma's generator does not feed a room directly. `UpdateVents`
//! (`OxygenGenerator.cs:119-135`, every 5 s, `VentUpdateInterval` at :18) gives each vent of the
//! generator's list `CurrFlow * hullVolume / totalHullVolume`, and returns with nothing when the
//! list is empty (:128). A vent's `OxygenFlow` setter clamps at 0 (`Vent.cs:10-13`) and the vent
//! puts its flow into its hull (`Vent.cs:24-28`). Ours: `split_by_volume`, the shares of the open
//! vents by volume; a shut vent takes nothing, so the rest goes to the open ones. The glue is
//! `dev_area/engine_room.rs` (`EngineAir::add_oxygen_through_vents`).

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

/// Shares of `flow` over vents given as `(volume, open)`: an open vent gets `flow * volume / total`,
/// where `total` is the volume of the open vents; a shut vent gets 0. With no open vent (or all of
/// them of zero volume) nothing goes anywhere, so every share is 0: the sum is the flow only while
/// one vent is open. Barotrauma's `UpdateVents` (`OxygenGenerator.cs:128`) does nothing without vents.
pub fn split_by_volume(flow: f64, vents: &[(f64, bool)]) -> Vec<f64> {
    let total: f64 = vents.iter().filter(|(_, open)| *open).map(|(v, _)| v.max(0.0)).sum();
    vents.iter().map(|&(v, open)| if open && total > 0.0 { flow * v.max(0.0) / total } else { 0.0 }).collect()
}
