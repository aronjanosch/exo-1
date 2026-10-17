//! The hull's sections (r37, by section in r40): a dark patch on each wall section of the engine
//! room, inside and out, shown while that section leaks. Each patch is a dev box so the wrench aims
//! at it (`crate::repair`).
use crate::dev_area::*;
use crate::dev_area::layout::*;
use crate::items::Items;
use item_core::wire::HULL_SECTIONS;
use reactor_core::hull::{openness, SECTION_CENTRES, WALLS_MAX, WALLS_MIN};

/// The hull breach (r37, by wall section in r40): a dark patch on each section, where the implosion
/// blast or a hit opens the wall. Each patch is a dev box so the wrench can aim at it. Section 0 is
/// the south-west half of the south wall, where r37's breach was.
pub const BREACH_IDS: [&str; HULL_SECTIONS] = ["engine_breach_0", "engine_breach_1", "engine_breach_2", "engine_breach_3", "engine_breach_4", "engine_breach_5", "engine_breach_6", "engine_breach_7"];
/// The same patch on each section's outer face (r43), so the wrench aims at it from outside. Same
/// section index as `BREACH_IDS`.
pub const BREACH_OUT_IDS: [&str; HULL_SECTIONS] = ["engine_breach_out_0", "engine_breach_out_1", "engine_breach_out_2", "engine_breach_out_3", "engine_breach_out_4", "engine_breach_out_5", "engine_breach_out_6", "engine_breach_out_7"];
/// A patch's size (m): along the south and north walls, and along the east and west walls.
const BREACH_SIZE_X: [f64; 3] = [1.0, 1.2, 0.1];
const BREACH_SIZE_Z: [f64; 3] = [0.1, 1.2, 1.0];

const BREACH_COLOUR: [f32; 3] = [0.05, 0.05, 0.06];

/// The inside patch of section `i`: on the wall's inner face.
fn inside_part(i: usize) -> Part {
    let size = if matches!(i, 0 | 1 | 4 | 5) { BREACH_SIZE_X } else { BREACH_SIZE_Z };
    Part { id: BREACH_IDS[i], at: SECTION_CENTRES[i], size, lift: 0.0, colour: BREACH_COLOUR }
}

/// The outer face of section `i` (r43): its inside centre pushed out by the wall thickness, along the
/// wall's outward normal. Sections 0 and 1 are on the south wall, 2 and 3 on the east, 4 and 5 on
/// the north, 6 and 7 on the west (`SECTION_CENTRES`).
fn outside_at(i: usize) -> [f64; 2] {
    let [x, y] = SECTION_CENTRES[i];
    if y == WALLS_MIN[1] {
        [x, y - WALL]
    } else if x == WALLS_MAX[0] {
        [x + WALL, y]
    } else if y == WALLS_MAX[1] {
        [x, y + WALL]
    } else {
        [x - WALL, y]
    }
}

/// The outside patch of section `i` (r43): the inside patch's size, on the outer face.
fn outside_part(i: usize) -> Part {
    let size = if matches!(i, 0 | 1 | 4 | 5) { BREACH_SIZE_X } else { BREACH_SIZE_Z };
    Part { id: BREACH_OUT_IDS[i], at: outside_at(i), size, lift: 0.0, colour: BREACH_COLOUR }
}

/// A section's dark patch on the wall (r37, by section in r40): shown while that section leaks,
/// hidden while it holds. The wrench aims at it either way (its collider is there).
#[derive(Component)]
pub(super) struct BreachDecal(usize);

/// Once per planet: both patches of every section, each a dev box with its decal marker.
pub(super) fn spawn_patches(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    for i in 0..HULL_SECTIONS {
        for p in [inside_part(i), outside_part(i)] {
            let e = spawn_part(&mut commands, &frame, &planet, &p);
            commands.entity(e).insert(BreachDecal(i));
            area.entities.push(e);
        }
    }
}

pub(super) fn breach_decal(items: Res<Items>, mut decals: Query<(&BreachDecal, &mut Visibility)>) {
    for (decal, mut v) in &mut decals {
        let leaking = items.reactor.is_some_and(|r| openness(r.breaches[decal.0]) > 0.0);
        let want = if leaking { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}
