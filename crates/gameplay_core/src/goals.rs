//! The family ladders (#230): the crew rises on a ladder by meeting the goals of its next rank and
//! paying the tribute. Records in `ladder/`, loaded by `Ladders::load`; the rank per ladder in
//! `Empire`, saved as its own section.
//!
//! The rules run on the events: `TributePaid` is the request. A rise raises the wallet charge, the
//! rank's unlocks (`UnlockBought`, free), `RankReached` and a flag; a refusal raises a notice and
//! nothing else. `Empire` changes its rank only on `RankReached`, which the host routes back to
//! every system, so the rank is set once, whoever raised it.
use std::collections::BTreeMap;

use content_core::{File, Loaded, Record, check_id, load_records};
use serde::{Deserialize, Serialize};

use crate::condition::Condition;
use crate::content::{Content, WALLET};
use crate::event::{Event, WorldEvent};
use crate::id::{Flag, TextKey, TrackId, UnlockId};
use crate::notice::{Arg, Notice, NoticeKind};
use crate::progress::Progress;
use crate::save::{Envelope, SaveError};
use crate::text::TextTable;

pub const FOLDER: &str = "ladder";
pub const SECTION: &str = "empire";
pub const SECTION_VERSION: u32 = 1;
/// The line the HUD shows when every goal of the next rank holds.
pub const PAY_TRIBUTE: &str = "empire.pay_tribute";
/// The notices of the rules, by key (the glue shows them; `Ladders::check_texts` checks the text).
pub const NOTICE_KEYS: [&str; 5] = ["notice.empire.goal_missing", "notice.empire.tribute_short", "notice.empire.cannot_afford", "notice.empire.rank_up", "notice.empire.top"];

/// One goal of a rank: a line for the HUD and the condition that makes it hold.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    pub text: TextKey,
    pub condition: Condition,
}

/// One rank: what it asks for, what it costs, what it gives.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rank {
    pub name: TextKey,
    /// Money paid on the rise. TODO(initiator): starting values.
    pub tribute: i64,
    #[serde(default)]
    pub requires: Vec<Goal>,
    /// Bought for free on the rise (their price must be 0; the tribute pays).
    #[serde(default)]
    pub unlocks: Vec<UnlockId>,
}

/// A ladder: the family, its name, its ranks in order.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ladder {
    pub id: String,
    pub name: TextKey,
    pub ranks: Vec<Rank>,
}

impl Record for Ladder {
    type Id = String;
    fn id(&self) -> &String {
        &self.id
    }
}

/// The loaded ladders, by id.
#[derive(Clone, Debug, PartialEq)]
pub struct Ladders {
    pub ladders: BTreeMap<String, Loaded<Ladder>>,
}

impl Ladders {
    /// Loads `ladder/` from `files` and checks ids, ranks, unlocks (free, no condition) and goal
    /// conditions against the kernel. Returns every error, each naming the file and the field.
    pub fn load(files: &[File], k: &Content) -> Result<Ladders, Vec<String>> {
        let mut e = Vec::new();
        let ladders = load_records::<Ladder>(files, FOLDER, &mut e);
        for Loaded { path, record: r } in ladders.values() {
            check_id(path, "id", &r.id, &mut e);
            if r.ranks.is_empty() {
                e.push(format!("{path}: ranks: empty"));
            }
            for (i, rank) in r.ranks.iter().enumerate() {
                if rank.tribute < 0 {
                    e.push(format!("{path}: ranks[{i}].tribute: must not be negative"));
                }
                for u in &rank.unlocks {
                    match k.unlocks.get(u) {
                        None => e.push(format!("{path}: unlocks: unknown unlock '{u}'")),
                        Some(x) if x.record.price != 0 => e.push(format!("{path}: unlocks: '{u}' costs {}, a rank unlock is free (the tribute pays)", x.record.price)),
                        Some(x) if x.record.condition.is_some() => e.push(format!("{path}: unlocks: '{u}' has a condition, a rank unlock has none")),
                        Some(_) => {}
                    }
                }
                for g in &rank.requires {
                    k.check_condition(path, "requires", &g.condition, &mut e);
                }
            }
        }
        if e.is_empty() { Ok(Ladders { ladders }) } else { Err(e) }
    }

    /// The text keys the ladders show: the ladder and rank names, the goals, the HUD's tribute line.
    pub fn check_texts(&self, table: &TextTable) -> Vec<String> {
        let mut e = Vec::new();
        let mut need = |path: &str, field: &str, key: &str| {
            if !table.has(key) {
                e.push(format!("{path}: {field}: no text '{key}'"));
            }
        };
        for Loaded { path, record: r } in self.ladders.values() {
            need(path, "name", r.name.as_str());
            for rank in &r.ranks {
                need(path, "ranks.name", rank.name.as_str());
                for g in &rank.requires {
                    need(path, "requires.text", g.text.as_str());
                }
            }
        }
        if !table.has(PAY_TRIBUTE) {
            e.push(format!("ladder: {PAY_TRIBUTE}: no text"));
        }
        e
    }
}

/// What the HUD shows of the next rank.
#[derive(Clone, Debug, PartialEq)]
pub struct GoalView {
    pub rank_name: TextKey,
    /// The first goal not yet met, or `PAY_TRIBUTE` when all hold.
    pub goal_text: TextKey,
    pub goals_done: usize,
    pub goals_total: usize,
    pub tribute: i64,
    pub ready_to_pay: bool,
}

/// What the rules raise: a domain event for every system, or a notice for the player.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Emit(WorldEvent),
    Notice(Notice),
}

/// The crew's rank on each ladder: how many ranks it has reached (0 at the start), so the next
/// rank is the one at that index.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Empire {
    reached: BTreeMap<String, u32>,
}

impl Empire {
    pub fn new() -> Empire {
        Empire::default()
    }

    /// Ranks reached on a ladder (0 before the first rise).
    pub fn rank(&self, ladder: &str) -> u32 {
        self.reached.get(ladder).copied().unwrap_or(0)
    }

    /// The HUD's view of the next rank of a ladder; None for an unknown ladder or the top rank.
    pub fn next_goal(&self, ladders: &Ladders, p: &Progress, k: &Content, ladder: &str) -> Option<GoalView> {
        let l = &ladders.ladders.get(ladder)?.record;
        let rank = l.ranks.get(self.rank(ladder) as usize)?;
        let holds: Vec<bool> = rank.requires.iter().map(|g| g.condition.holds(k, p, None)).collect();
        let goals_done = holds.iter().filter(|h| **h).count();
        let ready_to_pay = goals_done == rank.requires.len();
        let goal_text = match rank.requires.iter().zip(&holds).find(|(_, h)| !**h) {
            Some((g, _)) => g.text.clone(),
            None => TextKey::new(PAY_TRIBUTE),
        };
        Some(GoalView { rank_name: rank.name.clone(), goal_text, goals_done, goals_total: rank.requires.len(), tribute: rank.tribute, ready_to_pay })
    }

    /// Applies a world event: `TributePaid` asks for the next rank, `RankReached` sets the rank.
    /// Other events change nothing.
    pub fn apply_world(&mut self, ladders: &Ladders, k: &Content, p: &Progress, ev: &Event<WorldEvent>) -> Vec<Outcome> {
        match &ev.payload {
            WorldEvent::TributePaid { ladder, amount } => self.pay(ladders, k, p, ladder, *amount),
            WorldEvent::RankReached { ladder, rank } if ladders.ladders.contains_key(ladder) => {
                let r = self.reached.entry(ladder.clone()).or_insert(0);
                *r = (*r).max(rank + 1);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn pay(&self, ladders: &Ladders, k: &Content, p: &Progress, ladder: &str, amount: i64) -> Vec<Outcome> {
        let Some(l) = ladders.ladders.get(ladder) else { return Vec::new() };
        let n = self.rank(ladder);
        let Some(rank) = l.record.ranks.get(n as usize) else {
            return vec![Outcome::Notice(refusal("notice.empire.top").arg("ladder", Arg::Key(l.record.name.clone())))];
        };
        if let Some(g) = rank.requires.iter().find(|g| !g.condition.holds(k, p, None)) {
            return vec![Outcome::Notice(refusal("notice.empire.goal_missing").arg("goal", Arg::Key(g.text.clone())))];
        }
        if amount < rank.tribute {
            return vec![Outcome::Notice(refusal("notice.empire.tribute_short").arg("amount", Arg::Number(rank.tribute)))];
        }
        if p.wallet() < rank.tribute {
            return vec![Outcome::Notice(refusal("notice.empire.cannot_afford").arg("amount", Arg::Number(rank.tribute)))];
        }
        let mut out = Vec::new();
        if rank.tribute > 0 {
            out.push(Outcome::Emit(WorldEvent::TrackChanged { track: TrackId::new(WALLET), delta: -rank.tribute, player: None }));
        }
        for u in &rank.unlocks {
            out.push(Outcome::Emit(WorldEvent::UnlockBought { unlock: u.clone() }));
        }
        out.push(Outcome::Emit(WorldEvent::RankReached { ladder: ladder.to_string(), rank: n }));
        out.push(Outcome::Emit(WorldEvent::FlagRaised { flag: Flag::new(format!("rank:{ladder}:{n}")) }));
        out.push(Outcome::Notice(Notice::new(NoticeKind::Rank, "notice.empire.rank_up").arg("ladder", Arg::Key(l.record.name.clone())).arg("rank", Arg::Key(rank.name.clone()))));
        out
    }

    pub fn save(&self, env: &mut Envelope) {
        env.put(SECTION, SECTION_VERSION, self);
    }

    /// None if the save has no empire section; an error for a version this build does not read.
    pub fn load(env: &Envelope) -> Result<Option<Empire>, SaveError> {
        env.get(SECTION, SECTION_VERSION)
    }
}

/// A refusal the player sees: a warning, its arguments added by the caller.
fn refusal(key: &str) -> Notice {
    Notice::new(NoticeKind::Warning, key)
}
