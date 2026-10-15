//! The breathing mask (r38): the item "O2 MASK" (`content/items/o2_mask.json`) is put on and taken
//! off by its use, and worn it breathes from its tank, not the room. Its condition is the tank:
//! while worn the body needs air it runs down (the host, `mask_breath`), and at the O2 SHELF beside
//! the oxygen generator the held mask refills while the generator makes air (`mask_refill`).
//! The numbers and their sources (Barotrauma `divinggear.xml`, `containers.xml`,
//! `Character.cs:3817-3851`, `StatusEffect.cs:2819`) are in `health_core::mask`.
//!
//! Host only decides: who wears which mask, the drain, the refill. The clients hold the shelf and
//! send the use, as the coolant hold does (`coolant.rs`). The shelf's hold is not checked against
//! the position on the host (as with the coolant canister), so a client can claim a refill it is
//! not standing at. TODO(initiator): accept that, or check the position here too.
//!
//! The space suit (r41, #50) is the same gear: "SUIT" (`content/items/suit.json`) is put on by its
//! own use and breathes from its own tank the same way (`health_core::mask::Mask`, the same rates,
//! the shelf refills it too). Worn, it also counts as suited (`SuitWorn`): no vacuum damage
//! (`health_core::vacuum`). One gear is worn at a time, in the one slot the mask uses.
use crate::dev_area::*;
use crate::controls::Controls;
use crate::displays::ActiveDisplay;
use crate::health::{health_step, HealthState};
use crate::items::{Items, Role, LOCAL};
use crate::repair::RepairConfig;
use crate::ship::Ship;
use crate::walker::{Player, EYE_HEIGHT};
use avian3d::prelude::{Position, Rotation};
use bevy::math::DVec3;
use device_core::minigame::ray_hits_box;
use health_core::mask::Mask;
use health_core::Life;
use item_core::{FrameRef, Placement, PlayerId, Request};
use std::collections::{BTreeMap, BTreeSet};
use walker_core::Frame;

use crate::dev_area::layout::ROOM;
use crate::dev_area::ship::{ReactorConfig, ReactorSim};

pub const ITEM: &str = "o2_mask";
/// The use that puts the mask on or takes it off (`use` in the item's file).
pub const USE: &str = "wear_mask";
/// The space suit (r41): its item and its use, the same gear as the mask (see the module doc).
pub const SUIT_ITEM: &str = "suit";
pub const SUIT_USE: &str = "wear_suit";
/// The shelf beside the oxygen generator, relative to the engine room's centre (m). Clear of the
/// reactor (x -15..-13), the pump (z -18..-17) and the generator (z -13.4..-12.6, x -9.8..-9.0).
pub const SHELF_ID: &str = "o2_shelf";
pub const SHELF_AT: [f64; 2] = [4.6, 0.2];
pub const SHELF_SIZE: [f64; 3] = [0.8, 1.2, 0.8];

/// Host: who wears a mask, and in which slot it is.
#[derive(Resource, Default, Debug)]
pub struct MaskWorn(pub BTreeMap<PlayerId, usize>);

/// Host: the bodies whose worn gear is a suit (r41). The health step takes them as suited.
#[derive(Resource, Default, Debug)]
pub struct SuitWorn(pub BTreeSet<PlayerId>);

/// Whether an item kind is breathing gear (the mask or the suit).
pub fn is_gear_item(kind: &str) -> bool {
    kind == ITEM || kind == SUIT_ITEM
}

/// Whether an item's use puts breathing gear on or takes it off (the mask's or the suit's).
pub fn is_gear_use(effect: &str) -> bool {
    effect == USE || effect == SUIT_USE
}

/// Host: the oxygen fraction (0..1) each masked body breathed this step. The health step takes
/// it for that body instead of the room's air.
#[derive(Resource, Default, Debug)]
pub struct MaskAir(pub BTreeMap<PlayerId, f64>);

/// The shelf hold, here: whether it runs, and the held mask's tank (0..1) for the reticle's ring.
#[derive(Resource, Default, Debug)]
pub struct MaskHold {
    pub tank: Option<f64>,
    /// What the others were last told (`Items::ask_refill`), so a change is sent once.
    pub sent: bool,
}

impl MaskHold {
    /// The tank's fill (0..1) for the ring while the shelf refills it.
    pub fn progress(&self) -> Option<f64> {
        self.tank
    }
}

pub fn plugin(app: &mut App) {
    {
        let mut devices = app.world_mut().resource_mut::<DevAreaDevices>();
        devices.add(PlaceholderBox {
            id: SHELF_ID.into(),
            zone: ROOM,
            local_pos: SHELF_AT,
            size: SHELF_SIZE,
            turn_deg: 0.0,
            colour: [0.3, 0.45, 0.6],
            panel: None,
        });
    }
    app.init_resource::<MaskWorn>().init_resource::<MaskAir>().init_resource::<MaskHold>().init_resource::<SuitWorn>();
    app.add_systems(FixedUpdate, put_mask.in_set(crate::phases::Fx::Effects));
    app.add_systems(
        FixedUpdate,
        mask_input
            .after(crate::scenario::run_script)
            .after(crate::items_net::items_net_in)
            .before(crate::health::use_input)
            .before(crate::displays::use_display)
            .in_set(crate::phases::Fx::Input),
    );
    app.add_systems(FixedUpdate, mask_hold.after(crate::scenario::run_script).after(crate::items_net::items_net_in).in_set(crate::phases::Fx::Input));
    app.add_systems(FixedUpdate, mask_refill.after(crate::items_net::items_net_in).in_set(crate::phases::Fx::Effects));
    app.add_systems(FixedUpdate, (mask_toggle, mask_breath).chain().after(crate::items::deliver).before(health_step).in_set(crate::phases::Fx::Walker));
}

/// Once, on the corridor's extinguisher rack (the mask lies on top of it).
fn put_mask(time: Res<Time>, planet: Res<PlanetRes>, area: Res<DevArea>, mut items: ResMut<Items>, ships: Query<(), With<Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = true;
    let Some(d) = items.defs.get(ITEM).cloned() else { return };
    let rack = crate::dev_area::fire::extinguisher::boxes()[0].clone();
    let p = rack.zone.point(rack.local_pos);
    let (pos, up) = frame.ground(&planet, p[0], p[1], rack.size[1] + d.shape.extents[1] * 0.5 + 0.02);
    let fwd = frame.rotation(up, 0.0) * DVec3::NEG_Z;
    let at = Placement { frame: FrameRef::Planet(planet.id.0 as u32), pos: pos.to_array(), vel: [0.0; 3], forward: fwd.to_array() };
    items.spawn(time.elapsed_secs_f64(), d.stack_of(1), at);
}

fn holds_mask(items: &Items, who: PlayerId, slot: usize) -> bool {
    items.host.inventory(who).slot(slot).is_some_and(|s| is_gear_item(s.item.kind.as_str()))
}

/// Whether the slot holds a suit (host).
fn holds_suit(items: &Items, who: PlayerId, slot: usize) -> bool {
    items.host.inventory(who).slot(slot).is_some_and(|s| s.item.kind.as_str() == SUIT_ITEM)
}

/// Whether the ray from the eye hits the shelf's box within reach.
fn shelf_aimed(eye: DVec3, look: DVec3, machines: &Query<(&DevBox, &Position, &Rotation)>, reach: f64) -> bool {
    let dir = look.normalize_or_zero();
    machines.iter().any(|(b, p, r)| {
        if b.id != SHELF_ID {
            return false;
        }
        let inv = r.0.inverse();
        let o = inv * (eye - p.0);
        let v = inv * dir;
        ray_hits_box(o.to_array(), v.to_array(), (b.size * 0.5).to_array(), reach)
    })
}

/// The client's click on the mask in hand (not at the shelf): the use puts it on or takes it off.
fn mask_input(time: Res<Time>, mut controls: ResMut<Controls>, mut items: ResMut<Items>, cfg: Res<RepairConfig>, active: Res<ActiveDisplay>, players: Query<&Player>, machines: Query<(&DevBox, &Position, &Rotation)>) {
    let Ok(pl) = players.single() else { return };
    let slot = items.client.view.selected;
    let mask = items.client.view.slot(slot).and_then(|s| items.defs.of(s)).and_then(|d| d.use_effect.as_deref()).is_some_and(is_gear_use);
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let at_shelf = shelf_aimed(eye, pl.world_look(Frame::IDENTITY), &machines, cfg.0.reach_m);
    let usable = !controls.free_cursor && active.using().is_none() && !pl.seated && !pl.fly;
    if mask && usable && !at_shelf && controls.click {
        controls.click = false;
        let _ = items.request(time.elapsed_secs_f64(), Request::Use { slot, target: LOCAL });
    }
}

/// The shelf hold: while the player holds the button at the shelf with the mask in hand, the mask
/// refills (host, `mask_refill`), the others see REFILLING, and the ring shows the tank.
fn mask_hold(mut controls: ResMut<Controls>, mut items: ResMut<Items>, mut hold: ResMut<MaskHold>, cfg: Res<RepairConfig>, active: Res<ActiveDisplay>, players: Query<&Player>, machines: Query<(&DevBox, &Position, &Rotation)>) {
    let on_shelf = players.single().ok().filter(|pl| pl.ship.is_none() && !pl.seated && !pl.fly).filter(|_| !controls.free_cursor && active.using().is_none()).is_some_and(|pl| shelf_aimed(pl.w.pos + pl.up * EYE_HEIGHT, pl.world_look(Frame::IDENTITY), &machines, cfg.0.reach_m));
    let slot = items.client.view.selected;
    let held = items.client.view.slot(slot).and_then(|s| items.defs.of(s).map(|d| (d.use_effect.as_deref().is_some_and(is_gear_use), s.item.condition)));
    let running = on_shelf && controls.primary_held && matches!(held, Some((true, Some(_))));
    hold.tank = if running { held.and_then(|(_, c)| c).map(|c| c / 100.0) } else { None };
    if hold.sent != running {
        hold.sent = running;
        items.ask_refill(running);
    }
    if running {
        // The shelf's hold holds the click: the mask is not put on while it refills.
        controls.click = false;
    }
}

/// Host: the uses put a mask on, or take it off (a second use of the same slot).
fn mask_toggle(mut items: ResMut<Items>, mut worn: ResMut<MaskWorn>) {
    if items.role == Role::Client {
        return;
    }
    for (who, slot) in std::mem::take(&mut items.mask_uses) {
        if !holds_mask(&items, who, slot) {
            continue;
        }
        if worn.0.get(&who) == Some(&slot) {
            worn.0.remove(&who);
        } else {
            worn.0.insert(who, slot);
        }
    }
}

/// Host: a worn mask breathes from its tank (`Mask::breathe`), the tank drains, and the body's air
/// for the health step is the mask's. A mask that left its slot is taken off. The dead breathe
/// nothing, so their tank stays.
fn mask_breath(time: Res<Time>, mut items: ResMut<Items>, mut worn: ResMut<MaskWorn>, mut air: ResMut<MaskAir>, mut suits: ResMut<SuitWorn>, st: Res<HealthState>) {
    if items.role == Role::Client {
        return;
    }
    let dt = time.delta_secs_f64();
    let now = time.elapsed_secs_f64();
    air.0.clear();
    suits.0.clear();
    let held: Vec<(PlayerId, usize)> = worn.0.iter().map(|(p, s)| (*p, *s)).collect();
    for (who, slot) in held {
        if !holds_mask(&items, who, slot) {
            worn.0.remove(&who);
            continue;
        }
        // A suit protects whatever its tank holds (`vacuum.rs`): the tank only gives the air.
        if holds_suit(&items, who, slot) {
            suits.0.insert(who);
        }
        if st.bodies.get(&who).is_some_and(|b| b.health.state() == Life::Dead) {
            continue;
        }
        let before = items.host.condition(who, slot).unwrap_or(0.0) / 100.0;
        let mut m = Mask { worn: true, tank: before };
        // An empty tank breathes the room (Barotrauma), so the body gets no mask entry.
        if let Some(breathed) = m.breathe(dt) {
            air.0.insert(who, breathed);
        }
        let drop = (before - m.tank) * 100.0;
        if drop > 0.0 {
            items.wear(now, who, slot, drop);
        }
    }
}

/// Host: the held mask refills while the generator makes air (`Mask::refill`), and the holds
/// on the shelf are the ones the refill reads (`Items::refilling`).
fn mask_refill(time: Res<Time>, mut items: ResMut<Items>, sim: Res<ReactorSim>, cfg: Res<ReactorConfig>) {
    if items.role == Role::Client {
        return;
    }
    if sim.room.oxygen_flow(cfg.0.life_support.oxygen_generator_rate) <= 0.0 {
        return;
    }
    let dt = time.delta_secs_f64();
    let now = time.elapsed_secs_f64();
    for who in items.refilling.clone() {
        let slot = items.host.inventory(who).selected;
        if !holds_mask(&items, who, slot) {
            continue;
        }
        let before = items.host.condition(who, slot).unwrap_or(0.0) / 100.0;
        let mut m = Mask { worn: false, tank: before };
        m.refill(dt);
        let gain = (m.tank - before) * 100.0;
        if gain > 0.0 {
            items.wear(now, who, slot, -gain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mask_and_the_suit_are_the_breathing_gear_and_nothing_else_is() {
        assert!(is_gear_item("o2_mask") && is_gear_item("suit"));
        assert!(!is_gear_item("extinguisher"));
        assert!(is_gear_use("wear_mask") && is_gear_use("wear_suit"));
        assert!(!is_gear_use("extinguish"));
    }
}
