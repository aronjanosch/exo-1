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
    let ev = run(&mut r, 140.0, 500.0);
    let b = r.bands();
    let high = first(&ev, |e| *e == ReactorEvent::Alarm { kind: AlarmKind::TempHigh, on: true }).unwrap();
    let fire = first(&ev, |e| matches!(e, ReactorEvent::Fire { .. })).expect("a fire");
    let crit = first(&ev, |e| *e == ReactorEvent::Alarm { kind: AlarmKind::TempCritical, on: true }).unwrap();
    let melt = first(&ev, |e| *e == ReactorEvent::Meltdown).expect("a meltdown");
    println!(
        "manual fission 100, turbine 30: above {:.0} at {high:.1} s, fire at {fire:.1} s, above {:.0} at {crit:.1} s, meltdown at {melt:.1} s",
        b.temp_optimal.hi, b.temp_allowed.hi
    );
    assert!((fire - high - 30.0).abs() < 0.2);
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
    assert!(!r.condition().functioning() && r.rods().iter().all(|r| !r.spent()), "the rods keep their life");
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
    // The same setting starts fires for an unskilled operator only.
    let mut fires = Vec::new();
    for skill in [0.0, 100.0] {
        let mut r = reactor(skill);
        // Fission 47.5 with the output's heat: equilibrium near 67.
        r.set_targets(47.5, 50.0);
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
    assert!((times[1] - 30.0 / 1.7).abs() < 0.2);
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
    r.insert_rod(FuelRod { kind: "standard".into(), heat: 80.0, life: 1.0 }).unwrap();
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

/// The shipped values (`reactor.json`), for the tests that check the shipped tuning.
fn shipped() -> ReactorTuning {
    crate::tuning::Tuning::parse(include_str!("../../../../content/gameplay/reactor/reactor.json")).unwrap().reactor
}

/// The temperature is a damped second-order system: a step in fission overshoots its final rise by
/// 10..40 % (about a quarter for the shipped values) and is back within 2 % of it after 60 s.
#[test]
fn a_step_in_fission_overshoots_and_settles() {
    let mut r = reactor(50.0);
    r.set_targets(40.0, 50.0);
    run(&mut r, 300.0, 500.0);
    let before = r.temperature();
    // The final value: the new setting held long enough, on a copy.
    let mut end = r.clone();
    end.set_targets(45.0, 50.0);
    run(&mut end, 600.0, 500.0);
    let fin = end.temperature();
    let rise = fin - before;
    assert!(rise > 5.0, "a step must change the temperature: {rise:.2}");
    r.set_targets(45.0, 50.0);
    let mut peak = before;
    let mut at_60 = f64::NAN;
    for k in 1..=(120.0 / DT).round() as usize {
        r.step(DT, 500.0);
        peak = peak.max(r.temperature());
        if k == (60.0 / DT).round() as usize {
            at_60 = r.temperature();
        }
    }
    let over = (peak - fin) / rise;
    println!("step in fission: rise {rise:.2}, peak over the final by {:.0} %, at 60 s {at_60:.2} (final {fin:.2})", over * 100.0);
    assert!((0.10..=0.40).contains(&over), "overshoot {over:.2} of the rise");
    assert!((at_60 - fin).abs() < 0.02 * rise, "at 60 s {at_60:.2}, final {fin:.2}");
}

/// Cutting fission by hand is not a jump: the temperature falls over seconds.
#[test]
fn a_correction_takes_seconds() {
    let mut r = reactor(50.0);
    r.set_targets(40.0, 50.0);
    run(&mut r, 300.0, 500.0);
    let start = r.temperature();
    r.set_targets(0.0, 50.0);
    run(&mut r, 2.0, 500.0);
    let fell_2 = start - r.temperature();
    assert!(fell_2 < 3.0, "fell {fell_2:.2} degrees in 2 s");
    run(&mut r, 28.0, 500.0);
    let fell_30 = start - r.temperature();
    println!("fission cut to 0: fell {fell_2:.2} in 2 s, {fell_30:.2} in 30 s");
    assert!(fell_30 > 10.0, "fell {fell_30:.2} degrees in 30 s");
}

/// Output makes heat: at the same fission and turbine, the reactor that delivers more settles
/// hotter by `heat_output_factor` times its output difference as a share of the maximum. (The
/// output follows the turbine within its tolerance, so the difference is a few per cent of the
/// maximum, not the whole factor.)
#[test]
fn output_makes_heat() {
    let mut settled = Vec::new();
    for load in [1000.0, 0.0] {
        let mut r = reactor(50.0);
        r.set_targets(40.0, 50.0);
        run(&mut r, 400.0, load);
        settled.push((r.temperature(), r.output_kw()));
    }
    let (hot, cold) = (settled[0], settled[1]);
    let t = ReactorTuning::default();
    let expect = t.heat_output_factor * (hot.1 - cold.1) / t.max_output_kw;
    let diff = hot.0 - cold.0;
    println!("output {:.1} kW vs {:.1} kW: temperature {:.2} vs {:.2}, expected difference {expect:.2}", hot.1, cold.1, hot.0, cold.0);
    assert!(hot.1 > cold.1 + 10.0, "the outputs differ: {hot:?} {cold:?}");
    assert!((diff - expect).abs() < 0.05 * expect, "difference {diff:.2}, expected {expect:.2}");
}

/// Full turbine with full fission overheats the shipped reactor into the allowed band's top.
#[test]
fn turbine_max_fission_max_overheats() {
    let mut r = Reactor::new(shipped(), 3);
    r.insert_rod(FuelRod::standard()).unwrap();
    r.insert_rod(FuelRod::standard()).unwrap();
    r.set_operator_skill(50.0);
    assert!(r.set_power(true));
    r.set_targets(100.0, 100.0);
    let top = r.bands().temp_allowed.hi;
    let mut reached = None;
    for k in 1..=(120.0 / DT).round() as usize {
        r.step(DT, 500.0);
        if r.temperature() > top {
            reached = Some(k as f64 * DT);
            break;
        }
    }
    println!("shipped, fission 100 turbine 100: above {top:.0} at {reached:?} s");
    assert!(reached.is_some_and(|t| t <= 120.0), "temperature {:.1} after 120 s", r.temperature());
}

/// The emergency brake (Barotrauma has no SCRAM; pulling a rod is it). The pull returns the rod
/// and the core's temperature at that moment, and the fuel it allowed is gone in the same step:
/// fission is capped to what is left, and the next step's heat already uses the new fuel.
#[test]
fn pulling_a_rod_cuts_the_fuel_in_the_same_step() {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 50.0);
    run(&mut r, 60.0, 500.0);
    assert!(r.fission() > 80.0, "fission {:.1}", r.fission());
    let temp = r.temperature();
    assert!(r.pull_rod(9).is_none());
    let (rod, handled) = r.pull_rod(0).expect("a rod in slot 0");
    assert_eq!(rod.kind, "standard");
    assert_eq!(handled, temp);
    assert_eq!(r.available_fuel(), 80.0);
    let f = r.fission();
    assert!(f <= 80.0 + 1e-9, "fission {f:.2} right after the pull");
    r.step(DT, 500.0);
    assert!((r.heat_input() - f * 0.8 * 2.0).abs() < 1e-9, "heat {:.3} for fission {f:.2}", r.heat_input());
}

/// Barotrauma's advice (Reactor.cs:504-543, NeedMoreFuel(1.0) first, then TooMuchFuel).
#[test]
fn fuel_need_is_more_when_the_rods_are_gone_and_the_grid_asks() {
    let mut r = Reactor::new(ReactorTuning::default(), 3);
    assert!(r.set_power(true));
    r.step(DT, 500.0);
    assert_eq!(r.fuel_need(), FuelNeed::More);
    let mut idle = Reactor::new(ReactorTuning::default(), 3);
    idle.step(DT, 0.0);
    assert_eq!(idle.fuel_need(), FuelNeed::Ok);
}

#[test]
fn one_standard_rod_is_enough_for_half_load() {
    let mut r = Reactor::new(ReactorTuning::default(), 3);
    r.insert_rod(FuelRod::standard()).unwrap();
    assert!(r.set_power(true));
    r.step(DT, 500.0);
    assert_eq!(r.fuel_need(), FuelNeed::Ok);
}

#[test]
fn fuel_need_is_less_with_more_rods_than_the_load_needs() {
    let mut r = Reactor::new(ReactorTuning::default(), 3);
    for _ in 0..4 {
        r.insert_rod(FuelRod::standard()).unwrap();
    }
    // The turbine for 500 kW of a 1000 kW reactor.
    r.turbine_for_load = 50.0;
    // Four rods: the fission cap is 100, so no more is needed; the low end of the optimal fission
    // band makes 25 * 320 / 100 * 2 = 160 heat, above 1.5 * 50 = 75: too much.
    assert_eq!(r.fuel_need(), FuelNeed::Less);
}

/// Fuel use is proportional to fission: twice the fission burns twice the life.
#[test]
fn burn_rate_doubles_when_fission_doubles() {
    let burnt = |fission: f64| {
        let mut r = reactor(50.0);
        r.set_targets(fission, 50.0);
        r.fission = fission;
        let before = r.rods()[0].life;
        run(&mut r, 10.0, 500.0);
        before - r.rods()[0].life
    };
    let (low, high) = (burnt(40.0), burnt(80.0));
    println!("burnt in 10 s: {low:.3} at fission 40, {high:.3} at 80");
    assert!(low > 0.5 && (high / low - 2.0).abs() < 0.01, "{high} vs {low}");
}

/// A spent rod gives no heat: fission has nothing to take and the heat is zero.
#[test]
fn a_spent_rod_makes_no_heat() {
    let mut r = Reactor::new(ReactorTuning::default(), 3);
    r.insert_rod(FuelRod { kind: "standard".into(), heat: 80.0, life: 0.0 }).unwrap();
    assert!(r.set_power(true));
    r.set_targets(100.0, 50.0);
    run(&mut r, 10.0, 500.0);
    assert_eq!(r.available_fuel(), 0.0);
    assert!(r.fission() < 1e-9 && r.heat_input() < 1e-9, "fission {} heat {}", r.fission(), r.heat_input());
}

#[test]
fn a_melted_reactor_is_restarted_by_a_full_repair() {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 30.0);
    let ev = run(&mut r, 200.0, 500.0);
    assert!(first(&ev, |e| *e == ReactorEvent::Meltdown).is_some());
    assert!(r.melted() && !r.set_power(true), "broken and cold after the meltdown");
    // The rods keep their life (the core is gone, the fuel is not).
    assert!(r.rods().iter().all(|rod| rod.life > 0.0), "{:?}", r.rods());
    // Repaired from zero to full: the melted flag clears and the switch works again.
    r.condition_mut().restore(100.0);
    run(&mut r, 1.0, 0.0);
    assert!(!r.melted(), "the screen goes back to the normal view");
    assert!(r.set_power(true), "power can be switched on");
    assert_eq!(r.fission(), 0.0, "the core restarts cold");
    r.set_mode(Mode::Auto);
    let ev = run(&mut r, 240.0, 300.0);
    assert!(!ev.iter().any(|(_, e)| matches!(e, ReactorEvent::Meltdown | ReactorEvent::Fire { .. })), "{ev:?}");
    println!("restarted after a meltdown: fission {:.1}, output {:.1} kW, temp {:.1}", r.fission(), r.output_kw(), r.temperature());
    assert!(r.fission() > 0.0 && (r.output_kw() - 300.0).abs() < 15.0, "back on the load: {:.1}", r.output_kw());
}

#[test]
fn scram_without_coolant_lets_the_core_cool_below_the_allowed_band() {
    // The pump is gone and the flow has run down; the core runs critical; then SCRAM.
    let mut r = reactor(50.0);
    r.set_coolant(0.0);
    r.coolant = 0.0;
    r.set_targets(100.0, 30.0);
    let mut ev = Vec::new();
    while !r.alarm(AlarmKind::TempCritical) {
        ev.extend(run(&mut r, 1.0, 500.0));
        assert!(r.temperature() < 200.0 && !r.melted(), "no critical line reached");
    }
    run(&mut r, 20.0, 500.0);
    let (_, timer_at_scram) = r.timers();
    assert!(timer_at_scram > 0.1, "the meltdown timer is running: {timer_at_scram}");
    r.scram();
    let ev = run(&mut r, 15.0, 500.0);
    let b = r.bands();
    assert!(r.temperature() < b.temp_allowed.hi, "temperature {:.1} after 15 s, allowed up to {:.1}", r.temperature(), b.temp_allowed.hi);
    let (_, timer_now) = r.timers();
    assert!(timer_now < timer_at_scram, "the meltdown timer counts down: {timer_at_scram:.3} -> {timer_now:.3}");
    assert!(!ev.iter().any(|(_, e)| *e == ReactorEvent::Meltdown) && !r.melted());
}

// ---------- temperature boost (Barotrauma Reactor.cs 14, 52, 327, 502) ----------

/// A reactor on with two rods, fission and turbine held at 25 by hand, settled.
fn settled_boost_test_reactor() -> Reactor {
    let mut r = reactor(50.0);
    r.set_targets(25.0, 25.0);
    run(&mut r, 200.0, 500.0);
    assert!(!r.melted(), "the settling must not melt the core");
    r
}

/// The boost is heat: the heat of one step goes up by exactly the amount, and it fades to 0 at
/// one degree per second.
#[test]
fn a_boost_adds_its_amount_to_the_heat_and_fades_to_zero_in_25_s() {
    let mut r = settled_boost_test_reactor();
    r.step(DT, 500.0);
    let before = r.heat_input();
    assert!(r.boost(true));
    r.step(DT, 500.0);
    let boosted = r.heat_input();
    assert!((boosted - before - 25.0).abs() < 1e-6, "{before} -> {boosted}");

    let mut r = reactor(50.0);
    r.set_targets(25.0, 25.0);
    assert!(r.boost(true));
    assert_eq!(r.temperature_boost(), 25.0);
    run(&mut r, 10.0, 500.0);
    assert!((r.temperature_boost() - 15.0).abs() < 1e-6, "{}", r.temperature_boost());
    run(&mut r, 15.0, 500.0);
    assert!(r.temperature_boost().abs() < 1e-9, "{}", r.temperature_boost());
    run(&mut r, 5.0, 500.0);
    assert_eq!(r.temperature_boost(), 0.0, "it never overshoots past 0");
}

/// The minus boost is the same, with the sign turned, and it fades up to 0.
#[test]
fn a_minus_boost_fades_up_to_zero() {
    let mut r = reactor(50.0);
    assert!(r.boost(false));
    assert_eq!(r.temperature_boost(), -25.0);
    run(&mut r, 25.0, 500.0);
    assert!(r.temperature_boost().abs() < 1e-9, "{}", r.temperature_boost());
}

/// Barotrauma's AllowTemperatureBoost: a second boost is refused while |boost| >= 22.5, and the
/// refused one leaves the boost as it was.
#[test]
fn a_second_boost_is_refused_while_the_boost_is_at_least_90_percent_of_its_amount() {
    let mut r = reactor(50.0);
    assert!(r.boost(true));
    assert!(!r.boost_allowed());
    assert!(!r.boost(false), "refused at 25");
    assert_eq!(r.temperature_boost(), 25.0);
    run(&mut r, 2.0, 500.0);
    assert!(!r.boost(true), "refused at 23");
    run(&mut r, 1.0, 500.0);
    assert!(r.boost_allowed(), "allowed at 22");
    assert!(r.boost(false));
    assert_eq!(r.temperature_boost(), -25.0);
}

/// With the power off, or the reactor broken, a boost does nothing: refused, the boost stays 0.
#[test]
fn a_boost_does_nothing_while_the_power_is_off_or_the_reactor_is_broken() {
    let mut r = Reactor::new(ReactorTuning::default(), 3);
    r.insert_rod(FuelRod::standard()).unwrap();
    assert!(!r.boost(true), "power off");
    assert_eq!(r.temperature_boost(), 0.0);
    assert!(r.set_power(true));
    r.condition_mut().set_value(0.0);
    assert!(!r.boost(true), "broken");
    assert_eq!(r.temperature_boost(), 0.0);
}

// ---------- warning lamps (client Reactor.cs 621-632) ----------

/// A reactor on, with every lamp off: 500 kW asked, output 500, fission 50 on 160 fuel, the core
/// at 50 (inside the skill-50 optimal band, 35..65).
fn calm() -> Reactor {
    let mut r = reactor(50.0);
    r.temp = 50.0;
    r.output = 500.0;
    r.fission = 50.0;
    r
}

/// The lamps of `calm` after `change`, as their short letters.
fn lamps_after(change: impl FnOnce(&mut Reactor)) -> String {
    let mut r = calm();
    change(&mut r);
    r.warnings(500.0).short()
}

#[test]
fn the_lamps_are_all_off_in_a_calm_reactor() {
    assert_eq!(calm().warnings(500.0), Warnings::default());
    assert_eq!(lamps_after(|_| {}), "");
}

/// Each lamp switches on in its own situation, and only then.
#[test]
fn each_lamp_switches_on_in_its_own_situation() {
    assert_eq!(lamps_after(|r| r.temp = 70.0), "OH", "above 65");
    assert_eq!(lamps_after(|r| r.temp = 20.0), "LT", "below 35");
    assert_eq!(lamps_after(|r| r.output = 560.0), "HO", "above 110 % of 500");
    assert_eq!(lamps_after(|r| r.output = 800.0), "HO CO", "above 150 % too");
    assert_eq!(lamps_after(|r| r.output = 400.0), "LO", "below 90 % of 500");
    assert_eq!(lamps_after(|r| r.fuel = 0.4), "FO LF", "fuel below 1 % of the fission 50");
    assert_eq!(lamps_after(|r| r.fuel = 30.0), "LF", "fuel below the fission only");
    assert_eq!(lamps_after(|r| r.overheat_timer = 0.6 * 120.0), "MD", "meltdown timer over half");
}

/// The lamp lines are strict: exactly on a line the lamp stays off (Barotrauma's > and <).
#[test]
fn the_lamp_lines_are_strict() {
    assert_eq!(lamps_after(|r| r.temp = 65.0), "", "the optimal top itself is no overheat");
    assert_eq!(lamps_after(|r| r.output = 550.0), "", "110 % itself is no high output");
    assert_eq!(lamps_after(|r| r.output = 450.0), "", "90 % itself is no low output");
    assert_eq!(lamps_after(|r| r.fuel = 50.0), "", "fuel equal to the fission is no low fuel");
}

/// With the power off every lamp is off, except the meltdown lamp (its timer part does not need the
/// power, and it is on when the core melted) and the SCRAM lamp (warm with the power off).
#[test]
fn with_the_power_off_only_the_meltdown_and_scram_lamps_can_show() {
    let mut r = calm();
    r.temp = 70.0;
    r.output = 1000.0;
    r.power_on = false;
    assert_eq!(r.warnings(500.0).short(), "SC", "warm and off: SCRAM only");
    r.melted = true;
    assert_eq!(r.warnings(500.0).short(), "MD SC");
}

/// Critical heat (above the allowed band, 80 at skill 50) and cold (below 20) switch on with the
/// power on, and not with it off.
#[test]
fn critical_heat_and_cold_need_the_power_on() {
    assert_eq!(lamps_after(|r| r.temp = 85.0), "OH CH", "above the allowed top 80");
    assert_eq!(lamps_after(|r| r.temp = 80.0), "OH", "exactly on the allowed top is not critical");
    assert_eq!(lamps_after(|r| r.temp = 5.0), "LT CL", "below the allowed bottom 20");
    assert_eq!(lamps_after(|r| r.temp = 20.0), "LT", "exactly on the allowed bottom is not cold");
    assert_eq!(lamps_after(|r| {
        r.power_on = false;
        r.temp = 85.0;
    }), "SC", "off: no critical heat, the SCRAM lamp only");
    assert_eq!(lamps_after(|r| {
        r.power_on = false;
        r.temp = 5.0;
    }), "SC", "off: no cold, the SCRAM lamp only");
}

/// The SCRAM lamp is on while the power is off and the temperature is above 0.1, and on with the
/// power on never.
#[test]
fn scram_lamp_is_on_when_warm_with_the_power_off() {
    let off = |t: f64| lamps_after(|r| {
        r.power_on = false;
        r.temp = t;
    });
    assert_eq!(off(50.0), "SC", "warm and off");
    assert_eq!(off(0.1), "", "0.1 itself is not above 0.1");
    assert_eq!(off(0.0), "", "cold and off: no lamp");
    assert_eq!(lamps_after(|r| r.temp = 50.0), "", "on: no SCRAM lamp");
}

/// Meltdown blast (Barotrauma reactor.xml:45-58): a melt with fuel left blasts right after the
/// `Meltdown` event, with the reactor's `blast` values.
#[test]
fn a_melt_with_fuel_blasts_right_after_the_meltdown() {
    let mut r = reactor(50.0);
    let ev = r.force_meltdown();
    let at = ev.iter().position(|e| *e == ReactorEvent::Meltdown).expect("a meltdown");
    assert_eq!(ev.get(at + 1), Some(&ReactorEvent::Blast(Blast::default())), "{ev:?}");
    assert_eq!(ev.iter().filter(|e| matches!(e, ReactorEvent::Blast(_))).count(), 1);
}

/// Barotrauma's Conditional AvailableFuel > 1: a melt with no fuel, and with exactly `min_fuel`,
/// makes no blast.
#[test]
fn a_melt_without_fuel_makes_no_blast() {
    let mut r = reactor(50.0);
    // pull_rod shifts the list: slot 0 twice takes both rods.
    assert!(r.pull_rod(0).is_some() && r.pull_rod(0).is_some());
    assert_eq!(r.available_fuel(), 0.0);
    let ev = r.force_meltdown();
    assert_eq!(ev, vec![ReactorEvent::Meltdown, ReactorEvent::Device(DeviceEvent::Broke)], "{ev:?}");

    let mut r = Reactor::new(ReactorTuning::default(), 3);
    r.insert_rod(FuelRod { kind: "thin".into(), heat: 1.0, life: 100.0 }).unwrap();
    let ev = r.force_meltdown();
    assert!(!ev.iter().any(|e| matches!(e, ReactorEvent::Blast(_))), "exactly min_fuel is no blast: {ev:?}");
}

/// A melt keeps the rods' life (the initiator's decision): the rods are spent by burning, not by
/// the blast.
#[test]
fn a_melt_keeps_the_rods_life() {
    let mut r = reactor(50.0);
    let before: Vec<f64> = r.rods().iter().map(|x| x.life).collect();
    let fuel = r.available_fuel();
    r.force_meltdown();
    let after: Vec<f64> = r.rods().iter().map(|x| x.life).collect();
    assert_eq!(before, after);
    assert_eq!(r.available_fuel(), fuel);
}

/// PULSAR's core eject (manual 367-369, #300 r22): the core leaves through the bottom, the rods
/// with it, the reactor is dead. Our values; the rules are the ticket's.
fn hot_reactor() -> Reactor {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 100.0);
    run(&mut r, 60.0, 500.0);
    r
}

#[test]
fn eject_resets_the_core_and_takes_the_rods_with_it() {
    let mut r = hot_reactor();
    assert!(r.fission() > 0.0 && r.temperature() > 0.0);
    let ev = r.eject();
    assert_eq!(ev, vec![ReactorEvent::CoreEjected]);
    assert!(r.ejected());
    assert!(r.rods().is_empty(), "the rods leave with the core");
    assert_eq!(r.available_fuel(), 0.0);
    assert_eq!((r.fission(), r.turbine(), r.temperature(), r.output_kw()), (0.0, 0.0, 0.0, 0.0));
    assert_eq!(r.timers(), (0.0, 0.0));
    assert_eq!(r.condition().value(), 0.0, "the reactor is broken");
    assert!(!r.power_on());
    assert!(!r.set_power(true), "no power while the core is out");
}

#[test]
fn eject_is_once_and_refuses_a_second_core_in_the_slots() {
    let mut r = hot_reactor();
    assert_eq!(r.eject(), vec![ReactorEvent::CoreEjected]);
    assert!(r.eject().is_empty(), "a second eject does nothing");
    assert!(r.insert_rod(FuelRod::standard()).is_err(), "no rods go into an empty core");
}

#[test]
fn an_ejected_reactor_never_melts_or_blasts_even_with_manual_100_and_no_coolant() {
    let mut r = hot_reactor();
    r.eject();
    r.set_targets(100.0, 100.0);
    r.set_coolant(0.0);
    let ev = run(&mut r, 600.0, 500.0);
    assert!(!ev.iter().any(|(_, e)| matches!(e, ReactorEvent::Meltdown | ReactorEvent::Blast(_) | ReactorEvent::Fire { .. })), "{ev:?}");
    assert!(!r.melted());
    assert_eq!(r.temperature(), 0.0);
}

#[test]
fn an_ejected_core_is_not_melted_by_a_forced_melt_even_once_repaired_before_the_next_step() {
    let mut r = hot_reactor();
    r.eject();
    r.condition_mut().restore(100.0);
    assert!(r.force_meltdown().is_empty(), "the core is gone: nothing to melt");
    assert!(!r.melted());
}

#[test]
fn a_repair_installs_an_empty_core_that_takes_rods_again() {
    let mut r = hot_reactor();
    r.eject();
    r.condition_mut().restore(100.0);
    run(&mut r, 1.0, 0.0);
    assert!(!r.ejected(), "a repaired reactor has a core again");
    assert!(r.rods().is_empty(), "the new core is empty");
    assert!(r.set_power(true));
    assert!(r.insert_rod(FuelRod::standard()).is_ok(), "the crew loads rods");
    assert_eq!(r.fission(), 0.0, "the new core starts cold");
}

#[test]
fn a_melted_core_that_is_ejected_is_no_longer_melted() {
    let mut r = reactor(50.0);
    r.force_meltdown();
    assert!(r.melted());
    r.eject();
    assert!(!r.melted(), "the core is out: the MELTDOWN lamp goes with it");
    assert!(r.ejected());
}

#[test]
fn stability_is_full_when_the_core_is_cold_and_off() {
    let r = Reactor::new(ReactorTuning::default(), 3);
    assert!(!r.power_on());
    assert_eq!(r.stability(), 1.0);
}

#[test]
fn stability_is_one_minus_the_meltdown_timer_while_it_runs() {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 30.0);
    let mut steps = 0;
    while r.timers().1 < 0.5 {
        r.step(DT, 500.0);
        steps += 1;
        assert!(steps < 30 * 400, "the meltdown timer runs");
    }
    assert!(!r.melted());
    let (_, timer) = r.timers();
    assert!((r.stability() - (1.0 - timer)).abs() < 1e-12, "stability {} timer {timer}", r.stability());
    assert!(r.stability() > 0.0 && r.stability() < 1.0);
}

#[test]
fn stability_is_zero_once_the_core_has_melted() {
    let mut r = reactor(50.0);
    r.force_meltdown();
    assert!(r.melted());
    assert_eq!(r.stability(), 0.0);
}

#[test]
fn stability_is_zero_when_the_core_is_ejected() {
    let mut r = reactor(50.0);
    r.eject();
    assert!(r.ejected());
    assert_eq!(r.stability(), 0.0);
}
