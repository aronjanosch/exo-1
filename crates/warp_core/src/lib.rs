//! warp_core: planet registry, quantum drive state machine, speed curve and path, without Bevy
//! types, in f64 (spike 11). Modelled on the structure of Star Citizen's quantum travel
//! (`research/quantum-drive-reference.md`): spool up, align and calibrate, two-stage ramp, flight
//! on a spline, ramp-down, cooldown. Our own names and numbers; all values are test values
//! (`content/system/system.json`), not designed.
pub mod drive;
pub mod path;
pub mod system;

pub use drive::{Abort, Drive, Event, Phase, ShipView};
pub use path::{Blocker, Path};
pub use system::{DriveConfig, Obstacle, PlanetDef, PlanetId, System};

#[cfg(test)]
mod tests;
