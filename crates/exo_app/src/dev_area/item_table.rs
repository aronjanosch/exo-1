//! The item table (C1): a table in the engineering zone with a fire extinguisher, a wrench, a screwdriver and a
//! stack of bandages and two deglows on it, and one wrench lying in the parked ship's cabin, to try collecting,
//! holding, dropping and throwing. The host puts them into the world once the area and the ship
//! are there.
use super::*;
use crate::items::{Items, LOCAL};
use crate::ship::Ship;
use item_core::{FrameRef, Placement};

/// Where the table stands in the engineering zone (m from the zone's centre), facing the walkway.
pub const AT: [f64; 2] = [-3.0, -3.5];
/// Table size (m): width, height, depth.
pub const SIZE: [f64; 3] = [1.8, 0.9, 0.8];
/// What lies on it: item, count, offset along the table (m, east).
pub const ON_TABLE: [(&str, u32, f64); 5] = [("extinguisher", 1, -0.6), ("screwdriver", 1, -0.3), ("wrench", 1, 0.0), ("bandage", 4, 0.55), ("deglow", 2, 0.8)];
/// The wrench in the cabin: ship-local floor spot (x, z), left of the aisle.
pub const IN_CABIN: (&str, f64, f64) = ("wrench", -1.3, 0.8);

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(PlaceholderBox {
        id: "item_table".into(),
        zone: ENGINEERING,
        local_pos: AT,
        size: SIZE,
        turn_deg: 0.0,
        colour: [0.45, 0.36, 0.26],
        panel: None,
    });
    app.add_systems(FixedUpdate, put_items.in_set(crate::phases::Fx::Effects));
}

/// Once: the table's items (planet frame) and the cabin's wrench (ship frame).
fn put_items(time: Res<Time>, planet: Res<PlanetRes>, area: Res<DevArea>, mut items: ResMut<Items>, ships: Query<(), With<Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = true;
    let now = time.elapsed_secs_f64();
    for (id, n, along) in ON_TABLE {
        let Some(d) = items.defs.get(id).cloned() else { continue };
        let p = ENGINEERING.point([AT[0] + along, AT[1]]);
        let (pos, up) = frame.ground(&planet, p[0], p[1], SIZE[1] - SINK * 0.5 + d.shape.extents[1] * 0.5 + 0.02);
        let fwd = frame.rotation(up, 0.0) * DVec3::NEG_Z;
        let at = Placement { frame: FrameRef::Planet(planet.id.0 as u32), pos: pos.to_array(), vel: [0.0; 3], forward: fwd.to_array() };
        items.spawn(now, d.stack_of(n), at);
    }
    let (id, x, z) = IN_CABIN;
    if let Some(d) = items.defs.get(id).cloned() {
        let pos = DVec3::new(x, crate::cargo::FLOOR_Y + d.shape.extents[1] * 0.5 + 0.01, z);
        let at = Placement { frame: FrameRef::Ship(LOCAL), pos: pos.to_array(), vel: [0.0; 3], forward: [0.0, 0.0, -1.0] };
        items.spawn(now, d.stack_of(1), at);
    }
}
