//! planet_core: plain Rust, no engine types. Recipe, bake, height function, chunk build, scatter, sites, statistics.
pub mod chunk;
pub mod drainage;
pub mod landform;
pub mod look;
pub mod math;
pub mod planet;
pub mod recipe;
pub mod scatter;
pub mod site;
pub use landform::PlacedStamp;
pub use look::{Atlas, AtlasLayer, Spot, TimeShot, Viewpoint, Viewpoints};
pub use chunk::{ChunkOut, GRID, M};
pub use site::{Piece, Site};
pub use scatter::{EntryInfo, Instance, ScatterCell};
pub use math::*;
pub use planet::{BakeStats, Planet, Sample};
pub use recipe::Recipe;
