//! Tuning values from `content/tuning/*.json`, embedded like the planet recipe. A parse error
//! stops the game at startup with the message.
use bevy::prelude::*;
use flight_core::camera::CameraTuning;
use flight_core::sc::ScTuning;
use flight_core::ShipTuning;
use grab_core::GrabConfig;
use walker_core::{SuitConfig, WalkerConfig};

pub const SHIP: &str = include_str!("../../../content/tuning/ship.json");
pub const WALKER: &str = include_str!("../../../content/tuning/walker.json");
pub const SUIT: &str = include_str!("../../../content/tuning/suit.json");
pub const CAMERA: &str = include_str!("../../../content/tuning/camera.json");
pub const GRAB: &str = include_str!("../../../content/tuning/grab.json");
pub const HUD: &str = include_str!("../../../content/tuning/hud.json");
/// The SC flight model's six files, in the order of `flight_core::sc::tuning::FILES`.
pub const SC: [&str; 6] = [
    include_str!("../../../content/tuning/sc_ship.json"),
    include_str!("../../../content/tuning/sc_modes.json"),
    include_str!("../../../content/tuning/sc_linear.json"),
    include_str!("../../../content/tuning/sc_angular.json"),
    include_str!("../../../content/tuning/sc_drive.json"),
    include_str!("../../../content/tuning/sc_air.json"),
];

#[derive(Resource, Clone, Debug)]
pub struct Tuning {
    pub ship: ShipTuning,
    pub walker: WalkerConfig,
    pub suit: SuitConfig,
    pub camera: CameraTuning,
    pub grab: GrabConfig,
    pub hud: crate::hud::HudTuning,
    /// The SC flight model (F7, round 5).
    pub sc: ScTuning,
}

impl Tuning {
    pub fn load() -> Tuning {
        Tuning { ship: ok(ShipTuning::from_json(SHIP)), walker: ok(WalkerConfig::from_json(WALKER)), suit: ok(SuitConfig::from_json(SUIT)), camera: ok(CameraTuning::from_json(CAMERA)), grab: ok(GrabConfig::from_json(GRAB)), hud: ok(crate::hud::HudTuning::from_json(HUD)), sc: ok(ScTuning::from_json(SC)) }
    }
}

fn ok<T>(r: Result<T, String>) -> T {
    r.unwrap_or_else(|e| panic!("tuning: {e}"))
}
