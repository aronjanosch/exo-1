//! The ship snapshot the captain's overview and ship state displays read. Taken from PULSAR's
//! ship status screen: four system bars (engineering, weapons, life support, science) with a
//! condition from 0 to 20 and a fire mark, shields, hull, oxygen with its change, reactor
//! status; the room list, the clock and the mission line are ours.
//!
//! `ShipStatus` is plain data; the glue fills it from the real systems later (the reactor and fire
//! crates belong to the engineer lane, so nothing here depends on them). Until then
//! `PlaceholderStatus` makes a believable ship for the dev area: seeded values that wobble slowly,
//! and forced states (reactor hot, fire, breach, a system down) to test the panels with.
use std::f64::consts::TAU;

use gameplay_core::TextKey;
use gameplay_core::rng::Rng;
use serde::{Deserialize, Serialize};

use crate::RoomId;

/// A system bar's full condition; 0 is out of order.
pub const SYSTEM_MAX: u8 = 20;

/// Seconds the crew has in a star system (`ship.md`, "System clock and escape window").
pub const SYSTEM_CLOCK_S: f64 = 3600.0;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShipSystem {
    Engineering,
    Weapons,
    LifeSupport,
    Science,
}

impl ShipSystem {
    pub const ALL: [ShipSystem; 4] = [ShipSystem::Engineering, ShipSystem::Weapons, ShipSystem::LifeSupport, ShipSystem::Science];

    pub fn id(self) -> &'static str {
        match self {
            ShipSystem::Engineering => "engineering",
            ShipSystem::Weapons => "weapons",
            ShipSystem::LifeSupport => "life_support",
            ShipSystem::Science => "science",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SystemBar {
    pub system: ShipSystem,
    /// 0..=`SYSTEM_MAX`; none when the ship has no such system (the display shows "(-)").
    pub condition: Option<u8>,
    pub fire: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gauge {
    pub value: f64,
    pub max: f64,
}

impl Gauge {
    /// 0..1.
    pub fn fraction(&self) -> f64 {
        if self.max > 0.0 { (self.value / self.max).clamp(0.0, 1.0) } else { 0.0 }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Oxygen {
    /// Ship average, 0..1.
    pub level: f64,
    /// Change per second; breathing and fires pull it down.
    pub delta_per_s: f64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempBand {
    Cold,
    Normal,
    Hot,
    Critical,
}

impl TempBand {
    /// The band of a reactor temperature 0..1. TODO(initiator): bounds, with the engineer lane.
    pub fn of(temp: f64) -> TempBand {
        match temp {
            t if t < 0.25 => TempBand::Cold,
            t if t < 0.7 => TempBand::Normal,
            t if t < 0.9 => TempBand::Hot,
            _ => TempBand::Critical,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactorMode {
    Off,
    Running,
    Overheating,
    /// The safety shut it down.
    Shutdown,
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReactorStatus {
    /// 0..1.
    pub temp: f64,
    pub band: TempBand,
    /// 0..1.
    pub fuel: f64,
    pub mode: ReactorMode,
    /// What the reactor gives and what the grid asks, kW; none where no real reactor feeds it.
    pub output_kw: Option<f64>,
    pub load_kw: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoomStatus {
    pub id: RoomId,
    /// `room.<id>`.
    pub name: TextKey,
    /// The system that lives in the room, whose bar its fire marks.
    pub system: Option<ShipSystem>,
    /// 0..1.
    pub condition: f64,
    /// 0..1.
    pub oxygen: f64,
    pub fire: bool,
    /// A hull breach: the room loses air (or floods, on a planet with water).
    pub breach: bool,
    /// Crew members in the room.
    pub crew: u32,
}

/// The tracked mission in one line (from `mission_view`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MissionLine {
    pub title: TextKey,
    pub done: u32,
    pub of: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShipStatus {
    /// In the order of `ShipSystem::ALL`.
    pub systems: [SystemBar; 4],
    pub shields: Gauge,
    pub hull: Gauge,
    pub oxygen: Oxygen,
    pub reactor: ReactorStatus,
    pub rooms: Vec<RoomStatus>,
    /// Seconds left in this star system; none where no clock runs.
    pub clock_left_s: Option<f64>,
    pub mission: Option<MissionLine>,
}

impl ShipStatus {
    pub fn system(&self, s: ShipSystem) -> &SystemBar {
        &self.systems[ShipSystem::ALL.iter().position(|x| *x == s).unwrap()]
    }

    pub fn room(&self, id: &RoomId) -> Option<&RoomStatus> {
        self.rooms.iter().find(|r| r.id == *id)
    }
}

/// A state the dev area forces onto the placeholder ship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Forced {
    ReactorHot,
    Fire(RoomId),
    Breach(RoomId),
    SystemDown(ShipSystem),
}

/// Rooms of the placeholder ship, with their system and crew count.
/// TODO(initiator): room ids follow the ship greybox once it has names.
const ROOMS: [(&str, Option<ShipSystem>, u32); 6] = [
    ("bridge", None, 1),
    ("engine_room", Some(ShipSystem::Engineering), 1),
    ("weapons_bay", Some(ShipSystem::Weapons), 1),
    ("life_support", Some(ShipSystem::LifeSupport), 0),
    ("lab", Some(ShipSystem::Science), 1),
    ("cargo_hold", None, 0),
];

/// Wobbling values: shields, hull, oxygen, reactor temp, fuel, four systems, six room
/// conditions, six room oxygen levels.
const WAVES: usize = 21;

/// A seeded fake ship for the dev area. The same seed and the same steps give the same status.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaceholderStatus {
    t: f64,
    /// Phase (radians) and period (seconds) per wobbling value.
    waves: [(f64, f64); WAVES],
    forced: Vec<Forced>,
}

impl PlaceholderStatus {
    pub fn new(seed: u64) -> PlaceholderStatus {
        let mut rng = Rng::new(seed);
        let waves = std::array::from_fn(|_| (TAU * rng.unit(), 20.0 + 40.0 * rng.unit()));
        PlaceholderStatus { t: 0.0, waves, forced: Vec::new() }
    }

    pub fn step(&mut self, dt: f64) {
        self.t += dt;
    }

    pub fn force(&mut self, f: Forced) {
        if !self.forced.contains(&f) {
            self.forced.push(f);
        }
    }

    pub fn release(&mut self, f: &Forced) {
        self.forced.retain(|x| x != f);
    }

    pub fn clear(&mut self) {
        self.forced.clear();
    }

    pub fn forced(&self) -> &[Forced] {
        &self.forced
    }

    /// `base + amp × sin(...)` of wave `i`.
    fn wave(&self, i: usize, base: f64, amp: f64) -> f64 {
        let (phase, period) = self.waves[i];
        base + amp * (phase + TAU * self.t / period).sin()
    }

    /// The wave's change per second.
    fn wave_rate(&self, i: usize, amp: f64) -> f64 {
        let (phase, period) = self.waves[i];
        amp * TAU / period * (phase + TAU * self.t / period).cos()
    }

    /// The fake ship now. Placeholder values (TODO(initiator), they only fill the panels):
    /// shields 80 ± 10 of 100, hull 92 ± 3 of 100, oxygen 0.96 ± 0.02, reactor temperature
    /// 0.45 ± 0.1, fuel 0.8 ± 0.02, systems 17 ± 3 of 20, rooms at condition 0.9 ± 0.08 and
    /// oxygen 0.96 ± 0.02. A fire costs its room 0.3 condition and its system 6 points, sets the
    /// room's oxygen to 0.6 and the ship's oxygen change 0.01/s lower; a breach costs 15 hull,
    /// sets the room's oxygen to 0.3 and the change 0.02/s lower; a hot reactor sits at 0.95.
    pub fn status(&self) -> ShipStatus {
        let mut s = self.status_unforced();
        self.apply_forced(&mut s);
        s
    }

    /// The wobbling ship without the forced states: the glue lays real data over it, then
    /// `apply_forced`, so a forced state wins while it is set.
    pub fn status_unforced(&self) -> ShipStatus {
        let rooms: Vec<RoomStatus> = ROOMS
            .iter()
            .enumerate()
            .map(|(i, (id, system, crew))| RoomStatus {
                id: RoomId::new(*id),
                name: TextKey::new(format!("room.{id}")),
                system: *system,
                condition: self.wave(9 + i, 0.9, 0.08),
                oxygen: self.wave(15 + i, 0.96, 0.02),
                fire: false,
                breach: false,
                crew: *crew,
            })
            .collect();
        let systems = std::array::from_fn(|i| SystemBar { system: ShipSystem::ALL[i], condition: Some(self.wave(5 + i, 17.0, 3.0).round().clamp(0.0, SYSTEM_MAX as f64) as u8), fire: false });
        let temp = self.wave(3, 0.45, 0.1);
        ShipStatus {
            systems,
            shields: Gauge { value: self.wave(0, 80.0, 10.0), max: 100.0 },
            hull: Gauge { value: self.wave(1, 92.0, 3.0), max: 100.0 },
            oxygen: Oxygen { level: self.wave(2, 0.96, 0.02), delta_per_s: self.wave_rate(2, 0.02) },
            reactor: ReactorStatus { temp, band: TempBand::of(temp), fuel: self.wave(4, 0.8, 0.02), mode: ReactorMode::Running, output_kw: None, load_kw: None },
            rooms,
            clock_left_s: Some((SYSTEM_CLOCK_S - self.t).max(0.0)),
            mission: None,
        }
    }

    /// Lays the forced states over a status (see `status` for what each does). Rooms are found by
    /// id; a forced room the status does not list changes nothing.
    pub fn apply_forced(&self, s: &mut ShipStatus) {
        let fire = |id: &RoomId| self.forced.iter().any(|f| matches!(f, Forced::Fire(r) if r == id));
        let breach = |id: &RoomId| self.forced.iter().any(|f| matches!(f, Forced::Breach(r) if r == id));
        let (mut fires, mut breaches) = (0.0, 0.0);
        let mut burning_systems = Vec::new();
        for r in s.rooms.iter_mut() {
            if fire(&r.id) {
                r.condition -= 0.3;
                r.oxygen = 0.6;
                r.fire = true;
                fires += 1.0;
                burning_systems.extend(r.system);
            }
            if breach(&r.id) {
                r.oxygen = 0.3;
                r.breach = true;
                breaches += 1.0;
            }
        }
        for bar in s.systems.iter_mut() {
            if burning_systems.contains(&bar.system) {
                bar.fire = true;
                bar.condition = bar.condition.map(|c| (c as i32 - 6).clamp(0, SYSTEM_MAX as i32) as u8);
            }
            if self.forced.contains(&Forced::SystemDown(bar.system)) {
                bar.condition = Some(0);
            }
        }
        s.oxygen.delta_per_s -= 0.01 * fires + 0.02 * breaches;
        s.hull.value -= 15.0 * breaches;
        if self.forced.contains(&Forced::ReactorHot) {
            s.reactor.temp = 0.95;
            s.reactor.band = TempBand::of(0.95);
            s.reactor.mode = ReactorMode::Overheating;
        }
    }
}
