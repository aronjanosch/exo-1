//! Fuel rods by hand. A rod is an item you carry (`fuel_rod`, `fuel_rod_hot`, `fuel_rod_spicy`,
//! `content/items/fuel_rod*.json`); its item condition is the rod's life (100 new, 0 spent), kept when
//! it is carried, loaded or pulled. The reactor is an item container with fuel slots (Barotrauma style):
//! a rod in a slot is its available fuel.
//! A rack in the engine room holds loose rods. Load: a rod in hand, LMB held `HOLD_S` on the reactor, and
//! `Request::Use` puts it into a free slot (`insert_rod`); a full reactor refuses it with a notice.
//! Pull (the emergency brake, `pull_rod`): empty hands, LMB held `HOLD_S` on the reactor; the rod with
//! the most life comes back as an item. A core above its optimal band burns the hand once (`device_core::minigame::hurt_of`).
//! Values: TODO(initiator) (the rack's stock, the burn rule, hold times).
use crate::dev_area::*;
use crate::controls::Controls;
use crate::dev_area::engineering::coolant::HOLD_S;
use crate::dev_area::{layout::{REACTOR_AT, ROOM}};
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

pub const LOAD: &str = "load_rod";
/// Beside the coolant rack, against the north wall, relative to the engine room's centre (m).
pub const RACK_AT: [f64; 2] = [-3.6, 3.7];
pub const RACK_SIZE: [f64; 3] = [0.5, 0.9, 0.5];
pub const ON_RACK: [(&str, u32); 2] = [("fuel_rod", 2), ("fuel_rod_hot", 1)];

pub const KINDS: [(&str, &str); 3] = [("standard", "fuel_rod"), ("hot", "fuel_rod_hot"), ("spicy", "fuel_rod_spicy")];

pub fn kind_of_item(item: &str) -> Option<&'static str> {
    KINDS.iter().find(|(_, i)| *i == item).map(|(k, _)| *k)
}

pub fn item_of_kind(kind: &str) -> Option<&'static str> {
    KINDS.iter().find(|(k, _)| *k == kind).map(|(_, i)| *i)
}

/// The rod the brake pulls: the one with the most life (the first of equals), None when there
/// is no rod.
pub fn pull_index(rods: &[FuelRod]) -> Option<usize> {
    rods.iter().enumerate().reduce(|best, r| if r.1.life > best.1.life { r } else { best }).map(|(i, _)| i)
}

pub fn fuel_kind(kind: &str) -> FuelKind {
    match kind {
        "hot" => FuelKind::Hot,
        "spicy" => FuelKind::Spicy,
        _ => FuelKind::Standard,
    }
}

pub fn rod_stack(def: &ItemDef, life: f64) -> Stack {
    Stack::new(def.item().with_condition(life), 1)
}

pub fn reactor_front(area: &DevArea, planet: &PlanetRes) -> Option<Placement> {
    let frame = area.frame?;
    let (pos, up) = frame.ground(planet, REACTOR_AT[0], REACTOR_AT[1] + 1.4, 0.05);
    let fwd = frame.rotation(up, 0.0) * DVec3::NEG_Z;
    Some(Placement { frame: FrameRef::Planet(planet.id.0 as u32), pos: pos.to_array(), vel: [0.0; 3], forward: fwd.to_array() })
}

#[derive(Resource, Default, Debug)]
pub struct RodHold {
    pub held: f64,
}

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, host_rods.in_set(crate::dev_area::ship::ShipSet::Asks));
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
        zone: ROOM,
        local_pos: RACK_AT,
        size: RACK_SIZE,
        turn_deg: 0.0,
        colour: [0.4, 0.42, 0.46],
        panel: None,
    });
    app.add_systems(FixedUpdate, put_rods.in_set(crate::phases::Fx::Effects));
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brake_takes_the_most_life() {
        let rod = |life| FuelRod { kind: "standard".into(), heat: 80.0, life };
        assert_eq!(pull_index(&[]), None);
        assert_eq!(pull_index(&[rod(30.0), rod(90.0), rod(10.0)]), Some(1));
        assert_eq!(pull_index(&[rod(50.0), rod(50.0)]), Some(0), "the first of equals");
        assert_eq!(pull_index(&[rod(0.0)]), Some(0), "a spent rod is still a rod");
    }
}

#[allow(clippy::too_many_arguments)]
fn rod_hands(time: Res<Time>, mut controls: ResMut<Controls>, mut items: ResMut<Items>, cfg: Res<RepairConfig>, reactor: Res<crate::dev_area::ship::ReactorConfig>, active: Res<ActiveDisplay>, mut hold: ResMut<RodHold>, mut gp: ResMut<Gameplay>, players: Query<&Player>, machines: Query<(&DevBox, &Position, &Rotation)>) {
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

/// Host: the brake's pulls (a rod out of the reactor into the hand, a burn when the core is hot)
/// and the rods a hand loaded (each into a free slot, or back to the hand when none is free).
#[allow(clippy::too_many_arguments)]
fn host_rods(time: Res<Time>, mut items: ResMut<Items>, cfg: Res<crate::dev_area::ship::ReactorConfig>, mut sim: ResMut<crate::dev_area::ship::ReactorSim>, area: Res<DevArea>, planet: Res<PlanetRes>, repair: Res<RepairConfig>, mut health: Option<ResMut<crate::health::HealthState>>) {
    if items.role == crate::items::Role::Client {
        return;
    }
    for player in std::mem::take(&mut items.pull_asks) {
        let Some(i) = pull_index(sim.room.reactor().rods()) else {
            info!("reactor: the brake, no rod in a slot");
            continue;
        };
        let Some((rod, temp)) = sim.room.reactor_mut().pull_rod(i) else { continue };
        let band = sim.room.reactor().bands().temp_optimal;
        if temp > band.hi {
            let burn = device_core::minigame::hurt_of(device_core::minigame::Machine::Hot, device_core::minigame::Press::Miss, &repair.0).burn;
            if let Some(st) = health.as_deref_mut()
                && let Some(b) = st.bodies.get_mut(&player)
            {
                b.health.add(&st.tuning, "burn", burn);
            }
        }
        info!("reactor: the brake pulled the {} rod ({:.0} life) at {temp:.0} degrees", rod.kind, rod.life);
        let Some(def) = item_of_kind(&rod.kind).and_then(|id| items.defs.get(id)).cloned() else { continue };
        let now = time.elapsed_secs_f64();
        if let Err(stack) = items.give(now, player, rod_stack(&def, rod.life))
            && let Some(at) = reactor_front(&area, &planet)
        {
            items.spawn(now, stack, at);
        }
    }
    // Fuel rods the host took out of a hand (`dev_area::engineering::rods`): each goes into a free slot. A reactor
    for load in items.take_rod_loads() {
        let Some(mut rod) = kind_of_item(&load.item).and_then(|k| cfg.0.rods.rod(k)) else { continue };
        rod.life = load.life;
        let kind = rod.kind.clone();
        if let Err(rod) = sim.room.reactor_mut().insert_rod(rod) {
            info!("reactor: no free slot for the {} rod, it goes back", rod.kind);
            let Some(def) = items.defs.get(&load.item).cloned() else { continue };
            if let Err(stack) = items.give(time.elapsed_secs_f64(), load.player, rod_stack(&def, rod.life)) {
                if let Some(at) = reactor_front(&area, &planet) {
                    items.spawn(time.elapsed_secs_f64(), stack, at);
                }
            }
        } else {
            info!("reactor: the {kind} rod is in a slot ({:.0} life)", load.life);
        }
    }
}
