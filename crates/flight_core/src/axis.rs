//! The axis flight model (spike 13; the only one since the classic model went, #144):
//! `ShipController::step`, built after the structure of Star Citizen's flight control (structure
//! only; own names, code and values): acceleration limits per axis and direction, one linear and
//! one angular decay, a precision mode near the ground, a G-safety limit per direction, coupled
//! and decoupled.
//!
//! All values are TODO(initiator) (`content/tuning/ship.json`).
use crate::{lerp, limit_length, smoothstep, BodyState, FlightInput, PlanetEnv, ShipController};
use glam::{DQuat, DVec3};
use serde::Deserialize;

/// m/s²: one g, for the G-safety limits.
pub const G0: f64 = 9.81;

/// One number per axis and direction in ship space: forward is -Z, right +X, up +Y.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Dirs {
    pub forward: f64,
    pub backward: f64,
    pub left: f64,
    pub right: f64,
    pub up: f64,
    pub down: f64,
}

impl Dirs {
    pub fn splat(x: f64) -> Dirs {
        Dirs { forward: x, backward: x, left: x, right: x, up: x, down: x }
    }

    fn zip(&self, o: &Dirs, f: impl Fn(f64, f64) -> f64) -> Dirs {
        Dirs {
            forward: f(self.forward, o.forward),
            backward: f(self.backward, o.backward),
            left: f(self.left, o.left),
            right: f(self.right, o.right),
            up: f(self.up, o.up),
            down: f(self.down, o.down),
        }
    }

    pub fn mul(&self, o: &Dirs) -> Dirs {
        self.zip(o, |a, b| a * b)
    }

    pub fn scaled(&self, s: f64) -> Dirs {
        self.zip(self, |a, _| a * s)
    }

    /// The box these limits span, applied to a local vector (x right, y up, z backward).
    pub fn clamp(&self, v: DVec3) -> DVec3 {
        DVec3::new(v.x.clamp(-self.left, self.right), v.y.clamp(-self.down, self.up), v.z.clamp(-self.forward, self.backward))
    }

    /// Each component of a local vector times the limit in its direction (a stick -1..1 per axis
    /// to the full thrust that way).
    pub fn along(&self, v: DVec3) -> DVec3 {
        DVec3::new(
            v.x * if v.x >= 0.0 { self.right } else { self.left },
            v.y * if v.y >= 0.0 { self.up } else { self.down },
            v.z * if v.z >= 0.0 { self.backward } else { self.forward },
        )
    }

    /// The smaller limit per direction (two boxes overlapped).
    pub fn min(&self, o: &Dirs) -> Dirs {
        self.zip(o, f64::min)
    }

    /// How far the box reaches along a local unit vector (the most thrust that way).
    pub fn support(&self, u: DVec3) -> f64 {
        self.along(u).dot(u)
    }

    pub(crate) fn validate(&self, what: &str) -> Result<(), String> {
        for (n, v) in [("forward", self.forward), ("backward", self.backward), ("left", self.left), ("right", self.right), ("up", self.up), ("down", self.down)] {
            if !(v > 0.0 && v.is_finite()) {
                return Err(format!("{what}.{n} {v} out of range"));
            }
        }
        Ok(())
    }
}

/// One number per rotation axis: pitch (about +X, nose up), yaw (about +Y, nose left), roll.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rot {
    pub pitch: f64,
    pub yaw: f64,
    pub roll: f64,
}

impl Rot {
    pub(crate) fn validate(&self, what: &str) -> Result<(), String> {
        for (n, v) in [("pitch", self.pitch), ("yaw", self.yaw), ("roll", self.roll)] {
            if !(v > 0.0 && v.is_finite()) {
                return Err(format!("{what}.{n} {v} out of range"));
            }
        }
        Ok(())
    }
}

/// Landing mode (switched by hand, `ShipController::landing_mode`): inside a band of terrain
/// clearance the speed cap drops. The clearance counts less the distance a descent needs to stop
/// with the thrust upwards, so a fast descent enters the band early. Without landing mode only the
/// descent is held to what the thrust can still stop above the ground.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Precision {
    /// m: full precision at and below this clearance.
    pub full_below: f64,
    /// m: off above this clearance; smooth in between.
    pub off_above: f64,
    /// m/s: speed cap at full precision. Climbing away from the ground is not capped.
    pub speed: f64,
    /// Share of `speed` for the descent (the touchdown speed).
    pub landing_share: f64,
    /// Share of the rotation rates at full precision.
    pub rate_share: f64,
}

/// Speed caps out of the atmosphere; blended with the caps of `ShipTuning` by the air density.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpaceCaps {
    pub cruise_speed: f64,
    pub boost_speed_forward: f64,
    pub boost_speed_backward: f64,
}

/// The pilot's G tolerance and whether the assist keeps to it.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GSafety {
    /// The thrust stays inside `limit`.
    pub enabled: bool,
    /// Also scale pitch and yaw so a coupled turn (rate x velocity) stays inside `limit`: the nose
    /// follows the velocity. Off, the nose turns at its rate and the velocity slips behind.
    pub cap_turns: bool,
    /// g per direction of the felt acceleration (the thrust) in ship space; `up` presses the
    /// pilot into the seat, so does `forward`.
    pub limit: Dirs,
}

/// What the flight model did in its last step (HUD, F3 and scenarios).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisState {
    /// 0..1, 1 = full precision mode.
    pub precision: f64,
    /// The felt acceleration (the thrust), g.
    pub felt_g: f64,
    /// The thrusters gave less than the assist asked (an axis limit or G-safety).
    pub saturated: bool,
    /// G-safety lowered the pitch or yaw rate.
    pub rate_capped: bool,
}

/// B2: the local thrust of a brake along the velocity `lv` (local): `hold` (cancels gravity and
/// drag) plus the unit vector against `lv` times the largest `k` that keeps the thrust inside the
/// box `b`, capped at `speed * decay`. `None` when the ship is still or `hold` is outside the box
/// (the caller then brakes per axis, as before).
pub(crate) fn brake_along(b: &Dirs, hold: DVec3, lv: DVec3, decay: f64) -> Option<DVec3> {
    const EPS: f64 = 1e-9;
    let speed = lv.length();
    if speed < EPS {
        return None;
    }
    let inside = hold.x >= -b.left - EPS && hold.x <= b.right + EPS && hold.y >= -b.down - EPS && hold.y <= b.up + EPS && hold.z >= -b.forward - EPS && hold.z <= b.backward + EPS;
    if !inside {
        return None;
    }
    let u = -lv / speed;
    let mut k = speed * decay;
    for (h, c, lo, hi) in [(hold.x, u.x, -b.left, b.right), (hold.y, u.y, -b.down, b.up), (hold.z, u.z, -b.forward, b.backward)] {
        if c > 1e-12 {
            k = k.min((hi - h) / c);
        } else if c < -1e-12 {
            k = k.min((lo - h) / c);
        }
    }
    Some(hold + u * k.max(0.0))
}

/// A3: the velocity `v` (world) held to the speed cap in its own direction. The cap is the
/// ellipsoid the coupled stick goal spans (`cruise` sideways and up or down, `forward` or
/// `backward` along the ship's Z); the limit is the larger of that cap and the speed before the
/// step, so a speed above the cap is not cut. The direction is kept.
/// TODO(initiator): the landing precision band (landing mode) is not part of this cap; it still
/// limits the goal only.
pub(crate) fn refuse_thrust(v: DVec3, inv: DQuat, speed_before: f64, cruise: f64, forward: f64, backward: f64) -> DVec3 {
    let lv = inv * v;
    let s = lv.length();
    if s < 1e-9 {
        return v;
    }
    let d = lv / s;
    let cz = if d.z < 0.0 { forward } else { backward };
    let r = 1.0 / ((d.x / cruise).powi(2) + (d.y / cruise).powi(2) + (d.z / cz).powi(2)).sqrt();
    let limit = r.max(speed_before);
    if s <= limit { v } else { v * (limit / s) }
}

impl ShipController {
    /// Share of the upward thrust the descent limit counts on (the rest is reserve for the assist's
    /// lag and for terrain rising under the ship).
    pub const DESCENT_RESERVE: f64 = 0.6;

    /// One physics step. Returns the new linear and angular
    /// velocity; the caller writes them to the body before integration.
    pub fn step(&mut self, body: &BodyState, input: &FlightInput, env: &impl PlanetEnv, dt: f64) -> (DVec3, DVec3) {
        let (input, strength) = self.begin_step(input, dt);
        let input = &input;
        let (b, origin, v) = (body.rot, body.pos, body.lin_vel);
        let inv = b.inverse();
        let gravity = env.gravity_at(origin);
        let density = env.density_at(origin);
        let pos = env.to_planet(origin);
        let up = pos.normalize();
        self.planet_follow_strength = if self.horizon_follow { env.field_strength_at(origin) } else { 0.0 };
        let horizon_w = if self.horizon_follow { up.cross(v) / pos.length() * self.planet_follow_strength } else { DVec3::ZERO };

        let stick = if self.brake_active { DVec3::ZERO } else { limit_length(input.thrust, 1.0) };
        let assist = self.hover_assist || self.brake_active;
        self.terrain_clearance = self.clearance_at(env, origin);
        // On the ground without sideways, forward or upward input: settle straight down until
        // resting, then ask for nothing; set down below the slope limit, keep the spot (#92). A
        // push along a tilted hull slid the ship 63 m down a slope.
        let thrusting = stick.x.abs() >= 1e-5 || stick.z.abs() >= 1e-5 || stick.y > 1e-5;
        let (hold, settle) = if assist {
            self.ground_rules(env, origin, v, up, thrusting, input.grounded, dt)
        } else {
            self.ground_hold = None;
            self.ground_time = 0.0;
            (false, false)
        };
        let t = &self.tuning;
        let boost_share = if self.brake_active { 1.0 } else { strength };
        // Thrusters lose thrust in air; the caps are higher out of it.
        let limits = t.accel.scaled(lerp(1.0, t.atmosphere_thrust, density)).mul(&Dirs::splat(1.0).zip(&t.boost_accel, |one, m| lerp(one, m, boost_share)));
        let cruise = lerp(t.space.cruise_speed, t.cruise_speed, density);
        let (boost_forward, boost_backward) = (lerp(t.space.boost_speed_forward, t.boost_speed_forward, density), lerp(t.space.boost_speed_backward, t.boost_speed_backward, density));
        let g_limit = t.g_safety.enabled.then(|| t.g_safety.limit.scaled(G0));
        let thrust_box = g_limit.map_or(limits, |g| limits.min(&g));

        // What the thrust upwards (in the ship's attitude) can brake a descent with, and the
        // clearance left after it.
        let sink = (-v.dot(up)).max(0.0);
        let brake = (thrust_box.support(inv * up) - gravity.length()).max(1.0);
        let p = if self.landing_mode { 1.0 - smoothstep(t.precision.full_below, t.precision.off_above, self.terrain_clearance - sink * sink / (2.0 * brake)) } else { 0.0 };
        let forward_cap = lerp(cruise, boost_forward, strength);
        let backward_cap = lerp(cruise, boost_backward, strength);
        // No limit shown with the assist off (#110 point 5).
        self.forward_speed_limit = if assist { lerp(forward_cap, forward_cap.min(t.precision.speed), p) } else { 0.0 };

        // Coupled: a velocity goal from the stick, turning with the ship.
        let local_goal = DVec3::new(stick.x * cruise, stick.y * cruise, stick.z * if stick.z < 0.0 { forward_cap } else { backward_cap });
        let mut goal = b * local_goal;
        if p > 0.0 {
            // Precision mode caps the speed along the ground and the descent, not the climb.
            let vertical = goal.dot(up);
            let along = goal - up * vertical;
            let l = along.length();
            let along = if l > 1e-9 { along * (lerp(l, l.min(t.precision.speed), p) / l) } else { along };
            let descent = t.precision.speed * t.precision.landing_share;
            let vertical = if vertical < 0.0 { -lerp(-vertical, (-vertical).min(descent), p) } else { vertical };
            goal = along + up * vertical;
        }
        // Always, landing mode or not: no descent faster than the thrust upwards can stop above
        // the ground (with a reserve), ending at the landing mode's touchdown speed. While that
        // limit holds the goal, the assist also gets the braking the limit's curve asks for
        // (a * sink / limit), so it follows without the decay's lag (that touched down at 6 m/s).
        let touchdown = t.precision.speed * t.precision.landing_share;
        let a = Self::DESCENT_RESERVE * brake;
        let safe_sink = (touchdown * touchdown + 2.0 * a * self.terrain_clearance.max(0.0)).sqrt();
        let vertical = goal.dot(up);
        let mut descent_brake = DVec3::ZERO;
        if vertical < -safe_sink {
            // The ship's own down input points straight down while the limit holds: on a tilted
            // hull part of it pointed sideways, and a down stick near the ground drifted the ship
            // 7.4 m/s into a slide (#144). Forward and sideways input stay.
            let down = b * DVec3::new(0.0, (inv * goal).y.min(0.0), 0.0);
            goal += up * down.dot(up) - down;
            goal -= up * (goal.dot(up) + safe_sink).min(0.0);
            descent_brake = up * (a * sink / safe_sink);
        }
        if settle {
            goal = self.ground_goal(up);
        }
        self.commanded_speed = if assist { goal.length() } else { 0.0 };
        let coupled_accel = (goal - v) * t.linear_decay + body.ang_vel.cross(goal) + descent_brake;
        // Decoupled: full thrust along the stick, the velocity is kept (curving with the planet
        // while L is on).
        // Decoupled thrust stops at the same caps per axis (it ran away and was hard to correct).
        let lv = inv * v;
        let caps = DVec3::new(cruise, cruise, if stick.z < 0.0 { forward_cap } else { backward_cap });
        let free = |s: f64, v: f64, cap: f64| if s * v > 0.0 && v.abs() >= cap { 0.0 } else { s };
        let capped_stick = DVec3::new(free(stick.x, lv.x, caps.x), free(stick.y, lv.y, caps.y), free(stick.z, lv.z, caps.z));
        let decoupled_accel = b * limits.along(capped_stick) + horizon_w.cross(v);
        // The brake and the ground rules always damp.
        let c = if self.brake_active || settle { 1.0 } else { self.coupling };
        let drag = -v * self.tuning.drag_k * density * v.length();
        // What the thrusters can give: the axis limits and the pilot's tolerance.
        let fit = |a: DVec3| thrust_box.clamp(a);
        // Assist on: thrust also holds against gravity and drag; coupled and decoupled are each
        // limited, then blended, so the damping fades out with the blend. Off: thrust along the
        // stick only.
        // B2: the brake along the velocity, when the ground rules do not settle the ship.
        let held_brake = if self.brake_keeps_heading && self.brake_active && assist && !settle { brake_along(&thrust_box, inv * -(gravity + drag), inv * v, t.linear_decay) } else { None };
        let (asked, local) = if assist {
            let (coupled, decoupled) = (held_brake.unwrap_or(inv * (coupled_accel - gravity - drag)), inv * (decoupled_accel - gravity - drag));
            (coupled * c + decoupled * (1.0 - c), fit(coupled) * c + fit(decoupled) * (1.0 - c))
        } else {
            let a = limits.along(stick);
            (a, fit(a))
        };
        self.axis.saturated = (asked - local).length() > 1e-6;
        self.axis.felt_g = local.length() / G0;
        self.axis.precision = if assist { p } else { 0.0 };
        let v = self.ground_velocity(env, origin, v + (b * local + gravity + drag) * dt, up, hold, dt);
        // A3: the cap refuses thrust (not while the ground rules settle the ship).
        let v = if self.cap_refuses_thrust && assist && !settle { refuse_thrust(v, inv, body.lin_vel.length(), cruise, forward_cap, backward_cap) } else { v };

        // Rotation: target rate per axis, pitch and yaw in an ellipse.
        let over_speed = t.rate_over_speed.eval(v.length() / cruise);
        let boost_rate = |r: f64, m: f64| r * lerp(1.0, m, strength) * lerp(1.0, t.precision.rate_share, p) * over_speed;
        let rate = Rot { pitch: boost_rate(t.rate.pitch, t.boost_rate.pitch), yaw: boost_rate(t.rate.yaw, t.boost_rate.yaw), roll: boost_rate(t.rate.roll, t.boost_rate.roll) };
        let mut pitch = -input.mouse.y / dt + input.turn.x * rate.pitch;
        let mut yaw = -input.mouse.x / dt + input.turn.y * rate.yaw;
        let e = (pitch / rate.pitch).powi(2) + (yaw / rate.yaw).powi(2);
        if e > 1.0 {
            let s = e.sqrt();
            pitch /= s;
            yaw /= s;
        }
        // G-safety: a coupled turn needs rate x velocity to keep the velocity on the nose, on top
        // of holding against gravity; scale pitch and yaw so that stays inside the tolerance.
        self.axis.rate_capped = false;
        if let Some(g) = g_limit
            && t.g_safety.cap_turns
            && self.hover_assist
            && c > 0.0
        {
            let (lv, hold_up) = (inv * v, inv * -gravity);
            let turn = DVec3::new(pitch, yaw, 0.0).cross(lv);
            let mut s: f64 = 1.0;
            for (k, base, pos, neg) in [(turn.x, hold_up.x, g.right, g.left), (turn.y, hold_up.y, g.up, g.down), (turn.z, hold_up.z, g.backward, g.forward)] {
                if k > 1e-12 {
                    s = s.min((pos - base) / k);
                } else if k < -1e-12 {
                    s = s.min((neg + base) / -k);
                }
            }
            let scale = lerp(1.0, s.clamp(0.0, 1.0), c);
            self.axis.rate_capped = scale < 1.0 - 1e-9;
            pitch *= scale;
            yaw *= scale;
        }
        let roll = (input.roll * rate.roll).clamp(-rate.roll, rate.roll);
        let target_w = DVec3::new(pitch, yaw, roll);
        // Smooth the player's rotation, not the changing planet frame.
        let control = inv * (body.ang_vel - self.horizon_w);
        let err = target_w - control;
        let a = DVec3::new(
            (err.x * t.angular_decay).clamp(-t.angular_accel.pitch, t.angular_accel.pitch),
            (err.y * t.angular_decay).clamp(-t.angular_accel.yaw, t.angular_accel.yaw),
            (err.z * t.angular_decay).clamp(-t.angular_accel.roll, t.angular_accel.roll),
        );
        // Never past the target in one step.
        let step = DVec3::new(
            (a.x * dt).clamp(-err.x.abs(), err.x.abs()),
            (a.y * dt).clamp(-err.y.abs(), err.y.abs()),
            (a.z * dt).clamp(-err.z.abs(), err.z.abs()),
        );
        self.horizon_w = horizon_w;
        (v, b * (control + step) + horizon_w)
    }
}
