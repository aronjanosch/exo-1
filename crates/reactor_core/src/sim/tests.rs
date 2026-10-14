//! The harness runs the six scripts, the facts they show today, and the Barotrauma oracle checks.
use super::barotrauma::Baro;
use super::*;
use crate::engine_room::{EngineRoom, PumpMode};
use crate::reactor::{FuelRod, Mode, Reactor, ReactorTuning};

const DT: f64 = 0.1;

fn shipped() -> Tuning {
    let text = include_str!("../../../../content/gameplay/reactor/reactor.json");
    Tuning::parse(text).unwrap_or_else(|e| panic!("{e:#?}"))
}

/// Prints every script's table (`cargo test -p reactor_core sim -- --nocapture`). Asserts only
/// that each run finished and has rows.
#[test]
fn print_all_traces() {
    let t = shipped();
    for s in scripts() {
        let mut room = warm_room(&t, 7);
        let trace = run(&mut room, &s, DT, 5.0);
        println!(
            "== {} ({} s, {} rows) ==\n{}",
            s.name,
            s.duration_s,
            trace.rows.len(),
            trace.table()
        );
        let last = trace.last().expect("rows");
        println!(
            "summary {}: peak temp {:.1}, melted {}, fires {}, junction end {:.1}, reactor end {:.1}, power end {}",
            s.name,
            trace.peak_temp(),
            trace.melted,
            trace.count("Fire"),
            last.junction,
            last.reactor,
            last.power
        );
        assert!(!trace.rows.is_empty(), "{}: no rows", s.name);
        let last = trace.last().expect("rows");
        assert!(
            (last.t - s.duration_s).abs() < 1e-6,
            "{}: finished at {}",
            s.name,
            last.t
        );
    }
}

/// The reactor of the oracle comparisons: the shipped values, with the coolant's cooling and the
/// residual heat off (the oracle has neither), two standard rods, skill 50, power on, full flow.
fn comparable_reactor() -> Reactor {
    comparable_reactor_with(true)
}

/// `comparable_reactor` with the inertia switched on or off (off is the oracle's own path).
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
/// temperature, fission, turbine and output at every step.
///
/// Ignored since the inertia (#300): the temperature is a second-order system with the output's
/// heat, which the Barotrauma port does not have, so the two no longer agree by design.
#[test]
#[ignore = "#300: the temperature has inertia and output heat by design; the oracle is first-order"]
fn manual_matches_barotrauma() {
    assert_manual_matches(comparable_reactor());
}

/// The same comparison on the oracle's own path (`inertia: false`): our reactor agrees with it to
/// 1e-6 at every step, as it did before #300.
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

/// Auto through a load drop, both from the same warm start: prints both curves. Asserts nothing
/// beyond finishing: our Auto differs on purpose.
#[test]
fn auto_compared_with_barotrauma() {
    let mut ours = comparable_reactor();
    ours.set_mode(Mode::Auto);
    let mut theirs = oracle_for(&ours);
    theirs.auto_temp = true;
    let dt = DT;
    for _ in 0..1200 {
        ours.step(dt, 500.0);
        theirs.step(dt, 500.0);
    }
    println!("load drop 500 -> 150 kW at t = 0 (both on Auto)");
    println!(
        "{:>6} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "t", "temp us", "temp baro", "fis us", "fis baro", "out us", "out baro"
    );
    for k in 0..1200 {
        ours.step(dt, 150.0);
        theirs.step(dt, 150.0);
        if (k + 1) % 10 == 0 {
            println!(
                "{:>6.1} {:>9.2} {:>9.2} {:>9.2} {:>9.2} {:>9.1} {:>9.1}",
                (k + 1) as f64 * dt,
                ours.temperature(),
                theirs.temp,
                ours.fission(),
                theirs.fission,
                ours.output_kw(),
                theirs.output
            );
        }
    }
}

/// One named script on the warmed room, with its rows every `sample_every_s`, and the room.
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

/// The warm-up leaves the reactor on, running on the grid's 500 kW, in its optimal band.
#[test]
fn warm_room_runs_on_the_grid_in_the_band() {
    let room = warm_room(&shipped(), 7);
    let r = room.reactor();
    assert!(r.power_on() && !r.melted());
    assert!(
        (r.output_kw() - 500.0).abs() < 1.0,
        "output {}",
        r.output_kw()
    );
    assert!(
        r.bands().temp_optimal.contains(r.temperature()),
        "temp {}",
        r.temperature()
    );
    assert_eq!(room.flow(), 1.0);
}

/// The load drop overvolts the junction box (it loses condition), and Auto overshoots above the
/// optimal band for a while. No meltdown.
#[test]
fn load_drop_overvolts_the_junction_box_and_auto_overshoots() {
    let (trace, room) = trace_of("load_drop", 5.0);
    assert!(room.junction().condition().value() < 100.0);
    assert!(trace.peak_temp() > 60.0, "peak {}", trace.peak_temp());
    assert!(!trace.melted);
}

/// With no operator action a broken pump takes the flow away, fires come first, and the core melts.
#[test]
fn a_broken_pump_with_no_operator_melts_the_core_after_fires() {
    let (trace, room) = trace_of("pump_fail_full_load", 5.0);
    assert!(trace.melted && room.reactor().melted());
    let fire = trace.first("Fire").expect("a fire");
    let melt = trace.first("Meltdown").expect("a meltdown");
    assert!(fire < melt, "fire at {fire}, meltdown at {melt}");
    assert!(trace
        .rows
        .iter()
        .filter(|r| r.t >= 15.0)
        .all(|r| r.flow < 1e-9));
}

/// With no operator a broken pump at full load takes the core to the critical band, and the core
/// is not cooled from then on (the alarm is the first sign; the fires and the melt follow).
#[test]
fn a_broken_pump_at_full_load_reaches_temp_critical() {
    let (trace, _) = trace_of("pump_fail_full_load", 5.0);
    let t = trace.first("Alarm TempCritical on").expect("TempCritical");
    assert!(t > 10.0 && t < 300.0, "TempCritical at {t}");
    assert!(trace.melted);
}

/// The Low pump runs at its low speed from 10 s on, the flow follows it, and the tank drains.
#[test]
fn a_low_pump_at_full_load_runs_at_its_low_speed_and_drains_slowly() {
    let (trace, room) = trace_of("pump_low_full_load", 5.0);
    let at = row_at(&trace, 100.0);
    assert_eq!(at.pump_mode, PumpMode::Low);
    assert!((at.pump_speed - 0.5).abs() < 0.01, "speed {}", at.pump_speed);
    assert!((at.flow - 0.5).abs() < 0.01, "flow {}", at.flow);
    assert!(at.tank > 0.0 && at.tank < 100.0, "tank {}", at.tank);
    assert!(!trace.melted, "the Low pump at full load keeps the core alive");
    assert_eq!(room.pump().mode(), PumpMode::Low);
}

/// An empty tank is no flow for the rest of the run, and the core melts.
#[test]
fn an_empty_tank_stops_the_flow_and_the_core_melts() {
    let (trace, room) = trace_of("coolant_empty", 5.0);
    assert_eq!(room.pump().tank(), 0.0);
    assert!(trace
        .rows
        .iter()
        .filter(|r| r.t >= 15.0)
        .all(|r| r.flow == 0.0));
    assert!(trace.melted);
}

/// The scram fires once, on the first step the meltdown timer is above 0.9, and the reactor is off
/// after it.
#[test]
fn the_scram_fires_once_when_the_meltdown_timer_passes_nine_tenths() {
    let (trace, _) = trace_of("scram_last_second", DT);
    let at: Vec<usize> = trace
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.events.iter().any(|e| e == "do Scram"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(at.len(), 1, "scrams at rows {at:?}");
    let i = at[0];
    assert!(
        trace.rows[i].melt > 0.9 && trace.rows[i - 1].melt <= 0.9,
        "timer {} then {}",
        trace.rows[i - 1].melt,
        trace.rows[i].melt
    );
    assert!(!trace.rows[i].power);
}

/// After the repair and the restart at melt + 5 s the reactor runs on the grid again (the pump
/// breaks again later, so the core melts once more before the end).
#[test]
fn a_repaired_and_restarted_core_runs_again() {
    let (trace, room) = trace_of("meltdown_repair_restart", 5.0);
    assert!(trace.first("Meltdown").is_some());
    let at = row_at(&trace, 150.0);
    assert!(at.power && at.output_kw > 0.0, "{at:?}");
    assert!(room.reactor().melted());
}

/// A meltdown spends every rod.
#[test]
fn a_meltdown_spends_every_rod() {
    let (trace, room) = trace_of("turbine_max_fission_max", 5.0);
    assert!(trace.melted);
    assert!(!room.reactor().rods().is_empty() && room.reactor().rods().iter().all(|r| r.spent()));
}

/// Auto on the shipped tuning with the load wandering for 300 s: no TempCritical, no meltdown. Also
/// reports the peak temperature, the seconds the box is overvolted and its condition at the end.
#[test]
fn wandering_load_stays_out_of_temp_critical_over_300_s() {
    // The warm-up leaves about 75 % in the tank, and High (`pump_use_high`) runs it dry before
    // 300 s: the run starts with a full tank, as the test was written for.
    let script = scripts().into_iter().find(|s| s.name == "wandering_load").expect("the script");
    let mut room = warm_room(&shipped(), 7);
    room.pump_mut().set_tank(100.0);
    let trace = run(&mut room, &script, DT, 5.0);
    assert!(!trace.melted);
    assert_eq!(trace.count("Alarm TempCritical on"), 0, "{}", trace.table());
    // The same run again, stepped by hand, to count the seconds of overvoltage.
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
