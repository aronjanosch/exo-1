//! Shared fixtures for the SC model's tests (round 5): the six tuning files, a deep-space and a
//! flat air planet, and a loop that flies the ship for a while. `mod sc_common;` in a test file.
#![allow(dead_code)]
use flight_core::sc::{ModeCmds, ScShip, ScTuning};
use flight_core::{BodyState, Field, FlightInput, PlanetEnv};
use glam::DVec3;

pub const DT: f64 = 1.0 / 60.0;

pub fn tuning() -> ScTuning {
    ScTuning::from_json([
        include_str!("../../../../content/tuning/sc_ship.json"),
        include_str!("../../../../content/tuning/sc_modes.json"),
        include_str!("../../../../content/tuning/sc_linear.json"),
        include_str!("../../../../content/tuning/sc_angular.json"),
        include_str!("../../../../content/tuning/sc_drive.json"),
        include_str!("../../../../content/tuning/sc_air.json"),
    ])
    .unwrap()
}

/// Deep space: no gravity, no air.
pub struct Space {
    pub field: Field,
}

impl Default for Space {
    fn default() -> Self {
        Space { field: Field::default() }
    }
}

impl PlanetEnv for Space {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world
    }
    fn radius(&self) -> f64 {
        5000.0
    }
    fn height_at(&self, _dir: DVec3) -> f64 {
        0.0
    }
    fn field(&self) -> &Field {
        &self.field
    }
    fn gravity_at(&self, _world: DVec3) -> DVec3 {
        DVec3::ZERO
    }
    fn density_at(&self, _world: DVec3) -> f64 {
        0.0
    }
}

/// A planet seen from close by: gravity `g` straight down (-Y), air of `density`, flat ground at
/// y = 0 (the ship flies at y = height). World = planet frame shifted by the radius.
pub struct Air {
    pub field: Field,
    pub g: f64,
    pub density: f64,
}

impl Default for Air {
    fn default() -> Self {
        Air { field: Field::default(), g: 9.81, density: 1.0 }
    }
}

pub const RADIUS: f64 = 1.0e7;

impl PlanetEnv for Air {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world + DVec3::Y * RADIUS
    }
    fn radius(&self) -> f64 {
        RADIUS
    }
    fn height_at(&self, _dir: DVec3) -> f64 {
        0.0
    }
    fn field(&self) -> &Field {
        &self.field
    }
    fn gravity_at(&self, _world: DVec3) -> DVec3 {
        DVec3::NEG_Y * self.g
    }
    fn density_at(&self, _world: DVec3) -> f64 {
        self.density
    }
}

/// A ship at rest at `pos`, level, nose along -Z.
pub fn body_at(pos: DVec3) -> BodyState {
    BodyState { pos, ..BodyState::default() }
}

/// A piloted input with this thrust (local, x right, y up, z back).
pub fn thrust(t: DVec3) -> FlightInput {
    FlightInput { thrust: t, piloted: true, ..FlightInput::default() }
}

/// Flies `secs` with the same input, integrating the body like the app does. The taps in `cmds`
/// apply on the first step only.
pub fn fly(ship: &mut ScShip, body: &mut BodyState, input: &FlightInput, cmds: &ModeCmds, env: &impl PlanetEnv, secs: f64) {
    let n = (secs / DT).round() as usize;
    for i in 0..n {
        let c = if i == 0 { *cmds } else { ModeCmds { limiter_steps: 0, ..ModeCmds::default() } };
        let out = ship.step(body, input, &c, env, DT);
        body.lin_vel = out.lin_vel;
        body.ang_vel = out.ang_vel;
        body.integrate(DT);
    }
}
