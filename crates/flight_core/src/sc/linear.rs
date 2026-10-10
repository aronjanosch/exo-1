//! Stage 4 of the SC step: the thrust the flight computer asks for (ship space, m/s²), inside the
//! thrust box. Lane `sc-linear` (round 5, #195) owns this file and `modes.rs`.
//!
//! Coupled flies a velocity goal from the stick (`cruise` sideways and vertical, the caps along Z
//! at boost); decoupled pushes along the stick; `Modes::coupling` blends them. The cap refuses the
//! thrust along the velocity above it (A3), the brake takes one deceleration along the velocity
//! (B2), anti-drift keeps the path straight while an axis saturates, gravity compensation holds
//! against gravity and air (off: the ship falls in a frame that falls with it), G-safe cuts the
//! box, the strafe tapers with forward speed, comstab keeps the goal on the nose, and proximity
//! and landing mode limit the descent and the speed near the ground. Ported from the axis model
//! (`axis.rs`: `refuse_thrust`, `brake_along`, the precision band, the descent limit).
use super::modes::{Master, Modes};
use super::Frame;
use crate::axis::{Dirs, G0};
use crate::{lerp, limit_length, smoothstep, Curve, FlightInput, Interp};
use glam::{DQuat, DVec2, DVec3};
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

/// The landing mode's precision band (K), ported from the axis model's `Precision`.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LandingBand {
    /// m: full precision at and below this clearance.
    pub full_below: f64,
    /// m: off above this clearance; smooth in between.
    pub off_above: f64,
    /// m/s: speed cap along the ground at full precision.
    pub speed: f64,
    /// Share of `speed` for the descent (the touchdown speed).
    pub landing_share: f64,
}

/// `sc_linear.json`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LinearTuning {
    pub scm: Caps,
    pub nav: Caps,
    /// 1/s: coupled asks for this times the velocity error.
    pub decay: f64,
    /// A3: the cap refuses the thrust along the velocity above it.
    pub refuse_thrust: bool,
    /// Anti-drift: with an axis saturated, the velocity across the goal dies first.
    pub anti_drift: bool,
    /// Share of the lateral and vertical thrust over forward speed / cruise cap, unboosted.
    pub strafe_taper: Curve,
    /// The same, boosted (tapers across the run up to the boost cap).
    pub strafe_taper_boost: Curve,
    /// g per direction of the thrust while G-safe is on.
    pub g_limit: Dirs,
    /// The boost lifts G-safe.
    pub boost_disables_g_safe: bool,
    /// m/s: below this the brake's rate is proportional to the speed.
    pub brake_ease_speed: f64,
    /// s: time constant with which the goal follows the nose with comstab off.
    pub comstab_off_lag: f64,
    /// Share of the upward thrust the descent limit counts on.
    pub descent_reserve: f64,
    /// s: the goal's downward part is cut when the time to the ground is below this.
    pub proximity_time: f64,
    pub landing: LandingBand,
}

impl LinearTuning {
    pub fn validate(&self) -> Result<(), String> {
        self.scm.validate("scm")?;
        self.nav.validate("nav")?;
        if !(self.decay > 0.0 && self.decay.is_finite()) {
            return Err(format!("decay {} out of range", self.decay));
        }
        for (what, c) in [("strafe_taper", &self.strafe_taper), ("strafe_taper_boost", &self.strafe_taper_boost)] {
            c.validate().map_err(|e| format!("{what}: {e}"))?;
            if let Some(p) = c.points.iter().find(|p| p.y < 0.0) {
                return Err(format!("{what}: share {} below 0", p.y));
            }
        }
        self.g_limit.validate("g_limit")?;
        if !(self.brake_ease_speed > 0.0 && self.brake_ease_speed.is_finite()) {
            return Err(format!("brake_ease_speed {} out of range", self.brake_ease_speed));
        }
        if !(self.comstab_off_lag >= 0.0 && self.comstab_off_lag.is_finite()) {
            return Err(format!("comstab_off_lag {} out of range", self.comstab_off_lag));
        }
        if !(self.descent_reserve > 0.0 && self.descent_reserve.is_finite()) {
            return Err(format!("descent_reserve {} out of range", self.descent_reserve));
        }
        if !(self.proximity_time >= 0.0 && self.proximity_time.is_finite()) {
            return Err(format!("proximity_time {} out of range", self.proximity_time));
        }
        let l = &self.landing;
        if !(l.full_below >= 0.0 && l.off_above > l.full_below && l.off_above.is_finite()) {
            return Err(format!("landing: band {} to {} m out of range", l.full_below, l.off_above));
        }
        if !(l.speed > 0.0 && l.speed.is_finite()) {
            return Err(format!("landing: speed {} out of range", l.speed));
        }
        if !(l.landing_share > 0.0 && l.landing_share <= 1.0) {
            return Err(format!("landing: landing_share {} out of range", l.landing_share));
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
        let curve = |points: &[(f64, f64)]| Curve { interp: Interp::Linear, points: points.iter().map(|&(x, y)| DVec2::new(x, y)).collect() };
        LinearTuning {
            scm: Caps { cruise: 150.0, boost_forward: 250.0, boost_backward: 200.0 },
            nav: Caps { cruise: 400.0, boost_forward: 700.0, boost_backward: 400.0 },
            decay: 3.0,
            refuse_thrust: true,
            anti_drift: true,
            strafe_taper: curve(&[(0.0, 1.0), (1.0, 1.0), (1.5, 0.5)]),
            strafe_taper_boost: curve(&[(0.0, 1.0), (1.0, 0.6), (1.7, 0.35)]),
            g_limit: Dirs { forward: 8.0, backward: 6.0, left: 4.0, right: 4.0, up: 6.0, down: 3.0 },
            boost_disables_g_safe: false,
            brake_ease_speed: 3.0,
            comstab_off_lag: 3.0,
            descent_reserve: 0.6,
            proximity_time: 3.0,
            landing: LandingBand { full_below: 5.0, off_above: 40.0, speed: 15.0, landing_share: 0.2 },
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
    /// m/s, world: the velocity the coupled goal moves with: the wind while wind compensation is
    /// off (the ship drifts with the air mass), else zero.
    pub drift: DVec3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinearState {
    /// m/s, world: the fall gravity adds while gravity compensation is off. The goal moves with it,
    /// so the assist does not fight the fall. Dropped when compensation comes back on.
    pub fall: DVec3,
    /// The frame the goal turns in: the nose with comstab on, lagging behind it with comstab off.
    pub aim: DQuat,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinearOut {
    /// m/s², ship space: the thrust asked of the thrusters, inside `Env::thrust_box`.
    pub accel: DVec3,
    /// m/s: the forward speed cap in force (master mode, boost, limiter, air, landing band).
    pub cap: f64,
    /// The request did not fit the box.
    pub saturated: bool,
}

/// The largest k that keeps `hold + u * k` inside the box: the weakest axis in that direction
/// (ported from the axis model's `brake_along`).
fn brake_ceiling(b: &Dirs, hold: DVec3, u: DVec3) -> f64 {
    let mut k = f64::INFINITY;
    for (h, c, lo, hi) in [(hold.x, u.x, -b.left, b.right), (hold.y, u.y, -b.down, b.up), (hold.z, u.z, -b.forward, b.backward)] {
        if c > 1e-12 {
            k = k.min((hi - h) / c);
        } else if c < -1e-12 {
            k = k.min((lo - h) / c);
        }
    }
    k
}

/// Anti-drift (notes section 1, "Anti-drift"): thrust across the goal direction `g` (ship space)
/// kills the velocity across it at the full thrust available that way, and the thrust along the
/// goal follows the ratio of the along error to that across speed, so the path stays straight.
/// `None` when the velocity has no part across the goal (the plain law then applies).
/// m/s: anti-drift acts only on an across speed above this.
const ANTI_DRIFT_MIN: f64 = 0.5;

fn anti_drift(b: &Dirs, lv: DVec3, g: DVec3, goal: DVec3) -> Option<DVec3> {
    let across = lv - g * lv.dot(g);
    let n = across.length();
    // Below this the across speed is the ship settling, not drift.
    if n < ANTI_DRIFT_MIN {
        return None;
    }
    let p = -across / n;
    let avail = b.support(p);
    let along_err = (goal - lv).dot(g);
    Some(p * avail + g * (avail * along_err / n))
}

pub fn step(s: &mut LinearState, f: &Frame, input: &FlightInput, m: &Modes, e: &Env, t: &LinearTuning) -> LinearOut {
    let stick = if e.braking { DVec3::ZERO } else { limit_length(input.thrust, 1.0) };
    let c = t.caps(m.master);
    let scale = e.cap_scale * m.limiter;
    let cruise = c.cruise * scale;
    let forward = lerp(c.cruise, c.boost_forward, e.boost) * scale;
    let backward = lerp(c.cruise, c.boost_backward, e.boost) * scale;

    // Comstab: the goal turns with the nose at once, or follows it with a lag.
    s.aim = if m.comstab || t.comstab_off_lag <= 0.0 { f.rot } else { s.aim.slerp(f.rot, 1.0 - (-f.dt / t.comstab_off_lag).exp()) };
    let goal_loc = DVec3::new(stick.x * cruise, stick.y * cruise, stick.z * if stick.z < 0.0 { forward } else { backward });
    let goal_stick = s.aim * goal_loc;

    // Gravity compensation: on, thrust holds against gravity and air; off, the fall is integrated
    // into the goal instead (the ship falls with g).
    // The air (drag, lift, the wind with wind compensation on) is held either way; gravity only
    // with compensation on.
    let hold = if m.grav_comp { -f.gravity } else { DVec3::ZERO } - e.air_accel;
    // The goal uses the fall of the start of the step: the velocity then follows it with no lag.
    let fall = s.fall;
    if m.grav_comp || e.braking {
        s.fall = DVec3::ZERO;
    } else {
        s.fall += f.gravity * f.dt;
    }

    // The box: the thrusters' box, G-safe cut, the strafe taper on the lateral and vertical sides.
    let mut b = e.thrust_box;
    if m.g_safe && !(t.boost_disables_g_safe && e.boost > 0.0) {
        b = b.min(&t.g_limit.scaled(G0));
    }
    let x = (-f.lv.z).max(0.0) / cruise;
    let tap = lerp(t.strafe_taper.eval(x), t.strafe_taper_boost.eval(x), e.boost);
    b = Dirs { left: b.left * tap, right: b.right * tap, up: b.up * tap, down: b.down * tap, ..b };

    let sink = (-f.v.dot(f.up)).max(0.0);
    let brake_up = (b.support(f.inv * f.up) - f.gravity.length()).max(1.0);

    if e.braking {
        // B2: one deceleration along the velocity, the weakest axis as the ceiling, gravity and air
        // held; the rate eases to zero with the speed.
        let hold_l = b.clamp(f.inv * -(f.gravity + e.air_accel));
        let speed = f.lv.length();
        let thrust = if speed > 1e-9 {
            let u = -f.lv / speed;
            let rate = brake_ceiling(&b, hold_l, u).max(0.0) * (speed / t.brake_ease_speed).min(1.0);
            hold_l + u * rate
        } else {
            hold_l
        };
        let accel = b.clamp(thrust);
        return LinearOut { accel, cap: forward, saturated: (thrust - accel).length() > 1e-6 };
    }

    // The goal from the stick, with the fall; the landing band and the descent limit cut it.
    let mut goal = goal_stick + fall + e.drift;
    let mut forward_cap = forward;
    let mut descent = DVec3::ZERO;
    if m.landing {
        let p = 1.0 - smoothstep(t.landing.full_below, t.landing.off_above, f.clearance - sink * sink / (2.0 * brake_up));
        let vertical = goal.dot(f.up);
        let horizontal = goal - f.up * vertical;
        let l = horizontal.length();
        let horizontal = if l > 1e-9 { horizontal * (lerp(l, l.min(t.landing.speed), p) / l) } else { horizontal };
        let descent_cap = t.landing.speed * t.landing.landing_share;
        let vertical = if vertical < 0.0 { -lerp(-vertical, (-vertical).min(descent_cap), p) } else { vertical };
        goal = horizontal + f.up * vertical;
        forward_cap = lerp(forward, forward.min(t.landing.speed), p);
    }
    if m.proximity {
        // No descent faster than the upward thrust can stop above the ground (with a reserve),
        // ending at the touchdown speed; the limit also brakes the sink.
        let touch = t.landing.speed * t.landing.landing_share;
        let a = t.descent_reserve * brake_up;
        let safe = (touch * touch + 2.0 * a * f.clearance.max(0.0)).sqrt();
        let vertical = goal.dot(f.up);
        if vertical < -safe {
            goal -= f.up * (vertical + safe);
            descent = f.up * (a * sink / safe);
        }
        // Flying at the ground: the downward part of the goal is cut.
        if sink > 1e-9 && f.clearance / sink < t.proximity_time {
            let vertical = goal.dot(f.up);
            if vertical < 0.0 {
                goal -= f.up * vertical;
            }
        }
    }

    // Coupled: the velocity goal, the hold and the descent brake; decoupled: the stick's thrust
    // and the hold. The blend runs between them.
    let motion_l = f.inv * ((goal - f.v) * t.decay + descent);
    let hold_l = f.inv * hold;
    let mut coupled = motion_l + hold_l;
    if t.anti_drift && goal_stick.length_squared() > 1e-18 && b.clamp(coupled) != coupled {
        let g = f.inv * goal_stick.normalize();
        if let Some(biased) = anti_drift(&b, f.lv, g, f.inv * goal) {
            coupled = biased + hold_l;
        }
    }
    let decoupled = b.along(stick) + hold_l;
    let k = m.coupling;
    let mut asked = coupled * k + decoupled * (1.0 - k);

    // A3: above the cap only the thrust along the velocity is refused (steering across it works).
    // The limit is the larger of the cap in that direction and the speed before the step, so a
    // speed above the cap bleeds by the goal instead of being cut. The hold is not refused.
    // The step's speed after the push is the limit at most: the outward push allowed is what
    // reaches it, given the part of the push across the velocity (that part adds speed too).
    if t.refuse_thrust {
        let speed = f.lv.length();
        if speed > 1e-9 {
            let d = f.lv / speed;
            let cz = if d.z < 0.0 { forward } else { backward };
            let r = 1.0 / ((d.x / cruise).powi(2) + (d.y / cruise).powi(2) + (d.z / cz).powi(2)).sqrt();
            let limit = r.max(speed);
            let push = asked - hold_l;
            let out = push.dot(d);
            let across2 = (push.length_squared() - out * out).max(0.0) * f.dt * f.dt;
            let reach = (limit * limit - across2).max(0.0).sqrt();
            let allowed = (reach - speed) / f.dt;
            if out > allowed {
                asked -= d * (out - allowed);
            }
        }
    }

    let accel = b.clamp(asked);
    LinearOut { accel, cap: forward_cap, saturated: (asked - accel).length() > 1e-6 }
}
