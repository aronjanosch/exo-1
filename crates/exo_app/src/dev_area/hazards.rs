//! The dev area's hazards (#295): three greybox floor pads along the north wall, in the storage
//! strip. A hot plate burns whoever stands on it, a glowing barrel gives radiation, and the
//! no-air booth takes the air away. `health` reads the zones; the boxes are the look.
//! The rates are in `content/gameplay/health/health.json`. TODO(initiator): the booth's shape (a
//! flat pad for now, not yet a walled booth).
use super::*;

pub const HOT_PLATE: Zone = Zone { name: "hot plate", centre: [-12.0, 16.0], half: [1.5, 1.5] };
pub const GLOW_BARREL: Zone = Zone { name: "glowing barrel", centre: [0.0, 16.0], half: [1.5, 1.5] };
pub const NO_AIR: Zone = Zone { name: "no air", centre: [12.0, 16.0], half: [1.5, 1.5] };

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
}
