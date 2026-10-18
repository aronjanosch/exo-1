//! Coolant by hand, host side: the coolant pump's tank runs down while the pump runs, in proportion to the heat
//! it carries away, and a dry tank is no flow. The host adds `RoomTuning::coolant_canister` per emptied
//! canister it is told about (`host_refills`). The pump's housing, its screen, the canister rack and the
//! hold at the pump are gone (cleanup of the dev area): nothing in the world sends a refill now.
//! TODO(initiator): where the tank is refilled once the new pump stands.
//!
//! Research (where the rules come from; the tank from PULSAR, the hand refill from Barotrauma, no files copied):
//! - PULSAR: a coolant level in % and a pump state Off, Low, High; coolant is bought in 5 % steps and
//!   used up. `~/Work/exo-research/pulsar/assets/textassets/manual_plain.txt` ("The Coolant
//!   Screen"); `~/Work/exo-research/pulsar/cpp2il/metadata-dump/Assembly-CSharp.txt`
//!   (`ReactorCoolantLevelPercent`, `ReactorCoolingPumpState`, `PLContainmentScreen_Coolant`);
//!   report `~/Work/exo-research/reports/pulsar.md` section 3.2.
//! - Barotrauma: no coolant; the reactor burns antimatter canisters put into its slots by hand, a consumable.
//!   `~/Work/exo-research/barotrauma/content/Items/Reactor/reactor.xml` (`ItemContainer`, canister slot);
//!   `~/Work/exo-research/barotrauma/source/Barotrauma/BarotraumaShared/SharedSource/Items/Components/Machines/Reactor.cs`
//!   (`FillConsumptionRate`, the canisters' condition goes down).
use crate::dev_area::*;
use crate::items::Items;

pub const REFILL: &str = "refill_coolant";

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, host_refills.in_set(crate::dev_area::ship::ShipSet::Asks));
}

/// Host: each canister a hand emptied at the pump (`Items::take_refills`) fills the tank by
/// `coolant_canister` per cent.
fn host_refills(mut items: ResMut<Items>, cfg: Res<crate::dev_area::ship::ReactorConfig>, mut sim: ResMut<crate::dev_area::ship::ReactorSim>) {
    if items.role == crate::items::Role::Client {
        return;
    }
    for _ in 0..items.take_refills() {
        let added = sim.room.pump_mut().refill(cfg.0.room.coolant_canister);
        info!("reactor: coolant canister, the tank gains {added:.0} %");
    }
}
