//! Tuning values from `content/tuning/*.json`, embedded like the planet recipe. A parse error
//! stops the game at startup with the message.
use bevy::prelude::*;
use flight_core::ShipTuning;
use walker_core::{SuitConfig, WalkerConfig};

pub const SHIP: &str = include_str!("../../../content/tuning/ship.json");
pub const WALKER: &str = include_str!("../../../content/tuning/walker.json");
pub const SUIT: &str = include_str!("../../../content/tuning/suit.json");

#[derive(Resource, Clone, Debug)]
pub struct Tuning {
    pub ship: ShipTuning,
    pub walker: WalkerConfig,
    pub suit: SuitConfig,
}

impl Tuning {
    pub fn load() -> Tuning {
        Tuning { ship: ok(ShipTuning::from_json(SHIP)), walker: ok(WalkerConfig::from_json(WALKER)), suit: ok(SuitConfig::from_json(SUIT)) }
    }
}

fn ok<T>(r: Result<T, String>) -> T {
    r.unwrap_or_else(|e| panic!("tuning: {e}"))
}
