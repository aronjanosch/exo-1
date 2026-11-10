//! The inner ship's layout: the mini ship's rooms (rectangles with roofs), its walls and doors, the
//! machines' housings and the spots of the wall screens, in area metres (x east, y north). The
//! single source of where things stand inside; each owner module keeps what its parts do.
//!
//! The ship lies in the area's south half: a corridor runs east–west; north of it the bridge, the
//! turret room and the lab; south of it the engine room and the medbay; the airlock closes the
//! corridor's east end and is the only way out. The bridge, the turret room, the lab and the medbay are
//! empty (walls, roof, doors, floor: their machines and screens were removed); only the engine room has
//! the reactor and the junction boxes, the airlock its panels. TODO(initiator): sizes and order are greybox values (#306).
use super::*;
use crate::displays::DisplayPanel;
use reactor_core::Source;

/// The engine room. Its walls carry the hull's eight sections (`reactor_core::hull`), so it keeps
/// its place and size.
pub const ROOM: Zone = Zone { name: "engine room", centre: [-14.0, -15.0], half: [5.0, 4.0] };
/// The corridor: 3 m wide (#311), grown to the north so the rooms south of it keep their places.
pub const CORRIDOR: Zone = Zone { name: "corridor", centre: [-7.7, -9.2], half: [11.3, 1.5] };
pub const BRIDGE: Zone = Zone { name: "bridge", centre: [-15.0, -4.4], half: [4.0, 3.0] };
pub const TURRET_ROOM: Zone = Zone { name: "turret room", centre: [-7.7, -4.4], half: [3.0, 3.0] };
pub const LAB_ROOM: Zone = Zone { name: "lab room", centre: [-0.4, -4.4], half: [4.0, 3.0] };
pub const MEDBAY_ROOM: Zone = Zone { name: "medbay room", centre: [-5.7, -13.85], half: [3.0, 2.85] };
/// The storage room (#312): south of the corridor's east part, between the medbay's east wall (x -2.55, which is its west
/// wall) and the equipment room; a 1.6 m doorway without a door in the corridor's south wall (x -0.2..1.4).
/// TODO(initiator): size (start value).
pub const STORAGE_ROOM: Zone = Zone { name: "storage room", centre: [0.6, -13.5], half: [3.0, 2.5] };
/// The equipment room (#309, #311): 6 x 4 m at the corridor's east end, between the corridor (a
/// sliding door) and the airlock's inner door; its north wall lines up with the corridor's.
/// TODO(initiator): size (start value).
pub const EQUIPMENT: Zone = Zone { name: "equipment room", centre: [6.9, -9.7], half: [3.0, 2.0] };
/// The airlock: a 2 x 2 m room east of the equipment room; its outer door is the ship's way out.
pub const AIRLOCK: Zone = Zone { name: "airlock", centre: [11.2, -9.7], half: [1.0, 1.0] };
/// Where a walker starts in the dev area: outside in the dev zone (centre [18, 15.5]), 1.5 m south
/// of its middle so the item table (2.5 m north) stands in front.
pub const SPAWN: [f64; 2] = [18.0, 14.0];

/// The bridge door: its slab's centre when shut, in the corridor's north wall, 2 m wide. The slab
/// sinks into the floor when open, as the airlock's doors do.
pub const BRIDGE_DOOR_AT: [f64; 2] = [-15.0, -7.55];
pub const BRIDGE_DOOR_SIZE: [f64; 3] = [2.0, ROOM_HEIGHT, WALL];
/// The airlock's doors: the inner one closes the equipment room's east end, the outer one opens east
/// onto the outside. Their slabs sink into the floor when open (`hull::airlock`).
pub const INNER_DOOR_AT: [f64; 2] = [10.05, -9.7];
pub const OUTER_DOOR_AT: [f64; 2] = [12.35, -9.7];
pub const AIRLOCK_DOOR_SIZE: [f64; 3] = [WALL, ROOM_HEIGHT, 2.0];
/// The equipment room's door to the corridor (#309): its slab's centre when shut, in the west wall's
/// 2 m opening (y -10.7..-8.7). It sinks into the floor when open, as the airlock's doors do.
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

/// The reactor model's origin (#316): on the floor, its console facing the door (north), set back from the door.
/// TODO(initiator): the set-back (start value: 1.0 m south of the room's centre).
pub const REACTOR_SET_BACK: f64 = 1.0;
pub const REACTOR_AT: [f64; 2] = [ROOM.centre[0], ROOM.centre[1] - REACTOR_SET_BACK];
/// The base drum's box (the collision and what the wrench aims at: repair of the reactor); the rest of the model's collision is
/// `engineering::reactor_room`.
pub const REACTOR_SIZE: [f64; 3] = [3.0, 1.0, 3.0];
/// Where a walker stands in front of the reactor's drum, clear of the console (m, area): the bots' station and the blast scenario use it.
pub const REACTOR_FRONT_AT: [f64; 2] = [REACTOR_AT[0], REACTOR_AT[1] + 2.6];
/// The coolant pump and the oxygen generator have no housing now; their spots stay as where a fire of theirs starts
/// and where their smoke shows (`ignition_at`, `effects`), and their size as the fire's footprint. TODO(initiator): the spots with the new machines.
pub const COOLANT_PUMP_AT: [f64; 2] = [-10.5, -17.5];
pub const COOLANT_PUMP_SIZE: [f64; 3] = [1.0, 1.2, 1.0];
/// The three junction boxes (#322): where each stands on its wall (the model's origin, on the wall surface under the
/// cabinet's centre line) and the way it faces (`Mount::turn`: 0 south, 90 east, 180 north, -90 west).
/// JB-1 on the engine room's south wall, JB-2 on the corridor's south wall east of ER-01, JB-3 on the turret room's east wall
/// (it stands in for the shield area). TODO(initiator): the three spots.
pub struct JunctionSpot {
    pub at: [f64; 2],
    pub turn: f64,
}

pub const JUNCTION_SPOTS: [JunctionSpot; 3] = [
    JunctionSpot { at: [-17.0, -18.98], turn: 180.0 },
    JunctionSpot { at: [-10.0, -10.66], turn: 180.0 },
    JunctionSpot { at: [-4.74, -4.4], turn: -90.0 },
];
pub const JUNCTION_MODEL: &str = "ship/junction_box.glb";
/// The cabinet's size from `art/ship/junction_box.md` (m): width, height, depth; it hangs from `JUNCTION_LIFT` up.
pub const JUNCTION_BOX_SIZE: [f64; 3] = [0.56, 0.84, 0.17];
pub const JUNCTION_LIFT: f64 = 0.92;
/// Where the model's `spark_point` is: 3 cm in front of the door, at this height (m, from the floor).
pub const JUNCTION_SPARK_HEIGHT: f64 = 1.32;
pub const JUNCTION_SPARK_OUT: f64 = JUNCTION_BOX_SIZE[2] + 0.03;

/// The way a wall point faces on the floor plan (unit x, y) for a `turn` in degrees.
pub fn facing(turn: f64) -> [f64; 2] {
    [turn.to_radians().sin(), -turn.to_radians().cos()]
}

/// A point `out` metres in front of junction box `i`'s wall point.
pub fn junction_front(i: usize, out: f64) -> [f64; 2] {
    let s = &JUNCTION_SPOTS[i];
    let f = facing(s.turn);
    [s.at[0] + f[0] * out, s.at[1] + f[1] * out]
}

/// The cabinet's centre on the floor plan.
pub fn junction_centre(i: usize) -> [f64; 2] {
    junction_front(i, JUNCTION_BOX_SIZE[2] * 0.5)
}

/// Whether `id` is one of the junction boxes' hit volumes (`JUNCTION_BOX`).
pub fn is_junction_box(id: &str) -> bool {
    JUNCTION_BOX.contains(&id)
}
pub const OXYGEN_GENERATOR_AT: [f64; 2] = [-9.4, -13.0];
pub const OXYGEN_GENERATOR_SIZE: [f64; 3] = [0.8, 1.2, 0.8];
/// The bots' stand points for the helm and the gun (the bridge and the turret room are empty now: the screens are gone).
pub const HELM_AT: [f64; 2] = [BRIDGE.centre[0], BRIDGE.centre[1] + BRIDGE.half[1] - 0.04];
pub const GUNNERY_AT: [f64; 2] = [TURRET_ROOM.centre[0], TURRET_ROOM.centre[1] + TURRET_ROOM.half[1] - 0.04];
/// The airlock's panels (#309): the first on the airlock's north wall inside (the right-hand wall seen
/// from the outer door; face y -8.7), the second on that wall's outer face (y -8.4) beside the outer door,
/// the third in the equipment room on the inner door's wall (face x 9.9), south of the door, 0.5 m from
/// its frame and from the room's corner, facing west into the room.
pub const AIRLOCK_PANEL_AT: [[f64; 2]; 3] = [[11.2, -8.7], [12.0, -8.4], [9.9, -11.2]];
/// The airlock wall panel (#308): 0.20 x 0.32 m screen, its centre 1.4 m up (eye and hand height; start value).
pub const AIRLOCK_SCREEN_SIZE: [f64; 2] = [0.20, 0.32];
pub const AIRLOCK_PANEL_CENTRE: f64 = 1.4;
pub const AIRLOCK_PANEL_MODEL: &str = "ship/airlock_wall_panel.glb";

pub struct Mount {
    pub id: PanelId,
    pub name: &'static str,
    pub at: [f64; 2],
    pub lift: f64,
    pub size: [f64; 2],
    pub turn: f64,
}

use crate::dev_area::hull::panels as hull_panels;

/// The wall screens: only the airlock's three panels are left (the reactor's own screens are part of its model, `engineering::reactor_room`). A screen's turn is the way it faces: 0 south, 90 east, 180 north, -90 west.
pub const SCREENS: &[Mount] = &[
    // The airlock's panels (#309): one inside on the north wall, one outside beside the outer door, one in the equipment room.
    Mount { id: hull_panels::AIRLOCK_IN_ID, name: hull_panels::AIRLOCK_IN_NAME, at: AIRLOCK_PANEL_AT[0], lift: AIRLOCK_PANEL_CENTRE - AIRLOCK_SCREEN_SIZE[1] * 0.5, size: AIRLOCK_SCREEN_SIZE, turn: 0.0 },
    Mount { id: hull_panels::AIRLOCK_OUT_ID, name: hull_panels::AIRLOCK_OUT_NAME, at: AIRLOCK_PANEL_AT[1], lift: AIRLOCK_PANEL_CENTRE - AIRLOCK_SCREEN_SIZE[1] * 0.5, size: AIRLOCK_SCREEN_SIZE, turn: 180.0 },
    Mount { id: hull_panels::AIRLOCK_SHIP_ID, name: hull_panels::AIRLOCK_SHIP_NAME, at: AIRLOCK_PANEL_AT[2], lift: AIRLOCK_PANEL_CENTRE - AIRLOCK_SCREEN_SIZE[1] * 0.5, size: AIRLOCK_SCREEN_SIZE, turn: 270.0 },
];

pub const DOOR: &str = "engine_door";
pub const BRIDGE_DOOR: &str = "bridge_door";
pub const EQUIP_DOOR: &str = "equipment_door";
pub const REACTOR: &str = "engine_reactor";
/// The junction boxes' hit volumes (repair aims at them): JB-1, JB-2, JB-3.
pub const JUNCTION_BOX: [&str; 3] = ["junction_box_1", "junction_box_2", "junction_box_3"];

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
    } else if CORRIDOR.contains(p) || TURRET_ROOM.contains(p) || LAB_ROOM.contains(p) || MEDBAY_ROOM.contains(p) || STORAGE_ROOM.contains(p) {
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
pub const CORRIDOR_AREA: f64 = 4.0 * (CORRIDOR.half[0] * CORRIDOR.half[1] + TURRET_ROOM.half[0] * TURRET_ROOM.half[1] + LAB_ROOM.half[0] * LAB_ROOM.half[1] + MEDBAY_ROOM.half[0] * MEDBAY_ROOM.half[1] + STORAGE_ROOM.half[0] * STORAGE_ROOM.half[1]);
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
    // The storage room (x -2.4..3.6, y -16..-11): the medbay's east wall is its west wall, its doorway is the 1.6 m gap in the
    // corridor's south wall (x -0.2..1.4); its east wall stops at the equipment room's south wall.
    Part { id: "storage_wall_south", at: [0.75, -16.15], size: [6.3, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "storage_wall_east", at: [3.75, -14.0], size: [WALL, H, 4.6], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "storage_roof", at: [0.75, -13.5], size: [6.3, WALL, 5.6], lift: H, colour: WALL_COLOUR },
    // The corridor (x -19..3.6, y -10.7..-7.7, 3 m wide, #311): its south wall east of the medbay, its
    // west end, its north wall with the bridge door (x -16..-14) and the doorways of the turret room and the lab.
    Part { id: "corridor_wall_south_east_1", at: [-1.3, -10.85], size: [2.2, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_south_east_2", at: [2.575, -10.85], size: [2.35, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_west", at: [-19.15, -9.2], size: [WALL, H, 3.6], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_north_1", at: [-17.65, -7.55], size: [3.3, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_north_2", at: [-11.25, -7.55], size: [5.5, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_north_3", at: [-4.05, -7.55], size: [5.7, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_wall_north_4", at: [2.0, -7.55], size: [3.2, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "corridor_roof", at: [-7.7, -9.2], size: [23.2, WALL, 3.6], lift: H, colour: WALL_COLOUR },
    // The north row (y -7.4..-1.4): the bridge (x -19..-11), the turret room (x -10.7..-4.7) and the
    // lab (x -4.4..3.6), one roof over all three.
    Part { id: "north_wall", at: [-7.7, -1.25], size: [23.2, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "bridge_wall_west", at: [-19.15, -4.4], size: [WALL, H, 6.6], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "bridge_wall_east", at: [-10.85, -4.4], size: [WALL, H, 6.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "turret_wall_east", at: [-4.55, -4.4], size: [WALL, H, 6.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "lab_wall_east", at: [3.75, -4.25], size: [WALL, H, 6.3], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "north_roof", at: [-7.7, -4.4], size: [23.2, WALL, 6.6], lift: H, colour: WALL_COLOUR },
    // The equipment room (x 3.9..9.9, y -11.7..-7.7): closed to the corridor at its west end by a door (a 2 m
    // opening, wall stubs either side), closed by the airlock's inner door at its east end.
    Part { id: "equipment_wall_north", at: [6.9, -7.55], size: [6.6, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "equipment_wall_south", at: [6.9, -11.85], size: [6.6, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    // (the west wall's 2 m opening is closed by the equipment door's slab)
    Part { id: "equipment_wall_west_north", at: [3.75, -8.2], size: [WALL, H, 1.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "equipment_wall_west_south", at: [3.75, -11.2], size: [WALL, H, 1.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "equipment_wall_east_north", at: [10.05, -8.2], size: [WALL, H, 1.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "equipment_wall_east_south", at: [10.05, -11.2], size: [WALL, H, 1.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "equipment_roof", at: [6.9, -9.7], size: [6.6, WALL, 4.6], lift: H, colour: WALL_COLOUR },
    // The airlock (x 10.2..12.2): its north and south walls and roof; its doors close the ends.
    Part { id: "airlock_wall_north", at: [11.2, -8.55], size: [2.6, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "airlock_wall_south", at: [11.2, -10.85], size: [2.6, H, WALL], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "airlock_roof", at: [11.2, -9.7], size: [2.6, WALL, 2.6], lift: H, colour: WALL_COLOUR },
];

/// The machines' housings: only the reactor's drum (its collision and what the wrench aims at; the model is `engineering::reactor_room`).
const MACHINES: &[Part] = &[Part { id: REACTOR, at: REACTOR_AT, size: REACTOR_SIZE, lift: 0.0, colour: [0.55, 0.3, 0.2] }];

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
        Source::Junction(i) => junction_centre(i),
        Source::OxygenGenerator => OXYGEN_GENERATOR_AT,
    }
}

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

/// Once per planet: the mini ship's walls, the reactor's drum, the junction boxes' volumes and the airlock's wall screens.
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
    for (i, spot) in JUNCTION_SPOTS.iter().enumerate() {
        // The cabinet's volume: the collision and what repair aims at (the model is drawn by `engineering::junction_boxes`).
        let e = spawn_box(&mut commands, &frame, &planet, JUNCTION_BOX[i].to_string(), junction_centre(i), JUNCTION_BOX_SIZE, JUNCTION_LIFT, spot.turn, WALL_COLOUR);
        area.entities.push(e);
    }
    for m in SCREENS {
        let e = spawn_screen(&mut commands, &frame, &planet, m, m.at);
        area.entities.push(e);
    }
}

fn machine_of(s: Source) -> ([f64; 2], [f64; 3]) {
    match s {
        Source::Reactor => (REACTOR_AT, REACTOR_SIZE),
        Source::Pump => (COOLANT_PUMP_AT, COOLANT_PUMP_SIZE),
        Source::Junction(i) => (junction_centre(i), JUNCTION_BOX_SIZE),
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
    // A junction box's fire starts at its `spark_point`, in front of the door.
    if let Source::Junction(i) = s {
        return junction_front(i, JUNCTION_SPARK_OUT);
    }
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

    /// Every screen stands in a room of the ship (the airlock's outside panel hangs on the hull), and the walker starts in the dev zone.
    #[test]
    fn every_screen_is_in_a_room_and_the_walker_starts_outside() {
        for m in SCREENS {
            assert!(place_of(m.at) != Place::Outside || m.id == hull_panels::AIRLOCK_OUT_ID, "{} stands in a room", m.name);
        }
        assert!(place_of(SPAWN) == Place::Outside && DEV.contains(SPAWN), "the walker starts in the dev zone");
    }

    /// The storage room breathes the corridor's air, and what stands in it stands inside its walls.
    #[test]
    fn the_storage_room_holds_its_cabinets_and_crates() {
        assert_eq!(place_of(STORAGE_ROOM.centre), Place::Corridor);
        for c in storage::cabinets() {
            let sideways = (c.turn.abs() - 90.0).abs() < 1.0;
            let half = if sideways { [c.size[2] * 0.5, c.size[0] * 0.5] } else { [c.size[0] * 0.5, c.size[2] * 0.5] };
            let at = STORAGE_ROOM.point(c.at);
            for corner in [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]] {
                let p = [at[0] + corner[0] * half[0], at[1] + corner[1] * half[1]];
                assert!(STORAGE_ROOM.contains(p), "{} stands inside the storage room", c.id);
            }
        }
    }
}
