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
    /// A warm lamp in the engine room: it dims while the implosion countdown runs (#325), so the red pulse reads.
    engine_room: bool,
    /// The junction box that feeds this lamp (#328): broken or switched off, the lamp is a dim red glow.
    box_i: usize,
    /// The light this lamp keeps while its box is out, in lumens: the first lamps of each room keep the old dim glow, the lamps
    /// added for a lit room (#328) none, so a dead room looks as before.
    glow: f32,
}

/// Ours: a red lamp that is lit only in an emergency (the lights factor below 0.1). `engine_room`: the engine room's own;
/// it stays dark while the room's lamps are off by hand (#313).
#[derive(Component)]
pub(super) struct EmergencyLamp {
    engine_room: bool,
}

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
    let warm = WARM;
    // The first lamps are the original ones (they keep the box-out glow); `EXTRA_LAMPS` fill the rooms' corners for a lit room.
    let lamps: Vec<([f64; 2], f64, bool, bool)> = [([-16.5, -15.5], 2.9, false), ([-11.5, -15.5], 2.9, false), ([-15.0, -9.2], 2.9, false), ([-5.0, -9.2], 2.9, false), ([2.0, -9.2], 2.9, false), ([-14.0, -15.0], 2.6, true), ([-7.7, -9.2], 2.6, true), ([EQUIPMENT.centre[0] - 1.5, EQUIPMENT.centre[1]], 2.9, false), ([EQUIPMENT.centre[0] + 1.5, EQUIPMENT.centre[1]], 2.9, false), (AIRLOCK.centre, 2.9, false), (BRIDGE.centre, 2.9, false), (TURRET_ROOM.centre, 2.9, false), (LAB_ROOM.centre, 2.9, false), (MEDBAY_ROOM.centre, 2.9, false)]
        .into_iter()
        .map(|(p, h, alarm)| (p, h, alarm, true))
        .chain(EXTRA_LAMPS.iter().map(|&p| (p, 2.9, false, false)))
        .collect();
    for (p, h, alarm, glows) in lamps {
        let colour = if alarm { Color::srgb(1.0, 0.1, 0.05) } else { warm };
        let intensity = if alarm { 0.0 } else { ROOM_LAMP_INTENSITY };
        made.push(
            commands
                .spawn((PointLight { color: colour, intensity, range: LAMP_RANGE, shadow_maps_enabled: false, ..default() }, Transform::default(), WorldPos(at(p, h)), RoomLight { glow: if glows { BOX_OUT_GLOW } else { 0.0 }, alarm, engine_room: (p[0] - ROOM.centre[0]).abs() <= ROOM.half[0] && (p[1] - ROOM.centre[1]).abs() <= ROOM.half[1], box_i: box_of_point(p) }))
                .id(),
        );
    }
    // Ours: one red emergency lamp in the room, the corridor and the bridge, dark until the lights fail.
    for c in [ROOM.centre, CORRIDOR.centre, BRIDGE.centre] {
        made.push(
            commands
                .spawn((PointLight { color: Color::srgb(1.0, 0.1, 0.05), intensity: 0.0, range: 8.0, shadow_maps_enabled: false, ..default() }, Transform::default(), WorldPos(at(c, 2.6)), EmergencyLamp { engine_room: c == ROOM.centre }))
                .id(),
        );
    }
}

/// The room lamps' colour.
const WARM: Color = Color::srgb(1.0, 0.9, 0.75);
/// Peak intensity of the room's warm lamps, at full brightness (lumens). The camera's exposure is the Bevy default (EV 9.7: a wall
/// 3 m from a 60 000 lm lamp came out at about a twentieth of white), so a lit room needs several times that. TODO(initiator): look.
const ROOM_LAMP_INTENSITY: f32 = 300_000.0;
/// How far a lamp reaches (m). Lamps have no shadows and shine through walls, so this also bounds the leak into the next room.
const LAMP_RANGE: f32 = 9.0;
/// Further ceiling lamps (area metres) so that no room's wall is more than about 4 m from one (#328).
const EXTRA_LAMPS: [[f64; 2]; 14] = [
    [-17.5, -18.0], [-11.0, -18.0], [-17.5, -12.5], [-11.0, -12.5], // engine room corners (two lamps stand in its middle)
    [-10.0, -9.2], [-1.5, -9.2], // corridor between its three
    [-0.9, -13.5], [2.1, -13.5], // storage room
    [-5.7, -13.85], // medbay
    [-17.5, -4.4], [-12.5, -4.4], // bridge
    [-3.0, -4.4], [2.2, -4.4], // lab
    [-7.7, -4.4], // turret room
];
/// Ours: a lamp whose junction box is out keeps this much light, reddish, so the room stays playable (#328). It was 10 % of
/// the old 60 000 lm lamps and stays that in absolute terms. TODO(initiator): the value and the colour.
const BOX_OUT_GLOW: f32 = 6_000.0;
/// The ambient level (share of the interior ambient) a room with its box out counts as.
const BOX_OUT_SHARE: f32 = 0.1;
const BOX_OUT_COLOUR: Color = Color::srgb(1.0, 0.04, 0.02);
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
    mut interior: ResMut<crate::interior::Interior>,
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
    // TODO(initiator): how far the room's lamps dim while the countdown runs (start value 15 %).
    let countdown = items.reactor.as_ref().is_some_and(crate::dev_area::engineering::countdown::countdown_on);
    let by_hand = engine_lamps_off_by_hand(items.reactor.as_ref());
    for (l, mut light) in &mut lights {
        if !l.alarm {
            // The box-out glow wins; the hand switch below only counts while the box passes.
            if !crate::dev_area::engineering::junction_boxes::box_passes(items.reactor.as_ref(), l.box_i) {
                light.intensity = l.glow;
                light.color = BOX_OUT_COLOUR;
                interior.levels[l.box_i] = BOX_OUT_SHARE;
                continue;
            }
            if l.engine_room && by_hand {
                light.intensity = 0.0;
                interior.levels[l.box_i] = 0.0;
                continue;
            }
            light.color = WARM;
            let dim = if countdown && l.engine_room { 0.15 } else { 1.0 };
            light.intensity = ROOM_LAMP_INTENSITY * brightness as f32 * dim;
            interior.levels[l.box_i] = brightness as f32;
        }
    }
}

pub(super) fn emergency_lamps(items: Res<Items>, mut lamps: Query<(&mut PointLight, &EmergencyLamp)>) {
    let on = reactor_core::LightState::emergency(lights_voltage(&items));
    let by_hand = engine_lamps_off_by_hand(items.reactor.as_ref());
    for (mut light, l) in &mut lamps {
        light.intensity = if on && !(l.engine_room && by_hand) { EMERGENCY_INTENSITY } else { 0.0 };
    }
}

/// The engine room's lamps are switched off at the room panel (#313) and its box passes power: plain dark, no emergency glow.
/// With JB-1 out the room is unpowered anyway (the box-out glow shows) and the panel reads NO POWER.
pub fn engine_lamps_off_by_hand(n: Option<&item_core::wire::ReactorNews>) -> bool {
    n.is_some_and(|n| n.engine_lamps_off && crate::dev_area::engineering::junction_boxes::box_passes(Some(n), 0))
}

pub(super) fn alarm_lights(time: Res<Time>, alarm: Res<ShipAlarm>, mut lights: Query<(&RoomLight, &mut PointLight)>) {
    let k = alarm_pulse(alarm.level, time.elapsed_secs_f64()) as f32;
    for (l, mut light) in &mut lights {
        if l.alarm {
            light.intensity = 40_000.0 * k;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use item_core::wire::ReactorNews;

    #[test]
    fn off_by_hand_is_dark_only_while_jb1_passes_and_never_without_news() {
        let mut n = ReactorNews { engine_lamps_off: true, breakers: [true; item_core::wire::BREAKER_GROUPS], ..Default::default() };
        n.junctions[0].working = true;
        assert!(engine_lamps_off_by_hand(Some(&n)));
        n.engine_lamps_off = false;
        assert!(!engine_lamps_off_by_hand(Some(&n)));
        n.engine_lamps_off = true;
        n.junctions[0].working = false;
        assert!(!engine_lamps_off_by_hand(Some(&n)), "JB-1 out: the box-out glow, not plain dark");
        assert!(!engine_lamps_off_by_hand(None));
    }
}
