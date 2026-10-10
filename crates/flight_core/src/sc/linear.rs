//! Stage 4 of the SC step: the thrust the flight computer asks for (ship space, m/s²), inside the
//! thrust box. Lane `sc-linear` (round 5) owns this file and `modes.rs`.
//!
//! Scaffold: coupled flies towards a velocity goal from the stick, decoupled pushes along the stick,
//! blended by `Modes::coupling`; gravity compensation holds against gravity and air; the brake
//! asks for zero velocity. The lane replaces it with the full law (spec on the lane issue).
use super::modes::{Master, Modes};
use super::Frame;
use crate::axis::Dirs;
use crate::{lerp, limit_length, FlightInput};
use glam::DVec3;
use serde::Deserialize;

/// Speed caps of one master mode (m/s).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Caps {
    /// Coupled speed for full stick in any direction.
    pub cruise: f64,
    /// Forward and backward at full boost (at least `cruise`).
    pub boost_forward: f64,
    pub boost_backward: f64,
}

impl Caps {
    fn validate(&self, what: &str) -> Result<(), String> {
        if !(self.cruise > 0.0 && self.cruise.is_finite()) {
            return Err(format!("{what}.cruise {} out of range", self.cruise));
        }
        if self.boost_forward < self.cruise || self.boost_backward < self.cruise || !self.boost_forward.is_finite() || !self.boost_backward.is_finite() {
            return Err(format!("{what}: boost caps {} and {} below cruise {}", self.boost_forward, self.boost_backward, self.cruise));
        }
        Ok(())
    }
}

/// `sc_linear.json`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LinearTuning {
    pub scm: Caps,
    pub nav: Caps,
    /// 1/s: coupled asks for this times the velocity error.
    pub decay: f64,
}

impl LinearTuning {
    pub fn validate(&self) -> Result<(), String> {
        self.scm.validate("scm")?;
        self.nav.validate("nav")?;
        if !(self.decay > 0.0 && self.decay.is_finite()) {
            return Err(format!("decay {} out of range", self.decay));
        }
        Ok(())
    }

    pub fn caps(&self, m: Master) -> &Caps {
        match m {
            Master::Scm => &self.scm,
            Master::Nav => &self.nav,
        }
    }
}

impl Default for LinearTuning {
    fn default() -> Self {
        LinearTuning {
            scm: Caps { cruise: 150.0, boost_forward: 250.0, boost_backward: 200.0 },
            nav: Caps { cruise: 400.0, boost_forward: 700.0, boost_backward: 400.0 },
            decay: 3.0,
        }
    }
}

/// What the other stages give this one.
#[derive(Clone, Copy, Debug)]
pub struct Env {
    /// m/s² per direction the thrusters can give this step (air and boost included).
    pub thrust_box: Dirs,
    /// Boost strength 0..1.
    pub boost: f64,
    pub braking: bool,
    /// m/s², world: what the air does to the ship (drag, lift, wind).
    pub air_accel: DVec3,
    /// Share of the caps the air leaves (1 in space).
    pub cap_scale: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinearState {}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinearOut {
    /// m/s², ship space: the thrust asked of the thrusters, inside `Env::thrust_box`.
    pub accel: DVec3,
    /// m/s: the speed cap in force.
    pub cap: f64,
    /// The request did not fit the box.
    pub saturated: bool,
}

pub fn step(_s: &mut LinearState, f: &Frame, input: &FlightInput, m: &Modes, e: &Env, t: &LinearTuning) -> LinearOut {
    let stick = if e.braking { DVec3::ZERO } else { limit_length(input.thrust, 1.0) };
    let c = t.caps(m.master);
    let scale = e.cap_scale * m.limiter;
    let cruise = c.cruise * scale;
    let forward = lerp(c.cruise, c.boost_forward, e.boost) * scale;
    let backward = lerp(c.cruise, c.boost_backward, e.boost) * scale;
    let goal = f.rot * DVec3::new(stick.x * cruise, stick.y * cruise, stick.z * if stick.z < 0.0 { forward } else { backward });
    let hold = if m.grav_comp { -(f.gravity + e.air_accel) } else { DVec3::ZERO };
    let coupled = f.inv * ((goal - f.v) * t.decay + hold);
    let decoupled = e.thrust_box.along(stick) + f.inv * hold;
    let c = if e.braking { 1.0 } else { m.coupling };
    let asked = coupled * c + decoupled * (1.0 - c);
    let accel = e.thrust_box.clamp(asked);
    LinearOut { accel, cap: forward, saturated: (asked - accel).length() > 1e-6 }
}
