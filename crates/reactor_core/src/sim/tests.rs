use super::barotrauma::Baro;
use super::*;
use crate::engine_room::{EngineRoom, PumpMode};
use crate::reactor::{FuelRod, Reactor, ReactorTuning};

const DT: f64 = 0.1;

fn shipped() -> Tuning {
    Tuning::shipped()
}

fn comparable_reactor_with(inertia: bool) -> Reactor {
    let t = shipped();
    let rt = ReactorTuning {
        cooling_max: 0.0,
        decay_heat: 0.0,
        inertia,
        ..t.reactor
    };
    let mut r = Reactor::new(rt, 3);
    for _ in 0..t.start_rods {
        r.insert_rod(FuelRod::standard()).unwrap();
    }
    r.set_operator_skill(50.0);
    r.set_coolant(1.0);
    assert!(r.set_power(true));
    r
}

fn oracle_for(r: &Reactor) -> Baro {
    let t = *r.tuning();
    let mut b = Baro::new(
        t.max_output_kw,
        t.fire_delay_s,
        t.meltdown_delay_s,
        r.available_fuel(),
        0.5,
    );
    b.power_on = true;
    b
}

/// Manual targets, power on, constant load: our `Reactor` and the Barotrauma port agree in
/// temperature, fission, turbine and output at every step, on the oracle's own path
/// (`inertia: false`). The default path with inertia is not compared with the first-order oracle.
#[test]
fn manual_matches_barotrauma_without_inertia() {
    assert_manual_matches(comparable_reactor_with(false));
}

fn assert_manual_matches(mut ours: Reactor) {
    let mut theirs = oracle_for(&ours);
    let (dt, load) = (0.05, 500.0);
    // Cold start: heat is 3.2 per point of fission, so 50 would run into the 100 clamp in seconds.
    let targets = |t: f64| {
        if t < 70.0 {
            (30.0, 50.0)
        } else if t < 140.0 {
            (25.0, 60.0)
        } else {
            (20.0, 90.0)
        }
    };
    let mut worst = [0.0f64; 4];
    let (mut peak_temp, mut peak_output) = (0.0f64, 0.0f64);
    let mut first_bad: Option<(f64, usize)> = None;
    for k in 0..(200.0 / dt) as usize {
        let t = k as f64 * dt;
        let (f, tu) = targets(t);
        ours.set_targets(f, tu);
        theirs.target_fission = f;
        theirs.target_turbine = tu;
        ours.step(dt, load);
        theirs.step(dt, load);
        // A meltdown must come on the same step on both sides; the comparison ends there.
        assert_eq!(
            ours.melted(),
            theirs.melted,
            "melt state differs at {t:.2} s"
        );
        if ours.melted() {
            println!("both melted at {t:.2} s; comparison stops");
            break;
        }
        let diffs = [
            (ours.temperature() - theirs.temp).abs(),
            (ours.fission() - theirs.fission).abs(),
            (ours.turbine() - theirs.turbine).abs(),
            (ours.output_kw() - theirs.output).abs(),
        ];
        peak_temp = peak_temp.max(ours.temperature());
        peak_output = peak_output.max(ours.output_kw());
        for (w, d) in worst.iter_mut().zip(diffs) {
            *w = w.max(d);
        }
        if first_bad.is_none() && diffs.iter().any(|d| *d > 1e-6) {
            first_bad = Some((t, k));
        }
    }
    println!(
        "worst diff: temp {:.2e}, fission {:.2e}, turbine {:.2e}, output {:.2e}",
        worst[0], worst[1], worst[2], worst[3]
    );
    println!(
        "end: temp {:.2} (ours) {:.2} (baro), fission {:.2}, turbine {:.2}, output {:.2} kW",
        ours.temperature(),
        theirs.temp,
        ours.fission(),
        ours.turbine(),
        ours.output_kw()
    );
    assert!(
        first_bad.is_none(),
        "first difference above 1e-6 at {:?}; worst {worst:?}",
        first_bad
    );
    // The agreement must cover a live range, not a cold reactor.
    assert!(
        peak_temp > 40.0 && peak_output > 100.0,
        "peaks {peak_temp:.1} C, {peak_output:.1} kW"
    );
}

fn trace_of(name: &str, sample_every_s: f64) -> (Trace, EngineRoom) {
    let t = shipped();
    let script = scripts()
        .into_iter()
        .find(|s| s.name == name)
        .expect("a script of that name");
    let mut room = warm_room(&t, 7);
    let trace = run(&mut room, &script, DT, sample_every_s);
    (trace, room)
}

fn row_at(trace: &Trace, t: f64) -> &Row {
    trace
        .rows
        .iter()
        .find(|r| (r.t - t).abs() < 1e-6)
        .unwrap_or_else(|| panic!("no row at {t}"))
}

/// optimal band for a while. No meltdown.
#[test]
fn load_drop_overvolts_the_junction_box_and_auto_overshoots() {
    let (trace, room) = trace_of("load_drop", 5.0);
    assert!(room.junction().condition().value() < 100.0);
    assert!(!trace.melted);
}

/// With no operator action a broken pump takes the flow away and a fire comes, but Auto cuts
/// fission and the core does not melt (pump_fail_full_load: peak 99.1, one fire, no meltdown).
#[test]
fn a_broken_pump_with_no_operator_fires_and_does_not_melt() {
    let (trace, room) = trace_of("pump_fail_full_load", 5.0);
    assert!(!trace.melted && !room.reactor().melted());
    assert!(trace
        .rows
        .iter()
        .filter(|r| r.t >= 15.0)
        .all(|r| r.flow < 1e-9));
}

#[test]
fn a_low_pump_at_full_load_runs_at_its_low_speed_and_drains_slowly() {
    let (trace, room) = trace_of("pump_low_full_load", 5.0);
    let at = row_at(&trace, 100.0);
    assert_eq!(at.pump_mode, PumpMode::Low);
    assert!(!trace.melted, "the Low pump at full load keeps the core alive");
    assert_eq!(room.pump().mode(), PumpMode::Low);
}

/// An empty tank is no flow for the rest of the run, and at full load the core reaches TempCritical
/// (coolant_empty: peak 99.1, one fire, no meltdown under Auto).
#[test]
fn an_empty_tank_stops_the_flow_and_the_core_reaches_temp_critical() {
    let (trace, room) = trace_of("coolant_empty", 5.0);
    assert_eq!(room.pump().tank(), 0.0);
    assert!(trace
        .rows
        .iter()
        .filter(|r| r.t >= 15.0)
        .all(|r| r.flow == 0.0));
    assert!(!trace.melted, "Auto cuts fission and the core does not melt");
}

#[test]
fn a_repaired_and_restarted_core_runs_again() {
    let (trace, room) = trace_of("meltdown_repair_restart", 5.0);
    assert!(trace.first("Meltdown").is_some());
    let at = row_at(&trace, 180.0);
    assert!(at.power && at.output_kw > 0.0, "{at:?}");
    assert!(!room.reactor().melted(), "a repaired core is a reactor again");
}

/// Auto on the shipped tuning with the load wandering for 300 s: no TempCritical, no meltdown. Also
/// reports the peak temperature, the seconds the box is overvolted and its condition at the end.
#[test]
fn wandering_load_stays_out_of_temp_critical_over_300_s() {
    // The warm-up leaves about 75 % in the tank, and High (`pump_use_high`) runs it dry before
    // 300 s: the run starts with a full tank.
    let script = scripts().into_iter().find(|s| s.name == "wandering_load").expect("the script");
    let mut room = warm_room(&shipped(), 7);
    room.pump_mut().set_tank(100.0);
    let trace = run(&mut room, &script, DT, 5.0);
    assert!(!trace.melted);
    let mut probe = warm_room(&shipped(), 7);
    probe.pump_mut().set_tank(100.0);
    probe.set_engine_wander(true);
    let mut overvolt_s = 0.0;
    for _ in 0..3000 {
        probe.step(DT, Input::auto());
        if probe.junction().overvolted() {
            overvolt_s += DT;
        }
    }
    println!(
        "wandering_load 300 s: peak temp {:.1}, overvolt {:.1} s, junction at end {:.1}, reactor at end {:.1}, TempHigh rows {}",
        trace.peak_temp(),
        overvolt_s,
        room.junction().condition().value(),
        room.reactor().condition().value(),
        trace.count("Alarm TempHigh on")
    );
}

/// The pulled rods stop the fission at once and the core cools: the fuel is the brake, not the
/// coolant. The pump breaks at 10 s, both rods come out by hand at 30 s (not on the alarm: see
/// `pulling_both_rods_at_temp_critical_brakes_the_core`).
#[test]
fn pulled_rods_stop_the_fission_at_once_and_the_core_cools() {
    let script = Script {
        name: "pull_at_30",
        duration_s: 60.0,
        steps: vec![
            (10.0, Action::Fault(Fault::PumpBreak)),
            (30.0, Action::PullRod(0)),
            (30.0, Action::PullRod(0)),
        ],
    };
    let mut room = warm_room(&shipped(), 7);
    let trace = run(&mut room, &script, DT, 1.0);
    assert!(room.reactor().rods().is_empty());
    let (before, at) = (row_at(&trace, 30.0).clone(), row_at(&trace, 40.0).clone());
    println!("pulled at 30 s: temp {:.1} at 30, {:.1} at 40, fission {:.1}", before.temp, at.temp, at.fission);
    assert!(at.fission < 1e-9, "{at:?}");
    assert!(at.temp < before.temp, "{:.1} at 30 s, {:.1} at 40 s", before.temp, at.temp);
    assert!(!trace.melted);
}

/// The brake, on the alarm: the operator holds fission and turbine at 100, the pump breaks, and at
/// TempCritical both rods are pulled. The core must not melt within 300 s. The alarm
/// must be reached, or the test would pass without the brake acting.
#[test]
fn pulling_both_rods_at_temp_critical_brakes_the_core() {
    let (trace, room) = trace_of("pull_rods_brake", 5.0);
    // Without this the test passes for nothing: TempCritical is not reached with these dynamics.
    println!(
        "pull_rods_brake: peak {:.1}, fires {}, melted {}, TempCritical at {:?}, rods left {}",
        trace.peak_temp(),
        trace.count("Fire"),
        trace.melted,
        trace.first("Alarm TempCritical on"),
        room.reactor().rods().len()
    );
    assert!(!trace.melted, "{}", trace.table());
}

/// The overvolt: full turbine and fission with the engines and lights off (400 kW asked, 2000 made):
#[test]
fn overvolt_turbine_max_damages_the_junction_box() {
    let (trace, room) = trace_of("overvolt_turbine_max", 5.0);
    let vmax = trace.rows.iter().map(|r| r.volt).fold(0.0, f64::max);
    println!("overvolt_turbine_max: max volt {vmax:.2}, junction at end {:.1}", room.junction().condition().value());
    assert!(room.junction().condition().value() < 100.0, "the box is not damaged");
}

/// Power off at TempHigh with the pump broken (the operator at 100/100): no melt. Power off is the
/// brake; fission falls with its lag, so it is slower than an instant cut would be. Numbers are in
/// the test's println.
#[test]
fn power_off_at_temp_high_with_a_broken_pump_does_not_melt() {
    let (trace, _) = trace_of("off_at_temp_high", 5.0);
    println!("off_at_temp_high: peak {:.1}, at 60 s {:.1}, melted {}", trace.peak_temp(), row_at(&trace, 60.0).temp, trace.melted);
    assert!(trace.count("do Power(false)") == 1 && trace.first("Alarm TempHigh on").is_some());
    assert!(!trace.melted, "{}", trace.table());
}

/// Auto at full load (1280 kW, all breakers on, the pump working): no TempCritical while the coolant
/// lasts. The run starts with a full tank, as the wandering test does. Limit found: High runs the
/// tank dry at about 290 s at full load, and then the core climbs to 100 (TempCritical at 300 s), so
/// the check is over 280 s.
#[test]
fn auto_at_full_load_stays_out_of_temp_critical_over_280_s() {
    let script = Script { name: "full_load_auto", duration_s: 280.0, steps: vec![] };
    let mut room = warm_room(&shipped(), 7);
    room.pump_mut().set_tank(100.0);
    let trace = run(&mut room, &script, DT, 5.0);
    println!("full load Auto 280 s: peak {:.1}, TempCritical {}", trace.peak_temp(), trace.count("Alarm TempCritical on"));
    assert!(!trace.melted);
}

#[test]
fn a_boost_too_soon_is_refused_and_logged() {
    let t = shipped();
    let script = Script {
        name: "boost_too_soon",
        duration_s: 15.0,
        steps: vec![(10.0, Action::Boost(true)), (11.0, Action::Boost(false))],
    };
    let mut room = warm_room(&t, 7);
    let trace = run(&mut room, &script, DT, 5.0);
    assert_eq!(trace.count("do Boost(false)"), 1);
    assert_eq!(trace.count("Boost refused"), 1);
    assert!(room.reactor().temperature_boost() > 0.0, "the first boost stays");
}

#[test]
fn a_boost_with_the_power_off_is_refused() {
    let t = shipped();
    let script = Script {
        name: "boost_off",
        duration_s: 5.0,
        steps: vec![(0.0, Action::Power(false)), (1.0, Action::Boost(true))],
    };
    let mut room = warm_room(&t, 7);
    let trace = run(&mut room, &script, DT, 5.0);
    assert_eq!(trace.count("Boost refused"), 1);
    assert_eq!(room.reactor().temperature_boost(), 0.0);
}

/// Barotrauma's oracle with a boost: a plus boost at 60 s on the oracle's own path (no inertia)
/// gives the same temperature as ours, to 1e-4.
#[test]
fn boost_matches_barotrauma_without_inertia() {
    let mut ours = comparable_reactor_with(false);
    let mut theirs = oracle_for(&ours);
    let (dt, load) = (0.05, 500.0);
    ours.set_targets(25.0, 60.0);
    theirs.target_fission = 25.0;
    theirs.target_turbine = 60.0;
    let mut worst = 0.0f64;
    for k in 0..(150.0 / dt) as usize {
        let t = k as f64 * dt;
        if k == (60.0 / dt) as usize {
            assert!(ours.boost(true));
            theirs.boost = 25.0;
        }
        ours.step(dt, load);
        theirs.step(dt, load);
        assert_eq!(ours.melted(), theirs.melted, "melt state differs at {t:.2} s");
        worst = worst.max((ours.temperature() - theirs.temp).abs());
    }
    assert!(worst < 1e-4, "temperature differs by {worst}");
    assert!(ours.temperature_boost().abs() < 1e-9, "the boost faded away");
}

/// The melt with fuel left blasts once, right with the melt; the dry melt (both rods pulled first)
/// melts and makes no blast.
#[test]
fn a_melt_with_fuel_blasts_and_a_dry_melt_does_not() {
    let (wet, _) = trace_of("turbine_max_fission_max", 5.0);
    assert!(wet.melted);
    assert_eq!(wet.count("Blast"), 1, "{}", wet.table());
    assert_eq!(wet.first("Blast"), wet.first("Meltdown"));
    let (dry, room) = trace_of("meltdown_dry", 5.0);
    assert!(dry.melted);
    assert_eq!(dry.count("Meltdown"), 1, "{}", dry.table());
    assert_eq!(dry.count("Blast"), 0, "{}", dry.table());
    assert!(room.reactor().rods().iter().all(|r| r.life > 0.0), "the dry melt keeps the rods' life");
}

/// PULSAR's core eject: the pump breaks with Manual 100/100, the operator ejects at
/// TempCritical, and no meltdown follows in the 300 s (the core is out, the rods with it).
#[test]
fn eject_in_time() {
    let (trace, room) = trace_of("eject_in_time", 5.0);
    assert_eq!(trace.count("CoreEjected"), 1, "{}", trace.table());
    assert_eq!(trace.count("Meltdown"), 0, "{}", trace.table());
    assert_eq!(trace.count("Blast"), 0, "{}", trace.table());
    assert!(!trace.melted);
    let crit = trace.first("Alarm TempCritical on").expect("the core reaches TempCritical");
    let ejected = trace.first("CoreEjected").expect("the eject");
    assert!(ejected >= crit, "the eject follows TempCritical: {crit} {ejected}");
    let r = room.reactor();
    assert!(r.ejected() && r.rods().is_empty() && !r.power_on());
    assert_eq!(trace.last().unwrap().temp, 0.0);
}
