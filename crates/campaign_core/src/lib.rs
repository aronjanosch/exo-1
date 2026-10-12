//! campaign_core: the inputs of the star system generator, without Bevy types, in f64. Seeded
//! random numbers (SplitMix64, the same on every platform, since a save regenerates the campaign
//! map from its seed), the planet set, the name pools and the three system templates. All numbers
//! are TODO(initiator) assumptions in `content/campaign/`.
pub mod generate;
pub mod names;
pub mod rng;
pub mod set;
pub mod template;

pub use generate::{open_sky, Enemy, Generated, Generator, Site, Station};
pub use names::Names;
pub use rng::Rng;
pub use set::{PlanetSet, RecipeInfo};
pub use template::{EnemyKind, EnemyRule, PlanetRule, SiteRule, StarRule, StationRule, SystemKind, Template};

#[cfg(test)]
mod tests;
