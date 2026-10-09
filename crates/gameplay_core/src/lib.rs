//! gameplay_core: the kernel every gameplay system builds on, without engine types (#122, #125).
//! It knows no system: jobs, encounters and later farming or mining are crates of their own
//! (`DECISIONS.md` in the concept repo, "Gameplay crates and build order (D)").
//!
//! - `event`: the domain event envelope (id = sender + running number), the world events all
//!   systems share, and `Dedup`, which lets the host apply each id once.
//! - `content`: the shared records `commodity`, `location`, `progress_track`, `unlock`, loaded one
//!   per file and checked; systems check their references against it.
//! - `condition`: the small typed conditions from the data.
//! - `progress`: track values (money, XP), owned tags, bought unlocks, raised flags.
//!
//! Systems talk to each other only through domain events and conditions. A system's outcome is
//! more events (a payout is `TrackChanged`, a finished job raises a flag) that the host routes.
pub mod condition;
pub mod content;
pub mod event;
pub mod id;
pub mod progress;

pub use condition::Condition;
pub use content::{Content, File};
pub use event::{Dedup, Event, EventId, WorldEvent};
pub use id::*;
pub use progress::{Progress, Refusal};
