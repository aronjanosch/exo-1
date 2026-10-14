//! item_core: items, the player's inventory and the path every change takes, without engine
//! types, in f64 (step C1 of the interaction model; concept repo `design/interaction.md`).
//!
//! - `defs`: item definitions from data (`content/items/*.json`), built from components: how it
//!   is taken (collected into the inventory, or carried with the hands), how many hands it needs,
//!   its stack size, its own slots, its use effect (inert until items are used) and its shape.
//! - `container`: rows of slots with rules (from the dev-area round, #287); the inventory is one.
//! - `inventory`: the action bar, eight slots, the selected one is in the hand.
//! - `authority`: every change is a request from a player to the host, which confirms or rejects
//!   it and sends events; the acting client shows its request at once and reconciles on the
//!   answer (as in Barotrauma, which also plays over the internet).
//! - `wire`: the messages as bytes for the network (C2): what a client sends the host and what the
//!   host sends back, over a reliable ordered channel (`net_core::reliable`).
//!
//! All values are starting points for the playtest.
pub mod authority;
pub mod container;
pub mod defs;
pub mod inventory;
pub mod supply;
pub mod wire;

pub use authority::{Action, Client, Event, FrameRef, Host, Loose, LooseId, Placement, PlayerId, Reject, Request};
pub use defs::{ItemDef, ItemDefs, Take};
pub use inventory::{Inventory, BAR_SLOTS};
pub use authority::RackId;
