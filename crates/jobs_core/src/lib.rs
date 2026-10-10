//! jobs_core: jobs and their objectives as a system on top of `gameplay_core`, without engine
//! types (#123, #124; the board follows in #126).
//!
//! - `template`: the `job_template` record (objectives, reward, grading, deadline, modifiers,
//!   track, availability, once-only, follow-up) and its checked loader.
//! - `job`: offers and jobs as a state machine over domain events; the deliver objective as a
//!   reducer over crate events. Results are `Outcome`s for the host: spawn or release crates, and
//!   new domain events (payout, XP, the flag `job_completed:<template>`).
//! - save section: `Jobs::save` and `Jobs::load` (#128), versioned on their own.
//! - `giver`, `briefing`: the named contacts jobs come from (voice, standing, history) and briefings
//!   assembled from their parts (#167).
//! - `licence`: licences, per player, earned with an exam (an exam is a job template with checks:
//!   take off, reach a pad, land; #169).
//! - `map`: the map's pin list and the tracked job's next stop (#166).
//! - `grading`: payout = reward × band(delivered share) × condition factor × hazard factor.
//!
//! This crate reads only domain events and kernel state; it never calls another system.
pub mod briefing;
pub mod giver;
pub mod grading;
pub mod id;
pub mod job;
pub mod licence;
pub mod map;
pub mod template;

pub use briefing::{Briefing, briefing, briefing_with};
pub use giver::{Giver, GiverHistory, GiverId, Mood};
pub use grading::{Grade, grade};
pub use id::{JobId, TemplateId};
pub use job::{Check, CheckKind, CheckState, CrateMark, Job, JobEvent, JobState, Jobs, Leg, MAX_ACTIVE, ORDER_TEMPLATE, OrderTerms, Outcome, Refusal};
pub use licence::{Licence, LicenceId};
pub use template::{Exam, Honours, JobContent, JobTemplate};
