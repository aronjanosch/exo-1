//! flight_core: the SC flight model (`sc`), the ship's ground rules (`ground`), the arcade planet
//! field and the ship's own values. Plain Rust,
//! f64, glam; no Bevy types. Conventions: Y up, -Z forward, right-handed, angular velocity in
//! world space.
//!
//! All numbers are spike test values (assumptions for testing, not design).
pub mod audio;
pub mod camera;
pub mod ground;
pub mod hud;
pub mod limits;
pub mod sc;

pub use ground::{GroundHold, GroundRules, GroundTuning};
pub use limits::{Dirs, Rot, G0};

use glam::{DQuat, DVec2, DVec3};
use serde::Deserialize;

/// The chase camera sits at (0, 5.5, 17) in ship space, pitched by this.
pub const CHASE_CAMERA_PITCH_DEG: f64 = -10.0;
pub const CHASE_CAMERA_OFFSET: DVec3 = DVec3::new(0.0, 5.5, 17.0);

/// Hermite step, clamped.
pub fn smoothstep(from: f64, to: f64, x: f64) -> f64 {
    if (from - to).abs() < 1e-9 {
        return if x <= from { 0.0 } else { 1.0 };
    }
    let s = ((x - from) / (to - from)).clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

/// `v` shortened to at most `max`, direction kept.
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

/// Boost capacitor tuning (`ground.json`, `boost_capacitor`, #90). Starting values, not design.
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

    /// The speed stage of #24 (no capacitor, `drain_time` 0): full boost while held; the meter stays
    /// as it is (it must not refill it, #104 point 8).
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
    /// Mouse movement accumulated this step, radians: x yaw, y pitch.
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
