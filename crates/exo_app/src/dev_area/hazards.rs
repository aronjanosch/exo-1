//! The rates are in `content/gameplay/health/health.json`. TODO(initiator): the booth's shape (a
//! flat pad for now, not yet a walled booth).
//! `SPICY_RAD_PER_S` radiation per second. Inside the reactor it is shielded. The values are
//! consts, the item format has no custom fields: TODO(initiator).
use super::*;
use crate::dev_area::engine_room::{REACTOR_AT, REACTOR_SIZE};
use crate::env::PlanetRes;
use crate::items::Items;
use bevy::math::DVec3;
use item_core::{FrameRef, Placement, PlayerId};
use std::collections::BTreeSet;

pub const HOT_PLATE: Zone = Zone { name: "hot plate", centre: [-12.0, 16.0], half: [1.5, 1.5] };
pub const GLOW_BARREL: Zone = Zone { name: "glowing barrel", centre: [0.0, 16.0], half: [1.5, 1.5] };
pub const NO_AIR: Zone = Zone { name: "no air", centre: [12.0, 16.0], half: [1.5, 1.5] };

pub const SPICY_ITEM: &str = "fuel_rod_spicy";
/// The spicy rod's reach (m) and its radiation (per second) on each body in reach. TODO(initiator).
pub const SPICY_RADIUS_M: f64 = 1.25;
pub const SPICY_RAD_PER_S: f64 = 1.0;
/// The spicy rod on the dev area's pad by the barrel: the spot, relative to the barrel's centre,
/// on the pad's east edge, so a walker one step further east stands outside the barrel's glow.
pub const SPICY_AT: [f64; 2] = [1.45, 0.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RodSpot {
    pub at: DVec3,
    pub shielded: bool,
}

pub fn shielded_at(p: [f64; 2]) -> bool {
    Zone { name: "reactor", centre: REACTOR_AT, half: [REACTOR_SIZE[0] * 0.5, REACTOR_SIZE[2] * 0.5] }.contains(p)
}

/// The spicy rod's light, when it lies in the world or is in hand: green-cyan, dim, 3 m. The
/// colour is the fulgurium's (60, 200, 150). TODO(initiator): the intensity and the look.
pub fn spicy_light() -> PointLight {
    PointLight { color: Color::srgb(0.235, 0.784, 0.588), intensity: 6_000.0, range: 3.0, shadow_maps_enabled: false, ..default() }
}

pub fn spicy_irradiated(rods: &[RodSpot], bodies: &[(PlayerId, DVec3)], radius: f64) -> BTreeSet<PlayerId> {
    bodies.iter().filter(|(_, b)| rods.iter().any(|r| !r.shielded && r.at.distance(*b) <= radius)).map(|(id, _)| *id).collect()
}

pub fn plugin(app: &mut App) {
    let pads = [(&HOT_PLATE, "hazard_hot_plate", [0.95, 0.45, 0.1]), (&GLOW_BARREL, "hazard_glow_barrel", [0.3, 0.9, 0.3]), (&NO_AIR, "hazard_no_air", [0.2, 0.4, 0.95])];
    for (z, id, colour) in pads {
        app.world_mut().resource_mut::<DevAreaDevices>().add(PlaceholderBox {
            id: id.into(),
            zone: STORAGE,
            local_pos: [z.centre[0] - STORAGE.centre[0], z.centre[1] - STORAGE.centre[1]],
            size: [z.half[0] * 2.0, 0.05, z.half[1] * 2.0],
            turn_deg: 0.0,
            colour,
            panel: None,
        });
    }
    app.add_systems(FixedUpdate, put_spicy_rod.in_set(crate::phases::Fx::Effects));
}

fn put_spicy_rod(time: Res<Time>, planet: Res<PlanetRes>, area: Res<DevArea>, mut items: ResMut<Items>, ships: Query<(), With<crate::ship::Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    let Some(frame) = area.frame else { return };
    let Some(d) = items.defs.get(SPICY_ITEM).cloned() else { return };
    *done = true;
    let p = GLOW_BARREL.point(SPICY_AT);
    let (pos, up) = frame.ground(&planet, p[0], p[1], 0.05 + d.shape.extents[1] * 0.5 + 0.02);
    let fwd = frame.rotation(up, 0.0) * DVec3::NEG_Z;
    let at = Placement { frame: FrameRef::Planet(planet.id.0 as u32), pos: pos.to_array(), vel: [0.0; 3], forward: fwd.to_array() };
    items.spawn(time.elapsed_secs_f64(), d.stack_of(1), at);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot(x: f64, shielded: bool) -> RodSpot {
        RodSpot { at: DVec3::new(x, 0.0, 0.0), shielded }
    }

    #[test]
    fn a_body_in_reach_gets_the_radiation_and_one_out_of_reach_does_not() {
        let bodies = [(1, DVec3::new(1.0, 0.0, 0.0)), (2, DVec3::new(5.0, 0.0, 0.0))];
        let got = spicy_irradiated(&[spot(0.0, false)], &bodies, SPICY_RADIUS_M);
        assert_eq!(got, BTreeSet::from([1]), "1 m is in reach of 1.25 m, 5 m is not");
    }

    #[test]
    fn the_edge_of_reach_counts_and_just_beyond_does_not() {
        let bodies = [(1, DVec3::new(1.25, 0.0, 0.0)), (2, DVec3::new(1.26, 0.0, 0.0))];
        assert_eq!(spicy_irradiated(&[spot(0.0, false)], &bodies, 1.25), BTreeSet::from([1]));
    }

    #[test]
    fn a_rod_in_the_reactor_shields_everyone() {
        let bodies = [(1, DVec3::new(0.5, 0.0, 0.0))];
        assert!(spicy_irradiated(&[spot(0.0, true)], &bodies, SPICY_RADIUS_M).is_empty());
    }

    #[test]
    fn two_rods_irradiate_each_body_once_and_the_shielded_one_does_not_count() {
        let bodies = [(1, DVec3::ZERO), (2, DVec3::new(10.0, 0.0, 0.0)), (3, DVec3::new(-0.5, 0.0, 0.0))];
        let rods = [spot(0.0, false), spot(10.0, true), spot(-0.2, false)];
        assert_eq!(spicy_irradiated(&rods, &bodies, SPICY_RADIUS_M), BTreeSet::from([1, 3]), "the shielded rod at 10 m does not reach body 2");
        assert!(spicy_irradiated(&[], &bodies, SPICY_RADIUS_M).is_empty(), "no rods, no radiation");
    }
}
