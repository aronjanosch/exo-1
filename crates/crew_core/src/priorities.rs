//! Bot priorities. Taken from PULSAR: a table with one row per task and five levels (ignore, low,
//! medium, high, max), a page per bot class, overrides by alert level, captain's order or a custom
//! condition, save and load; from Barotrauma: scoring by urgency and distance, orders above
//! autonomous work, the stickiness that keeps a bot on its task, and the skill gap under which a
//! bot leaves a job alone. The numbers are ours.
//!
//! The glue lists open tasks (stations to fill, fires, breaches, broken devices) each tick it
//! wants a bot to think; `choose` scores them for one bot:
//!
//! - a task the bot's order points at: `ORDER_SCORE` + the order's priority bonus;
//! - otherwise the level from the bot's page (overrides first), `Level::base`, skipped at Ignore;
//! - plus `URGENCY_W` × urgency (0..1) and `DIST_W` × a distance factor (1 near, 0.1 at
//!   `DIST_FAR_M` and beyond).
//!
//! An order without a matching open task (follow me, stay, go to) is a choice of its own at
//! `ORDER_SCORE`. A bot keeps its current pick unless a new one beats it by `SWITCH_MARGIN`, and
//! within `HOLD_S` of picking it switches only for an order. A task taken by a player or another
//! bot is not open (`Claims`); a player who sits down at a bot's station takes it at once.
use std::collections::BTreeMap;

use gameplay_core::ClientId;
use gameplay_core::rng::Rng;
use serde::{Deserialize, Serialize};

use crate::alert::AlertLevel;
use crate::orders::{Order, OrderKind, OrderPriority};
use crate::{Refusal, Role, Target, captain_only};

/// Score of a task the bot's captain order points at, above every table level.
pub const ORDER_SCORE: f64 = 100.0;
/// Weight of a task's urgency (0..1).
pub const URGENCY_W: f64 = 10.0;
/// Weight of the distance factor.
pub const DIST_W: f64 = 5.0;
/// Distance at which the distance factor bottoms out at 0.1, metres.
pub const DIST_FAR_M: f64 = 40.0;
/// A new pick must beat the current one by this much. TODO(initiator): starting value.
pub const SWITCH_MARGIN: f64 = 10.0;
/// Seconds a bot keeps a fresh pick against everything but an order. TODO(initiator).
pub const HOLD_S: f64 = 3.0;
/// A bot leaves a task alone whose skill need is more than this above its skill.
pub const SKILL_SLACK: f64 = 20.0;
/// At most this much random score, to break exact ties between bots.
pub const JITTER: f64 = 0.01;
/// Version of the JSON table format.
pub const TABLE_VERSION: u32 = 1;

/// The kinds of work a bot can pick.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    OperateTurret,
    /// The engine or power board.
    OperateEngine,
    Repair,
    ExtinguishFire,
    PatchBreach,
    Heal,
    Scan,
    FetchCargo,
    /// Stay at the bot's own role station (the helm for a pilot bot).
    StayAtStation,
}

impl TaskKind {
    pub const ALL: [TaskKind; 9] = [
        TaskKind::OperateTurret,
        TaskKind::OperateEngine,
        TaskKind::Repair,
        TaskKind::ExtinguishFire,
        TaskKind::PatchBreach,
        TaskKind::Heal,
        TaskKind::Scan,
        TaskKind::FetchCargo,
        TaskKind::StayAtStation,
    ];

    pub fn id(self) -> &'static str {
        match self {
            TaskKind::OperateTurret => "operate_turret",
            TaskKind::OperateEngine => "operate_engine",
            TaskKind::Repair => "repair",
            TaskKind::ExtinguishFire => "extinguish_fire",
            TaskKind::PatchBreach => "patch_breach",
            TaskKind::Heal => "heal",
            TaskKind::Scan => "scan",
            TaskKind::FetchCargo => "fetch_cargo",
            TaskKind::StayAtStation => "stay_at_station",
        }
    }

    /// Whether the task is a seat at a station (an `Operate` order points at it).
    pub fn is_station(self) -> bool {
        matches!(self, TaskKind::OperateTurret | TaskKind::OperateEngine | TaskKind::Scan | TaskKind::StayAtStation)
    }
}

/// The five levels of a row.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    #[default]
    Ignore,
    Low,
    Medium,
    High,
    Max,
}

impl Level {
    pub const ALL: [Level; 5] = [Level::Ignore, Level::Low, Level::Medium, Level::High, Level::Max];

    /// Base score; none for Ignore. Levels are 20 apart, more than urgency and distance add
    /// together (15), so a higher level always wins. TODO(initiator): starting values.
    pub fn base(self) -> Option<f64> {
        match self {
            Level::Ignore => None,
            Level::Low => Some(20.0),
            Level::Medium => Some(40.0),
            Level::High => Some(60.0),
            Level::Max => Some(80.0),
        }
    }
}

/// When an override applies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum When {
    /// The ship is at this alert level.
    Alert(AlertLevel),
    /// The bot's current captain order is of this kind (a free-text order too).
    Order(OrderKind),
    /// The glue raised this flag (docked, landed, in combat).
    Custom(String),
}

/// A row's level while a condition holds. The first active override of a row wins.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Override {
    pub task: TaskKind,
    pub when: When,
    /// Applies while the condition does not hold.
    #[serde(default)]
    pub inverted: bool,
    pub level: Level,
}

/// The bot's view of the ship this tick.
#[derive(Copy, Clone, Debug)]
pub struct Situation<'a> {
    pub alert: AlertLevel,
    /// The order that applies to the bot's role (`Orders::for_role`).
    pub order: Option<&'a Order>,
    /// Flags for `When::Custom`.
    pub flags: &'a [String],
    pub now_s: f64,
}

impl When {
    fn holds(&self, sit: &Situation) -> bool {
        match self {
            When::Alert(a) => sit.alert == *a,
            When::Order(k) => sit.order.is_some_and(|o| o.kind == *k),
            When::Custom(f) => sit.flags.iter().any(|x| x == f),
        }
    }
}

/// One bot class's rows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    /// A missing row is Ignore.
    pub levels: BTreeMap<TaskKind, Level>,
    #[serde(default)]
    pub overrides: Vec<Override>,
}

impl Page {
    fn new(rows: [(TaskKind, Level); 9]) -> Page {
        Page { levels: rows.into_iter().collect(), overrides: Vec::new() }
    }

    /// The level the captain set for a row.
    pub fn base(&self, task: TaskKind) -> Level {
        self.levels.get(&task).copied().unwrap_or_default()
    }

    /// The level in this situation: the first active override of the row, else the base.
    pub fn level(&self, task: TaskKind, sit: &Situation) -> Level {
        self.overrides.iter().find(|o| o.task == task && o.when.holds(sit) != o.inverted).map_or(self.base(task), |o| o.level)
    }
}

/// The captain's bot setup: a page per bot class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriorityTable {
    pub version: u32,
    pub pages: BTreeMap<Role, Page>,
}

impl Default for PriorityTable {
    /// Bots fill the pilot, engineer and tactical slots; turrets come first, the scientist needs
    /// no bot, so nobody scans. TODO(initiator): starting levels.
    fn default() -> PriorityTable {
        use Level::*;
        use TaskKind::*;
        let pilot = Page::new([(StayAtStation, Max), (OperateTurret, High), (ExtinguishFire, Medium), (PatchBreach, Medium), (Repair, Low), (Heal, Low), (FetchCargo, Low), (OperateEngine, Ignore), (Scan, Ignore)]);
        let engineer = Page::new([(OperateEngine, Max), (Repair, High), (ExtinguishFire, High), (PatchBreach, High), (OperateTurret, Medium), (StayAtStation, Medium), (Heal, Low), (FetchCargo, Low), (Scan, Ignore)]);
        let tactical = Page::new([(OperateTurret, Max), (ExtinguishFire, High), (PatchBreach, Medium), (Heal, Medium), (StayAtStation, Medium), (Repair, Low), (FetchCargo, Low), (OperateEngine, Ignore), (Scan, Ignore)]);
        PriorityTable { version: TABLE_VERSION, pages: BTreeMap::from([(Role::Pilot, pilot), (Role::Engineer, engineer), (Role::Tactical, tactical)]) }
    }
}

impl PriorityTable {
    /// A class's page; none for a class without bots (captain, scientist).
    pub fn page(&self, class: Role) -> Option<&Page> {
        self.pages.get(&class)
    }

    pub fn page_mut(&mut self, class: Role) -> Option<&mut Page> {
        self.pages.get_mut(&class)
    }

    /// The captain sets a row; a class without a page gets one. The captain has no bot page.
    pub fn set_level(&mut self, by: Role, class: Role, task: TaskKind, level: Level) -> Result<(), Refusal> {
        captain_only(by)?;
        if class == Role::Captain {
            return Err(Refusal::NoSuchSlot);
        }
        self.pages.entry(class).or_default().levels.insert(task, level);
        Ok(())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a table always serialises")
    }

    /// Reads a saved table; unknown fields and other versions are refused.
    pub fn from_json(s: &str) -> Result<PriorityTable, String> {
        let t: PriorityTable = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if t.version != TABLE_VERSION {
            return Err(format!("priority table version {} (this build reads {TABLE_VERSION})", t.version));
        }
        Ok(t)
    }
}

/// An open piece of work, as the glue sees it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(pub u64);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BotId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    /// Stable while the work exists (the glue's numbering).
    pub id: TaskId,
    pub kind: TaskKind,
    /// The station, device or room of the work.
    pub target: Target,
    /// Where the work is, metres in ship space.
    pub pos: [f64; 3],
    /// 0..1: how bad it is (fire size, breach size, damage, an enemy in range for a turret).
    pub urgency: f64,
    /// Skill the work asks for (0..100); 0 for anyone.
    pub need_skill: f64,
}

/// Who holds a task.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Taker {
    Player(ClientId),
    Bot(BotId),
}

/// Which bot or player holds which task. A bot holds at most one.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Claims {
    by_task: BTreeMap<TaskId, Taker>,
}

impl Claims {
    pub fn taker(&self, task: TaskId) -> Option<Taker> {
        self.by_task.get(&task).copied()
    }

    /// A bot takes a free task (or keeps its own). False if somebody else holds it.
    pub fn claim(&mut self, task: TaskId, bot: BotId) -> bool {
        match self.taker(task) {
            Some(t) if t != Taker::Bot(bot) => false,
            _ => {
                self.release_bot(bot);
                self.by_task.insert(task, Taker::Bot(bot));
                true
            }
        }
    }

    /// Records a bot's choice: it holds the chosen task and nothing else.
    pub fn commit(&mut self, bot: BotId, choice: &Choice) {
        match choice.pick {
            Pick::Task(t) => {
                self.claim(t, bot);
            }
            Pick::Order { .. } => self.release_bot(bot),
        }
    }

    pub fn release_bot(&mut self, bot: BotId) {
        self.by_task.retain(|_, t| *t != Taker::Bot(bot));
    }

    /// A player sits down at the task's station: they hold it, and the bot that held it (if any)
    /// is named so the glue can send it off.
    pub fn player_takes(&mut self, task: TaskId, who: ClientId) -> Option<BotId> {
        match self.by_task.insert(task, Taker::Player(who)) {
            Some(Taker::Bot(b)) => Some(b),
            _ => None,
        }
    }

    /// The player stands up; the station is free.
    pub fn player_leaves(&mut self, task: TaskId, who: ClientId) {
        if self.taker(task) == Some(Taker::Player(who)) {
            self.by_task.remove(&task);
        }
    }

    /// Drops the claims of tasks that are gone.
    pub fn retain_open(&mut self, tasks: &[Task]) {
        self.by_task.retain(|id, _| tasks.iter().any(|t| t.id == *id));
    }
}

/// What a bot does.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Task(TaskId),
    /// An order that has no open task: follow, stay, report, or go to the order's target.
    Order { kind: OrderKind, target: Option<Target> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub pick: Pick,
    pub score: f64,
    /// The table level it was picked at; none when an order picked it.
    pub level: Option<Level>,
    /// When the bot first picked it.
    pub since_s: f64,
}

/// One bot, as `choose` needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct BotView {
    pub id: BotId,
    /// The role slot the bot fills; its page in the table.
    pub class: Role,
    pub pos: [f64; 3],
    /// 0..100, lower than a player's (`skills-and-talents.md`, "Bot skill").
    pub skill: f64,
    /// Its last choice, for the hysteresis.
    pub current: Option<Choice>,
}

fn order_bonus(p: OrderPriority) -> f64 {
    match p {
        OrderPriority::Low => 0.0,
        OrderPriority::Normal => 2.0,
        OrderPriority::Urgent => 4.0,
    }
}

/// Whether the order points at this task.
fn order_points_at(o: &Order, t: &Task) -> bool {
    let kind_fits = match o.kind {
        OrderKind::Operate => t.kind.is_station(),
        OrderKind::Repair => t.kind == TaskKind::Repair,
        OrderKind::Extinguish => t.kind == TaskKind::ExtinguishFire,
        OrderKind::FixBreach => t.kind == TaskKind::PatchBreach,
        _ => false,
    };
    kind_fits && o.target.as_ref() == Some(&t.target)
}

fn dist_factor(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    1.0 - 0.9 * (d / DIST_FAR_M).clamp(0.0, 1.0)
}

/// The score of a task for a bot, and its level; none if the bot leaves it alone.
fn score(bot: &BotView, t: &Task, claims: &Claims, page: Option<&Page>, sit: &Situation) -> Option<(f64, Option<Level>)> {
    if claims.taker(t.id).is_some_and(|k| k != Taker::Bot(bot.id)) || t.need_skill > bot.skill + SKILL_SLACK {
        return None;
    }
    let extra = URGENCY_W * t.urgency.clamp(0.0, 1.0) + DIST_W * dist_factor(bot.pos, t.pos);
    if let Some(o) = sit.order.filter(|o| order_points_at(o, t)) {
        return Some((ORDER_SCORE + order_bonus(o.priority) + extra, None));
    }
    let level = page?.level(t.kind, sit);
    Some((level.base()? + extra, Some(level)))
}

/// Picks what one bot does now; none if nothing is worth doing (the glue lets it idle).
/// `rng` adds at most `JITTER` to each score, so equal bots spread over equal tasks.
pub fn choose(bot: &BotView, tasks: &[Task], claims: &Claims, table: &PriorityTable, sit: &Situation, rng: &mut Rng) -> Option<Choice> {
    let page = table.page(bot.class);
    let mut best: Option<Choice> = None;
    let mut consider = |pick: Pick, score: f64, level: Option<Level>, rng: &mut Rng| {
        let score = score + JITTER * rng.unit();
        if best.as_ref().is_none_or(|b| score > b.score) {
            best = Some(Choice { pick, score, level, since_s: sit.now_s });
        }
    };
    let mut order_has_task = false;
    for t in tasks {
        if let Some((s, level)) = score(bot, t, claims, page, sit) {
            order_has_task |= level.is_none();
            consider(Pick::Task(t.id), s, level, rng);
        }
    }
    if let Some(o) = sit.order
        && !order_has_task
        && !matches!(o.kind, OrderKind::Custom(_))
    {
        consider(Pick::Order { kind: o.kind, target: o.target.clone() }, ORDER_SCORE + order_bonus(o.priority), None, rng);
    }
    let best = best?;

    // Hysteresis: keep the current pick while it is still open, unless the new one beats it by
    // the margin (and, inside the hold window, is an order the current one is not).
    let Some(cur) = &bot.current else { return Some(best) };
    if best.pick == cur.pick {
        return Some(Choice { since_s: cur.since_s, ..best });
    }
    let fresh = match &cur.pick {
        Pick::Task(id) => tasks.iter().find(|t| t.id == *id).and_then(|t| score(bot, t, claims, page, sit)),
        Pick::Order { kind, target } => sit.order.filter(|o| o.kind == *kind && o.target == *target).map(|o| (ORDER_SCORE + order_bonus(o.priority), None)),
    };
    let Some((cur_score, cur_level)) = fresh else { return Some(best) };
    let held = sit.now_s - cur.since_s < HOLD_S;
    let order_beats = best.level.is_none() && cur_level.is_some();
    if best.score >= cur_score + SWITCH_MARGIN && (!held || order_beats) {
        Some(best)
    } else {
        Some(Choice { pick: cur.pick.clone(), score: cur_score, level: cur_level, since_s: cur.since_s })
    }
}
