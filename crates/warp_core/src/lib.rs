//! warp_core: planet registry, quantum drive state machine, speed curve and path, without Bevy
pub mod drive;
pub mod load;
pub mod path;
pub mod system;

pub use drive::{Abort, Drive, Event, Phase, ShipView};
pub use load::{Load, Loader, Pending};
pub use path::{Blocker, Path};
pub use system::{DriveConfig, Obstacle, PlanetDef, PlanetId, StarDef, System};

#[cfg(test)]
mod tests;
