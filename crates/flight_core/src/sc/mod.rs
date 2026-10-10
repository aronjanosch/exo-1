//! The SC flight model (round 5, epic #143): a second flight model next to `axis`, switched with
//! F7. Built after the structure of Star Citizen's flight control as far as our research reaches
//! (concept repo `docs/research/flight-feel.md`, `docs/research/flight-controller-notes.md`, the
//! IFCS switches of the local records); own names, code and values. Thrust is a force on the
//! ship's mass, torque turns it against its inertia.
//!
//! One step runs stages in this order, each in its own file (one lane owns each in round 5):
//!
//! 1. `modes`: the pilot's switches (coupled, gravity compensation, G-safe, comstab, master mode,
//!    speed limiter, ...).
//! 2. `drive::begin`: the boost this step.
//! 3. `air`: what the atmosphere does (drag, lift, wind, turbulence, thrust lost in air).
//!    The proximity assist (`Modes::proximity`) is `linear`'s: it limits the descent near the ground.
//! 4. `linear`: the thrust the flight computer asks for, inside the thrust box.
//! 5. `angular`: the angular acceleration it asks for, inside the torque box.
//! 6. `drive::shape`: what the thrusters really give (spool, jerk, boost).
//! 7. Integration: force over mass and torque over inertia, plus gravity and air.
//!
//! All values are TODO(initiator) (`content/tuning/sc_*.json`).
pub mod air;
pub mod angular;
pub mod drive;
pub mod linear;
pub mod modes;
pub mod tuning;

pub use modes::{Master, ModeCmds, Modes};
pub use tuning::ScTuning;

use crate::axis::{Dirs, Rot, G0};
use crate::{BodyState, FlightInput, PlanetEnv};
use glam::{DQuat, DVec3};

/// What one step sees, computed once before the stages run.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub rot: DQuat,
    pub inv: DQuat,
    /// World position and velocity.
    pub pos: DVec3,
    pub v: DVec3,
    /// The velocity in ship space (x right, y up, z back).
    pub lv: DVec3,
    /// The angular velocity in ship space (x pitch, y yaw, z roll).
    pub w_local: DVec3,
    /// World gravity at the ship (m/s²).
    pub gravity: DVec3,
    /// The planet's up at the ship.
    pub up: DVec3,
    /// Air density 0..1 (1 at the surface).
    pub density: f64,
    /// m above the planet's reference sphere.
    pub altitude: f64,
    /// m above the terrain under the ship's centre.
    pub clearance: f64,
    pub dt: f64,
}

impl Frame {
    pub fn new(body: &BodyState, env: &impl PlanetEnv, dt: f64) -> Frame {
        let inv = body.rot.inverse();
        let p = env.to_planet(body.pos);
        let up = p.normalize_or_zero();
        let altitude = p.length() - env.radius();
        Frame {
            rot: body.rot,
            inv,
            pos: body.pos,
            v: body.lin_vel,
            lv: inv * body.lin_vel,
            w_local: inv * body.ang_vel,
            gravity: env.gravity_at(body.pos),
            up,
            density: env.density_at(body.pos),
            altitude,
            clearance: altitude - env.height_at(up),
            dt,
        }
    }
}

/// What the ship shows and what sound, camera and HUD read (the only state they read).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScStatus {
    pub coupled: bool,
    /// 1 = fully coupled, 0 = fully decoupled; between: the blend runs (`modes::ModeTuning::decouple_time`).
    pub coupling: f64,
    pub grav_comp: bool,
    pub g_safe: bool,
    pub comstab: bool,
    pub proximity: bool,
    pub landing: bool,
    pub wind_comp: bool,
    pub master: Master,
    /// Speed limiter as a share of the cap, 1 = off.
    pub limiter: f64,
    /// m/s: the speed cap that holds right now (master mode, boost, limiter, air, landing).
    pub cap: f64,
    pub speed: f64,
    /// The felt acceleration (the thrust over the mass), g.
    pub felt_g: f64,
    /// The thrusters gave less than the flight computer asked.
    pub saturated: bool,
    pub braking: bool,
    pub boost_charge: f64,
    pub boost_active: bool,
    pub boost_ready: bool,
    /// Signed share of the full thrust per ship axis (x right, y up, z back), -1..1.
    pub thrust_share: DVec3,
    /// m/s, world: the wind at the ship.
    pub wind: DVec3,
    /// 0..1: how hard the air shakes the ship (camera trauma source).
    pub turbulence: f64,
    /// G-safe or comstab lowered the turn rate.
    pub rate_capped: bool,
}

impl Default for ScStatus {
    fn default() -> Self {
        ScStatus {
            coupled: true,
            coupling: 1.0,
            grav_comp: true,
            g_safe: true,
            comstab: true,
            proximity: true,
            landing: false,
            wind_comp: true,
            master: Master::Scm,
            limiter: 1.0,
            cap: 0.0,
            speed: 0.0,
            felt_g: 0.0,
            saturated: false,
            braking: false,
            boost_charge: 1.0,
            boost_active: false,
            boost_ready: true,
            thrust_share: DVec3::ZERO,
            wind: DVec3::ZERO,
            turbulence: 0.0,
            rate_capped: false,
        }
    }
}

/// One step's result. The velocities are what the caller writes to the body; force and torque are
/// the same step as forces (world, N and N m), for a caller that lets the physics integrate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScOut {
    pub lin_vel: DVec3,
    pub ang_vel: DVec3,
    pub force: DVec3,
    pub torque: DVec3,
}

/// The SC flight model: tuning, the state of each stage and the status.
#[derive(Clone, Debug)]
pub struct ScShip {
    pub tuning: ScTuning,
    pub modes: Modes,
    pub linear: linear::LinearState,
    pub angular: angular::AngularState,
    pub drive: drive::DriveState,
    pub air: air::AirState,
    pub status: ScStatus,
}

impl Default for ScShip {
    fn default() -> Self {
        ScShip::new(ScTuning::default())
    }
}

impl ScShip {
    pub fn new(tuning: ScTuning) -> ScShip {
        ScShip { tuning, modes: Modes::default(), linear: Default::default(), angular: Default::default(), drive: Default::default(), air: Default::default(), status: ScStatus::default() }
    }

    /// Test setup after placing the ship by hand: switches as at the start, no state left.
    pub fn reset_state(&mut self) {
        let t = self.tuning.clone();
        *self = ScShip::new(t);
    }

    /// A step the caller does not fly (parked, the quantum drive holds the ship): the boost is
    /// released and its meter runs on.
    pub fn skip_step(&mut self, dt: f64) {
        drive::begin(&mut self.drive, false, false, &self.tuning.drive, dt);
        self.status.boost_active = false;
        self.status.boost_charge = self.drive.boost.charge;
    }

    /// The thrust box per direction (m/s²): the thrusters' force over the mass, times what the air
    /// leaves and the boost adds.
    pub fn thrust_box(&self, thrust_scale: f64, boost: f64) -> Dirs {
        let t = &self.tuning;
        drive::boosted(&t.ship.thrust.scaled(thrust_scale / t.ship.mass), &t.drive, boost)
    }

    /// The angular acceleration box per axis (rad/s²): the torque over the inertia.
    pub fn torque_box(&self) -> Rot {
        let s = &self.tuning.ship;
        Rot { pitch: s.torque.pitch / s.inertia.pitch, yaw: s.torque.yaw / s.inertia.yaw, roll: s.torque.roll / s.inertia.roll }
    }

    /// One physics step.
    pub fn step(&mut self, body: &BodyState, input: &FlightInput, cmds: &ModeCmds, env: &impl PlanetEnv, dt: f64) -> ScOut {
        let f = Frame::new(body, env, dt);
        self.modes.update(cmds, &self.tuning.modes, dt);
        let braking = input.piloted && input.brake;
        let boost = drive::begin(&mut self.drive, input.boost, braking, &self.tuning.drive, dt);
        let air = air::step(&mut self.air, &f, &self.modes, &self.tuning.air);
        let thrust_box = self.thrust_box(air.thrust_scale, if braking { 1.0 } else { boost });
        let lin = linear::step(&mut self.linear, &f, input, &self.modes, &linear::Env { thrust_box, boost, braking, air_accel: air.accel, cap_scale: air.cap_scale, drift: if self.modes.wind_comp { DVec3::ZERO } else { air.wind } }, &self.tuning.linear);
        let torque_box = self.torque_box();
        let ang = angular::step(&mut self.angular, &f, input, &self.modes, &angular::Env { accel_box: torque_box, boost, cap: lin.cap, thrust_box, air_hold: air.angular_hold }, &self.tuning.angular);
        let shaped = drive::shape(&mut self.drive, &drive::Asked { linear: lin.accel, angular: ang.accel, boost, thrust_box }, &self.tuning.drive, dt);

        let s = &self.tuning.ship;
        let linear_world = f.rot * shaped.linear;
        let v = f.v + (linear_world + f.gravity + air.accel + air.push) * dt;
        let angular_world = f.rot * (shaped.angular + air.angular + air.angular_hold);
        let w = body.ang_vel + angular_world * dt;
        let torque_local = DVec3::new(shaped.angular.x * s.inertia.pitch, shaped.angular.y * s.inertia.yaw, shaped.angular.z * s.inertia.roll);

        let share = |a: f64, pos: f64, neg: f64| if a >= 0.0 { a / pos } else { a / neg };
        let full = self.thrust_box(1.0, 0.0);
        let m = &self.modes;
        self.status = ScStatus {
            coupled: m.coupled,
            coupling: m.coupling,
            grav_comp: m.grav_comp,
            g_safe: m.g_safe,
            comstab: m.comstab,
            proximity: m.proximity,
            landing: m.landing,
            wind_comp: m.wind_comp,
            master: m.master,
            limiter: m.limiter,
            cap: lin.cap,
            speed: v.length(),
            felt_g: shaped.linear.length() / G0,
            saturated: lin.saturated,
            braking,
            boost_charge: self.drive.boost.charge,
            boost_active: self.drive.boost.active,
            boost_ready: self.drive.boost.ready(&self.tuning.drive.boost_capacitor),
            thrust_share: DVec3::new(
                share(shaped.linear.x, full.right, full.left),
                share(shaped.linear.y, full.up, full.down),
                share(shaped.linear.z, full.backward, full.forward),
            )
            .clamp(DVec3::NEG_ONE, DVec3::ONE),
            wind: air.wind,
            turbulence: air.turbulence,
            rate_capped: ang.rate_capped,
        };
        ScOut { lin_vel: v, ang_vel: w, force: linear_world * s.mass, torque: f.rot * torque_local }
    }
}
