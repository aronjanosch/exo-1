//! The SC model's tuning: one file per stage (`content/tuning/sc_<stage>.json`), so each stage's
//! lane edits only its own file. All values TODO(initiator); none are taken from Star Citizen.
use super::{air::AirTuning, angular::AngularTuning, drive::DriveTuning, linear::LinearTuning, modes::ModeTuning};
use crate::limits::{Dirs, Rot};
use serde::Deserialize;

/// The ship's body (`sc_ship.json`): mass, inertia and what its thrusters give.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShipBody {
    /// kg.
    pub mass: f64,
    /// kg m² about the ship's axes (pitch about x, yaw about y, roll about z).
    pub inertia: Rot,
    /// N per direction in vacuum, all thrusters that push that way together.
    pub thrust: Dirs,
    /// N m per rotation axis, both directions.
    pub torque: Rot,
    /// m, ship space: where cargo locked on the cabin floor weighs (its centre), for the inertia
    /// it adds (#198, #88).
    pub cargo_point: [f64; 3],
}

impl ShipBody {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.mass > 0.0 && self.mass.is_finite()) {
            return Err(format!("mass {} out of range", self.mass));
        }
        self.inertia.validate("inertia")?;
        self.thrust.validate("thrust")?;
        self.torque.validate("torque")?;
        if !self.cargo_point.iter().all(|v| v.is_finite()) {
            return Err(format!("cargo_point {:?} out of range", self.cargo_point));
        }
        Ok(())
    }
}

impl Default for ShipBody {
    fn default() -> Self {
        // A 2 t hull 4.6 x 3.2 x 8.3 m as a box (the greybox ship), thrust for 60 m/s² forward and
        // the other accelerations of the greybox, torque for about 8 rad/s² pitch and yaw.
        let m = 2000.0;
        let (w, h, d): (f64, f64, f64) = (4.6, 3.2, 8.3);
        let inertia = Rot { pitch: m / 12.0 * (h * h + d * d), yaw: m / 12.0 * (w * w + d * d), roll: m / 12.0 * (w * w + h * h) };
        ShipBody {
            mass: m,
            inertia,
            thrust: Dirs { forward: 60.0 * m, backward: 40.0 * m, left: 24.0 * m, right: 24.0 * m, up: 50.0 * m, down: 30.0 * m },
            torque: Rot { pitch: 8.0 * inertia.pitch, yaw: 8.0 * inertia.yaw, roll: 14.0 * inertia.roll },
            cargo_point: [0.0, -1.2, 0.0],
        }
    }
}

/// All stages' tuning.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScTuning {
    pub ship: ShipBody,
    pub modes: ModeTuning,
    pub linear: LinearTuning,
    pub angular: AngularTuning,
    pub drive: DriveTuning,
    pub air: AirTuning,
}

/// The files in the order `ScTuning::from_json` takes them.
pub const FILES: [&str; 6] = ["sc_ship.json", "sc_modes.json", "sc_linear.json", "sc_angular.json", "sc_drive.json", "sc_air.json"];

impl ScTuning {
    /// Parses and validates the six files, in the order of `FILES`.
    pub fn from_json(texts: [&str; 6]) -> Result<ScTuning, String> {
        fn one<T: serde::de::DeserializeOwned>(name: &str, s: &str, check: impl Fn(&T) -> Result<(), String>) -> Result<T, String> {
            let t: T = content_core::parse_strict(name, s)?;
            check(&t).map_err(|e| format!("{name}: {e}"))?;
            Ok(t)
        }
        Ok(ScTuning {
            ship: one(FILES[0], texts[0], ShipBody::validate)?,
            modes: one(FILES[1], texts[1], ModeTuning::validate)?,
            linear: one(FILES[2], texts[2], LinearTuning::validate)?,
            angular: one(FILES[3], texts[3], AngularTuning::validate)?,
            drive: one(FILES[4], texts[4], DriveTuning::validate)?,
            air: one(FILES[5], texts[5], AirTuning::validate)?,
        })
    }
}
