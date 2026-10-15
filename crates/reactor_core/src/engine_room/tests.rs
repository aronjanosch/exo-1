use super::*;
use crate::reactor::Blast;

const DT: f64 = 1.0 / 30.0;

fn room() -> EngineRoom {
    let mut r = EngineRoom::new(RoomTuning::default(), ReactorTuning::default(), 11);
    assert!(r.set_power(true));
    r
}

fn safe_room() -> EngineRoom {
    let tuning = RoomTuning {
        pump: DeviceTuning { fail_chance: 0.0, ..DeviceTuning::default() },
        junction: DeviceTuning { fail_chance: 0.0, ..DeviceTuning::default() },
        ..RoomTuning::default()
    };
    let mut r = EngineRoom::new(tuning, ReactorTuning::default(), 11);
    assert!(r.set_power(true));
    r
}

/// Seconds a room runs on Auto to reach its steady state. Past about 350 s the High pump has run the
/// tank dry at full load, so `settle` refills it after.
const SETTLE_S: f64 = 300.0;

/// A room settled on Auto, with a full tank (the settle drains about 80 % at High).
fn settle(r: &mut EngineRoom) {
    run(r, SETTLE_S, Input::auto());
    r.pump_mut().set_tank(100.0);
}

fn run(r: &mut EngineRoom, secs: f64, input: Input) -> (Vec<(f64, RoomEvent)>, Vec<f64>) {
    let mut ev = Vec::new();
    let mut temps = Vec::new();
    let mut t = 0.0;
    for _ in 0..(secs / DT).round() as usize {
        t += DT;
        ev.extend(r.step(DT, input).into_iter().map(|e| (t, e)));
        temps.push(r.reactor().temperature());
    }
    (ev, temps)
}

fn is_fire(e: &RoomEvent) -> bool {
    matches!(e, RoomEvent::Reactor(ReactorEvent::Fire { .. }) | RoomEvent::Fire { .. })
}

#[test]
fn no_overvoltage_no_damage() {
    let mut r = room();
    run(&mut r, 120.0, Input::auto());
    run(&mut r, 60.0, Input::auto());
    assert_eq!(r.junction().condition().value(), 100.0);
}

#[test]
fn a_broken_junction_box_cuts_its_consumers() {
    let mut r = room();
    run(&mut r, 30.0, Input::auto());
    assert!(r.grid().breaker(Group::Engines));
    let ev = r.trigger(Fault::Surge);
    assert!(ev.is_empty() || matches!(ev.as_slice(), [RoomEvent::Device { .. }]));
    r.junction_mut().condition_mut().set_value(0.0);
    run(&mut r, 1.0, Input::auto());
    for g in Group::ALL {
        assert_eq!(r.grid().factor(g), 0.0, "{g:?}");
        assert!(!r.grid().supply(g).on);
    }
}

#[test]
fn surge_breaks_a_weak_junction_box_with_an_event() {
    let tuning = RoomTuning { surge_damage: 100.0, ..RoomTuning::default() };
    let mut r = EngineRoom::new(tuning, ReactorTuning::default(), 4);
    let ev = r.trigger(Fault::Surge);
    assert_eq!(ev, vec![RoomEvent::Device { source: Source::Junction, event: DeviceEvent::Broke }]);
    assert!(!r.junction().condition().functioning());
}

#[test]
fn alarm_level_follows_the_room() {
    let mut r = safe_room();
    let input = Input::manual(47.0, 64.0);
    run(&mut r, 120.0, input);
    assert_eq!(r.level(), AlarmLevel::None);
    r.trigger(Fault::PumpBreak);
    run(&mut r, DT * 2.0, input);
    assert_eq!(r.level(), AlarmLevel::Warn);
    let mut secs = 0.0;
    while r.level() != AlarmLevel::Danger && secs < 40.0 {
        run(&mut r, DT, input);
        secs += DT;
    }
    assert!(r.reactor().temperature() > r.reactor().bands().temp_allowed.hi);
    assert_eq!(r.level(), AlarmLevel::Danger, "after {secs:.1} s");
    // Repaired pump, the flow coasts back up and the temperature falls below the optimal band: None again.
    // Repair until the pump is full (not just running: a pump at the broken limit runs at half).
    let input = Input::manual(10.0, 50.0);
    let mut full = false;
    while !full {
        let o = r.pump_mut().condition_mut().repair_step(DT, 50.0, device_core::Tool::WRENCH);
        full = matches!(o, device_core::RepairOutcome::Done { .. });
        r.step(DT, input);
    }
    run(&mut r, 90.0, input);
    assert!(!r.reactor().melted(), "the core melted");
    assert_eq!(r.level(), AlarmLevel::None, "temp {}", r.reactor().temperature());
}

#[test]
fn the_flow_runs_on_after_power_off_on_the_backup() {
    // The pump has a backup supply: with the reactor delivering nothing (power off, cold start,
    // broken) it runs at full factor, so the flow stays while the core cools on the decay heat.
    // A warm start: from cold the engines stall under the minimum voltage until the grid comes up,
    // which swings the grid for a while (the engines' gate, Barotrauma's `Voltage < MinVoltage`).
    let mut r = room();
    r.warm_start();
    assert!((r.flow() - 1.0).abs() < 1e-9, "the pump is on and running: {}", r.flow());
    r.set_power(false);
    run(&mut r, 1.0, Input::auto());
    assert_eq!(r.flow(), 1.0, "no output, the backup runs the pump");
    run(&mut r, 5.0, Input::auto());
    assert_eq!(r.flow(), 1.0, "still on the backup");
    assert!(r.set_power(true));
    run(&mut r, 120.0, Input::auto());
    assert!(r.reactor().output_kw() > 0.0 && (r.flow() - 1.0).abs() < 1e-9, "on the grid again: {}", r.flow());
}

#[test]
fn the_machines_have_power_only_through_a_working_box_and_a_running_reactor() {
    let mut r = room();
    assert!(!r.powered(), "cold, output 0");
    run(&mut r, 120.0, Input::auto());
    assert!(r.powered());
    r.set_power(false);
    run(&mut r, 1.0, Input::auto());
    assert!(!r.powered(), "powered off: no output, no power");
    assert!(r.set_power(true));
    run(&mut r, 120.0, Input::auto());
    assert!(r.powered());
    r.junction_mut().condition_mut().set_value(0.0);
    run(&mut r, 1.0, Input::auto());
    assert!(!r.powered(), "a broken box passes nothing");
}

#[test]
fn breakers_are_the_only_switch_of_the_consumers() {
    let mut r = room();
    run(&mut r, 120.0, Input::auto());
    let load = r.grid().request_kw();
    r.set_breaker(Group::Lights, false);
    assert_eq!(r.grid().request_kw(), load - 80.0);
    run(&mut r, 60.0, Input::auto());
    assert_eq!(r.grid().factor(Group::Lights), 0.0);
    r.set_breaker(Group::Lights, true);
    assert_eq!(r.grid().request_kw(), load);
}

#[test]
fn overvolt_is_seen_by_the_box() {
    let mut r = room();
    run(&mut r, 60.0, Input::manual(47.0, 64.0));
    assert!(!r.junction().overvolted());
    r.trigger(Fault::LoadDrop);
    run(&mut r, 1.0, Input::manual(47.0, 64.0));
    assert!(r.junction().overvolted(), "100 kW demand, 470 kW supply");
}

fn refuel(r: &mut EngineRoom) {
    while let Some(i) = r.reactor().rods().iter().position(|x| x.spent()) {
        r.reactor_mut().remove_rod(i);
    }
    while r.reactor_mut().insert_rod(FuelRod::standard()).is_ok() {}
}

#[test]
fn normal_running_wears_the_machines_to_the_floor_and_never_breaks_them() {
    // The shipped values: 30 minutes of normal running at about 500 kW.
    let shipped: Tuning = Tuning::load(&[content_core::File { path: crate::tuning::REACTOR_PATH.into(), text: include_str!("../../../../content/gameplay/reactor/reactor.json").into() }]).unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = EngineRoom::new(shipped.room, shipped.reactor, 11);
    // Steady load: a wandering one swings the core above its optimal band now and then, and that
    // heat wears the pump as overload below its floor (see the sim's `wandering_load`).
    r.set_engine_wander(false);
    assert!(r.set_power(true));
    let mut broke = Vec::new();
    for _ in 0..6 {
        refuel(&mut r);
        r.pump_mut().refill(100.0);
        let (ev, _) = run(&mut r, 300.0, Input::auto());
        broke.extend(ev.into_iter().filter(|(_, e)| matches!(e, RoomEvent::Device { event: DeviceEvent::Broke, .. })));
    }
    assert!(broke.is_empty(), "{broke:?}");
    // Wear runs down to each device's floor (the reactor's is clamped up to its broken limit), and
    // stops there. Below the repair threshold it asks for repair, but it never breaks anything.
    // The junction box has no normal wear yet (only overvoltage damage): it stays full.
    assert_eq!(r.junction().condition().value(), 100.0);
    for (name, c) in [("reactor", r.reactor().condition()), ("pump", r.pump().condition())] {
        let t = c.tuning();
        let floor = t.wear_floor.max(t.broken_below);
        println!("{name}: {:.1} (floor {floor}, repair below {})", c.value(), t.repair_threshold);
        assert!(c.functioning(), "{name} broke by wear");
    }
}

#[test]
fn a_running_reactor_without_steam_gives_no_power_and_says_so() {
    // The pump is broken (no coolant) and the turbine draws more heat than the low fission makes:
    // the temperature falls to 0. The reactor is still on (power_on), but its output is 0 at 0 °C,
    // so the machines are dark. The screens read `powered`, so they show NO POWER.
    let mut r = room();
    r.trigger(Fault::PumpBreak);
    let input = Input::manual(6.0, 49.0);
    let mut cold_on = false;
    for _ in 0..(120.0 / DT) as usize {
        r.step(DT, input);
        if r.reactor().temperature() == 0.0 && r.reactor().power_on() {
            cold_on = true;
            break;
        }
    }
    assert!(cold_on, "the temperature falls to 0 with the reactor on");
    assert!(r.reactor().power_on(), "still on");
    assert_eq!(r.reactor().output_kw(), 0.0, "no steam, no output");
    assert!(!r.powered(), "the machines are dark while the reactor reads on");
}

// core settles in the optimal band instead of climbing (TODO(initiator): is that the feel we
// past the optimal band by a wide margin (TODO(initiator): does this hold at the allowed band?).

// Was 300..=600 s at 500 kW; at 1280 kW the tank lasts 291 s. TODO(initiator): is 4 minutes at full

#[test]
fn the_pump_uses_more_coolant_the_more_heat_it_carries() {
    let mut cool = room();
    refuel(&mut cool);
    let mut hot = room();
    refuel(&mut hot);
    run(&mut cool, 60.0, Input::manual(20.0, 20.0));
    run(&mut hot, 60.0, Input::manual(90.0, 20.0));
    assert!(
        hot.pump().tank() < cool.pump().tank() - 1.0,
        "hot {} should use more than cool {}",
        hot.pump().tank(),
        cool.pump().tank()
    );
}

#[test]
fn an_empty_tank_is_no_flow_and_the_core_stays_alive() {
    let mut full = room();
    refuel(&mut full);
    let mut dry = room();
    refuel(&mut dry);
    // Warm both up on Auto (a cold room has no power, so no flow either way), then dry one.
    run(&mut full, SETTLE_S, Input::auto());
    run(&mut dry, SETTLE_S, Input::auto());
    // The settle drains the tank: both start full again, then one is dried.
    full.pump_mut().set_tank(100.0);
    dry.pump_mut().set_tank(100.0);
    dry.pump_mut().set_tank(0.0);
    assert_eq!(dry.flow(), 0.0, "no tank, no flow");
    assert!(full.flow() > 0.0);
    run(&mut full, 120.0, Input::auto());
    run(&mut dry, 120.0, Input::auto());
    assert!(!dry.reactor().melted() && !full.reactor().melted(), "both stay alive for the comparison");
    // Under Auto the dry room ends colder (dry 45.3 vs full 53.0 measured). Not asserted; the empty
    // tank's heat is the sim's coolant_empty at full load.
}

#[test]
fn a_canister_refills_the_tank_up_to_full_and_brings_the_flow_back() {
    let mut r = room();
    run(&mut r, 60.0, Input::auto());
    r.pump_mut().set_tank(30.0);
    assert_eq!(r.pump_mut().refill(50.0), 50.0);
    assert_eq!(r.pump().tank(), 80.0);
    assert_eq!(r.pump_mut().refill(50.0), 20.0, "only what fits is taken");
    assert_eq!(r.pump().tank(), 100.0);
    r.pump_mut().set_tank(0.0);
    assert_eq!(r.flow(), 0.0);
    r.pump_mut().refill(50.0);
    assert!(r.flow() > 0.0, "the flow comes back with coolant in the tank");
}

// The ticket's "a few minutes" does not follow from these values (see the report, TODO(initiator)).

#[test]
fn undervoltage_is_seen_and_damages_nothing() {
    let t = RoomTuning::default();
    let mut jb = JunctionBox::new(&t, 3);
    let (through, ev) = jb.pass(0.1, 50.0, 100.0, &t);
    assert_eq!(through, 50.0);
    assert!(jb.undervolted());
    assert!(ev.is_empty());
    assert_eq!(jb.condition().value(), 100.0);
    jb.pass(0.1, 100.0, 100.0, &t);
    assert!(!jb.undervolted());
    jb.pass(0.1, 0.0, 0.0, &t);
    assert!(!jb.undervolted(), "no demand is not undervoltage");
}

#[test]
fn a_room_with_demand_and_no_power_is_undervolted_and_one_with_no_demand_is_not() {
    let mut r = EngineRoom::new(RoomTuning::default(), ReactorTuning::default(), 11);
    r.step(DT, Input::auto());
    assert_eq!(r.voltage(), 0.0);
    assert!(r.junction().undervolted());
    for g in Group::ALL {
        r.set_breaker(g, false);
    }
    r.step(DT, Input::auto());
    assert_eq!(r.voltage(), 1.0);
    assert!(!r.junction().undervolted());
}

#[test]
fn the_engine_throttle_sets_the_engines_demand_as_the_force_follows() {
    // A warm start: a cold room has no voltage, and the engines stall under the minimum voltage.
    let mut r = room();
    r.warm_start();
    let full = r.grid().group(Group::Engines).demand_kw;
    r.set_engine_throttle(50.0);
    assert_eq!(r.engine_throttle(), 50.0, "the target at once");
    run(&mut r, 5.0, Input::auto());
    let want = full * 0.5_f64.powf(1.5);
    assert!((r.grid().group(Group::Engines).demand_kw - want).abs() < 1e-3, "about 283 kW at half throttle");
    r.set_engine_throttle(150.0);
    assert_eq!(r.engine_throttle(), 100.0, "clamped to -100..100");
    r.set_engine_throttle(-150.0);
    assert_eq!(r.engine_throttle(), -100.0);
}

#[test]
fn wander_keeps_the_throttle_in_range_and_moves_the_engines_demand() {
    let t = RoomTuning { engine_wander: true, ..RoomTuning::default() };
    let mut r = EngineRoom::new(t, ReactorTuning::default(), 5);
    assert!(r.set_power(true));
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    let mut demands = Vec::new();
    for _ in 0..(300.0 / DT) as usize {
        r.step(DT, Input::auto());
        let th = r.engine_throttle();
        assert!((30.0..=100.0).contains(&th), "throttle {th}");
        lo = lo.min(th);
        hi = hi.max(th);
        demands.push(r.grid().group(Group::Engines).demand_kw);
    }
    println!("wander 300 s: throttle {lo:.1}..{hi:.1}");
    let first = demands[0];
    assert!(demands.iter().any(|d| *d != first));
    r.set_engine_wander(false);
    let held = r.engine_throttle();
    for _ in 0..(60.0 / DT) as usize {
        r.step(DT, Input::auto());
    }
    assert_eq!(r.engine_throttle(), held);
}

#[test]
fn wander_is_the_same_for_the_same_seed() {
    let t = RoomTuning { engine_wander: true, ..RoomTuning::default() };
    let run_one = |seed: u64| {
        let mut r = EngineRoom::new(t, ReactorTuning::default(), seed);
        let mut v = Vec::new();
        for _ in 0..600 {
            r.step(DT, Input::auto());
            v.push(r.engine_throttle());
        }
        v
    };
    assert_eq!(run_one(9), run_one(9));
    assert_ne!(run_one(9), run_one(10));
}

#[test]
fn a_new_pump_starts_high_and_running_and_set_pump_maps_to_high_and_off() {
    let mut r = room();
    assert_eq!(r.pump().mode(), PumpMode::High);
    assert!(r.pump().on());
    assert_eq!(r.pump().speed(), 1.0);
    r.set_pump(false);
    assert_eq!(r.pump().mode(), PumpMode::Off);
    assert!(!r.pump().on());
    r.set_pump(true);
    assert_eq!(r.pump().mode(), PumpMode::High);
    r.set_pump_mode(PumpMode::Low);
    assert_eq!(r.pump().mode(), PumpMode::Low);
    assert!(r.pump().on(), "Low is on");
    r.pump_mut().set_on(false);
    assert_eq!(r.pump().mode(), PumpMode::Off);
    r.pump_mut().set_on(true);
    assert_eq!(r.pump().mode(), PumpMode::High);
}

#[test]
fn a_short_grid_slows_the_pump_and_no_output_does_not() {
    let mut r = room();
    settle(&mut r);
    assert!((r.flow() - 1.0).abs() < 1e-9, "full power, full flow");
    r.grid_mut().set_demand(Group::Lights, 2000.0);
    run(&mut r, 20.0, Input::auto());
    let v = r.voltage();
    assert!((r.flow() - v).abs() < 0.02, "flow {} follows the voltage {v}", r.flow());
    // With no output at all the backup carries the pump, whatever the grid asks.
    r.set_power(false);
    run(&mut r, 1.0, Input::auto());
    assert_eq!(r.flow(), 1.0, "no output: the backup, not the voltage");
}

#[test]
fn the_power_factor_on_the_backup_is_one_with_no_output() {
    let r = EngineRoom::new(RoomTuning::default(), ReactorTuning::default(), 11);
    assert_eq!(r.power_factor(), 1.0);
    assert_eq!(r.flow(), 1.0);
}

/// The blast breaks the coolant pump and the junction box at once (`device_damage` 500 each), and
/// the room passes the blast on as `Reactor(Blast)` for the glue.
#[test]
fn a_blast_breaks_the_pump_and_the_junction_box() {
    let mut r = room();
    let ev = r.melt_now();
    assert!(ev.contains(&RoomEvent::Reactor(ReactorEvent::Blast(Blast::default()))), "{ev:?}");
    assert!(ev.contains(&RoomEvent::Device { source: Source::Pump, event: DeviceEvent::Broke }), "{ev:?}");
    assert!(ev.contains(&RoomEvent::Device { source: Source::Junction, event: DeviceEvent::Broke }), "{ev:?}");
    assert!(!r.pump().condition().functioning() && !r.junction().condition().functioning());
}

/// Without fuel the melt is only the melt: the pump and the junction box keep working.
#[test]
fn a_melt_without_fuel_leaves_the_pump_and_the_junction_box_alone() {
    let mut r = room();
    assert!(r.reactor_mut().pull_rod(0).is_some() && r.reactor_mut().pull_rod(0).is_some());
    let ev = r.melt_now();
    assert!(ev.contains(&RoomEvent::Reactor(ReactorEvent::Meltdown)), "{ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, RoomEvent::Reactor(ReactorEvent::Blast(_)) | RoomEvent::Device { .. })), "{ev:?}");
    assert!(r.pump().condition().functioning() && r.junction().condition().functioning());
}

/// The natural meltdown through `step` (pump broken, both rods at 100 on the brake) blasts in the
/// same step as the melt, and the devices break in that step too.
#[test]
fn a_natural_meltdown_blasts_in_its_step() {
    let mut r = room();
    r.trigger(Fault::PumpBreak);
    let mut melt_step = None;
    for i in 0..(900.0 / DT) as usize {
        let ev = r.step(DT, Input::manual(100.0, 100.0));
        if ev.contains(&RoomEvent::Reactor(ReactorEvent::Meltdown)) {
            assert!(ev.contains(&RoomEvent::Reactor(ReactorEvent::Blast(Blast::default()))), "{ev:?}");
            assert!(ev.contains(&RoomEvent::Device { source: Source::Junction, event: DeviceEvent::Broke }), "{ev:?}");
            melt_step = Some(i);
            break;
        }
    }
    assert!(melt_step.is_some(), "the core melts within 900 s");
}

// ---------- warm start (Barotrauma `Reactor.cs:613-624`, `PowerUpImmediately`) ----------

#[test]
fn a_warm_start_costs_no_coolant_and_no_rod_life() {
    let mut r = room();
    let tank = r.pump().tank();
    let lives: Vec<f64> = r.reactor().rods().iter().map(|rod| rod.life).collect();
    let _ = r.warm_start();
    assert_eq!(r.pump().tank(), tank);
    let after: Vec<f64> = r.reactor().rods().iter().map(|rod| rod.life).collect();
    assert_eq!(after, lives);
}

#[test]
fn a_warm_start_leaves_no_fire_and_no_alarm_events() {
    let mut r = room();
    let (_, ev) = r.warm_start_events();
    assert!(!ev.iter().any(is_fire), "fire during warm start: {ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, RoomEvent::Reactor(ReactorEvent::Alarm { .. }))), "alarm during warm start: {ev:?}");
}

// ---------- Emergency cooling (PULSAR's manual button) ----------

/// A settled room on Auto with a full tank (the settle in `settle`).
fn settled_room() -> EngineRoom {
    let mut r = safe_room();
    settle(&mut r);
    r
}

#[test]
fn emergency_cooling_takes_its_tank_cost_at_once() {
    let mut r = settled_room();
    let cost = r.tuning().ecool_tank_cost;
    assert_eq!(r.pump().tank(), 100.0);
    assert!(r.emergency_cool());
    assert!((r.pump().tank() - (100.0 - cost)).abs() < 1e-12, "tank {}", r.pump().tank());
    assert!(r.ecool_active());
}

#[test]
fn emergency_cooling_lasts_its_duration_and_then_the_normal_flow_is_all() {
    let mut r = settled_room();
    assert!(r.emergency_cool());
    let dur = r.tuning().ecool_duration_s;
    let _ = run(&mut r, dur - 1.0, Input::auto());
    assert!(r.ecool_active(), "still on 1 s before the end");
    let _ = run(&mut r, 2.0, Input::auto());
    assert!(!r.ecool_active(), "off after the duration");
}

#[test]
fn emergency_cooling_has_a_cooldown_that_counts_down_and_then_it_is_ready_again() {
    let mut r = settled_room();
    let cd = r.tuning().ecool_cooldown_s;
    assert_eq!(r.ecool_ready_in_s(), 0.0);
    assert!(r.emergency_cool());
    assert!((r.ecool_ready_in_s() - cd).abs() < 1e-9, "ready in {}", r.ecool_ready_in_s());
    let _ = run(&mut r, 30.0, Input::auto());
    assert!((r.ecool_ready_in_s() - (cd - 30.0)).abs() < 0.1, "ready in {}", r.ecool_ready_in_s());
    assert!(!r.emergency_cool(), "refused during the cooldown");
    let _ = run(&mut r, cd - 30.0 + 1.0, Input::auto());
    assert_eq!(r.ecool_ready_in_s(), 0.0);
    assert!(r.emergency_cool(), "ready after the cooldown");
}

#[test]
fn emergency_cooling_is_refused_with_a_tank_below_its_cost() {
    let mut r = settled_room();
    let cost = r.tuning().ecool_tank_cost;
    r.pump_mut().set_tank(cost - 0.1);
    assert!(!r.emergency_cool());
    assert!((r.pump().tank() - (cost - 0.1)).abs() < 1e-12, "a refused press takes nothing");
    assert!(!r.ecool_active());
    r.pump_mut().set_tank(cost);
    assert!(r.emergency_cool(), "exactly the cost is enough");
    assert_eq!(r.pump().tank(), 0.0);
}

#[test]
fn emergency_cooling_is_refused_with_a_broken_pump() {
    let mut r = settled_room();
    r.pump_mut().condition_mut().set_value(0.0);
    assert!(!r.pump().condition().functioning());
    assert!(!r.emergency_cool());
    assert_eq!(r.pump().tank(), 100.0);
    assert_eq!(r.ecool_ready_in_s(), 0.0);
    assert!(!r.ecool_active());
}

#[test]
fn the_emergency_press_is_an_event_of_the_step() {
    let mut r = settled_room();
    assert!(r.emergency_cool());
    let ev = r.step(DT, Input::auto());
    assert_eq!(ev.iter().filter(|e| **e == RoomEvent::EmergencyCool).count(), 1, "{ev:?}");
    let ev = r.step(DT, Input::auto());
    assert!(!ev.contains(&RoomEvent::EmergencyCool), "the event comes once");
}

/// The aux reactor (PULSAR manual 325-337): aux groups are fed by it, not by the main reactor.
fn aux_room() -> EngineRoom {
    let mut r = room();
    r.set_aux(true);
    r
}

#[test]
fn aux_disabled_keeps_the_main_request_and_the_boost_at_zero() {
    let mut r = room();
    assert!(!r.aux_enabled());
    assert_eq!(r.aux_boost_kw(), 0.0);
    let _ = r.step(DT, Input::auto());
    assert_eq!(r.aux_boost_kw(), 0.0);
    assert!((r.main_load_kw() - r.grid().request_kw()).abs() < 1e-9, "main load is the whole request");
}

#[test]
fn aux_on_the_main_request_is_the_engines_demand_only() {
    let mut r = aux_room();
    let _ = r.step(DT, Input::auto());
    assert!((r.main_load_kw() - 800.0).abs() < 1e-9, "main load {}", r.main_load_kw());
}

#[test]
fn aux_lights_off_raises_the_boost_and_the_output_range_top_by_the_lights_demand() {
    let mut r = aux_room();
    settle(&mut r);
    // 500 aux output - 400 life support - 80 lights = 20 kW spare.
    let (_, top_before) = r.reactor().output_range();
    r.set_aux_switch(Group::Lights, false);
    let (_, top_after) = r.reactor().output_range();
    // The top rises by the boost times the turbine's share of the max (the band's top, capped at 1).
    let share = (top_after - top_before) / 80.0;
    assert!(share > 0.0 && share <= 1.0 + 1e-9, "range top rose {} kW, share {share}", top_after - top_before);
}

#[test]
fn a_melted_main_reactor_with_aux_leaves_life_support_powered() {
    let mut r = aux_room();
    let _ = r.step(DT, Input::auto());
    let _ = r.melt_now();
    let _ = r.step(DT, Input::auto());
    assert!(r.reactor().melted(), "the reactor melted");
    assert!(r.grid().supply(Group::LifeSupport).on, "life support stays on the aux reactor");
    assert!((r.grid().factor(Group::LifeSupport) - 1.0).abs() < 1e-9);
    assert_eq!(r.grid().factor(Group::Engines), 0.0, "engines are on the main reactor and get nothing");

    // Control: the same melt with aux off leaves life support without power.
    let mut c = room();
    let _ = c.step(DT, Input::auto());
    let _ = c.melt_now();
    let _ = c.step(DT, Input::auto());
    assert_eq!(c.grid().factor(Group::LifeSupport), 0.0);
}

/// A room with the pump off (no coolant use), its condition set to `pct` (0..100).
fn pump_off_at(pct: f64) -> EngineRoom {
    let mut r = room();
    r.pump_mut().set_on(false);
    r.pump_mut().condition_mut().set_value(pct);
    r
}

#[test]
fn a_pump_at_or_above_half_condition_does_not_leak() {
    let mut r = pump_off_at(100.0);
    assert_eq!(r.pump_leak_rate(), 0.0);
    run(&mut r, 60.0, Input::auto());
    assert_eq!(r.pump().tank(), 100.0, "a sound pump keeps its coolant, whether it runs or not");
}

#[test]
fn a_worn_pump_leaks_by_the_condition_squared_curve_whether_running_or_not() {
    // (1 - c / 0.5)^2 of pump_leak_max (0.01 per s): 0.25 condition leaks a quarter of it.
    let mut r = pump_off_at(25.0);
    assert!((r.pump_leak_rate() - 0.0025).abs() < 1e-12);
    run(&mut r, 10.0, Input::auto());
    assert!((r.pump().tank() - 97.5).abs() < 1e-6, "10 s at 0.0025 per s is 2.5 % of the tank, got {}", r.pump().tank());
}

#[test]
fn a_broken_pump_empties_the_tank_in_about_a_hundred_seconds_and_never_below_zero() {
    let mut r = pump_off_at(0.0);
    assert!((r.pump_leak_rate() - 0.01).abs() < 1e-12);
    run(&mut r, 99.0, Input::auto());
    assert!(r.pump().tank() > 0.0, "not yet empty at 99 s");
    run(&mut r, 2.0, Input::auto());
    assert_eq!(r.pump().tank(), 0.0, "empty by 101 s");
    run(&mut r, 200.0, Input::auto());
    assert!(r.pump().tank() >= 0.0, "the tank never goes below zero");
    assert_eq!(r.pump().tank(), 0.0);
}

/// A room whose oxygen generator wears from the first second (no delay before its normal wear).
fn oxygen_room(powered: bool) -> EngineRoom {
    let tuning = RoomTuning {
        oxygen_generator: DeviceTuning { wear_delay_s: (0.0, 0.0), ..DeviceTuning::default() },
        ..RoomTuning::default()
    };
    let mut r = EngineRoom::new(tuning, ReactorTuning::default(), 11);
    if powered {
        assert!(r.set_power(true));
    }
    r
}

#[test]
fn the_oxygen_generator_wears_while_its_life_support_has_power() {
    let mut r = oxygen_room(true);
    let mut powered_steps = 0;
    for _ in 0..(120.0 / DT).round() as usize {
        r.step(DT, Input::auto());
        if r.grid().factor(Group::LifeSupport) >= 0.5 {
            powered_steps += 1;
        }
    }
    assert!(powered_steps > 0, "the room must get power for this test to mean anything");
    assert!(r.oxygen_generator().condition().value() < 100.0, "a running generator wears");
}

#[test]
fn the_oxygen_generator_does_not_wear_unpowered() {
    // Never powered: the reactor makes nothing, so life support's factor stays under 0.5.
    let mut r = oxygen_room(false);
    run(&mut r, 120.0, Input::auto());
    assert!(r.grid().factor(Group::LifeSupport) < 0.5, "no power, no air");
    assert_eq!(r.oxygen_generator().condition().value(), 100.0);
}

#[test]
fn the_oxygen_flow_halves_at_condition_0_707() {
    // Barotrauma's condition squared: at 1 / sqrt(2) condition the flow is half of the full flow.
    let mut r = room();
    r.warm_start();
    assert!(r.grid().factor(Group::LifeSupport) >= 0.5, "the room is powered");
    let full = r.oxygen_flow(2.0);
    assert!(full > 0.0);
    r.oxygen_generator_mut().condition_mut().set_value(100.0 / 2f64.sqrt());
    let half = r.oxygen_flow(2.0);
    assert!((half / full - 0.5).abs() < 1e-6, "full {full}, half {half}");
}

#[test]
fn a_blast_breaks_the_oxygen_generator() {
    let mut r = room();
    let ev = r.melt_now();
    assert!(ev.contains(&RoomEvent::Device { source: Source::OxygenGenerator, event: DeviceEvent::Broke }), "{ev:?}");
    assert!(!r.oxygen_generator().condition().functioning());
}

/// The meltdown blast opens the hull at the room's blast damage (r37; Barotrauma's explosions
/// damage the walls). A melt without fuel does not.
#[test]
fn a_blast_breaches_the_hull_and_a_melt_without_fuel_does_not() {
    let mut r = room();
    assert_eq!(r.breach().damage(), 0.0);
    let _ = r.melt_now();
    assert_eq!(r.breach().damage(), RoomTuning::default().blast_breach_damage);
    assert!(r.breach().open() > 0.0, "0.6 is past the leak threshold");

    let mut r = room();
    assert!(r.reactor_mut().pull_rod(0).is_some() && r.reactor_mut().pull_rod(0).is_some());
    let _ = r.melt_now();
    assert_eq!(r.breach().damage(), 0.0);
}

/// The dev fault HULL BREACH sets the same damage as the blast, and the wrench's hold takes it
/// down at the pump's fix speed (25 s for the whole wall): it closes the leak when it is mended.
#[test]
fn the_dev_breach_is_mended_by_the_hold_at_the_pump_speed() {
    let t = RoomTuning::default();
    let mut r = room();
    assert!(r.trigger(Fault::HullBreach).is_empty());
    assert_eq!(r.breach().damage(), t.blast_breach_damage);
    // The hold of 0.6 damage at 1 / 25 per second takes 15 s.
    for _ in 0..(15.0 * 30.0) as usize {
        r.mend_breach(DT);
    }
    assert!(r.breach().damage() < 1e-9, "{}", r.breach().damage());
    assert_eq!(r.breach().open(), 0.0, "a repaired breach closes the leak");
    r.mend_breach(10.0);
    assert_eq!(r.breach().damage(), 0.0, "mending a sound wall does nothing");
}

#[test]
fn the_room_powers_the_jump_drive_only_while_it_charges() {
    let mut r = safe_room();
    settle(&mut r);
    assert_eq!(r.grid().request_of(Group::JumpDrive), 0.0, "cold drive asks nothing");
    assert!(r.jump_mut().arm());
    run(&mut r, 10.0, Input::auto());
    assert_eq!(r.grid().group(Group::JumpDrive).demand_kw, 400.0, "charging draws the JUMP demand");
    assert!(r.jump().charge > 0.0, "the grid's power charges it");
    r.jump_mut().fill();
    run(&mut r, DT * 3.0, Input::auto());
    assert_eq!(r.grid().group(Group::JumpDrive).demand_kw, 0.0, "a full drive asks nothing");
}
