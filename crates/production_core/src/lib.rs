//! production_core: recipes and stations, groundwork for production chains (G), and the round 6
//! lab (#228). A station transforms commodities: it has a recipe, state, and inputs/outputs inventory.
//! A recipe defines inputs (commodity id + amount), outputs, station kind, and time.
//!
//! - `recipe`: the `recipe` record, validated loader, RecipeId and StationKind.
//! - `station`: Station state machine (Idle, Loading, Running, Done, Blocked), input/output methods.
//! - `state`: Production state, save section.
//! - `ingredient`: the lab's records (`ingredient`, `product_base`, `lab/tuning.json`) and their loader.
//! - `mixing`: the pure rules of a brew (mix, needle, quality, price).
//! - `lab`: kettles, the recipe book, the save section (`lab`).
//! - `devices`: the device chain after the kettle (#287): `Batch`, heater, centrifuge, syringe
//!   filler and syringe, their tuning (`lab/devices.json`).
//! - `skill`: how skill scales device times and rolls bonus quality; seeded `Dice`.
//!
//! Taken from Barotrauma: the shape of its fabricator's skill formulas (time scaled by the skill
//! margin over the requirement, two bonus quality tiers rolled by skill) and the skill-based
//! success roll when a medical item is applied.

pub mod devices;
pub mod ingredient;
pub mod lab;
pub mod mixing;
pub mod recipe;
pub mod skill;
pub mod station;
pub mod state;

pub use devices::{Batch, Centrifuge, DeviceRefusal, DeviceTuning, Dose, Heater, Spin, Syringe, Target, fill_syringe};
pub use ingredient::{Ingredient, LabContent, ProductBase, Transform, Tuning};
pub use lab::{Kettle, Lab, Outcome as LabOutcome, Refusal as LabRefusal};
pub use mixing::{mix, needle, price, quality};
pub use recipe::{Recipe, RecipeContent, RecipeId, StationKind};
pub use station::{Refusal, Station};
pub use state::{Production, SECTION, SECTION_VERSION};
