//! General tuning and the starting ship type, embedded like the planet recipe. A parse error
//! stops the game at startup with the message.
use bevy::prelude::*;
use flight_core::camera::CameraTuning;
use flight_core::ship_type::ShipType;
use flight_core::GroundTuning;
use grab_core::GrabConfig;
use walker_core::{SuitConfig, WalkerConfig};

pub const GROUND: &str = include_str!("../../../content/tuning/ground.json");
pub const WALKER: &str = include_str!("../../../content/tuning/walker.json");
pub const SUIT: &str = include_str!("../../../content/tuning/suit.json");
pub const CAMERA: &str = include_str!("../../../content/tuning/camera.json");
pub const GRAB: &str = include_str!("../../../content/tuning/grab.json");
pub const HUD: &str = include_str!("../../../content/tuning/hud.json");
pub const SHIP_MODEL: &str = include_str!("../../../content/ship/kackfass_1/type.json");
/// The SC flight model's six files, in the order of `flight_core::sc::tuning::FILES`.
pub const SC: [&str; 6] = [
    include_str!("../../../content/ship/kackfass_1/sc_ship.json"),
    include_str!("../../../content/ship/kackfass_1/sc_modes.json"),
    include_str!("../../../content/ship/kackfass_1/sc_linear.json"),
    include_str!("../../../content/ship/kackfass_1/sc_angular.json"),
    include_str!("../../../content/ship/kackfass_1/sc_drive.json"),
    include_str!("../../../content/ship/kackfass_1/sc_air.json"),
];

#[derive(Resource, Clone, Debug)]
pub struct Tuning {
    /// The ground rules of the ship (`ground.json`).
    pub ground: GroundTuning,
    pub walker: WalkerConfig,
    pub suit: SuitConfig,
    pub camera: CameraTuning,
    pub grab: GrabConfig,
    pub hud: crate::hud::HudTuning,
    /// Starting ship definition; each instance receives its own flight state.
    pub ship_type: ShipType,
}

impl Tuning {
    pub fn load() -> Tuning {
        Tuning { ground: ok(GroundTuning::from_json(GROUND)), walker: ok(WalkerConfig::from_json(WALKER)), suit: ok(SuitConfig::from_json(SUIT)), camera: ok(CameraTuning::from_json(CAMERA)), grab: ok(GrabConfig::from_json(GRAB)), hud: ok(crate::hud::HudTuning::from_json(HUD)), ship_type: ok(ShipType::from_json(SHIP_MODEL, SC)) }
    }
}

fn ok<T>(r: Result<T, String>) -> T {
    r.unwrap_or_else(|e| panic!("tuning: {e}"))
}
