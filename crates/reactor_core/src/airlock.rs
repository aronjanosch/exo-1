//! The airlock (r40, #267; one-button cycle #309): an inner and an outer door that are never open
//! together, and a pressure cycle in between that pumps the air back into the ship instead of losing
//! it. The glue (`exo_app` dev_area airlock) moves the air and shows the doors; this file is the state
//! machine.
//!
//! One push runs the whole cycle: the warning light is on, the open door (if any) closes and locks,
//! the air moves for `cycle_s` (in on the way in, back into the ship on the way out), the light goes
//! out and the other door opens. The airlock rests where the last cycle left it (`Side`): inside the
//! inner door is an ordinary door (`toggle_inner`, F from the ship), the outer one is locked; outside
//! the outer door stays open and the inner one is locked. A press during a cycle is ignored; there is
//! no cancel.
//!
//! Sources:
//! - Barotrauma's airlock is a hull flagged as a wet room (`Hull.IsWetRoom`, read by
//!   `IndoorsSteeringManager.cs:776` and `HumanAIController.cs:633`) between two doors. Its content is
//!   `content/Items/Assemblies/AutomaticHorizontalAirlock.xml`: "When one door is toggled, the other is
//!   automatically closed", with a pump in the room (`smallpump`). We close the open door for the
//!   player at the start of the cycle (#309).
//! - The door's open state is `Door.cs:176-186` (`IsOpen` sets `OpenState` to 1 or 0; the slide follows
//!   it). The stuck and weld fields (`Door.cs:116-128`) are not here yet: no weld in 0.1.
//! - Barotrauma's pump moves water (`Pump.cs`), so the cycle is ours. PULSAR has a slot type AIRLOCK
//!   (`docs/research/pulsar-reference.md:77`, slot types), which is the same job for the ship.
//! - Values: the cycle length and the pumped share are ours, `TODO(initiator)`: 3 s and 95 %. The
//!   5 % that stay in the room leave with the outer door (into space, when the outside is vacuum).
use serde::{Deserialize, Serialize};

/// `airlock` in hull/hull.json (r40). Every value is a start value, `TODO(initiator)`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AirlockTuning {
    #[serde(default, rename = "_comment")]
    pub comment: Option<String>,
    /// Seconds of the pressure change of one cycle (`TODO(initiator)`: 3 s).
    pub cycle_s: f64,
    /// Seconds the open door takes to shut at the start of a cycle, before the air moves: the glue's
    /// door slab takes the same time to slide (`DOOR_TIME`).
    #[serde(default = "default_close_s")]
    pub close_s: f64,
    /// The share of the room's air the cycle moves over (0..1, `TODO(initiator)`: 95 %).
    pub pump_share: f64,
}

fn default_close_s() -> f64 {
    1.0
}

impl Default for AirlockTuning {
    fn default() -> AirlockTuning {
        AirlockTuning { comment: None, cycle_s: 3.0, close_s: default_close_s(), pump_share: 0.95 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Door {
    Inner,
    Outer,
}

/// Where the airlock rests: `Inside` (air in, the inner door is the ship's, the outer one locked) or
/// `Outside` (chamber empty, the outer door open, the inner one locked). A cycle moves it to the other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    #[default]
    Inside,
    Outside,
}

/// Which way the cycle's air moves: `Out` from the room to the corridor (the pump back, before the
/// outer door opens), `In` from the corridor into the room (the fill before the inner door opens).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Out,
    In,
}

/// The airlock's state as a save holds it: doors, the rest side, the cycle and its time (the tuning
/// comes back).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AirlockSave {
    pub inner_open: bool,
    pub outer_open: bool,
    #[serde(default)]
    pub side: Side,
    /// The door the running cycle opens at its end.
    pub cycling: Option<Door>,
    pub t: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Airlock {
    cycle_s: f64,
    close_s: f64,
    pump_share: f64,
    inner_open: bool,
    outer_open: bool,
    side: Side,
    /// The cycle running, and the door it opens at its end; None when the room is idle.
    cycling: Option<Door>,
    /// Seconds into the cycle (the door's closing first, then the pressure change).
    t: f64,
}

impl Airlock {
    /// Both doors shut, at rest inside (a test's start).
    pub fn new(t: &AirlockTuning) -> Airlock {
        Airlock { cycle_s: t.cycle_s, close_s: t.close_s, pump_share: t.pump_share, inner_open: false, outer_open: false, side: Side::Inside, cycling: None, t: 0.0 }
    }

    /// The ship as it stands on the ground at the start: at rest outside, the outer door open, so a crew
    /// outside walks straight in (the initiator, 2026-10-15); the inner door shut and locked.
    pub fn outer_open(t: &AirlockTuning) -> Airlock {
        Airlock { outer_open: true, side: Side::Outside, ..Airlock::new(t) }
    }

    pub fn to_save(&self) -> AirlockSave {
        AirlockSave { inner_open: self.inner_open, outer_open: self.outer_open, side: self.side, cycling: self.cycling, t: self.t }
    }

    pub fn from_save(t: &AirlockTuning, s: &AirlockSave) -> Airlock {
        // The doors are never open together, whatever a save says: the inner one wins inside.
        let outer_open = s.outer_open && !s.inner_open;
        Airlock { cycle_s: t.cycle_s, close_s: t.close_s, pump_share: t.pump_share, inner_open: s.inner_open, outer_open, side: s.side, cycling: s.cycling, t: s.t }
    }

    pub fn is_open(&self, d: Door) -> bool {
        match d {
            Door::Inner => self.inner_open,
            Door::Outer => self.outer_open,
        }
    }

    pub fn side(&self) -> Side {
        self.side
    }

    /// The door the running cycle opens at its end; None when idle. The warning light is on while this is Some.
    pub fn cycling(&self) -> Option<Door> {
        self.cycling
    }

    /// The air's direction of the running cycle; None when idle.
    pub fn direction(&self) -> Option<Dir> {
        match self.cycling? {
            Door::Inner => Some(Dir::In),
            Door::Outer => Some(Dir::Out),
        }
    }

    /// The cycle's progress over the closing and the pressure change, 0..1 (0 when idle).
    pub fn progress(&self) -> f64 {
        if self.cycling.is_some() { (self.t / (self.close_s + self.cycle_s)).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// The one press of the panels: starts the cycle to the other side and closes the open door at
    /// once (the door shuts and locks, then the air moves, then the target door opens). Ignored (false)
    /// while a cycle runs; there is no cancel.
    pub fn press(&mut self) -> bool {
        if self.cycling.is_some() {
            return false;
        }
        self.cycling = Some(match self.side {
            Side::Inside => Door::Outer,
            Side::Outside => Door::Inner,
        });
        self.t = 0.0;
        self.inner_open = false;
        self.outer_open = false;
        true
    }

    /// F on the inner door from the ship: an ordinary door while the airlock rests inside, locked
    /// otherwise (outside, or while a cycle runs). True when it moved the door.
    pub fn toggle_inner(&mut self) -> bool {
        if self.cycling.is_some() || self.side != Side::Inside {
            return false;
        }
        self.inner_open = !self.inner_open;
        true
    }

    /// The captain's remote close (r41, CP-01's SEAL): shuts an open door while no cycle runs. It never
    /// opens a door, so the doors stay apart. True when it shut one.
    pub fn close(&mut self, d: Door) -> bool {
        if self.cycling.is_some() || !self.is_open(d) {
            return false;
        }
        self.set_open(d, false);
        true
    }

    /// Runs the cycle's timer; at its end the target door opens, the light goes out and the airlock
    /// rests on the new side.
    pub fn step(&mut self, dt: f64) {
        let Some(target) = self.cycling else { return };
        self.t += dt;
        if self.t + TIME_EPS >= self.close_s + self.cycle_s {
            self.cycling = None;
            self.t = 0.0;
            self.side = match target {
                Door::Inner => Side::Inside,
                Door::Outer => Side::Outside,
            };
            self.set_open(target, true);
        }
    }

    /// The share of the cycle's air that moves in the next `dt` seconds (0..`pump_share` over the whole
    /// pressure change, linear in time; nothing while the door closes). Call it before `step(dt)`. The
    /// glue multiplies it by the air at the start.
    pub fn pump_share(&self, dt: f64) -> f64 {
        let end = self.close_s + self.cycle_s;
        if self.cycling.is_none() || self.t + TIME_EPS >= end {
            return 0.0;
        }
        let from = self.t.max(self.close_s);
        let to = (self.t + dt).min(end);
        self.pump_share * (to - from).max(0.0) / self.cycle_s
    }

    fn set_open(&mut self, d: Door, open: bool) {
        match d {
            Door::Inner => self.inner_open = open,
            Door::Outer => self.outer_open = open,
        }
    }
}

/// Float slack on the cycle's clock: steps of 0.1 s add up to 3.999... for a 4 s cycle.
const TIME_EPS: f64 = 1e-9;

#[cfg(test)]
mod tests {
    use super::*;

    fn lock() -> Airlock {
        Airlock::new(&AirlockTuning::default())
    }

    fn total() -> f64 {
        let t = AirlockTuning::default();
        t.close_s + t.cycle_s
    }

    /// Runs `secs` in steps of `dt` and returns the share of air moved.
    fn run(l: &mut Airlock, secs: f64, dt: f64) -> f64 {
        let mut moved = 0.0;
        let mut left = secs;
        while left > 1e-12 {
            let h = dt.min(left);
            moved += l.pump_share(h);
            l.step(h);
            left -= h;
        }
        moved
    }

    #[test]
    fn the_doors_are_never_open_together() {
        // A fixed pseudo-random run of presses, F on the inner door, remote closes and time: the
        // invariant holds at every step.
        let mut l = lock();
        let mut seed: u64 = 0x2d40;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for _ in 0..20_000 {
            match next() % 4 {
                0 => {
                    l.press();
                }
                1 => {
                    l.toggle_inner();
                }
                2 => {
                    l.close(if next() % 2 == 0 { Door::Inner } else { Door::Outer });
                }
                _ => {}
            }
            let dt = [0.05, 0.5, 1.0, 3.0][next() % 4];
            l.step(dt);
            assert!(!(l.is_open(Door::Inner) && l.is_open(Door::Outer)), "both doors open");
            if l.cycling().is_some() {
                assert!(!l.is_open(Door::Inner) && !l.is_open(Door::Outer), "a door is open during a cycle");
            }
        }
    }

    #[test]
    fn a_cycle_always_finishes_and_opens_the_other_door() {
        for dt in [0.01, 0.1, 0.37, 1.0, 5.0] {
            // Out from inside, with the inner door open as a cycle in leaves it.
            let mut l = lock();
            assert!(l.toggle_inner());
            assert!(l.press());
            assert!(!l.is_open(Door::Inner), "the open door shuts at once");
            run(&mut l, total() + 0.5, dt);
            assert_eq!(l.cycling(), None, "dt {dt}");
            assert!(l.is_open(Door::Outer) && !l.is_open(Door::Inner));
            assert_eq!(l.side(), Side::Outside);
            // Back in.
            assert!(l.press());
            assert!(!l.is_open(Door::Outer), "the outer door shuts at once");
            run(&mut l, total() + 0.5, dt);
            assert!(l.is_open(Door::Inner) && !l.is_open(Door::Outer));
            assert_eq!(l.side(), Side::Inside);
        }
    }

    #[test]
    fn the_target_door_opens_only_after_the_whole_cycle() {
        let mut l = lock();
        assert!(l.press());
        run(&mut l, total() - 0.1, 0.1);
        assert!(!l.is_open(Door::Outer), "still cycling");
        assert_eq!(l.cycling(), Some(Door::Outer));
        run(&mut l, 0.2, 0.1);
        assert!(l.is_open(Door::Outer), "the cycle is over");
        assert_eq!(l.cycling(), None);
    }

    #[test]
    fn a_press_during_a_cycle_is_ignored() {
        let mut l = lock();
        assert!(l.press());
        run(&mut l, 1.5, 0.1);
        let before = l;
        assert!(!l.press(), "no second press, no cancel");
        assert!(!l.toggle_inner(), "the inner door is locked during the cycle");
        assert!(!l.close(Door::Inner));
        assert_eq!(l, before);
    }

    #[test]
    fn the_inner_door_is_an_ordinary_door_only_while_resting_inside() {
        let mut l = lock();
        assert!(l.toggle_inner() && l.is_open(Door::Inner));
        assert!(l.toggle_inner() && !l.is_open(Door::Inner));
        let mut out = Airlock::outer_open(&AirlockTuning::default());
        assert_eq!(out.side(), Side::Outside);
        assert!(!out.toggle_inner(), "outside the inner door is locked");
        assert!(out.is_open(Door::Outer) && !out.is_open(Door::Inner));
    }

    #[test]
    fn the_captains_close_shuts_an_open_door_and_opens_nothing() {
        let mut l = lock();
        assert!(!l.close(Door::Inner), "shut already: nothing to close");
        assert!(l.toggle_inner());
        assert!(l.close(Door::Inner), "the remote close shuts the open inner door");
        assert!(!l.is_open(Door::Inner) && !l.is_open(Door::Outer), "a close opens no door");
    }

    #[test]
    fn a_cycle_moves_the_pump_share_whatever_the_step() {
        for dt in [0.01, 0.1, 0.37, 1.0, 6.0] {
            let mut l = lock();
            assert!(l.press());
            let moved = run(&mut l, total(), dt);
            assert!((moved - 0.95).abs() < 1e-9, "dt {dt}: moved {moved}");
        }
    }

    #[test]
    fn no_air_moves_while_the_door_closes() {
        let mut l = lock();
        assert!(l.press());
        assert_eq!(l.pump_share(AirlockTuning::default().close_s), 0.0);
        assert_eq!(run(&mut l, AirlockTuning::default().close_s, 0.1), 0.0);
    }

    #[test]
    fn outward_cycles_pump_out_and_inward_cycles_fill() {
        let mut l = lock();
        assert_eq!(l.direction(), None);
        assert!(l.press());
        assert_eq!(l.direction(), Some(Dir::Out), "from inside the cycle pumps the air back");
        run(&mut l, total(), 0.1);
        assert!(l.press());
        assert_eq!(l.direction(), Some(Dir::In), "from outside the cycle fills from the corridor");
    }

    #[test]
    fn a_save_keeps_the_side_and_the_doors_apart() {
        let t = AirlockTuning::default();
        let mut l = lock();
        l.press();
        run(&mut l, 2.0, 0.1);
        let back = Airlock::from_save(&t, &l.to_save());
        assert_eq!(back, l);
        let bad = AirlockSave { inner_open: true, outer_open: true, side: Side::Inside, cycling: None, t: 0.0 };
        let l = Airlock::from_save(&t, &bad);
        assert!(!(l.is_open(Door::Inner) && l.is_open(Door::Outer)));
    }
}
