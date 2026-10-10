//! Stage 3 of the SC step: what the atmosphere does to the ship. Lane `sc-air` (round 5) owns
//! this file.
//!
//! Scaffold: quadratic drag with the density, thrust lost in air, the caps lower in air; no lift,
//! no wind. The lane adds lift and drag per axis, wind, turbulence near the ground and the
//! proximity assist's ground check (spec on the lane issue).
use super::modes::Modes;
use super::Frame;
use crate::lerp;
use glam::DVec3;
use serde::Deserialize;

/// `sc_air.json`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AirTuning {
    /// Share of the thrust at full density (1 in vacuum).
    pub thrust_share: f64,
    /// Share of the speed caps at full density.
    pub cap_share: f64,
    /// Quadratic drag, a = k * density * v².
    pub drag_k: f64,
}

impl AirTuning {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.thrust_share > 0.0 && self.thrust_share <= 1.0) {
            return Err(format!("thrust_share {} out of range", self.thrust_share));
        }
        if !(self.cap_share > 0.0 && self.cap_share <= 1.0) {
            return Err(format!("cap_share {} out of range", self.cap_share));
        }
        if !(self.drag_k >= 0.0 && self.drag_k.is_finite()) {
            return Err(format!("drag_k {} out of range", self.drag_k));
        }
        Ok(())
    }
}

impl Default for AirTuning {
    fn default() -> Self {
        AirTuning { thrust_share: 0.6, cap_share: 0.6, drag_k: 0.0005 }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AirState {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AirOut {
    /// m/s², world: drag, lift and wind on the ship.
    pub accel: DVec3,
    /// rad/s², ship space: what the air turns (turbulence, weathervaning).
    pub angular: DVec3,
    /// Share of the thrust the air leaves.
    pub thrust_scale: f64,
    /// Share of the speed caps the air leaves.
    pub cap_scale: f64,
    /// m/s, world.
    pub wind: DVec3,
    /// 0..1.
    pub turbulence: f64,
}

pub fn step(_s: &mut AirState, f: &Frame, _m: &Modes, t: &AirTuning) -> AirOut {
    let drag = -f.v * t.drag_k * f.density * f.v.length();
    AirOut {
        accel: drag,
        angular: DVec3::ZERO,
        thrust_scale: lerp(1.0, t.thrust_share, f.density),
        cap_scale: lerp(1.0, t.cap_share, f.density),
        wind: DVec3::ZERO,
        turbulence: 0.0,
    }
}
