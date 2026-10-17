//! The ship snapshot the captain's overview and ship state displays read. Taken from PULSAR's
//! ship status screen: four system bars (engineering, weapons, life support, science) with a
//! condition from 0 to 20 and a fire mark, shields, hull, oxygen with its change, reactor
//! status; the room list, the clock and the mission line are ours.
//!
//! `ShipStatus` is plain data; the glue in `exo_app` (`captain/live_status.rs`) fills it from the
//! reactor news, so nothing here depends on the reactor or fire crates.

use gameplay_core::TextKey;
use serde::{Deserialize, Serialize};

use crate::RoomId;

/// A system bar's full condition; 0 is out of order.
pub const SYSTEM_MAX: u8 = 20;

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
pub enum HeatBand {
    Cold,
    Normal,
    Hot,
    Critical,
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
    pub heat: f64,
    pub band: HeatBand,
    /// 0..1.
    pub fill: f64,
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
