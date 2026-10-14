//! Rules of #286 for the reactor, with numbers.
use super::*;

const DT: f64 = 1.0 / 30.0;

fn reactor(skill: f64) -> Reactor {
    let mut r = Reactor::new(ReactorTuning::default(), 3);
    // Two rods: the pump's cooling offset needs the fuel for the optimal band.
    r.insert_rod(FuelRod::standard()).unwrap();
    r.insert_rod(FuelRod::standard()).unwrap();
    r.set_operator_skill(skill);
    assert!(r.set_power(true));
    r
}

/// Runs `secs` at a constant load; returns every event with its time.
fn run(r: &mut Reactor, secs: f64, load: f64) -> Vec<(f64, ReactorEvent)> {
    let mut out = Vec::new();
    let mut t = 0.0;
    for _ in 0..(secs / DT).round() as usize {
        t += DT;
        out.extend(r.step(DT, load).into_iter().map(|e| (t, e)));
    }
    out
}

fn first(ev: &[(f64, ReactorEvent)], f: impl Fn(&ReactorEvent) -> bool) -> Option<f64> {
    ev.iter().find(|(_, e)| f(e)).map(|(t, _)| *t)
}

#[test]
fn auto_holds_a_stable_optimal_temperature() {
    let mut r = reactor(50.0);
    let load = 500.0;
    let ev = run(&mut r, 240.0, load);
    assert!(!ev.iter().any(|(_, e)| matches!(e, ReactorEvent::Fire { .. })), "{ev:?}");
    // The last minute: temperature and output.
    let (mut tmin, mut tmax, mut omin, mut omax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for _ in 0..(60.0 / DT) as usize {
        r.step(DT, load);
        tmin = tmin.min(r.temperature());
        tmax = tmax.max(r.temperature());
        omin = omin.min(r.output_kw());
        omax = omax.max(r.output_kw());
    }
    let b = r.bands();
    println!(
        "auto, load {load} kW, skill 50: temp {tmin:.2}..{tmax:.2} (optimal {:.1}..{:.1}), output {omin:.1}..{omax:.1} kW, fission {:.1}, turbine {:.1}",
        b.temp_optimal.lo, b.temp_optimal.hi, r.fission(), r.turbine()
    );
    assert!(b.temp_optimal.contains(tmin) && b.temp_optimal.contains(tmax));
    assert!(tmax - tmin < 2.0, "temperature swings {:.2}", tmax - tmin);
    assert!((omin - load).abs() < 0.03 * load && (omax - load).abs() < 0.03 * load);
}

#[test]
fn auto_follows_a_load_change() {
    let mut r = reactor(50.0);
    run(&mut r, 180.0, 300.0);
    let ev = run(&mut r, 180.0, 800.0);
    assert!(!ev.iter().any(|(_, e)| matches!(e, ReactorEvent::Fire { .. })), "{ev:?}");
    println!("auto after a step 300 -> 800 kW: output {:.1} kW, temp {:.2}", r.output_kw(), r.temperature());
    assert!((r.output_kw() - 800.0).abs() < 25.0);
}

#[test]
fn overheating_starts_a_fire_and_nothing_shuts_the_reactor_down() {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 30.0);
    assert_eq!(r.mode(), Mode::Manual);
    let ev = run(&mut r, 130.0, 500.0);
    let b = r.bands();
    let high = first(&ev, |e| *e == ReactorEvent::Alarm { kind: AlarmKind::TempHigh, on: true }).unwrap();
    let fire = first(&ev, |e| matches!(e, ReactorEvent::Fire { .. })).expect("a fire");
    let crit = first(&ev, |e| *e == ReactorEvent::Alarm { kind: AlarmKind::TempCritical, on: true }).unwrap();
    let melt = first(&ev, |e| *e == ReactorEvent::Meltdown).expect("a meltdown");
    println!(
        "manual fission 100, turbine 30: above {:.0} at {high:.1} s, fire at {fire:.1} s, above {:.0} at {crit:.1} s, meltdown at {melt:.1} s",
        b.temp_optimal.hi, b.temp_allowed.hi
    );
    assert!((fire - high - 20.0).abs() < 0.2);
    assert!((melt - crit - 120.0).abs() < 0.2);
    assert!(fire < melt);
    // No automatic shutdown: the reactor stays on, hot, until it melts.
    assert!(r.melted() && !r.power_on() && r.output_kw() == 0.0);
}

#[test]
fn scram_drops_fission_and_power_at_once_and_the_temperature_falls() {
    let mut r = reactor(50.0);
    run(&mut r, 120.0, 500.0);
    assert!(r.output_kw() > 400.0);
    r.scram();
    assert!(!r.power_on());
    assert_eq!(r.fission(), 0.0);
    r.step(DT, 500.0);
    assert_eq!(r.output_kw(), 0.0, "output at once");
    let hot = r.temperature();
    // The glue sends its manual setting every step: with the power off it does not bring fission back.
    r.set_targets(80.0, 50.0);
    run(&mut r, 5.0, 500.0);
    assert!(r.fission() < 1e-6, "fission {}", r.fission());
    run(&mut r, 60.0, 500.0);
    println!("scram at {hot:.1}: 60 s later {:.2}", r.temperature());
    assert!(r.temperature() < hot - 20.0 && r.temperature() < 5.0);
    assert_eq!(r.output_kw(), 0.0);
    assert!(!r.melted());
}

#[test]
fn after_a_scram_the_power_switch_starts_it_again() {
    let mut r = reactor(50.0);
    run(&mut r, 60.0, 300.0);
    r.scram();
    run(&mut r, 5.0, 300.0);
    assert!(r.set_power(true), "no lockout after a scram");
    let ev = run(&mut r, 120.0, 300.0);
    assert!(!ev.iter().any(|(_, e)| matches!(e, ReactorEvent::Fire { .. } | ReactorEvent::Meltdown)));
    assert!((r.output_kw() - 300.0).abs() < 15.0, "back on the load: {:.1}", r.output_kw());
}

#[test]
fn left_hot_it_melts_down() {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 30.0);
    let ev = run(&mut r, 200.0, 500.0);
    let crit = first(&ev, |e| *e == ReactorEvent::Alarm { kind: AlarmKind::TempCritical, on: true }).unwrap();
    let melt = first(&ev, |e| *e == ReactorEvent::Meltdown).expect("a meltdown");
    println!("critical at {crit:.1} s, meltdown at {melt:.1} s");
    assert!((melt - crit - 120.0).abs() < 0.2);
    assert!(!r.condition().functioning() && r.rods().iter().all(|r| r.spent()));
    assert!(r.melted());
    assert!(!r.set_power(true));
    assert_eq!(r.output_kw(), 0.0);
}

#[test]
fn high_skill_widens_the_safe_band() {
    let lo = Bands::new(0.0, 50.0, 80.0);
    let hi = Bands::new(100.0, 50.0, 80.0);
    println!(
        "skill 0: optimal {:.0}..{:.0}, allowed {:.0}..{:.0}; skill 100: optimal {:.0}..{:.0}, allowed {:.0}..{:.0}",
        lo.temp_optimal.lo, lo.temp_optimal.hi, lo.temp_allowed.lo, lo.temp_allowed.hi, hi.temp_optimal.lo, hi.temp_optimal.hi, hi.temp_allowed.lo, hi.temp_allowed.hi
    );
    assert!(hi.temp_optimal.hi > lo.temp_optimal.hi && hi.temp_allowed.hi > lo.temp_allowed.hi);
    assert!(hi.turbine_optimal.hi - hi.turbine_optimal.lo > lo.turbine_optimal.hi - lo.turbine_optimal.lo);
    // The same setting (equilibrium near 67) starts fires for an unskilled operator only.
    let mut fires = Vec::new();
    for skill in [0.0, 100.0] {
        let mut r = reactor(skill);
        r.set_targets(49.0, 50.0);
        let ev = run(&mut r, 120.0, 500.0);
        fires.push(ev.iter().filter(|(_, e)| matches!(e, ReactorEvent::Fire { .. })).count());
        println!("skill {skill}: temp {:.1}, fires in 120 s: {}", r.temperature(), fires.last().unwrap());
    }
    assert!(fires[0] > 0 && fires[1] == 0);
}

#[test]
fn low_condition_speeds_up_the_fire_timer() {
    let mut times = Vec::new();
    for cond in [100.0, 30.0] {
        let mut r = reactor(50.0);
        r.condition_mut().set_value(cond);
        r.set_targets(100.0, 30.0);
        let ev = run(&mut r, 60.0, 500.0);
        let high = first(&ev, |e| *e == ReactorEvent::Alarm { kind: AlarmKind::TempHigh, on: true }).unwrap();
        let fire = first(&ev, |e| matches!(e, ReactorEvent::Fire { .. })).unwrap();
        times.push(fire - high);
    }
    println!("fire after overheating: {:.1} s at condition 100, {:.1} s at 30", times[0], times[1]);
    assert!((times[1] - 20.0 / 1.7).abs() < 0.2);
}

#[test]
fn fuel_burns_down_and_runs_out() {
    let mut r = reactor(50.0);
    // Fission 43.75 with turbine 50 holds the temperature (the pump's offset takes 40).
    r.set_targets(43.75, 50.0);
    run(&mut r, 100.0, 500.0);
    let life = r.rods()[0].life;
    // About fission 43.75 for 100 s (minus the ramp): 0.0875 per s.
    println!("rod life after 100 s at fission 43.75: {life:.2}");
    assert!(life > 90.5 && life < 92.0);
    r.remove_rod(0);
    r.remove_rod(0);
    r.insert_rod(FuelRod { heat: 80.0, life: 1.0 }).unwrap();
    let ev = run(&mut r, 60.0, 300.0);
    assert!(ev.iter().any(|(_, e)| *e == ReactorEvent::Alarm { kind: AlarmKind::FuelLow, on: true }));
    assert!(r.rods()[0].spent());
    run(&mut r, 30.0, 300.0);
    println!("spent: fission {:.2}, temp {:.2}, output {:.1}", r.fission(), r.temperature(), r.output_kw());
    assert!(r.fission() < 0.1 && r.output_kw() < 1.0);
    assert_eq!(r.insert_rod(FuelRod::standard()), Ok(()));
    let ev = run(&mut r, 1.0, 300.0);
    assert!(ev.iter().any(|(_, e)| *e == ReactorEvent::Alarm { kind: AlarmKind::FuelLow, on: false }));
}

#[test]
fn output_stays_in_its_range() {
    let mut r = reactor(50.0);
    r.set_targets(70.0, 40.0);
    run(&mut r, 60.0, 400.0);
    let (lo, hi) = r.output_range();
    println!("turbine {:.1}, temp {:.1}: range {lo:.1}..{hi:.1} kW", r.turbine(), r.temperature());
    r.step(DT, 50.0);
    assert!((r.output_kw() - r.output_range().0).abs() < 1e-6, "low load gives the minimum");
    r.step(DT, 5000.0);
    assert!((r.output_kw() - r.output_range().1).abs() < 1e-6, "high load gives the maximum");
    // A cold reactor gives nothing whatever the turbine.
    let mut cold = Reactor::new(ReactorTuning::default(), 1);
    cold.set_targets(0.0, 100.0);
    cold.step(DT, 500.0);
    assert_eq!(cold.output_kw(), 0.0);
}

#[test]
fn alarms_do_not_flicker() {
    let mut r = reactor(50.0);
    let line = r.bands().temp_optimal.hi;
    let mut ev = Vec::new();
    // Temperature wobbling ±1 around the line.
    for i in 0..200 {
        r.temp = line + if i % 2 == 0 { 1.0 } else { -1.0 };
        r.update_alarms(&mut ev);
    }
    let n = ev.iter().filter(|e| matches!(e, ReactorEvent::Alarm { kind: AlarmKind::TempHigh, .. })).count();
    assert_eq!(n, 1, "{ev:?}");
    r.temp = line - 3.5;
    ev.clear();
    r.update_alarms(&mut ev);
    assert_eq!(ev, vec![ReactorEvent::Alarm { kind: AlarmKind::TempHigh, on: false }]);
    assert_eq!(AlarmKind::TempHigh.notice().key.0, "reactor.temp_high");
}

#[test]
fn slots_are_limited() {
    let mut r = reactor(0.0);
    for _ in 0..2 {
        r.insert_rod(FuelRod::standard()).unwrap();
    }
    assert!(r.insert_rod(FuelRod::standard()).is_err());
    assert_eq!(r.available_fuel(), 320.0);
}
