//! Coolant by hand: the coolant pump's tank runs down while the pump runs, in proportion to the heat
//! it carries away, and a dry tank is no flow. A canister from the engine room's rack refills it: a hand
//! holds LMB on the pump with a canister in the selected slot (`refill_coolant`, `Request::Use`), the
//! hold shows on the reticle as the repair hold does, and the host adds `RoomTuning::coolant_canister`
//! per canister it takes (`host_refills`).
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
use crate::controls::Controls;
use crate::dev_area::layout::ROOM;
use crate::displays::ActiveDisplay;
use crate::items::{Items, LOCAL};
use crate::repair::{aimed, RepairConfig};
use crate::walker::{Player, EYE_HEIGHT};
use avian3d::prelude::{Position, Rotation};
use bevy::math::DVec3;
use item_core::wire::RepairDevice;
use item_core::{FrameRef, Placement, Request};
use walker_core::Frame;

pub const REFILL: &str = "refill_coolant";
pub const HOLD_S: f64 = 3.0;
/// The rack in the engine room's north-west corner, relative to the room's centre (m): against the
/// north wall (inner face y -11.0), west of the door's walk. Its size (m).
pub const RACK_AT: [f64; 2] = [-4.5, 3.7];
pub const RACK_SIZE: [f64; 3] = [0.5, 0.9, 0.5];
pub const ON_RACK: u32 = 3;

#[derive(Resource, Default, Debug)]
pub struct CoolantHold {
    pub held: f64,
    /// What the others were last told (`Items::ask_refill`), so a change is sent once.
    pub sent: bool,
}

impl CoolantHold {
    /// The hold's progress (0..1) for the reticle's ring.
    pub fn progress(&self) -> Option<f64> {
        (self.held > 0.0).then(|| (self.held / HOLD_S).clamp(0.0, 1.0))
    }

    fn sync(&mut self, items: &mut Items, running: bool) {
        if self.sent != running {
            self.sent = running;
            items.ask_refill(running);
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, host_refills.in_set(crate::dev_area::ship::ShipSet::Asks));
    app.world_mut().resource_mut::<DevAreaDevices>().add(PlaceholderBox {
        id: "coolant_rack".into(),
        zone: ROOM,
        local_pos: RACK_AT,
        size: RACK_SIZE,
        turn_deg: 0.0,
        colour: [0.4, 0.42, 0.46],
        panel: None,
    });
    app.init_resource::<CoolantHold>();
    app.add_systems(FixedUpdate, put_canisters.in_set(crate::phases::Fx::Effects));
    app.add_systems(
        FixedUpdate,
        coolant_use
            .after(crate::scenario::run_script)
            .after(crate::items_net::items_net_in)
            .before(crate::health::use_input)
            .before(crate::displays::use_display)
            .in_set(crate::phases::Fx::Input),
    );
}

fn put_canisters(time: Res<Time>, planet: Res<PlanetRes>, area: Res<DevArea>, mut items: ResMut<Items>, ships: Query<(), With<crate::ship::Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = true;
    let Some(d) = items.defs.get(REFILL_ITEM).cloned() else { return };
    let p = ROOM.point(RACK_AT);
    let (pos, up) = frame.ground(&planet, p[0], p[1], RACK_SIZE[1] + d.shape.extents[1] * 0.5 + 0.02);
    let fwd = frame.rotation(up, 0.0) * DVec3::NEG_Z;
    let at = Placement { frame: FrameRef::Planet(planet.id.0 as u32), pos: pos.to_array(), vel: [0.0; 3], forward: fwd.to_array() };
    items.spawn(time.elapsed_secs_f64(), d.stack_of(ON_RACK), at);
}

pub const REFILL_ITEM: &str = "coolant_canister";

#[allow(clippy::too_many_arguments)]
fn coolant_use(time: Res<Time>, mut controls: ResMut<Controls>, mut items: ResMut<Items>, cfg: Res<RepairConfig>, active: Res<ActiveDisplay>, mut hold: ResMut<CoolantHold>, players: Query<&Player>, machines: Query<(&DevBox, &Position, &Rotation)>) {
    let dt = time.delta_secs_f64();
    let Ok(pl) = players.single() else {
        hold.held = 0.0;
        hold.sync(&mut items, false);
        return;
    };
    let slot = items.client.view.selected;
    let canister = items.client.view.slot(slot).and_then(|s| items.defs.of(s)).and_then(|d| d.use_effect.as_deref()) == Some(REFILL);
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let at_pump = aimed(eye, pl.world_look(Frame::IDENTITY), &machines, cfg.0.reach_m) == Some(RepairDevice::CoolantPump);
    let on_foot = pl.ship.is_none() && !pl.seated && !pl.fly && active.using().is_none() && !controls.free_cursor;
    if !(canister && at_pump && on_foot) || !controls.primary_held {
        hold.held = 0.0;
        hold.sync(&mut items, false);
        return;
    }
    hold.held += dt;
    if hold.held >= HOLD_S {
        hold.held = 0.0;
        controls.click = false;
        let _ = items.request(time.elapsed_secs_f64(), Request::Use { slot, target: LOCAL });
    }
    // Still running after a refill: the others keep seeing REFILLING, no flicker at the end.
    hold.sync(&mut items, true);
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
