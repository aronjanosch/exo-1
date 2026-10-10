//! Stage 5 of the SC step: the angular acceleration the flight computer asks for (ship space,
//! rad/s²: x pitch, y yaw, z roll), inside the torque box. Lane `sc-angular` (round 5) owns this
//! file.
//!
//! The law (after the rotation of Star Citizen's flight control, structure only; own values):
//!
//! 1. One budget: the pitch, yaw and roll requests (stick and direct mouse) are scaled together
//!    so their normalised rates stay on one ellipsoid.
//! 2. Pitch and yaw are a second-order underdamped spring on the rate: they overshoot the target
//!    and settle; the overshoot is not clamped away.
//! 3. A reversal (target opposite to the spin) is a constant deceleration of `reversal_share` of
//!    the box until the spin has crossed zero, then item 2.
//! 4. Roll is first order towards its target; release (target 0) is a constant deceleration of
//!    `roll_release_share` of the box, stopping at zero.
//! 5. Rates follow the speed (`rate_over_speed`, speed over the cap) and boost (`boost_rate`).
//! 6. G-safe (`Modes::g_safe`, coupled): pitch and yaw scale so rate x velocity stays inside the
//!    G limit per direction (own copy of the values).
//! 7. Landing mode: the rates times `landing_rate_share` near the ground.
//!
//! All values are TODO(initiator) (`content/tuning/sc_angular.json`).
use super::modes::Modes;
use super::StepState;
use crate::limits::{Dirs, Rot, G0};
use crate::{lerp, smoothstep, Curve, FlightInput, Interp};
use glam::{DVec2, DVec3};
use serde::Deserialize;

/// One number per pitch and yaw.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PitchYaw {
    pub pitch: f64,
    pub yaw: f64,
}

/// `sc_angular.json`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AngularTuning {
    /// rad/s at full stick.
    pub rate: Rot,
    /// Multipliers on `rate` at full boost.
    pub boost_rate: Rot,
    /// Share of the rates (y) over the speed as a share of the cap (x): a corner speed in the
    /// middle, slower at rest and at the top.
    pub rate_over_speed: Curve,
    /// 1/s: the natural frequency of the pitch and yaw spring.
    pub natural_frequency: PitchYaw,
    /// The damping ratio of the pitch and yaw spring (below 1 overshoots).
    pub damping_ratio: PitchYaw,
    /// Share of the box for a reversal (the spin brought to zero at a constant rate).
    pub reversal_share: f64,
    /// 1/s: roll asks for this times its rate error.
    pub roll_decay: f64,
    /// Share of the box for a roll release (target 0): a constant deceleration to zero.
    pub roll_release_share: f64,
    /// Share of the rates at full landing mode (near the ground).
    pub landing_rate_share: f64,
    /// m: full landing share at and below this clearance.
    pub landing_full_below: f64,
    /// m: no landing share above this clearance; smooth in between.
    pub landing_off_above: f64,
    /// g per direction of the G-safe turn cap (the centripetal push of a turn plus the gravity hold).
    pub g_limit: Dirs,
}

impl AngularTuning {
    pub fn validate(&self) -> Result<(), String> {
        self.rate.validate("rate")?;
        self.boost_rate.validate("boost_rate")?;
        self.rate_over_speed.validate().map_err(|e| format!("rate_over_speed: {e}"))?;
        if let Some(p) = self.rate_over_speed.points.iter().find(|p| p.y <= 0.0) {
            return Err(format!("rate_over_speed: share {} must be above 0", p.y));
        }
        for (n, v) in [
            ("natural_frequency.pitch", self.natural_frequency.pitch),
            ("natural_frequency.yaw", self.natural_frequency.yaw),
            ("damping_ratio.pitch", self.damping_ratio.pitch),
            ("damping_ratio.yaw", self.damping_ratio.yaw),
            ("roll_decay", self.roll_decay),
        ] {
            if !(v > 0.0 && v.is_finite()) {
                return Err(format!("{n} {v} out of range"));
            }
        }
        for (n, v) in [("reversal_share", self.reversal_share), ("roll_release_share", self.roll_release_share), ("landing_rate_share", self.landing_rate_share)] {
            if !(v > 0.0 && v <= 1.0) {
                return Err(format!("{n} {v} out of range"));
            }
        }
        if !(self.landing_full_below >= 0.0 && self.landing_off_above > self.landing_full_below && self.landing_off_above.is_finite()) {
            return Err(format!("landing band {} to {} out of range", self.landing_full_below, self.landing_off_above));
        }
        self.g_limit.validate("g_limit")
    }
}

impl Default for AngularTuning {
    fn default() -> Self {
        AngularTuning {
            rate: Rot { pitch: 1.4, yaw: 1.1, roll: 2.4 },
            boost_rate: Rot { pitch: 1.2, yaw: 1.2, roll: 1.0 },
            rate_over_speed: Curve { interp: Interp::Linear, points: vec![DVec2::new(0.0, 0.85), DVec2::new(0.5, 1.0), DVec2::new(1.0, 0.8)] },
            natural_frequency: PitchYaw { pitch: 4.0, yaw: 4.0 },
            damping_ratio: PitchYaw { pitch: 0.5, yaw: 0.5 },
            reversal_share: 0.6,
            roll_decay: 6.0,
            roll_release_share: 0.5,
            landing_rate_share: 0.5,
            landing_full_below: 5.0,
            landing_off_above: 40.0,
            g_limit: Dirs { forward: 8.0, backward: 6.0, left: 4.0, right: 4.0, up: 6.0, down: 3.0 },
        }
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
    /// rad/s², ship space: what the air turns that the flight computer holds against
    /// (`AirOut::angular_hold`).
    pub air_hold: DVec3,
}

/// The angular stage's memory: per pitch and yaw the acceleration the spring asked last step (the
/// spring is second order on the rate, so its acceleration is the state that carries over), and per
/// pitch and yaw the spin's sign when a reversal started (0 = no reversal).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AngularState {
    accel: DVec2,
    reversing: DVec2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AngularOut {
    /// rad/s², ship space.
    pub accel: DVec3,
    /// G-safe or comstab lowered the rate.
    pub rate_capped: bool,
}

/// The product of target and spin (rad²/s²) that counts as a reversal: float noise around zero spin
/// must not start one.
const REVERSAL_EPS: f64 = 1e-9;

/// One pitch or yaw axis: the reversal in front of the spring. `reversing` is the spin's sign when a
/// reversal started (0 = none); `accel` is the spring's acceleration last step. Returns this step's
/// acceleration: the reversal's constant deceleration, or the spring
/// `a' = wn² (target - spin) - 2 zeta wn a` integrated over `dt` (the rate overshoots and settles).
fn pitch_yaw_axis(reversing: &mut f64, accel: &mut f64, spin: f64, target: f64, dt: f64, p: (f64, f64, f64, f64)) -> f64 {
    let (wn, zeta, box_, share) = p;
    if *reversing != 0.0 && spin * *reversing > 0.0 {
        // Still on the side the reversal started from: a constant deceleration to zero.
        *accel = -*reversing * share * box_;
    } else if target * spin < -REVERSAL_EPS {
        *reversing = spin.signum();
        *accel = -*reversing * share * box_;
    } else {
        *reversing = 0.0;
        let a = *accel + dt * (wn * wn * (target - spin) - 2.0 * zeta * wn * *accel);
        *accel = a.clamp(-box_, box_);
    }
    *accel
}

/// Roll: first order towards the target; release (target 0) brings the spin to zero at a constant
/// deceleration, never past zero.
fn roll_axis(spin: f64, target: f64, dt: f64, decay: f64, release_share: f64, box_: f64) -> f64 {
    if target == 0.0 {
        if spin == 0.0 {
            return 0.0;
        }
        -spin.signum() * (release_share * box_).min(spin.abs() / dt)
    } else {
        (decay * (target - spin)).clamp(-box_, box_)
    }
}

/// Landing mode's share of the landing rates: 1 at and below the full band, 0 above the off band.
fn landing_share(t: &AngularTuning, clearance: f64) -> f64 {
    1.0 - smoothstep(t.landing_full_below, t.landing_off_above, clearance)
}

pub fn step(s: &mut AngularState, f: &StepState, input: &FlightInput, m: &Modes, e: &Env, t: &AngularTuning) -> AngularOut {
    let dt = f.dt;
    let w = f.w_local;

    // The rates this step: the corner over the speed, boost and landing mode.
    let speed_share = if e.cap > 0.0 { f.v.length() / e.cap } else { 0.0 };
    let over_speed = t.rate_over_speed.eval(speed_share);
    let landing = if m.landing { lerp(1.0, t.landing_rate_share, landing_share(t, f.clearance)) } else { 1.0 };
    let rate = |base: f64, boost: f64| base * lerp(1.0, boost, e.boost) * over_speed * landing;
    let r = &t.rate;
    let rates = DVec3::new(rate(r.pitch, t.boost_rate.pitch), rate(r.yaw, t.boost_rate.yaw), rate(r.roll, t.boost_rate.roll));

    // 1. One budget: the requests (direct mouse as a rate, stick times the rate) scaled together
    // onto the ellipsoid of the three rates.
    let mut req = DVec3::new(-input.mouse.y / dt + input.turn.x * rates.x, -input.mouse.x / dt + input.turn.y * rates.y, input.roll * rates.z);
    let norm = (req / rates).length();
    if norm > 1.0 {
        req /= norm;
    }

    // 6. G-safe turn cap: rate x velocity (the centripetal push of a coupled turn) plus the gravity
    // hold stays inside the limit per direction. Scales pitch and yaw only.
    let mut rate_capped = false;
    if m.g_safe && m.coupling > 0.0 {
        let g = t.g_limit.scaled(G0);
        let hold_up = f.inv * -f.gravity;
        let turn = DVec3::new(req.x, req.y, 0.0).cross(f.lv);
        let mut k_scale: f64 = 1.0;
        for (k, base, pos, neg) in [(turn.x, hold_up.x, g.right, g.left), (turn.y, hold_up.y, g.up, g.down), (turn.z, hold_up.z, g.backward, g.forward)] {
            if k > 1e-12 {
                k_scale = k_scale.min((pos - base) / k);
            } else if k < -1e-12 {
                k_scale = k_scale.min((neg + base) / -k);
            }
        }
        let scale = lerp(1.0, k_scale.clamp(0.0, 1.0), m.coupling);
        rate_capped = scale < 1.0 - 1e-9;
        req.x *= scale;
        req.y *= scale;
    }

    // 2 and 3. Pitch and yaw: the reversal, then the spring. 4. Roll.
    let b = &e.accel_box;
    let (mut rev, mut acc) = (s.reversing, s.accel);
    let pitch = pitch_yaw_axis(&mut rev.x, &mut acc.x, w.x, req.x, dt, (t.natural_frequency.pitch, t.damping_ratio.pitch, b.pitch, t.reversal_share));
    let yaw = pitch_yaw_axis(&mut rev.y, &mut acc.y, w.y, req.y, dt, (t.natural_frequency.yaw, t.damping_ratio.yaw, b.yaw, t.reversal_share));
    s.reversing = rev;
    s.accel = acc;
    let roll = roll_axis(w.z, req.z, dt, t.roll_decay, t.roll_release_share, b.roll);

    // The air's known moments are held on top, inside the box.
    let held = DVec3::new(pitch, yaw, roll) - e.air_hold;
    let held = held.clamp(DVec3::new(-b.pitch, -b.yaw, -b.roll), DVec3::new(b.pitch, b.yaw, b.roll));
    AngularOut { accel: held, rate_capped }
}
