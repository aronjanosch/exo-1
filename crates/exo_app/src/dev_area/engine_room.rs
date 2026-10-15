use super::*;
use crate::dev_area::reactor_screen;
use crate::displays::DisplayPanel;
use crate::env::PlanetRes;
use crate::items::{Items, NetOut, Role};
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
    app.add_systems(FixedUpdate, (room_state.after(crate::items_net::items_net_in), room_step).chain().in_set(crate::phases::Fx::Input));
}

/// Window only: the lights of the room and the corridor (on when the lights breaker is on and the
/// machines have power, else dark; the red alarm lights stay on emergency power), and the alarm's
/// pulse.
pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (spawn_lights, (room_lights, alarm_lights).after(spawn_lights)).after(super::spawn_devices).in_set(crate::phases::Frame::World));
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
}

fn room_lights(time: Res<Time>, items: Res<Items>, mut flicker: Local<crate::effects::LampFlicker>, mut lights: Query<(&RoomLight, &mut PointLight)>) {
    let on = items.reactor.is_some_and(|r| r.powered && r.breakers[item_core::wire::BreakerGroup::Lights.index()]);
    let lit = flicker.update(on, time.elapsed_secs_f64());
    for (l, mut light) in &mut lights {
        if !l.alarm {
            light.intensity = if lit { 60_000.0 } else { 0.0 };
        }
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
    fn the_aux_screen_sits_beside_the_junction_box_screen_at_its_height() {
        let screens: &[Mount] = &SCREENS;
        let jb = screens.iter().find(|m| m.id == reactor_screen::JUNCTION_ID).expect("the junction box screen is mounted");
        let aux = screens.iter().find(|m| m.id == reactor_screen::AUX_ID).expect("the AUX screen is mounted");
        assert_eq!(aux.at[1], jb.at[1], "on the same wall");
        assert_eq!(aux.lift, jb.lift, "the same height");
        assert_eq!(aux.size, jb.size, "the same size");
        assert_eq!(aux.turn, jb.turn, "facing the same way");
        let jb_east = jb.at[0] + jb.size[0] * 0.5;
        let aux_west = aux.at[0] - aux.size[0] * 0.5;
        assert!(aux_west >= jb_east && aux_west - jb_east < 0.1, "beside the junction box screen, no overlap: {jb_east} and {aux_west}");
    }

    #[test]
    fn the_status_screen_sits_beside_the_readout_on_the_same_wall() {
        let screens: &[Mount] = &SCREENS;
        let m = screens.iter().find(|m| m.id == reactor_screen::STATUS_ID).expect("the status screen is mounted");
        let readout_east = REACTOR_SCREEN_AT[0] + READOUT_SIZE[0] * 0.5;
        let status_west = m.at[0] - m.size[0] * 0.5;
        assert!(status_west >= readout_east && status_west - readout_east < 0.1, "the status screen touches the readout's east edge: {readout_east} and {status_west}");
        assert_eq!(m.at[1], REACTOR_SCREEN_AT[1], "on the same wall");
        assert_eq!(m.turn, 180.0, "facing into the room, as the readout does");
        assert!((m.lift + m.size[1] * 0.5 - (REACTOR_SCREEN_LIFT + READOUT_SIZE[1] * 0.5)).abs() < 1e-9, "centred at the readout's height");
        assert!(m.at[0] + m.size[0] * 0.5 < ROOM.centre[0] + ROOM.half[0] - 0.2, "clear of the room's east corner: {}", m.at[0] + m.size[0] * 0.5);
        assert_eq!(m.size, READOUT_SIZE, "as big as the readout (the M tier)");
    }

    #[test]
    fn the_room_and_the_corridor_are_where_the_layout_says() {
        assert_eq!(place_of([-14.0, -15.0]), Place::Engine);
        assert_eq!(place_of([-14.0, -8.0]), Place::Corridor);
        assert_eq!(place_of([0.0, -12.0]), Place::Outside, "the pad main is outside");
        assert_eq!(place_of([-14.0, -4.0]), Place::Outside, "the walkway past the corridor's end");
        assert!((ROOM.half[0] * 2.0 - 10.0).abs() < 1e-9 && (ROOM.half[1] * 2.0 - 8.0).abs() < 1e-9, "10 x 8 m");
        assert!((CORRIDOR.half[0] * 2.0 - 2.0).abs() < 1e-9 && (CORRIDOR.half[1] * 2.0 - 6.0).abs() < 1e-9, "2 m wide, 6 m long");
        for z in [&ROOM, &CORRIDOR] {
            let inside = |p: [f64; 2]| p[0].abs() <= HALF_EXTENT[0] && p[1].abs() <= HALF_EXTENT[1];
            assert!(inside([z.centre[0] - z.half[0], z.centre[1] - z.half[1]]) && inside([z.centre[0] + z.half[0], z.centre[1] + z.half[1]]), "{} is in the area", z.name);
        }
        assert!(ROOM.centre[0] + ROOM.half[0] < PAD_MAIN[0] - 5.0);
    }

    #[test]
    fn the_door_is_in_the_doorway_and_the_slide_clears_it() {
        assert!((DOOR_AT[0] - (CORRIDOR.centre[0])).abs() < 1e-9);
        assert!((DOOR_SIZE[0] - 2.0).abs() < 1e-9);
        let open_west = DOOR_AT[0] + DOOR_SLIDE - DOOR_SIZE[0] * 0.5;
        assert!((open_west - (CORRIDOR.centre[0] + CORRIDOR.half[0])).abs() < 1e-9);
        assert!(DOOR_AT[1] < ROOM.centre[1] + ROOM.half[1] + 1e-9 && DOOR_AT[1] + WALL * 0.5 <= -11.0 + 1e-9);
    }

    #[test]
    fn the_machines_stand_inside_the_room() {
        for p in MACHINES.iter().filter(|p| p.lift == 0.0) {
            assert!(ROOM.contains([p.at[0] - p.size[0] * 0.5, p.at[1] - p.size[2] * 0.5]) && ROOM.contains([p.at[0] + p.size[0] * 0.5, p.at[1] + p.size[2] * 0.5]), "{} is in the room", p.id);
            assert!(p.size[1] < ROOM_HEIGHT);
        }
        assert_eq!(REACTOR_SIZE, [2.0, 2.5, 2.0]);
        assert_eq!(COOLANT_PUMP_SIZE, [1.0, 1.2, 1.0]);
        assert_eq!(JUNCTION_BOX_SIZE, [0.6, 0.9, 0.3]);
    }

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

    #[test]
    fn the_alarm_pulses_and_tones_by_level() {
        assert_eq!(alarm_pulse(Alarm::None, 1.3), 0.0);
        assert_eq!(tone_every(Alarm::None), None);
        assert_eq!(tone_every(Alarm::Warn), Some(2.0));
        assert_eq!(tone_every(Alarm::Danger), Some(1.0));
        for t in [0.0, 0.1, 0.2, 0.3] {
            let (w, d) = (alarm_pulse(Alarm::Warn, t), alarm_pulse(Alarm::Danger, t));
            assert!((0.0..=1.0).contains(&w) && (0.0..=1.0).contains(&d));
        }
        assert!((alarm_pulse(Alarm::Danger, 0.2) - 1.0).abs() < 1e-9, "a quarter of the Danger period is the top");
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

#[cfg(test)]
mod placement_tests {
    use super::*;

    fn inside_box(p: [f64; 2], c: [f64; 2], size: [f64; 3]) -> bool {
        (p[0] - c[0]).abs() < size[0] * 0.5 && (p[1] - c[1]).abs() < size[2] * 0.5
    }

    #[test]
    fn ignition_is_outside_every_machine_and_inside_the_room() {
        for s in [Source::Reactor, Source::Pump, Source::Junction] {
            let p = ignition_at(s);
            for m in [Source::Reactor, Source::Pump, Source::Junction] {
                let (c, size) = machine_of(m);
                assert!(!inside_box(p, c, size), "{s:?} fire at {p:?} is inside the {m:?} box");
            }
            assert!((p[0] - ROOM.centre[0]).abs() < ROOM.half[0], "{s:?} fire at {p:?} is outside the room (x)");
            assert!((p[1] - ROOM.centre[1]).abs() < ROOM.half[1], "{s:?} fire at {p:?} is outside the room (y)");
        }
    }

    #[test]
    fn ignition_sits_on_the_front_face_at_half_depth_plus_out() {
        assert_eq!(ignition_at(Source::Reactor), [REACTOR_AT[0], REACTOR_AT[1] + REACTOR_SIZE[2] * 0.5 + IGNITION_OUT]);
        assert_eq!(ignition_at(Source::Pump), [COOLANT_PUMP_AT[0] - (COOLANT_PUMP_SIZE[0] * 0.5 + IGNITION_OUT), COOLANT_PUMP_AT[1]]);
    }
}
