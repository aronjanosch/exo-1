//! Rules for the engine room: auto and manual, pump, junction box, faults and alarm levels.
use super::*;

const DT: f64 = 1.0 / 30.0;

/// Two rods, skill 50, power on, nothing else changed.
fn room() -> EngineRoom {
    let mut r = EngineRoom::new(RoomTuning::default(), ReactorTuning::default(), 11);
    assert!(r.set_power(true));
    r
}

/// Same room, but the pump and the junction box never fail a repair roll (for timing tests).
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

/// Steps `secs` with `input`, returns every event with its time and the room's temperature
/// trace, sampled each step.
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

fn first(ev: &[(f64, RoomEvent)], f: impl Fn(&RoomEvent) -> bool) -> Option<f64> {
    ev.iter().find(|(_, e)| f(e)).map(|(t, _)| *t)
}

fn is_fire(e: &RoomEvent) -> bool {
    matches!(e, RoomEvent::Reactor(ReactorEvent::Fire { .. }) | RoomEvent::Fire { .. })
}

fn alarm_on(kind: AlarmKind) -> impl Fn(&RoomEvent) -> bool {
    move |e| *e == RoomEvent::Reactor(ReactorEvent::Alarm { kind, on: true })
}

#[test]
fn auto_holds_steady_at_constant_load() {
    let mut r = room();
    let (ev, _) = run(&mut r, 240.0, Input::auto());
    assert!(!ev.iter().any(|(_, e)| is_fire(e)), "{ev:?}");
    let (_, temps) = run(&mut r, 60.0, Input::auto());
    let (tmin, tmax) = temps.iter().fold((f64::MAX, f64::MIN), |(a, b), &t| (a.min(t), b.max(t)));
    let b = r.reactor().bands();
    let out = r.reactor().output_kw();
    println!(
        "auto, 500 kW, pump on: temp {tmin:.2}..{tmax:.2} (optimal {:.1}..{:.1}), output {out:.1} kW, level {:?}",
        b.temp_optimal.lo, b.temp_optimal.hi, r.level()
    );
    assert!(b.temp_optimal.contains(tmin) && b.temp_optimal.contains(tmax));
    assert!((out - 500.0).abs() < 15.0, "{out}");
    assert_eq!(r.level(), AlarmLevel::None);
}

/// Steady state at 500 kW, then the load drops to the life support alone.
fn settle_then_drop(r: &mut EngineRoom) {
    run(r, 120.0, Input::auto());
    r.trigger(Fault::LoadDrop);
}

#[test]
fn auto_overshoots_after_a_load_drop() {
    // The controller follows the turbine fast and cuts fission slowly, so after a sudden drop
    // the temperature climbs above the optimal band for several seconds before it turns back.
    // No fire: the fire timer needs 20 s above the band.
    let mut r = room();
    settle_then_drop(&mut r);
    let (_, temps) = run(&mut r, 150.0, Input::auto());
    let hi = r.reactor().bands().temp_optimal.hi;
    let peak = temps.iter().cloned().fold(f64::MIN, f64::max);
    let above = temps.iter().filter(|&&t| t > hi).count() as f64 * DT;
    println!("auto after LoadDrop: peak {peak:.1}, above {hi:.0} for {above:.1} s, output {:.1} kW", r.reactor().output_kw());
    assert!(above >= 5.0, "above the optimal band for {above:.1} s");
    assert!(peak < r.reactor().bands().temp_allowed.hi, "the overshoot stays short of the allowed band: {peak}");
}

#[test]
fn manual_with_a_good_input_avoids_the_overshoot() {
    let mut r = room();
    settle_then_drop(&mut r);
    // The good player sees the drop at once and sets fission and turbine for the new load (100 kW):
    // turbine 10 gives the minimum output near the load, fission holds the temperature near 50.
    let (_, temps) = run(&mut r, 120.0, Input::manual(31.0, 10.0));
    let peak = temps.iter().cloned().fold(f64::MIN, f64::max);
    println!("manual after LoadDrop: peak {peak:.1}, output {:.1} kW", r.reactor().output_kw());
    assert!(peak < r.reactor().bands().temp_optimal.hi, "manual peak {peak}");
    assert!(temps.iter().all(|&t| t < r.reactor().bands().temp_optimal.hi), "manual stays inside the optimal band");
}

#[test]
fn pump_break_climbs_to_fire_then_meltdown() {
    let mut r = room();
    // Fission 43.75 with two rods and the pump on: the temperature settles at 50.
    let input = Input::manual(43.75, 50.0);
    run(&mut r, 30.0, input);
    let before = r.reactor().temperature();
    assert!((before - 50.0).abs() < 1.0, "{before}");
    let ev_now = r.trigger(Fault::PumpBreak);
    assert!(matches!(ev_now.as_slice(), [RoomEvent::Device { source: Source::Pump, event: DeviceEvent::Broke }]), "{ev_now:?}");
    assert_eq!(r.pump().factor(), 0.0);
    let (ev, temps) = run(&mut r, 150.0, input);
    let b = r.reactor().bands();
    let crit = first(&ev, alarm_on(AlarmKind::TempCritical)).expect("critical");
    let fire = first(&ev, |e| matches!(e, RoomEvent::Reactor(ReactorEvent::Fire { .. }))).expect("a fire");
    let melt = first(&ev, |e| *e == RoomEvent::Reactor(ReactorEvent::Meltdown)).expect("a meltdown");
    println!(
        "pump broken: temp {:.1} -> {:.1}; above optimal {:.0} at {fire:.1} - 20 s; critical at {crit:.1} s, fire {fire:.1} s, meltdown {melt:.1} s",
        before, temps.iter().cloned().fold(f64::MIN, f64::max), b.temp_optimal.hi
    );
    assert!(fire < melt);
    assert!((melt - crit - 120.0).abs() < 0.2, "meltdown {melt} crit {crit}");
    // The reactor sits broken with spent rods until repaired and restarted.
    assert!(!r.reactor().condition().functioning());
    assert!(r.reactor().rods().iter().all(|rod| rod.spent()));
    assert_eq!(r.reactor().output_kw(), 0.0);
    assert!(!r.set_power(true));
}

#[test]
fn repairing_the_pump_stops_the_climb() {
    let mut r = safe_room();
    let input = Input::manual(43.75, 50.0);
    run(&mut r, 30.0, input);
    r.trigger(Fault::PumpBreak);
    let (_, tr) = run(&mut r, 5.0, input);
    let peak = tr.iter().cloned().fold(f64::MIN, f64::max);
    assert!(peak > 85.0, "the break should climb: {peak}");
    // Repair at skill 50 until the pump works again.
    let mut steps = 0;
    while !r.pump().condition().functioning() || r.pump().factor() < 1.0 {
        let o = r.pump_mut().condition_mut().repair_step(DT, 50.0, device_core::Tool::WRENCH);
        r.step(DT, input);
        steps += 1;
        assert!(steps < 30 * 60, "repair did not finish: {o:?}");
        if matches!(o, device_core::RepairOutcome::Done { .. }) {
            break;
        }
    }
    // The pump works again: the temperature goes back down to the level the pump holds.
    let at_repair = r.reactor().temperature();
    let (_, tail) = run(&mut r, 10.0, input);
    let later = *tail.last().unwrap();
    println!("pump repaired after {:.1} s: temp peak {peak:.1}, at repair {at_repair:.1}, ten seconds on {later:.1}", steps as f64 * DT);
    assert!(at_repair < peak - 20.0, "the climb did not stop: {peak} -> {at_repair}");
    assert!(later <= at_repair + 0.01, "still climbing after the repair: {at_repair} -> {later}");
}

#[test]
fn overvoltage_damages_the_junction_box_and_can_spark() {
    let mut r = room();
    run(&mut r, 60.0, Input::manual(43.75, 50.0));
    // Load drops to 100 kW; the turbine keeps the output near 470 kW: 4.7x the demand.
    r.trigger(Fault::LoadDrop);
    let (ev, _) = run(&mut r, 60.0, Input::manual(43.75, 50.0));
    let sparks = ev.iter().filter(|(_, e)| *e == RoomEvent::Sparks).count();
    let value = r.junction().condition().value();
    println!("overvolt 60 s: junction {value:.1}, sparks {sparks}, output {:.1} kW, demand {:.0} kW", r.reactor().output_kw(), r.grid().request_kw());
    // 2 per second of overvoltage: broken after 50 s.
    assert!(!r.junction().condition().functioning(), "junction {value}");
    assert!(sparks > 0, "seeded: expected sparks in 60 s");
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
    run(&mut r, 120.0, Input::manual(43.75, 50.0));
    assert_eq!(r.level(), AlarmLevel::None);
    // A broken pump needs repair at once: Warn.
    r.trigger(Fault::PumpBreak);
    run(&mut r, DT * 2.0, Input::manual(43.75, 50.0));
    assert_eq!(r.level(), AlarmLevel::Warn);
    // Above the allowed band: Danger.
    let (_, _) = run(&mut r, 10.0, Input::manual(43.75, 50.0));
    assert!(r.reactor().temperature() > r.reactor().bands().temp_allowed.hi);
    assert_eq!(r.level(), AlarmLevel::Danger);
    // Repaired pump, temperature falls back below the optimal band: None again.
    let input = Input::manual(10.0, 50.0);
    while r.pump().factor() < 1.0 {
        r.pump_mut().condition_mut().repair_step(DT, 50.0, device_core::Tool::WRENCH);
        r.step(DT, input);
    }
    run(&mut r, 60.0, input);
    assert_eq!(r.level(), AlarmLevel::None, "temp {}", r.reactor().temperature());
}

#[test]
fn a_pump_switched_off_takes_no_heat_away_and_the_temperature_climbs() {
    let mut r = safe_room();
    let input = Input::manual(43.75, 50.0);
    run(&mut r, 30.0, input);
    let before = r.reactor().temperature();
    assert!((before - 50.0).abs() < 1.0, "{before}");
    r.set_pump(false);
    assert_eq!(r.flow(), 0.0);
    let (_, temps) = run(&mut r, 20.0, input);
    assert!(*temps.last().unwrap() > before + 10.0, "climbs: {before} -> {:?}", temps.last());
    // On again: the flow is back and the climb stops.
    r.set_pump(true);
    assert_eq!(r.flow(), 1.0);
    let (_, temps) = run(&mut r, 60.0, input);
    assert!(temps.windows(2).rev().take(300).all(|w| w[1] <= w[0] + 1e-9), "falls back once on");
}

#[test]
fn the_flow_needs_a_running_reactor() {
    let mut r = room();
    assert_eq!(r.flow(), 1.0, "the pump is on from the start");
    r.reactor_mut().scram();
    assert_eq!(r.flow(), 0.0, "no reactor, no coolant");
    run(&mut r, 1.0, Input::auto());
    assert!(r.set_power(true));
    assert_eq!(r.flow(), 1.0);
}

#[test]
fn the_machines_have_power_only_through_a_working_box_and_a_running_reactor() {
    let mut r = room();
    assert!(!r.powered(), "cold, output 0");
    run(&mut r, 120.0, Input::auto());
    assert!(r.powered());
    r.reactor_mut().scram();
    run(&mut r, 1.0, Input::auto());
    assert!(!r.powered(), "scrammed: no output, no power");
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
    assert_eq!(r.grid().request_kw(), load - 50.0);
    run(&mut r, 60.0, Input::auto());
    assert!((r.reactor().output_kw() - (load - 50.0)).abs() < 20.0, "the load drops: {:.1}", r.reactor().output_kw());
    assert_eq!(r.grid().factor(Group::Lights), 0.0);
    r.set_breaker(Group::Lights, true);
    assert_eq!(r.grid().request_kw(), load);
}

#[test]
fn overvolt_is_seen_by_the_box() {
    let mut r = room();
    run(&mut r, 60.0, Input::manual(43.75, 50.0));
    assert!(!r.junction().overvolted());
    r.trigger(Fault::LoadDrop);
    run(&mut r, 1.0, Input::manual(43.75, 50.0));
    assert!(r.junction().overvolted(), "100 kW demand, 470 kW supply");
}

/// Keeps four fresh rods in the slots (the spent ones go), so a long run is not cut by the fuel.
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
    assert!(r.set_power(true));
    let mut broke = Vec::new();
    for _ in 0..6 {
        refuel(&mut r);
        let (ev, _) = run(&mut r, 300.0, Input::auto());
        broke.extend(ev.into_iter().filter(|(_, e)| matches!(e, RoomEvent::Device { event: DeviceEvent::Broke, .. })));
    }
    assert!(broke.is_empty(), "{broke:?}");
    for (name, c) in [("reactor", r.reactor().condition()), ("pump", r.pump().condition()), ("junction", r.junction().condition())] {
        println!("{name}: {:.1}", c.value());
        assert!(c.functioning(), "{name} broke by wear");
        assert!(c.value() >= c.tuning().repair_threshold - 1e-9, "{name} asks for repair after normal running: {}", c.value());
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
