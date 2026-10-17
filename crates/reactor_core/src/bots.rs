//! The bots (r42, #253; r44 bodies): crew that man an empty station at a lower skill than a player.
//! r42 made them abstract; r44 gives them a body. A bot is a walker figure that walks to its job, and
//! works it only once it is there. Each station's bot works the station's controls through the same
//! asks a player's screen sends (`Action`, which the glue maps to `ReactorAsk`), and a station with a
//! player on it is no job for a bot.
//!
//! The jobs (r44): one list of rows on CP-01, `Row`: the four stations (our r42 list, the captain sets
//! the levels) and the three work rows of crew_core's bot tasks (FIRE, REPAIR, BREACH), all five levels
//! each. A bot takes the offered job of the highest level above Ignore; of equal levels, the nearest.
//! A station's job is its screen (`Layout::stations`), a work job's place is the device or the fire
//! (the glue offers those as `Job`s through `Bots::tick`'s `extra`). A job a player is on is not
//! offered: a station while a player is at it or within the grace after they leave, a device or a
//! fire while a player works on it. A bot walks at `BotTuning::walk_speed` (straight line, no
//! pathfinding, no collision: TODO(initiator)) and stops `reach_m` from the place. A bot whose job
//! goes (a player takes it, or it is no longer offered) drops it at once; a job of a higher row takes
//! the bot over. The bot acts only once it has arrived (`Walker::arrived`).
//!
//! Only the station rows act here. The work rows (FIRE, REPAIR, BREACH) have no action yet: the bot
//! walks to them and stops there (TODO(initiator): the work, see the r44 report).
//!
//! The behaviours follow Barotrauma: AIObjectiveOperateItem.cs:54-177 gives an operate objective the
//! priority 0 when another crew member operates the item (line 151, `IsItemOperatedByAnother`) and when
//! a player last used the reactor (lines 108-113, `reactor.LastUserWasPlayer`): the bot leaves a station
//! a player has. The turret's auto-operation is Turret.cs `UpdateAutoOperate` (1006-1055), run only while
//! nobody is at the turret (`AutoOperate && ActiveUser == null`, Turret.cs:607-609), so it fires when a
//! shot is ready, not on a schedule of its own.
//!
//! The priority list (PULSAR, docs/research/pulsar/pulsar-mech-bot-priorities.md): five levels per row,
//! Ignore, Low, Medium, High, Max (section 1, from the manual). The station rows we port are the names
//! the dump gives (section 3.1): MAN_TURRET ("man turret") for GUNNERY, ENG_REACTOR_SAFETY ("engineering
//! reactor safety") for REACTOR, the four ENG_*_FACTOR rows ("power distribution per conduit") for
//! POWER, and the HELM is ours. ENG_JUMP_SHIP ("jump ship") is not ported: the jump is the captain's
//! call (#253). Ignore is zero weight: a row at Ignore is never taken. Section 6 of the note asks whether
//! Ignore disables or only weights; we disable (TODO(initiator)). The work rows are crew_core's bot task
//! kinds (ExtinguishFire, Repair, PatchBreach, crew_core/src/priorities.rs), with their levels from its
//! engineer page (High each).
//!
//! Gunnery skill: the bot's gun skill is `BotTuning::skill` (TODO(initiator): 20; #253 asks a fixed skill
//! or the crew average, we take fixed). It goes to the gun's jam chance (`Weapons::set_skill`, the r41
//! shape). A bot clears a jam at `clear_speed` (half) of a player's hold speed.
//!
//! Units: time s, power as the grid's, temperature in degrees, throttle -100..100 %, distance m in the
//! dev area's ground (x east, y north).
use crate::engine_room::{EngineRoom, PumpMode};
use crate::enemy::Outcome;
use crate::grid::Group;
use serde::{Deserialize, Serialize};

/// The stations a bot can man, in the fixed order ties follow (#253: GN-01, the reactor, the junction
/// box's POWER, the HELM).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Station {
    Gunnery,
    Reactor,
    Power,
    Helm,
}

pub const STATIONS: usize = 4;

impl Station {
    pub const ALL: [Station; STATIONS] = [Station::Gunnery, Station::Reactor, Station::Power, Station::Helm];

    pub fn index(self) -> usize {
        self as usize
    }
}

/// One row of CP-01's list (r44): the four stations, then the three work rows from crew_core's bot
/// tasks. The order is the wire's and the screen's.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Row {
    Gunnery,
    Reactor,
    Power,
    Helm,
    /// A fire to put out (crew_core `ExtinguishFire`).
    Fire,
    /// A device to repair (crew_core `Repair`): the reactor, the pump, the junction box, the oxygen
    /// generator, or the reactor console.
    Repair,
    /// A hull breach to patch (crew_core `PatchBreach`).
    Breach,
}

pub const ROWS: usize = 7;

impl Row {
    pub const ALL: [Row; ROWS] = [Row::Gunnery, Row::Reactor, Row::Power, Row::Helm, Row::Fire, Row::Repair, Row::Breach];

    pub fn index(self) -> usize {
        self as usize
    }

    /// The station a station row names; None for the work rows.
    pub fn station(self) -> Option<Station> {
        match self {
            Row::Gunnery => Some(Station::Gunnery),
            Row::Reactor => Some(Station::Reactor),
            Row::Power => Some(Station::Power),
            Row::Helm => Some(Station::Helm),
            Row::Fire | Row::Repair | Row::Breach => None,
        }
    }
}

impl From<Station> for Row {
    fn from(s: Station) -> Row {
        match s {
            Station::Gunnery => Row::Gunnery,
            Station::Reactor => Row::Reactor,
            Station::Power => Row::Power,
            Station::Helm => Row::Helm,
        }
    }
}

/// The highest priority level (PULSAR's five nodes: 0 Ignore, 1 Low, 2 Medium, 3 High, 4 Max).
pub const MAX_LEVEL: u8 = 4;
/// The most bots the dev setting offers (BOTS 0/1/2/3).
pub const MAX_BOTS: u8 = 3;

/// The bots' numbers (`bots` in crew/bots.json). TODO(initiator): every value.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BotTuning {
    /// Bots at the start (the dev setting BOTS; 0..=3). TODO(initiator): 2, most sessions have two players.
    pub count: u8,
    /// The gunnery skill of a bot, 0..100 (`Weapons::set_skill`). TODO(initiator): 20.
    pub skill: f64,
    /// The priority level of each row, in `Row::ALL` order (gunnery, reactor, power, helm, fire, repair,
    /// breach). TODO(initiator): [3, 2, 2, 1] for the stations (GUNNERY High, REACTOR Medium, POWER
    /// Medium, HELM Low); [3, 3, 3] for the work rows (crew_core's engineer page, High each).
    pub levels: [u8; ROWS],
    /// Seconds between two bot action rounds (the host's cadence). TODO(initiator): 0.5.
    pub tick_s: f64,
    /// The jam clearing's speed against a player's hold (1 is a player). TODO(initiator): 0.5.
    pub clear_speed: f64,
    /// The POWER station's engines limit during an encounter (0..1). TODO(initiator): 0.6.
    pub fight_engines_limit: f64,
    /// The HELM's throttle, % (the normal hold). TODO(initiator): 100.
    pub throttle: f64,
    /// The HELM's throttle in an encounter while the reactor is hot, %. TODO(initiator): 30.
    pub hot_fight_throttle: f64,
    /// Seconds after a player leaves a station before a bot may take it (the player keeps their
    /// setting, e.g. Manual, while they walk off). TODO(initiator): 20.
    pub grace_s: f64,
    /// A bot's walking speed, m/s. TODO(initiator): 2.5 (crew_core's demo bot's speed).
    pub walk_speed: f64,
    /// How close a bot stops to a job's place, m. TODO(initiator): 1.5.
    pub reach_m: f64,
}

impl Default for BotTuning {
    fn default() -> BotTuning {
        BotTuning {
            count: 2,
            skill: 20.0,
            levels: [3, 2, 2, 1, 3, 3, 3],
            tick_s: 0.5,
            clear_speed: 0.5,
            fight_engines_limit: 0.6,
            throttle: 100.0,
            hot_fight_throttle: 30.0,
            grace_s: 20.0,
            walk_speed: 2.5,
            reach_m: 1.5,
        }
    }
}

/// What a bot does at its station, as plain values. The glue maps each to the ask a player's screen sends.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Action {
    /// GN-01: one shot (`ReactorAsk::Fire`).
    Fire,
    /// GN-01: the clearing hold, on or off (`ReactorAsk::ClearJam`).
    Hold(bool),
    /// The reactor's mode: `false` is Auto (`ReactorAsk::SetManual`).
    SetManual(bool),
    /// The coolant pump's mode (`ReactorAsk::SetPumpMode`).
    SetPump(PumpMode),
    /// A consumer group's limit, 0..1 (`ReactorAsk::SetLimit`).
    SetLimit(Group, f64),
    /// The engines' throttle, % (`ReactorAsk::SetThrottle`).
    SetThrottle(f64),
}

/// Where the things are on the ground (dev area metres, x east, y north), as the glue lays them out.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Layout {
    /// Each station's screen, by `Station::index`: the place a station job walks to.
    pub stations: [[f64; 2]; STATIONS],
    /// Where the bots start: the pads, one per bot slot (`MAX_BOTS`).
    pub pads: [[f64; 2]; MAX_BOTS as usize],
}

/// One piece of work a bot can take. `key` names it while it exists: a station is its index in
/// `Station::ALL`, the glue numbers the work jobs from `FIRST_WORK_KEY` on. `place` is where the bot
/// walks to. `taken_by_player`: a player is on it, so no bot takes or keeps it.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Job {
    pub key: u64,
    pub row: Row,
    pub place: [f64; 2],
    pub taken_by_player: bool,
}

/// The first key of a work job (a station's keys are below it).
pub const FIRST_WORK_KEY: u64 = 100;

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// The job a bot at `pos` takes among `jobs`: the highest row level above Ignore, the nearest of equal
/// levels, the first in `jobs` of equal distance. A job a player is on is no job. Returns its index.
pub fn choose(pos: [f64; 2], jobs: &[Job], levels: &[u8; ROWS]) -> Option<usize> {
    let mut best: Option<(usize, u8, f64)> = None;
    for (i, j) in jobs.iter().enumerate() {
        let level = levels[j.row.index()];
        if j.taken_by_player || level == 0 {
            continue;
        }
        let d = dist(pos, j.place);
        let better = match best {
            None => true,
            Some((_, best_level, best_d)) => level > best_level || (level == best_level && d < best_d),
        };
        if better {
            best = Some((i, level, d));
        }
    }
    best.map(|b| b.0)
}

/// One bot's body and its job. The glue draws the figure at `pos` and reads `job` and `arrived`.
#[derive(Clone, Debug, PartialEq)]
pub struct Walker {
    /// Dev area metres.
    pub pos: [f64; 2],
    pub job: Option<Job>,
    /// The bot is within reach of its job's place: the job is worked from here.
    pub arrived: bool,
}

impl Walker {
    pub fn at(pos: [f64; 2]) -> Walker {
        Walker { pos, job: None, arrived: false }
    }

    /// The bot's choice for this frame. Its job stays while it is offered (no player on it, still a
    /// level above Ignore), and a job of a higher row takes it over. `held`: the keys the other bots
    /// hold, which this bot may not take. A job that goes drops the bot's arrival at once.
    pub fn think(&mut self, jobs: &[Job], levels: &[u8; ROWS], held: &[u64]) {
        let current = self.job.and_then(|j| jobs.iter().find(|x| x.key == j.key).copied()).filter(|j| !j.taken_by_player && levels[j.row.index()] > 0);
        let offered: Vec<Job> = jobs.iter().filter(|j| !held.contains(&j.key)).copied().collect();
        let pick = choose(self.pos, &offered, levels).map(|i| offered[i]);
        let next = match (current, pick) {
            (Some(c), Some(p)) if levels[p.row.index()] > levels[c.row.index()] => Some(p),
            (Some(c), _) => Some(c),
            (None, p) => p,
        };
        if next.map(|j| j.key) != self.job.map(|j| j.key) {
            self.arrived = false;
        }
        self.job = next;
    }

    /// Walks `dt` s at `speed` m/s toward the job's place and stops `reach` m short of it. `arrived`
    /// once it is there; a bot with no job stands still and is not arrived.
    pub fn walk(&mut self, dt: f64, speed: f64, reach: f64) {
        let Some(j) = self.job else {
            self.arrived = false;
            return;
        };
        let d = dist(self.pos, j.place);
        if d <= reach {
            self.arrived = true;
            return;
        }
        let step = (speed * dt).min(d - reach);
        self.pos = [self.pos[0] + (j.place[0] - self.pos[0]) / d * step, self.pos[1] + (j.place[1] - self.pos[1]) / d * step];
        self.arrived = dist(self.pos, j.place) <= reach + 1e-9;
    }
}

/// The engine room's encounter is on (the enemy has not ended it).
fn fighting(room: &EngineRoom) -> bool {
    room.encounter().is_some_and(|e| e.outcome() == Outcome::None)
}

/// The reactor is above its optimal temperature band.
fn hot(room: &EngineRoom) -> bool {
    room.reactor().temperature() > room.reactor().bands().temp_optimal.hi
}

/// What a bot does at `station` now. Pure: the room is read, nothing is changed.
pub fn bot_step(station: Station, t: &BotTuning, room: &EngineRoom) -> Vec<Action> {
    match station {
        Station::Gunnery => {
            // Waiting while overheated is the skill difference: a bot never presses a locked gun.
            let w = room.weapons();
            if !fighting(room) {
                return vec![Action::Hold(false)];
            }
            if w.jammed() {
                return vec![Action::Hold(true)];
            }
            let mut out = vec![Action::Hold(false)];
            if !w.overheated() && w.shots() > 0 && w.reload_left() <= 0.0 {
                out.push(Action::Fire);
            }
            out
        }
        Station::Reactor => {
            let mut out = vec![Action::SetManual(false)];
            if hot(room) {
                out.push(Action::SetPump(PumpMode::High));
            }
            out
        }
        Station::Power => {
            let fight = fighting(room);
            [Group::Engines, Group::LifeSupport, Group::Lights, Group::Shields, Group::Weapons, Group::JumpDrive]
                .into_iter()
                .map(|g| Action::SetLimit(g, if fight && g == Group::Engines { t.fight_engines_limit } else { 1.0 }))
                .collect()
        }
        Station::Helm => {
            let pct = if fighting(room) && hot(room) { t.hot_fight_throttle } else { t.throttle };
            vec![Action::SetThrottle(pct)]
        }
    }
}

/// The bots' state: the count, the captain's levels, the walkers (one per bot). The host steps it.
#[derive(Clone, Debug)]
pub struct Bots {
    tuning: BotTuning,
    count: u8,
    levels: [u8; ROWS],
    /// Seconds since the last action round.
    timer: f64,
    walkers: Vec<Walker>,
    /// Seconds since a player last used each station (`f64::INFINITY` before any use).
    since_used: [f64; STATIONS],
}

impl Bots {
    pub fn new(t: &BotTuning) -> Bots {
        Bots { tuning: *t, count: t.count.min(MAX_BOTS), levels: t.levels.map(|l| l.min(MAX_LEVEL)), timer: 0.0, walkers: Vec::new(), since_used: [f64::INFINITY; STATIONS] }
    }

    /// The same settings (count and levels) with no state: a new room starts with no bot at a station.
    pub fn fresh(&self) -> Bots {
        Bots::new(&BotTuning { count: self.count, levels: self.levels, ..self.tuning })
    }

    pub fn count(&self) -> u8 {
        self.count
    }

    pub fn set_count(&mut self, n: u8) {
        self.count = n.min(MAX_BOTS);
    }

    pub fn level(&self, row: Row) -> u8 {
        self.levels[row.index()]
    }

    pub fn set_level(&mut self, row: Row, level: u8) {
        self.levels[row.index()] = level.min(MAX_LEVEL);
    }

    /// The bots' bodies, one per bot, in slot order.
    pub fn walkers(&self) -> &[Walker] {
        &self.walkers
    }

    /// A bot is at the station's screen and runs it.
    pub fn manned(&self, s: Station) -> bool {
        self.manned_row(Row::from(s))
    }

    /// A bot is at a row's job and works it (a work row has no action yet: its bot stands there).
    pub fn manned_row(&self, row: Row) -> bool {
        self.walkers.iter().any(|w| w.arrived && w.job.is_some_and(|j| j.row == row))
    }

    /// The gun's skill: the bot's while a bot mans GN-01, else 0 (a player's gun has no skill yet).
    pub fn gun_skill(&self) -> f64 {
        if self.manned(Station::Gunnery) { self.tuning.skill } else { 0.0 }
    }

    /// The share of a player's jam-clearing speed: the bot's while it mans GN-01, else 1.
    pub fn clear_scale(&self) -> f64 {
        if self.manned(Station::Gunnery) { self.tuning.clear_speed } else { 1.0 }
    }

    /// One frame of `dt` seconds. `occupied` by `Station::index` (a player is at the station). A station
    /// stays a player's for `grace_s` after they leave it. The bots (one walker each, up to `count`,
    /// starting at `layout.pads`) think and walk; the bots act once per `tick_s`, and only the ones that
    /// have arrived at a station row. `extra`: the work jobs the glue offers (FIRE, REPAIR, BREACH), keys
    /// from `FIRST_WORK_KEY`. A walker that stops manning GN-01 releases the hold at once. Returns the
    /// (station, action) pairs to apply, in order.
    pub fn tick(&mut self, dt: f64, occupied: [bool; STATIONS], room: &EngineRoom, layout: &Layout, extra: &[Job]) -> Vec<(Station, Action)> {
        let before = self.manned(Station::Gunnery);
        for s in Station::ALL {
            let i = s.index();
            self.since_used[i] = if occupied[i] { 0.0 } else { self.since_used[i] + dt };
        }
        self.walkers.truncate(self.count as usize);
        while self.walkers.len() < self.count as usize {
            self.walkers.push(Walker::at(layout.pads[self.walkers.len()]));
        }
        let mut jobs: Vec<Job> = Station::ALL
            .iter()
            .map(|&s| {
                let age = self.since_used[s.index()];
                Job { key: s.index() as u64, row: Row::from(s), place: layout.stations[s.index()], taken_by_player: age <= 0.0 || age < self.tuning.grace_s }
            })
            .collect();
        jobs.extend_from_slice(extra);
        for i in 0..self.walkers.len() {
            let held: Vec<u64> = self.walkers.iter().enumerate().filter(|(j, _)| *j != i).filter_map(|(_, w)| w.job.map(|j| j.key)).collect();
            let w = &mut self.walkers[i];
            w.think(&jobs, &self.levels, &held);
            w.walk(dt, self.tuning.walk_speed, self.tuning.reach_m);
        }
        let mut out = Vec::new();
        if before && !self.manned(Station::Gunnery) {
            out.push((Station::Gunnery, Action::Hold(false)));
        }
        self.timer += dt;
        if self.timer + 1e-9 < self.tuning.tick_s {
            return out;
        }
        self.timer = 0.0;
        for s in Station::ALL {
            if self.manned(s) {
                out.extend(bot_step(s, &self.tuning, room).into_iter().map(|a| (s, a)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_room::{Input, RoomTuning};
    use crate::reactor::ReactorTuning;

    const LEVELS: [u8; ROWS] = [3, 2, 2, 1, 3, 3, 3];

    /// Stations 10 m apart along x, the pads 10 m south of the first.
    fn layout() -> Layout {
        Layout { stations: [[0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [30.0, 0.0]], pads: [[0.0, -10.0], [2.0, -10.0], [4.0, -10.0]] }
    }

    fn station_job(s: Station, taken: bool) -> Job {
        Job { key: s.index() as u64, row: Row::from(s), place: layout().stations[s.index()], taken_by_player: taken }
    }

    fn work(key: u64, row: Row, place: [f64; 2]) -> Job {
        Job { key, row, place, taken_by_player: false }
    }

    fn room() -> EngineRoom {
        let mut r = EngineRoom::new(RoomTuning::default(), ReactorTuning::default(), 1);
        r.set_power(true);
        r
    }

    /// An encounter that runs, with a charged gun.
    fn fight(r: &mut EngineRoom) {
        assert!(r.start_ambush());
        r.weapons_mut().step(10.0, 1.0);
    }

    /// Fires until the gun jams (seeded: the same shot jams on every run), cooling between shots.
    fn jam(r: &mut EngineRoom) {
        for _ in 0..2000 {
            if r.weapons().jammed() {
                return;
            }
            r.weapons_mut().step(10.0, 1.0);
            r.weapons_mut().fire();
        }
        panic!("the gun never jammed");
    }

    /// Fires until the gun locks with heat (each shot after its reload).
    fn overheat(r: &mut EngineRoom) {
        for _ in 0..20 {
            if r.weapons().overheated() {
                return;
            }
            r.weapons_mut().step(0.25, 1.0);
            assert!(r.weapons_mut().fire());
        }
        panic!("the gun never overheated");
    }

    /// Runs the reactor on manual full fission until it is above its optimal band.
    fn heat_up(r: &mut EngineRoom) {
        for _ in 0..20000 {
            let _ = r.step(0.5, Input::manual(100.0, 0.0));
            if r.reactor().temperature() > r.reactor().bands().temp_optimal.hi {
                return;
            }
        }
        panic!("the reactor never got hot");
    }

    /// Ticks the bots (0.5 s rounds) until `s` is manned, at most 200 s.
    fn arrive(b: &mut Bots, r: &EngineRoom, s: Station) {
        for _ in 0..400 {
            if b.manned(s) {
                return;
            }
            b.tick(0.5, [false; STATIONS], r, &layout(), &[]);
        }
        panic!("no bot reached {s:?}");
    }

    #[test]
    fn the_highest_row_level_is_chosen_before_the_distance() {
        // The helm is 1 m away at Low; the fire is 9 m away at High: the fire.
        let helm_near = Job { place: [1.0, 0.0], ..station_job(Station::Helm, false) };
        let jobs = [helm_near, work(100, Row::Fire, [9.0, 0.0])];
        assert_eq!(choose([0.0, 0.0], &jobs, &LEVELS), Some(1));
    }

    #[test]
    fn of_equal_levels_the_nearest_is_chosen() {
        let jobs = [work(100, Row::Repair, [8.0, 0.0]), work(101, Row::Repair, [3.0, 4.0])];
        assert_eq!(choose([0.0, 0.0], &jobs, &LEVELS), Some(1), "3-4-5 is nearer than 8");
        assert_eq!(choose([0.0, 0.0], &jobs, &[3, 3, 3, 3, 3, 3, 3]), Some(1));
    }

    #[test]
    fn ignore_and_a_job_a_player_is_on_are_never_chosen() {
        let ignored = [0, 2, 2, 1, 0, 0, 0];
        assert_eq!(choose([0.0, 0.0], &[work(100, Row::Fire, [0.0, 0.0]), station_job(Station::Helm, false)], &ignored), Some(1));
        assert_eq!(choose([0.0, 0.0], &[station_job(Station::Gunnery, true), station_job(Station::Helm, false)], &LEVELS), Some(1));
        assert_eq!(choose([0.0, 0.0], &[station_job(Station::Gunnery, true)], &LEVELS), None);
        assert_eq!(choose([0.0, 0.0], &[], &LEVELS), None);
    }

    #[test]
    fn a_walker_walks_at_its_speed_and_stops_one_reach_short() {
        let mut w = Walker::at([0.0, 0.0]);
        w.think(&[station_job(Station::Helm, false)], &LEVELS, &[]);
        assert_eq!(w.job.map(|j| j.row), Some(Row::Helm));
        w.walk(1.0, 2.0, 1.5);
        assert!((w.pos[0] - 2.0).abs() < 1e-9, "2 m in 1 s at 2 m/s, got {:?}", w.pos);
        assert!(!w.arrived);
        w.walk(100.0, 2.0, 1.5);
        assert!(w.arrived);
        assert!((w.pos[0] - 28.5).abs() < 1e-9, "the helm is at 30 m: it stops 1.5 m short, got {:?}", w.pos);
        w.walk(1.0, 2.0, 1.5);
        assert!((w.pos[0] - 28.5).abs() < 1e-9, "arrived: no more walking");
    }

    #[test]
    fn a_walker_drops_a_job_a_player_takes_at_once() {
        let mut w = Walker::at([0.0, 0.0]);
        w.think(&[station_job(Station::Helm, false)], &LEVELS, &[]);
        w.think(&[station_job(Station::Helm, true)], &LEVELS, &[]);
        assert_eq!(w.job, None);
        assert!(!w.arrived);
    }

    #[test]
    fn a_job_that_is_no_longer_offered_is_dropped() {
        let mut w = Walker::at([0.0, 0.0]);
        w.think(&[work(100, Row::Fire, [1.0, 0.0])], &LEVELS, &[]);
        assert!(w.job.is_some());
        w.think(&[], &LEVELS, &[]);
        assert_eq!(w.job, None);
    }

    #[test]
    fn a_higher_row_takes_a_walker_over_and_an_equal_one_does_not() {
        let mut w = Walker::at([0.0, 0.0]);
        w.think(&[station_job(Station::Helm, false)], &LEVELS, &[]);
        // A fire (High, 3) at 50 m against the helm (Low, 1): the walker switches.
        let jobs = [station_job(Station::Helm, false), work(100, Row::Fire, [50.0, 0.0])];
        w.think(&jobs, &LEVELS, &[]);
        assert_eq!(w.job.map(|j| j.row), Some(Row::Fire));
        // A repair (High, 3) nearer than the fire does not take a bot off a fire of the same level.
        let jobs = [work(100, Row::Fire, [50.0, 0.0]), work(101, Row::Repair, [1.0, 0.0])];
        w.think(&jobs, &LEVELS, &[]);
        assert_eq!(w.job.map(|j| j.row), Some(Row::Fire));
    }

    #[test]
    fn a_job_held_by_another_bot_is_not_offered() {
        let mut w = Walker::at([0.0, 0.0]);
        w.think(&[station_job(Station::Helm, false)], &LEVELS, &[Station::Helm.index() as u64]);
        assert_eq!(w.job, None);
    }

    #[test]
    fn the_bots_spawn_at_the_pads_one_per_slot() {
        let r = room();
        let mut b = Bots::new(&BotTuning::default());
        b.tick(0.0, [false; STATIONS], &r, &layout(), &[]);
        let pos: Vec<[f64; 2]> = b.walkers().iter().map(|w| w.pos).collect();
        assert_eq!(pos.len(), 2);
        assert_eq!(pos[0], layout().pads[0]);
        assert_eq!(pos[1], layout().pads[1]);
        b.set_count(0);
        b.tick(0.0, [false; STATIONS], &r, &layout(), &[]);
        assert!(b.walkers().is_empty(), "no bots, no bodies");
    }

    #[test]
    fn a_bot_walks_to_its_station_before_it_works_it() {
        let r = room();
        let mut b = Bots::new(&BotTuning { count: 1, ..BotTuning::default() });
        // The gunnery is 10 m from the pad (less the reach): the bot is not manned for the first 2 s.
        let mut acts_walking = Vec::new();
        for _ in 0..4 {
            acts_walking.extend(b.tick(0.5, [false; STATIONS], &r, &layout(), &[]));
        }
        assert!(!b.manned(Station::Gunnery));
        assert!(acts_walking.iter().all(|(s, _)| *s != Station::Gunnery), "no gunnery action while walking");
        arrive(&mut b, &r, Station::Gunnery);
        assert!(b.manned(Station::Gunnery));
        let acts = b.tick(0.5, [false; STATIONS], &r, &layout(), &[]);
        assert!(acts.contains(&(Station::Gunnery, Action::Hold(false))), "arrived: the bot acts");
    }

    #[test]
    fn the_highest_row_gets_the_first_bot_and_no_two_bots_take_one_job() {
        let r = room();
        let mut b = Bots::new(&BotTuning { count: 2, levels: [0, 0, 0, 1, 0, 0, 0], ..BotTuning::default() });
        for _ in 0..400 {
            b.tick(0.5, [false; STATIONS], &r, &layout(), &[]);
        }
        let on_helm = b.walkers().iter().filter(|w| w.job.is_some_and(|j| j.row == Row::Helm)).count();
        assert_eq!(on_helm, 1, "one bot per station");
        assert_eq!(b.walkers().iter().filter(|w| w.job.is_none()).count(), 1, "the second bot has no job");
    }

    #[test]
    fn a_player_at_a_station_is_no_job_and_a_bot_at_it_drops_it_at_once() {
        let r = room();
        let mut b = Bots::new(&BotTuning { count: 1, ..BotTuning::default() });
        arrive(&mut b, &r, Station::Gunnery);
        // A player sits down at GN-01: the bot releases the hold in this very tick and walks on.
        let acts = b.tick(0.5, [true, false, false, false], &r, &layout(), &[]);
        assert!(acts.contains(&(Station::Gunnery, Action::Hold(false))));
        assert!(!b.manned(Station::Gunnery));
        assert!(b.walkers()[0].job.is_some_and(|j| j.row != Row::Gunnery), "the next job is another station");
    }

    #[test]
    fn a_station_a_player_left_stays_theirs_for_the_grace_time() {
        let r = room();
        let mut b = Bots::new(&BotTuning { count: 1, grace_s: 20.0, levels: [4, 0, 0, 0, 0, 0, 0], ..BotTuning::default() });
        b.tick(0.0, [false; STATIONS], &r, &layout(), &[]);
        // Gunnery was used 5 s ago: under the grace, no job.
        b.since_used[0] = 5.0;
        b.tick(0.0, [false; STATIONS], &r, &layout(), &[]);
        assert_eq!(b.walkers()[0].job, None);
        b.since_used[0] = 25.0;
        b.tick(0.0, [false; STATIONS], &r, &layout(), &[]);
        assert!(b.walkers()[0].job.is_some_and(|j| j.row == Row::Gunnery), "the grace is over");
    }

    #[test]
    fn a_work_job_is_walked_to_and_is_no_action_here() {
        let r = room();
        let mut b = Bots::new(&BotTuning { count: 1, levels: [0, 0, 0, 0, 3, 3, 3], ..BotTuning::default() });
        let fire = work(FIRST_WORK_KEY, Row::Fire, [0.0, -6.0]);
        b.tick(0.5, [false; STATIONS], &r, &layout(), &[fire]);
        assert_eq!(b.walkers()[0].job.map(|j| j.key), Some(FIRST_WORK_KEY));
        for _ in 0..10 {
            let acts = b.tick(0.5, [false; STATIONS], &r, &layout(), &[fire]);
            assert!(acts.is_empty(), "the work rows have no action yet (TODO)");
        }
        assert!(b.walkers()[0].arrived, "it walks to the fire");
    }

    #[test]
    fn the_count_and_the_levels_are_clamped_and_a_fresh_room_keeps_them() {
        let mut b = Bots::new(&BotTuning::default());
        b.set_count(9);
        b.set_level(Row::Power, 7);
        assert_eq!((b.count(), b.level(Row::Power)), (MAX_BOTS, MAX_LEVEL));
        let f = b.fresh();
        assert_eq!((f.count(), f.level(Row::Power)), (MAX_BOTS, MAX_LEVEL));
        assert!(f.walkers().is_empty());
    }

    #[test]
    fn the_level_is_clamped_to_max() {
        let mut b = Bots::new(&BotTuning { levels: [9; ROWS], ..BotTuning::default() });
        assert_eq!(b.level(Row::Breach), MAX_LEVEL);
        b.set_level(Row::Fire, 200);
        assert_eq!(b.level(Row::Fire), MAX_LEVEL);
    }

    #[test]
    fn the_work_rows_are_the_stations_after_them_and_the_stations_map_both_ways() {
        for s in Station::ALL {
            assert_eq!(Row::from(s).station(), Some(s));
        }
        assert_eq!(Row::Fire.station(), None);
        assert_eq!(Row::ALL.len(), ROWS);
    }

    #[test]
    fn gunnery_fires_in_a_fight_when_a_shot_is_charged_and_not_outside_one() {
        let t = BotTuning::default();
        let mut r = room();
        assert!(!bot_step(Station::Gunnery, &t, &r).contains(&Action::Fire), "no fight, no shot");
        fight(&mut r);
        assert!(bot_step(Station::Gunnery, &t, &r).contains(&Action::Fire));
    }

    #[test]
    fn gunnery_does_not_fire_with_an_empty_capacitor() {
        let t = BotTuning::default();
        let mut r = room();
        assert!(r.start_ambush());
        assert!(!bot_step(Station::Gunnery, &t, &r).contains(&Action::Fire));
    }

    #[test]
    fn gunnery_waits_while_overheated_and_fires_once_it_has_cooled() {
        let t = BotTuning::default();
        let mut r = room();
        fight(&mut r);
        overheat(&mut r);
        assert!(!bot_step(Station::Gunnery, &t, &r).contains(&Action::Fire));
        r.weapons_mut().step(30.0, 1.0);
        assert!(bot_step(Station::Gunnery, &t, &r).contains(&Action::Fire));
    }

    #[test]
    fn gunnery_holds_at_a_jam_and_fires_nothing_until_it_is_cleared() {
        let t = BotTuning::default();
        let mut r = room();
        fight(&mut r);
        jam(&mut r);
        assert_eq!(bot_step(Station::Gunnery, &t, &r), vec![Action::Hold(true)]);
    }

    #[test]
    fn the_reactor_is_on_auto_and_its_pump_is_high_only_when_hot() {
        let t = BotTuning::default();
        let mut r = room();
        assert_eq!(bot_step(Station::Reactor, &t, &r), vec![Action::SetManual(false)]);
        heat_up(&mut r);
        assert!(bot_step(Station::Reactor, &t, &r).contains(&Action::SetPump(PumpMode::High)));
    }

    #[test]
    fn power_gives_a_fight_its_shields_and_weapons_and_takes_the_engines_down() {
        let t = BotTuning::default();
        let mut r = room();
        fight(&mut r);
        let acts = bot_step(Station::Power, &t, &r);
        assert!(acts.contains(&Action::SetLimit(Group::Engines, t.fight_engines_limit)));
        assert!(acts.contains(&Action::SetLimit(Group::Shields, 1.0)));
        assert!(acts.contains(&Action::SetLimit(Group::Weapons, 1.0)));
        let quiet = room();
        assert!(bot_step(Station::Power, &t, &quiet).iter().all(|a| matches!(a, Action::SetLimit(_, v) if *v == 1.0)), "outside a fight every limit is 1");
    }

    #[test]
    fn the_helm_holds_full_throttle_and_thirty_only_in_a_hot_fight() {
        let t = BotTuning::default();
        let mut r = room();
        assert_eq!(bot_step(Station::Helm, &t, &r), vec![Action::SetThrottle(100.0)]);
        fight(&mut r);
        assert_eq!(bot_step(Station::Helm, &t, &r), vec![Action::SetThrottle(100.0)], "a cold reactor in a fight: full");
        heat_up(&mut r);
        assert_eq!(bot_step(Station::Helm, &t, &r), vec![Action::SetThrottle(30.0)]);
    }

    #[test]
    fn a_bot_at_the_gun_has_its_skill_and_clears_at_half_speed() {
        let r = room();
        let mut b = Bots::new(&BotTuning::default());
        assert_eq!((b.gun_skill(), b.clear_scale()), (0.0, 1.0), "no bot yet");
        arrive(&mut b, &r, Station::Gunnery);
        assert_eq!((b.gun_skill(), b.clear_scale()), (20.0, 0.5));
    }

    #[test]
    fn bots_act_once_per_tick_and_not_between() {
        let r = room();
        let mut b = Bots::new(&BotTuning { count: 0, ..BotTuning::default() });
        b.tick(0.25, [false; STATIONS], &r, &layout(), &[]);
        assert!(b.walkers().is_empty());
        let mut b = Bots::new(&BotTuning { count: 1, ..BotTuning::default() });
        arrive(&mut b, &r, Station::Gunnery);
        let mut rounds = 0;
        for _ in 0..10 {
            if !b.tick(0.1, [false; STATIONS], &r, &layout(), &[]).is_empty() {
                rounds += 1;
            }
        }
        assert_eq!(rounds, 2, "1 s of 0.1 s frames is two 0.5 s rounds");
    }
}
