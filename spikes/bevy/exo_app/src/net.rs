//! Spike 10 glue shared by the recorder and the live network: building a snapshot of the local
//! ship and walker in the shared frame (planet id plus planet-relative f64).
use crate::env::PlanetRes;
use crate::walker::Player;
use avian3d::prelude::*;
use bevy::math::{DMat3, DQuat, DVec3};
use net_core::snapshot::{FrameKind, Snapshot};

/// Orientation of a walker from its heading and up (same convention as the camera look basis).
pub fn walker_quat(forward: DVec3, up: DVec3) -> DQuat {
    let f = (forward - up * forward.dot(up)).normalize_or_zero();
    let f = if f == DVec3::ZERO { DVec3::NEG_Z } else { f };
    DQuat::from_mat3(&DMat3::from_cols(f.cross(up), up, -f))
}

/// Snapshot of the local ship and walker. `owner` is the slot (1..8); the walker is described in
/// the frame it lives in: a ship cabin (always the local ship here) or the planet.
pub fn build_snapshot(owner: u32, planet_id: u32, t: f64, seq: u32, planet: &PlanetRes, ship: (&Position, &Rotation, &LinearVelocity), player: &Player) -> Snapshot {
    let mut s = Snapshot::new(owner, t, ship.0.0 - planet.centre, ship.2.0, ship.1.0);
    s.planet = planet_id;
    s.seq = seq;
    if player.ship.is_some() {
        s.frame = FrameKind::Ship;
        s.frame_id = owner;
        s.wp = player.w.pos;
        s.wv = player.w.vel;
        s.wq = walker_quat(player.w.forward, DVec3::Y);
    } else {
        s.frame = FrameKind::Planet;
        s.frame_id = 0;
        s.wp = player.w.pos - planet.centre;
        s.wv = player.w.vel;
        s.wq = walker_quat(player.w.forward, planet.up(player.w.pos));
    }
    s
}
