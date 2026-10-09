//! Notices (#165): every system reports its moments as `Notice`s instead of log lines. A
//! notice names a kind, a text key (a pool, see `text`), arguments and a weight; the glue shows
//! it as a banner or a toast with the sound of its kind.
//!
//! `NoticeQueue` paces them: one notice at a time with a short gap, never two banners at once,
//! and big ones (rank, unlock) held until nothing happened to the objectives for a few seconds.
//! Nothing here blocks input; `skip` runs the rest of a ritual fast.
use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::id::TextKey;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    /// Something new can be done (an offer, a counter).
    Available,
    Accepted,
    /// Progress on an objective (picked up, delivered).
    Updated,
    Completed,
    Failed,
    /// One line of a payout or XP.
    Reward,
    /// A track reached a new level.
    Rank,
    Unlock,
    Warning,
}

impl NoticeKind {
    pub const ALL: [NoticeKind; 9] = [
        NoticeKind::Available,
        NoticeKind::Accepted,
        NoticeKind::Updated,
        NoticeKind::Completed,
        NoticeKind::Failed,
        NoticeKind::Reward,
        NoticeKind::Rank,
        NoticeKind::Unlock,
        NoticeKind::Warning,
    ];

    /// The id of the synthesized sound of this kind (`exo_app::audio` makes them).
    pub fn sound(self) -> &'static str {
        match self {
            NoticeKind::Available | NoticeKind::Updated => "ping",
            NoticeKind::Accepted => "accept",
            NoticeKind::Reward => "coin",
            NoticeKind::Completed | NoticeKind::Rank | NoticeKind::Unlock => "fanfare",
            NoticeKind::Failed | NoticeKind::Warning => "buzz",
        }
    }

    pub fn default_weight(self) -> Weight {
        match self {
            NoticeKind::Accepted | NoticeKind::Completed | NoticeKind::Failed => Weight::Banner,
            NoticeKind::Rank | NoticeKind::Unlock => Weight::Calm,
            NoticeKind::Available | NoticeKind::Updated | NoticeKind::Reward | NoticeKind::Warning => Weight::Toast,
        }
    }

    /// Whether it tells that an objective changed (the queue waits for calm after these).
    fn objective_change(self) -> bool {
        matches!(self, NoticeKind::Accepted | NoticeKind::Updated | NoticeKind::Completed | NoticeKind::Failed)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weight {
    /// A small line that comes and goes.
    Toast,
    /// A big line; one at a time.
    Banner,
    /// A banner held until the objectives have been quiet for a while.
    Calm,
}

/// A value for a `{name}` in the text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Arg {
    Text(String),
    Number(i64),
    /// A text key, shown as one of its lines (a job title, a place name).
    Key(TextKey),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub kind: NoticeKind,
    pub key: TextKey,
    pub args: Vec<(String, Arg)>,
    pub weight: Weight,
}

impl Notice {
    pub fn new(kind: NoticeKind, key: impl Into<String>) -> Notice {
        Notice { kind, key: TextKey::new(key), args: Vec::new(), weight: kind.default_weight() }
    }

    pub fn args(mut self, args: Vec<(String, Arg)>) -> Notice {
        self.args = args;
        self
    }

    pub fn arg(mut self, name: &str, a: Arg) -> Notice {
        self.args.push((name.to_string(), a));
        self
    }

    pub fn weight(mut self, w: Weight) -> Notice {
        self.weight = w;
        self
    }
}

/// Times of the queue, seconds. TODO(initiator): starting values, the playtest decides.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Pacing {
    /// Between two notices.
    pub gap_s: f64,
    /// A banner stays this long (and holds the banner slot).
    pub banner_s: f64,
    pub toast_s: f64,
    /// Objectives quiet for this long before a `Calm` notice comes.
    pub calm_s: f64,
}

impl Default for Pacing {
    fn default() -> Pacing {
        Pacing { gap_s: 0.6, banner_s: 3.0, toast_s: 2.5, calm_s: 4.0 }
    }
}

/// A notice released for showing.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub notice: Notice,
    pub banner: bool,
    /// How long it stays on screen.
    pub seconds: f64,
}

/// How much faster a skipped ritual runs.
const SKIP_SPEED: f64 = 8.0;

#[derive(Clone, Debug)]
pub struct NoticeQueue {
    pacing: Pacing,
    pending: VecDeque<Notice>,
    clock: f64,
    gap_until: f64,
    banner_until: f64,
    last_change: f64,
    speed: f64,
}

impl Default for NoticeQueue {
    fn default() -> NoticeQueue {
        NoticeQueue::new(Pacing::default())
    }
}

impl NoticeQueue {
    pub fn new(pacing: Pacing) -> NoticeQueue {
        NoticeQueue { pacing, pending: VecDeque::new(), clock: 0.0, gap_until: 0.0, banner_until: 0.0, last_change: 0.0, speed: 1.0 }
    }

    pub fn pacing(&self) -> Pacing {
        self.pacing
    }

    pub fn push(&mut self, n: Notice) {
        if n.kind.objective_change() {
            self.objective_changed();
        }
        self.pending.push_back(n);
    }

    /// Something happened to the objectives: big notices wait for calm from here.
    pub fn objective_changed(&mut self) {
        self.last_change = self.clock;
    }

    /// The player skips the ritual: what is queued comes out in order, fast.
    pub fn skip(&mut self) {
        if !self.pending.is_empty() {
            self.speed = SKIP_SPEED;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Advances time by `dt` seconds and releases at most one notice.
    pub fn tick(&mut self, dt: f64) -> Vec<Shown> {
        self.clock += dt;
        if self.clock < self.gap_until {
            return Vec::new();
        }
        let banner_free = self.clock >= self.banner_until;
        let calm = self.clock - self.last_change >= self.pacing.calm_s / self.speed;
        // In order. A held calm notice lets the ones behind it pass; a banner waiting for its slot
        // holds back everything behind it, so order is kept.
        let mut pick = None;
        for (i, n) in self.pending.iter().enumerate() {
            match n.weight {
                Weight::Toast => {
                    pick = Some(i);
                    break;
                }
                Weight::Banner if banner_free => {
                    pick = Some(i);
                    break;
                }
                Weight::Banner => break,
                Weight::Calm if banner_free && calm => {
                    pick = Some(i);
                    break;
                }
                Weight::Calm => {}
            }
        }
        let Some(i) = pick else {
            if self.pending.is_empty() {
                self.speed = 1.0;
            }
            return Vec::new();
        };
        let notice = self.pending.remove(i).expect("index from the scan");
        let banner = notice.weight != Weight::Toast;
        let seconds = if banner { self.pacing.banner_s } else { self.pacing.toast_s } / self.speed;
        self.gap_until = self.clock + self.pacing.gap_s / self.speed;
        if banner {
            self.banner_until = self.clock + seconds;
        }
        if self.pending.is_empty() {
            self.speed = 1.0;
        }
        vec![Shown { notice, banner, seconds }]
    }
}
