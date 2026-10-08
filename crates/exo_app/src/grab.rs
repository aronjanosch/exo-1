//! Grab (#82, #83): the crate the walker holds, with the hands or the grab tool.
use crate::cargo::{crate_world, Crate};
use bevy::math::DVec3;
use bevy::prelude::*;
use grab_core::{BreakTimer, GrabConfig, Reach};
use walker_core::Frame;

#[derive(Clone, Copy, Debug)]
pub struct Held {
    pub crate_e: Entity,
    pub reach: Reach,
    /// Distance from the eye to the hold point (m); the tool reels it in.
    pub dist: f64,
    pub breaker: BreakTimer,
}

#[derive(Resource, Default, Debug)]
pub struct Grab {
    pub held: Option<Held>,
}

impl Grab {
    /// Takes hold of crate `c` (entity `e`); `eye` is world space, `ship` the own cabin's frame.
    pub fn start(&mut self, cfg: &GrabConfig, e: Entity, c: &Crate, reach: Reach, eye: DVec3, ship: &Frame) {
        let (centre, _) = crate_world(c, ship);
        let dist = match reach {
            Reach::Hands => cfg.hold_gap + c.body.half.z,
            Reach::Tool => centre.distance(eye),
        };
        self.held = Some(Held { crate_e: e, reach, dist, breaker: BreakTimer::default() });
    }

    pub fn release(&mut self) {
        self.held = None;
    }
}
