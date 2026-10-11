//! heat_core: the wanted level, scans and their answers, paying it down (#227), without engine
//! types. Heat rises with broken rules and deals, decays with time, and official pads scan the
//! crew more often the hotter it is. A scan is a choice: comply, bribe, talk or flee.
//!
//! - `tuning`: the `heat/tuning.json` record and its checked loader.
//! - `patrol`: air patrols that order a hot ship in an airspace to halt, and their cooldown (#231).
//! - `state`: `Heat`, the points, the stars, the open scan, the seeded rolls, the save section.
//!
//! Heat always decays and can be paid down, so no mistake locks the crew out for good.
pub mod patrol;
pub mod state;
pub mod tuning;

pub use patrol::{Patrol, PatrolOut};
pub use state::{Heat, Outcome, SECTION, SECTION_VERSION};
pub use tuning::{CLEAN_AFTER_S, FOLDER, HeatContent, NOTICE_DOWN, NOTICE_UP, STARS_MAX, Tuning, ViolationPoints};
