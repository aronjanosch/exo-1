//! grab_core: crates and how hands and the grab tool move them, without engine types, in f64
//! (milestone C, #80 and #81). Learned from the hold-point regulators of other games
//! (`research/grab-and-cargo.md` in the concept repo); our own implementation and numbers.
//!
//! - `CrateTable`: the standard crate sizes (`content/cargo/crates.json`).
//! - Hold: a velocity servo towards a hold point with a force cap per holder and a speed cap
//!   that falls with mass, so heavy crates lag. Several holders add their caps (shared carry).
//! - `budget`: which loose objects go (cap, persistence cap, timeout, distance), #85.
//! - `CrateBody`: a box that falls, slides with friction and sleeps at rest, moved by sweeps
//!   through a `BoxWorld`. It lives in a frame like the walker (planet or ship cabin).
//!
//! All values are starting points for the playtest (`TODO(initiator)` in the data).
pub mod budget;

use glam::{DMat3, DQuat, DVec3};
use serde::Deserialize;
use walker_core::{Frame, Hit};

// ---------- data ----------

/// One standard crate size (#80).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CrateSize {
    pub name: String,
    /// Full edge lengths (x, y, z), metres.
    pub extents: [f64; 3],
    /// kg.
    pub mass: f64,
    /// 1 = one hand, 2 = both hands (carry state, `carry`).
    pub hands: u8,
    /// Players it takes to lift it (the hold caps add up).
    pub holders: u8,
}

impl CrateSize {
    pub fn half(&self) -> DVec3 {
        DVec3::from_array(self.extents) * 0.5
    }
}

/// `content/cargo/crates.json`: sizes whose edges double from row to row.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CrateTable {
    pub sizes: Vec<CrateSize>,
}

impl CrateTable {
    pub fn from_json(s: &str) -> Result<CrateTable, String> {
        let t: CrateTable = content_core::parse_strict("crates.json", s)?;
        if t.sizes.is_empty() {
            return Err("crates.json: no sizes".into());
        }
        for (i, z) in t.sizes.iter().enumerate() {
            let at = format!("crates.json: size '{}'", z.name);
            if z.extents.iter().any(|e| !(*e > 0.0)) {
                return Err(format!("{at}: extents must be positive"));
            }
            if !(z.mass > 0.0) {
                return Err(format!("{at}: mass must be positive"));
            }
            if !(1..=2).contains(&z.hands) {
                return Err(format!("{at}: hands must be 1 or 2"));
            }
            if z.holders == 0 {
                return Err(format!("{at}: holders must be at least 1"));
            }
            if t.sizes[..i].iter().any(|o| o.name == z.name) {
                return Err(format!("{at}: name used twice"));
            }
            if i > 0 {
                let prev = &t.sizes[i - 1];
                if (0..3).any(|k| (z.extents[k] / prev.extents[k] - 2.0).abs() > 1e-6) {
                    return Err(format!("{at}: edges must double from '{}'", prev.name));
                }
            }
        }
        Ok(t)
    }

    pub fn get(&self, name: &str) -> Option<&CrateSize> {
        self.sizes.iter().find(|s| s.name == name)
    }
}

/// `content/tuning/grab.json`.
#[derive(Deserialize, Copy, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GrabConfig {
    /// Hands reach this far (m) at full force, nothing beyond.
    pub hand_range: f64,
    /// The grab tool: full force to here (m), then linear down to zero at `tool_max_range`.
    pub tool_full_range: f64,
    pub tool_max_range: f64,
    /// Force cap of one holder (N), hands and tool alike.
    pub hand_force: f64,
    /// Up to this mass (kg) the hold reaches `max_speed`; above it the speed cap falls as 1/mass.
    pub ref_mass: f64,
    pub max_speed: f64,
    /// The speed cap never falls below this (m/s).
    pub min_speed: f64,
    /// Wanted closing speed per metre of error (1/s).
    pub gain: f64,
    /// Time constant (s) in which the hold reaches the wanted speed, force permitting.
    pub response_time: f64,
    /// Error (m) that counts towards the break timer.
    pub break_distance: f64,
    /// The hold breaks after the error stayed above `break_distance` this long (s).
    pub break_time: f64,
    /// Throw impulse (N s) and the most speed it gives (m/s).
    pub throw_impulse: f64,
    pub throw_max_speed: f64,
    /// Mass of a player for reaction forces (kg).
    pub holder_mass: f64,
    /// Turn rate of a held crate (rad/s), falls with mass like the speed cap.
    pub max_turn_rate: f64,
    /// Share of the turn rate left while the crate touches something.
    pub contact_turn_share: f64,
    /// The view turns at `turn_ref_mass / (turn_ref_mass + mass)` of its rate while holding.
    pub turn_ref_mass: f64,
    /// Walking speed share with both hands busy.
    pub two_hand_speed_share: f64,
    /// Friction coefficient of a crate on any floor, also as an impulse at each impact.
    /// TODO(initiator): start value 0.8 (was 0.5; crates slid too far in the playtest).
    pub friction: f64,
    /// Below this speed (m/s) for `sleep_time` (s) a crate on a floor sleeps.
    pub sleep_speed: f64,
    pub sleep_time: f64,
    /// Interaction cone, half angle in degrees.
    pub cone_half_angle_deg: f64,
    /// Gap between the eye and the near face of a crate held in the hands (m).
    pub hold_gap: f64,
    /// The tool reels a crate in to this distance from the eye (m), at `tool_reel_speed` (m/s).
    pub tool_hold_distance: f64,
    pub tool_reel_speed: f64,
    /// Impacts up to this speed (m/s) leave a crate's condition alone (#130).
    pub impact_safe_speed: f64,
    /// Condition lost per m/s of impact speed above the safe speed (condition runs 1 to 0).
    pub impact_loss_per_speed: f64,
}

impl GrabConfig {
    pub fn from_json(s: &str) -> Result<GrabConfig, String> {
        let c: GrabConfig = content_core::parse_strict("grab.json", s)?;
        if !(c.hand_range > 0.0 && c.tool_full_range >= c.hand_range && c.tool_max_range > c.tool_full_range) {
            return Err("grab.json: ranges must grow: hand_range <= tool_full_range < tool_max_range".into());
        }
        if c.impact_safe_speed < 0.0 || c.impact_loss_per_speed < 0.0 {
            return Err("grab.json: impact_safe_speed and impact_loss_per_speed must not be negative".into());
        }
        Ok(c)
    }
}

/// Condition a crate loses in one impact at `speed` (m/s into the surface): nothing up to the
/// safe speed, then linear in the excess (#130).
pub fn impact_loss(cfg: &GrabConfig, speed: f64) -> f64 {
    (speed - cfg.impact_safe_speed).max(0.0) * cfg.impact_loss_per_speed
}

// ---------- hold ----------

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Reach {
    Hands,
    Tool,
}

/// One player holding a crate. All vectors in one frame.
#[derive(Copy, Clone, Debug)]
pub struct Holder {
    /// Where this holder wants the crate's centre.
    pub target: DVec3,
    /// Velocity of the hold point (the holder's own).
    pub target_vel: DVec3,
    /// The hand or tool tip the force falls off from (the eye).
    pub source: DVec3,
    pub reach: Reach,
}

#[derive(Clone, Debug, Default)]
pub struct HoldOutput {
    /// Force on the crate (N), gravity compensation included.
    pub force: DVec3,
    /// Reaction force on each holder, in the order given; they sum to `-force`.
    pub reactions: Vec<DVec3>,
}

/// Share of the force cap left at `distance` from the source.
pub fn falloff(cfg: &GrabConfig, reach: Reach, distance: f64) -> f64 {
    match reach {
        Reach::Hands => (distance <= cfg.hand_range) as u8 as f64,
        Reach::Tool => ((cfg.tool_max_range - distance) / (cfg.tool_max_range - cfg.tool_full_range)).clamp(0.0, 1.0),
    }
}

/// Speed cap of the hold for a crate of `mass`.
pub fn speed_cap(cfg: &GrabConfig, mass: f64) -> f64 {
    (cfg.max_speed * (cfg.ref_mass / mass).min(1.0)).max(cfg.min_speed)
}

/// Force on a crate (`mass`, `pos`, `vel`) from its holders, in gravity `g` (acceleration vector).
/// The holders aim for the mean of their targets; their caps add up, each scaled by its falloff.
pub fn hold_force(cfg: &GrabConfig, mass: f64, pos: DVec3, vel: DVec3, g: DVec3, holders: &[Holder]) -> HoldOutput {
    if holders.is_empty() {
        return HoldOutput::default();
    }
    let n = holders.len() as f64;
    let target = holders.iter().map(|h| h.target).sum::<DVec3>() / n;
    let target_vel = holders.iter().map(|h| h.target_vel).sum::<DVec3>() / n;
    let caps: Vec<f64> = holders.iter().map(|h| cfg.hand_force * falloff(cfg, h.reach, pos.distance(h.source))).collect();
    let total: f64 = caps.iter().sum();
    let want_vel = target_vel + ((target - pos) * cfg.gain).clamp_length_max(speed_cap(cfg, mass));
    let want = mass * ((want_vel - vel) / cfg.response_time - g);
    let force = want.clamp_length_max(total);
    let reactions = caps.iter().map(|c| if total > 0.0 { -force * (c / total) } else { DVec3::ZERO }).collect();
    HoldOutput { force, reactions }
}

/// Acceleration of a holder from its reaction force (weightless: the crate pulls the player).
pub fn holder_accel(cfg: &GrabConfig, reaction: DVec3) -> DVec3 {
    reaction / cfg.holder_mass
}

/// Turn rate (rad/s) a held crate gets when `want` is asked: capped by mass, damped on contact.
pub fn turn_rate(cfg: &GrabConfig, mass: f64, want: f64, contact: bool) -> f64 {
    let cap = cfg.max_turn_rate * (cfg.ref_mass / mass).min(1.0) * if contact { cfg.contact_turn_share } else { 1.0 };
    want.clamp(-cap, cap)
}

/// Share of the view's turn rate left while holding a crate of `mass`.
pub fn view_turn_share(cfg: &GrabConfig, mass: f64) -> f64 {
    cfg.turn_ref_mass / (cfg.turn_ref_mass + mass)
}

/// Drops the hold after a sustained error, or at once when the holder stands on the crate.
#[derive(Copy, Clone, Debug, Default)]
pub struct BreakTimer {
    /// Seconds the error has stayed above `break_distance`.
    pub t: f64,
}

impl BreakTimer {
    /// True when the hold breaks this step.
    pub fn step(&mut self, cfg: &GrabConfig, error: f64, standing_on: bool, dt: f64) -> bool {
        if standing_on {
            return true;
        }
        self.t = if error > cfg.break_distance { self.t + dt } else { 0.0 };
        self.t >= cfg.break_time - 1e-9
    }
}

/// Velocity of a thrown crate: its own plus the throw impulse along `dir`, less for heavy crates.
pub fn throw_velocity(cfg: &GrabConfig, mass: f64, vel: DVec3, dir: DVec3) -> DVec3 {
    vel + dir.normalize() * (cfg.throw_impulse / mass).min(cfg.throw_max_speed)
}

/// Velocity change of the thrower (the opposite impulse).
pub fn throw_kick(cfg: &GrabConfig, mass: f64, dir: DVec3) -> DVec3 {
    let dv = (cfg.throw_impulse / mass).min(cfg.throw_max_speed);
    -dir.normalize() * dv * mass / cfg.holder_mass
}

/// What carrying costs the walker.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Carry {
    pub speed_share: f64,
    pub can_run: bool,
    pub can_jump: bool,
}

/// Carry state for a crate that takes `hands` hands (0 = nothing held).
pub fn carry(cfg: &GrabConfig, hands: u8) -> Carry {
    if hands >= 2 {
        Carry { speed_share: cfg.two_hand_speed_share, can_run: false, can_jump: false }
    } else {
        Carry { speed_share: 1.0, can_run: true, can_jump: true }
    }
}

/// Distance to `target` if it lies within `half_angle` (radians) of `look` and `max_dist` of `eye`.
pub fn in_cone(eye: DVec3, look: DVec3, target: DVec3, half_angle: f64, max_dist: f64) -> Option<f64> {
    let d = target - eye;
    let dist = d.length();
    if dist > max_dist {
        return None;
    }
    if dist < 1e-9 {
        return Some(0.0);
    }
    (d.angle_between(look) <= half_angle).then_some(dist)
}

// ---------- crate body ----------

/// What a crate needs from the physics world, world space. `half` are half extents, `rot` the
/// crate's orientation.
pub trait BoxWorld {
    /// First hit when moving the box by `motion` (not zero), or None.
    fn sweep(&self, center: DVec3, half: DVec3, rot: DQuat, motion: DVec3) -> Option<Hit>;
    /// Displacement out of every overlap (zero when free).
    fn depenetrate(&self, center: DVec3, half: DVec3, rot: DQuat) -> DVec3;
}

/// A crate as a box that stays upright about its frame's up and turns about it only.
/// Position (the centre) and velocity are in its frame, like the walker.
#[derive(Clone, Debug)]
pub struct CrateBody {
    pub half: DVec3,
    pub mass: f64,
    pub pos: DVec3,
    pub vel: DVec3,
    /// Heading (local -z), frame coordinates, perpendicular to `up`.
    pub forward: DVec3,
    /// Up of the frame at the crate, frame coordinates (last step's).
    pub up: DVec3,
    pub grounded: bool,
    /// Normal of the floor it stood on in the last step, frame coordinates.
    pub floor_normal: DVec3,
    /// Touched anything in the last step.
    pub contact: bool,
    pub asleep: bool,
    /// Largest impact speed (m/s into a surface) in the last step; 0 without one (#130).
    pub impact: f64,
    rest_t: f64,
}

const SKIN: f64 = 0.005;
const SLIDES: usize = 4;

impl CrateBody {
    pub fn new(size: &CrateSize, pos: DVec3, forward: DVec3) -> CrateBody {
        CrateBody { half: size.half(), mass: size.mass, pos, vel: DVec3::ZERO, forward, up: DVec3::Y, grounded: false, floor_normal: DVec3::Y, contact: false, asleep: false, impact: 0.0, rest_t: 0.0 }
    }

    /// Orientation in the frame: -z along `forward`, +y along `up`.
    pub fn rot(&self) -> DQuat {
        let up = self.up.normalize();
        let mut f = self.forward - up * self.forward.dot(up);
        if f.length_squared() < 1e-12 {
            f = up.any_orthonormal_vector();
        }
        let f = f.normalize();
        DQuat::from_mat3(&DMat3::from_cols(f.cross(up), up, -f))
    }

    pub fn wake(&mut self) {
        self.asleep = false;
        self.rest_t = 0.0;
    }

    /// Moves the crate into another frame, keeping its world pose. `frame_vel_change` is old
    /// frame velocity minus new frame velocity at the crate, world space (as
    /// `walker_core::Walker::change_frame`).
    pub fn change_frame(&mut self, old: &Frame, new: &Frame, frame_vel_change: DVec3) {
        let world_vel = old.rot * self.vel + frame_vel_change;
        self.pos = new.to_local(old.to_world(self.pos));
        self.vel = new.rot.inverse() * world_vel;
        let to_new = new.rot.inverse() * old.rot;
        self.forward = to_new * self.forward;
        self.up = to_new * self.up;
        self.wake();
    }

    /// One fixed step. `up` and `gravity` (m/s², magnitude) in frame coordinates as for the
    /// walker; `accel` is every other acceleration (hold force / mass, the cabin's inertia),
    /// frame coordinates; `turn` a turn rate about up (rad/s).
    #[allow(clippy::too_many_arguments)]
    pub fn step(&mut self, cfg: &GrabConfig, frame: &Frame, up: DVec3, gravity: f64, accel: DVec3, turn: f64, world: &impl BoxWorld, dt: f64) {
        self.up = up;
        self.impact = 0.0;
        if self.asleep {
            let limit = if gravity > 0.0 { cfg.friction * gravity } else { 1e-3 };
            let horizontal = accel - up * accel.dot(up);
            if horizontal.length() <= limit && accel.dot(up) < gravity && turn == 0.0 {
                return;
            }
            self.wake();
        }
        if turn != 0.0 {
            self.forward = DQuat::from_axis_angle(up, turn * dt) * self.forward;
        }
        let rot = frame.rot * self.rot();
        let push = world.depenetrate(frame.to_world(self.pos), self.half, rot);
        self.contact = push.length_squared() > 0.0;
        self.pos += frame.rot.inverse() * push;

        // On a floor, everything pulling along it fights friction against the floor's push
        // (Coulomb: static up to friction x normal force, then kinetic). Without a floor, free.
        let a = accel - up * gravity;
        let n = self.floor_normal;
        let pressing = -a.dot(n);
        if self.grounded && gravity > 0.0 && pressing > 0.0 {
            let limit = cfg.friction * pressing;
            let a_t = a + n * pressing;
            let v_n = self.vel.dot(n);
            let mut v_t = self.vel - n * v_n;
            if v_t.length() < 1e-2 && a_t.length() <= limit {
                // Held by static friction: no move at all this step.
                self.vel = DVec3::ZERO;
                self.contact = true;
                return self.settle(cfg, turn, dt);
            }
            v_t += a_t * dt;
            let s = v_t.length();
            v_t = if s > limit * dt { v_t * (1.0 - limit * dt / s) } else { DVec3::ZERO };
            self.vel = v_t + n * (v_n - pressing * dt);
        } else {
            self.vel += a * dt;
        }

        // Resting on a floor its friction already acts above; the impulse is for real impacts.
        let resting_on = self.grounded.then_some(self.floor_normal);
        self.grounded = false;
        let mut motion = self.vel * dt;
        for _ in 0..SLIDES {
            if motion.length_squared() < 1e-14 {
                break;
            }
            let Some(hit) = world.sweep(frame.to_world(self.pos), self.half, rot, frame.rot * motion) else {
                self.pos += motion;
                break;
            };
            self.contact = true;
            let len = motion.length();
            let dir = motion / len;
            let travel = (hit.distance - SKIN).max(0.0).min(len);
            self.pos += dir * travel;
            let n = frame.rot.inverse() * hit.normal;
            if n.dot(up) > 0.64 {
                self.grounded = true;
                self.floor_normal = n;
            }
            let rest = motion - dir * travel;
            motion = rest - n * rest.dot(n).min(0.0);
            // The impact stops the motion into the surface, and Coulomb friction as an impulse
            // takes friction x impact speed off the slide along it (never reverses it).
            let v_in = -self.vel.dot(n);
            let same_floor = resting_on.is_some_and(|f| f.dot(n) > 0.999);
            if v_in > 0.0 && !same_floor {
                self.impact = self.impact.max(v_in);
                let v_t = self.vel + n * v_in;
                let s = v_t.length();
                let keep = if s > 0.0 { (1.0 - cfg.friction * v_in / s).max(0.0) } else { 0.0 };
                self.vel = v_t * keep;
                motion = (motion - n * motion.dot(n)) * keep + n * motion.dot(n);
            }
        }

        if !(self.grounded || gravity == 0.0) {
            self.rest_t = 0.0;
            return;
        }
        self.settle(cfg, turn, dt);
    }

    /// Counts time at rest (on a floor or weightless); after `sleep_time` the crate sleeps.
    fn settle(&mut self, cfg: &GrabConfig, turn: f64, dt: f64) {
        let still = self.vel.length() < cfg.sleep_speed && turn == 0.0;
        self.rest_t = if still { self.rest_t + dt } else { 0.0 };
        if self.rest_t >= cfg.sleep_time {
            self.asleep = true;
            self.vel = DVec3::ZERO;
        }
    }
}

#[cfg(test)]
mod tests;
