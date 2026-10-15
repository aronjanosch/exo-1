use super::*;
use crate::dev_area::captain_screen;
use crate::dev_area::reactor_screen;
use crate::dev_area::startup_screen;
use crate::dev_area::status_monitor;
use crate::displays::DisplayPanel;
use crate::env::PlanetRes;
use crate::items::{Items, NetOut, Role};
use crate::net_live::RemoteWalker;
use crate::origin::WorldPose;
use crate::walker::Player;
use crate::origin::WorldPos;
use crate::Layer;
use fire_core::{split_by_volume, Deck, FireTuning, GapId, LeakId, RoomId};
use item_core::PlayerId;
use std::collections::BTreeSet;
use reactor_core::hull::{openness, SECTION_CENTRES, WALLS_MAX, WALLS_MIN};
use reactor_core::engine_room::AirSave;
use reactor_core::{Airlock, AirlockTuning, Dir, Door, Goal, Source, Tuning};
use display_core::PanelId;
use item_core::wire::{Alarm, RoomAlarmKind, RoomAsk, RoomNews, HULL_SECTIONS};

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
const WALL: f64 = 0.3;
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
/// The bridge door's state (r44): open, as the engine room's `EngineDoor`.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct BridgeDoor {
    pub open: bool,
}

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
    Mount { id: reactor_screen::PUMP_ID, name: reactor_screen::PUMP_NAME, at: [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 0.55], lift: 0.5, size: SMALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::JUNCTION_ID, name: reactor_screen::JUNCTION_NAME, at: JUNCTION_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::AUX_ID, name: reactor_screen::AUX_NAME, at: AUX_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::BATTERY_ID, name: reactor_screen::BATTERY_NAME, at: BATTERY_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::SHIELD_ID, name: reactor_screen::SHIELD_NAME, at: SHIELD_AT, lift: JUNCTION_LIFT, size: TALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::DOOR_ID, name: reactor_screen::DOOR_NAME, at: [-13.04, -10.0], lift: 1.3, size: SMALL_SCREEN_SIZE, turn: -90.0 },
    Mount { id: reactor_screen::FAULTS_ID, name: reactor_screen::FAULTS_NAME, at: reactor_screen::FAULTS_AT, lift: 1.2, size: SMALL_SCREEN_SIZE, turn: 0.0 },
    Mount { id: reactor_screen::STATUS_ID, name: reactor_screen::STATUS_NAME, at: STATUS_AT, lift: STATUS_LIFT, size: STATUS_SIZE, turn: 180.0 },
    Mount { id: status_monitor::ID, name: status_monitor::NAME, at: status_monitor::AT, lift: status_monitor::LIFT, size: status_monitor::SIZE, turn: status_monitor::TURN },
    // The vents (r38, r44): each is a small grille panel with a valve on its wall, facing its room.
    // The engine room's on the west wall (turn 90 faces +x), the corridor's on the east wall beside
    // the bridge door's panel (turn -90 faces -x), the bridge's on its north wall (turn 0 faces -y).
    Mount { id: reactor_screen::VENT_ENGINE_ID, name: reactor_screen::VENT_ENGINE_NAME, at: VENT_AT[0], lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 90.0 },
    Mount { id: reactor_screen::VENT_CORRIDOR_ID, name: reactor_screen::VENT_CORRIDOR_NAME, at: VENT_AT[1], lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: -90.0 },
    // The bridge (r44): the helm on the east wall, the gunnery and the captain's console on the south
    // wall, the jump drive and the bridge's vent on the north wall. TODO(initiator): the layout.
    Mount { id: reactor_screen::HELM_ID, name: reactor_screen::HELM_NAME, at: HELM_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: -90.0 },
    Mount { id: reactor_screen::GUNNERY_ID, name: reactor_screen::GUNNERY_NAME, at: GUNNERY_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 180.0 },
    Mount { id: reactor_screen::JUMP_ID, name: reactor_screen::JUMP_NAME, at: JUMP_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 0.0 },
    Mount { id: reactor_screen::VENT_BRIDGE_ID, name: reactor_screen::VENT_BRIDGE_NAME, at: BRIDGE_VENT_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 0.0 },
    // The bridge door's panel (r44, BD-01): on the corridor's east wall beside the door, north of the corridor's vent.
    Mount { id: reactor_screen::BRIDGE_DOOR_ID, name: reactor_screen::BRIDGE_DOOR_NAME, at: BRIDGE_DOOR_PANEL_AT, lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: -90.0 },
    // The airlock's two panels (r40): the inner one on the corridor's west wall (faces east, into the
    // corridor), the outer one on the airlock's west wall (inside, by the outer door). Both show IN, OUT, CYCLE.
    Mount { id: reactor_screen::AIRLOCK_INNER_ID, name: reactor_screen::AIRLOCK_INNER_NAME, at: AIRLOCK_PANEL_AT[0], lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 90.0 },
    Mount { id: reactor_screen::AIRLOCK_OUTER_ID, name: reactor_screen::AIRLOCK_OUTER_NAME, at: AIRLOCK_PANEL_AT[1], lift: VENT_LIFT, size: SMALL_SCREEN_SIZE, turn: 90.0 },
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
    pub airlock: RoomId,
    pub outside: RoomId,
    /// The bridge (r44): its volume is its size, and its door is a gap from the corridor.
    pub bridge: RoomId,
    pub door: GapId,
    pub bridge_door: GapId,
    /// One hole per wall section (r37's breach, by section in r40): opened and closed each step
    /// from the hull's openness.
    pub breaches: [LeakId; HULL_SECTIONS],
    /// The airlock's inner door: corridor to airlock.
    pub inner: GapId,
    /// The outer door onto the breathable outside (airlock to outside). Open only while the outside is not vacuum.
    pub outer: GapId,
    /// The outer door onto vacuum (airlock to space, a leak). Open only while the outside is vacuum.
    pub outer_leak: LeakId,
    /// The airlock's cycle and doors (`reactor_core::Airlock`).
    pub lock: Airlock,
    /// The outside is vacuum (r40): from the config at start, the dev VACUUM switch after that.
    pub vacuum: bool,
    /// m³ the running cycle moves at its full share: the air at its start (see `request`).
    cycle_air: f64,
    /// Each airlock door's openness, 0..1 (the slab's progress), as the deck has it.
    door_open: [f64; 2],
}

/// m³: the room 10 x 8 x 3.2 m, the corridor 2 x 6 x 3.2 m, the airlock 2 x 2 x 3.2 m; the outside has no end.
const ROOM_VOLUME: f64 = 10.0 * 8.0 * ROOM_HEIGHT;
const CORRIDOR_VOLUME: f64 = 2.0 * 6.0 * ROOM_HEIGHT;
const AIRLOCK_VOLUME: f64 = 2.0 * 2.0 * ROOM_HEIGHT;
const OUTSIDE_VOLUME: f64 = 1.0e6;
/// m³: the bridge 8 x 6 x 3.2 m (r44).
const BRIDGE_VOLUME: f64 = 8.0 * 6.0 * ROOM_HEIGHT;

impl EngineAir {
    pub fn new() -> EngineAir {
        EngineAir::with_tuning(&AirlockTuning::default())
    }

    /// The air at the start: the rooms full, the doors shut. The outside is empty when it is vacuum
    /// (`airlock.outside_vacuum`), and nothing flows to it then but through the outer door's leak.
    pub fn with_tuning(t: &AirlockTuning) -> EngineAir {
        let mut deck = Deck::new(FireTuning::default(), 0x296);
        let engine = deck.add_room(ROOM_VOLUME);
        let corridor = deck.add_room(CORRIDOR_VOLUME);
        let airlock = deck.add_room(AIRLOCK_VOLUME);
        let outside = deck.add_room(OUTSIDE_VOLUME);
        let bridge = deck.add_room(BRIDGE_VOLUME);
        if t.outside_vacuum {
            deck.room_mut(outside).oxygen = 0.0;
        }
        let door = deck.add_gap(engine, corridor, DVec3::new(DOOR_AT[0], DOOR_AT[1], 0.0), DOOR_AREA, 0.0);
        let bridge_door = deck.add_gap(corridor, bridge, DVec3::new(BRIDGE_DOOR_AT[0], BRIDGE_DOOR_AT[1], 0.0), DOOR_AREA, 0.0);
        let inner = deck.add_gap(corridor, airlock, DVec3::new(INNER_DOOR_AT[0], INNER_DOOR_AT[1], 0.0), DOOR_AREA, 0.0);
        let outer = deck.add_gap(airlock, outside, DVec3::new(OUTER_DOOR_AT[0], OUTER_DOOR_AT[1], 0.0), DOOR_AREA, 0.0);
        let outer_leak = deck.add_leak(airlock, DVec3::new(OUTER_DOOR_AT[0], OUTER_DOOR_AT[1], 0.0), 0.0, 0.0);
        // Shut at the start: the area and the openness come from the reactor each step.
        let breaches = SECTION_CENTRES.map(|c| deck.add_leak(engine, DVec3::new(c[0], c[1], 0.0), 0.0, 0.0));
        EngineAir { deck, engine, corridor, airlock, outside, bridge, door, bridge_door, breaches, inner, outer, outer_leak, lock: Airlock::new(t), vacuum: t.outside_vacuum, cycle_air: 0.0, door_open: [0.0; 2] }
    }

    pub fn room_of(&self, place: Place) -> RoomId {
        match place {
            Place::Engine => self.engine,
            Place::Corridor => self.corridor,
            Place::Airlock => self.airlock,
            Place::Bridge => self.bridge,
            Place::Outside => self.outside,
        }
    }

    /// A panel's press (r40): the goal goes to `Airlock::press`. A cycle that starts takes its air
    /// reference now: an outward cycle pumps the airlock's air, an inward one fills the airlock to the
    /// corridor's fraction (the air it takes from the corridor).
    /// The captain's SEAL (r41): shuts the inner door when it is open and no cycle runs (`Airlock::close`).
    pub fn seal_inner(&mut self) -> bool {
        self.lock.close(Door::Inner)
    }

    pub fn request(&mut self, goal: Goal) -> bool {
        let idle = self.lock.cycling().is_none();
        if !self.lock.press(goal) {
            return false;
        }
        if idle && self.lock.cycling().is_some() {
            self.cycle_air = match self.lock.direction() {
                Some(Dir::Out) => self.deck.room(self.airlock).oxygen,
                Some(Dir::In) => self.deck.room(self.airlock).volume * self.deck.room(self.corridor).oxygen_fraction(),
                None => 0.0,
            };
        }
        true
    }

    /// One step of the airlock's cycle: the pump moves its share of the reference air (what fits;
    /// the rest stays where it was), then the cycle's timer runs.
    pub fn airlock_step(&mut self, dt: f64) {
        let share = self.lock.pump_share(dt);
        if share > 0.0 {
            let want = share * self.cycle_air;
            match self.lock.direction() {
                Some(Dir::Out) => self.move_air(self.airlock, self.corridor, want),
                Some(Dir::In) => self.move_air(self.corridor, self.airlock, want),
                None => {}
            }
        }
        self.lock.step(dt);
    }

    /// Moves up to `want` m³ from one room to another: what the source has and the target has room for.
    /// Takes and adds through the deck's `breathe` and `add_oxygen`, so no air is made or lost.
    fn move_air(&mut self, from: RoomId, to: RoomId, want: f64) {
        let (f, t) = (*self.deck.room(from), *self.deck.room(to));
        let free = (t.volume - t.oxygen).max(0.0);
        let m = want.min(f.oxygen).min(free).max(0.0);
        self.deck.breathe(from, m);
        self.deck.add_oxygen(to, m);
    }

    /// An airlock door's openness, 0..1 (the slab's progress). The outer door opens onto the
    /// outside's air, or onto vacuum (a leak) while the outside is vacuum.
    pub fn set_door(&mut self, d: Door, open: f64) {
        match d {
            Door::Inner => {
                self.door_open[0] = open;
                self.deck.set_open(self.inner, open);
            }
            Door::Outer => {
                self.door_open[1] = open;
                self.apply_outer();
            }
        }
    }

    fn apply_outer(&mut self) {
        let open = self.door_open[1];
        let (gap, leak) = if self.vacuum { (0.0, open) } else { (open, 0.0) };
        self.deck.set_open(self.outer, gap);
        self.deck.set_leak(self.outer_leak, DOOR_AREA, leak);
    }

    /// The dev VACUUM switch (r40): the outside becomes empty (vacuum) or full of breathable air. The
    /// doors' openings follow at once.
    pub fn set_vacuum(&mut self, on: bool) {
        self.vacuum = on;
        let outside = self.outside;
        let volume = self.deck.room(outside).volume;
        self.deck.room_mut(outside).oxygen = if on { 0.0 } else { volume };
        self.apply_outer();
    }

    /// The airlock as the panels show it (r40).
    pub fn airlock_news(&self) -> item_core::wire::AirlockNews {
        use item_core::wire::AirlockCycle;
        let cycling = match self.lock.cycling() {
            None => AirlockCycle::Idle,
            Some(Goal::Door(Door::Inner)) => AirlockCycle::Inner,
            Some(Goal::Door(Door::Outer)) => AirlockCycle::Outer,
            Some(Goal::Hold) => AirlockCycle::Hold,
        };
        item_core::wire::AirlockNews { inner_open: self.lock.is_open(Door::Inner), outer_open: self.lock.is_open(Door::Outer), cycling, progress: self.lock.progress() }
    }

    /// The generator's air (m³) through the three vents (r38, r44): `open[0]` the engine room's, `open[1]`
    /// the corridor's, `open[2]` the bridge's. Each open vent gets its room's volume share
    /// (`fire_core::split_by_volume`, Barotrauma `OxygenGenerator.UpdateVents`); with all shut the air goes nowhere.
    pub fn add_oxygen_through_vents(&mut self, m3: f64, open: [bool; 3]) {
        let (e, c, b) = (self.engine, self.corridor, self.bridge);
        let shares = split_by_volume(m3, &[(self.deck.room(e).volume, open[0]), (self.deck.room(c).volume, open[1]), (self.deck.room(b).volume, open[2])]);
        self.deck.add_oxygen(e, shares[0]);
        self.deck.add_oxygen(c, shares[1]);
        self.deck.add_oxygen(b, shares[2]);
    }

    /// The air as a save holds it (r42): each room's oxygen in m³ and the airlock.
    pub fn to_save(&self) -> AirSave {
        let rooms = [self.engine, self.corridor, self.airlock, self.outside, self.bridge];
        AirSave {
            oxygen: rooms.map(|r| self.deck.room(r).oxygen),
            vacuum: self.vacuum,
            lock: self.lock.to_save(),
            cycle_air: self.cycle_air,
            door_open: self.door_open,
        }
    }

    /// The air of a save, the rooms and doors as they were. Breaches and the outer leak follow the
    /// hull and the vacuum at the next step, as they do in a new room.
    pub fn from_save(t: &AirlockTuning, s: &AirSave) -> EngineAir {
        let mut a = EngineAir::with_tuning(t);
        for (r, ox) in [a.engine, a.corridor, a.airlock, a.outside, a.bridge].into_iter().zip(s.oxygen) {
            a.deck.room_mut(r).oxygen = ox;
        }
        a.vacuum = s.vacuum;
        a.lock = Airlock::from_save(t, &s.lock);
        a.cycle_air = s.cycle_air;
        a.door_open = s.door_open;
        a.deck.set_open(a.inner, s.door_open[0]);
        a.apply_outer();
        a
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

/// The hull breach (r37, by wall section in r40): a dark patch on each section, where the meltdown
/// blast or a hit opens the wall. Each patch is a dev box so the wrench can aim at it. Section 0 is
/// the south-west half of the south wall, where r37's breach was.
pub const BREACH_IDS: [&str; HULL_SECTIONS] = ["engine_breach_0", "engine_breach_1", "engine_breach_2", "engine_breach_3", "engine_breach_4", "engine_breach_5", "engine_breach_6", "engine_breach_7"];
/// The same patch on each section's outer face (r43), so the wrench aims at it from outside. Same
/// section index as `BREACH_IDS`.
pub const BREACH_OUT_IDS: [&str; HULL_SECTIONS] = ["engine_breach_out_0", "engine_breach_out_1", "engine_breach_out_2", "engine_breach_out_3", "engine_breach_out_4", "engine_breach_out_5", "engine_breach_out_6", "engine_breach_out_7"];
/// A patch's size (m): along the south and north walls, and along the east and west walls.
const BREACH_SIZE_X: [f64; 3] = [1.0, 1.2, 0.1];
const BREACH_SIZE_Z: [f64; 3] = [0.1, 1.2, 1.0];

/// The outer face of section `i` (r43): its inside centre pushed out by the wall thickness, along the
/// wall's outward normal. Sections 0 and 1 are on the south wall, 2 and 3 on the east, 4 and 5 on
/// the north, 6 and 7 on the west (`SECTION_CENTRES`).
fn outside_at(i: usize) -> [f64; 2] {
    let [x, y] = SECTION_CENTRES[i];
    if y == WALLS_MIN[1] {
        [x, y - WALL]
    } else if x == WALLS_MAX[0] {
        [x + WALL, y]
    } else if y == WALLS_MAX[1] {
        [x, y + WALL]
    } else {
        [x - WALL, y]
    }
}

/// The outside patch of section `i` (r43): the inside patch's size, on the outer face.
fn outside_part(i: usize) -> Part {
    let size = if matches!(i, 0 | 1 | 4 | 5) { BREACH_SIZE_X } else { BREACH_SIZE_Z };
    Part { id: BREACH_OUT_IDS[i], at: outside_at(i), size, lift: 0.0, colour: BREACH_COLOUR }
}
const BREACH_COLOUR: [f32; 3] = [0.05, 0.05, 0.06];

const WALL_COLOUR: [f32; 3] = [0.42, 0.44, 0.47];
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
    Part { id: BREACH_IDS[0], at: SECTION_CENTRES[0], size: BREACH_SIZE_X, lift: 0.0, colour: BREACH_COLOUR },
    Part { id: BREACH_IDS[1], at: SECTION_CENTRES[1], size: BREACH_SIZE_X, lift: 0.0, colour: BREACH_COLOUR },
    Part { id: BREACH_IDS[2], at: SECTION_CENTRES[2], size: BREACH_SIZE_Z, lift: 0.0, colour: BREACH_COLOUR },
    Part { id: BREACH_IDS[3], at: SECTION_CENTRES[3], size: BREACH_SIZE_Z, lift: 0.0, colour: BREACH_COLOUR },
    Part { id: BREACH_IDS[4], at: SECTION_CENTRES[4], size: BREACH_SIZE_X, lift: 0.0, colour: BREACH_COLOUR },
    Part { id: BREACH_IDS[5], at: SECTION_CENTRES[5], size: BREACH_SIZE_X, lift: 0.0, colour: BREACH_COLOUR },
    Part { id: BREACH_IDS[6], at: SECTION_CENTRES[6], size: BREACH_SIZE_Z, lift: 0.0, colour: BREACH_COLOUR },
    Part { id: BREACH_IDS[7], at: SECTION_CENTRES[7], size: BREACH_SIZE_Z, lift: 0.0, colour: BREACH_COLOUR },
];

/// The airlock's walls (r40): the room's west and east walls, from the inner door's line to the outer
/// door's. The doors close the ends (`airlock.rs`).
const AIRLOCK_WALLS: &[Part] = &[
    Part { id: "engine_airlock_west", at: [-15.15, -3.7], size: [WALL, ROOM_HEIGHT, 2.0], lift: 0.0, colour: WALL_COLOUR },
    Part { id: "engine_airlock_east", at: [-12.85, -3.7], size: [WALL, ROOM_HEIGHT, 2.0], lift: 0.0, colour: WALL_COLOUR },
];

pub(super) fn span(lift: f64, height: f64) -> (f64, f64) {
    if lift > 0.0 { (lift, lift + height) } else { (-SINK, height) }
}

/// A door slab: its shut centre, the slide's direction (east, m) and how far it has slid (0..1).
#[derive(Component, Clone, Debug)]
pub struct Slab {
    pub shut: DVec3,
    pub slide: DVec3,
    pub progress: f64,
}

/// The bridge door's slab (r44): its shut centre, the vector it sinks along when open, and how far it has
/// sunk (0 shut, 1 open). A separate component so the engine door's walker pick does not take it.
#[derive(Component, Clone, Debug)]
pub struct BridgeSlab {
    pub shut: DVec3,
    pub sink: DVec3,
    pub progress: f64,
}

pub fn walker_place(area: &DevArea, planet: &PlanetRes, p: DVec3) -> Option<Place> {
    let f = area.frame?;
    Some(place_of(area_of(&f, planet, p)))
}

pub fn plugin(app: &mut App) {
    let tuning = Tuning::load(&crate::content_files("gameplay")).unwrap_or_else(|e| panic!("engine room: {e:?}"));
    app.init_resource::<ShipAlarm>().init_resource::<EngineDoor>().init_resource::<BridgeDoor>().insert_resource(EngineAir::with_tuning(&tuning.airlock));
    app.add_systems(Update, spawn_room.after(super::spawn_devices).in_set(crate::phases::Frame::World));
    app.init_resource::<BodyVacuum>();
    app.add_systems(FixedUpdate, (room_state.after(crate::items_net::items_net_in), room_step, bridge_step, super::airlock::airlock_step, breath_step).chain().in_set(crate::phases::Fx::Input));
}

/// Window only: the lights of the room and the corridor follow the grid's lights factor
/// (`reactor_core::LightState`: Barotrauma's flicker and fade, `lights.rs`), the red emergency
/// lamps come on below `EMERGENCY_BELOW`, and the alarm's pulse. The red alarm lights stay on
/// emergency power.
pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (spawn_lights, (room_lights, alarm_lights, emergency_lamps).after(spawn_lights)).after(super::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(Update, breach_decal.after(super::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(Update, (spawn_alarm_lamps, alarm_lamps).chain().after(super::spawn_devices).in_set(crate::phases::Frame::World));
}

/// The room alarm's red lamp (r40, #165): one in the engine room, one in the corridor and one in the bridge (r44), at
/// `ALARM_LAMP_HEIGHT`. Copies the reactor's critical lamp (`effects::critical_lamp_sync`): a light
/// and a lens that show while the room's alarm is on. The corridor is open to the sky, so its lamp
/// hangs on a mast at the same height (TODO(initiator)).
#[derive(Component)]
struct RoomAlarmLamp {
    room: usize,
}

#[derive(Component)]
struct RoomAlarmLens {
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

fn spawn_alarm_lamps(mut commands: Commands, planet: Res<PlanetRes>, area: Res<DevArea>, mut done: Local<Option<warp_core::PlanetId>>, mut made: Local<Vec<Entity>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>) {
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

fn alarm_lamps(time: Res<Time>, items: Res<Items>, mut lights: Query<(&RoomAlarmLamp, &mut PointLight)>, mut lens: Query<(&RoomAlarmLens, &mut Visibility)>) {
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

/// A section's dark patch on the wall (r37, by section in r40): shown while that section leaks,
/// hidden while it holds. The wrench aims at it either way (its collider is there).
#[derive(Component)]
struct BreachDecal(usize);

fn breach_decal(items: Res<Items>, mut decals: Query<(&BreachDecal, &mut Visibility)>) {
    for (decal, mut v) in &mut decals {
        let leaking = items.reactor.is_some_and(|r| openness(r.breaches[decal.0]) > 0.0);
        let want = if leaking { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

pub(super) fn room_state(mut items: ResMut<Items>, mut alarm: ResMut<ShipAlarm>, mut door: ResMut<EngineDoor>, mut bridge: ResMut<BridgeDoor>) {
    let asks = std::mem::take(&mut items.room_asks);
    match items.role {
        Role::Client => {
            for a in asks {
                items.net_out.push(NetOut::Ask(a));
            }
            door.open = items.room.door_open;
            bridge.open = items.room.bridge_door;
            alarm.level = items.room.alarm;
        }
        Role::Host | Role::Local => {
            for a in asks {
                match a {
                    RoomAsk::ToggleDoor => door.open = !door.open,
                    // The captain's CP-01 (r41): the press sets the door, as a signal sets Barotrauma's door.
                    RoomAsk::SetDoor(open) => door.open = open,
                    RoomAsk::CycleAlarm => alarm.level = alarm.level.next(),
                    // The bridge door (r44): its panel toggles it, CP-01 sets it.
                    RoomAsk::ToggleBridgeDoor => bridge.open = !bridge.open,
                    RoomAsk::SetBridgeDoor(open) => bridge.open = open,
                }
            }
            let now = RoomNews { door_open: door.open, alarm: alarm.level, bridge_door: bridge.open };
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

/// Every step (r44): the bridge door's slab follows the door's state and sinks into the floor, and its
/// gap to the corridor opens by the slab's progress, as the engine door's does.
pub(super) fn bridge_step(time: Res<Time>, bridge: Res<BridgeDoor>, mut air: ResMut<EngineAir>, mut slabs: Query<(&mut BridgeSlab, &mut Position)>) {
    let dt = time.delta_secs_f64();
    let goal = if bridge.open { 1.0 } else { 0.0 };
    let step = dt / DOOR_TIME;
    for (mut slab, mut pos) in &mut slabs {
        slab.progress = if slab.progress < goal { (slab.progress + step).min(goal) } else { (slab.progress - step).max(goal) };
        pos.0 = slab.shut + slab.sink * slab.progress;
        let gap = air.bridge_door;
        air.deck.set_open(gap, slab.progress);
    }
}

/// Host only: every living player breathes the air of the room they stand in, `breath_m3_per_s`
/// a second (`reactor_core` `RoomTuning`). Barotrauma `Character.cs:3835-3848` takes
/// `Hull.OxygenConsumptionSpeed` (`Hull.cs:124`, 700 px²/s) times dt from the hull the character is
/// in; the hull's area is in px (`Hull.cs:378-381`, 100 px = 1 m, `Physics.cs:22`), which is how the
/// rate in m³ comes out. Not ported: Barotrauma's "only while the body's own oxygen is lower"
/// (the body has no oxygen store here). The outside has no end, so nobody drains it. Unconscious
/// players breathe too: every body that is not dead counts. A body in its mask (r38) breathes
/// the mask while its tank has air (`dev_area/mask.rs`, Barotrauma `Character.cs:3851` with `UseHullOxygen` false).
/// Host: the bodies standing in vacuum this step (r41): the room they stand in has a pressure below
/// `health_core::vacuum::VACUUM_BELOW`. `breath_step` sets it, the health step reads it. The mask's
/// air is not the room's, so a masked body in vacuum is in it still.
#[derive(Resource, Default, Debug)]
pub struct BodyVacuum(pub BTreeSet<PlayerId>);

pub(super) fn breath_step(time: Res<Time>, items: Res<Items>, cfg: Res<super::reactor::ReactorConfig>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, health: Option<Res<crate::health::HealthState>>, masked: Option<Res<super::mask::MaskAir>>, mut vacuum: ResMut<BodyVacuum>, mut air: ResMut<EngineAir>) {
    if items.role == Role::Client {
        return;
    }
    vacuum.0.clear();
    let Some(area) = area.as_deref() else { return };
    let dt = time.delta_secs_f64();
    let rate = cfg.0.room.breath_m3_per_s * dt;
    for (id, p) in crate::health::walker_points(&players, &remotes) {
        let Some(q) = crate::health::area_pos(Some(area), &planet, Some(p)) else { continue };
        let room = air.room_of(place_of(q));
        if health_core::vacuum::in_vacuum(air.deck.room(room).pressure()) {
            vacuum.0.insert(id);
        }
        if health.as_ref().and_then(|h| h.bodies.get(&id)).is_some_and(|b| b.health.state() == health_core::Life::Dead) {
            continue;
        }
        // A body breathing from its mask's tank (r38) takes no room air; an empty tank breathes the room.
        if masked.as_ref().is_some_and(|m| m.0.contains_key(&id)) {
            continue;
        }
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
        Source::OxygenGenerator => OXYGEN_GENERATOR_AT,
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_room(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, door: Res<EngineDoor>, bridge: Res<BridgeDoor>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    let outside: Vec<Part> = (0..HULL_SECTIONS).map(outside_part).collect();
    for p in PARTS.iter().chain(AIRLOCK_WALLS.iter()).chain(MACHINES.iter()).chain(outside.iter()) {
        let (ground, up) = frame.ground(&planet, p.at[0], p.at[1], 0.0);
        let rot = frame.rotation(up, 0.0);
        let (bottom, top) = span(p.lift, p.size[1]);
        let centre = ground + up * ((bottom + top) * 0.5);
        let size = DVec3::new(p.size[0], top - bottom, p.size[2]);
        let mut e = commands.spawn((
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            Position(centre),
            Rotation(rot),
            CollisionLayers::new(Layer::World, LayerMask::ALL),
            DevBox { id: p.id.into(), colour: p.colour, size },
        ));
        if let Some(i) = BREACH_IDS.iter().position(|&id| id == p.id).or_else(|| BREACH_OUT_IDS.iter().position(|&id| id == p.id)) {
            e.insert(BreachDecal(i));
        }
        area.entities.push(e.id());
    }
    let (ground, up) = frame.ground(&planet, REACTOR_SCREEN_AT[0], REACTOR_SCREEN_AT[1], 0.0);
    let rot = frame.rotation(up, 180.0);
    let centre = ground + up * (REACTOR_SCREEN_LIFT + READOUT_SIZE[1] * 0.5);
    let e = commands.spawn(DisplayPanel { id: reactor_screen::READOUT_ID, name: reactor_screen::READOUT_NAME.into(), size_m: DVec2::from(READOUT_SIZE), centre, rot }).id();
    area.entities.push(e);
    for m in SCREENS.iter() {
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
    // The bridge door (r44): a slab in the corridor's east wall's opening, as the engine door's.
    let d = BRIDGE_DOOR_SIZE;
    let (ground, up) = frame.ground(&planet, BRIDGE_DOOR_AT[0], BRIDGE_DOOR_AT[1], 0.0);
    let rot = frame.rotation(up, 0.0);
    let (bottom, top) = span(0.0, d[1]);
    let shut = ground + up * ((bottom + top) * 0.5);
    let size = DVec3::new(d[0], top - bottom, d[2]);
    let progress = if bridge.open { 1.0 } else { 0.0 };
    let sink = -up * d[1];
    let e = commands
        .spawn((
            RigidBody::Kinematic,
            Collider::cuboid(size.x, size.y, size.z),
            Position(shut + sink * progress),
            Rotation(rot),
            CollisionLayers::new(Layer::World, LayerMask::ALL),
            DevBox { id: BRIDGE_DOOR.into(), colour: [0.55, 0.42, 0.2], size },
            BridgeSlab { shut, sink, progress },
        ))
        .id();
    area.entities.push(e);
    super::airlock::spawn_doors(&mut commands, &frame, &planet, &mut area);
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
    let lamps = [([-16.5, -15.5], 2.9, false), ([-11.5, -15.5], 2.9, false), ([-14.0, -7.7], 2.9, false), ([-14.0, -15.0], 2.6, true), ([-14.0, -8.6], 2.6, true), (AIRLOCK.centre, 2.9, false), (BRIDGE.centre, 2.9, false)];
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
    use reactor_core::{AirlockTuning, Door, Goal};

    /// r37: the breach's hole loses air only while it is open; a mended wall keeps what it has.
    #[test]
    fn a_breach_loses_air_only_while_it_is_open() {
        let mut a = EngineAir::new();
        let full = a.deck.room(a.engine).oxygen;
        for _ in 0..60 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert_eq!(a.deck.room(a.engine).oxygen, full, "a shut breach keeps the air");
        a.deck.set_leak(a.breaches[0], 0.5, 1.0);
        for _ in 0..300 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        let leaking = a.deck.room(a.engine).oxygen;
        assert!(leaking < full * 0.99, "an open breach lets the air out");
        assert!((a.deck.lost_to_space() - (full - leaking)).abs() < 1e-9, "the lost air is the air that left");
        a.deck.set_leak(a.breaches[0], 0.5, 0.0);
        for _ in 0..300 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert_eq!(a.deck.room(a.engine).oxygen, leaking, "a mended breach stops the loss");
    }

    /// r38: the generator's air goes through the vents by volume; a shut vent sends it all to the other room.
    #[test]
    fn a_shut_engine_vent_sends_the_generators_air_to_the_corridor() {
        let mut a = EngineAir::new();
        a.deck.breathe(a.engine, 5.0);
        a.deck.breathe(a.corridor, 5.0);
        let (e0, c0) = (a.deck.room(a.engine).oxygen, a.deck.room(a.corridor).oxygen);
        a.add_oxygen_through_vents(0.5, [false, true, false]);
        assert_eq!(a.deck.room(a.engine).oxygen, e0, "the shut engine vent takes nothing");
        assert!((a.deck.room(a.corridor).oxygen - c0 - 0.5).abs() < 1e-9, "the corridor gets all of it");
        a.add_oxygen_through_vents(0.5, [true, true, true]);
        let shared = a.deck.room(a.engine).oxygen - e0;
        let expected = 0.5 * ROOM_VOLUME / (ROOM_VOLUME + CORRIDOR_VOLUME + BRIDGE_VOLUME);
        assert!((shared - expected).abs() < 1e-9, "all open: the engine room takes its volume share ({shared})");
    }

    /// r44: the bridge is a fourth room of the deck with its own volume; its door is a gap from the
    /// corridor that opens with the slab, and the air still adds up while it flows.
    #[test]
    fn the_bridge_shares_air_through_its_door_and_the_air_adds_up() {
        let mut a = EngineAir::new();
        assert!((a.deck.room(a.bridge).volume - BRIDGE_VOLUME).abs() < 1e-12, "the bridge's volume is its size");
        assert_eq!(a.room_of(place_of(BRIDGE.centre)), a.bridge, "the bridge's zone is the bridge's room");
        a.deck.breathe(a.corridor, 5.0);
        let total = a.deck.total_oxygen();
        let full = a.deck.room(a.bridge).oxygen;
        a.deck.set_open(a.bridge_door, 0.0);
        for _ in 0..60 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert_eq!(a.deck.room(a.bridge).oxygen, full, "a shut bridge door keeps the bridge's air");
        a.deck.set_open(a.bridge_door, 1.0);
        for _ in 0..300 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert!(a.deck.room(a.bridge).oxygen < full * 0.99, "an open bridge door lets the bridge's air flow to the thinner corridor");
        assert!((a.deck.total_oxygen() - total).abs() < 1e-9, "the air is conserved");
    }

    /// Runs the cycle and the air for `secs` at 30 steps a second, with the door's openness fixed.
    fn cycle(a: &mut EngineAir, secs: f64) {
        for _ in 0..(secs * 30.0).round() as usize {
            a.airlock_step(1.0 / 30.0);
            a.deck.exchange_air(1.0 / 30.0);
        }
    }

    /// r40: an outward cycle pumps 95 % of the airlock's air back into the corridor; the rest stays.
    #[test]
    fn the_airlock_pumps_its_air_back_into_the_corridor() {
        let mut a = EngineAir::new();
        a.deck.breathe(a.corridor, 19.2);
        let total = a.deck.total_oxygen();
        let (lock, corridor) = (a.deck.room(a.airlock).oxygen, a.deck.room(a.corridor).oxygen);
        assert!(a.request(Goal::Door(Door::Outer)), "the outer door's cycle starts");
        cycle(&mut a, 6.0);
        assert!(a.lock.is_open(Door::Outer), "the outer door opens at the cycle's end");
        let moved = a.deck.room(a.corridor).oxygen - corridor;
        assert!((moved - 0.95 * lock).abs() < 1e-6, "the corridor gets 95 % of the airlock's air ({moved})");
        assert!((a.deck.room(a.airlock).oxygen - 0.05 * lock).abs() < 1e-6, "5 % stays in the airlock");
        assert!((a.deck.total_oxygen() - total).abs() < 1e-9, "the air is conserved through the cycle");
    }

    /// r40: the inward cycle fills the airlock from the corridor, and the air still adds up.
    #[test]
    fn the_inward_cycle_fills_the_airlock_from_the_corridor() {
        let mut a = EngineAir::new();
        a.deck.breathe(a.airlock, 12.0);
        let total = a.deck.total_oxygen();
        let before = a.deck.room(a.airlock).oxygen;
        assert!(a.request(Goal::Door(Door::Inner)));
        cycle(&mut a, 6.0);
        assert!(a.lock.is_open(Door::Inner));
        assert!(a.deck.room(a.airlock).oxygen > before + 10.0, "the airlock is filled to the corridor's pressure");
        assert!((a.deck.total_oxygen() - total).abs() < 1e-9, "the air came out of the corridor");
    }

    /// r40: with the outside vacuum, the outer door opens onto space: the outside stays empty, the air
    /// that leaves the airlock is lost to space, and the total adds up.
    #[test]
    fn the_outer_door_opens_onto_vacuum() {
        let mut a = EngineAir::new();
        assert!(a.vacuum);
        assert_eq!(a.deck.room(a.outside).oxygen, 0.0, "the outside starts empty");
        a.deck.breathe(a.corridor, 19.2);
        assert!(a.request(Goal::Door(Door::Outer)));
        cycle(&mut a, 6.0);
        let total = a.deck.total_oxygen() + a.deck.lost_to_space();
        for _ in 0..90 {
            a.set_door(Door::Outer, 1.0);
            a.airlock_step(1.0 / 30.0);
            a.deck.exchange_air(1.0 / 30.0);
            assert_eq!(a.deck.room(a.outside).oxygen, 0.0, "the outside stays vacuum");
        }
        assert!(a.deck.room(a.airlock).oxygen < 0.01 * a.deck.room(a.airlock).volume, "the airlock empties into space");
        assert!((a.deck.total_oxygen() + a.deck.lost_to_space() - total).abs() < 1e-9, "what left went to space");
        assert_eq!(a.oxygen_at(AIRLOCK.centre), a.deck.room(a.airlock).oxygen_fraction());
    }

    /// r40: with the outside breathable (the config's false, the dev VACUUM off), the outer door is a
    /// gap to a full outside: the airlock takes air from it.
    #[test]
    fn a_breathable_outside_gives_air_through_the_outer_door() {
        let mut a = EngineAir::with_tuning(&AirlockTuning { outside_vacuum: false, ..AirlockTuning::default() });
        a.deck.breathe(a.airlock, 12.0);
        assert!((a.deck.room(a.outside).oxygen_fraction() - 1.0).abs() < 1e-12);
        a.set_door(Door::Outer, 1.0);
        for _ in 0..300 {
            a.deck.exchange_air(1.0 / 30.0);
        }
        assert!(a.deck.room(a.airlock).oxygen_fraction() > 0.9, "the airlock takes the outside's air");
        a.set_vacuum(true);
        assert_eq!(a.deck.room(a.outside).oxygen, 0.0, "the dev VACUUM empties the outside");
    }

    /// r40: a press while the cycle runs is refused, and a press stops it.
    #[test]
    fn a_press_during_a_cycle_is_refused_and_stops_it() {
        let mut a = EngineAir::new();
        assert!(a.request(Goal::Door(Door::Inner)));
        assert!(!a.request(Goal::Door(Door::Outer)), "another door's press during a cycle is refused");
        assert!(a.request(Goal::Door(Door::Inner)), "the same press stops the cycle");
        assert_eq!(a.lock.cycling(), None);
        assert!(!a.lock.is_open(Door::Inner) && !a.lock.is_open(Door::Outer));
    }

    #[test]
    fn the_deck_has_the_room_full_of_air_and_the_door_shut() {
        let mut a = EngineAir::new();
        assert!((a.oxygen_at(ROOM.centre) - 1.0).abs() < 1e-9);
        assert!((a.oxygen_at([0.0, -12.0]) - 0.0).abs() < 1e-9, "the outside is vacuum (r40)");
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
