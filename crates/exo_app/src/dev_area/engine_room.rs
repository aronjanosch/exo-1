//! The engine room (#296, slice S2 of the reactor): a greybox room in the dev area's south strip,
//! 10 x 8 m and 3.2 m high, with a short corridor to the open dev area and a door between the two.
//! The room has its air (`fire_core::Deck`: the room, the corridor and "outside", the door as a
//! gap), an alarm light and a quiet alarm tone (`audio`). The reactor and the fire come later; the
//! placeholders stand where the machines go.
//!
//! - Area metres (x east, y north from the place's centre, `dev_area`). The room's zone is
//!   x -19..-9, y -19..-11; the corridor x -15..-13, y -10.7..-4.7 and it opens to the walkway at
//!   its north end. The door is a slab on the room's side of the north wall, slides 2 m east when
//!   open; the walls are boxes on `Layer::World` the walker cannot pass.
//! - Host authority: the door and the alarm are the host's (`item_core::wire::RoomNews`). A use
//!   of F or a click on the test console is a request (`RoomAsk`): the host applies it, a client
//!   sends it and takes the host's state from the wire (`room_state`).
//! - The air: each fixed step the deck steps; the door's gap open fraction is the door's slide
//!   (`room_step`). `health` reads the air of the room the walker stands in (`EngineAir::oxygen_at`).
//! - Machines and the reactor screen are named placeholders (the constants below), so the next
//!   steps attach to them.
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

/// The room's interior and the corridor's, as zones (area metres).
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

/// The machines' placeholders: the reactor (centre-back), the coolant pump, the junction box on
/// the south wall, and the reactor screen's dark panel on the east wall. Area metres and sizes
/// (m, width x height x depth).
pub const REACTOR_AT: [f64; 2] = [-14.0, -17.8];
pub const REACTOR_SIZE: [f64; 3] = [2.0, 2.5, 2.0];
pub const COOLANT_PUMP_AT: [f64; 2] = [-10.5, -17.5];
pub const COOLANT_PUMP_SIZE: [f64; 3] = [1.0, 1.2, 1.0];
pub const JUNCTION_BOX_AT: [f64; 2] = [-17.0, -18.85];
pub const JUNCTION_BOX_SIZE: [f64; 3] = [0.6, 0.9, 0.3];
/// The M-tier screen's spot: on the south wall's inner face next to the reactor (east of it),
/// its height from the floor.
pub const REACTOR_SCREEN_AT: [f64; 2] = [-11.8, -18.96];
pub const REACTOR_SCREEN_LIFT: f64 = 0.9;
/// The readout's size on the wall (m): the M screen's 1.6 : 1 aspect.
pub const READOUT_SIZE: [f64; 2] = [1.6, 1.0];
/// The S screens' size (m): the 4 : 3 canvas.
pub const SMALL_SCREEN_SIZE: [f64; 2] = [0.48, 0.36];

/// A screen standing on a machine or a wall: its panel, the area point it stands at, the height
/// of its bottom (m), its size (m, width and height) and its turn (degrees, 0 south, counter-
/// clockwise, so it faces where the walker comes from).
pub struct Mount {
    pub id: PanelId,
    pub name: &'static str,
    pub at: [f64; 2],
    pub lift: f64,
    pub size: [f64; 2],
    pub turn: f64,
}

/// The coolant pump's screen on its north face (the walker's side), the junction box's on the
/// south wall above the box, and the door's in the corridor on the east wall by the doorway, at
/// eye height; the dev faults screen inside the room on the north wall, west of the door.
pub const SCREENS: [Mount; 4] = [
    Mount { id: reactor_screen::PUMP_ID, name: reactor_screen::PUMP_NAME, at: [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 0.55], lift: 0.5, size: SMALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::JUNCTION_ID, name: reactor_screen::JUNCTION_NAME, at: [JUNCTION_BOX_AT[0], -18.98], lift: 1.2, size: SMALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::DOOR_ID, name: reactor_screen::DOOR_NAME, at: [-13.04, -10.0], lift: 1.3, size: SMALL_SCREEN_SIZE, turn: -90.0 },
    Mount { id: reactor_screen::FAULTS_ID, name: reactor_screen::FAULTS_NAME, at: [-17.5, -11.04], lift: 1.2, size: SMALL_SCREEN_SIZE, turn: 0.0 },
];

/// The device ids of the parts (`DevBox::id`).
pub const DOOR: &str = "engine_door";
pub const REACTOR: &str = "engine_reactor";
pub const COOLANT_PUMP: &str = "engine_coolant_pump";
pub const JUNCTION_BOX: &str = "engine_junction_box";
pub const REACTOR_SCREEN: &str = "engine_reactor_screen";

/// Where the walker is in the engine room's layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Engine,
    Corridor,
    Outside,
}

/// Where a point (area metres) is: the room, the corridor, or the rest of the dev area.
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

/// Host and clients: the alarm level as the room has it (the host sets it, clients take it from
/// the wire).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct ShipAlarm {
    pub level: Alarm,
}

/// The door's target: open or shut (the host sets it, clients take it from the wire).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct EngineDoor {
    pub open: bool,
}

/// The engine room's air: the deck and the ids of its rooms and gaps.
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

    /// The room a place is in, as the deck has it.
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

/// A part of the room: its id, its place in area metres, its size (width along x, height, depth
/// along z) and the height of its bottom (0 stands on the ground, sunk a little as the boxes are).
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
    // The north wall has the doorway between x -15 and -13.
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

/// The bottom and the top of a part above the ground (m): on the ground the bottom is sunk.
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

/// The walker's place as the engine room sees it, if it is in the dev area on foot.
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

/// The state of the room: the requests (host and one process apply them, a client sends them),
/// the host's changes to the wire, and a client's copy of the wire.
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

/// Every process: the door slides towards its target and the air's gap follows it. The air is
/// stepped by `fire::fire_step` (host), after this.
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

/// Where a fire of each source starts (area metres): the machine's centre on the floor.
pub fn source_at(s: Source) -> [f64; 2] {
    match s {
        Source::Reactor => REACTOR_AT,
        Source::Pump => COOLANT_PUMP_AT,
        Source::Junction => JUNCTION_BOX_AT,
    }
}

/// Every process: the room's parts as colliders, and the door as a kinematic slab, once per planet
/// (and again on a planet swap, after the dev area despawned them).
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
    // The reactor's readout: on the south wall next to the reactor, facing north into the room
    // (turn 180 from the pads' south), its centre 0.9 m up plus half its height.
    let (ground, up) = frame.ground(&planet, REACTOR_SCREEN_AT[0], REACTOR_SCREEN_AT[1], 0.0);
    let rot = frame.rotation(up, 180.0);
    let centre = ground + up * (REACTOR_SCREEN_LIFT + READOUT_SIZE[1] * 0.5);
    let e = commands.spawn(DisplayPanel { id: reactor_screen::READOUT_ID, name: reactor_screen::READOUT_NAME.into(), size_m: DVec2::from(READOUT_SIZE), centre, rot }).id();
    area.entities.push(e);
    // The machines' screens (S tier), each on its mount.
    for m in &SCREENS {
        let (ground, up) = frame.ground(&planet, m.at[0], m.at[1], 0.0);
        let rot = frame.rotation(up, m.turn);
        let centre = ground + up * (m.lift + m.size[1] * 0.5);
        let e = commands.spawn(DisplayPanel { id: m.id.clone(), name: m.name.into(), size_m: DVec2::from(m.size), centre, rot }).id();
        area.entities.push(e);
    }
    // The door: a kinematic slab in the room's side of the north wall, slid as the host says.
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

/// A light of the room (window only): the alarm's lights pulse, the others are on.
#[derive(Component)]
struct RoomLight {
    alarm: bool,
}

/// Window: the lights, once per planet, as the walls are.
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

/// Window: the room's white lights follow the lights breaker and the power (the snapshot). A power
/// loss flickers them for a second, then they go dark; the return flickers them on (`effects`).
fn room_lights(time: Res<Time>, items: Res<Items>, mut flicker: Local<crate::effects::LampFlicker>, mut lights: Query<(&RoomLight, &mut PointLight)>) {
    let on = items.reactor.is_some_and(|r| r.powered && r.breakers[item_core::wire::BreakerGroup::Lights.index()]);
    let lit = flicker.update(on, time.elapsed_secs_f64());
    for (l, mut light) in &mut lights {
        if !l.alarm {
            light.intensity = if lit { 60_000.0 } else { 0.0 };
        }
    }
}

/// Window: the alarm lights follow the level: off at None, a pulse at Warn, a faster one at Danger.
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
        // The room's south edge and the pad strip do not meet: the pad main is 7 m east of the room.
        assert!(ROOM.centre[0] + ROOM.half[0] < PAD_MAIN[0] - 5.0);
    }

    #[test]
    fn the_door_is_in_the_doorway_and_the_slide_clears_it() {
        // The shut slab covers the doorway: its span is the doorway's width (x -15..-13).
        assert!((DOOR_AT[0] - (CORRIDOR.centre[0])).abs() < 1e-9);
        assert!((DOOR_SIZE[0] - 2.0).abs() < 1e-9);
        // Open, the slab is east of the doorway: its west edge is at the doorway's east edge.
        let open_west = DOOR_AT[0] + DOOR_SLIDE - DOOR_SIZE[0] * 0.5;
        assert!((open_west - (CORRIDOR.centre[0] + CORRIDOR.half[0])).abs() < 1e-9);
        // The slab is on the room's side of the wall, not in the wall.
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
        // Danger pulses faster than Warn: a Warn period is 2 s, Danger 0.8 s.
        for t in [0.0, 0.1, 0.2, 0.3] {
            let (w, d) = (alarm_pulse(Alarm::Warn, t), alarm_pulse(Alarm::Danger, t));
            assert!((0.0..=1.0).contains(&w) && (0.0..=1.0).contains(&d));
        }
        assert!((alarm_pulse(Alarm::Danger, 0.2) - 1.0).abs() < 1e-9, "a quarter of the Danger period is the top");
    }
}
