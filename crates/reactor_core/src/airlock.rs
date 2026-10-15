//! The airlock (r40, #267): an inner and an outer door that are never open together, and a pressure
//! cycle in between that pumps the air back into the corridor instead of losing it. The glue
//! (`exo_app` dev_area airlock) moves the air and shows the doors; this file is the state machine.
//!
//! Sources:
//! - Barotrauma's airlock is a hull flagged as a wet room (`Hull.IsWetRoom`, read by
//!   `IndoorsSteeringManager.cs:776` and `HumanAIController.cs:633`) between two doors. Its content is
//!   `content/Items/Assemblies/AutomaticHorizontalAirlock.xml`: "When one door is toggled, the other is
//!   automatically closed", with a pump in the room (`smallpump`). We refuse instead of closing the other
//!   door for the player: the player closes it first, the panel says so.
//! - The door's open state is `Door.cs:176-186` (`IsOpen` sets `OpenState` to 1 or 0; the slide follows
//!   it). The stuck and weld fields (`Door.cs:116-128`) are not here yet: no weld in 0.1.
//! - Barotrauma's pump moves water (`Pump.cs`), so the cycle is ours. PULSAR has a slot type AIRLOCK
//!   (`docs/research/pulsar-reference.md:77`, slot types), which is the same job for the ship.
//! - Values: the cycle length and the pumped share are ours, `TODO(initiator)`: 6 s and 95 %. The
//!   5 % that stay in the room leave with the outer door (into space, when the outside is vacuum).
use serde::{Deserialize, Serialize};

/// `airlock` in reactor.json (r40). Every value is a start value, `TODO(initiator)`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AirlockTuning {
    #[serde(default, rename = "_comment")]
    pub comment: Option<String>,
    /// The outside has no air: the outer door opens onto vacuum (true), or onto the breathable outside (false).
    pub outside_vacuum: bool,
    /// Seconds of one cycle (`TODO(initiator)`: 6 s).
    pub cycle_s: f64,
    /// The share of the room's air the cycle moves over (0..1, `TODO(initiator)`: 95 %).
    pub pump_share: f64,
}

impl Default for AirlockTuning {
    fn default() -> AirlockTuning {
        AirlockTuning { comment: None, outside_vacuum: true, cycle_s: 6.0, pump_share: 0.95 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Door {
    Inner,
    Outer,
}

/// What a cycle is for: the door it opens at its end, or nothing (`Hold`, the pump alone).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    Door(Door),
    Hold,
}

/// Which way the cycle's air moves: `Out` from the room to the corridor (the pump back), `In` from the
/// corridor into the room (the fill before the inner door opens).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Out,
    In,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Airlock {
    cycle_s: f64,
    pump_share: f64,
    inner_open: bool,
    outer_open: bool,
    /// The cycle running, and what it is for; None when the room is idle.
    cycling: Option<Goal>,
    /// Seconds into the cycle.
    t: f64,
}

impl Airlock {
    pub fn new(t: &AirlockTuning) -> Airlock {
        Airlock { cycle_s: t.cycle_s, pump_share: t.pump_share, inner_open: false, outer_open: false, cycling: None, t: 0.0 }
    }

    pub fn is_open(&self, d: Door) -> bool {
        match d {
            Door::Inner => self.inner_open,
            Door::Outer => self.outer_open,
        }
    }

    pub fn cycling(&self) -> Option<Goal> {
        self.cycling
    }

    /// The air's direction of the running cycle; None when idle.
    pub fn direction(&self) -> Option<Dir> {
        match self.cycling? {
            Goal::Door(Door::Inner) => Some(Dir::In),
            Goal::Door(Door::Outer) | Goal::Hold => Some(Dir::Out),
        }
    }

    /// The cycle's progress, 0..1 (0 when idle).
    pub fn progress(&self) -> f64 {
        if self.cycling.is_some() { (self.t / self.cycle_s).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// A press from a panel: starts the goal's cycle, or stops it when it runs; closes the goal's door
    /// when that door is open. Refused (false) while the other door is open, while another cycle runs,
    /// or for a `Hold` while a door is open. The doors never open together: a door opens only at the
    /// end of its own cycle, and only when the other door is shut.
    pub fn press(&mut self, goal: Goal) -> bool {
        if let Some(running) = self.cycling {
            if running == goal {
                self.cancel();
                return true;
            }
            return false;
        }
        match goal {
            Goal::Door(d) if self.is_open(d) => {
                self.set_open(d, false);
                true
            }
            Goal::Door(d) => {
                if self.is_open(other(d)) {
                    return false;
                }
                self.cycling = Some(goal);
                self.t = 0.0;
                true
            }
            Goal::Hold => {
                if self.inner_open || self.outer_open {
                    return false;
                }
                self.cycling = Some(goal);
                self.t = 0.0;
                true
            }
        }
    }

    /// Stops a running cycle. Its door stays shut and the air it moved stays where it went.
    pub fn cancel(&mut self) {
        self.cycling = None;
        self.t = 0.0;
    }

    /// Runs the cycle's timer; at its end the goal's door opens.
    pub fn step(&mut self, dt: f64) {
        let Some(goal) = self.cycling else { return };
        self.t += dt;
        if self.t + TIME_EPS >= self.cycle_s {
            self.cycling = None;
            self.t = 0.0;
            if let Goal::Door(d) = goal {
                self.set_open(d, true);
            }
        }
    }

    /// The share of the cycle's air that moves in the next `dt` seconds (0..`pump_share` over the whole
    /// cycle, linear in time). Call it before `step(dt)`. The glue multiplies it by the air at the start.
    pub fn pump_share(&self, dt: f64) -> f64 {
        if self.cycling.is_none() || self.t + TIME_EPS >= self.cycle_s {
            return 0.0;
        }
        let advance = (self.t + dt).min(self.cycle_s) - self.t;
        self.pump_share * advance.max(0.0) / self.cycle_s
    }

    fn set_open(&mut self, d: Door, open: bool) {
        match d {
            Door::Inner => self.inner_open = open,
            Door::Outer => self.outer_open = open,
        }
    }
}

/// Float slack on the cycle's clock: steps of 0.1 s add up to 5.999... for a 6 s cycle.
const TIME_EPS: f64 = 1e-9;

fn other(d: Door) -> Door {
    match d {
        Door::Inner => Door::Outer,
        Door::Outer => Door::Inner,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock() -> Airlock {
        Airlock::new(&AirlockTuning::default())
    }

    /// Runs `secs` in steps of `dt` (the cycle is over when it returns).
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
        // A fixed pseudo-random run of presses and time: the invariant holds at every step.
        let mut l = lock();
        let mut seed: u64 = 0x2d40;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };
        for _ in 0..20_000 {
            let goal = match next() % 3 {
                0 => Goal::Door(Door::Inner),
                1 => Goal::Door(Door::Outer),
                _ => Goal::Hold,
            };
            let _ = l.press(goal);
            let dt = [0.05, 0.5, 1.0, 3.0][next() % 4];
            l.step(dt);
            assert!(!(l.is_open(Door::Inner) && l.is_open(Door::Outer)), "both doors open");
        }
    }

    #[test]
    fn the_outer_door_opens_only_after_the_cycle() {
        let mut l = lock();
        assert!(l.press(Goal::Door(Door::Outer)));
        run(&mut l, 5.9, 0.1);
        assert!(!l.is_open(Door::Outer), "still cycling");
        assert_eq!(l.cycling(), Some(Goal::Door(Door::Outer)));
        run(&mut l, 0.2, 0.1);
        assert!(l.is_open(Door::Outer), "the cycle is over");
        assert_eq!(l.cycling(), None);
    }

    #[test]
    fn a_door_opens_only_when_the_other_is_shut() {
        let mut l = lock();
        assert!(l.press(Goal::Door(Door::Inner)));
        run(&mut l, 6.0, 0.5);
        assert!(l.is_open(Door::Inner));
        assert!(!l.press(Goal::Door(Door::Outer)), "the inner door is open: refused");
        assert!(!l.press(Goal::Hold), "a door is open: no pump");
        assert!(l.press(Goal::Door(Door::Inner)), "a press closes the open door");
        assert!(!l.is_open(Door::Inner));
        assert!(l.press(Goal::Door(Door::Outer)));
        assert_eq!(l.cycling(), Some(Goal::Door(Door::Outer)));
    }

    #[test]
    fn an_abort_mid_cycle_is_safe() {
        let mut l = lock();
        assert!(l.press(Goal::Door(Door::Outer)));
        run(&mut l, 3.0, 0.1);
        assert!(l.press(Goal::Door(Door::Outer)), "the same press stops the cycle");
        assert_eq!(l.cycling(), None);
        assert!(!l.is_open(Door::Outer) && !l.is_open(Door::Inner), "both shut after an abort");
        assert_eq!(l.progress(), 0.0);
        // The next cycle starts from the top, and the other door is free to go.
        assert!(l.press(Goal::Door(Door::Inner)));
        let moved = run(&mut l, 6.0, 0.1);
        assert!((moved - 0.95).abs() < 1e-9, "a new cycle moves its full share ({moved})");
        assert!(l.is_open(Door::Inner) && !l.is_open(Door::Outer));
    }

    #[test]
    fn a_cycle_moves_the_pump_share_whatever_the_step() {
        for dt in [0.01, 0.1, 0.37, 1.0, 6.0] {
            let mut l = lock();
            assert!(l.press(Goal::Door(Door::Outer)));
            let moved = run(&mut l, 6.0, dt);
            assert!((moved - 0.95).abs() < 1e-9, "dt {dt}: moved {moved}");
        }
    }

    #[test]
    fn outward_cycles_pump_out_and_inward_cycles_fill() {
        let mut l = lock();
        assert_eq!(l.direction(), None);
        assert!(l.press(Goal::Door(Door::Outer)));
        assert_eq!(l.direction(), Some(Dir::Out));
        l.cancel();
        assert!(l.press(Goal::Hold));
        assert_eq!(l.direction(), Some(Dir::Out), "the pump alone pumps out");
        l.cancel();
        assert!(l.press(Goal::Door(Door::Inner)));
        assert_eq!(l.direction(), Some(Dir::In), "the inner door's cycle fills from the corridor");
    }

    #[test]
    fn a_hold_cycle_pumps_and_opens_no_door() {
        let mut l = lock();
        assert!(l.press(Goal::Hold));
        let moved = run(&mut l, 6.0, 0.25);
        assert!((moved - 0.95).abs() < 1e-9);
        assert!(!l.is_open(Door::Inner) && !l.is_open(Door::Outer));
        assert_eq!(l.cycling(), None);
    }
}
