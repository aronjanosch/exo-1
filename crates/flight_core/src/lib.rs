//! flight_core: the assisted-flight ship controller and the arcade planet field,
//! ported from the Godot spikes (`spikes/planet/ship.gd`, `planet_field.gd`,
//! `main.gd`). Plain Rust, f64, glam; no Bevy types. Godot conventions: Y up,
//! -Z forward, right-handed, angular velocity in world space.
//!
//! All numbers are spike test values (assumptions for testing, not design).
pub mod axis;
pub mod camera;
pub mod hud;

pub use axis::{AxisState, Dirs, GSafety, Precision, Rot, SpaceCaps, G0};

use glam::{DQuat, DVec2, DVec3};
use serde::Deserialize;

/// Godot's chase camera sits at (0, 5.5, 17) in ship space, pitched by this.
pub const CHASE_CAMERA_PITCH_DEG: f64 = -10.0;
pub const CHASE_CAMERA_OFFSET: DVec3 = DVec3::new(0.0, 5.5, 17.0);

/// Godot's `smoothstep`: Hermite step, clamped.
pub fn smoothstep(from: f64, to: f64, x: f64) -> f64 {
    if (from - to).abs() < 1e-9 {
        return if x <= from { 0.0 } else { 1.0 };
    }
    let s = ((x - from) / (to - from)).clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

/// Godot's `Vector3.limit_length`.
pub fn limit_length(v: DVec3, max: f64) -> DVec3 {
    let l = v.length();
    if l > 0.0 && max < l { v / l * max } else { v }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Arcade planetary influence: full inside atmosphere, smoothly absent in space.
/// Heights are metres above the reference sphere, independent of terrain.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub surface_gravity: f64,    // m/s², spike value
    pub atmosphere_height: f64,  // m, start value, tune by feel
    pub gravity_end_height: f64, // m, no planetary influence above this altitude
}

impl Default for Field {
    fn default() -> Self {
        Field { surface_gravity: 9.81, atmosphere_height: 1200.0, gravity_end_height: 6000.0 }
    }
}

impl Field {
    pub fn strength(&self, altitude: f64) -> f64 {
        // Keep the interval valid even if provisional values are misordered.
        let end = self.gravity_end_height.max(self.atmosphere_height + 1.0);
        1.0 - smoothstep(self.atmosphere_height, end, altitude)
    }
}

/// What the controller needs from a planet. Positions are world positions
/// unless a name says otherwise; `to_planet` converts to planet-relative.
pub trait PlanetEnv {
    fn to_planet(&self, world: DVec3) -> DVec3;
    fn radius(&self) -> f64;
    /// Terrain height above the base radius along a unit direction.
    fn height_at(&self, dir: DVec3) -> f64;
    fn field(&self) -> &Field;

    /// Shared gravity/planet-follow envelope.
    fn field_strength_at(&self, world: DVec3) -> f64 {
        let altitude = self.to_planet(world).length() - self.radius();
        self.field().strength(altitude)
    }
    /// Arcade radial gravity: full in atmosphere, softly fading to zero in space.
    fn gravity_at(&self, world: DVec3) -> DVec3 {
        -self.to_planet(world).normalize_or_zero() * self.field().surface_gravity * self.field_strength_at(world)
    }
    /// Atmosphere density 0..1: full at the surface, gone at atmosphere_height.
    fn density_at(&self, world: DVec3) -> f64 {
        let alt = self.to_planet(world).length() - self.radius();
        1.0 - smoothstep(0.0, self.field().atmosphere_height, alt)
    }
}

/// Localized artificial gravity (LAG) in the cabin. Off while the ship is landed, so a parked ship
/// on a slope is just a slope under planet gravity; on in flight. G switches it by hand while
/// landed (later one of the ship systems). The field comes up and goes down over `ramp_time`.
#[derive(Clone, Copy, Debug)]
pub struct Lag {
    /// 0..1, share of the ship's gravity in the cabin (the rest is planet gravity).
    pub level: f64,
    pub landed: bool,
    /// Switched on by hand while landed; cleared when the ship takes off or lands.
    pub manual_on: bool,
    pub ramp_time: f64,
    pub g: f64,
}

impl Default for Lag {
    fn default() -> Self {
        // Assumed values: 1 s ramp (initiator agreed to "about 1 s"), Earth gravity.
        Lag { level: 0.0, landed: true, manual_on: false, ramp_time: 1.0, g: 9.81 }
    }
}

impl Lag {
    /// Landed: the hull touches the ground below `LANDED_SPEED` m/s; airborne again without contact
    /// above `AIRBORNE_CLEARANCE` m.
    pub const LANDED_SPEED: f64 = 0.3;
    pub const AIRBORNE_CLEARANCE: f64 = 2.0;

    /// Fully on, for a ship whose LAG state is not known (another player's ship until it is sent).
    pub fn full() -> Lag {
        Lag { level: 1.0, landed: false, ..Lag::default() }
    }

    pub fn is_on(&self) -> bool {
        !self.landed || self.manual_on
    }

    /// G: switches the field by hand, only while landed.
    pub fn toggle(&mut self) {
        if self.landed {
            self.manual_on = !self.manual_on;
        }
    }

    /// One step. `grounded`: the hull touches the ground; `clearance` is the ship's height above
    /// the terrain under its centre (m). Contact decides landing (a ship hovering low has none, a
    /// parked one over a dip has), the clearance only taking off.
    pub fn step(&mut self, grounded: bool, clearance: f64, speed: f64, dt: f64) {
        let landed = if self.landed { grounded || clearance < Self::AIRBORNE_CLEARANCE } else { grounded && speed < Self::LANDED_SPEED };
        if landed != self.landed {
            self.landed = landed;
            self.manual_on = false;
        }
        let target = if self.is_on() { 1.0 } else { 0.0 };
        self.level = move_towards(self.level, target, dt / self.ramp_time);
    }

    /// Gravity in the cabin, world space: the planet's turned towards the ship's floor by `level`,
    /// strength blended. Turning instead of adding keeps an upside-down ship from cancelling the
    /// two halfway.
    pub fn gravity(&self, ship_up: DVec3, planet_gravity: DVec3) -> DVec3 {
        let ship = -ship_up * self.g;
        let pg = planet_gravity.length();
        if self.level >= 1.0 || pg < 1e-9 {
            return ship * self.level;
        }
        let from = planet_gravity / pg;
        let dir = DQuat::IDENTITY.slerp(DQuat::from_rotation_arc(from, -ship_up), self.level) * from;
        dir * lerp(pg, self.g, self.level)
    }
}

fn move_towards(x: f64, target: f64, step: f64) -> f64 {
    x + (target - x).clamp(-step, step)
}

/// Boost capacitor tuning (`ship.json`, `boost_capacitor`, #90). Starting values, not design.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BoostCapacitorTuning {
    /// Seconds from full to empty while boosting; 0 = no capacitor, boost is always there (#24).
    pub drain_time: f64,
    /// Seconds from empty to full.
    pub recharge_time: f64,
    /// Seconds after the last use before recharging starts.
    pub recharge_delay: f64,
    /// Charge 0..1 needed to start a boost; a running boost lasts until empty.
    pub start_charge: f64,
    /// Boost strength 0..1 (of the full boost) over the charge 0..1.
    pub strength_curve: Curve,
    /// Held through empty, the boost starts again by itself at the start charge (weak pulses);
    /// false: it needs a new press (#104 point 9). TODO(initiator): which one.
    pub restart_while_held: bool,
}

impl BoostCapacitorTuning {
    pub fn validate(&self) -> Result<(), String> {
        let bad = |what: &str, v: f64| Err(format!("boost_capacitor: {what} {v} out of range"));
        if !(self.drain_time >= 0.0 && self.drain_time.is_finite()) {
            return bad("drain_time", self.drain_time);
        }
        if !(self.recharge_time > 0.0 && self.recharge_time.is_finite()) {
            return bad("recharge_time", self.recharge_time);
        }
        if !(self.recharge_delay >= 0.0 && self.recharge_delay.is_finite()) {
            return bad("recharge_delay", self.recharge_delay);
        }
        if !(0.0..=1.0).contains(&self.start_charge) {
            return bad("start_charge", self.start_charge);
        }
        self.strength_curve.validate().map_err(|e| format!("boost_capacitor: strength_curve: {e}"))
    }
}

impl Default for BoostCapacitorTuning {
    fn default() -> Self {
        BoostCapacitorTuning {
            drain_time: 3.0,
            recharge_time: 6.0,
            recharge_delay: 1.0,
            start_charge: 0.2,
            strength_curve: Curve { interp: Interp::Linear, points: vec![DVec2::new(0.0, 0.0), DVec2::new(1.0, 1.0)] },
            restart_while_held: false,
        }
    }
}

/// The boost's charge meter (#90): drains while boosting, recharges after a pause.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoostCapacitor {
    /// 0..1, starts full.
    pub charge: f64,
    /// A boost is running (started at or above the start charge, ends when released or empty).
    pub active: bool,
    /// Seconds since the last use.
    idle: f64,
    /// Ran empty while held: the next boost needs a new press (unless `restart_while_held`).
    needs_release: bool,
}

impl Default for BoostCapacitor {
    fn default() -> Self {
        BoostCapacitor { charge: 1.0, active: false, idle: 0.0, needs_release: false }
    }
}

impl BoostCapacitor {
    pub fn with_charge(charge: f64) -> Self {
        BoostCapacitor { charge, ..BoostCapacitor::default() }
    }

    /// A boost runs or could start now (charge at or above the start charge).
    pub fn ready(&self, t: &BoostCapacitorTuning) -> bool {
        self.active || self.charge > 0.0 && self.charge >= t.start_charge
    }

    /// The speed stage of #24: full boost while held; the meter stays as it is (F6 must not refill
    /// it, #104 point 8).
    pub fn stage(&mut self, want: bool) -> f64 {
        self.active = want;
        if want { 1.0 } else { 0.0 }
    }

    /// One step with boost held (`held`) while the brake may block it: the brake stops the boost
    /// but is no new press for a boost that ran empty.
    pub fn step_braking(&mut self, held: bool, braking: bool, t: &BoostCapacitorTuning, dt: f64) -> f64 {
        let latched = self.needs_release && held;
        let strength = self.step(held && !braking, t, dt);
        self.needs_release |= latched;
        strength
    }

    /// One step with boost held (`want`) or not; returns the boost strength 0..1 for this step.
    pub fn step(&mut self, want: bool, t: &BoostCapacitorTuning, dt: f64) -> f64 {
        if t.drain_time <= 0.0 {
            return self.stage(want);
        }
        if !want {
            self.active = false;
            self.needs_release = false;
        } else if !self.needs_release && self.ready(t) {
            self.active = true;
        }
        if self.active {
            let strength = t.strength_curve.eval(self.charge).clamp(0.0, 1.0);
            self.charge = (self.charge - dt / t.drain_time).max(0.0);
            self.idle = 0.0;
            if self.charge == 0.0 {
                self.active = false;
                self.needs_release = !t.restart_while_held;
            }
            return strength;
        }
        self.idle += dt;
        if self.idle >= t.recharge_delay {
            self.charge = (self.charge + dt / t.recharge_time).min(1.0);
        }
        0.0
    }
}

/// Rigid-body state. `integrate` is the test fixture's integrator: equivalent
/// to Jolt with no contacts, no engine gravity and no damping.
#[derive(Clone, Copy, Debug)]
pub struct BodyState {
    pub pos: DVec3,
    pub rot: DQuat,
    pub lin_vel: DVec3,
    pub ang_vel: DVec3,
}

impl Default for BodyState {
    fn default() -> Self {
        BodyState { pos: DVec3::ZERO, rot: DQuat::IDENTITY, lin_vel: DVec3::ZERO, ang_vel: DVec3::ZERO }
    }
}

impl BodyState {
    pub fn integrate(&mut self, dt: f64) {
        self.pos += self.lin_vel * dt;
        self.rot = (DQuat::from_scaled_axis(self.ang_vel * dt) * self.rot).normalize();
    }
}

/// One physics step of input. `thrust` is local: x right, y up, z = -W.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlightInput {
    pub thrust: DVec3,
    pub roll: f64,
    pub boost: bool,
    pub brake: bool,
    /// Mouse movement accumulated this step, radians (Godot's `_mouse`): x yaw, y pitch.
    pub mouse: DVec2,
    /// Stick deflection -1..1: x pitch (nose up), y yaw (nose left); turns at deflection times
    /// `turn_rate` (virtual-joystick mouse, pad).
    pub turn: DVec2,
    pub piloted: bool,
    /// The hull touches the ground (contacts, from the physics).
    pub grounded: bool,
}

/// The mouse as a virtual joystick: moving the mouse moves an offset (an angle, mouse axes:
/// x right, y down) that stays where it is; its distance from the centre past a dead zone is the
/// deflection.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VirtualStick {
    pub offset: DVec2,
}

impl VirtualStick {
    pub fn push(&mut self, delta: DVec2, max_angle: f64) {
        self.offset = (self.offset + delta).clamp_length_max(max_angle);
    }

    /// Deflection as `FlightInput::turn` (x pitch, y yaw): zero inside `deadzone`, 1 at
    /// `max_angle`, through `curve` (identity without one).
    pub fn deflection(&self, deadzone: f64, max_angle: f64, curve: Option<&Curve>) -> DVec2 {
        let r = self.offset.length();
        if r <= deadzone || max_angle <= deadzone {
            return DVec2::ZERO;
        }
        let m = ((r - deadzone) / (max_angle - deadzone)).min(1.0);
        let m = curve.map_or(m, |c| c.eval(m));
        let d = self.offset / r * m;
        // Mouse right turns the nose right (negative yaw), mouse up (negative y) lifts it.
        DVec2::new(-d.y, -d.x)
    }
}

/// Ship input ramps to full deflection over a time instead of snapping (#25): per axis the
/// progress runs from 0 to 1 over the ramp time while the axis is held in one direction, and the
/// output is the input capped at `curve(progress)`. Release or a reversal starts over.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputRamp {
    progress: [f64; 6],
    /// Ramped input of the last step: thrust x, y, z, roll, turn x, turn y.
    pub out: [f64; 6],
}

impl InputRamp {
    fn axis(&mut self, i: usize, target: f64, dt: f64, time: f64, curve: &Curve) -> f64 {
        let prev = self.out[i];
        if target == 0.0 || (prev != 0.0 && prev.signum() != target.signum()) {
            self.progress[i] = 0.0;
        }
        if target == 0.0 {
            self.out[i] = 0.0;
            return 0.0;
        }
        self.progress[i] = if time > 0.0 { (self.progress[i] + dt / time).min(1.0) } else { 1.0 };
        // Below 1e-9 of full counts as full (the progress sums dt steps).
        let cap = if self.progress[i] > 1.0 - 1e-9 { 1.0 } else { curve.eval(self.progress[i]) };
        self.out[i] = target.signum() * target.abs().min(cap);
        self.out[i]
    }

    /// Ramps thrust, roll and turn; the direct mouse is not a deflection and passes unchanged.
    pub fn apply(&mut self, input: &FlightInput, t: &ShipTuning, dt: f64) -> FlightInput {
        let (lt, at, c) = (t.linear_ramp_time, t.angular_ramp_time, &t.ramp_curve);
        let thrust = DVec3::new(self.axis(0, input.thrust.x, dt, lt, c), self.axis(1, input.thrust.y, dt, lt, c), self.axis(2, input.thrust.z, dt, lt, c));
        let roll = self.axis(3, input.roll, dt, at, c);
        let turn = DVec2::new(self.axis(4, input.turn.x, dt, at, c), self.axis(5, input.turn.y, dt, at, c));
        FlightInput { thrust, roll, turn, ..*input }
    }
}

/// Interpolation between neighbouring curve points.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Interp {
    /// `lerp` with `smoothstep` between the points (the ship's forward speed curve).
    Smooth,
    /// Plain `lerp` (input curves, where `Smooth` cannot express the identity).
    Linear,
}

/// A response curve: points (x, y) with ascending x, clamped at both ends.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(from = "RawCurve")]
pub struct Curve {
    pub interp: Interp,
    pub points: Vec<DVec2>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCurve {
    interp: Interp,
    points: Vec<[f64; 2]>,
}

impl From<RawCurve> for Curve {
    fn from(r: RawCurve) -> Curve {
        Curve { interp: r.interp, points: r.points.into_iter().map(DVec2::from).collect() }
    }
}

impl Curve {
    pub fn new(interp: Interp, points: Vec<DVec2>) -> Result<Curve, String> {
        let c = Curve { interp, points };
        c.validate()?;
        Ok(c)
    }

    /// At least 2 points, x strictly ascending, all values finite. Parsing does not check this;
    /// the owner of a curve calls it and puts the curve's name in front of the reason.
    pub fn validate(&self) -> Result<(), String> {
        let points = &self.points;
        if points.len() < 2 {
            return Err(format!("curve needs at least 2 points, has {}", points.len()));
        }
        if let Some(p) = points.iter().find(|p| !p.is_finite()) {
            return Err(format!("curve point {p} is not finite"));
        }
        if let Some(w) = points.windows(2).find(|w| w[1].x <= w[0].x) {
            return Err(format!("curve x must be strictly ascending: {} then {}", w[0].x, w[1].x));
        }
        Ok(())
    }

    pub fn eval(&self, x: f64) -> f64 {
        let c = &self.points;
        for i in 1..c.len() {
            let (lo, hi) = (c[i - 1], c[i]);
            if x <= hi.x {
                let t = match self.interp {
                    Interp::Smooth => smoothstep(lo.x, hi.x, x),
                    Interp::Linear => ((x - lo.x) / (hi.x - lo.x)).clamp(0.0, 1.0),
                };
                return lerp(lo.y, hi.y, t);
            }
        }
        c[c.len() - 1].y
    }

    pub fn last_y(&self) -> f64 {
        self.points[self.points.len() - 1].y
    }
}

/// Parses a tuning object: every field required, unknown fields rejected, except an optional
/// `_comment` string (as in the planet recipes). `what` names the file in errors.
pub fn parse_tuning<T: serde::de::DeserializeOwned>(what: &str, s: &str) -> Result<T, String> {
    let mut v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("{what}: {e}"))?;
    if let Some(o) = v.as_object_mut()
        && let Some(c) = o.remove("_comment")
        && !c.is_string()
    {
        return Err(format!("{what}: _comment must be a string"));
    }
    serde_json::from_value(v).map_err(|e| format!("{what}: {e}"))
}

/// The ship's tuning values (`content/tuning/ship.json`): the axis flight model (spike 13) and the
/// parts around it (ramp, decoupling, boost capacitor, ground hold). TODO(initiator): all values.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShipTuning {
    /// m/s: coupled speed for full stick in any direction (the stick is normalised into a ball),
    /// in full atmosphere; decoupled thrust stops at the same caps.
    pub cruise_speed: f64,
    /// m/s: the forward and backward caps at full boost (at least `cruise_speed`).
    pub boost_speed_forward: f64,
    pub boost_speed_backward: f64,
    /// The same caps out of the atmosphere (faster in space).
    pub space: SpaceCaps,
    /// m/s²: what the thrusters give per axis and direction in vacuum.
    pub accel: Dirs,
    /// Share of that thrust in full atmosphere (density 1), blended by the density.
    pub atmosphere_thrust: f64,
    /// Multipliers on `accel` at full boost (the brake always gets them).
    pub boost_accel: Dirs,
    /// 1/s: the assist asks for this times the velocity error (saturated far from the goal,
    /// exponential close to it).
    pub linear_decay: f64,
    /// rad/s: turn rates at full stick; pitch and yaw share an ellipse.
    pub rate: Rot,
    /// Share of the turn rates (y) over the speed as a share of the cruise cap (x): a corner speed
    /// in the middle, slower at rest and at the top.
    pub rate_over_speed: Curve,
    /// Multipliers on `rate` at full boost.
    pub boost_rate: Rot,
    /// rad/s²: angular acceleration cap per axis.
    pub angular_accel: Rot,
    /// 1/s: like `linear_decay`, for the turn rates.
    pub angular_decay: f64,
    pub precision: Precision,
    pub g_safety: GSafety,
    /// Quadratic drag a = k * density * v².
    pub drag_k: f64,
    /// Seconds from no input to full deflection, thrust and rotation (#25).
    pub linear_ramp_time: f64,
    pub angular_ramp_time: f64,
    /// Deflection cap (y) over the ramp's progress (x, 0..1).
    pub ramp_curve: Curve,
    /// Seconds over which switching to decoupled (or back) blends the assist's damping (#26).
    pub decouple_time: f64,
    /// The boost's charge meter (#90).
    pub boost_capacitor: BoostCapacitorTuning,
    /// Degrees: a ship that touches down on ground at most this steep keeps its spot until thrust
    /// (ground hold, #92). Steeper ground: no hold. TODO(initiator): the value; what a ship does on
    /// steeper ground.
    pub landing_slope_limit: f64,
}

impl ShipTuning {
    pub fn from_json(s: &str) -> Result<ShipTuning, String> {
        let t: ShipTuning = parse_tuning("ship.json", s)?;
        t.validate().map_err(|e| format!("ship.json: {e}"))?;
        Ok(t)
    }

    /// Speeds, rates and decays positive, drag and times not negative, shares in range; everything
    /// finite (#106 point 5).
    pub fn validate(&self) -> Result<(), String> {
        let positive = [
            ("cruise_speed", self.cruise_speed),
            ("boost_speed_forward", self.boost_speed_forward),
            ("boost_speed_backward", self.boost_speed_backward),
            ("space.cruise_speed", self.space.cruise_speed),
            ("space.boost_speed_forward", self.space.boost_speed_forward),
            ("space.boost_speed_backward", self.space.boost_speed_backward),
            ("linear_decay", self.linear_decay),
            ("angular_decay", self.angular_decay),
            ("precision.speed", self.precision.speed),
        ];
        let not_negative = [
            ("drag_k", self.drag_k),
            ("linear_ramp_time", self.linear_ramp_time),
            ("angular_ramp_time", self.angular_ramp_time),
            ("decouple_time", self.decouple_time),
        ];
        for (what, v, ok) in positive.iter().map(|&(w, v)| (w, v, v > 0.0)).chain(not_negative.iter().map(|&(w, v)| (w, v, v >= 0.0))) {
            if !(ok && v.is_finite()) {
                return Err(format!("{what} {v} out of range"));
            }
        }
        if self.boost_speed_forward < self.cruise_speed || self.boost_speed_backward < self.cruise_speed {
            return Err(format!("boost speeds {} and {} below cruise_speed {}", self.boost_speed_forward, self.boost_speed_backward, self.cruise_speed));
        }
        let s = &self.space;
        if s.boost_speed_forward < s.cruise_speed || s.boost_speed_backward < s.cruise_speed {
            return Err(format!("space: boost speeds {} and {} below cruise_speed {}", s.boost_speed_forward, s.boost_speed_backward, s.cruise_speed));
        }
        if !(self.atmosphere_thrust > 0.0 && self.atmosphere_thrust <= 1.0) {
            return Err(format!("atmosphere_thrust {} out of range", self.atmosphere_thrust));
        }
        self.rate_over_speed.validate().map_err(|e| format!("rate_over_speed: {e}"))?;
        if let Some(p) = self.rate_over_speed.points.iter().find(|p| p.y <= 0.0) {
            return Err(format!("rate_over_speed: share {} must be above 0", p.y));
        }
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
        if !(p.landing_share > 0.0 && p.landing_share <= 1.0) {
            return Err(format!("precision.landing_share {} out of range", p.landing_share));
        }
        if !(p.rate_share > 0.0 && p.rate_share <= 1.0) {
            return Err(format!("precision.rate_share {} out of range", p.rate_share));
        }
        self.ramp_curve.validate().map_err(|e| format!("ramp_curve: {e}"))?;
        self.boost_capacitor.validate()?;
        if !(0.0..=90.0).contains(&self.landing_slope_limit) {
            return Err(format!("landing_slope_limit {} out of range (0 to 90 degrees)", self.landing_slope_limit));
        }
        Ok(())
    }
}

impl Default for ShipTuning {
    fn default() -> Self {
        ShipTuning {
            cruise_speed: 150.0,
            boost_speed_forward: 350.0,
            boost_speed_backward: 200.0,
            space: SpaceCaps { cruise_speed: 300.0, boost_speed_forward: 600.0, boost_speed_backward: 400.0 },
            accel: Dirs { forward: 60.0, backward: 40.0, left: 24.0, right: 24.0, up: 50.0, down: 30.0 },
            atmosphere_thrust: 0.5,
            boost_accel: Dirs { forward: 2.0, backward: 1.5, left: 1.25, right: 1.25, up: 1.25, down: 1.25 },
            linear_decay: 3.0,
            rate: Rot { pitch: 1.6, yaw: 1.6, roll: 2.4 },
            rate_over_speed: Curve { interp: Interp::Linear, points: vec![DVec2::new(0.0, 0.85), DVec2::new(0.5, 1.0), DVec2::new(1.0, 0.8)] },
            boost_rate: Rot { pitch: 1.2, yaw: 1.2, roll: 1.0 },
            angular_accel: Rot { pitch: 8.0, yaw: 8.0, roll: 14.0 },
            angular_decay: 12.0,
            precision: Precision { full_below: 5.0, off_above: 40.0, speed: 15.0, landing_share: 0.2, rate_share: 1.0 },
            g_safety: GSafety { enabled: true, cap_turns: true, limit: Dirs { forward: 8.0, backward: 6.0, left: 4.0, right: 4.0, up: 6.0, down: 3.0 } },
            drag_k: 0.0005,
            linear_ramp_time: 0.3,
            angular_ramp_time: 0.25,
            ramp_curve: Curve { interp: Interp::Smooth, points: vec![DVec2::new(0.0, 0.25), DVec2::new(1.0, 1.0)] },
            decouple_time: 4.0,
            boost_capacitor: BoostCapacitorTuning::default(),
            landing_slope_limit: 35.0,
        }
    }
}


/// A ship set down below the slope limit keeps its spot until thrust (#92). Positions are planet
/// frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundHold {
    /// The centre at touchdown: settling, the ship may sink and tip onto the slope along the up
    /// through it, not sideways.
    pub at: DVec3,
    /// Resting, the ship is held at this centre.
    pub rest: Option<DVec3>,
}

/// Assisted-flight controller: its tuning plus the runtime state.
#[derive(Clone, Debug)]
pub struct ShipController {
    pub tuning: ShipTuning,
    pub hover_assist: bool,   // H
    pub horizon_follow: bool, // L
    pub brake_active: bool,
    pub commanded_speed: f64,
    pub forward_speed_limit: f64,
    pub terrain_clearance: f64,
    /// Effective L influence; zero outside the field.
    pub planet_follow_strength: f64,
    /// Coupled flight wanted (C switches); `coupling` follows it over `decouple_time`.
    pub coupled: bool,
    /// 1 = coupled (the assist damps towards the requested velocity), 0 = decoupled (thrust only
    /// along the input, the ship keeps gliding).
    pub coupling: f64,
    pub ramp: InputRamp,
    /// Seconds the hull has rested on the ground (touching, not sinking) without a break.
    pub ground_time: f64,
    /// The boost's charge (#90); `boost_strength` is what it gave the last step (0..1).
    pub boost: BoostCapacitor,
    pub boost_strength: f64,
    /// Dev switch (F6): boost as the speed stage of #24, the capacitor ignored. Not a tuning value.
    pub boost_stage: bool,
    /// Landing mode (K, spike 13): the precision band near the ground is on.
    pub landing_mode: bool,
    /// What the flight model did in its last step.
    pub axis: AxisState,
    /// Set down below `landing_slope_limit`: held to the spot until thrust (#92).
    pub ground_hold: Option<GroundHold>,
    /// Seconds without hull contact; a hold lets go after `GROUND_HOLD_RELEASE_TIME`.
    contact_lost: f64,

    horizon_w: DVec3,
}

impl Default for ShipController {
    fn default() -> Self {
        ShipController::new(ShipTuning::default())
    }
}

impl ShipController {
    pub fn new(tuning: ShipTuning) -> Self {
        ShipController {
            forward_speed_limit: tuning.cruise_speed,
            tuning,
            hover_assist: true,
            horizon_follow: true,
            brake_active: false,
            commanded_speed: 0.0,
            terrain_clearance: 0.0,
            planet_follow_strength: 1.0,
            coupled: true,
            coupling: 1.0,
            ramp: InputRamp::default(),
            ground_time: 0.0,
            boost: BoostCapacitor::default(),
            boost_strength: 0.0,
            boost_stage: false,
            landing_mode: false,
            axis: AxisState::default(),
            ground_hold: None,
            contact_lost: 0.0,
            horizon_w: DVec3::ZERO,
        }
    }

    /// Test setup after placing the ship by hand: coupled, full charge, no smoothed state left.
    pub fn reset_state(&mut self) {
        self.horizon_w = DVec3::ZERO;
        self.ramp = InputRamp::default();
        self.axis = AxisState::default();
        self.boost = BoostCapacitor::default();
        self.boost_strength = 0.0;
        self.coupled = true;
        self.coupling = 1.0;
        self.ground_time = 0.0;
        self.ground_hold = None;
        self.contact_lost = 0.0;
        self.landing_mode = false;
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
    fn on_ground(&self, grounded: bool, up: DVec3, v: DVec3) -> bool {
        grounded && self.terrain_clearance < self.ground_clearance() && (v - up * v.dot(up)).length() < Self::GROUND_HOLD_SPEED
    }

    /// A step the caller does not fly (the quantum drive holds the ship): the boost is released
    /// and its meter runs on, and no planet-follow rate stays behind for the next step (#110
    /// point 6).
    pub fn skip_step(&mut self, dt: f64) {
        self.boost.step(false, &self.tuning.boost_capacitor, dt);
        self.boost_strength = 0.0;
        self.horizon_w = DVec3::ZERO;
    }

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

    /// The step's preamble: the input ramp (scripted test input, nobody piloting, is not
    /// ramped), the coupling blend, the brake and the boost. Returns the input to fly and the boost
    /// strength 0..1.
    fn begin_step(&mut self, input: &FlightInput, dt: f64) -> (FlightInput, f64) {
        let input = if input.piloted { self.ramp.apply(input, &self.tuning, dt) } else { *input };
        let target = if self.coupled { 1.0 } else { 0.0 };
        self.coupling = move_towards(self.coupling, target, if self.tuning.decouple_time > 0.0 { dt / self.tuning.decouple_time } else { 1.0 });
        self.brake_active = input.piloted && input.brake;
        // The brake neither uses nor drains the charge (TODO(initiator), #90).
        let strength = if self.boost_stage {
            self.boost.stage(input.boost && !self.brake_active)
        } else {
            self.boost.step_braking(input.boost, self.brake_active, &self.tuning.boost_capacitor, dt)
        };
        self.boost_strength = strength;
        (input, strength)
    }

    /// One physics step (Godot's `_integrate_forces`). Returns the new linear and
    /// angular velocity; the caller writes them to the body before integration.

    /// Ground hold (#92), the part of the step before the velocity goal. `thrusting`: any stick
    /// input but down. Returns `hold` (low, slow and touching without thrust: settle straight
    /// down) and whether the step settles or rests (`hold` or a ground hold).
    fn ground_rules(&mut self, env: &impl PlanetEnv, origin: DVec3, v: DVec3, up: DVec3, thrusting: bool, grounded: bool, dt: f64) -> (bool, bool) {
        let hold = !thrusting && self.on_ground(grounded, up, v);
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
        let left = settling && self.contact_lost >= Self::GROUND_HOLD_RELEASE_TIME || self.terrain_clearance >= self.ground_clearance();
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
}
