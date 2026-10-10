//! Stages 2 and 6 of the SC step: the boost (`begin`) and what the thrusters really give
//! (`shape`). Lane `sc-body` (round 5) owns this file, `tuning.rs` (`ShipBody`) and the force glue
//! in the app.
//!
//! Scaffold: the boost capacitor of the axis model; the thrusters give at once what is asked.
//! The lane adds spool per thruster group, jerk limits and the boost ramp (spec on the lane issue).
use crate::axis::Dirs;
use crate::{lerp, BoostCapacitor, BoostCapacitorTuning};
use glam::DVec3;
use serde::Deserialize;

/// `sc_drive.json`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DriveTuning {
    pub boost_capacitor: BoostCapacitorTuning,
    /// Multipliers on the thrust per direction at full boost.
    pub boost_thrust: Dirs,
}

impl DriveTuning {
    pub fn validate(&self) -> Result<(), String> {
        self.boost_capacitor.validate()?;
        self.boost_thrust.validate("boost_thrust")
    }
}

impl Default for DriveTuning {
    fn default() -> Self {
        DriveTuning { boost_capacitor: BoostCapacitorTuning::default(), boost_thrust: Dirs { forward: 2.0, backward: 1.5, left: 1.25, right: 1.25, up: 1.25, down: 1.25 } }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DriveState {
    pub boost: BoostCapacitor,
}

/// The box `b` with the boost's multipliers at strength `boost` (0..1).
pub fn boosted(b: &Dirs, t: &DriveTuning, boost: f64) -> Dirs {
    let m = |x: f64| lerp(1.0, x, boost);
    let k = &t.boost_thrust;
    Dirs { forward: b.forward * m(k.forward), backward: b.backward * m(k.backward), left: b.left * m(k.left), right: b.right * m(k.right), up: b.up * m(k.up), down: b.down * m(k.down) }
}

/// The boost this step, 0..1. The brake stops a boost and does not drain the charge.
pub fn begin(s: &mut DriveState, boost: bool, braking: bool, t: &DriveTuning, dt: f64) -> f64 {
    s.boost.step_braking(boost, braking, &t.boost_capacitor, dt)
}

/// What the flight computer asked of the thrusters this step.
#[derive(Clone, Copy, Debug)]
pub struct Asked {
    /// m/s², ship space.
    pub linear: DVec3,
    /// rad/s², ship space.
    pub angular: DVec3,
    pub boost: f64,
    pub thrust_box: Dirs,
}

/// What the thrusters give.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Given {
    pub linear: DVec3,
    pub angular: DVec3,
}

pub fn shape(_s: &mut DriveState, a: &Asked, _t: &DriveTuning, _dt: f64) -> Given {
    Given { linear: a.linear, angular: a.angular }
}
