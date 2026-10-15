//! Life support (`hull/hull.json`, block `life_support`): the air the oxygen generator makes and
//! the air a breathing body takes. The generator machine stands in the engine room (its condition
//! is the engine room's); the numbers of the air are the ship's.
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifeSupportTuning {
    /// The oxygen generator's air, m³ per second at full life support power (`fire_core::oxygen`).
    /// TODO(initiator): value; 2.13 refills the 256 m³ engine room from half in about 60 s.
    pub oxygen_generator_rate: f64,
    /// The air one breathing player takes out of the room, m³ per second. Barotrauma
    /// `Hull.OxygenConsumptionSpeed` 700 px²/s over a 1000 × 320 px hull is 0.0022 of the room a
    /// second, which is 0.56 m³/s in our 256 m³ room. TODO(initiator): value; at 0.56 the full
    /// generator (2.13) outruns about 3.8 players.
    pub breath_m3_per_s: f64,
}

impl Default for LifeSupportTuning {
    fn default() -> LifeSupportTuning {
        LifeSupportTuning { oxygen_generator_rate: 2.13, breath_m3_per_s: 0.56 }
    }
}
