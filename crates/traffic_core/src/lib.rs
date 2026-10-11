//! traffic_core: airspace, the tower, clearance and landing grades (round 6, #226), without
//! engine types. The tower answers a hail after a beat, may hold the sender, and assigns a pad
//! and a time window. Landings are graded; breaking a rule costs a fine and is said in the
//! tower's voice.
//!
//! - `town`: the `town` and `traffic_tuning` records, the checked loader, the text keys.
//! - `state`: `Traffic`, per pilot: airspace, speeding, clearance and the pending answer; the
//!   save section.
//!
//! The systems never call each other: the events come in through `apply_world`, the glue calls
//! `observe` each step with the airspace the ship is in.
pub mod state;
pub mod town;

pub use state::{Clearance, Outcome, Pending, Traffic, SECTION, SECTION_VERSION};
pub use town::{Town, Tuning, TrafficContent, pool_keys, town_of_pad, MIN_POOL};
