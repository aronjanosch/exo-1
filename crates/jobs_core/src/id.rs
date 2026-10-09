//! Ids of the jobs system.
use std::fmt;

use serde::{Deserialize, Serialize};

/// A `job_template` record.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TemplateId(pub String);

impl TemplateId {
    pub fn new(s: impl Into<String>) -> TemplateId {
        TemplateId(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TemplateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One offer or job, numbered by `Jobs` in the order they are made.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(pub u64);
