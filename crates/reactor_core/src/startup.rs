//! The ship's cold start (r42, #268 "Ship on and off as a plain switch"), after PULSAR's six-step
//! startup. Sources: the manual's "Ship Startup Sequence" (`pulsar/assets/textassets/manual_plain.txt`
//! lines 370-381: six steps, the order is enforced, pulling the first lever cuts all power at once);
//! the concept repo's `docs/design/ship.md` row "Cooperative startup" (the crew starts the ship
//! together) and `docs/research/pulsar/pulsar-reference.md` section 1 ("ship startup (six steps, teamwork)").
//! Barotrauma has no ship startup.
//!
//! Our mapping of the six steps (the manual's wording in brackets):
//! 1. MAIN BREAKER on at SP-01 [lever 1, the switchboard's first lever: our breaker is a switch on the panel].
//! 2. BOOT at SP-01, `boot_s` of booting [boot the Ship OS on the Startup Screen].
//! 3. The reactor's POWER on at the readout [lever 2, the middle one: the reactor is the source the
//!    lever feeds, so its switch is the counterpart; TODO(initiator): confirm].
//! 4. ARM the jump drive at JD-01 [prime the warp core on the Startup Screen: PULSAR's warp core is
//!    the jump drive, its prime is the ARM].
//! 5. LIFE SUPPORT and LIGHTS breakers on at the junction box [lever 3, the right one].
//! 6. OVERRIDE at SP-01 [manual override at the main computer screen to allow access to the other systems].
//!
//! The stages are the steps reached: `Breaker` is step 1 done, `Booting` is step 2 running, `Reactor`
//! is step 2 done and waiting for step 3, and so on. The checks of steps 3, 4 and 5 are the room's
//! real state: they move the stage on when they hold, and never before the step before them is done.
//! The master is live from `Reactor` on (the booted OS): the grid and the screens other than SP-01
//! come up at boot, so steps 3 to 5 can be done at their panels. TODO(initiator): should the screens
//! stay dark until the OVERRIDE (the manual's "allow access to other systems")?
//! Turning the master off (step 1 undone) is the manual's lever 1: the whole ship goes cold.
use serde::{Deserialize, Serialize};

/// The stages in order: each is the state after the steps before it (see the module doc).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Cold,
    Breaker,
    Booting,
    Reactor,
    Jump,
    Breakers,
    Ready,
}

/// The room's real state the checks read.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Checks {
    pub reactor_on: bool,
    /// The jump drive is not COLD_START (the ARM was pressed).
    pub jump_armed: bool,
    /// Life Support and Lights are both on at the junction box.
    pub breakers_on: bool,
}

/// `startup` in engineering/engineering.json: how long the OS takes to boot (step 2). TODO(initiator): the value.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct StartupTuning {
    pub boot_s: f64,
}

impl Default for StartupTuning {
    fn default() -> StartupTuning {
        StartupTuning { boot_s: 3.0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Startup {
    stage: Stage,
    boot_s: f64,
    /// Seconds of booting left (0 when not booting).
    boot_left: f64,
}

impl Startup {
    /// A cold ship: nothing done.
    pub fn new(tuning: StartupTuning) -> Startup {
        Startup { stage: Stage::Cold, boot_s: tuning.boot_s, boot_left: 0.0 }
    }

    /// A ship that is already up (the default start, warm and running).
    pub fn ready(tuning: StartupTuning) -> Startup {
        Startup { stage: Stage::Ready, ..Startup::new(tuning) }
    }

    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// The master is live: the grid delivers and the screens other than SP-01 show.
    pub fn live(&self) -> bool {
        self.stage >= Stage::Reactor
    }

    /// Seconds of booting left (0 when the OS is not booting).
    pub fn boot_left(&self) -> f64 {
        self.boot_left
    }

    /// Step 1 on (MAIN BREAKER on: Cold to Breaker), or step 1 off, which cuts everything: any
    /// stage back to Cold. Returns false when the breaker is already in that position.
    pub fn master(&mut self, on: bool) -> bool {
        if on {
            if self.stage != Stage::Cold {
                return false;
            }
            self.stage = Stage::Breaker;
        } else {
            self.reset();
        }
        true
    }

    /// Step 2: BOOT, from Breaker only. Refused otherwise.
    pub fn boot(&mut self) -> bool {
        if self.stage != Stage::Breaker {
            return false;
        }
        self.stage = Stage::Booting;
        self.boot_left = self.boot_s;
        true
    }

    /// Step 6: OVERRIDE, from Breakers only, and only while the breakers are still on. Refused otherwise.
    pub fn override_startup(&mut self, c: Checks) -> bool {
        if self.stage != Stage::Breakers || !c.breakers_on {
            return false;
        }
        self.stage = Stage::Ready;
        self.boot_left = 0.0;
        true
    }

    /// Advances the booting timer and the checks of steps 3 and 4. Each check moves the stage one
    /// step on, and only from the stage that waits for it, so a step cannot be skipped.
    pub fn step(&mut self, dt: f64, c: Checks) {
        if self.stage == Stage::Booting {
            self.boot_left = (self.boot_left - dt).max(0.0);
            if self.boot_left <= 0.0 {
                self.stage = Stage::Reactor;
            }
        }
        if self.stage == Stage::Reactor && c.reactor_on {
            self.stage = Stage::Jump;
        }
        if self.stage == Stage::Jump && c.jump_armed {
            self.stage = Stage::Breakers;
        }
    }

    /// The checklist's ticks, steps 1 to 6 (step 5 is ticked by the live breakers, so it un-ticks
    /// when they go off; the stage keeps the earlier steps).
    pub fn ticks(&self, c: Checks) -> [bool; 6] {
        [
            self.stage >= Stage::Breaker,
            self.stage >= Stage::Reactor,
            self.stage >= Stage::Jump,
            self.stage >= Stage::Breakers,
            self.stage >= Stage::Breakers && c.breakers_on,
            self.stage == Stage::Ready,
        ]
    }

    /// COLD SHIP (and the master off): the whole ship back to Cold.
    pub fn reset(&mut self) {
        self.stage = Stage::Cold;
        self.boot_left = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_room::{EngineRoom, Input};
    use crate::grid::Group;
    use crate::tuning::Tuning;

    const ALL_ON: Checks = Checks { reactor_on: true, jump_armed: true, breakers_on: true };
    const NONE: Checks = Checks { reactor_on: false, jump_armed: false, breakers_on: false };

    fn stage_of(s: &Startup) -> u8 {
        s.stage() as u8
    }

    #[test]
    fn steps_only_go_forward_in_order() {
        let mut s = Startup::new(StartupTuning::default());
        let mut last = stage_of(&s);
        // Every press and tick, in the shipped order and then some: the stage never falls back
        // except through the master off.
        assert!(s.master(true));
        assert!(s.boot());
        for _ in 0..40 {
            s.step(0.1, NONE);
            assert!(stage_of(&s) >= last, "went back");
            last = stage_of(&s);
        }
        assert_eq!(s.stage(), Stage::Reactor);
        s.step(0.1, ALL_ON);
        assert_eq!(s.stage(), Stage::Breakers, "two checks in one step, in order");
        assert!(s.override_startup(ALL_ON));
        assert_eq!(s.stage(), Stage::Ready);
        assert!(!s.master(true), "already on");
        assert_eq!(s.stage(), Stage::Ready);
    }

    #[test]
    fn a_skipped_step_blocks_the_next_one() {
        // Boot before the breaker is refused.
        let mut s = Startup::new(StartupTuning::default());
        assert!(!s.boot());
        assert_eq!(s.stage(), Stage::Cold);
        // Override before the boot, with every check true, is refused.
        assert!(s.master(true));
        assert!(!s.override_startup(ALL_ON));
        // A check that holds early does not skip the boot: the stage waits at Breaker.
        let mut t = Startup::new(StartupTuning::default());
        t.master(true);
        t.step(10.0, ALL_ON);
        assert_eq!(t.stage(), Stage::Breaker, "the reactor and the jump do not stand in for the boot");
        // The reactor on while booting does not skip the boot's timer.
        t.boot();
        t.step(1.0, ALL_ON);
        assert_eq!(t.stage(), Stage::Booting);
        // The jump armed before the reactor on: the stage waits at Reactor (step 3 first).
        let mut u = Startup::new(StartupTuning::default());
        u.master(true);
        u.boot();
        u.step(3.0, Checks { reactor_on: false, jump_armed: true, breakers_on: true });
        assert_eq!(u.stage(), Stage::Reactor, "the jump does not stand in for the reactor");
        u.step(0.0, Checks { reactor_on: true, jump_armed: false, breakers_on: false });
        assert_eq!(u.stage(), Stage::Jump);
        // Override with the breakers off is refused, the stage stays at Breakers.
        u.step(0.0, Checks { reactor_on: true, jump_armed: true, breakers_on: false });
        assert_eq!(u.stage(), Stage::Breakers);
        assert!(!u.override_startup(Checks { breakers_on: false, ..ALL_ON }));
        assert!(u.override_startup(ALL_ON));
    }

    #[test]
    fn cold_ship_resets_everything_to_cold() {
        let mut s = Startup::ready(StartupTuning::default());
        assert!(s.live());
        s.reset();
        assert_eq!(s.stage(), Stage::Cold);
        assert!(!s.live());
        assert_eq!(s.boot_left(), 0.0);
        // Mid-boot too: the timer is gone with the stage.
        s.master(true);
        s.boot();
        s.step(1.0, NONE);
        s.master(false);
        assert_eq!(s.stage(), Stage::Cold);
        assert_eq!(s.boot_left(), 0.0);
        assert!(!s.boot(), "the boot needs the breaker again");
    }

    #[test]
    fn the_checklist_ticks_follow_the_stage_and_the_live_breakers() {
        let mut s = Startup::new(StartupTuning::default());
        assert_eq!(s.ticks(ALL_ON), [false; 6]);
        s.master(true);
        assert_eq!(s.ticks(NONE), [true, false, false, false, false, false]);
        s.boot();
        s.step(3.0, NONE);
        s.step(0.0, Checks { reactor_on: true, jump_armed: true, breakers_on: false });
        assert_eq!(s.ticks(Checks { breakers_on: false, ..ALL_ON }), [true, true, true, true, false, false]);
        assert_eq!(s.ticks(ALL_ON), [true, true, true, true, true, false]);
        s.override_startup(ALL_ON);
        assert_eq!(s.ticks(ALL_ON), [true; 6]);
    }

    #[test]
    fn master_off_makes_the_grid_deliver_nothing_and_cuts_the_battery() {
        let tuning = Tuning::default();
        let mut room = EngineRoom::from_tuning(&tuning, 7);
        assert!(room.master_on(), "the default start is warm and running");
        room.cold_ship();
        assert!(!room.master_on());
        // Breakers on and the reactor on: only the master keeps the grid from delivering.
        for g in Group::ALL {
            room.set_breaker(g, true);
        }
        room.set_power(true);
        room.battery_mut().set_charge_kwmin(500.0);
        room.battery_mut().set_recharge(0.0);
        for _ in 0..50 {
            room.step(0.1, Input::auto());
        }
        for g in Group::ALL {
            assert_eq!(room.grid().supply(g).delivered_kw, 0.0, "{g:?} got power with the master off");
        }
        assert_eq!(room.battery().charge_kwmin(), 500.0, "the battery is cut off with the master");
        assert_eq!(room.battery().output_kw(), 0.0);
    }

    #[test]
    fn the_room_runs_the_six_steps_in_order() {
        let mut room = EngineRoom::from_tuning(&Tuning::default(), 3);
        room.cold_ship();
        assert!(room.master_breaker(true), "step 1");
        assert!(!room.master_on());
        assert!(room.startup_boot(), "step 2");
        for _ in 0..40 {
            room.step(0.1, Input::auto());
        }
        assert_eq!(room.startup().stage(), Stage::Reactor, "booted, the reactor is still off");
        assert!(room.master_on(), "the OS is up: the screens and the grid are live");
        assert!(!room.startup_override(), "no override before the steps");
        room.set_power(true);
        room.step(0.1, Input::auto());
        assert_eq!(room.startup().stage(), Stage::Jump, "step 3 checked on the reactor");
        assert!(room.jump_mut().arm());
        room.step(0.1, Input::auto());
        assert_eq!(room.startup().stage(), Stage::Breakers, "step 4 checked on the jump drive");
        assert!(!room.startup_override(), "the breakers are still off");
        room.set_breaker(Group::LifeSupport, true);
        room.set_breaker(Group::Lights, true);
        assert!(room.startup_override(), "step 6, with step 5 on");
        assert_eq!(room.startup().stage(), Stage::Ready);
    }

    #[test]
    fn cold_ship_on_the_room_turns_the_reactor_and_the_breakers_off() {
        let mut room = EngineRoom::from_tuning(&Tuning::default(), 5);
        assert!(room.set_power(true), "a fresh room's reactor starts off: the power is on first");
        assert!(room.reactor().power_on());
        room.jump_mut().arm();
        room.cold_ship();
        assert_eq!(room.startup().stage(), Stage::Cold);
        assert!(!room.reactor().power_on());
        assert_eq!(room.jump().stage, crate::jump::JumpStage::ColdStart);
        for g in Group::ALL {
            assert!(!room.grid().breaker(g), "{g:?} breaker still on");
        }
    }
}
