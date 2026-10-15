//! The ship's alarm (`ShipAlarm`, the reactor's and the fire's level) and the rooms' red alarm lamps
//! (window only).
use crate::dev_area::*;
use crate::dev_area::layout::*;
use crate::items::Items;
use crate::origin::WorldPos;
use item_core::wire::{Alarm, RoomAlarmKind};

/// The alarm light's strength, 0..1, at a time: off at None, a slow pulse at Warn, a faster one at
/// Danger (`TODO(initiator)`: the periods are start values).
pub fn alarm_pulse(level: Alarm, t: f64) -> f64 {
    let period = match level {
        Alarm::None => return 0.0,
        Alarm::Warn => 2.0,
        Alarm::Danger => 0.8,
    };
    0.5 + 0.5 * (std::f64::consts::TAU * t / period).sin()
}

/// The seconds between two alarm tones (`TODO(initiator)`: start values); None without a tone.
pub fn tone_every(level: Alarm) -> Option<f64> {
    match level {
        Alarm::None => None,
        Alarm::Warn => Some(2.0),
        Alarm::Danger => Some(1.0),
    }
}

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct ShipAlarm {
    pub level: Alarm,
}

/// The room alarm's red lamp (r40, #165): one in the engine room, one in the corridor and one in the bridge (r44), at
/// `ALARM_LAMP_HEIGHT`. Copies the reactor's critical lamp (`effects::critical_lamp_sync`): a light
/// and a lens that show while the room's alarm is on. The corridor is open to the sky, so its lamp
/// hangs on a mast at the same height (TODO(initiator)).
#[derive(Component)]
pub(super) struct RoomAlarmLamp {
    room: usize,
}

#[derive(Component)]
pub(super) struct RoomAlarmLens {
    room: usize,
}

/// The lamp's height (m) over the floor: the ceiling less a lamp's drop.
const ALARM_LAMP_HEIGHT: f64 = ROOM_HEIGHT - 0.3;
/// The lamp's peak intensity (cd-like, as the room's other lamps) and its blink. Fire and breach
/// blink at Barotrauma's alarm buzzer rate (`signalitems.xml` alarmbuzzer, blinkfrequency 2.6 Hz);
/// thin air blinks slower. TODO(initiator): the intensity and the slow rate.
const ALARM_LAMP_INTENSITY: f32 = 30_000.0;
const THIN_AIR_BLINK_S: f64 = 1.0;

/// Whether the room's alarm lamp is lit now: never at Clear, blinking while the alarm is on (SILENCE
/// does not stop it).
pub fn alarm_lamp_lit(kind: RoomAlarmKind, t: f64) -> bool {
    let period = match kind {
        RoomAlarmKind::Clear => return false,
        RoomAlarmKind::Fire | RoomAlarmKind::Breach => 1.0 / 2.6,
        RoomAlarmKind::LowO2 => THIN_AIR_BLINK_S,
    };
    t.rem_euclid(period) < period / 2.0
}

pub(super) fn spawn_alarm_lamps(mut commands: Commands, planet: Res<PlanetRes>, area: Res<DevArea>, mut done: Local<Option<warp_core::PlanetId>>, mut made: Local<Vec<Entity>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(_) = area.frame else { return };
    *done = Some(planet.id);
    for e in made.drain(..) {
        commands.entity(e).despawn();
    }
    let mesh = meshes.add(Sphere::new(0.08));
    let mat = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.1, 0.05), emissive: LinearRgba::rgb(8.0, 0.3, 0.1), unlit: true, ..default() });
    for (room, centre) in [ROOM.centre, CORRIDOR.centre, BRIDGE.centre].into_iter().enumerate() {
        let Some((at, _)) = area.ground(&planet, centre, ALARM_LAMP_HEIGHT) else { continue };
        made.push(commands.spawn((WorldPos(at), Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden, RoomAlarmLens { room })).id());
        made.push(commands.spawn((WorldPos(at), PointLight { color: Color::srgb(1.0, 0.1, 0.05), intensity: 0.0, range: 10.0, shadow_maps_enabled: false, ..default() }, Transform::default(), RoomAlarmLamp { room })).id());
    }
}

pub(super) fn alarm_lamps(time: Res<Time>, items: Res<Items>, mut lights: Query<(&RoomAlarmLamp, &mut PointLight)>, mut lens: Query<(&RoomAlarmLens, &mut Visibility)>) {
    let t = time.elapsed_secs_f64();
    let kind_of = |room: usize| items.reactor.map_or(RoomAlarmKind::Clear, |r| r.alarms[room].kind);
    for (l, mut light) in &mut lights {
        light.intensity = if alarm_lamp_lit(kind_of(l.room), t) { ALARM_LAMP_INTENSITY } else { 0.0 };
    }
    for (l, mut v) in &mut lens {
        let want = if alarm_lamp_lit(kind_of(l.room), t) { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}
