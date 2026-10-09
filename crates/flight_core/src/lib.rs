//! flight_core: the assisted-flight ship controller and the arcade planet field,
//! ported from the Godot spikes (`spikes/planet/ship.gd`, `planet_field.gd`,
//! `main.gd`). Plain Rust, f64, glam; no Bevy types. Godot conventions: Y up,
//! -Z forward, right-handed, angular velocity in world space.
//!
//! All numbers are spike test values (assumptions for testing, not design).
pub mod camera;
pub mod hud;

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
    /// Landed below `LANDED_CLEARANCE` m and `LANDED_SPEED` m/s, airborne above `AIRBORNE_CLEARANCE`.
    pub const LANDED_CLEARANCE: f64 = 1.5;
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

    /// One step. `clearance` is the ship's height above the terrain (m).
    pub fn step(&mut self, clearance: f64, speed: f64, dt: f64) {
        let landed = if self.landed { clearance < Self::AIRBORNE_CLEARANCE } else { clearance < Self::LANDED_CLEARANCE && speed < Self::LANDED_SPEED };
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
}

impl BoostCapacitorTuning {
    pub fn validate(&self) -> Result<(), String> {
        let bad = |what: &str, v: f64| Err(format!("boost_capacitor: {what} {v} out of range"));
        if !(self.drain_time >= 0.0) {
            return bad("drain_time", self.drain_time);
        }
        if !(self.recharge_time > 0.0) {
            return bad("recharge_time", self.recharge_time);
        }
        if !(self.recharge_delay >= 0.0) {
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
}

impl Default for BoostCapacitor {
    fn default() -> Self {
        BoostCapacitor { charge: 1.0, active: false, idle: 0.0 }
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

    /// The speed stage of #24: full boost while held, the meter stays full.
    pub fn stage(&mut self, want: bool) -> f64 {
        self.charge = 1.0;
        self.active = want;
        self.idle = 0.0;
        if want { 1.0 } else { 0.0 }
    }

    /// One step with boost held (`want`) or not; returns the boost strength 0..1 for this step.
    pub fn step(&mut self, want: bool, t: &BoostCapacitorTuning, dt: f64) -> f64 {
        if t.drain_time <= 0.0 {
            return self.stage(want);
        }
        if !want {
            self.active = false;
        } else if self.ready(t) {
            self.active = true;
        }
        if self.active {
            let strength = t.strength_curve.eval(self.charge).clamp(0.0, 1.0);
            self.charge = (self.charge - dt / t.drain_time).max(0.0);
            self.idle = 0.0;
            if self.charge == 0.0 {
                self.active = false;
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

/// The ship's tuning values (`content/tuning/ship.json`). Spike test values throughout.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShipTuning {
    pub thrust_accel: f64, // m/s²
    pub boost_factor: f64,
    pub turn_rate: f64, // rad/s cap
    pub roll_rate: f64,
    pub assisted_accel: f64,   // m/s²; all assisted correction shares one budget
    pub assisted_braking: f64, // m/s²
    pub assisted_boost_accel: f64,
    /// s; authority scale, not a guaranteed arrival time
    pub assisted_acceleration_time: f64,
    pub assisted_braking_time: f64,  // s; before support and settling
    pub release_braking: f64,        // m/s²; gentle neutral input while piloted
    pub release_braking_time: f64,   // s; cruise-scaled neutral authority
    pub velocity_response_time: f64, // s; ease into the requested velocity
    pub thrust_response_time: f64,   // s; full thrust builds over several ticks
    pub assisted_reverse_speed: f64,
    pub assisted_strafe_speed: f64,
    pub assisted_vertical_speed: f64,
    /// (terrain clearance in metres, forward speed in m/s).
    pub forward_speed_curve: Curve,
    /// Quadratic drag a = k * density * v². Terminal speed at the surface about
    /// 200 m/s with normal thrust, about 450 m/s with boost.
    pub drag_k: f64,
    /// Landing aid: sink rate capped to this share of the clearance per second (min 2 m/s).
    pub landing_sink_factor: f64,
    /// Assisted boost: the forward speed limit times this, at most the curve's top (gentle near
    /// terrain, see `step`).
    pub assisted_boost_speed_factor: f64,
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
        t.forward_speed_curve.validate().map_err(|e| format!("ship.json: forward_speed_curve: {e}"))?;
        t.ramp_curve.validate().map_err(|e| format!("ship.json: ramp_curve: {e}"))?;
        t.boost_capacitor.validate().map_err(|e| format!("ship.json: {e}"))?;
        if !(0.0..=90.0).contains(&t.landing_slope_limit) {
            return Err(format!("ship.json: landing_slope_limit {} out of range (0 to 90 degrees)", t.landing_slope_limit));
        }
        Ok(t)
    }
}

impl Default for ShipTuning {
    fn default() -> Self {
        ShipTuning {
            thrust_accel: 20.0,
            boost_factor: 5.0,
            turn_rate: 2.5,
            roll_rate: 1.8,
            assisted_accel: 30.0,
            assisted_braking: 40.0,
            assisted_boost_accel: 60.0,
            assisted_acceleration_time: 3.5,
            assisted_braking_time: 2.25,
            release_braking: 14.0,
            release_braking_time: 6.0,
            velocity_response_time: 0.35,
            thrust_response_time: 0.15,
            assisted_reverse_speed: 25.0,
            assisted_strafe_speed: 20.0,
            assisted_vertical_speed: 15.0,
            forward_speed_curve: Curve {
                interp: Interp::Smooth,
                points: vec![
                    DVec2::new(30.0, 45.0),
                    DVec2::new(150.0, 60.0),
                    DVec2::new(600.0, 150.0),
                    DVec2::new(1200.0, 350.0),
                ],
            },
            drag_k: 0.0005,
            landing_sink_factor: 0.5,
            assisted_boost_speed_factor: 2.5,
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
    /// Set down below `landing_slope_limit`: held to the spot until thrust (#92).
    pub ground_hold: Option<GroundHold>,

    horizon_w: DVec3,
    correction_accel: DVec3,
}

impl Default for ShipController {
    fn default() -> Self {
        ShipController::new(ShipTuning::default())
    }
}

impl ShipController {
    pub fn new(tuning: ShipTuning) -> Self {
        ShipController {
            tuning,
            hover_assist: true,
            horizon_follow: true,
            brake_active: false,
            commanded_speed: 0.0,
            forward_speed_limit: 45.0,
            terrain_clearance: 0.0,
            planet_follow_strength: 1.0,
            coupled: true,
            coupling: 1.0,
            ramp: InputRamp::default(),
            ground_time: 0.0,
            boost: BoostCapacitor::default(),
            boost_strength: 0.0,
            boost_stage: false,
            ground_hold: None,
            horizon_w: DVec3::ZERO,
            correction_accel: DVec3::ZERO,
        }
    }

    /// m/s: on the ground the assist settles the ship at this speed (no slide, see `step`).
    pub const GROUND_SETTLE_SPEED: f64 = 0.5;
    /// s: resting this long on the ground (not sinking) ends the settle push.
    pub const GROUND_SETTLE_TIME: f64 = 0.3;
    /// m: the slope under the ship is measured this far to each side (about half the hull's width).
    pub const GROUND_SLOPE_SPAN: f64 = 2.0;
    /// m: a held ship found farther than this sideways from its spot was moved by something else
    /// (a teleport); the hold lets go instead of pulling it back.
    pub const GROUND_HOLD_REACH: f64 = 1.0;

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

    pub fn forward_speed_at(&self, clearance: f64) -> f64 {
        self.tuning.forward_speed_curve.eval(clearance)
    }

    /// Uses the cruise envelope as well as actual speed so authority does not
    /// fade away throughout a stop.
    pub fn braking_budget(&self, speed: f64, cruise_limit: f64) -> f64 {
        self.tuning.assisted_braking.max(speed.max(cruise_limit) / self.tuning.assisted_braking_time)
    }

    /// Preview terrain over the braking horizon. Lowers the requested speed; it
    /// does not snap velocity or promise collision avoidance.
    fn flight_clearance(&self, env: &impl PlanetEnv, world: DVec3, v: DVec3, current: f64) -> f64 {
        let p = env.to_planet(world);
        let up = p.normalize();
        let sink = (-v.dot(up)).max(0.0);
        let lead = 0.5 + self.tuning.thrust_response_time * 3.0;
        let preview_time = lead + v.length() / self.tuning.assisted_braking;
        let mut clearance = current - sink * lead - sink * sink / (2.0 * self.tuning.assisted_braking);
        let horizon_w = up.cross(v) / p.length() * env.field_strength_at(world);
        for i in 1..4 {
            let t = preview_time * i as f64 / 3.0;
            let mut preview = world + v * t;
            if self.horizon_follow && horizon_w.length_squared() > 1e-8 {
                // Integrate a turning velocity, keeping full travel distance at partial follow.
                let radial_speed = v.dot(up);
                let tangent = v - up * radial_speed;
                let wl = horizon_w.length();
                let angle = wl * t;
                let turned = DQuat::from_axis_angle(horizon_w / wl, angle * 0.5) * tangent;
                preview = world + turned * (2.0 * (angle * 0.5).sin() / wl) + up * radial_speed * t;
            }
            clearance = clearance.min(self.clearance_at(env, preview));
        }
        clearance
    }

    /// One physics step (Godot's `_integrate_forces`). Returns the new linear and
    /// angular velocity; the caller writes them to the body before integration.
    pub fn step(&mut self, body: &BodyState, input: &FlightInput, env: &impl PlanetEnv, dt: f64) -> (DVec3, DVec3) {
        // Scripted test input (nobody piloting) is not ramped.
        let ramped;
        let input = if input.piloted {
            ramped = self.ramp.apply(input, &self.tuning, dt);
            &ramped
        } else {
            input
        };
        let target = if self.coupled { 1.0 } else { 0.0 };
        self.coupling = move_towards(self.coupling, target, if self.tuning.decouple_time > 0.0 { dt / self.tuning.decouple_time } else { 1.0 });
        let b = body.rot;
        let origin = body.pos;
        let gravity = env.gravity_at(origin);
        let density = env.density_at(origin);
        self.planet_follow_strength = if self.horizon_follow { env.field_strength_at(origin) } else { 0.0 };

        let mut thrust_in = input.thrust;
        self.brake_active = input.piloted && input.brake;
        // The brake neither uses nor drains the charge (TODO(initiator), #90).
        let want = input.boost && !self.brake_active;
        let strength = if self.boost_stage { self.boost.stage(want) } else { self.boost.step(want, &self.tuning.boost_capacitor, dt) };
        self.boost_strength = strength;
        let boost = 1.0 + (self.tuning.boost_factor - 1.0) * strength;
        if self.brake_active {
            thrust_in = DVec3::ZERO;
        }

        let mut v = body.lin_vel;
        let drag = -v * self.tuning.drag_k * density * v.length();
        if self.hover_assist || self.brake_active {
            let pos = env.to_planet(origin);
            let up = pos.normalize();
            self.terrain_clearance = self.clearance_at(env, origin);
            let clearance = self.flight_clearance(env, origin, v, self.terrain_clearance);
            self.forward_speed_limit = self.forward_speed_at(clearance);
            if strength > 0.0 {
                // Boost stays gentle near terrain and cannot exceed high-altitude cruise; a weak
                // charge gives part of it.
                let top = self.tuning.forward_speed_curve.last_y();
                self.forward_speed_limit = lerp(
                    self.forward_speed_limit,
                    top.min(self.forward_speed_limit * self.tuning.assisted_boost_speed_factor),
                    smoothstep(30.0, 150.0, clearance) * strength,
                );
            }
            let request = limit_length(thrust_in, 1.0);
            let forward_speed = if request.z < 0.0 { self.forward_speed_limit } else { self.tuning.assisted_reverse_speed };
            let mut goal = b * DVec3::new(
                request.x * self.tuning.assisted_strafe_speed,
                request.y * self.tuning.assisted_vertical_speed,
                request.z * forward_speed,
            );
            // Thrust is any input but down (down only presses the ship onto the ground).
            let thrusting = request.x.abs() >= 1e-5 || request.z.abs() >= 1e-5 || request.y > 1e-5;
            // On the ground without thrust: settle gently straight down and keep no sideways speed.
            // Pressed down at the landing sink rate onto a slope, the contact turned the push into a
            // 20 s slide.
            let hold = input.grounded && !thrusting;
            // Resting: on the ground and no longer sinking although pushed (all contacts carry it).
            let sinking = body.lin_vel.dot(up) < -0.05;
            self.ground_time = if input.grounded && !sinking { self.ground_time + dt } else { 0.0 };
            // Ground hold (#92): set down below the slope limit, the ship keeps its spot until
            // thrust. Settling still tips it onto the slope; each step the contact turns part of the
            // push sideways (0.55 m on 33 degrees in `full`), the hold takes that back.
            let here = env.to_planet(origin);
            let sideways = |d: DVec3| d - up * d.dot(up);
            let strayed = self.ground_hold.is_some_and(|h| sideways(here - h.rest.unwrap_or(h.at)).length() > Self::GROUND_HOLD_REACH);
            if thrusting || strayed {
                self.ground_hold = None;
            } else if hold && self.ground_hold.is_none() && self.ground_slope(env, origin) <= self.tuning.landing_slope_limit.to_radians() {
                self.ground_hold = Some(GroundHold { at: here, rest: None });
            }
            if let Some(h) = &mut self.ground_hold
                && h.rest.is_none()
                && self.ground_time >= Self::GROUND_SETTLE_TIME
            {
                // Where it rests, within one step's push of the touchdown spot (pulled further onto
                // it, a slope would put it into the ground).
                h.rest = Some(here);
            }
            if hold || self.ground_hold.is_some() {
                // Settle until it rests, tipping onto the slope; then no push at all (a push on a
                // slope creeps).
                goal = if self.ground_time < Self::GROUND_SETTLE_TIME { -up * Self::GROUND_SETTLE_SPEED } else { DVec3::ZERO };
            }
            // Slow the requested descent near the ground, without clamping momentum.
            let sink_goal = -goal.dot(up);
            let sink_cap = 2.0f64.max(self.terrain_clearance.max(0.0) * self.tuning.landing_sink_factor);
            if sink_goal > sink_cap {
                goal += up * (sink_goal - sink_cap);
            }
            self.commanded_speed = goal.length();
            let correction = (goal - v) / self.tuning.velocity_response_time;
            let reference_speed = v.length().max(goal.length().max(self.forward_speed_limit));
            let mut budget = self.tuning.assisted_accel.max(reference_speed / self.tuning.assisted_acceleration_time);
            if strength > 0.0 {
                budget = budget.max(lerp(budget, self.tuning.assisted_boost_accel, strength));
            }
            if correction.dot(v) < 0.0 {
                budget = self.braking_budget(v.length(), self.forward_speed_limit);
            }
            // Only neutral piloted input gets the gentle release response (Godot's is_zero_approx).
            let neutral = thrust_in.abs().max_element() < 1e-5;
            if input.piloted && neutral && !self.brake_active {
                budget = self.tuning.release_braking.max(v.length().max(self.forward_speed_limit) / self.tuning.release_braking_time);
            }
            let mut curve_accel = DVec3::ZERO;
            if self.horizon_follow {
                curve_accel = (up.cross(v) / pos.length()).cross(v) * self.planet_follow_strength;
            }
            // Gravity cancellation is the arcade hover assumption. Reserve thrust
            // for the curved path and drag first, then spend the rest on correction.
            let support = limit_length(curve_accel - drag, budget);
            let available = (budget - support.length()).max(0.0);
            let mut desired = limit_length(correction, available);
            // Decoupled: the damping towards the requested velocity blends out; what stays is
            // thrust along the input. The brake always damps.
            let c = if self.brake_active { 1.0 } else { self.coupling };
            if c < 1.0 {
                let raw = limit_length(b * limit_length(thrust_in, 1.0) * self.tuning.thrust_accel * boost, available);
                desired = desired * c + raw * (1.0 - c);
            }
            self.correction_accel = self
                .correction_accel
                .lerp(desired, 1.0 - (-dt / self.tuning.thrust_response_time).exp());
            // Bound acceleration, never snap velocity to the new target.
            self.correction_accel = limit_length(self.correction_accel, available);
            let thrust = support + self.correction_accel;
            v += (thrust + drag) * dt;
            match self.ground_hold {
                // Back to the spot within one step, whatever the contacts did last step.
                Some(GroundHold { rest: Some(p), .. }) => v = (p - here) / dt,
                Some(GroundHold { at, rest: None }) => v = up * v.dot(up) + sideways(at - here) / dt,
                None if hold => v = up * v.dot(up),
                None => {}
            }
        } else {
            self.correction_accel = DVec3::ZERO;
            self.commanded_speed = 0.0;
            self.ground_hold = None;
            v += (b * limit_length(thrust_in, 1.0)) * self.tuning.thrust_accel * boost * dt;
            v += gravity * dt;
            v -= v * (self.tuning.drag_k * density * v.length() * dt).min(1.0);
        }

        // Rotation: mouse movement is an angle per step, capped at turn_rate and
        // smoothed a little so the ship has some weight.
        let rate = self.tuning.turn_rate;
        let pitch = (-input.mouse.y / dt + input.turn.x * rate).clamp(-rate, rate);
        let yaw = (-input.mouse.x / dt + input.turn.y * rate).clamp(-rate, rate);
        let target_w = b * DVec3::new(pitch, yaw, input.roll * self.tuning.roll_rate);
        // Smooth the player's rotation, not the changing planet frame.
        let control_w = body.ang_vel - self.horizon_w;
        self.horizon_w = DVec3::ZERO;
        if self.horizon_follow {
            // d(up)/dt = v_tangential / r  =>  w = up x v / r.
            let pos = env.to_planet(origin);
            self.horizon_w = pos.normalize().cross(v) / pos.length() * self.planet_follow_strength;
        }
        let w = control_w.lerp(target_w, (12.0 * dt).min(1.0)) + self.horizon_w;
        (v, w)
    }
}
