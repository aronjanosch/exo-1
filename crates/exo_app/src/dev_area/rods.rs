//! Fuel rods by hand (#300, lane rods). A rod is an item you carry (`fuel_rod`, `fuel_rod_hot`,
//! `fuel_rod_spicy`, `content/items/fuel_rod*.json`); its item condition is the rod's life (100 new,
//! 0 spent), kept when it is carried, loaded or pulled. Barotrauma style: the reactor is an item
//! container with fuel slots, a rod in a slot is its available fuel.
//!
//! - A rod rack in the corridor, north of the extinguisher rack, holds two standard rods and one hot rod
//!   (loose items on the rack, as the coolant canisters are). Taken with F.
//! - Load: a rod in the selected slot, the look on the reactor within reach, LMB held `HOLD_S`
//!   seconds: `Request::Use` goes to the host, which puts the rod into a free slot (`insert_rod`)
//!   and the item is gone. Refused with a notice when all slots are full (the client checks its
//!   snapshot first; the host gives the rod back if it was full after all).
//! - Pull (the emergency brake, `reactor_core` `pull_rod`): empty hands, the look on the reactor,
//!   LMB held `HOLD_S` seconds: `Pull` goes to the host, which pulls the rod with the
//!   most life. The rod comes back as an item with its kind and life. A core above its optimal band
//!   burns the hand once (the reactor's miss burn, `device_core::minigame::hurt_of`, as repair uses it).
//!
//! Values: TODO(initiator) (the rack's stock, the burn rule, hold times).
use super::*;
use crate::controls::Controls;
use crate::dev_area::coolant::HOLD_S;
use crate::dev_area::engine_room::{CORRIDOR, REACTOR_AT};
use crate::displays::ActiveDisplay;
use crate::gameplay::Gameplay;
use crate::items::{Items, LOCAL};
use crate::repair::{aimed, RepairConfig};
use crate::walker::{Player, EYE_HEIGHT};
use avian3d::prelude::{Position, Rotation};
use bevy::math::DVec3;
use gameplay_core::notice::{Notice, NoticeKind};
use item_core::container::Stack;
use item_core::wire::{FuelKind, RepairDevice};
use item_core::{FrameRef, ItemDef, Placement, Request};
use reactor_core::FuelRod;
use walker_core::Frame;

/// The `use` effect of a fuel rod: loading it into the reactor.
pub const LOAD: &str = "load_rod";
/// The rack in the corridor, relative to the corridor's centre (m), its size (m). On the extinguisher
/// rack's side (x = -0.6), but north of it (y = 1.5): from the walkway south of the extinguisher
/// rack, the aim ray runs through this rack's loose rods, and `interact` takes a loose item in the
/// cone before a rack (#300, `scenario_extinguisher`).
pub const RACK_AT: [f64; 2] = [-0.6, 1.5];
pub const RACK_SIZE: [f64; 3] = [0.5, 0.9, 0.5];
/// The rods on the rack when the area is made: (item id, how many), in a row across the rack.
pub const ON_RACK: [(&str, u32); 2] = [("fuel_rod", 2), ("fuel_rod_hot", 1)];

/// The item of a fuel rod kind (`reactor.json` `rods`), and the kind of a rod item.
pub const KINDS: [(&str, &str); 3] = [("standard", "fuel_rod"), ("hot", "fuel_rod_hot"), ("spicy", "fuel_rod_spicy")];

/// The rod kind (`reactor.json`) of a fuel rod item, None for anything else.
pub fn kind_of_item(item: &str) -> Option<&'static str> {
    KINDS.iter().find(|(_, i)| *i == item).map(|(k, _)| *k)
}

/// The item of a rod kind.
pub fn item_of_kind(kind: &str) -> Option<&'static str> {
    KINDS.iter().find(|(k, _)| *k == kind).map(|(_, i)| *i)
}

/// The rod the brake pulls: the one with the most life (the first of equals), None when there
/// is no rod.
pub fn pull_index(rods: &[FuelRod]) -> Option<usize> {
    rods.iter().enumerate().reduce(|best, r| if r.1.life > best.1.life { r } else { best }).map(|(i, _)| i)
}

/// The kind of a rod on the readout (`reactor.json` `rods`); an unknown kind shows as standard.
pub fn fuel_kind(kind: &str) -> FuelKind {
    match kind {
        "hot" => FuelKind::Hot,
        "spicy" => FuelKind::Spicy,
        _ => FuelKind::Standard,
    }
}

/// The stack of one rod of `def` with the given life (its item condition).
pub fn rod_stack(def: &ItemDef, life: f64) -> Stack {
    Stack::new(def.item().with_condition(life), 1)
}

/// Where a rod lies when it is dropped at the reactor's front (the hands are full): on the floor,
/// a step in front of the reactor.
pub fn reactor_front(area: &DevArea, planet: &PlanetRes) -> Option<Placement> {
    let frame = area.frame?;
    let (pos, up) = frame.ground(planet, REACTOR_AT[0], REACTOR_AT[1] + 1.4, 0.05);
    let fwd = frame.rotation(up, 0.0) * DVec3::NEG_Z;
    Some(Placement { frame: FrameRef::Planet(planet.id.0 as u32), pos: pos.to_array(), vel: [0.0; 3], forward: fwd.to_array() })
}

/// The client's hold on the reactor with a rod in hand or empty hands: seconds so far.
#[derive(Resource, Default, Debug)]
pub struct RodHold {
    pub held: f64,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<RodHold>();
    app.add_systems(
        FixedUpdate,
        rod_hands
            .after(crate::scenario::run_script)
            .after(crate::items_net::items_net_in)
            .before(crate::health::use_input)
            .before(crate::displays::use_display)
            .in_set(crate::phases::Fx::Input),
    );
    app.world_mut().resource_mut::<DevAreaDevices>().add(PlaceholderBox {
        id: "rod_rack".into(),
        zone: CORRIDOR,
        local_pos: RACK_AT,
        size: RACK_SIZE,
        turn_deg: 0.0,
        colour: [0.4, 0.42, 0.46],
        panel: None,
    });
    app.add_systems(FixedUpdate, put_rods.in_set(crate::phases::Fx::Effects));
}

/// Once: the rods on the rack, in a row across its top (planet frame).
fn put_rods(time: Res<Time>, planet: Res<PlanetRes>, area: Res<DevArea>, mut items: ResMut<Items>, ships: Query<(), With<crate::ship::Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = true;
    let mut n = 0;
    for (id, count) in ON_RACK {
        let Some(d) = items.defs.get(id).cloned() else { continue };
        for _ in 0..count {
            // Across the rack: the rods a hand's width apart, the middle one at the centre.
            let dx = (n as f64 - 1.0) * 0.15;
            let p = CORRIDOR.point([RACK_AT[0] + dx, RACK_AT[1]]);
            let (pos, up) = frame.ground(&planet, p[0], p[1], RACK_SIZE[1] + d.shape.extents[1] * 0.5 + 0.02);
            let fwd = frame.rotation(up, 0.0) * DVec3::NEG_Z;
            let at = Placement { frame: FrameRef::Planet(planet.id.0 as u32), pos: pos.to_array(), vel: [0.0; 3], forward: fwd.to_array() };
            items.spawn(time.elapsed_secs_f64(), d.stack_of(1), at);
            n += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_items_match() {
        assert_eq!(kind_of_item("fuel_rod"), Some("standard"));
        assert_eq!(kind_of_item("fuel_rod_hot"), Some("hot"));
        assert_eq!(kind_of_item("fuel_rod_spicy"), Some("spicy"));
        assert_eq!(kind_of_item("coolant_canister"), None);
        for (kind, item) in KINDS {
            assert_eq!(item_of_kind(kind), Some(item));
            assert_eq!(kind_of_item(item), Some(kind));
        }
        assert_eq!(item_of_kind("uranium"), None);
    }

    #[test]
    fn the_brake_takes_the_most_life() {
        let rod = |life| FuelRod { kind: "standard".into(), heat: 80.0, life };
        assert_eq!(pull_index(&[]), None);
        assert_eq!(pull_index(&[rod(30.0), rod(90.0), rod(10.0)]), Some(1));
        assert_eq!(pull_index(&[rod(50.0), rod(50.0)]), Some(0), "the first of equals");
        assert_eq!(pull_index(&[rod(0.0)]), Some(0), "a spent rod is still a rod");
    }
}

/// Client: the look on the reactor within reach, LMB held `HOLD_S` seconds, with a fuel rod in the
/// selected slot (load: `Request::Use` goes to the host) or the hands empty (the brake:
/// `Pull` goes to the host). A load into a reactor with no free slot (its snapshot)
/// is refused with a notice and no request. Any other state ends the hold.
#[allow(clippy::too_many_arguments)]
fn rod_hands(time: Res<Time>, mut controls: ResMut<Controls>, mut items: ResMut<Items>, cfg: Res<RepairConfig>, reactor: Res<crate::dev_area::reactor::ReactorConfig>, active: Res<ActiveDisplay>, mut hold: ResMut<RodHold>, mut gp: ResMut<Gameplay>, players: Query<&Player>, machines: Query<(&DevBox, &Position, &Rotation)>) {
    let dt = time.delta_secs_f64();
    let Ok(pl) = players.single() else {
        hold.held = 0.0;
        return;
    };
    let slot = items.client.view.selected;
    let in_hand = items.client.view.slot(slot).and_then(|s| items.defs.of(s)).map(|d| d.use_effect.as_deref() == Some(LOAD));
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let at_reactor = aimed(eye, pl.world_look(Frame::IDENTITY), &machines, cfg.0.reach_m) == Some(RepairDevice::Reactor);
    let on_foot = pl.ship.is_none() && !pl.seated && !pl.fly && active.using().is_none() && !controls.free_cursor;
    // A rod in hand loads it; empty hands pull. Any other item in hand does nothing here.
    let load = in_hand == Some(true);
    let pull = in_hand.is_none();
    if !(at_reactor && on_foot && (load || pull)) || !controls.primary_held {
        hold.held = 0.0;
        return;
    }
    hold.held += dt;
    if hold.held < HOLD_S {
        return;
    }
    hold.held = 0.0;
    controls.click = false;
    if pull {
        items.ask_pull();
        return;
    }
    let slots = reactor.0.reactor.slots;
    let used = items.reactor.map_or(0, |r| r.fuel.iter().filter(|t| t.rod.is_some()).count());
    if used >= slots {
        gp.notify(Notice::new(NoticeKind::Warning, "notice.rods.full"));
        return;
    }
    let _ = items.request(time.elapsed_secs_f64(), Request::Use { slot, target: LOCAL });
}
