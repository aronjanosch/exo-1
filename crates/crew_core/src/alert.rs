//! Alert levels: green (no threat), yellow (a threat may come), red (danger now). Taken from
//! PULSAR: three levels set by the captain, a time of the last change and an automatic yellow;
//! the rule for the automatic yellow is ours.
//!
//! The captain sets the level. On top, a threat flag from the glue (an enemy in sensor range, a
//! hail turned hostile) raises green to yellow on its rising edge, unless the captain changed the
//! level in the last `CAPTAIN_HOLD_S` seconds. The automatic rule never lowers and never goes to
//! red. Changes come out as `AlertEvent`s for lights, sounds and the bots' overrides.
use std::collections::VecDeque;

use gameplay_core::notice::{Notice, NoticeKind};
use serde::{Deserialize, Serialize};

use crate::{Refusal, Role, captain_only};

/// Seconds after a captain's call in which the automatic yellow stays quiet.
/// TODO(initiator): starting value.
pub const CAPTAIN_HOLD_S: f64 = 30.0;

/// Changes kept in the log.
pub const LOG_LEN: usize = 8;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertLevel {
    #[default]
    Green,
    Yellow,
    Red,
}

impl AlertLevel {
    pub const ALL: [AlertLevel; 3] = [AlertLevel::Green, AlertLevel::Yellow, AlertLevel::Red];

    pub fn id(self) -> &'static str {
        match self {
            AlertLevel::Green => "green",
            AlertLevel::Yellow => "yellow",
            AlertLevel::Red => "red",
        }
    }

    /// The notice everybody gets when the ship goes to this level (`notice.alert.<id>`).
    pub fn notice(self) -> Notice {
        let kind = if self == AlertLevel::Green { NoticeKind::Updated } else { NoticeKind::Warning };
        Notice::new(kind, format!("notice.alert.{}", self.id()))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertCause {
    Captain,
    /// The automatic yellow on a new threat.
    Auto,
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AlertChange {
    pub from: AlertLevel,
    pub to: AlertLevel,
    pub cause: AlertCause,
    pub at_s: f64,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AlertEvent {
    Changed { from: AlertLevel, to: AlertLevel, cause: AlertCause, at_s: f64 },
}

/// The ship's alert state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    level: AlertLevel,
    log: VecDeque<AlertChange>,
    /// When the captain last changed the level.
    captain_s: Option<f64>,
    /// The threat flag at the last `update`, for the rising edge.
    threat: bool,
}

impl Alert {
    pub fn level(&self) -> AlertLevel {
        self.level
    }

    /// Time of the last change, by anyone; none before the first.
    pub fn last_change_s(&self) -> Option<f64> {
        self.log.back().map(|c| c.at_s)
    }

    /// The last `LOG_LEN` changes, oldest first.
    pub fn log(&self) -> Vec<AlertChange> {
        self.log.iter().copied().collect()
    }

    /// The captain sets the level. None if it already is that level; the call still counts as
    /// the captain's decision for `CAPTAIN_HOLD_S`.
    pub fn set(&mut self, by: Role, to: AlertLevel, now_s: f64) -> Result<Option<AlertEvent>, Refusal> {
        captain_only(by)?;
        self.captain_s = Some(now_s);
        if to == self.level {
            return Ok(None);
        }
        Ok(Some(self.change(to, AlertCause::Captain, now_s)))
    }

    /// Feeds the threat flag each tick; raises green to yellow on a new threat.
    pub fn update(&mut self, threat: bool, now_s: f64) -> Option<AlertEvent> {
        let rising = threat && !self.threat;
        self.threat = threat;
        let held = self.captain_s.is_some_and(|t| now_s - t < CAPTAIN_HOLD_S);
        if rising && !held && self.level == AlertLevel::Green {
            return Some(self.change(AlertLevel::Yellow, AlertCause::Auto, now_s));
        }
        None
    }

    fn change(&mut self, to: AlertLevel, cause: AlertCause, at_s: f64) -> AlertEvent {
        let from = self.level;
        self.level = to;
        if self.log.len() == LOG_LEN {
            self.log.pop_front();
        }
        self.log.push_back(AlertChange { from, to, cause, at_s });
        AlertEvent::Changed { from, to, cause, at_s }
    }
}
