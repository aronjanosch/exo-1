//! The axis model (spike 13): a second assisted-flight model next to the classic one in
//! `ShipController::step`, built after the structure of Star Citizen's flight control (structure
//! only; own names, code and values): acceleration limits per axis and direction, one linear and
//! one angular decay, a precision mode near the ground, a G-safety limit per direction, coupled
//! and decoupled. It uses the same inputs, boost capacitor, input ramp and H/C switches as the
//! classic model, and `drag_k`, the ramp and `decouple_time` from `ShipTuning`.
//!
//! All values are TODO(initiator) (`content/tuning/ship_axis.json`).
use crate::{lerp, limit_length, move_towards, parse_tuning, smoothstep, BodyState, FlightInput, PlanetEnv, ShipController};
use glam::DVec3;
use serde::Deserialize;

/// m/s²: one g, for the G-safety limits.
pub const G0: f64 = 9.81;

/// Which model a ship flies (F7, a dev switch like F6; not a tuning value).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlightModel {
    #[default]
    Classic,
    Axis,
}

impl FlightModel {
    pub fn next(self) -> FlightModel {
        match self {
            FlightModel::Classic => FlightModel::Axis,
            FlightModel::Axis => FlightModel::Classic,
        }
    }

    /// The HUD word. TODO(initiator): name and word.
    pub fn label(self) -> &'static str {
        match self {
            FlightModel::Classic => "CLASSIC",
            FlightModel::Axis => "AXIS",
        }
    }
}

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

    fn validate(&self, what: &str) -> Result<(), String> {
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
    fn validate(&self, what: &str) -> Result<(), String> {
        for (n, v) in [("pitch", self.pitch), ("yaw", self.yaw), ("roll", self.roll)] {
            if !(v > 0.0 && v.is_finite()) {
                return Err(format!("{what}.{n} {v} out of range"));
            }
        }
        Ok(())
    }
}

/// Precision mode near the ground: inside a band of terrain clearance the speed cap drops. The
/// clearance counts less the distance a descent needs to stop with the thrust upwards, so a fast
/// descent enters the band early.
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

/// The pilot's G tolerance and whether the assist keeps to it.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GSafety {
    pub enabled: bool,
    /// g per direction of the felt acceleration (the thrust) in ship space; `up` presses the
    /// pilot into the seat, so does `forward`.
    pub limit: Dirs,
}

/// The axis model's values (`content/tuning/ship_axis.json`). TODO(initiator): all of them.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AxisTuning {
    /// m/s: coupled speed for full stick in any direction (the stick is normalised into a ball).
    pub cruise_speed: f64,
    /// m/s: the forward and backward caps at full boost.
    pub boost_speed_forward: f64,
    pub boost_speed_backward: f64,
    /// m/s²: what the thrusters give per axis and direction.
    pub accel: Dirs,
    /// Multipliers on `accel` at full boost (the brake always gets them).
    pub boost_accel: Dirs,
    /// 1/s: the assist asks for this times the velocity error (saturated far from the goal,
    /// exponential close to it).
    pub linear_decay: f64,
    /// rad/s: turn rates at full stick; pitch and yaw share an ellipse.
    pub rate: Rot,
    /// Multipliers on `rate` at full boost.
    pub boost_rate: Rot,
    /// rad/s²: angular acceleration cap per axis.
    pub angular_accel: Rot,
    /// 1/s: like `linear_decay`, for the turn rates.
    pub angular_decay: f64,
    pub precision: Precision,
    pub g_safety: GSafety,
}

impl AxisTuning {
    pub fn from_json(s: &str) -> Result<AxisTuning, String> {
        let t: AxisTuning = parse_tuning("ship_axis.json", s)?;
        t.validate().map_err(|e| format!("ship_axis.json: {e}"))?;
        Ok(t)
    }

    pub fn validate(&self) -> Result<(), String> {
        let pos = |what: &str, v: f64| if v > 0.0 && v.is_finite() { Ok(()) } else { Err(format!("{what} {v} out of range")) };
        pos("cruise_speed", self.cruise_speed)?;
        pos("boost_speed_forward", self.boost_speed_forward)?;
        pos("boost_speed_backward", self.boost_speed_backward)?;
        pos("linear_decay", self.linear_decay)?;
        pos("angular_decay", self.angular_decay)?;
        self.accel.validate("accel")?;
        self.boost_accel.validate("boost_accel")?;
        self.rate.validate("rate")?;
        self.boost_rate.validate("boost_rate")?;
        self.angular_accel.validate("angular_accel")?;
        self.g_safety.limit.validate("g_safety.limit")?;
        let p = &self.precision;
        if !(p.full_below >= 0.0 && p.off_above > p.full_below && p.off_above.is_finite()) {
            return Err(format!("precision: band {} to {} m out of range", p.full_below, p.off_above));
        }
        pos("precision.speed", p.speed)?;
        if !(p.landing_share > 0.0 && p.landing_share <= 1.0) {
            return Err(format!("precision.landing_share {} out of range", p.landing_share));
        }
        if !(p.rate_share > 0.0 && p.rate_share <= 1.0) {
            return Err(format!("precision.rate_share {} out of range", p.rate_share));
        }
        Ok(())
    }
}

impl Default for AxisTuning {
    fn default() -> Self {
        AxisTuning {
            cruise_speed: 150.0,
            boost_speed_forward: 350.0,
            boost_speed_backward: 80.0,
            accel: Dirs { forward: 30.0, backward: 20.0, left: 12.0, right: 12.0, up: 25.0, down: 15.0 },
            boost_accel: Dirs { forward: 2.0, backward: 1.5, left: 1.25, right: 1.25, up: 1.25, down: 1.25 },
            linear_decay: 2.0,
            rate: Rot { pitch: 1.6, yaw: 1.6, roll: 2.4 },
            boost_rate: Rot { pitch: 1.2, yaw: 1.2, roll: 1.0 },
            angular_accel: Rot { pitch: 6.0, yaw: 6.0, roll: 10.0 },
            angular_decay: 8.0,
            precision: Precision { full_below: 15.0, off_above: 80.0, speed: 4.0, landing_share: 0.5, rate_share: 0.7 },
            g_safety: GSafety { enabled: true, limit: Dirs { forward: 8.0, backward: 4.0, left: 4.0, right: 4.0, up: 6.0, down: 3.0 } },
        }
    }
}

/// What the axis model did in its last step (HUD, F3 and scenarios).
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

impl ShipController {
    /// One step of the axis model; same contract as `step` (new linear and angular velocity).
    pub(crate) fn step_axis(&mut self, body: &BodyState, input: &FlightInput, env: &impl PlanetEnv, dt: f64) -> (DVec3, DVec3) {
        // Scripted test input (nobody piloting) is not ramped, as in the classic model.
        let ramped;
        let input = if input.piloted {
            ramped = self.ramp.apply(input, &self.tuning, dt);
            &ramped
        } else {
            input
        };
        let target = if self.coupled { 1.0 } else { 0.0 };
        self.coupling = move_towards(self.coupling, target, if self.tuning.decouple_time > 0.0 { dt / self.tuning.decouple_time } else { 1.0 });
        let t = &self.axis_tuning;
        let (b, origin, v) = (body.rot, body.pos, body.lin_vel);
        let inv = b.inverse();
        let gravity = env.gravity_at(origin);
        let density = env.density_at(origin);
        let pos = env.to_planet(origin);
        let up = pos.normalize();
        self.planet_follow_strength = if self.horizon_follow { env.field_strength_at(origin) } else { 0.0 };
        let horizon_w = if self.horizon_follow { up.cross(v) / pos.length() * self.planet_follow_strength } else { DVec3::ZERO };

        self.brake_active = input.piloted && input.brake;
        // The brake neither uses nor drains the charge (TODO(initiator), #90).
        let want = input.boost && !self.brake_active;
        let strength = if self.boost_stage { self.boost.stage(want) } else { self.boost.step(want, &self.tuning.boost_capacitor, dt) };
        self.boost_strength = strength;
        let stick = if self.brake_active { DVec3::ZERO } else { limit_length(input.thrust, 1.0) };

        self.terrain_clearance = self.clearance_at(env, origin);
        // What the thrust upwards can brake a descent with, and the clearance left after it.
        let sink = (-v.dot(up)).max(0.0);
        let brake = (t.accel.up - gravity.length()).max(1.0);
        let p = 1.0 - smoothstep(t.precision.full_below, t.precision.off_above, self.terrain_clearance - sink * sink / (2.0 * brake));
        let forward_cap = lerp(t.cruise_speed, t.boost_speed_forward, strength);
        let backward_cap = lerp(t.cruise_speed, t.boost_speed_backward, strength);
        self.forward_speed_limit = lerp(forward_cap, forward_cap.min(t.precision.speed), p);

        // Coupled: a velocity goal from the stick, turning with the ship.
        let local_goal = DVec3::new(stick.x * t.cruise_speed, stick.y * t.cruise_speed, stick.z * if stick.z < 0.0 { forward_cap } else { backward_cap });
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
        // On the ground without sideways, forward or upward input: settle straight down until
        // resting, then ask for nothing (the classic model's rule; a push along a tilted hull
        // slid the ship 63 m down a slope).
        let hold = input.grounded && stick.x.abs() < 1e-5 && stick.z.abs() < 1e-5 && stick.y <= 1e-5;
        let sinking = v.dot(up) < -0.05;
        self.ground_time = if input.grounded && !sinking { self.ground_time + dt } else { 0.0 };
        if hold {
            goal = if self.ground_time < Self::GROUND_SETTLE_TIME { -up * Self::GROUND_SETTLE_SPEED } else { DVec3::ZERO };
        }
        self.commanded_speed = goal.length();
        let coupled_accel = (goal - v) * t.linear_decay + body.ang_vel.cross(goal);
        // Decoupled: full thrust along the stick, the velocity is kept (curving with the planet
        // while L is on).
        let boost_share = if self.brake_active { 1.0 } else { strength };
        let limits = t.accel.mul(&Dirs::splat(1.0).zip(&t.boost_accel, |one, m| lerp(one, m, boost_share)));
        let decoupled_accel = b * limits.along(stick) + horizon_w.cross(v);
        let c = if self.brake_active { 1.0 } else { self.coupling };
        let drag = -v * self.tuning.drag_k * density * v.length();
        // What the thrusters can give: the axis limits, then the pilot's tolerance.
        let g_limit = t.g_safety.enabled.then(|| t.g_safety.limit.scaled(G0));
        let fit = |a: DVec3| {
            let l = limits.clamp(a);
            g_limit.map_or(l, |g| g.clamp(l))
        };
        // Assist on: thrust also holds against gravity and drag; coupled and decoupled are each
        // limited, then blended, so the damping fades out with the blend. Off: thrust along the
        // stick only.
        let (asked, local) = if self.hover_assist || self.brake_active {
            let (coupled, decoupled) = (inv * (coupled_accel - gravity - drag), inv * (decoupled_accel - gravity - drag));
            (coupled * c + decoupled * (1.0 - c), fit(coupled) * c + fit(decoupled) * (1.0 - c))
        } else {
            let a = limits.along(stick);
            (a, fit(a))
        };
        self.axis.saturated = (asked - local).length() > 1e-6;
        self.axis.felt_g = local.length() / G0;
        self.axis.precision = p;
        let v = v + (b * local + gravity + drag) * dt;

        // Rotation: target rate per axis, pitch and yaw in an ellipse.
        let boost_rate = |r: f64, m: f64| r * lerp(1.0, m, strength) * lerp(1.0, t.precision.rate_share, p);
        let rate = Rot { pitch: boost_rate(t.rate.pitch, t.boost_rate.pitch), yaw: boost_rate(t.rate.yaw, t.boost_rate.yaw), roll: boost_rate(t.rate.roll, t.boost_rate.roll) };
        let mut pitch = -input.mouse.y / dt + input.turn.x * rate.pitch;
        let mut yaw = -input.mouse.x / dt + input.turn.y * rate.yaw;
        let e = (pitch / rate.pitch).powi(2) + (yaw / rate.yaw).powi(2);
        if e > 1.0 {
            let s = e.sqrt();
            pitch /= s;
            yaw /= s;
        }
        // G-safety: a coupled turn at speed s needs s * rate sideways; keep that within the
        // tolerance in the direction the turn pulls.
        self.axis.rate_capped = false;
        if t.g_safety.enabled && self.hover_assist && c > 0.0 {
            let lv = inv * v;
            let g = t.g_safety.limit.scaled(G0);
            let (pitch_speed, yaw_speed) = (DVec3::new(0.0, lv.y, lv.z).length(), DVec3::new(lv.x, 0.0, lv.z).length());
            let cap = |r: f64, speed: f64, pos: f64, neg: f64| {
                let lim = if r >= 0.0 { pos } else { neg } / speed.max(1e-6);
                lerp(r.abs(), r.abs().min(lim), c).copysign(r)
            };
            // Nose up needs thrust up, nose left thrust to the left.
            let (p2, y2) = (cap(pitch, pitch_speed, g.up, g.down), cap(yaw, yaw_speed, g.left, g.right));
            self.axis.rate_capped = (p2 - pitch).abs() > 1e-9 || (y2 - yaw).abs() > 1e-9;
            (pitch, yaw) = (p2, y2);
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
