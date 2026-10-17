//! The inner ship's layout: the mini ship's rooms (rectangles with roofs), its walls and doors, the
//! machines' housings and the spots of the wall screens, in area metres (x east, y north). The
//! single source of where things stand inside; each owner module keeps what its parts do.
//!
//! The ship lies in the area's south half: a corridor runs east–west; north of it the bridge, the
//! turret room and the lab; south of it the engine room and the medbay; the airlock closes the
//! corridor's east end and is the only way out. Each role's room is shown a second time in its open
//! zone outside (`ROLE_ZONES`): the same screens (same panel, same ship) and dummy housings, to look
//! at a role's machines quickly. TODO(initiator): sizes and order are greybox values (#306).
use super::*;
use crate::displays::DisplayPanel;
use reactor_core::Source;

/// The engine room. Its walls carry the hull's eight sections (`reactor_core::hull`), so it keeps
/// its place and size.
pub const ROOM: Zone = Zone { name: "engine room", centre: [-14.0, -15.0], half: [5.0, 4.0] };
pub const CORRIDOR: Zone = Zone { name: "corridor", centre: [-7.7, -9.7], half: [11.3, 1.0] };
pub const BRIDGE: Zone = Zone { name: "bridge", centre: [-15.0, -5.4], half: [4.0, 3.0] };
pub const TURRET_ROOM: Zone = Zone { name: "turret room", centre: [-7.7, -5.4], half: [3.0, 3.0] };
pub const LAB_ROOM: Zone = Zone { name: "lab room", centre: [-0.4, -5.4], half: [4.0, 3.0] };
pub const MEDBAY_ROOM: Zone = Zone { name: "medbay room", centre: [-5.7, -13.85], half: [3.0, 2.85] };
/// The equipment room (#309): an empty 3.5 x 3 m room at the corridor's east end, between the corridor
/// (a sliding door, #309) and the airlock's inner door. TODO(initiator): size and what goes in.
pub const EQUIPMENT: Zone = Zone { name: "equipment room", centre: [5.65, -9.7], half: [1.75, 1.5] };
/// The airlock: a 2 x 2 m room east of the equipment room; its outer door is the ship's way out.
pub const AIRLOCK: Zone = Zone { name: "airlock", centre: [8.7, -9.7], half: [1.0, 1.0] };
/// Where a walker starts in the dev area: the corridor's west end, by the bridge door.
pub const SPAWN: [f64; 2] = [-17.5, -9.7];

/// The rooms that have a role's zone outside, and that zone (`ROLE_ZONES` in `dev_area`).
pub const ROOM_ZONES: [(Zone, Zone); 5] = [(ROOM, ENGINEERING), (BRIDGE, BRIDGE_ZONE), (TURRET_ROOM, TURRETS), (LAB_ROOM, LAB), (MEDBAY_ROOM, MEDBAY)];

/// The bridge door: its slab's centre when shut, in the corridor's north wall, 2 m wide. The slab
/// sinks into the floor when open, as the airlock's doors do.
pub const BRIDGE_DOOR_AT: [f64; 2] = [-15.0, -8.55];
pub const BRIDGE_DOOR_SIZE: [f64; 3] = [2.0, ROOM_HEIGHT, WALL];
/// The airlock's doors: the inner one closes the equipment room's east end, the outer one opens east
/// onto the outside. Their slabs sink into the floor when open (`hull::airlock`).
pub const INNER_DOOR_AT: [f64; 2] = [7.55, -9.7];
pub const OUTER_DOOR_AT: [f64; 2] = [9.85, -9.7];
pub const AIRLOCK_DOOR_SIZE: [f64; 3] = [WALL, ROOM_HEIGHT, 2.0];
/// The equipment room's door to the corridor (#309): its slab's centre when shut, in the west wall's
/// 2 m opening. It sinks into the floor when open, as the airlock's doors do.
pub const EQUIP_DOOR_AT: [f64; 2] = [3.75, -9.7];
pub const EQUIP_DOOR_SIZE: [f64; 3] = [WALL, ROOM_HEIGHT, 2.0];
/// The rooms' height (m); every room has a roof.
pub const ROOM_HEIGHT: f64 = 3.2;
/// Thickness of the walls and the doors (m).
pub const WALL: f64 = 0.3;
/// The engine room's door: its slab's centre when shut (area metres), its size, and how far it
/// slides east when open (m), and how long that takes (s).
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
/// The oxygen generator: a box on the engine room's east wall (inner face x -9.0), off the walk from
/// the door (x -14) to the machines and clear of the reactor, pump and junction box screens.
pub const OXYGEN_GENERATOR_AT: [f64; 2] = [-9.4, -13.0];
pub const OXYGEN_GENERATOR_SIZE: [f64; 3] = [0.8, 1.2, 0.8];
pub const JUNCTION_AT: [f64; 2] = [JUNCTION_BOX_AT[0], -18.98];
pub const JUNCTION_LIFT: f64 = 1.2;
pub const AUX_GAP: f64 = 0.05;
pub const AUX_AT: [f64; 2] = [JUNCTION_AT[0] + TALL_SCREEN_SIZE[0] + AUX_GAP, JUNCTION_AT[1]];
/// The battery's screen: beside the AUX screen, on the same line. TODO(initiator): the spot on the wall.
pub const BATTERY_AT: [f64; 2] = [AUX_AT[0] + TALL_SCREEN_SIZE[0] + AUX_GAP, JUNCTION_AT[1]];
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
/// The status monitor ST-01 on the engine room's east wall, south of the oxygen generator.
pub const MONITOR_AT: [f64; 2] = [-9.04, -16.9];
/// The ship's power panel SP-01 on the engine room's west wall, south of the vent.
pub const STARTUP_AT: [f64; 2] = [-18.96, -16.5];

/// The bridge's wall points (panels sit 0.04 m inside their wall's face): the helm on the north wall
/// (the ship's front), the jump drive and the shield on the west wall, the captain's console CP-01
/// on the east wall, the bridge's vent on the north wall.
pub const HELM_AT: [f64; 2] = [BRIDGE.centre[0], BRIDGE.centre[1] + BRIDGE.half[1] - 0.04];
pub const JUMP_AT: [f64; 2] = [BRIDGE.centre[0] - BRIDGE.half[0] + 0.04, -4.4];
pub const SHIELD_AT: [f64; 2] = [BRIDGE.centre[0] - BRIDGE.half[0] + 0.04, -6.4];
pub const CAPTAIN_AT: [f64; 2] = [BRIDGE.centre[0] + BRIDGE.half[0] - 0.04, BRIDGE.centre[1]];
pub const BRIDGE_VENT_AT: [f64; 2] = [-17.5, BRIDGE.centre[1] + BRIDGE.half[1] - 0.04];
/// The gun's screen GN-01 on the turret room's north wall.
pub const GUNNERY_AT: [f64; 2] = [TURRET_ROOM.centre[0], TURRET_ROOM.centre[1] + TURRET_ROOM.half[1] - 0.04];
/// The corridor's wall points: the faces are y -10.66 (south wall, facing north) and y -8.74
/// (north wall, facing south). ER-01 sits east of the engine door, BD-01 east of the bridge door.
/// The airlock's panels (#309): the first on the airlock's north wall inside (the right-hand wall seen
/// from the outer door; face y -8.7), the second on that wall's outer face (y -8.4) beside the outer door,
/// the third in the equipment room on the inner door's wall (face x 7.4), right of the door seen from the
/// room (south of it), facing east.
pub const ENGINE_DOOR_PANEL_AT: [f64; 2] = [-12.2, -10.66];
pub const BRIDGE_DOOR_PANEL_AT: [f64; 2] = [-13.4, -8.74];
pub const AIRLOCK_PANEL_AT: [[f64; 2]; 3] = [[8.7, -8.7], [9.5, -8.4], [7.4, -10.95]];
/// The airlock wall panel (#308): 0.20 x 0.32 m screen, its centre 1.4 m up (eye and hand height; start value).
pub const AIRLOCK_SCREEN_SIZE: [f64; 2] = [0.20, 0.32];
pub const AIRLOCK_PANEL_CENTRE: f64 = 1.4;
pub const AIRLOCK_PANEL_MODEL: &str = "ship/airlock_wall_panel.glb";
/// The vents' wall points: the engine room's west wall and the corridor's south wall.
pub const VENT_AT: [[f64; 2]; 2] = [[-18.9, -14.0], [1.5, -10.66]];
pub const VENT_LIFT: f64 = 1.3;

pub struct Mount {
    pub id: PanelId,
    pub name: &'static str,
    pub at: [f64; 2],
    pub lift: f64,
    pub size: [f64; 2],
    pub turn: f64,
}

use crate::dev_area::engineering::panels as engineering_panels;
use crate::dev_area::hull::panels as hull_panels;

/// The wall screens. A screen's turn is the way it faces: 0 south, 90 east, 180 north, -90 west.
pub const SCREENS: &[Mount] = &[
    // The engine room: the reactor's readout by the reactor, the pump's on the pump, the junction
    // box on the south wall, the engine room's vent on the west wall. Parked (#306 Parkplatz): RX-02,
    // AUX, the battery, ST-01 and SP-01 (their `Mount` lines are in git history, commit of #306).
    Mount { id: engineering_panels::READOUT_ID, name: engineering_panels::READOUT_NAME, at: REACTOR_SCREEN_AT, lift: REACTOR_SCREEN_LIFT, size: READOUT_SIZE, turn: 180.0 },
    Mount { id: engineering_panels::PUMP_ID, name: engineering_panels::PUMP_NAME, at: [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 0.55], lift: 0.5, size: SMALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: engineering_panels::JUNCTION_ID, name: engineering_panels::JUNCTION_NAME, at: JUNCTION_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: hull_panels::VENT_ENGINE_ID, name: hull_panels::VENT_ENGINE_NAME, at: VENT_AT[0], lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 90.0 },
    // The corridor: ER-01 and the corridor's vent on the south wall, BD-01 on the north wall. The
    // airlock's panels (#309): one inside on the north wall, one outside beside the outer door.
    Mount { id: hull_panels::DOOR_ID, name: hull_panels::DOOR_NAME, at: ENGINE_DOOR_PANEL_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: hull_panels::VENT_CORRIDOR_ID, name: hull_panels::VENT_CORRIDOR_NAME, at: VENT_AT[1], lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: hull_panels::BRIDGE_DOOR_ID, name: hull_panels::BRIDGE_DOOR_NAME, at: BRIDGE_DOOR_PANEL_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 0.0 },
    Mount { id: hull_panels::AIRLOCK_IN_ID, name: hull_panels::AIRLOCK_IN_NAME, at: AIRLOCK_PANEL_AT[0], lift: AIRLOCK_PANEL_CENTRE - AIRLOCK_SCREEN_SIZE[1] * 0.5, size: AIRLOCK_SCREEN_SIZE, turn: 0.0 },
    Mount { id: hull_panels::AIRLOCK_OUT_ID, name: hull_panels::AIRLOCK_OUT_NAME, at: AIRLOCK_PANEL_AT[1], lift: AIRLOCK_PANEL_CENTRE - AIRLOCK_SCREEN_SIZE[1] * 0.5, size: AIRLOCK_SCREEN_SIZE, turn: 180.0 },
    Mount { id: hull_panels::AIRLOCK_SHIP_ID, name: hull_panels::AIRLOCK_SHIP_NAME, at: AIRLOCK_PANEL_AT[2], lift: AIRLOCK_PANEL_CENTRE - AIRLOCK_SCREEN_SIZE[1] * 0.5, size: AIRLOCK_SCREEN_SIZE, turn: 270.0 },
    // The bridge: only its vent. Parked (#306 Parkplatz): the helm, JD-01, the shield and CP-01.
    Mount { id: hull_panels::VENT_BRIDGE_ID, name: hull_panels::VENT_BRIDGE_NAME, at: BRIDGE_VENT_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 0.0 },
    // The turret room: empty. Parked (#306 Parkplatz): GN-01 and the gun's housing (`TURRET_GUN`).
];

pub const DOOR: &str = "engine_door";
pub const BRIDGE_DOOR: &str = "bridge_door";
pub const EQUIP_DOOR: &str = "equipment_door";
pub const REACTOR: &str = "engine_reactor";
pub const COOLANT_PUMP: &str = "engine_coolant_pump";
pub const JUNCTION_BOX: &str = "engine_junction_box";
pub const OXYGEN_GENERATOR: &str = "engine_oxygen_generator";
pub const TURRET_GUN: &str = "turret_gun";

/// Where a point is, for the air: each room of the ship, or outside. The turret room, the lab, the
/// medbay open onto the corridor without a door, so they breathe the corridor's air.
/// TODO(initiator): each gets its own air once the wire's room count grows (`ROOMS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Engine,
    Corridor,
    /// The equipment room: its own air behind its door (#309).
    Equipment,
    Airlock,
    Bridge,
    Outside,
}

pub fn place_of(p: [f64; 2]) -> Place {
    if ROOM.contains(p) {
        Place::Engine
    } else if CORRIDOR.contains(p) || TURRET_ROOM.contains(p) || LAB_ROOM.contains(p) || MEDBAY_ROOM.contains(p) {
        Place::Corridor
    } else if EQUIPMENT.contains(p) {
        Place::Equipment
    } else if AIRLOCK.contains(p) {
        Place::Airlock
    } else if BRIDGE.contains(p) {
        Place::Bridge
    } else {
        Place::Outside
    }
}

/// The floor area of the rooms that share the corridor's air (m²): the equipment room has its own.
pub const CORRIDOR_AREA: f64 = 4.0 * (CORRIDOR.half[0] * CORRIDOR.half[1] + TURRET_ROOM.half[0] * TURRET_ROOM.half[1] + LAB_ROOM.half[0] * LAB_ROOM.half[1] + MEDBAY_ROOM.half[0] * MEDBAY_ROOM.half[1]);
/// The equipment room's floor area (m²).
pub const EQUIPMENT_AREA: f64 = 4.0 * EQUIPMENT.half[0] * EQUIPMENT.half[1];

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
const H: f64 = ROOM_HEIGHT;

/// The mini ship's walls and roofs. Wall centre lines are 0.15 m outside the rooms' edges; the
/// doorways of the turret room, the lab and the medbay are 1.6 m gaps, the doors' openings 2 m.
const PARTS: &[Part] = &[
    // The engine room (x -19..-9, y -19..-11), its door in the north wall at x -15..-13.
    Part { id: "engine_wall_west", at: [-19.15, -15.15], size: [WALL, H, 8.3], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_east", at: [-8.85, -15.15], size: [WALL, H, 8.3], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_south", at: [-14.0, -19.15], size: [10.6, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_north_west", at: [-17.15, -10.85], size: [4.3, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_wall_north_east", at: [-10.85, -10.85], size: [4.3, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_ceiling", at: [-14.0, -15.0], size: [10.6, WALL, 8.6], lift: H, colour: WALL_COLOUR },
    // The medbay (x -8.7..-2.7, y -16.7..-11), its doorway in the corridor's south wall at x -6.5..-4.9.
    Part { id: "medbay_wall_north_west", at: [-7.6, -10.85], size: [2.2, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "medbay_wall_north_east", at: [-3.65, -10.85], size: [2.5, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "medbay_wall_south", at: [-5.55, -16.85], size: [6.3, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "medbay_wall_east", at: [-2.55, -13.85], size: [WALL, H, 6.3], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "medbay_roof", at: [-5.55, -13.85], size: [6.3, WALL, 6.3], lift: H, colour: WALL_COLOUR },
    // The corridor (x -19..3.6, y -10.7..-8.7): its south wall east of the medbay, its west end, its
    // north wall with the bridge door (x -16..-14) and the doorways of the turret room and the lab.
    Part { id: "corridor_wall_south_east", at: [0.675, -10.85], size: [6.15, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_west", at: [-19.15, -9.7], size: [WALL, H, 2.6], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_north_1", at: [-17.65, -8.55], size: [3.3, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_north_2", at: [-11.25, -8.55], size: [5.5, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_north_3", at: [-4.05, -8.55], size: [5.7, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_north_4", at: [2.15, -8.55], size: [3.5, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_roof", at: [-7.7, -9.7], size: [23.2, WALL, 2.6], lift: H, colour: WALL_COLOUR },
    // The north row (y -8.4..-2.4): the bridge (x -19..-11), the turret room (x -10.7..-4.7) and the
    // lab (x -4.4..3.6), one roof over all three.
    Part { id: "north_wall", at: [-7.7, -2.25], size: [23.2, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "bridge_wall_west", at: [-19.15, -5.4], size: [WALL, H, 6.6], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "bridge_wall_east", at: [-10.85, -5.4], size: [WALL, H, 6.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "turret_wall_east", at: [-4.55, -5.4], size: [WALL, H, 6.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "lab_wall_east", at: [3.75, -5.25], size: [WALL, H, 6.3], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "north_roof", at: [-7.7, -5.4], size: [23.2, WALL, 6.6], lift: H, colour: WALL_COLOUR },
    // The equipment room (x 3.9..7.4, y -11.2..-8.2): closed to the corridor at its west end by a door (a 2 m
    // opening, wall stubs either side), closed by the airlock's inner door at its east end.
    Part { id: "equipment_wall_north", at: [5.65, -8.05], size: [4.1, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "equipment_wall_south", at: [5.65, -11.35], size: [4.1, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    // (the west wall's 2 m opening is closed by the equipment door's slab)
    Part { id: "equipment_wall_west_north", at: [3.75, -8.3], size: [WALL, H, 0.8], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "equipment_wall_west_south", at: [3.75, -11.1], size: [WALL, H, 0.8], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "equipment_wall_east_north", at: [7.55, -8.3], size: [WALL, H, 0.8], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "equipment_wall_east_south", at: [7.55, -11.1], size: [WALL, H, 0.8], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "equipment_roof", at: [5.65, -9.7], size: [4.1, WALL, 3.6], lift: H, colour: WALL_COLOUR },
    // The airlock (x 7.7..9.7): its north and south walls and roof; its doors close the ends.
    Part { id: "airlock_wall_north", at: [8.7, -8.55], size: [2.6, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "airlock_wall_south", at: [8.7, -10.85], size: [2.6, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "airlock_roof", at: [8.7, -9.7], size: [2.6, WALL, 2.6], lift: H, colour: WALL_COLOUR },
];

/// The machines' housings: the engine room's, and the gun's breech in the turret room (west of GN-01,
/// off the walk from the doorway to the screen). TODO(initiator): the gun's place and size.
const MACHINES: &[Part] = &[
    Part { id: REACTOR, at: REACTOR_AT, size: REACTOR_SIZE, lift: 0.0, colour: [0.55, 0.3, 0.2] },
    Part { id: COOLANT_PUMP, at: COOLANT_PUMP_AT, size: COOLANT_PUMP_SIZE, lift: 0.0, colour: DEVICE_COLOUR },
    Part { id: JUNCTION_BOX, at: JUNCTION_BOX_AT, size: JUNCTION_BOX_SIZE, lift: 0.0, colour: [0.85, 0.7, 0.2] },
    Part { id: OXYGEN_GENERATOR, at: OXYGEN_GENERATOR_AT, size: OXYGEN_GENERATOR_SIZE, lift: 0.0, colour: [0.3, 0.5, 0.45] },
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

/// The outside copy of a point in one of the role rooms: the same spot in the role's zone. None for a
/// point in the corridor, the airlock or outside.
pub fn outside_copy(p: [f64; 2]) -> Option<[f64; 2]> {
    ROOM_ZONES.iter().find(|(room, _)| room.contains(p)).map(|(room, zone)| [p[0] - room.centre[0] + zone.centre[0], p[1] - room.centre[1] + zone.centre[1]])
}

/// Marks a role zone's copy of a room's screen or housing: the screen is the same panel as the
/// one inside (the same ship), the housing a dummy (repairs and rods work inside only).
#[derive(Component, Clone, Copy, Debug)]
pub struct Mirror;

/// The thin board an outside screen hangs on, so it does not float (m).
const BOARD_DEPTH: f64 = 0.06;
const BOARD_COLOUR: [f32; 3] = [0.18, 0.19, 0.21];

/// Spawns one part as a static box on the ground (`Layer::World`), with the part sunk below a
/// floor-standing part's foot (`span`).
pub fn spawn_part(commands: &mut Commands, frame: &Frame, planet: &PlanetRes, p: &Part) -> Entity {
    spawn_box(commands, frame, planet, p.id.to_string(), p.at, p.size, p.lift, 0.0, p.colour)
}

#[allow(clippy::too_many_arguments)]
fn spawn_box(commands: &mut Commands, frame: &Frame, planet: &PlanetRes, id: String, at: [f64; 2], size: [f64; 3], lift: f64, turn: f64, colour: [f32; 3]) -> Entity {
    let (ground, up) = frame.ground(planet, at[0], at[1], 0.0);
    let rot = frame.rotation(up, turn);
    let (bottom, top) = span(lift, size[1]);
    let centre = ground + up * ((bottom + top) * 0.5);
    let size = DVec3::new(size[0], top - bottom, size[2]);
    commands
        .spawn((
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            Position(centre),
            Rotation(rot),
            CollisionLayers::new(Layer::World, LayerMask::ALL),
            DevBox { id, colour, size },
        ))
        .id()
}

fn is_airlock_panel(id: &PanelId) -> bool {
    *id == hull_panels::AIRLOCK_IN_ID || *id == hull_panels::AIRLOCK_OUT_ID || *id == hull_panels::AIRLOCK_SHIP_ID
}

fn spawn_screen(commands: &mut Commands, frame: &Frame, planet: &PlanetRes, m: &Mount, at: [f64; 2]) -> Entity {
    let (ground, up) = frame.ground(planet, at[0], at[1], 0.0);
    let rot = frame.rotation(up, m.turn);
    let mut centre = ground + up * (m.lift + m.size[1] * 0.5);
    let panel = DisplayPanel { id: m.id.clone(), name: m.name.into(), size_m: DVec2::from(m.size), centre, rot };
    if is_airlock_panel(&m.id) {
        // The wall panel's model has its origin at the wall surface; the screen stands in front of the glass.
        centre += rot * DVec3::Z * crate::displays::FRAMED_SCREEN_OFFSET;
        return commands.spawn((DisplayPanel { centre, ..panel }, crate::displays::FramedBy(AIRLOCK_PANEL_MODEL))).id();
    }
    commands.spawn(panel).id()
}

/// Once per planet: the mini ship's walls, the machines' housings and the wall screens, and each role
/// room's copy in its zone outside: the screens on boards, the housings as dummies.
fn spawn_room(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    for p in PARTS.iter().chain(MACHINES.iter()) {
        let e = spawn_part(&mut commands, &frame, &planet, p);
        area.entities.push(e);
    }
    for p in MACHINES {
        let Some(at) = outside_copy(p.at) else { continue };
        let e = spawn_box(&mut commands, &frame, &planet, format!("{}_outside", p.id), at, p.size, p.lift, 0.0, p.colour);
        commands.entity(e).insert(Mirror);
        area.entities.push(e);
    }
    for m in SCREENS {
        let e = spawn_screen(&mut commands, &frame, &planet, m, m.at);
        area.entities.push(e);
        let Some(at) = outside_copy(m.at) else { continue };
        let e = spawn_screen(&mut commands, &frame, &planet, m, at);
        commands.entity(e).insert(Mirror);
        area.entities.push(e);
        if is_airlock_panel(&m.id) {
            continue;
        }
        // The board behind the screen: the screen faces along its turn, the board sits just behind it.
        let back = (m.turn.to_radians().sin() * -(BOARD_DEPTH * 0.5 + 0.01), m.turn.to_radians().cos() * (BOARD_DEPTH * 0.5 + 0.01));
        let board = [at[0] + back.0, at[1] + back.1];
        let size = [m.size[0] + 0.1, m.size[1] + 0.1, BOARD_DEPTH];
        let e = spawn_box(&mut commands, &frame, &planet, format!("{}_board", m.name), board, size, m.lift - 0.05, m.turn, BOARD_COLOUR);
        commands.entity(e).insert(Mirror);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every screen stands in a room of the ship, and a role room's screen has its copy in its zone.
    #[test]
    fn every_screen_is_in_a_room_and_role_rooms_have_their_copy() {
        for m in SCREENS {
            // The airlock's outside panel hangs on the hull beside the outer door.
            assert!(place_of(m.at) != Place::Outside || m.id == hull_panels::AIRLOCK_OUT_ID, "{} stands in a room", m.name);
            if let Some(at) = outside_copy(m.at) {
                assert!(ROLE_ZONES.iter().any(|z| z.contains(at)), "{}'s copy stands in a role zone", m.name);
            }
        }
        assert_eq!(place_of(SPAWN), Place::Corridor, "the walker starts inside the ship");
    }
}
