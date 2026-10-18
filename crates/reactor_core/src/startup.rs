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

/// `startup` in engineering/engineering.json: how long the OS takes to boot (step 2), and the
/// reactor's start-up stand (#319). TODO(initiator): every value.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct StartupTuning {
    pub boot_s: f64,
    /// PRIME: seconds the magnetic trap takes to charge from the battery.
    pub prime_s: f64,
    /// PRIME: kW the trap draws from the battery while it charges.
    pub prime_kw: f64,
    /// Orderly shutdown: seconds the reactor runs down (injection stops, output falls) before it is off.
    pub shutdown_s: f64,
    /// RUN-UP: factor on Auto's speed while the reactor runs up to demand (1 = the reactor's own).
    pub run_up_boost: f64,
}

impl Default for StartupTuning {
    fn default() -> StartupTuning {
        StartupTuning { boot_s: 3.0, prime_s: 10.0, prime_kw: 400.0, shutdown_s: 5.0, run_up_boost: 8.0 }
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

// ---------------------------------------------------------------------------------------------
// The reactor's start-up stand (#319): three boot toggles, the rotary START lever, the magnetic trap.

/// The lever's detents: OFF, PRIME, RUN (rotary: RUN goes round to OFF).
pub const LEVER_OFF: u8 = 0;
pub const LEVER_PRIME: u8 = 1;
pub const LEVER_RUN: u8 = 2;

/// The stages of the reactor's start, one procedure for the cold start and after a SCRAM. Cold: the screens are dark
/// (the three toggles were never all on). Boot: awake on battery, waiting for the conditions and PRIME. Prime: the trap
/// charges (and waits charged for RUN). RunUp: ignited, output climbing to demand. Running. ShutDown: the orderly OFF,
/// the reactor runs down for `shutdown_s`, then Boot. (Reuses the pattern of the old SP-01 stages above.)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StartStage {
    Cold,
    Boot,
    Prime,
    RunUp,
    Running,
    ShutDown,
}

/// What the stand shows the sim: the three boot toggles and the lever detent (the host reads them off the kit's `Controls`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Stand {
    pub toggles: [bool; 3],
    pub lever: u8,
}

impl Stand {
    /// A warm, running reactor: toggles on, lever at RUN (the dev area's start, and a room nobody drives).
    pub fn warm() -> Stand {
        Stand { toggles: [true; 3], lever: LEVER_RUN }
    }

    pub fn cold() -> Stand {
        Stand { toggles: [false; 3], lever: LEVER_OFF }
    }

    pub fn toggles_on(&self) -> bool {
        self.toggles.iter().all(|t| *t)
    }
}

impl Default for Stand {
    fn default() -> Stand {
        Stand::warm()
    }
}

/// What PRIME needs from the room: the fill of the best canister (0..100) and the battery's charge (kW min).
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Supplies {
    pub fill: f64,
    pub battery_kwmin: f64,
}

/// What a step asks of the room.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct StartOut {
    /// RUN just ignited the core: power on.
    pub ignite: bool,
    /// The orderly shutdown has run down: power off.
    pub power_off: bool,
    /// kW min the trap took from the battery in this step.
    pub draw_kwmin: f64,
}

/// The BOOT screen's rows, true = met.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct StartChecks {
    pub battery_bus: bool,
    pub trap: bool,
    pub injector: bool,
    pub canister: bool,
    pub battery: bool,
    pub lever_off: bool,
}

impl StartChecks {
    pub fn open(&self) -> usize {
        [self.battery_bus, self.trap, self.injector, self.canister, self.battery, self.lever_off].iter().filter(|m| !**m).count()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReactorStart {
    stage: StartStage,
    tuning: StartupTuning,
    /// The magnetic trap's charge, 0..1.
    trap: f64,
    /// Seconds of the orderly shutdown left.
    winding: f64,
}

fn fin(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

impl ReactorStart {
    /// A cold ship: dark, the toggles never on.
    pub fn cold(tuning: StartupTuning) -> ReactorStart {
        ReactorStart { stage: StartStage::Cold, tuning, trap: 0.0, winding: 0.0 }
    }

    /// A warm, running reactor (the default start).
    pub fn warm(tuning: StartupTuning) -> ReactorStart {
        ReactorStart { stage: StartStage::Running, trap: 1.0, ..ReactorStart::cold(tuning) }
    }

    pub fn stage(&self) -> StartStage {
        self.stage
    }

    /// The trap's charge, 0..1.
    pub fn trap(&self) -> f64 {
        self.trap
    }

    pub fn trap_charged(&self) -> bool {
        self.trap >= 1.0
    }

    /// Seconds of PRIME left (0 when not charging).
    pub fn prime_left_s(&self) -> f64 {
        if self.stage == StartStage::Prime { (1.0 - self.trap).max(0.0) * self.tuning.prime_s.max(0.0) } else { 0.0 }
    }

    /// The screens are awake (not the cold start before the toggles).
    pub fn awake(&self) -> bool {
        self.stage != StartStage::Cold
    }

    /// The reactor is in its run-up (the screen's RUN-UP).
    pub fn run_up(&self) -> bool {
        self.stage == StartStage::RunUp
    }

    pub fn run_up_boost(&self) -> f64 {
        self.tuning.run_up_boost
    }

    /// kW min of battery charge PRIME needs: the trap's whole draw.
    pub fn prime_need_kwmin(&self) -> f64 {
        self.tuning.prime_s.max(0.0) * self.tuning.prime_kw.max(0.0) / 60.0
    }

    pub fn checks(&self, stand: &Stand, s: &Supplies) -> StartChecks {
        StartChecks {
            battery_bus: stand.toggles[0],
            trap: stand.toggles[1],
            injector: stand.toggles[2],
            canister: fin(s.fill) > 0.0,
            battery: fin(s.battery_kwmin) >= self.prime_need_kwmin(),
            lever_off: stand.lever == LEVER_OFF,
        }
    }

    /// PRIME may latch: Boot, all three toggles, a canister with fill, battery for the whole trap.
    fn prime_ok(&self, stand: &Stand, s: &Supplies) -> bool {
        let c = self.checks(stand, s);
        self.stage == StartStage::Boot && c.battery_bus && c.trap && c.injector && c.canister && c.battery
    }

    /// May the START lever go to detent `target`? The kit's `refused` set is its negation. OFF always; PRIME only when
    /// `prime_ok`; RUN only with the trap charged (stage Prime).
    pub fn allows(&self, target: u8, stand: &Stand, s: &Supplies) -> bool {
        match target {
            LEVER_OFF => true,
            LEVER_PRIME => self.prime_ok(stand, s),
            LEVER_RUN => self.stage == StartStage::Prime && self.trap_charged(),
            _ => false,
        }
    }

    /// SCRAM or core eject: back to Boot (awake on battery), the trap discharged. A cold ship stays cold.
    pub fn emergency(&mut self) {
        if self.stage != StartStage::Cold {
            self.stage = StartStage::Boot;
            self.trap = 0.0;
            self.winding = 0.0;
        }
    }

    /// The ignition did not take (the reactor is broken): back to Boot.
    pub fn fail(&mut self) {
        self.emergency();
    }

    /// The dev menu's cold start: dark, trap empty.
    pub fn go_cold(&mut self) {
        self.stage = StartStage::Cold;
        self.trap = 0.0;
        self.winding = 0.0;
    }

    /// The dev area's warm start: running, trap charged.
    pub fn go_warm(&mut self) {
        self.stage = StartStage::Running;
        self.trap = 1.0;
        self.winding = 0.0;
    }

    /// Advances `dt` seconds on the stand and the supplies. `output_reached`: the reactor's output has reached the demand.
    pub fn step(&mut self, dt: f64, stand: &Stand, s: &Supplies, output_reached: bool) -> StartOut {
        let dt = fin(dt).max(0.0);
        let mut out = StartOut::default();
        match self.stage {
            StartStage::Cold => {
                if stand.toggles_on() {
                    self.stage = StartStage::Boot;
                }
            }
            StartStage::Boot => {
                if stand.lever == LEVER_PRIME && self.prime_ok(stand, s) {
                    self.stage = StartStage::Prime;
                    self.trap = 0.0;
                }
            }
            StartStage::Prime => {
                let lost = !stand.toggles_on() || fin(s.fill) <= 0.0 || fin(s.battery_kwmin) <= 0.0 || stand.lever == LEVER_OFF;
                if lost {
                    self.stage = StartStage::Boot;
                    self.trap = 0.0;
                } else {
                    if self.trap < 1.0 {
                        let prime_s = self.tuning.prime_s;
                        let want_kwmin = self.tuning.prime_kw.max(0.0) / 60.0 * dt;
                        let got = want_kwmin.min(fin(s.battery_kwmin));
                        let share = if want_kwmin > 0.0 { got / want_kwmin } else { 1.0 };
                        let step = if prime_s > 0.0 { dt / prime_s * share } else { 1.0 };
                        self.trap = (self.trap + step).min(1.0);
                        out.draw_kwmin = got;
                    }
                    if self.trap_charged() && stand.lever == LEVER_RUN {
                        self.stage = StartStage::RunUp;
                        out.ignite = true;
                    }
                }
            }
            StartStage::RunUp | StartStage::Running => {
                if stand.lever == LEVER_OFF {
                    self.stage = StartStage::ShutDown;
                    self.winding = self.tuning.shutdown_s.max(0.0);
                } else if self.stage == StartStage::RunUp && output_reached {
                    self.stage = StartStage::Running;
                }
            }
            StartStage::ShutDown => {
                self.winding -= dt;
                if self.winding <= 0.0 {
                    self.stage = StartStage::Boot;
                    self.trap = 0.0;
                    self.winding = 0.0;
                    out.power_off = true;
                }
            }
        }
        out
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

#[cfg(test)]
mod stand_tests {
    use super::*;
    use crate::engine_room::{EngineRoom, Input};
    use crate::tuning::Tuning;

    fn cold_room() -> EngineRoom {
        let mut room = EngineRoom::from_tuning(&Tuning::default(), 11);
        room.set_wear(false);
        room.cold_reactor();
        let cap = room.battery().tuning().capacity_kwmin;
        room.battery_mut().set_charge_kwmin(cap);
        room
    }

    fn run(room: &mut EngineRoom, secs: f64) {
        for _ in 0..(secs * 10.0) as usize {
            room.step(0.1, Input::auto());
        }
    }

    fn stand(room: &mut EngineRoom, toggles: [bool; 3], lever: u8) {
        room.set_stand(Stand { toggles, lever });
    }

    fn assert_finite(room: &EngineRoom) {
        let r = room.reactor();
        for (n, v) in [("heat", r.heat()), ("output", r.output_kw()), ("injection", r.injection()), ("converter", r.converter()), ("trap", room.start().trap()), ("battery", room.battery().charge_kwmin())] {
            assert!(v.is_finite(), "{n} is {v}");
        }
    }

    /// Cold start through to RUN; returns the room in RunUp.
    fn to_run_up(room: &mut EngineRoom) {
        stand(room, [true; 3], LEVER_OFF);
        run(room, 0.5);
        assert_eq!(room.start().stage(), StartStage::Boot);
        stand(room, [true; 3], LEVER_PRIME);
        run(room, 0.5);
        assert_eq!(room.start().stage(), StartStage::Prime);
        run(room, 11.0);
        assert!(room.start().trap_charged());
        stand(room, [true; 3], LEVER_RUN);
        run(room, 0.5);
        assert_eq!(room.start().stage(), StartStage::RunUp);
    }

    #[test]
    fn the_screens_wake_only_with_all_three_toggles() {
        let mut room = cold_room();
        assert!(!room.master_on());
        for off in 0..3 {
            let mut t = [true; 3];
            t[off] = false;
            stand(&mut room, t, LEVER_OFF);
            run(&mut room, 1.0);
            assert_eq!(room.start().stage(), StartStage::Cold, "toggle {off} off: still dark");
            assert!(!room.master_on());
        }
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 0.2);
        assert_eq!(room.start().stage(), StartStage::Boot);
        assert!(room.master_on(), "awake on battery");
        // A toggle falling later does not put the screens to sleep.
        stand(&mut room, [false, true, true], LEVER_OFF);
        run(&mut room, 1.0);
        assert_eq!(room.start().stage(), StartStage::Boot);
    }

    #[test]
    fn prime_never_latches_without_toggles_fill_and_charge() {
        let base = |room: &mut EngineRoom| {
            stand(room, [true; 3], LEVER_OFF);
            run(room, 0.3);
        };
        // A toggle off.
        for off in 0..3 {
            let mut room = cold_room();
            base(&mut room);
            let mut t = [true; 3];
            t[off] = false;
            stand(&mut room, t, LEVER_OFF);
            assert!(!room.lever_allows(LEVER_PRIME), "toggle {off}");
            stand(&mut room, t, LEVER_PRIME);
            run(&mut room, 2.0);
            assert_eq!(room.start().stage(), StartStage::Boot, "toggle {off}: PRIME did not latch");
        }
        // No canister.
        let mut room = cold_room();
        base(&mut room);
        while room.reactor_mut().remove_canister(0).is_some() {}
        assert!(!room.lever_allows(LEVER_PRIME));
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 2.0);
        assert_eq!(room.start().stage(), StartStage::Boot, "no canister");
        assert_finite(&room);
        // A spent canister (no fill) is no canister.
        let mut room = cold_room();
        base(&mut room);
        room.reactor_mut().set_canister_lives(&[0.0, 0.0]);
        assert!(!room.lever_allows(LEVER_PRIME), "spent");
        // Too little battery.
        let mut room = cold_room();
        base(&mut room);
        let need = room.start().prime_need_kwmin();
        room.battery_mut().set_charge_kwmin(need * 0.9);
        assert!(!room.lever_allows(LEVER_PRIME), "short of charge");
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 2.0);
        assert_eq!(room.start().stage(), StartStage::Boot, "short of charge");
        // All there.
        let mut room = cold_room();
        base(&mut room);
        assert!(room.lever_allows(LEVER_PRIME));
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 0.5);
        assert_eq!(room.start().stage(), StartStage::Prime);
    }

    #[test]
    fn run_never_ignites_before_prime_has_finished() {
        let mut room = cold_room();
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 0.3);
        // RUN straight from Boot: refused, and the stage machine ignores it.
        assert!(!room.lever_allows(LEVER_RUN));
        stand(&mut room, [true; 3], LEVER_RUN);
        run(&mut room, 2.0);
        assert_eq!(room.start().stage(), StartStage::Boot);
        assert!(!room.reactor().power_on());
        // RUN while PRIME still charges: not allowed, no ignition, and PRIME goes on.
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 3.0);
        assert!(!room.lever_allows(LEVER_RUN));
        stand(&mut room, [true; 3], LEVER_RUN);
        run(&mut room, 3.0);
        assert!(!room.reactor().power_on(), "no ignition while the trap charges");
        assert_eq!(room.start().stage(), StartStage::Prime);
        run(&mut room, 6.0);
        assert!(room.start().trap_charged());
        assert_eq!(room.start().stage(), StartStage::RunUp, "charged with the lever at RUN: ignition");
        assert!(room.reactor().power_on());
    }

    #[test]
    fn prime_draws_the_trap_from_the_battery_and_a_lost_condition_returns_to_boot() {
        let mut room = cold_room();
        let full = room.battery().charge_kwmin();
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 0.3);
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 5.5);
        let t = room.start().trap();
        assert!((0.4..0.65).contains(&t), "trap {t}");
        assert!(room.start().prime_left_s() > 3.0);
        assert!(room.battery().charge_kwmin() < full, "the battery pays");
        // The lever back to OFF during PRIME: Boot, the trap is discharged.
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 0.3);
        assert_eq!(room.start().stage(), StartStage::Boot);
        assert_eq!(room.start().trap(), 0.0);
        // A toggle falling during PRIME does the same.
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 2.0);
        assert_eq!(room.start().stage(), StartStage::Prime);
        stand(&mut room, [true, false, true], LEVER_PRIME);
        run(&mut room, 0.3);
        assert_eq!(room.start().stage(), StartStage::Boot);
    }

    #[test]
    fn no_nan_with_zero_canisters_or_an_empty_battery_during_prime() {
        let mut room = cold_room();
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 0.3);
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 1.0);
        assert_eq!(room.start().stage(), StartStage::Prime);
        while room.reactor_mut().remove_canister(0).is_some() {}
        run(&mut room, 2.0);
        assert_eq!(room.start().stage(), StartStage::Boot, "the canisters left");
        assert_finite(&room);
        // Battery empties in the middle.
        let mut room = cold_room();
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 0.3);
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 1.0);
        room.battery_mut().set_charge_kwmin(0.0);
        run(&mut room, 2.0);
        assert_eq!(room.start().stage(), StartStage::Boot, "the battery ran out");
        assert_finite(&room);
        // A zero-length trap and a zero draw are no division by zero.
        let mut t = Tuning::default();
        t.startup.prime_s = 0.0;
        t.startup.prime_kw = 0.0;
        let mut room = EngineRoom::from_tuning(&t, 3);
        room.cold_reactor();
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 0.3);
        room.battery_mut().set_charge_kwmin(1.0);
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 0.5);
        assert_finite(&room);
    }

    #[test]
    fn run_up_reaches_the_demand_and_a_manual_input_is_overridden() {
        let mut room = cold_room();
        to_run_up(&mut room);
        let mut secs: f64 = 0.0;
        while room.start().stage() == StartStage::RunUp && secs < 400.0 {
            // The screen asks for MANUAL at 0: the run-up is Auto's anyway.
            room.step(0.1, Input::manual(0.0, 0.0));
            secs += 0.1;
        }
        assert_eq!(room.start().stage(), StartStage::Running, "run-up ended after {secs:.0} s");
        assert!(room.reactor().output_kw() >= 0.8 * room.main_load_kw(), "output {}", room.reactor().output_kw());
        assert_finite(&room);
        assert!(secs < 60.0, "run-up took {secs:.1} s");
    }

    #[test]
    fn off_from_any_state_ends_without_nan() {
        for stage in 0..5 {
            let mut room = cold_room();
            stand(&mut room, [true; 3], LEVER_OFF);
            run(&mut room, 0.3);
            if stage >= 1 {
                stand(&mut room, [true; 3], LEVER_PRIME);
                run(&mut room, 2.0);
            }
            if stage >= 2 {
                run(&mut room, 9.0);
                stand(&mut room, [true; 3], LEVER_RUN);
                run(&mut room, 0.5);
            }
            if stage >= 3 {
                run(&mut room, 60.0);
            }
            if stage >= 4 {
                while room.start().stage() == StartStage::RunUp {
                    run(&mut room, 1.0);
                }
            }
            stand(&mut room, [true; 3], LEVER_OFF);
            run(&mut room, 30.0);
            assert_eq!(room.start().stage(), StartStage::Boot, "from case {stage}");
            assert!(!room.reactor().power_on(), "case {stage}: the reactor is off");
            assert_eq!(room.start().trap(), 0.0, "case {stage}: the trap is discharged");
            assert_eq!(room.stand().toggles, [true; 3], "an orderly OFF leaves the toggles on");
            assert!(room.master_on(), "the screens stay awake");
            assert_finite(&room);
        }
    }

    #[test]
    fn an_orderly_off_runs_the_output_down_and_the_battery_takes_over() {
        let mut room = cold_room();
        to_run_up(&mut room);
        while room.start().stage() == StartStage::RunUp {
            run(&mut room, 1.0);
        }
        // A cold chamber swings for a while after the run-up; wait for it to settle on the demand.
        let mut waited = 0;
        while room.reactor().output_kw() < 0.95 * room.main_load_kw() && waited < 240 {
            run(&mut room, 1.0);
            waited += 1;
        }
        let load = room.main_load_kw();
        assert!(room.reactor().output_kw() > 0.9 * load);
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 0.2);
        assert_eq!(room.start().stage(), StartStage::ShutDown);
        assert!(room.reactor().power_on(), "running down, not off yet");
        run(&mut room, 3.0);
        assert!(room.reactor().output_kw() < 0.5 * load, "output runs down ({} kW)", room.reactor().output_kw());
        assert!(room.battery().output_kw() > 0.0, "the battery takes over");
        // No new PRIME in the middle of the shutdown.
        assert!(!room.lever_allows(LEVER_PRIME));
        run(&mut room, 5.0);
        assert_eq!(room.start().stage(), StartStage::Boot);
        assert_eq!(room.reactor().output_kw(), 0.0);
        assert!(room.lever_allows(LEVER_PRIME), "restart needs PRIME and RUN only");
    }

    #[test]
    fn a_scram_or_an_eject_drops_the_stage_to_boot_and_the_restart_runs_again() {
        let mut room = cold_room();
        to_run_up(&mut room);
        assert!(room.scram().is_some());
        assert_eq!(room.start().stage(), StartStage::Boot, "awake on battery, not dark");
        assert!(room.master_on());
        assert_eq!(room.start().trap(), 0.0);
        // The host drops the stand with it; a lever left at RUN would not ignite anything.
        stand(&mut room, [true; 3], LEVER_RUN);
        run(&mut room, 2.0);
        assert_eq!(room.start().stage(), StartStage::Boot);
        assert!(!room.reactor().power_on());
        // The restart: toggles, PRIME, RUN again.
        to_run_up(&mut room);
        let events = room.eject_core();
        assert!(!events.is_empty());
        assert_eq!(room.start().stage(), StartStage::Boot);
        assert!(room.master_on());
        // An eject on an ejected core, and a SCRAM without a core, change nothing.
        assert!(room.scram().is_none());
        assert_finite(&room);
    }

    #[test]
    fn the_coolant_pump_runs_on_battery_through_boot_prime_and_after_a_scram() {
        let mut room = cold_room();
        room.pump_mut().set_tank(100.0);
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 5.0);
        assert!(room.pump().speed() > 0.9, "BOOT: speed {}", room.pump().speed());
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 5.0);
        assert!(room.pump().speed() > 0.9, "PRIME");
        assert!(room.pump().on());
        run(&mut room, 6.0);
        stand(&mut room, [true; 3], LEVER_RUN);
        run(&mut room, 5.0);
        room.scram();
        stand(&mut room, [false; 3], LEVER_OFF);
        run(&mut room, 20.0);
        assert!(room.pump().on() && room.pump().speed() > 0.9, "after the SCRAM: speed {}", room.pump().speed());
        assert!(room.flow() > 0.0);
    }

    #[test]
    fn the_battery_charges_by_itself_only_while_running_with_headroom_and_never_over_capacity() {
        let mut room = cold_room();
        room.battery_mut().set_recharge(1.0);
        let cap = room.battery().tuning().capacity_kwmin;
        room.battery_mut().set_charge_kwmin(cap * 0.5);
        // Not charging in Boot, PRIME or RUN-UP; PRIME only draws.
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 1.0);
        let boot = room.battery().charge_kwmin();
        assert_eq!(room.charge_request(1000.0), 0.0);
        stand(&mut room, [true; 3], LEVER_PRIME);
        run(&mut room, 3.0);
        assert_eq!(room.charge_request(1000.0), 0.0, "no charging in PRIME");
        assert!(room.battery().charge_kwmin() < boot);
        run(&mut room, 8.0);
        stand(&mut room, [true; 3], LEVER_RUN);
        while room.start().stage() != StartStage::Running {
            run(&mut room, 0.1);
            assert!(room.start().stage() != StartStage::RunUp || room.charge_request(1000.0) == 0.0, "no charging in RUN-UP");
        }
        // Running: the output carries the charge draw on top of the demand, and the battery fills.
        run(&mut room, 20.0);
        let load = room.main_load_kw();
        let before = room.battery().charge_kwmin();
        run(&mut room, 60.0);
        assert!(room.battery().charge_kwmin() > before, "charges while running");
        assert!(room.reactor().output_kw() > load, "output {} over the demand {load}", room.reactor().output_kw());
        assert!(room.reactor().output_kw() <= room.reactor().tuning().max_output_kw);
        // Full: no request, never over the capacity.
        room.battery_mut().set_charge_kwmin(cap);
        run(&mut room, 10.0);
        assert_eq!(room.charge_request(load), 0.0);
        assert!(room.battery().charge_kwmin() <= cap);
        // No headroom: never over the reactor's maximum.
        room.battery_mut().set_charge_kwmin(cap * 0.5);
        assert_eq!(room.charge_request(room.reactor().tuning().max_output_kw), 0.0);
        // Off: nothing.
        stand(&mut room, [true; 3], LEVER_OFF);
        run(&mut room, 15.0);
        assert_eq!(room.charge_request(load), 0.0);
        assert_finite(&room);
        // A zero maximum recharge is no NaN.
        let mut t = Tuning::default();
        t.battery.max_recharge_kw = 0.0;
        let mut r2 = EngineRoom::from_tuning(&t, 5);
        r2.battery_mut().set_recharge(1.0);
        assert_eq!(r2.charge_request(100.0), 0.0);
    }
}
