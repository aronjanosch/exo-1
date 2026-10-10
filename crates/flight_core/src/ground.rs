//! The ground rules (#92): what a ship does while it rests on the ground and the SC model does not
//! fly it (no thrust but down, gravity compensation on, slow). It settles straight down until
//! resting, keeps its spot on a slope below the limit until thrust, and turns at the rates of the
//! ground. The SC model has no landing gear yet (#162); this piece stands in for it.
//!
//! The values are `content/tuning/ground.json`, all placeholders (TODO(initiator)).
use crate::limits::Rot;
use crate::{BodyState, Curve, FlightInput, Interp, PlanetEnv};
use glam::{DVec2, DVec3};
use serde::Deserialize;

/// The ground rules' values (`content/tuning/ground.json`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GroundTuning {
    /// m/s: the speed `rate_over_speed` is read at, as a share of this.
    pub cruise_speed: f64,
    /// 1/s: the settle push and the damping to rest decay this times the velocity error.
    pub linear_decay: f64,
    /// rad/s: turn rates at full stick; pitch and yaw share an ellipse.
    pub rate: Rot,
    /// Share of the rates (y) over the speed as a share of `cruise_speed` (x).
    pub rate_over_speed: Curve,
    /// rad/s²: angular acceleration cap per axis.
    pub angular_accel: Rot,
    /// 1/s: the turn rates decay this times the error.
    pub angular_decay: f64,
    /// Degrees: a ship that touches down on ground at most this steep keeps its spot until thrust
    /// (ground hold, #92). Steeper ground: no hold. TODO(initiator): the value; what a ship does on
    /// steeper ground.
    pub landing_slope_limit: f64,
}

impl GroundTuning {
    pub fn from_json(s: &str) -> Result<GroundTuning, String> {
        let t: GroundTuning = content_core::parse_strict("ground.json", s)?;
        t.validate().map_err(|e| format!("ground.json: {e}"))?;
        Ok(t)
    }

    /// Speeds and decays positive, shares above zero, slope limit in range, everything finite.
    pub fn validate(&self) -> Result<(), String> {
        for (what, v) in [("cruise_speed", self.cruise_speed), ("linear_decay", self.linear_decay), ("angular_decay", self.angular_decay)] {
            if !(v > 0.0 && v.is_finite()) {
                return Err(format!("{what} {v} out of range"));
            }
        }
        self.rate.validate("rate")?;
        self.angular_accel.validate("angular_accel")?;
        self.rate_over_speed.validate().map_err(|e| format!("rate_over_speed: {e}"))?;
        if let Some(p) = self.rate_over_speed.points.iter().find(|p| p.y <= 0.0) {
            return Err(format!("rate_over_speed: share {} must be above 0", p.y));
        }
        if !(0.0..=90.0).contains(&self.landing_slope_limit) {
            return Err(format!("landing_slope_limit {} out of range (0 to 90 degrees)", self.landing_slope_limit));
        }
        Ok(())
    }
}

impl Default for GroundTuning {
    fn default() -> Self {
        GroundTuning {
            cruise_speed: 150.0,
            linear_decay: 3.0,
            rate: Rot { pitch: 1.6, yaw: 1.6, roll: 2.4 },
            rate_over_speed: Curve { interp: Interp::Linear, points: vec![DVec2::new(0.0, 0.85), DVec2::new(0.5, 1.0), DVec2::new(1.0, 0.8)] },
            angular_accel: Rot { pitch: 8.0, yaw: 8.0, roll: 14.0 },
            angular_decay: 12.0,
            landing_slope_limit: 35.0,
        }
    }
}

/// A ship set down below `landing_slope_limit` keeps its spot until thrust (#92). Positions are
/// planet frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundHold {
    /// The centre at touchdown: settling, the ship may sink and tip onto the slope along the up
    /// through it, not sideways.
    pub at: DVec3,
    /// Resting, the ship is held at this centre.
    pub rest: Option<DVec3>,
}

/// The ground rules and their state.
#[derive(Clone, Debug)]
pub struct GroundRules {
    pub tuning: GroundTuning,
    /// Seconds the hull has rested on the ground (touching, not sinking) without a break.
    pub ground_time: f64,
    /// Set down below `landing_slope_limit`: held to the spot until thrust (#92).
    pub ground_hold: Option<GroundHold>,
    /// Seconds without hull contact; a hold lets go after `GROUND_HOLD_RELEASE_TIME`.
    contact_lost: f64,
}

impl Default for GroundRules {
    fn default() -> Self {
        GroundRules::new(GroundTuning::default())
    }
}

impl GroundRules {
    pub fn new(tuning: GroundTuning) -> Self {
        GroundRules { tuning, ground_time: 0.0, ground_hold: None, contact_lost: 0.0 }
    }

    /// Thrust or the SC model's flight ends the ground state: no hold, no settle time left.
    pub fn release(&mut self) {
        self.ground_time = 0.0;
        self.ground_hold = None;
        self.contact_lost = 0.0;
    }

    /// m/s: on the ground the assist settles the ship at this speed (no slide, see `step`).
    pub const GROUND_SETTLE_SPEED: f64 = 0.5;
    /// s: resting this long on the ground (not sinking) ends the settle push.
    pub const GROUND_SETTLE_TIME: f64 = 0.3;
    /// m: the slope under the ship is measured this far to each side (about half the hull's width).
    pub const GROUND_SLOPE_SPAN: f64 = 2.0;
    /// m: a held ship found farther than this from its spot was moved by something else (a
    /// teleport, a depenetration); the hold lets go instead of pulling it back.
    pub const GROUND_HOLD_REACH: f64 = 1.0;
    /// m/s: the ground rules (settle, hold) start only this slow along the ground; a brush at speed
    /// is not a landing (#104 point 1).
    pub const GROUND_HOLD_SPEED: f64 = 5.0;
    /// m: half the hull's length; a level ship touching a slope with a corner has its centre up to
    /// this times the slope's tangent above the terrain.
    pub const HULL_HALF_LENGTH: f64 = 4.0;
    /// s: a settling hold lets go when the hull has not touched for this long (a one-step flicker
    /// keeps it; a resting hold keeps the ship on its spot, where the contact may drop out).
    pub const GROUND_HOLD_RELEASE_TIME: f64 = 0.25;

    /// m: the ground rules start only this low above the terrain under the centre: a corner touch
    /// on a slope at the slope limit, plus half a metre.
    pub fn ground_clearance(&self) -> f64 {
        Self::HULL_HALF_LENGTH * self.tuning.landing_slope_limit.min(80.0).to_radians().tan() + 0.5
    }

    /// Low and slow enough with the hull touching for the ground rules (settle, hold).
    fn on_ground(&self, grounded: bool, clearance: f64, up: DVec3, v: DVec3) -> bool {
        grounded && clearance < self.ground_clearance() && (v - up * v.dot(up)).length() < Self::GROUND_HOLD_SPEED
    }

    /// Height of a world point above the terrain under it, m.
    pub fn clearance_at(&self, env: &impl PlanetEnv, world: DVec3) -> f64 {
        let p = env.to_planet(world);
        p.length() - env.radius() - env.height_at(p.normalize())
    }

    /// Slope of the terrain under a point, radians: the angle between the planet's up and the
    /// ground's normal, from the heights `GROUND_SLOPE_SPAN` to each side.
    pub fn ground_slope(&self, env: &impl PlanetEnv, world: DVec3) -> f64 {
        let up = env.to_planet(world).normalize();
        let r = env.radius();
        let ground = |side: DVec3| {
            let dir = (up * r + side * Self::GROUND_SLOPE_SPAN).normalize();
            dir * (r + env.height_at(dir))
        };
        let (a, b) = up.any_orthonormal_pair();
        let normal = (ground(a) - ground(-a)).cross(ground(b) - ground(-b)).normalize();
        normal.dot(up).abs().min(1.0).acos()
    }

    /// Ground hold (#92), the part of the step before the velocity goal. `thrusting`: any stick
    /// input but down. Returns `hold` (low, slow and touching without thrust: settle straight
    /// down) and whether the step settles or rests (`hold` or a ground hold).
    fn ground_rules(&mut self, env: &impl PlanetEnv, origin: DVec3, v: DVec3, up: DVec3, thrusting: bool, grounded: bool, dt: f64) -> (bool, bool) {
        let clearance = self.clearance_at(env, origin);
        let hold = !thrusting && self.on_ground(grounded, clearance, up, v);
        // Resting: on the ground and no longer sinking although pushed (all contacts carry it).
        let sinking = v.dot(up) < -0.05;
        self.ground_time = if grounded && !sinking { self.ground_time + dt } else { 0.0 };
        // Set down below the slope limit, the ship keeps its spot until thrust. Settling still tips
        // it onto the slope; each step the contact turns part of the push sideways (0.55 m on 33
        // degrees in `full`), the hold takes that back.
        let here = env.to_planet(origin);
        let sideways = |d: DVec3| d - up * d.dot(up);
        // Moved away by something else: settling, sideways (it may sink); resting, in any direction
        // (the rest spot would pull a lifted ship back down at distance / dt). Also contact lost
        // for a while, or high above the ground: no hold.
        let strayed = self.ground_hold.is_some_and(|h| match h.rest {
            Some(p) => (here - p).length(),
            None => sideways(here - h.at).length(),
        } > Self::GROUND_HOLD_REACH);
        self.contact_lost = if grounded { 0.0 } else { self.contact_lost + dt };
        // A resting hold keeps the ship exactly on its spot, so the contact may drop out; only a
        // settling one (pushed down) counts contact lost.
        let settling = self.ground_hold.is_some_and(|h| h.rest.is_none());
        let left = settling && self.contact_lost >= Self::GROUND_HOLD_RELEASE_TIME || clearance >= self.ground_clearance();
        if thrusting || strayed || left {
            self.ground_hold = None;
        } else if hold && self.ground_hold.is_none() && self.ground_slope(env, origin) <= self.tuning.landing_slope_limit.to_radians() {
            self.ground_hold = Some(GroundHold { at: here, rest: None });
        }
        if let Some(h) = &mut self.ground_hold
            && h.rest.is_none()
            && self.ground_time >= Self::GROUND_SETTLE_TIME
        {
            // Where it rests, within one step's push of the touchdown spot (pulled further onto it,
            // a slope would put it into the ground).
            h.rest = Some(here);
        }
        (hold, hold || self.ground_hold.is_some())
    }

    /// The velocity goal while on the ground: settle until it rests, tipping onto the slope; then
    /// no push at all (a push on a slope creeps).
    fn ground_goal(&self, up: DVec3) -> DVec3 {
        if self.ground_time < Self::GROUND_SETTLE_TIME { -up * Self::GROUND_SETTLE_SPEED } else { DVec3::ZERO }
    }

    /// The step's new velocity under the ground rules: a ground hold goes back to its spot within
    /// one step, whatever the contacts did last step; a hold without one keeps no sideways speed.
    fn ground_velocity(&self, env: &impl PlanetEnv, origin: DVec3, v: DVec3, up: DVec3, hold: bool, dt: f64) -> DVec3 {
        let here = env.to_planet(origin);
        match self.ground_hold {
            Some(GroundHold { rest: Some(p), .. }) => (p - here) / dt,
            Some(GroundHold { at, rest: None }) => {
                let d = at - here;
                up * v.dot(up) + (d - up * d.dot(up)) / dt
            }
            None if hold => up * v.dot(up),
            None => v,
        }
    }

    /// One step on the ground. Returns the new linear and angular velocity (world); the caller
    /// writes them to the body. The thrusters' share is not modelled: gravity is held and the
    /// velocity is damped towards the goal (settle or rest).
    pub fn step(&mut self, body: &BodyState, input: &FlightInput, env: &impl PlanetEnv, dt: f64) -> (DVec3, DVec3) {
        let (origin, b) = (body.pos, body.rot);
        let v = body.lin_vel;
        let up = env.to_planet(origin).normalize();
        let braking = input.piloted && input.brake;
        let thrusting = !braking && (input.thrust.x.abs() >= 1e-5 || input.thrust.z.abs() >= 1e-5 || input.thrust.y > 1e-5);
        let (hold, settle) = self.ground_rules(env, origin, v, up, thrusting, input.grounded, dt);
        let goal = if settle { self.ground_goal(up) } else { DVec3::ZERO };
        let v = v + ((goal - v) * self.tuning.linear_decay + body.ang_vel.cross(goal)) * dt;
        let v = self.ground_velocity(env, origin, v, up, hold, dt);

        // Turning: target rate per axis from the mouse and the stick, pitch and yaw in an ellipse.
        let t = &self.tuning;
        let over_speed = t.rate_over_speed.eval(v.length() / t.cruise_speed);
        let rate = Rot { pitch: t.rate.pitch * over_speed, yaw: t.rate.yaw * over_speed, roll: t.rate.roll * over_speed };
        let mut pitch = -input.mouse.y / dt + input.turn.x * rate.pitch;
        let mut yaw = -input.mouse.x / dt + input.turn.y * rate.yaw;
        let e = (pitch / rate.pitch).powi(2) + (yaw / rate.yaw).powi(2);
        if e > 1.0 {
            let s = e.sqrt();
            pitch /= s;
            yaw /= s;
        }
        let roll = (input.roll * rate.roll).clamp(-rate.roll, rate.roll);
        let target_w = DVec3::new(pitch, yaw, roll);
        let inv = b.inverse();
        let control = inv * body.ang_vel;
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
        (v, b * (control + step))
    }
}
