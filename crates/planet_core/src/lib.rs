//! planet_core: plain Rust, no Godot types. Recipe, bake, height function, chunk build, scatter, sites, statistics.
pub mod chunk;
pub mod landform;
pub mod look;
pub mod math;
pub mod planet;
pub mod recipe;
pub mod scatter;
pub use landform::PlacedStamp;
pub use look::{Atlas, AtlasLayer, Spot, Viewpoint, Viewpoints};
pub use chunk::{ChunkOut, GRID, M};
pub use scatter::{EntryInfo, Instance, ScatterCell};
pub use math::*;
pub use planet::{BakeStats, Planet, Sample};
pub use recipe::Recipe;
