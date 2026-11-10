//! Window only: inside the mini ship the world's light stays out (#328 follow-up). The sky's ambient light and the
//! sun (its shadows are off, so it shines through the roofs) would fill a room whose lamps are out, so the
//! camera's ambient and the sun and night lights follow where the view stands: outside the world's values;
//! inside a room a low interior ambient times that room's lamp level. The lamps are point lights and stay
//! as they are, so a lit room seen through a doorway stays bright.
use crate::dev_area::layout::*;
use crate::dev_area::{DevArea, Zone};
use crate::env::PlanetRes;
use crate::origin::RenderOrigin;
use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;

/// Ambient brightness inside a room whose lamps are fully on. TODO(initiator): look and feel (the world's is about 400).
const INTERIOR_AMBIENT: f32 = 80.0;
/// Time constant of the easing (s): about 0.3 s to settle, so a door does not pop.
const EASE_SECS: f32 = 0.1;

/// Every room of the mini ship (the storage room counts as part of the corridor's box, see `box_of_point`).
const ROOMS: [Zone; 9] = [ROOM, CORRIDOR, BRIDGE, TURRET_ROOM, LAB_ROOM, MEDBAY_ROOM, STORAGE_ROOM, EQUIPMENT, AIRLOCK];

#[derive(Resource, Clone, Copy, Debug)]
pub struct Interior {
    /// 0 outside, 1 inside a room (eased).
    pub amount: f32,
    /// The lamp level of the room the view is in, 0..1 (eased).
    pub level: f32,
    /// Each junction box's lamp level now, written by `room_lights`.
    pub levels: [f32; 3],
}

impl Default for Interior {
    fn default() -> Self {
        Interior { amount: 0.0, level: 1.0, levels: [1.0; 3] }
    }
}

/// The area point is in a room (floor plan only; the height is checked by the caller).
pub fn room_at(p: [f64; 2]) -> bool {
    ROOMS.iter().any(|z| z.contains(p))
}

/// The ambient brightness the camera gets: the world's outside, the interior value times the lamp level inside.
pub fn blend_ambient(world: f32, i: &Interior) -> f32 {
    world * (1.0 - i.amount) + INTERIOR_AMBIENT * i.level * i.amount
}

pub fn window_plugin(app: &mut App) {
    app.init_resource::<Interior>();
    app.add_systems(Update, update_interior.after(crate::view::update_camera).in_set(crate::phases::Frame::Camera));
}

fn update_interior(time: Res<Time>, origin: Res<RenderOrigin>, planet: Res<PlanetRes>, area: Res<DevArea>, mut interior: ResMut<Interior>, mut ambient: ResMut<GlobalAmbientLight>) {
    let mut inside = false;
    let mut level = interior.level;
    if let Some(frame) = area.frame {
        let p = crate::dev_area::area_of(&frame, &planet, origin.view);
        if room_at(p) {
            let (ground, up) = frame.ground(&planet, p[0], p[1], 0.0);
            let h = (origin.view - ground).dot(up);
            if (-0.5..ROOM_HEIGHT).contains(&h) {
                inside = true;
                level = interior.levels[box_of_point(p)];
            }
        }
    }
    let k = 1.0 - (-time.delta_secs() / EASE_SECS).exp();
    let target = if inside { 1.0 } else { 0.0 };
    interior.amount += (target - interior.amount) * k;
    if inside {
        interior.level += (level - interior.level) * k;
    }
    ambient.brightness = blend_ambient(ambient.brightness, &interior);
}
