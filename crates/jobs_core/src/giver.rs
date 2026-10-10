//! Givers (#167): the few named contacts jobs come from. A giver has a counter at a location, a
//! voice (pools of lines, with greetings by mood), a standing track and a history with the crew.
//! Standing rises with completed jobs and falls with failures, never below its floor: a mistake
//! does not lock a giver for good (research: "one mistake locking a whole faction").
use std::fmt;

use content_core::{Loaded, Record};
use gameplay_core::text::TextTable;
use gameplay_core::{Condition, LocationId, TextKey, TrackId};
use serde::{Deserialize, Serialize};

/// A `giver` record.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GiverId(pub String);

impl GiverId {
    pub fn new(s: impl Into<String>) -> GiverId {
        GiverId(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GiverId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The folder this reads.
pub const FOLDER: &str = "giver";

/// Pools of a voice need this many lines, so a line never comes twice in a row by lack of choice.
pub const MIN_POOL: usize = 3;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GiverKind {
    Legal,
    /// The small odd crime family; its jobs are not offered in milestone D.
    Family,
}

/// The keys of the greeting pools by mood.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Greetings {
    pub first_job: TextKey,
    pub regular: TextKey,
    pub after_failure: TextKey,
}

/// A giver's voice: text keys of pools in the text table.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Voice {
    pub greeting: Greetings,
    pub intro: TextKey,
    /// The reason when neither the cargo nor the route has one of its own.
    pub reason: TextKey,
    pub sign_off: TextKey,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Giver {
    pub id: GiverId,
    pub name: TextKey,
    pub kind: GiverKind,
    /// Where the counter is (a marker with a prompt that opens the briefing).
    pub location: LocationId,
    /// The crew track of its standing (a floor keeps it from going negative); its thresholds are
    /// the ranks that conditions of templates ask for.
    pub standing: TrackId,
    /// Standing gained per completed job. TODO(initiator): a starting value.
    pub gain: i64,
    /// Standing lost per failed job (expired, or nothing delivered).
    pub loss: i64,
    /// When the giver offers jobs at all; none means always.
    #[serde(default)]
    pub available: Option<Condition>,
    pub voice: Voice,
}

impl Record for Giver {
    type Id = GiverId;
    fn id(&self) -> &GiverId {
        &self.id
    }
}

/// How the giver feels about the crew.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mood {
    FirstJob,
    Regular,
    AfterFailure,
}

/// What happened between the crew and one giver.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GiverHistory {
    pub completed: u32,
    /// Failed jobs since the last completed one.
    pub failure_streak: u32,
}

impl GiverHistory {
    pub fn mood(&self) -> Mood {
        if self.failure_streak > 0 {
            Mood::AfterFailure
        } else if self.completed == 0 {
            Mood::FirstJob
        } else {
            Mood::Regular
        }
    }
}

impl Greetings {
    pub fn key(&self, m: Mood) -> &TextKey {
        match m {
            Mood::FirstJob => &self.first_job,
            Mood::Regular => &self.regular,
            Mood::AfterFailure => &self.after_failure,
        }
    }
}

/// Errors for voice keys without a pool or with too few lines.
pub(crate) fn check_texts(g: &Loaded<Giver>, table: &TextTable, e: &mut Vec<String>) {
    let v = &g.record.voice;
    let keys = [
        ("voice.greeting.first_job", &v.greeting.first_job),
        ("voice.greeting.regular", &v.greeting.regular),
        ("voice.greeting.after_failure", &v.greeting.after_failure),
        ("voice.intro", &v.intro),
        ("voice.reason", &v.reason),
        ("voice.sign_off", &v.sign_off),
    ];
    for (field, k) in keys {
        match table.lines(k.as_str()) {
            None => e.push(format!("{}: {field}: no text '{k}'", g.path)),
            Some(l) if l.len() < MIN_POOL => e.push(format!("{}: {field}: '{k}' needs at least {MIN_POOL} lines (it has {})", g.path, l.len())),
            Some(_) => {}
        }
    }
    if !table.has(g.record.name.as_str()) {
        e.push(format!("{}: name: no text '{}'", g.path, g.record.name));
    }
}
