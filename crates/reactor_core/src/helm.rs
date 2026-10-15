//! The helm's dev stand-in (`bridge/bridge.json`, block `helm`): until a pilot flies, the engines'
//! target can random-walk. It belongs to the bridge, not the engine room: the engine room only
//! feels the demand.
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HelmTuning {
    /// The engines' target random-walks while on (the force follows it). Off in code and in the
    /// file. TODO(initiator): on or off by default.
    pub engine_wander: bool,
    /// Seconds between two new targets.
    pub wander_every_s: f64,
    /// The lowest wander target, 0..1 of full throttle.
    pub wander_min: f64,
}

impl Default for HelmTuning {
    fn default() -> HelmTuning {
        HelmTuning { engine_wander: false, wander_every_s: 6.0, wander_min: 0.3 }
    }
}
