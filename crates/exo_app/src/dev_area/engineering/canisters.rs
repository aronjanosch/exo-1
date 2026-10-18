//! Antimatter canisters by hand. A canister is an item you carry (`antimatter_canister`, `antimatter_canister_hot`, `antimatter_canister_spicy`,
//! `content/items/antimatter_canister*.json`); its item condition is the canister's life (100 new, 0 spent), kept when
//! it is carried, loaded or pulled. The reactor is an item container with canister slot (Barotrauma style):
//! a canister in a slot is its available fill.
//! A rack in the engine room holds loose canisters. Load and pull are F at a slot of the reactor model
//! (`reactor_room::press`): `Request::Load` puts the canister in hand into the slot looked at (`insert_canister_at`), the brake
//! (`ask_pull_from`) takes that slot's canister back as an item. A core above its optimal band burns the hand once
//! (`device_core::minigame::hurt_of`).
//! Values: TODO(initiator) (the rack's stock, the burn rule).
use crate::dev_area::*;
use crate::dev_area::layout::{REACTOR_FRONT_AT, ROOM};
use crate::repair::RepairConfig;
use item_core::wire::CanisterKind;
use crate::items::Items;
use bevy::math::DVec3;
use item_core::container::Stack;
use item_core::{FrameRef, ItemDef, Placement};

pub const LOAD: &str = "load_canister";
/// Beside the coolant rack, against the north wall, relative to the engine room's centre (m).
pub const RACK_AT: [f64; 2] = [-3.6, 3.7];
pub const RACK_SIZE: [f64; 3] = [0.5, 0.9, 0.5];
/// Parked (#306 Parkplatz): only the plain canister lies on the rack; the hot one was ("antimatter_canister_hot", 1).
pub const ON_RACK: [(&str, u32); 1] = [("antimatter_canister", 2)];

pub const KINDS: [(&str, &str); 3] = [("standard", "antimatter_canister"), ("hot", "antimatter_canister_hot"), ("spicy", "antimatter_canister_spicy")];

pub fn kind_of_item(item: &str) -> Option<&'static str> {
    KINDS.iter().find(|(_, i)| *i == item).map(|(k, _)| *k)
}

pub fn item_of_kind(kind: &str) -> Option<&'static str> {
    KINDS.iter().find(|(k, _)| *k == kind).map(|(_, i)| *i)
}

pub fn canister_kind(kind: &str) -> CanisterKind {
    match kind {
        "hot" => CanisterKind::Hot,
        "spicy" => CanisterKind::Spicy,
        _ => CanisterKind::Standard,
    }
}

pub fn canister_stack(def: &ItemDef, life: f64) -> Stack {
    Stack::new(def.item().with_condition(life), 1)
}

/// The selected bar slot holds a canister (the item whose use is `LOAD`).
pub fn holds_canister(items: &Items) -> bool {
    let slot = items.client.view.selected;
    items.client.view.slot(slot).and_then(|s| items.defs.of(s)).is_some_and(|d| d.use_effect.as_deref() == Some(LOAD))
}

pub fn reactor_front(area: &DevArea, planet: &PlanetRes) -> Option<Placement> {
    let frame = area.frame?;
    let (pos, up) = frame.ground(planet, REACTOR_FRONT_AT[0], REACTOR_FRONT_AT[1], 0.05);
    let fwd = frame.rotation(up, 0.0) * DVec3::NEG_Z;
    Some(Placement { frame: FrameRef::Planet(planet.id.0 as u32), pos: pos.to_array(), vel: [0.0; 3], forward: fwd.to_array() })
}

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, host_canisters.in_set(crate::dev_area::ship::ShipSet::Asks));
    app.world_mut().resource_mut::<DevAreaDevices>().add(PlaceholderBox {
        id: "canister_rack".into(),
        zone: ROOM,
        local_pos: RACK_AT,
        size: RACK_SIZE,
        turn_deg: 0.0,
        colour: [0.4, 0.42, 0.46],
        panel: None,
    });
    app.add_systems(FixedUpdate, put_canisters.in_set(crate::phases::Fx::Effects));
}

fn put_canisters(time: Res<Time>, planet: Res<PlanetRes>, area: Res<DevArea>, mut items: ResMut<Items>, ships: Query<(), With<crate::ship::Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = true;
    let mut n = 0;
    for (id, count) in ON_RACK {
        let Some(d) = items.defs.get(id).cloned() else { continue };
        for _ in 0..count {
            let dx = (n as f64 - 1.0) * 0.15;
            let p = ROOM.point([RACK_AT[0] + dx, RACK_AT[1]]);
            let (pos, up) = frame.ground(&planet, p[0], p[1], RACK_SIZE[1] + d.shape.extents[1] * 0.5 + 0.02);
            let fwd = frame.rotation(up, 0.0) * DVec3::NEG_Z;
            let at = Placement { frame: FrameRef::Planet(planet.id.0 as u32), pos: pos.to_array(), vel: [0.0; 3], forward: fwd.to_array() };
            items.spawn(time.elapsed_secs_f64(), d.stack_of(1), at);
            n += 1;
        }
    }
}

/// Host: the brake's pulls (a slot's canister out of the reactor into the hand, a burn when the core is hot)
/// and the canisters a hand loaded (each into a free slot, or back to the hand when none is free).
#[allow(clippy::too_many_arguments)]
fn host_canisters(time: Res<Time>, mut items: ResMut<Items>, cfg: Res<crate::dev_area::ship::ReactorConfig>, mut sim: ResMut<crate::dev_area::ship::ReactorSim>, area: Res<DevArea>, planet: Res<PlanetRes>, repair: Res<RepairConfig>, mut health: Option<ResMut<crate::health::HealthState>>) {
    if items.role == crate::items::Role::Client {
        return;
    }
    for (player, slot) in std::mem::take(&mut items.pull_asks) {
        // F at a slot names it (an empty one gives nothing).
        let Some((canister, heat)) = sim.room.reactor_mut().pull_canister_from_slot(slot as usize) else {
            info!("reactor: the brake, no canister to pull");
            continue;
        };
        let band = sim.room.reactor().bands().heat_optimal;
        if heat > band.hi {
            let burn = device_core::minigame::hurt_of(device_core::minigame::Machine::Hot, device_core::minigame::Press::Miss, &repair.0).burn;
            if let Some(st) = health.as_deref_mut()
                && let Some(b) = st.bodies.get_mut(&player)
            {
                b.health.add(&st.tuning, "burn", burn);
            }
        }
        info!("reactor: the brake pulled the {} canister ({:.0} life) at {heat:.0} degrees", canister.kind, canister.life);
        let Some(def) = item_of_kind(&canister.kind).and_then(|id| items.defs.get(id)).cloned() else { continue };
        let now = time.elapsed_secs_f64();
        if let Err(stack) = items.give(now, player, canister_stack(&def, canister.life))
            && let Some(at) = reactor_front(&area, &planet)
        {
            items.spawn(now, stack, at);
        }
    }
    // Antimatter canisters the host took out of a hand (`dev_area::engineering::canisters`): each goes into a free slot. A reactor
    for load in items.take_canister_loads() {
        let Some(mut canister) = kind_of_item(&load.item).and_then(|k| cfg.0.canisters.canister(k)) else { continue };
        canister.life = load.life;
        let kind = canister.kind.clone();
        let put = match load.into {
            Some(slot) => sim.room.reactor_mut().insert_canister_at(slot as usize, canister),
            None => sim.room.reactor_mut().insert_canister(canister),
        };
        if let Err(canister) = put {
            info!("reactor: no free slot for the {} canister, it goes back", canister.kind);
            let Some(def) = items.defs.get(&load.item).cloned() else { continue };
            if let Err(stack) = items.give(time.elapsed_secs_f64(), load.player, canister_stack(&def, canister.life)) {
                if let Some(at) = reactor_front(&area, &planet) {
                    items.spawn(time.elapsed_secs_f64(), stack, at);
                }
            }
        } else {
            info!("reactor: the {kind} canister is in a slot ({:.0} life)", load.life);
        }
    }
}
