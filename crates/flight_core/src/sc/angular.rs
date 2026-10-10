//! Stage 5 of the SC step: the angular acceleration the flight computer asks for (ship space,
//! rad/s²: x pitch, y yaw, z roll), inside the torque box. Lane `sc-angular` (round 5) owns this
//! file.
//!
//! Scaffold: a target rate per axis from the stick (pitch and yaw in an ellipse), reached at
//! `decay` times the rate error, never past the target in one step. The lane replaces it with the
//! full law (spec on the lane issue).
use super::modes::Modes;
use super::Frame;
use crate::axis::{Dirs, Rot};
use crate::FlightInput;
use glam::DVec3;
use serde::Deserialize;

/// `sc_angular.json`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AngularTuning {
    /// rad/s at full stick.
    pub rate: Rot,
    /// 1/s: the flight computer asks for this times the rate error.
    pub decay: f64,
}

impl AngularTuning {
    pub fn validate(&self) -> Result<(), String> {
        self.rate.validate("rate")?;
        if !(self.decay > 0.0 && self.decay.is_finite()) {
            return Err(format!("decay {} out of range", self.decay));
        }
        Ok(())
    }
}

impl Default for AngularTuning {
    fn default() -> Self {
        AngularTuning { rate: Rot { pitch: 1.4, yaw: 1.1, roll: 2.4 }, decay: 12.0 }
    }
}

/// What the other stages give this one.
#[derive(Clone, Copy, Debug)]
pub struct Env {
    /// rad/s² per axis the torque gives.
    pub accel_box: Rot,
    /// Boost strength 0..1.
    pub boost: f64,
    /// m/s: the linear speed cap in force.
    pub cap: f64,
    /// m/s² the thrusters give per direction (for a G-safe turn cap).
    pub thrust_box: Dirs,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AngularState {}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AngularOut {
    /// rad/s², ship space.
    pub accel: DVec3,
    /// G-safe or comstab lowered the rate.
    pub rate_capped: bool,
}

pub fn step(_s: &mut AngularState, f: &Frame, input: &FlightInput, _m: &Modes, e: &Env, t: &AngularTuning) -> AngularOut {
    let r = &t.rate;
    let mut pitch = -input.mouse.y / f.dt + input.turn.x * r.pitch;
    let mut yaw = -input.mouse.x / f.dt + input.turn.y * r.yaw;
    let el = (pitch / r.pitch).powi(2) + (yaw / r.yaw).powi(2);
    if el > 1.0 {
        let s = el.sqrt();
        pitch /= s;
        yaw /= s;
    }
    let roll = (input.roll * r.roll).clamp(-r.roll, r.roll);
    let err = DVec3::new(pitch, yaw, roll) - f.w_local;
    let b = &e.accel_box;
    let a = DVec3::new((err.x * t.decay).clamp(-b.pitch, b.pitch), (err.y * t.decay).clamp(-b.yaw, b.yaw), (err.z * t.decay).clamp(-b.roll, b.roll));
    // Never past the target in one step.
    let most = err.abs() / f.dt;
    AngularOut { accel: a.clamp(-most, most), rate_capped: false }
}
