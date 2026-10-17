//! types, in f64 (concept repo `design/interaction.md`).
//! All values are starting points for the playtest.
pub mod authority;
pub mod container;
pub mod defs;
pub mod inventory;
pub mod supply;
pub mod wire;

pub use authority::{Action, Client, Event, FrameRef, Host, Loose, LooseId, Placement, PlayerId, Reject, Request};
pub use defs::{ItemDef, ItemDefs, Take};
pub use inventory::{Inventory, BAR_SLOTS, CANISTER_TAG, SUIT_TAG};
pub use authority::RackId;
