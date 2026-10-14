//! Fuel rods by hand (#300, lane rods). A rod is an item you carry (`fuel_rod`, `fuel_rod_hot`,
//! `fuel_rod_spicy`, `content/items/fuel_rod*.json`); its item condition is the rod's life (100 new,
//! 0 spent), kept when it is carried, loaded or pulled. Barotrauma style: the reactor is an item
//! container with fuel slots, a rod in a slot is its available fuel.
//!
//! - A rod rack in the corridor, next to the coolant rack, holds two standard rods and one hot rod
//!   (loose items on the rack, as the coolant canisters are). Taken with F.
//! - Load: a rod in the selected slot, the look on the reactor within reach, LMB held `HOLD_S`
//!   seconds: `Request::Use` goes to the host, which puts the rod into a free slot (`insert_rod`)
//!   and the item is gone. Refused with a notice when all slots are full (the client checks its
//!   snapshot first; the host gives the rod back if it was full after all).
//! - Pull (the emergency brake, `reactor_core` `pull_rod`): empty hands, the look on the reactor,
//!   LMB held `HOLD_S` seconds: `ReactorAsk::PullRod` goes to the host, which pulls the rod with the
//!   most life. The rod comes back as an item with its kind and life. A core above its optimal band
//!   burns the hand once (the reactor's miss burn, `device_core::minigame::hurt_of`, as repair uses it).
//!
//! Values: TODO(initiator) (the rack's stock, the burn rule, hold times).
use super::*;
use crate::dev_area::engine_room::CORRIDOR;
use crate::items::Items;
use bevy::math::DVec3;
use item_core::{FrameRef, Placement};
use reactor_core::FuelRod;

/// The `use` effect of a fuel rod: loading it into the reactor.
pub const LOAD: &str = "load_rod";
/// The rack in the corridor, relative to the corridor's centre (m), its size (m). Beside the
/// coolant rack (`coolant::RACK_AT` is at x = 0.6); this one sits at x = -0.6, the walkway between.
pub const RACK_AT: [f64; 2] = [-0.6, -1.5];
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

pub fn plugin(app: &mut App) {
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
