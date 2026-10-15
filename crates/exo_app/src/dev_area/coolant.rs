//! Not yet: the hold has no ring on the reticle (TODO(initiator)), other players see no refill, the
//! pump states Low and High (later, with the pump's second setting).
use super::*;
use crate::controls::Controls;
use crate::dev_area::engine_room::CORRIDOR;
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
/// The rack in the corridor, relative to the corridor's centre (m), its size (m).
pub const RACK_AT: [f64; 2] = [0.6, -1.5];
pub const RACK_SIZE: [f64; 3] = [0.5, 0.9, 0.5];
pub const ON_RACK: u32 = 3;

#[derive(Resource, Default, Debug)]
pub struct CoolantHold {
    pub held: f64,
}

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(PlaceholderBox {
        id: "coolant_rack".into(),
        zone: CORRIDOR,
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
    let p = CORRIDOR.point(RACK_AT);
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
        return;
    };
    let slot = items.client.view.selected;
    let canister = items.client.view.slot(slot).and_then(|s| items.defs.of(s)).and_then(|d| d.use_effect.as_deref()) == Some(REFILL);
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let at_pump = aimed(eye, pl.world_look(Frame::IDENTITY), &machines, cfg.0.reach_m) == Some(RepairDevice::CoolantPump);
    let on_foot = pl.ship.is_none() && !pl.seated && !pl.fly && active.using().is_none() && !controls.free_cursor;
    if !(canister && at_pump && on_foot) || !controls.primary_held {
        hold.held = 0.0;
        return;
    }
    hold.held += dt;
    if hold.held >= HOLD_S {
        hold.held = 0.0;
        controls.click = false;
        let _ = items.request(time.elapsed_secs_f64(), Request::Use { slot, target: LOCAL });
    }
}
