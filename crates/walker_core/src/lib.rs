//! walker_core: first-person walker without engine types. Port of spikes/planet/player.gd
//! (radial gravity, floor snap, 50 degree floor limit) with its own move-and-slide over a
//! `World` that only answers sweeps and overlaps. The engine side (Avian shape casts) lives
//! in the Bevy crate.
//!
//! The walker lives in a frame: the planet (identity frame, world coordinates) or a ship
//! cabin (the ship's pose). Position and velocity are stored in that frame, so a moving
//! ship carries the walker without any velocity of its own (spike 3 pattern).
//! All numbers are spike test values (assumptions), not designed.
use glam::{DQuat, DVec2, DVec3};
pub mod bladder;
pub mod urine;
use serde::Deserialize;

/// Result of a sweep, world space.
#[derive(Copy, Clone, Debug)]
pub struct Hit {
    /// Distance travelled along the motion direction before contact.
    pub distance: f64,
    /// Surface normal of what was hit, pointing towards the walker.
    pub normal: DVec3,
    /// Velocity of what was hit (a moving ship), world space; the walker stops only relative to it.
    pub velocity: DVec3,
}

/// What the walker needs from the physics world. Positions are the feet (bottom of the
/// capsule), `up` the capsule axis, all in world space.
pub trait World {
    /// First hit when moving the capsule by `motion`, or None. `motion` is not zero.
    fn sweep(&self, feet: DVec3, up: DVec3, motion: DVec3) -> Option<Hit>;
    /// Displacement that moves the capsule out of every overlap (zero when free).
    fn depenetrate(&self, feet: DVec3, up: DVec3) -> DVec3;
}

/// Local-to-world transform of the frame the walker lives in.
#[derive(Copy, Clone, Debug)]
pub struct Frame {
    pub origin: DVec3,
    pub rot: DQuat,
}

impl Frame {
    pub const IDENTITY: Frame = Frame { origin: DVec3::ZERO, rot: DQuat::IDENTITY };
    pub fn to_world(&self, p: DVec3) -> DVec3 {
        self.origin + self.rot * p
    }
    pub fn to_local(&self, p: DVec3) -> DVec3 {
        self.rot.inverse() * (p - self.origin)
    }
}

/// Look direction from a heading (perpendicular to `up`) and a pitch (radians, positive looks up).
pub fn look_dir(forward: DVec3, up: DVec3, pitch: f64) -> DVec3 {
    forward * pitch.cos() + up * pitch.sin()
}

/// Inverse of `look_dir` about another `up`: heading and pitch that give `look`. Used on a frame
/// change, so the view keeps its direction in the world (issue #7). Looking straight along `up`
/// has no heading; then `fallback` (projected) is used.
pub fn split_look(look: DVec3, up: DVec3, fallback: DVec3) -> (DVec3, f64) {
    let look = look.normalize();
    let pitch = look.dot(up).clamp(-1.0, 1.0).asin();
    let mut f = look - up * look.dot(up);
    if f.length_squared() < 1e-12 {
        f = fallback - up * fallback.dot(up);
    }
    if f.length_squared() < 1e-12 {
        f = up.any_orthonormal_vector();
    }
    (f.normalize(), pitch)
}

/// Orientation with camera axes (-z looks, +y is the head) that looks exactly along `look`, the
/// head as close to `up` as it gets. Looking straight along `up` falls back to any head.
pub fn look_rot(look: DVec3, up: DVec3) -> DQuat {
    let f = look.normalize();
    let mut u = up - f * up.dot(f);
    if u.length_squared() < 1e-12 {
        u = f.any_orthonormal_vector();
    }
    let u = u.normalize();
    DQuat::from_mat3(&glam::DMat3::from_cols(f.cross(u), u, -f))
}

/// Turns the unit vector `from` towards `to`: time constant `time` (s), at most `max_rate` rad/s.
/// Used for righting the view after leaving a tilted cabin in gravity.
pub fn turn_towards(from: DVec3, to: DVec3, dt: f64, time: f64, max_rate: f64) -> DVec3 {
    let angle = from.angle_between(to);
    if angle < 1e-4 {
        return to;
    }
    let turn = (angle * dt / time).min(max_rate * dt).min(angle);
    DQuat::IDENTITY.slerp(DQuat::from_rotation_arc(from, to), turn / angle) * from
}

/// Free body orientation after this step's mouse yaw and pitch and roll (radians), each about the
/// body's own axes (issue #8).
pub fn turn_body(body: DQuat, yaw: f64, pitch: f64, roll: f64) -> DQuat {
    (body * DQuat::from_rotation_y(yaw) * DQuat::from_rotation_x(pitch) * DQuat::from_rotation_z(roll)).normalize()
}

/// Suit thrusters for weightless movement (issue #8). Assumed values, tune by feel.
/// `content/tuning/suit.json`.
#[derive(Deserialize, Copy, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SuitConfig {
    /// m/s² per axis at full input.
    pub accel: f64,
    pub boost_factor: f64,
    /// m/s²; the brake (X) takes velocity down to rest with this at most.
    pub brake: f64,
    /// s; below accel/brake the brake eases out like this, so it reaches rest without overshoot.
    pub brake_time: f64,
    /// Roll rate (Q/E), rad/s. Assumed value.
    pub roll_rate: f64,
}

impl Default for SuitConfig {
    fn default() -> Self {
        SuitConfig { accel: 2.0, boost_factor: 3.0, brake: 4.0, brake_time: 0.3, roll_rate: 1.5 }
    }
}

impl SuitConfig {
    pub fn from_json(s: &str) -> Result<SuitConfig, String> {
        parse_tuning("suit.json", s)
    }
}

/// What the suit asks for this step: `thrust` in body axes (x right, y up, z back, like the ship),
/// each -1..1.
#[derive(Copy, Clone, Debug, Default)]
pub struct SuitInput {
    pub thrust: DVec3,
    pub boost: bool,
    pub brake: bool,
}

/// Acceleration from the suit, world space. `rot` is the body orientation, `vel` the velocity the
/// brake stops (world space). The brake overrides thrust, like the ship's firm brake.
pub fn suit_accel(cfg: &SuitConfig, rot: DQuat, vel: DVec3, input: &SuitInput) -> DVec3 {
    if input.brake {
        return (-vel / cfg.brake_time).clamp_length_max(cfg.brake);
    }
    let boost = if input.boost { cfg.boost_factor } else { 1.0 };
    rot * input.thrust.clamp_length_max(1.0) * cfg.accel * boost
}

/// `content/tuning/walker.json`.
#[derive(Deserialize, Copy, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WalkerConfig {
    pub radius: f64,
    /// Total capsule height (Godot CapsuleShape3D convention).
    pub height: f64,
    pub walk_speed: f64,
    pub run_speed: f64,
    pub jump_speed: f64,
    /// Step-off: below this speed (m/s) walking and stopping ease over `start_time`, above it
    /// the walker is at full speed at once. So a short tap of W is a slow step.
    pub start_speed: f64,
    pub start_time: f64,
    pub floor_max_angle_deg: f64,
    pub snap_length: f64,
    /// Gap kept to every surface after a sweep.
    pub skin: f64,
    pub max_slides: usize,
    /// Largest look angle above or below the horizon, radians.
    pub pitch_limit: f64,
    /// Righting the view after leaving a tilted cabin in gravity: its up turns to the planet's
    /// with this time constant (s), at most `righting_max_rate` rad/s. Assumed values.
    pub righting_time: f64,
    pub righting_max_rate: f64,
}

impl Default for WalkerConfig {
    fn default() -> Self {
        // player.gd values; skin and slide count are this port's choice.
        WalkerConfig {
            radius: 0.35,
            height: 1.8,
            walk_speed: 5.0,
            run_speed: 12.0,
            jump_speed: 5.0,
            // Initiator: 0 to 3 m/s within 0.1 s, then full speed.
            start_speed: 3.0,
            start_time: 0.1,
            floor_max_angle_deg: 50.0,
            snap_length: 0.5,
            skin: 0.01,
            max_slides: 4,
            pitch_limit: 1.5,
            righting_time: 0.5,
            righting_max_rate: std::f64::consts::FRAC_PI_2,
        }
    }
}

impl WalkerConfig {
    pub fn from_json(s: &str) -> Result<WalkerConfig, String> {
        parse_tuning("walker.json", s)
    }
}

/// Parses a tuning object: every field required, unknown fields rejected, except an optional
/// `_comment` string (as in the planet recipes). Same rule as `flight_core::parse_tuning`.
fn parse_tuning<T: serde::de::DeserializeOwned>(what: &str, s: &str) -> Result<T, String> {
    let mut v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("{what}: {e}"))?;
    if let Some(o) = v.as_object_mut()
        && let Some(c) = o.remove("_comment")
        && !c.is_string()
    {
        return Err(format!("{what}: _comment must be a string"));
    }
    serde_json::from_value(v).map_err(|e| format!("{what}: {e}"))
}

#[derive(Copy, Clone, Debug, Default)]
pub struct WalkInput {
    /// x right, y forward, each -1..1.
    pub dir: DVec2,
    pub run: bool,
    pub jump: bool,
    /// Heading change this step, radians, positive turns left (mouse look).
    pub yaw: f64,
    /// Weightless only: acceleration from equipment (suit thrusters), frame coordinates.
    pub accel: DVec3,
    /// Share of the walking speed taken away (carrying a crate in both hands, #83); 0 = none.
    pub slow: f64,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct StepInfo {
    pub hits: u32,
    pub snapped: bool,
    pub depenetrated: f64,
}

#[derive(Clone, Debug)]
pub struct Walker {
    pub cfg: WalkerConfig,
    /// Feet, frame coordinates.
    pub pos: DVec3,
    /// Frame coordinates (relative to the frame, not to the world).
    pub vel: DVec3,
    /// Velocity the legs aim for (frame coordinates, horizontal part used). Collisions change
    /// `vel` only, so walking up a slope does not lose speed every step.
    pub move_vel: DVec3,
    /// Heading, frame coordinates, kept perpendicular to up.
    pub forward: DVec3,
    pub grounded: bool,
    pub floor_normal: DVec3,
}

impl Walker {
    pub fn new(pos: DVec3, forward: DVec3) -> Walker {
        Walker {
            cfg: WalkerConfig::default(),
            pos,
            vel: DVec3::ZERO,
            move_vel: DVec3::ZERO,
            forward,
            grounded: false,
            floor_normal: DVec3::Y,
        }
    }

    fn is_floor(&self, normal: DVec3, up: DVec3) -> bool {
        normal.dot(up) >= self.cfg.floor_max_angle_deg.to_radians().cos()
    }

    /// Keep the heading, make it perpendicular to `up` (player.gd `_align_to_up`).
    pub fn align(&mut self, up: DVec3, yaw: f64) {
        let mut f = self.forward;
        if yaw != 0.0 {
            f = DQuat::from_axis_angle(up, yaw) * f;
        }
        f -= up * f.dot(up);
        if f.length_squared() < 1e-12 {
            f = up.any_orthonormal_vector();
        }
        self.forward = f.normalize();
    }

    /// Stops the walker at once (teleports, sitting down).
    pub fn halt(&mut self) {
        self.vel = DVec3::ZERO;
        self.move_vel = DVec3::ZERO;
    }

    /// Moves the walker into another frame, keeping its world position. `frame_vel_change`
    /// is old frame velocity minus new frame velocity at the walker, world space
    /// (player.gd: `velocity -= ship.linear_velocity` on entering).
    pub fn change_frame(&mut self, old: &Frame, new: &Frame, frame_vel_change: DVec3) {
        let world_pos = old.to_world(self.pos);
        let world_vel = old.rot * self.vel + frame_vel_change;
        let world_fwd = old.rot * self.forward;
        self.pos = new.to_local(world_pos);
        self.vel = new.rot.inverse() * world_vel;
        self.move_vel = self.vel;
        self.forward = new.rot.inverse() * world_fwd;
    }

    /// One fixed step. `up` and `gravity` (m/s², magnitude) are in frame coordinates.
    pub fn step(&mut self, frame: &Frame, up: DVec3, gravity: f64, input: &WalkInput, world: &impl World, dt: f64) -> StepInfo {
        let mut info = StepInfo::default();
        self.align(up, input.yaw);
        let world_up = frame.rot * up;

        // Out of any overlap first (terrain patch appeared around us, ship turned into us).
        let push = world.depenetrate(frame.to_world(self.pos), world_up);
        if push.length_squared() > 0.0 {
            info.depenetrated = push.length();
            self.pos += frame.rot.inverse() * push;
        }

        // Weightless: nothing presses the feet onto a floor, so the walker can neither stand nor
        // push off; it keeps its velocity (issue #5) and only equipment changes it (#8).
        let weightless = gravity == 0.0;
        let mut jumping = false;
        if weightless {
            self.vel += input.accel * dt;
            self.move_vel = self.vel;
        } else {
            let right = self.forward.cross(up);
            let speed = if input.run { self.cfg.run_speed } else { self.cfg.walk_speed } * (1.0 - input.slow);
            let target = (right * input.dir.x + self.forward * input.dir.y).clamp_length_max(1.0) * speed;
            let current = self.move_vel - up * self.move_vel.dot(up);
            let start = self.cfg.start_speed;
            let horizontal = if current.length() < start - 1e-9 || target.length() < 1e-9 && current.length() <= start {
                // Stepping off or coming to a stop: ease below the step-off speed.
                let step = target.clamp_length_max(start);
                current + (step - current).clamp_length_max(start / self.cfg.start_time * dt)
            } else {
                target
            };
            self.move_vel = horizontal;
            let mut vertical = self.vel.dot(up);
            if self.grounded {
                jumping = input.jump;
                vertical = if jumping { self.cfg.jump_speed } else { 0.0 };
            } else {
                vertical -= gravity * dt;
            }
            self.vel = horizontal + up * vertical;
        }

        let was_grounded = self.grounded && !weightless;
        self.grounded = false;
        let mut motion = self.vel * dt;
        for _ in 0..self.cfg.max_slides {
            if motion.length_squared() < 1e-14 {
                break;
            }
            let world_motion = frame.rot * motion;
            let Some(hit) = world.sweep(frame.to_world(self.pos), world_up, world_motion) else {
                self.pos += motion;
                break;
            };
            info.hits += 1;
            let len = motion.length();
            let dir = motion / len;
            let travel = (hit.distance - self.cfg.skin).max(0.0).min(len);
            self.pos += dir * travel;
            let n = frame.rot.inverse() * hit.normal;
            let rest = motion - dir * travel;
            // Into the surface relative to its own motion (issue #9: a moving ship).
            let surface_v = frame.rot.inverse() * hit.velocity;
            let into = |v: DVec3| n * (v - surface_v).dot(n).min(0.0);
            if weightless {
                motion = rest - n * rest.dot(n).min(0.0);
                self.vel -= into(self.vel);
            } else if self.is_floor(n, up) {
                self.grounded = true;
                self.floor_normal = n;
                motion = rest - n * rest.dot(n).min(0.0);
                if !jumping {
                    // Standing on it: no further fall, slide horizontal speed along the floor.
                    self.vel -= into(self.vel);
                }
            } else if n.dot(up) > -0.1 {
                // Wall or too steep (also in the air): slide along it, never upwards.
                motion = rest - n * rest.dot(n).min(0.0);
                motion -= up * motion.dot(up).max(0.0);
                self.vel -= into(self.vel);
                self.vel -= up * self.vel.dot(up).max(0.0);
            } else {
                motion = rest - n * rest.dot(n).min(0.0);
                self.vel -= into(self.vel);
            }
        }

        // Floor snap (Godot floor_snap_length): stay on the ground over small steps and crests.
        if !self.grounded && was_grounded && !jumping && self.vel.dot(up) <= 1e-6 {
            let probe = -world_up * self.cfg.snap_length;
            if let Some(hit) = world.sweep(frame.to_world(self.pos), world_up, probe) {
                let n = frame.rot.inverse() * hit.normal;
                if self.is_floor(n, up) {
                    self.pos -= up * (hit.distance - self.cfg.skin).max(0.0);
                    self.grounded = true;
                    self.floor_normal = n;
                    info.snapped = true;
                    self.vel -= up * self.vel.dot(up);
                }
            }
        }
        info
    }
}
