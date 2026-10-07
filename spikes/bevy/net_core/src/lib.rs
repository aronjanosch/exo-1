//! net_core: snapshot format, interpolation buffer, clock sync, fault injection and the replay
//! metrics of spike 4, without Bevy types, in f64. Port of spikes/network/{snapshot,buffer,link}.gd.
//! No sockets here: the replay matrix runs in `cargo test` (see `replay`).
//! All numbers are spike test values (assumptions), not designed.
pub mod buffer;
pub mod clock;
pub mod link;
pub mod metrics;
pub mod replay;
pub mod snapshot;
pub mod wire;

/// Planet centres in the shared world frame (spike 4: two planets 200 km apart). Assumption.
pub const PLANET_CENTRES: [glam::DVec3; 2] = [glam::DVec3::ZERO, glam::DVec3::new(200_000.0, 0.0, 0.0)];
pub const MAX_OWNER: u32 = 8;

/// Planet-relative to world (f64, exact for any planet). Shared frame of spike 4: snapshots hold
/// no per-client local coordinates, so every client rebuilds its own view from these.
pub fn to_world(planet: u32, rel: glam::DVec3) -> glam::DVec3 {
    PLANET_CENTRES[planet as usize] + rel
}
pub fn to_planet(planet: u32, world: glam::DVec3) -> glam::DVec3 {
    world - PLANET_CENTRES[planet as usize]
}
