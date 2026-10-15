//! gameplay_core: the kernel every gameplay system builds on, without engine types.
//! Systems talk to each other only through domain events and conditions. A system's outcome is
//! more events (a payout is `TrackChanged`, a finished job raises a flag) that the host routes.
pub mod condition;
pub mod content;
pub mod event;
pub mod id;
pub mod notice;
pub mod product;
pub mod progress;
pub mod rng;
pub mod save;
pub mod text;

pub use condition::Condition;
pub use content::{Content, File};
pub use event::{Dedup, Event, EventId, WorldEvent};
pub use product::ProductSpec;
pub use id::*;
pub use progress::{Progress, Refusal};
