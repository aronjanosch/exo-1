//! The bots (r42, #253): crew that man an empty station at a lower skill than a player. Abstract, as
//! #253 asks: no body, no walking, no pathfinding, no repairs. A bot is a station operator. Each
//! station's bot works the station's controls through the same asks a player's screen sends (`Action`,
//! which the glue maps to `ReactorAsk`), and a station with a player on it is left alone.
//!
//! The priority list (PULSAR, docs/research/pulsar-mech-bot-priorities.md): five levels per row,
//! Ignore, Low, Medium, High, Max (section 1, from the manual), one row per station, the captain sets
//! them. The rows we port are the names the dump gives (section 3.1): MAN_TURRET ("man turret") for
//! GUNNERY, ENG_REACTOR_SAFETY ("engineering reactor safety") for REACTOR, the four ENG_*_FACTOR rows
//! ("power distribution per conduit") for POWER, and the HELM is ours. ENG_JUMP_SHIP ("jump ship") is
//! not ported: the jump is the captain's call (#253). Ignore is zero weight: a row at Ignore is never
//! taken. Section 6 of the note asks whether Ignore disables or only weights; we disable (TODO(initiator)).
//!
//! The behaviours follow Barotrauma: AIObjectiveOperateItem.cs:54-177 gives an operate objective the
//! priority 0 when another crew member operates the item (line 151, `IsItemOperatedByAnother`) and when
//! a player last used the reactor (lines 108-113, `reactor.LastUserWasPlayer`): the bot leaves a station
//! a player has. The turret's auto-operation is Turret.cs `UpdateAutoOperate` (1006-1055), run only while
//! nobody is at the turret (`AutoOperate && ActiveUser == null`, Turret.cs:607-609), so it fires when a
//! shot is ready, not on a schedule of its own.
//!
//! Gunnery skill: the bot's gun skill is `BotTuning::skill` (TODO(initiator): 20; #253 asks a fixed skill
//! or the crew average, we take fixed). It goes to the gun's jam chance (`Weapons::set_skill`, the r41
//! shape). A bot clears a jam at `clear_speed` (half) of a player's hold speed.
//!
//! Units: time s, power as the grid's, temperature in degrees, throttle -100..100 %.
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

/// The highest priority level (PULSAR's five nodes: 0 Ignore, 1 Low, 2 Medium, 3 High, 4 Max).
pub const MAX_LEVEL: u8 = 4;
/// The most bots the dev setting offers (BOTS 0/1/2/3).
pub const MAX_BOTS: u8 = 3;

/// One row of the captain's list: a station and its level (0..=`MAX_LEVEL`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Priority {
    pub station: Station,
    pub level: u8,
}

impl Priority {
    /// The row at `level`, clamped to 0..=`MAX_LEVEL`.
    pub fn new(station: Station, level: u8) -> Priority {
        Priority { station, level: level.min(MAX_LEVEL) }
    }
}

/// The bots' numbers (`bots` in reactor.json). TODO(initiator): every value.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BotTuning {
    /// Bots at the start (the dev setting BOTS; 0..=3). TODO(initiator): 2, most sessions have two players.
    pub count: u8,
    /// The gunnery skill of a bot, 0..100 (`Weapons::set_skill`). TODO(initiator): 20.
    pub skill: f64,
    /// The priority level of each station, in `Station::ALL` order (gunnery, reactor, power, helm).
    /// TODO(initiator): [3, 2, 2, 1] (GUNNERY High, REACTOR Medium, POWER Medium, HELM Low).
    pub levels: [u8; STATIONS],
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
}

impl Default for BotTuning {
    fn default() -> BotTuning {
        BotTuning { count: 2, skill: 20.0, levels: [3, 2, 2, 1], tick_s: 0.5, clear_speed: 0.5, fight_engines_limit: 0.6, throttle: 100.0, hot_fight_throttle: 30.0, grace_s: 20.0 }
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

/// The assignment: which stations get a bot. `bots` bots, `since_used` by `Station::index` (the seconds
/// since a player last used the station; `f64::INFINITY` for never; 0 while a player is at it), `grace_s`
/// the seconds a station stays a player's after they leave it, `list` the captain's rows. A station is
/// taken by a player while its age is under the grace (a player at it is age 0, which holds at any
/// grace). Ignore (level 0) is never taken, a station with no row is not taken, the highest level goes
/// first, ties go by `Station::ALL`'s order, one bot per station. Returns the stations in the order
/// they were taken.
pub fn assign(bots: u8, since_used: [f64; STATIONS], grace_s: f64, list: &[Priority]) -> Vec<Station> {
    let free = since_used.map(|age| !(age <= 0.0 || age < grace_s));
    let mut rows: Vec<Priority> = list.iter().copied().filter(|p| p.level > 0 && free[p.station.index()]).collect();
    rows.sort_by(|a, b| b.level.cmp(&a.level).then(a.station.cmp(&b.station)));
    let mut taken: Vec<Station> = Vec::new();
    for p in rows {
        if taken.len() >= bots as usize {
            break;
        }
        if !taken.contains(&p.station) {
            taken.push(p.station);
        }
    }
    taken
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

/// The bots' state: the count, the captain's levels, which stations they man now. The host steps it.
#[derive(Clone, Debug)]
pub struct Bots {
    tuning: BotTuning,
    count: u8,
    levels: [u8; STATIONS],
    /// Seconds since the last action round.
    timer: f64,
    manned: [bool; STATIONS],
    /// Seconds since a player last used each station (`f64::INFINITY` before any use).
    since_used: [f64; STATIONS],
}

impl Bots {
    pub fn new(t: &BotTuning) -> Bots {
        Bots { tuning: *t, count: t.count.min(MAX_BOTS), levels: t.levels.map(|l| l.min(MAX_LEVEL)), timer: 0.0, manned: [false; STATIONS], since_used: [f64::INFINITY; STATIONS] }
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

    pub fn level(&self, s: Station) -> u8 {
        self.levels[s.index()]
    }

    pub fn set_level(&mut self, s: Station, level: u8) {
        self.levels[s.index()] = level.min(MAX_LEVEL);
    }

    pub fn list(&self) -> [Priority; STATIONS] {
        Station::ALL.map(|s| Priority::new(s, self.level(s)))
    }

    pub fn manned(&self, s: Station) -> bool {
        self.manned[s.index()]
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
    /// stays a player's for `grace_s` after they leave it. The manning follows that at once, and a bot
    /// that leaves the gun releases its hold at once. The bots act once per `tick_s`. Returns the
    /// (station, action) pairs to apply, in order.
    pub fn tick(&mut self, dt: f64, occupied: [bool; STATIONS], room: &EngineRoom) -> Vec<(Station, Action)> {
        let before = self.manned;
        for s in Station::ALL {
            let i = s.index();
            self.since_used[i] = if occupied[i] { 0.0 } else { self.since_used[i] + dt };
        }
        self.manned = [false; STATIONS];
        for s in assign(self.count, self.since_used, self.tuning.grace_s, &self.list()) {
            self.manned[s.index()] = true;
        }
        let mut out = Vec::new();
        if before[Station::Gunnery.index()] && !self.manned(Station::Gunnery) {
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

    /// Every station unused for ever: no player has been at any of them.
    const ALL_FREE: [f64; STATIONS] = [f64::INFINITY; STATIONS];
    const GRACE: f64 = 20.0;

    fn room() -> EngineRoom {
        let mut r = EngineRoom::new(RoomTuning::default(), ReactorTuning::default(), 1);
        r.set_power(true);
        r
    }

    fn list(levels: [u8; STATIONS]) -> Vec<Priority> {
        Station::ALL.iter().zip(levels).map(|(s, l)| Priority::new(*s, l)).collect()
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

    #[test]
    fn the_highest_level_takes_the_first_bot() {
        assert_eq!(assign(1, ALL_FREE, GRACE, &list([1, 4, 2, 3])), vec![Station::Reactor]);
    }

    #[test]
    fn bots_go_down_the_list_and_stop_at_their_count() {
        assert_eq!(assign(2, ALL_FREE, GRACE, &list([1, 4, 2, 3])), vec![Station::Reactor, Station::Helm]);
        assert_eq!(assign(3, ALL_FREE, GRACE, &list([1, 4, 2, 3])), vec![Station::Reactor, Station::Helm, Station::Power]);
    }

    #[test]
    fn ignore_is_never_taken_however_many_bots() {
        assert_eq!(assign(3, ALL_FREE, GRACE, &list([0, 0, 0, 1])), vec![Station::Helm]);
        assert_eq!(assign(3, ALL_FREE, GRACE, &list([0, 0, 0, 0])), Vec::<Station>::new());
    }

    #[test]
    fn ties_go_by_the_station_order() {
        assert_eq!(assign(2, ALL_FREE, GRACE, &list([2, 2, 2, 2])), vec![Station::Gunnery, Station::Reactor]);
        assert_eq!(assign(4, ALL_FREE, GRACE, &list([2, 2, 2, 2])), Station::ALL.to_vec());
    }

    #[test]
    fn a_player_occupied_station_is_skipped() {
        let used_now = [0.0, f64::INFINITY, f64::INFINITY, f64::INFINITY];
        assert_eq!(assign(1, used_now, GRACE, &list([4, 1, 1, 1])), vec![Station::Reactor]);
    }

    #[test]
    fn a_station_a_player_left_stays_theirs_for_the_grace_time() {
        // Gunnery was used 5 s ago (under the 20 s grace): the bot takes the reactor, the next row.
        let left = [5.0, f64::INFINITY, f64::INFINITY, f64::INFINITY];
        assert_eq!(assign(1, left, GRACE, &list([4, 1, 1, 1])), vec![Station::Reactor]);
        // 25 s after: the grace is over and the gunnery's bot may take it back.
        let gone = [25.0, f64::INFINITY, f64::INFINITY, f64::INFINITY];
        assert_eq!(assign(1, gone, GRACE, &list([4, 1, 1, 1])), vec![Station::Gunnery]);
    }

    #[test]
    fn no_bots_takes_nothing_and_a_station_without_a_row_is_not_taken() {
        assert_eq!(assign(0, ALL_FREE, GRACE, &list([4, 4, 4, 4])), Vec::<Station>::new());
        assert_eq!(assign(3, ALL_FREE, GRACE, &[Priority::new(Station::Helm, 2)]), vec![Station::Helm]);
    }

    #[test]
    fn the_level_is_clamped_to_max() {
        assert_eq!(Priority::new(Station::Power, 9).level, MAX_LEVEL);
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
    fn a_player_at_a_station_takes_it_over_and_the_bot_lets_go_of_the_gun() {
        let mut r = room();
        fight(&mut r);
        let mut b = Bots::new(&BotTuning::default());
        let acts = b.tick(0.5, [false; STATIONS], &r);
        assert!(b.manned(Station::Gunnery));
        assert!(acts.iter().any(|(s, _)| *s == Station::Gunnery));
        let acts = b.tick(0.1, [true, false, false, false], &r);
        assert!(!b.manned(Station::Gunnery));
        assert!(acts.contains(&(Station::Gunnery, Action::Hold(false))), "the hold is released at once");
        assert!(b.manned(Station::Power), "the list's next row takes the free station");
    }

    #[test]
    fn a_bot_at_the_gun_has_its_skill_and_clears_at_half_speed() {
        let r = room();
        let mut b = Bots::new(&BotTuning::default());
        assert_eq!((b.gun_skill(), b.clear_scale()), (0.0, 1.0), "no bot yet");
        b.tick(0.0, [false; STATIONS], &r);
        assert!(b.manned(Station::Gunnery));
        assert_eq!((b.gun_skill(), b.clear_scale()), (20.0, 0.5));
    }

    #[test]
    fn bots_act_once_per_tick_and_not_between() {
        let r = room();
        let mut b = Bots::new(&BotTuning::default());
        assert!(b.tick(0.25, [false; STATIONS], &r).is_empty(), "the first 0.25 s is no round");
        assert!(!b.tick(0.25, [false; STATIONS], &r).is_empty(), "the round at 0.5 s");
        assert!(b.tick(0.1, [false; STATIONS], &r).is_empty());
    }

    #[test]
    fn the_count_and_the_levels_are_clamped_and_a_fresh_room_keeps_them() {
        let mut b = Bots::new(&BotTuning::default());
        b.set_count(9);
        b.set_level(Station::Power, 7);
        assert_eq!((b.count(), b.level(Station::Power)), (MAX_BOTS, MAX_LEVEL));
        let f = b.fresh();
        assert_eq!((f.count(), f.level(Station::Power)), (MAX_BOTS, MAX_LEVEL));
        assert!(!f.manned(Station::Gunnery));
    }
}
