//! Conditions from the data, shared by unlocks, locations, job templates and encounters (#125).
//! Small on purpose: systems express their own facts as flags.
use serde::{Deserialize, Serialize};

use crate::content::{Content, Owner};
use crate::id::{ClientId, Flag, Tag, TrackId};
use crate::progress::Progress;

/// In JSON: `{ "track_at_least": { "track": "freight_xp", "value": 100 } }`, `{ "has_tag": "x" }`,
/// `{ "flag_set": "job_completed:first_haul" }`, `{ "all": [ … ] }`, `{ "any": [ … ] }`,
/// `{ "not": { … } }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    /// A track's value is at least `value`. A personal track is read for the player asking; with
    /// no player it does not hold.
    TrackAtLeast { track: TrackId, value: i64 },
    /// The crew owns the tag (from an unlock).
    HasTag(Tag),
    /// A system raised the flag.
    FlagSet(Flag),
    /// Every condition holds (an empty list holds).
    All(Vec<Condition>),
    /// At least one holds (an empty list does not).
    Any(Vec<Condition>),
    Not(Box<Condition>),
}

impl Condition {
    /// Whether it holds for the crew, read as `player` for personal tracks.
    pub fn holds(&self, content: &Content, p: &Progress, player: Option<ClientId>) -> bool {
        match self {
            Condition::TrackAtLeast { track, value } => match content.tracks.get(track).map(|t| t.record.owner) {
                Some(Owner::Player) if player.is_none() => false,
                Some(_) => p.value(content, track, player).is_some_and(|v| v >= *value),
                None => false,
            },
            Condition::HasTag(t) => p.has_tag(t),
            Condition::FlagSet(f) => p.has_flag(f),
            Condition::All(cs) => cs.iter().all(|c| c.holds(content, p, player)),
            Condition::Any(cs) => cs.iter().any(|c| c.holds(content, p, player)),
            Condition::Not(c) => !c.holds(content, p, player),
        }
    }
}
