//! production_core: recipes and stations, groundwork for production chains (G).
//! A station transforms commodities: it has a recipe, state, and inputs/outputs inventory.
//! A recipe defines inputs (commodity id + amount), outputs, station kind, and time.
//!
//! - `recipe`: the `recipe` record, validated loader, RecipeId and StationKind.
//! - `station`: Station state machine (Idle, Loading, Running, Done, Blocked), input/output methods.
//! - `state`: Production state, save section.

pub mod recipe;
pub mod station;
pub mod state;

pub use recipe::{Recipe, RecipeContent, RecipeId, StationKind};
pub use station::{Refusal, Station};
pub use state::{Production, SECTION, SECTION_VERSION};
