//! Licences (#169): per player, earned with an exam. A licence is a personal track (0 or 1) that
//! its exam job template grants; conditions ask for it like for any track. The seat of the ship
//! refuses without a licence that allows piloting; riding along and carrying never needs one.
use std::fmt;

use content_core::Record;
use gameplay_core::{Tag, TextKey, TrackId};
use serde::{Deserialize, Serialize};

use crate::id::TemplateId;

pub const FOLDER: &str = "licence";

/// A `licence` record.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LicenceId(pub String);

impl LicenceId {
    pub fn new(s: impl Into<String>) -> LicenceId {
        LicenceId(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LicenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Licence {
    pub id: LicenceId,
    pub name: TextKey,
    /// The personal track that is 1 once the licence is earned (the exam grants it).
    pub track: TrackId,
    /// The job template of the exam.
    pub exam: TemplateId,
    /// What it allows, as tags the game asks for (`pilot_ship`).
    pub allows: Vec<Tag>,
}

impl Record for Licence {
    type Id = LicenceId;
    fn id(&self) -> &LicenceId {
        &self.id
    }
}
