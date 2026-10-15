use super::*;
use crate::dev_area::reactor_screen;
use crate::displays::DisplayPanel;
use crate::env::PlanetRes;
use crate::items::{Items, NetOut, Role};
use crate::net_live::RemoteWalker;
use crate::origin::WorldPose;
use crate::walker::Player;
use crate::origin::WorldPos;
use crate::Layer;
use fire_core::{Deck, FireTuning, GapId, RoomId};
use reactor_core::Source;
use display_core::PanelId;
use item_core::wire::{Alarm, RoomAsk, RoomNews};

pub const ROOM: Zone = Zone { name: "engine room", centre: [-14.0, -15.0], half: [5.0, 4.0] };
pub const CORRIDOR: Zone = Zone { name: "corridor", centre: [-14.0, -7.7], half: [1.0, 3.0] };
/// The room's height (m); the room has a ceiling, the corridor is open to the sky.
pub const ROOM_HEIGHT: f64 = 3.2;
/// Thickness of the walls and the door (m).
const WALL: f64 = 0.3;
/// The door: its slab's centre when shut (area metres), its size, and how far it slides east
/// when open (m), and how long that takes (s).
pub const DOOR_AT: [f64; 2] = [-14.0, -11.15];
pub const DOOR_SIZE: [f64; 3] = [2.0, 3.2, WALL];
pub const DOOR_SLIDE: f64 = 2.0;
pub const DOOR_TIME: f64 = 1.0;
/// The gap's area when fully open (m²): the door's opening.
pub const DOOR_AREA: f64 = 2.0;
/// Where the walker looks at the door from (m): its reach from the slab's centre.
pub const DOOR_REACH: f64 = 2.5;

pub const REACTOR_AT: [f64; 2] = [-14.0, -17.8];
pub const REACTOR_SIZE: [f64; 3] = [2.0, 2.5, 2.0];
pub const COOLANT_PUMP_AT: [f64; 2] = [-10.5, -17.5];
pub const COOLANT_PUMP_SIZE: [f64; 3] = [1.0, 1.2, 1.0];
pub const JUNCTION_BOX_AT: [f64; 2] = [-17.0, -18.85];
pub const JUNCTION_BOX_SIZE: [f64; 3] = [0.6, 0.9, 0.3];
pub const JUNCTION_AT: [f64; 2] = [JUNCTION_BOX_AT[0], -18.98];
pub const JUNCTION_LIFT: f64 = 1.2;
pub const AUX_GAP: f64 = 0.05;
pub const AUX_AT: [f64; 2] = [JUNCTION_AT[0] + TALL_SCREEN_SIZE[0] + AUX_GAP, JUNCTION_AT[1]];
/// The M-tier screen's spot: on the south wall's inner face next to the reactor (east of it),
/// its height from the floor.
pub const REACTOR_SCREEN_AT: [f64; 2] = [-11.8, -18.96];
pub const REACTOR_SCREEN_LIFT: f64 = 0.9;
/// The readout's size on the wall (m): the M screen's 1.6 : 1 aspect.
pub const READOUT_SIZE: [f64; 2] = [1.6, 1.0];
/// The S screens' size (m): the 4 : 3 canvas.
pub const SMALL_SCREEN_SIZE: [f64; 2] = [0.48, 0.36];
/// The junction box's and AUX screens' size (m): the S width, one tier taller (320 x 320 px).
pub const TALL_SCREEN_SIZE: [f64; 2] = [0.48, 0.48];
pub const STATUS_SIZE: [f64; 2] = READOUT_SIZE;
pub const STATUS_GAP: f64 = 0.05;
pub const STATUS_AT: [f64; 2] = [REACTOR_SCREEN_AT[0] + READOUT_SIZE[0] * 0.5 + STATUS_GAP + STATUS_SIZE[0] * 0.5, REACTOR_SCREEN_AT[1]];
pub const STATUS_LIFT: f64 = REACTOR_SCREEN_LIFT + (READOUT_SIZE[1] - STATUS_SIZE[1]) * 0.5;

pub struct Mount {
    pub id: PanelId,
    pub name: &'static str,
    pub at: [f64; 2],
    pub lift: f64,
    pub size: [f64; 2],
    pub turn: f64,
}

pub const SCREENS: [Mount; 6] = [
    Mount { id: reactor_screen::PUMP_ID, name: reactor_screen::PUMP_NAME, at: [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 0.55], lift: 0.5, size: SMALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::JUNCTION_ID, name: reactor_screen::JUNCTION_NAME, at: JUNCTION_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::AUX_ID, name: reactor_screen::AUX_NAME, at: AUX_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::DOOR_ID, name: reactor_screen::DOOR_NAME, at: [-13.04, -10.0], lift: 1.3, size: SMALL_SCREEN_SIZE, turn: -90.0 },
    Mount { id: reactor_screen::FAULTS_ID, name: reactor_screen::FAULTS_NAME, at: reactor_screen::FAULTS_AT, lift: 1.2, size: SMALL_SCREEN_SIZE, turn: 0.0 },
    Mount { id: reactor_screen::STATUS_ID, name: reactor_screen::STATUS_NAME, at: STATUS_AT, lift: STATUS_LIFT, size: STATUS_SIZE, turn: 180.0 },
];

pub const DOOR: &str = "engine_door";
pub const REACTOR: &str = "engine_reactor";
pub const COOLANT_PUMP: &str = "engine_coolant_pump";
pub const JUNCTION_BOX: &str = "engine_junction_box";
pub const REACTOR_SCREEN: &str = "engine_reactor_screen";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Engine,
    Corridor,
    Outside,
}

pub fn place_of(p: [f64; 2]) -> Place {
    if ROOM.contains(p) {
        Place::Engine
    } else if CORRIDOR.contains(p) {
        Place::Corridor
    } else {
        Place::Outside
    }
}

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

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct EngineDoor {
    pub open: bool,
}

#[derive(Resource)]
pub struct EngineAir {
    pub deck: Deck,
    pub engine: RoomId,
    pub corridor: RoomId,
    pub outside: RoomId,
    pub door: GapId,
}

/// m³: the room 10 x 8 x 3.2 m, the corridor 2 x 6 x 3.2 m; the outside has no end.
const ROOM_VOLUME: f64 = 10.0 * 8.0 * ROOM_HEIGHT;
const CORRIDOR_VOLUME: f64 = 2.0 * 6.0 * ROOM_HEIGHT;
const OUTSIDE_VOLUME: f64 = 1.0e6;

impl EngineAir {
    pub fn new() -> EngineAir {
        let mut deck = Deck::new(FireTuning::default(), 0x296);
        let engine = deck.add_room(ROOM_VOLUME);
        let corridor = deck.add_room(CORRIDOR_VOLUME);
        let outside = deck.add_room(OUTSIDE_VOLUME);
        let door = deck.add_gap(engine, corridor, DVec3::new(DOOR_AT[0], DOOR_AT[1], 0.0), DOOR_AREA, 0.0);
        deck.add_gap(corridor, outside, DVec3::new(DOOR_AT[0], CORRIDOR.centre[1] + CORRIDOR.half[1], 0.0), DOOR_AREA, 1.0);
        EngineAir { deck, engine, corridor, outside, door }
    }

    pub fn room_of(&self, place: Place) -> RoomId {
        match place {
            Place::Engine => self.engine,
            Place::Corridor => self.corridor,
            Place::Outside => self.outside,
        }
    }

    /// The air fraction (0..1) of the room at an area point: what a body breathes there.
    pub fn oxygen_at(&self, p: [f64; 2]) -> f64 {
        self.deck.room(self.room_of(place_of(p))).oxygen_fraction()
    }
}

impl Default for EngineAir {
    fn default() -> EngineAir {
        EngineAir::new()
    }
}

struct Part {
    id: &'static str,
    at: [f64; 2],
    size: [f64; 3],
    lift: f64,
    colour: [f32; 3],
}

const WALL_COLOUR: [f32; 3] = [0.42, 0.44, 0.47];
const DEVICE_COLOUR: [f32; 3] = [0.3, 0.33, 0.38];

const PARTS: [Part; 8] = [
    Part { id: "engine_wall_west", at: [-19.15, -15.15], size: [WALL, ROOM_HEIGHT, 8.3], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_east", at: [-8.85, -15.15], size: [WALL, ROOM_HEIGHT, 8.3], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_south", at: [-14.0, -19.15], size: [10.6, ROOM_HEIGHT, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_north_west", at: [-17.15, -10.85], size: [4.3, ROOM_HEIGHT, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_north_east", at: [-10.85, -10.85], size: [4.3, ROOM_HEIGHT, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_corridor_west", at: [-15.15, -7.7], size: [WALL, ROOM_HEIGHT, 6.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_corridor_east", at: [-12.85, -7.7], size: [WALL, ROOM_HEIGHT, 6.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_ceiling", at: [-14.0, -15.0], size: [10.6, WALL, 8.6], lift: ROOM_HEIGHT, colour: WALL_COLOUR },
];

/// The machines and the screen, standing in the room (not in `PARTS` only because they are
/// named constants above).
const MACHINES: [Part; 3] = [
    Part { id: REACTOR, at: REACTOR_AT, size: REACTOR_SIZE, lift: 0.0, colour: [0.55, 0.3, 0.2] },
    Part { id: COOLANT_PUMP, at: COOLANT_PUMP_AT, size: COOLANT_PUMP_SIZE, lift: 0.0, colour: DEVICE_COLOUR },
    Part { id: JUNCTION_BOX, at: JUNCTION_BOX_AT, size: JUNCTION_BOX_SIZE, lift: 0.0, colour: [0.85, 0.7, 0.2] },
];

fn span(lift: f64, height: f64) -> (f64, f64) {
    if lift > 0.0 { (lift, lift + height) } else { (-SINK, height) }
}

/// A door slab: its shut centre, the slide's direction (east, m) and how far it has slid (0..1).
#[derive(Component, Clone, Debug)]
pub struct Slab {
    pub shut: DVec3,
    pub slide: DVec3,
    pub progress: f64,
}

pub fn walker_place(area: &DevArea, planet: &PlanetRes, p: DVec3) -> Option<Place> {
    let f = area.frame?;
    Some(place_of(area_of(&f, planet, p)))
}

pub fn plugin(app: &mut App) {
    app.init_resource::<ShipAlarm>().init_resource::<EngineDoor>().insert_resource(EngineAir::new());
    app.add_systems(Update, spawn_room.after(super::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(FixedUpdate, (room_state.after(crate::items_net::items_net_in), room_step, breath_step).chain().in_set(crate::phases::Fx::Input));
}

/// Window only: the lights of the room and the corridor follow the grid's lights factor
/// (`reactor_core::LightState`: Barotrauma's flicker and fade, `lights.rs`), the red emergency
/// lamps come on below `EMERGENCY_BELOW`, and the alarm's pulse. The red alarm lights stay on
/// emergency power.
pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (spawn_lights, (room_lights, alarm_lights, emergency_lamps).after(spawn_lights)).after(super::spawn_devices).in_set(crate::phases::Frame::World));
}

pub(super) fn room_state(mut items: ResMut<Items>, mut alarm: ResMut<ShipAlarm>, mut door: ResMut<EngineDoor>) {
    let asks = std::mem::take(&mut items.room_asks);
    match items.role {
        Role::Client => {
            for a in asks {
                items.net_out.push(NetOut::Ask(a));
            }
            door.open = items.room.door_open;
            alarm.level = items.room.alarm;
        }
        Role::Host | Role::Local => {
            for a in asks {
                match a {
                    RoomAsk::ToggleDoor => door.open = !door.open,
                    RoomAsk::CycleAlarm => alarm.level = alarm.level.next(),
                }
            }
            let now = RoomNews { door_open: door.open, alarm: alarm.level };
            if now != items.room {
                items.room = now;
                if items.role == Role::Host {
                    items.net_out.push(NetOut::Room { to: None, r: now });
                }
            }
        }
    }
}

pub(super) fn room_step(time: Res<Time>, door: Res<EngineDoor>, mut air: ResMut<EngineAir>, mut slabs: Query<(&mut Slab, &mut Position)>) {
    let dt = time.delta_secs_f64();
    let goal = if door.open { 1.0 } else { 0.0 };
    let step = dt / DOOR_TIME;
    for (mut slab, mut pos) in &mut slabs {
        slab.progress = if slab.progress < goal { (slab.progress + step).min(goal) } else { (slab.progress - step).max(goal) };
        pos.0 = slab.shut + slab.slide * slab.progress;
        let door = air.door;
        air.deck.set_open(door, slab.progress);
    }
}

/// Host only: every living player breathes the air of the room they stand in, `breath_m3_per_s`
/// a second (`reactor_core` `RoomTuning`). Barotrauma `Character.cs:3835-3848` takes
/// `Hull.OxygenConsumptionSpeed` (`Hull.cs:124`, 700 px²/s) times dt from the hull the character is
/// in; the hull's area is in px (`Hull.cs:378-381`, 100 px = 1 m, `Physics.cs:22`), which is how the
/// rate in m³ comes out. Not ported: Barotrauma's "only while the body's own oxygen is lower"
/// (the body has no oxygen store here). The outside has no end, so nobody drains it. Unconscious
/// players breathe too: every body that is not dead counts.
pub(super) fn breath_step(time: Res<Time>, items: Res<Items>, cfg: Res<super::reactor::ReactorConfig>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, health: Option<Res<crate::health::HealthState>>, mut air: ResMut<EngineAir>) {
    if items.role == Role::Client {
        return;
    }
    let Some(area) = area.as_deref() else { return };
    let dt = time.delta_secs_f64();
    let rate = cfg.0.room.breath_m3_per_s * dt;
    for (id, p) in crate::health::walker_points(&players, &remotes) {
        if health.as_ref().and_then(|h| h.bodies.get(&id)).is_some_and(|b| b.health.state() == health_core::Life::Dead) {
            continue;
        }
        let Some(q) = crate::health::area_pos(Some(area), &planet, Some(p)) else { continue };
        let room = air.room_of(place_of(q));
        if room != air.outside {
            air.deck.breathe(room, rate);
        }
    }
}

pub fn source_at(s: Source) -> [f64; 2] {
    match s {
        Source::Reactor => REACTOR_AT,
        Source::Pump => COOLANT_PUMP_AT,
        Source::Junction => JUNCTION_BOX_AT,
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_room(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, door: Res<EngineDoor>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    for p in PARTS.iter().chain(MACHINES.iter()) {
        let (ground, up) = frame.ground(&planet, p.at[0], p.at[1], 0.0);
        let rot = frame.rotation(up, 0.0);
        let (bottom, top) = span(p.lift, p.size[1]);
        let centre = ground + up * ((bottom + top) * 0.5);
        let size = DVec3::new(p.size[0], top - bottom, p.size[2]);
        let e = commands
            .spawn((
                RigidBody::Static,
                Collider::cuboid(size.x, size.y, size.z),
                Position(centre),
                Rotation(rot),
                CollisionLayers::new(Layer::World, LayerMask::ALL),
                DevBox { id: p.id.into(), colour: p.colour, size },
            ))
            .id();
        area.entities.push(e);
    }
    let (ground, up) = frame.ground(&planet, REACTOR_SCREEN_AT[0], REACTOR_SCREEN_AT[1], 0.0);
    let rot = frame.rotation(up, 180.0);
    let centre = ground + up * (REACTOR_SCREEN_LIFT + READOUT_SIZE[1] * 0.5);
    let e = commands.spawn(DisplayPanel { id: reactor_screen::READOUT_ID, name: reactor_screen::READOUT_NAME.into(), size_m: DVec2::from(READOUT_SIZE), centre, rot }).id();
    area.entities.push(e);
    for m in &SCREENS {
        let (ground, up) = frame.ground(&planet, m.at[0], m.at[1], 0.0);
        let rot = frame.rotation(up, m.turn);
        let centre = ground + up * (m.lift + m.size[1] * 0.5);
        let e = commands.spawn(DisplayPanel { id: m.id.clone(), name: m.name.into(), size_m: DVec2::from(m.size), centre, rot }).id();
        area.entities.push(e);
    }
    let d = DOOR_SIZE;
    let (ground, up) = frame.ground(&planet, DOOR_AT[0], DOOR_AT[1], 0.0);
    let rot = frame.rotation(up, 0.0);
    let (bottom, top) = span(0.0, d[1]);
    let shut = ground + up * ((bottom + top) * 0.5);
    let size = DVec3::new(d[0], top - bottom, d[2]);
    let progress = if door.open { 1.0 } else { 0.0 };
    let e = commands
        .spawn((
            RigidBody::Kinematic,
            Collider::cuboid(size.x, size.y, size.z),
            Position(shut + frame.east * (DOOR_SLIDE * progress)),
            Rotation(rot),
            CollisionLayers::new(Layer::World, LayerMask::ALL),
            DevBox { id: DOOR.into(), colour: [0.55, 0.42, 0.2], size },
            Slab { shut, slide: frame.east * DOOR_SLIDE, progress },
        ))
        .id();
    area.entities.push(e);
}

#[derive(Component)]
struct RoomLight {
    alarm: bool,
}

/// Ours: a red lamp that is lit only in an emergency (the lights factor below 0.1).
#[derive(Component)]
struct EmergencyLamp;

fn spawn_lights(mut commands: Commands, planet: Res<PlanetRes>, area: Res<DevArea>, mut done: Local<Option<warp_core::PlanetId>>, mut made: Local<Vec<Entity>>) {
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
    let lamps = [([-16.5, -15.5], 2.9, false), ([-11.5, -15.5], 2.9, false), ([-14.0, -7.7], 2.9, false), ([-14.0, -15.0], 2.6, true), ([-14.0, -8.6], 2.6, true)];
    for (p, h, alarm) in lamps {
        let colour = if alarm { Color::srgb(1.0, 0.1, 0.05) } else { warm };
        let intensity = if alarm { 0.0 } else { 60_000.0 };
        made.push(
            commands
                .spawn((PointLight { color: colour, intensity, range: 12.0, shadow_maps_enabled: false, ..default() }, Transform::default(), WorldPos(at(p, h)), RoomLight { alarm }))
                .id(),
        );
    }
    // Ours: one red emergency lamp in the room and one in the corridor, dark until the lights fail.
    for c in [ROOM.centre, CORRIDOR.centre] {
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

fn room_lights(
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

fn emergency_lamps(items: Res<Items>, mut lamps: Query<&mut PointLight, With<EmergencyLamp>>) {
    let on = reactor_core::LightState::emergency(lights_voltage(&items));
    for mut light in &mut lamps {
        light.intensity = if on { EMERGENCY_INTENSITY } else { 0.0 };
    }
}

fn alarm_lights(time: Res<Time>, alarm: Res<ShipAlarm>, mut lights: Query<(&RoomLight, &mut PointLight)>) {
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

    #[test]
    fn the_deck_has_the_room_full_of_air_and_the_door_shut() {
        let mut a = EngineAir::new();
        assert!((a.oxygen_at(ROOM.centre) - 1.0).abs() < 1e-9);
        assert!((a.oxygen_at([0.0, -12.0]) - 1.0).abs() < 1e-9);
        a.deck.room_mut(a.engine).oxygen *= 0.25;
        assert!((a.oxygen_at(ROOM.centre) - 0.25).abs() < 1e-9, "the room's air is what the walker breathes");
        assert!((a.oxygen_at(CORRIDOR.centre) - 1.0).abs() < 1e-9, "the corridor has its own air");
        let before = a.deck.total_oxygen();
        for _ in 0..60 {
            let _ = a.deck.step(1.0 / 30.0, &[]);
        }
        assert!((a.deck.total_oxygen() - before).abs() < 1e-6 * before, "air is conserved");
    }
}

fn machine_of(s: Source) -> ([f64; 2], [f64; 3]) {
    match s {
        Source::Reactor => (REACTOR_AT, REACTOR_SIZE),
        Source::Pump => (COOLANT_PUMP_AT, COOLANT_PUMP_SIZE),
        Source::Junction => (JUNCTION_BOX_AT, JUNCTION_BOX_SIZE),
    }
}

/// A machine's half footprint on the floor (m, x and y): what fire measures its reach to.
pub fn footprint_of(s: Source) -> [f64; 2] {
    let (_, size) = machine_of(s);
    [size[0] * 0.5, size[2] * 0.5]
}

/// How far out from a machine's centre a fire starts, beyond its half depth (m).
pub const IGNITION_OUT: f64 = 0.3;

/// The machines' centres are inside their housings, so a fire there shows no flame.
pub fn ignition_at(s: Source) -> [f64; 2] {
    let (c, size) = machine_of(s);
    let to = [ROOM.centre[0] - c[0], ROOM.centre[1] - c[1]];
    if to[0].abs() > to[1].abs() {
        [c[0] + to[0].signum() * (size[0] * 0.5 + IGNITION_OUT), c[1]]
    } else {
        [c[0], c[1] + to[1].signum() * (size[2] * 0.5 + IGNITION_OUT)]
    }
}
