//! flight_core: the assisted-flight ship controller and the arcade planet field,
//! ported from the Godot spikes (`spikes/planet/ship.gd`, `planet_field.gd`,
//! `main.gd`). Plain Rust, f64, glam; no Bevy types. Godot conventions: Y up,
//! -Z forward, right-handed, angular velocity in world space.
//!
//! All numbers are spike test values (assumptions for testing, not design).
use glam::{DQuat, DVec2, DVec3};

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
    pub piloted: bool,
}

/// Assisted-flight controller. Spike test values throughout.
#[derive(Clone, Debug)]
pub struct ShipController {
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
    pub forward_speed_curve: Vec<DVec2>,
    /// Quadratic drag a = k * density * v². Terminal speed at the surface about
    /// 200 m/s with normal thrust, about 450 m/s with boost.
    pub drag_k: f64,
    /// Landing aid: sink rate capped to this share of the clearance per second (min 2 m/s).
    pub landing_sink_factor: f64,

    pub hover_assist: bool,   // H
    pub horizon_follow: bool, // L
    pub brake_active: bool,
    pub commanded_speed: f64,
    pub forward_speed_limit: f64,
    pub terrain_clearance: f64,
    /// Effective L influence; zero outside the field.
    pub planet_follow_strength: f64,

    horizon_w: DVec3,
    correction_accel: DVec3,
}

impl Default for ShipController {
    fn default() -> Self {
        ShipController {
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
            forward_speed_curve: vec![
                DVec2::new(30.0, 45.0),
                DVec2::new(150.0, 60.0),
                DVec2::new(600.0, 150.0),
                DVec2::new(1200.0, 350.0),
            ],
            drag_k: 0.0005,
            landing_sink_factor: 0.5,
            hover_assist: true,
            horizon_follow: true,
            brake_active: false,
            commanded_speed: 0.0,
            forward_speed_limit: 45.0,
            terrain_clearance: 0.0,
            planet_follow_strength: 1.0,
            horizon_w: DVec3::ZERO,
            correction_accel: DVec3::ZERO,
        }
    }
}

impl ShipController {
    pub fn clearance_at(&self, env: &impl PlanetEnv, world: DVec3) -> f64 {
        let p = env.to_planet(world);
        p.length() - env.radius() - env.height_at(p.normalize())
    }

    pub fn forward_speed_at(&self, clearance: f64) -> f64 {
        let c = &self.forward_speed_curve;
        for i in 1..c.len() {
            let (lo, hi) = (c[i - 1], c[i]);
            if clearance <= hi.x {
                return lerp(lo.y, hi.y, smoothstep(lo.x, hi.x, clearance));
            }
        }
        c[c.len() - 1].y
    }

    /// Uses the cruise envelope as well as actual speed so authority does not
    /// fade away throughout a stop.
    pub fn braking_budget(&self, speed: f64, cruise_limit: f64) -> f64 {
        self.assisted_braking.max(speed.max(cruise_limit) / self.assisted_braking_time)
    }

    /// Preview terrain over the braking horizon. Lowers the requested speed; it
    /// does not snap velocity or promise collision avoidance.
    fn flight_clearance(&self, env: &impl PlanetEnv, world: DVec3, v: DVec3, current: f64) -> f64 {
        let p = env.to_planet(world);
        let up = p.normalize();
        let sink = (-v.dot(up)).max(0.0);
        let lead = 0.5 + self.thrust_response_time * 3.0;
        let preview_time = lead + v.length() / self.assisted_braking;
        let mut clearance = current - sink * lead - sink * sink / (2.0 * self.assisted_braking);
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
        let b = body.rot;
        let origin = body.pos;
        let gravity = env.gravity_at(origin);
        let density = env.density_at(origin);
        self.planet_follow_strength = if self.horizon_follow { env.field_strength_at(origin) } else { 0.0 };

        let mut thrust_in = input.thrust;
        let mut boost = if input.boost { self.boost_factor } else { 1.0 };
        self.brake_active = input.piloted && input.brake;
        if self.brake_active {
            thrust_in = DVec3::ZERO;
            boost = 1.0;
        }

        let mut v = body.lin_vel;
        let drag = -v * self.drag_k * density * v.length();
        if self.hover_assist || self.brake_active {
            let pos = env.to_planet(origin);
            let up = pos.normalize();
            self.terrain_clearance = self.clearance_at(env, origin);
            let clearance = self.flight_clearance(env, origin, v, self.terrain_clearance);
            self.forward_speed_limit = self.forward_speed_at(clearance);
            if boost > 1.0 {
                // Boost stays gentle near terrain and cannot exceed high-altitude cruise.
                let top = self.forward_speed_curve.last().unwrap().y;
                self.forward_speed_limit = lerp(
                    self.forward_speed_limit,
                    top.min(self.forward_speed_limit * 2.5),
                    smoothstep(30.0, 150.0, clearance),
                );
            }
            let request = limit_length(thrust_in, 1.0);
            let forward_speed = if request.z < 0.0 { self.forward_speed_limit } else { self.assisted_reverse_speed };
            let mut goal = b * DVec3::new(
                request.x * self.assisted_strafe_speed,
                request.y * self.assisted_vertical_speed,
                request.z * forward_speed,
            );
            // Slow the requested descent near the ground, without clamping momentum.
            let sink_goal = -goal.dot(up);
            let sink_cap = 2.0f64.max(self.terrain_clearance.max(0.0) * self.landing_sink_factor);
            if sink_goal > sink_cap {
                goal += up * (sink_goal - sink_cap);
            }
            self.commanded_speed = goal.length();
            let correction = (goal - v) / self.velocity_response_time;
            let reference_speed = v.length().max(goal.length().max(self.forward_speed_limit));
            let mut budget = self.assisted_accel.max(reference_speed / self.assisted_acceleration_time);
            if boost > 1.0 {
                budget = budget.max(self.assisted_boost_accel);
            }
            if correction.dot(v) < 0.0 {
                budget = self.braking_budget(v.length(), self.forward_speed_limit);
            }
            // Only neutral piloted input gets the gentle release response (Godot's is_zero_approx).
            let neutral = thrust_in.abs().max_element() < 1e-5;
            if input.piloted && neutral && !self.brake_active {
                budget = self.release_braking.max(v.length().max(self.forward_speed_limit) / self.release_braking_time);
            }
            let mut curve_accel = DVec3::ZERO;
            if self.horizon_follow {
                curve_accel = (up.cross(v) / pos.length()).cross(v) * self.planet_follow_strength;
            }
            // Gravity cancellation is the arcade hover assumption. Reserve thrust
            // for the curved path and drag first, then spend the rest on correction.
            let support = limit_length(curve_accel - drag, budget);
            let available = (budget - support.length()).max(0.0);
            let desired = limit_length(correction, available);
            self.correction_accel = self
                .correction_accel
                .lerp(desired, 1.0 - (-dt / self.thrust_response_time).exp());
            // Bound acceleration, never snap velocity to the new target.
            self.correction_accel = limit_length(self.correction_accel, available);
            let thrust = support + self.correction_accel;
            v += (thrust + drag) * dt;
        } else {
            self.correction_accel = DVec3::ZERO;
            self.commanded_speed = 0.0;
            v += (b * limit_length(thrust_in, 1.0)) * self.thrust_accel * boost * dt;
            v += gravity * dt;
            v -= v * (self.drag_k * density * v.length() * dt).min(1.0);
        }

        // Rotation: mouse movement is an angle per step, capped at turn_rate and
        // smoothed a little so the ship has some weight.
        let pitch = (-input.mouse.y / dt).clamp(-self.turn_rate, self.turn_rate);
        let yaw = (-input.mouse.x / dt).clamp(-self.turn_rate, self.turn_rate);
        let target_w = b * DVec3::new(pitch, yaw, input.roll * self.roll_rate);
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
