//! Spike 10 glue shared by the recorder and the live network: building a snapshot of the local
//! ship and walker in the shared frame (planet id plus planet-relative f64).
use crate::env::PlanetRes;
use crate::walker::Player;
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use net_core::snapshot::{FrameKind, Snapshot};

/// Orientation of a walker from its heading and up (camera axes, like the view).
pub fn walker_quat(forward: DVec3, up: DVec3) -> DQuat {
    walker_core::look_rot(forward - up * forward.dot(up), up)
}

/// Snapshot of the local ship and walker. `owner` is the slot (1..8); the walker is described in
/// the frame it lives in: a ship cabin or the planet.
/// `frame_owner`: slot of the ship whose cabin the walker is in (the own slot, or the owner of a
/// remote ship the walker boarded). Ignored while the walker is outside.
pub fn build_snapshot(owner: u32, frame_owner: u32, planet_id: u32, t: f64, seq: u32, planet: &PlanetRes, ship: (&Position, &Rotation, &LinearVelocity), player: &Player) -> Snapshot {
    let mut s = Snapshot::new(owner, t, ship.0.0 - planet.centre, ship.2.0, ship.1.0);
    s.planet = planet_id;
    s.seq = seq;
    if player.ship.is_some() {
        s.frame = FrameKind::Ship;
        s.frame_id = frame_owner;
        s.wp = player.w.pos;
        s.wv = player.w.vel;
        s.wq = walker_quat(player.w.forward, player.cabin_up);
    } else {
        s.frame = FrameKind::Planet;
        s.frame_id = 0;
        s.wp = player.w.pos - planet.centre;
        s.wv = player.w.vel;
        s.wq = player.body.unwrap_or_else(|| walker_quat(player.w.forward, player.up));
    }
    s
}
