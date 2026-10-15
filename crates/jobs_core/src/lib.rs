//! This crate reads only domain events and kernel state; it never calls another system.
pub mod board;
pub mod briefing;
pub mod giver;
pub mod grading;
pub mod id;
pub mod job;
pub mod licence;
pub mod map;
pub mod template;
pub mod texts;

pub use briefing::{Briefing, briefing, briefing_with};
pub use giver::{Giver, GiverHistory, GiverId, Mood};
pub use grading::{Grade, grade};
pub use id::{JobId, TemplateId};
pub use job::{Check, CheckKind, CheckState, CrateMark, Job, JobEvent, JobState, Jobs, Leg, MAX_ACTIVE, ORDER_TEMPLATE, OrderTerms, Outcome, Refusal};
pub use licence::{Licence, LicenceId};
pub use template::{Exam, Honours, JobContent, JobTemplate};
pub use texts::check_glue_keys;
