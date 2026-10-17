//! Window only: the rooms' lamps follow the grid's lights factor (`reactor_core::LightState`:
//! Barotrauma's flicker and fade), the red emergency lamps come on when the lights fail, and the
//! alarm lamps pulse with the ship's alarm.
use crate::dev_area::*;
use crate::dev_area::layout::*;
use crate::dev_area::hull::alarms::{alarm_pulse, ShipAlarm};
use crate::items::Items;
use crate::origin::WorldPos;

#[derive(Component)]
pub(super) struct RoomLight {
    alarm: bool,
}

/// Ours: a red lamp that is lit only in an emergency (the lights factor below 0.1).
#[derive(Component)]
pub(super) struct EmergencyLamp;

pub(super) fn spawn_lights(mut commands: Commands, planet: Res<PlanetRes>, area: Res<DevArea>, mut done: Local<Option<warp_core::PlanetId>>, mut made: Local<Vec<Entity>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(_) = area.frame else { return };
    *done = Some(planet.id);
    for e in made.drain(..) {
        commands.entity(e).despawn();
    }
    let at = |p: [f64; 2], h: f64| area.ground(&planet, p, h).map(|(w, _)| w).unwrap_or(DVec3::ZERO);
    let warm = Color::srgb(1.0, 0.9, 0.75);
    let lamps = [([-16.5, -15.5], 2.9, false), ([-11.5, -15.5], 2.9, false), ([-15.0, -9.2], 2.9, false), ([-5.0, -9.2], 2.9, false), ([2.0, -9.2], 2.9, false), ([-14.0, -15.0], 2.6, true), ([-7.7, -9.2], 2.6, true), ([EQUIPMENT.centre[0] - 1.5, EQUIPMENT.centre[1]], 2.9, false), ([EQUIPMENT.centre[0] + 1.5, EQUIPMENT.centre[1]], 2.9, false), (AIRLOCK.centre, 2.9, false), (BRIDGE.centre, 2.9, false), (TURRET_ROOM.centre, 2.9, false), (LAB_ROOM.centre, 2.9, false), (MEDBAY_ROOM.centre, 2.9, false)];
    for (p, h, alarm) in lamps {
        let colour = if alarm { Color::srgb(1.0, 0.1, 0.05) } else { warm };
        let intensity = if alarm { 0.0 } else { 60_000.0 };
        made.push(
            commands
                .spawn((PointLight { color: colour, intensity, range: 12.0, shadow_maps_enabled: false, ..default() }, Transform::default(), WorldPos(at(p, h)), RoomLight { alarm }))
                .id(),
        );
    }
    // Ours: one red emergency lamp in the room, the corridor and the bridge, dark until the lights fail.
    for c in [ROOM.centre, CORRIDOR.centre, BRIDGE.centre] {
        made.push(
            commands
                .spawn((PointLight { color: Color::srgb(1.0, 0.1, 0.05), intensity: 0.0, range: 8.0, shadow_maps_enabled: false, ..default() }, Transform::default(), WorldPos(at(c, 2.6)), EmergencyLamp))
                .id(),
        );
    }
}

/// Peak intensity of the room's warm lamps, at full brightness.
const ROOM_LAMP_INTENSITY: f32 = 60_000.0;
/// Ours: the emergency lamps' intensity while on. `TODO(initiator)`.
const EMERGENCY_INTENSITY: f32 = 8_000.0;

/// The grid's lights factor from the snapshot (0 with no snapshot yet: the emergency lamps).
fn lights_voltage(items: &Items) -> f64 {
    items.reactor.map_or(0.0, |r| r.lights_factor)
}

pub(super) fn room_lights(
    time: Res<Time>,
    items: Res<Items>,
    sounds: Option<Res<crate::audio_slice::SliceSounds>>,
    mut commands: Commands,
    mut frame: Local<u64>,
    mut state: Local<reactor_core::LightState>,
    mut lights: Query<(&RoomLight, &mut PointLight)>,
) {
    let voltage = lights_voltage(&items);
    *frame = frame.wrapping_add(1);
    // Each client rolls its own flicker; the voltage is the synced part.
    let roll = crate::effects::noise(*frame);
    let brightness = state.step(time.delta_secs_f64(), voltage, roll);
    if state.flickered() && voltage > reactor_core::lights::ZAP_ABOVE {
        if let Some(s) = sounds {
            crate::audio_slice::one_shot(&mut commands, &s.zap, crate::audio_slice::ZAP_VOL);
        }
    }
    for (l, mut light) in &mut lights {
        if !l.alarm {
            light.intensity = ROOM_LAMP_INTENSITY * brightness as f32;
        }
    }
}

pub(super) fn emergency_lamps(items: Res<Items>, mut lamps: Query<&mut PointLight, With<EmergencyLamp>>) {
    let on = reactor_core::LightState::emergency(lights_voltage(&items));
    for mut light in &mut lamps {
        light.intensity = if on { EMERGENCY_INTENSITY } else { 0.0 };
    }
}

pub(super) fn alarm_lights(time: Res<Time>, alarm: Res<ShipAlarm>, mut lights: Query<(&RoomLight, &mut PointLight)>) {
    let k = alarm_pulse(alarm.level, time.elapsed_secs_f64()) as f32;
    for (l, mut light) in &mut lights {
        if l.alarm {
            light.intensity = 40_000.0 * k;
        }
    }
}
