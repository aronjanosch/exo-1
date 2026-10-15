//! The inner ship's layout: its rooms, walls, machine housings and the spots of the wall screens,
//! in area metres (x east, y north). The single source of where things stand inside; each owner
//! module (`hull`, `engineering`, `bridge`, ...) keeps what its parts do.
//! TODO(initiator): the layout is a greybox and is rebuilt with the mini ship (#306, step 2).
use super::*;
use crate::dev_area::bridge::captain_screen;
use crate::dev_area::engineering::startup_screen;
use crate::dev_area::hull::status_monitor;
use crate::displays::DisplayPanel;
use reactor_core::Source;

pub const ROOM: Zone = Zone { name: "engine room", centre: [-14.0, -15.0], half: [5.0, 4.0] };
pub const CORRIDOR: Zone = Zone { name: "corridor", centre: [-14.0, -7.7], half: [1.0, 3.0] };
/// The airlock (r40, #267): a 2 x 2 m room at the corridor's far end, where the corridor met the
/// outside. Its inner door is the corridor's north edge (y -4.7), its outer door 2 m further north.
pub const AIRLOCK: Zone = Zone { name: "airlock", centre: [-14.0, -3.7], half: [1.0, 1.0] };
/// The bridge (r44): the room east of the corridor, where HM-01, GN-01, JD-01 and CP-01 stand. Its
/// walls are outside the zone (the corridor's east wall is its west side). TODO(initiator): the layout.
pub const BRIDGE: Zone = Zone { name: "bridge", centre: [-9.0, -7.7], half: [4.0, 3.0] };
/// The bridge door (r44): its slab's centre when shut, in the corridor's east wall, 2 m wide. The slab
/// sinks into the floor when open, as the airlock's doors do.
pub const BRIDGE_DOOR_AT: [f64; 2] = [-12.85, -7.7];
pub const BRIDGE_DOOR_SIZE: [f64; 3] = [WALL, ROOM_HEIGHT, 2.0];
pub const INNER_DOOR_AT: [f64; 2] = [-14.0, -4.7];
pub const OUTER_DOOR_AT: [f64; 2] = [-14.0, -2.7];
/// The room's height (m); the room has a ceiling, the corridor is open to the sky.
pub const ROOM_HEIGHT: f64 = 3.2;
/// Thickness of the walls and the door (m).
pub const WALL: f64 = 0.3;
/// The airlock's doors: their slabs sink into the floor when open (`airlock.rs`).
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
/// The oxygen generator (r37): a box on the east wall (inner face x -9.0), off the walk from the
/// door (x -14) to the machines and clear of the reactor, pump and junction box screens.
pub const OXYGEN_GENERATOR_AT: [f64; 2] = [-9.4, -13.0];
pub const OXYGEN_GENERATOR_SIZE: [f64; 3] = [0.8, 1.2, 0.8];
pub const JUNCTION_AT: [f64; 2] = [JUNCTION_BOX_AT[0], -18.98];
pub const JUNCTION_LIFT: f64 = 1.2;
pub const AUX_GAP: f64 = 0.05;
pub const AUX_AT: [f64; 2] = [JUNCTION_AT[0] + TALL_SCREEN_SIZE[0] + AUX_GAP, JUNCTION_AT[1]];
/// The battery's screen (r38): beside the AUX screen, on the same line. TODO(initiator): the spot on the wall.
pub const BATTERY_AT: [f64; 2] = [AUX_AT[0] + TALL_SCREEN_SIZE[0] + AUX_GAP, JUNCTION_AT[1]];
/// The shield's screen (r40, SH-01): beside the battery's, on the same line. TODO(initiator): the spot on the wall.
pub const SHIELD_AT: [f64; 2] = [BATTERY_AT[0] + TALL_SCREEN_SIZE[0] + AUX_GAP, JUNCTION_AT[1]];
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

pub const SCREENS: &[Mount] = &[
    Mount { id: crate::dev_area::engineering::panels::PUMP_ID, name: crate::dev_area::engineering::panels::PUMP_NAME, at: [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 0.55], lift: 0.5, size: SMALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: crate::dev_area::engineering::panels::JUNCTION_ID, name: crate::dev_area::engineering::panels::JUNCTION_NAME, at: JUNCTION_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: crate::dev_area::engineering::panels::AUX_ID, name: crate::dev_area::engineering::panels::AUX_NAME, at: AUX_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: crate::dev_area::engineering::panels::BATTERY_ID, name: crate::dev_area::engineering::panels::BATTERY_NAME, at: BATTERY_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: crate::dev_area::bridge::panels::SHIELD_ID, name: crate::dev_area::bridge::panels::SHIELD_NAME, at: SHIELD_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: crate::dev_area::hull::panels::DOOR_ID, name: crate::dev_area::hull::panels::DOOR_NAME, at: [-13.04, -10.0], lift: 1.3, size: SMALL_SCREEN_SIZE, turn: -90.0 },
    Mount { id: crate::dev_area::dev::faults::FAULTS_ID, name: crate::dev_area::dev::faults::FAULTS_NAME, at: crate::dev_area::dev::faults::FAULTS_AT, lift: 1.2, size: SMALL_SCREEN_SIZE, turn: 0.0 },
    Mount { id: crate::dev_area::engineering::panels::STATUS_ID, name: crate::dev_area::engineering::panels::STATUS_NAME, at: STATUS_AT, lift: STATUS_LIFT, size: STATUS_SIZE, turn: 180.0 },
    Mount { id: status_monitor::ID, name: status_monitor::NAME, at: status_monitor::AT, lift: status_monitor::LIFT, size: status_monitor::SIZE, turn: status_monitor::TURN },
    // The vents (r38, r44): each is a small grille panel with a valve on its wall, facing its room.
    // The engine room's on the west wall (turn 90 faces +x), the corridor's on the east wall beside
    // the bridge door's panel (turn -90 faces -x), the bridge's on its north wall (turn 0 faces -y).
    Mount { id: crate::dev_area::hull::panels::VENT_ENGINE_ID, name: crate::dev_area::hull::panels::VENT_ENGINE_NAME, at: VENT_AT[0], lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 90.0 },
    Mount { id: crate::dev_area::hull::panels::VENT_CORRIDOR_ID, name: crate::dev_area::hull::panels::VENT_CORRIDOR_NAME, at: VENT_AT[1], lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: -90.0 },
    // The bridge (r44): the helm on the east wall, the gunnery and the captain's console on the south
    // wall, the jump drive and the bridge's vent on the north wall. TODO(initiator): the layout.
    Mount { id: crate::dev_area::bridge::panels::HELM_ID, name: crate::dev_area::bridge::panels::HELM_NAME, at: HELM_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: -90.0 },
    Mount { id: crate::dev_area::turrets::panels::GUNNERY_ID, name: crate::dev_area::turrets::panels::GUNNERY_NAME, at: GUNNERY_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: crate::dev_area::bridge::panels::JUMP_ID, name: crate::dev_area::bridge::panels::JUMP_NAME, at: JUMP_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 0.0 },
    Mount { id: crate::dev_area::hull::panels::VENT_BRIDGE_ID, name: crate::dev_area::hull::panels::VENT_BRIDGE_NAME, at: BRIDGE_VENT_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 0.0 },
    // The bridge door's panel (r44, BD-01): on the corridor's east wall beside the door, north of the corridor's vent.
    Mount { id: crate::dev_area::hull::panels::BRIDGE_DOOR_ID, name: crate::dev_area::hull::panels::BRIDGE_DOOR_NAME, at: BRIDGE_DOOR_PANEL_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: -90.0 },
    // The airlock's two panels (r40): the inner one on the corridor's west wall (faces east, into the
    // corridor), the outer one on the airlock's west wall (inside, by the outer door). Both show IN, OUT, CYCLE.
    Mount { id: crate::dev_area::hull::panels::AIRLOCK_INNER_ID, name: crate::dev_area::hull::panels::AIRLOCK_INNER_NAME, at: AIRLOCK_PANEL_AT[0], lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 90.0 },
    Mount { id: crate::dev_area::hull::panels::AIRLOCK_OUTER_ID, name: crate::dev_area::hull::panels::AIRLOCK_OUTER_NAME, at: AIRLOCK_PANEL_AT[1], lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 90.0 },
    // The ship's power panel (r42, SP-01): the cold start's checklist, on the corridor's east wall between the gunnery and the reactor room's panel.
    Mount { id: startup_screen::ID, name: startup_screen::NAME, at: startup_screen::AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: startup_screen::TURN },
    // The captain's console (r41, CP-01): on the bridge's south wall, beside the gunnery (r44).
    Mount { id: captain_screen::ID, name: captain_screen::NAME, at: captain_screen::AT, lift: VENT_LIFT, size: captain_screen::SIZE, turn: captain_screen::TURN },
];
/// The airlock panels' wall points (area metres): inner (corridor's west wall), outer (airlock's west wall).
pub const AIRLOCK_PANEL_AT: [[f64; 2]; 2] = [[-15.04, -5.0], [-15.04, -3.0]];
/// The bridge's wall points (area metres, r44). The helm stands on the east (front) wall, the gunnery
/// and the captain's console on the south wall (facing north), the jump drive and the bridge's vent
/// on the north wall (facing south). The panels sit 0.04 m inside their wall's face.
pub const HELM_AT: [f64; 2] = [BRIDGE.centre[0] + BRIDGE.half[0] - 0.04, BRIDGE.centre[1]];
pub const GUNNERY_AT: [f64; 2] = [-9.5, BRIDGE.centre[1] - BRIDGE.half[1] + 0.04];
pub const JUMP_AT: [f64; 2] = [-9.5, BRIDGE.centre[1] + BRIDGE.half[1] - 0.04];
pub const BRIDGE_VENT_AT: [f64; 2] = [-12.0, BRIDGE.centre[1] + BRIDGE.half[1] - 0.04];
/// The bridge door's panel (area metres): the corridor's east wall, in the stretch north of the door's
/// opening (y -6.7) and clear of the corridor's vent.
pub const BRIDGE_DOOR_PANEL_AT: [f64; 2] = [-13.04, -6.1];
/// The vents' wall points (area metres): the engine room's west wall and the corridor's east wall,
/// north of the bridge door's opening.
pub const VENT_AT: [[f64; 2]; 2] = [[-18.9, -14.0], [-13.04, -5.0]];
pub const VENT_LIFT: f64 = 1.3;

pub const DOOR: &str = "engine_door";
pub const BRIDGE_DOOR: &str = "bridge_door";
pub const REACTOR: &str = "engine_reactor";
pub const COOLANT_PUMP: &str = "engine_coolant_pump";
pub const JUNCTION_BOX: &str = "engine_junction_box";
pub const OXYGEN_GENERATOR: &str = "engine_oxygen_generator";
pub const REACTOR_SCREEN: &str = "engine_reactor_screen";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Engine,
    Corridor,
    Airlock,
    Bridge,
    Outside,
}

pub fn place_of(p: [f64; 2]) -> Place {
    if ROOM.contains(p) {
        Place::Engine
    } else if CORRIDOR.contains(p) {
        Place::Corridor
    } else if AIRLOCK.contains(p) {
        Place::Airlock
    } else if BRIDGE.contains(p) {
        Place::Bridge
    } else {
        Place::Outside
    }
}

/// A static greybox part: a wall, a roof or a machine's housing (area metres, size x, height, z).
pub struct Part {
    pub id: &'static str,
    pub at: [f64; 2],
    pub size: [f64; 3],
    pub lift: f64,
    pub colour: [f32; 3],
}

pub const WALL_COLOUR: [f32; 3] = [0.42, 0.44, 0.47];
const DEVICE_COLOUR: [f32; 3] = [0.3, 0.33, 0.38];

const PARTS: &[Part] = &[
    Part { id: "engine_wall_west", at: [-19.15, -15.15], size: [WALL, ROOM_HEIGHT, 8.3], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_east", at: [-8.85, -15.15], size: [WALL, ROOM_HEIGHT, 8.3], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_south", at: [-14.0, -19.15], size: [10.6, ROOM_HEIGHT, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_north_west", at: [-17.15, -10.85], size: [4.3, ROOM_HEIGHT, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_north_east", at: [-10.85, -10.85], size: [4.3, ROOM_HEIGHT, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_corridor_west", at: [-15.15, -7.7], size: [WALL, ROOM_HEIGHT, 6.0], lift: 0.0, colour: WALL_COLOUR },
    // The corridor's east wall, cut for the bridge door (r44): the opening is -8.7..-6.7.
    Part { id: "engine_corridor_east_south", at: [-12.85, -9.7], size: [WALL, ROOM_HEIGHT, 2.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_corridor_east_north", at: [-12.85, -5.7], size: [WALL, ROOM_HEIGHT, 2.0], lift: 0.0, colour: WALL_COLOUR },
    // The bridge's walls (r44), outside its zone: the south wall from the engine room's east wall to
    // the bridge's east wall, the east wall (the front), the north wall to the corridor's east wall.
    Part { id: "engine_bridge_south", at: [-6.85, -10.85], size: [3.7, ROOM_HEIGHT, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_bridge_east", at: [-4.85, -7.7], size: [WALL, ROOM_HEIGHT, 6.3], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_bridge_north", at: [-8.85, -4.55], size: [7.7, ROOM_HEIGHT, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_ceiling", at: [-14.0, -15.0], size: [10.6, WALL, 8.6], lift: ROOM_HEIGHT, colour: WALL_COLOUR },
];

/// The machines and the screen, standing in the room (not in `PARTS` only because they are
/// named constants above).
const MACHINES: &[Part] = &[
    Part { id: REACTOR, at: REACTOR_AT, size: REACTOR_SIZE, lift: 0.0, colour: [0.55, 0.3, 0.2] },
    Part { id: COOLANT_PUMP, at: COOLANT_PUMP_AT, size: COOLANT_PUMP_SIZE, lift: 0.0, colour: DEVICE_COLOUR },
    Part { id: JUNCTION_BOX, at: JUNCTION_BOX_AT, size: JUNCTION_BOX_SIZE, lift: 0.0, colour: [0.85, 0.7, 0.2] },
    Part { id: OXYGEN_GENERATOR, at: OXYGEN_GENERATOR_AT, size: OXYGEN_GENERATOR_SIZE, lift: 0.0, colour: [0.3, 0.5, 0.45] },
];

/// The airlock's walls (r40): the room's west and east walls, from the inner door's line to the outer
/// door's. The doors close the ends (`airlock.rs`).
const AIRLOCK_WALLS: &[Part] = &[
    Part { id: "engine_airlock_west", at: [-15.15, -3.7], size: [WALL, ROOM_HEIGHT, 2.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_airlock_east", at: [-12.85, -3.7], size: [WALL, ROOM_HEIGHT, 2.0], lift: 0.0, colour: WALL_COLOUR },
];

pub fn span(lift: f64, height: f64) -> (f64, f64) {
    if lift > 0.0 { (lift, lift + height) } else { (-SINK, height) }
}

pub fn walker_place(area: &DevArea, planet: &PlanetRes, p: DVec3) -> Option<Place> {
    let f = area.frame?;
    Some(place_of(area_of(&f, planet, p)))
}

pub fn source_at(s: Source) -> [f64; 2] {
    match s {
        Source::Reactor => REACTOR_AT,
        Source::Pump => COOLANT_PUMP_AT,
        Source::Junction => JUNCTION_BOX_AT,
        Source::OxygenGenerator => OXYGEN_GENERATOR_AT,
    }
}

/// Spawns one part as a static box on the ground (`Layer::World`), with the part sunk below a
/// floor-standing part's foot (`span`).
pub fn spawn_part(commands: &mut Commands, frame: &Frame, planet: &PlanetRes, p: &Part) -> Entity {
    let (ground, up) = frame.ground(planet, p.at[0], p.at[1], 0.0);
    let rot = frame.rotation(up, 0.0);
    let (bottom, top) = span(p.lift, p.size[1]);
    let centre = ground + up * ((bottom + top) * 0.5);
    let size = DVec3::new(p.size[0], top - bottom, p.size[2]);
    commands
        .spawn((
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            Position(centre),
            Rotation(rot),
            CollisionLayers::new(Layer::World, LayerMask::ALL),
            DevBox { id: p.id.into(), colour: p.colour, size },
        ))
        .id()
}

/// Once per planet: the inner ship's walls, the machines' housings and the wall screens.
fn spawn_room(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    for p in PARTS.iter().chain(AIRLOCK_WALLS.iter()).chain(MACHINES.iter()) {
        let e = spawn_part(&mut commands, &frame, &planet, p);
        area.entities.push(e);
    }
    let (ground, up) = frame.ground(&planet, REACTOR_SCREEN_AT[0], REACTOR_SCREEN_AT[1], 0.0);
    let rot = frame.rotation(up, 180.0);
    let centre = ground + up * (REACTOR_SCREEN_LIFT + READOUT_SIZE[1] * 0.5);
    let e = commands.spawn(DisplayPanel { id: crate::dev_area::engineering::panels::READOUT_ID, name: crate::dev_area::engineering::panels::READOUT_NAME.into(), size_m: DVec2::from(READOUT_SIZE), centre, rot }).id();
    area.entities.push(e);
    for m in SCREENS.iter() {
        let (ground, up) = frame.ground(&planet, m.at[0], m.at[1], 0.0);
        let rot = frame.rotation(up, m.turn);
        let centre = ground + up * (m.lift + m.size[1] * 0.5);
        let e = commands.spawn(DisplayPanel { id: m.id.clone(), name: m.name.into(), size_m: DVec2::from(m.size), centre, rot }).id();
        area.entities.push(e);
    }
}

fn machine_of(s: Source) -> ([f64; 2], [f64; 3]) {
    match s {
        Source::Reactor => (REACTOR_AT, REACTOR_SIZE),
        Source::Pump => (COOLANT_PUMP_AT, COOLANT_PUMP_SIZE),
        Source::Junction => (JUNCTION_BOX_AT, JUNCTION_BOX_SIZE),
        Source::OxygenGenerator => (OXYGEN_GENERATOR_AT, OXYGEN_GENERATOR_SIZE),
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

pub fn plugin(app: &mut App) {
    app.add_systems(Update, spawn_room.after(super::spawn_devices).in_set(crate::phases::Frame::World));
}
