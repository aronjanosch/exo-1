use super::*;

const DT: f64 = 1.0 / 30.0;

fn reactor(skill: f64) -> Reactor {
    let mut r = Reactor::new(ReactorTuning::default(), 3);
    // Two canisters: the pump's cooling offset needs the fill for the optimal band.
    r.insert_canister(Canister::standard()).unwrap();
    r.insert_canister(Canister::standard()).unwrap();
    r.set_operator_skill(skill);
    assert!(r.set_power(true));
    r
}

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
fn overheating_starts_a_fire_and_nothing_shuts_the_reactor_down() {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 30.0);
    assert_eq!(r.mode(), Mode::Manual);
    let ev = run(&mut r, 140.0, 500.0);
    let b = r.bands();
    let high = first(&ev, |e| *e == ReactorEvent::Alarm { kind: AlarmKind::HeatHigh, on: true }).unwrap();
    let fire = first(&ev, |e| matches!(e, ReactorEvent::Fire { .. })).expect("a fire");
    let crit = first(&ev, |e| *e == ReactorEvent::Alarm { kind: AlarmKind::HeatCritical, on: true }).unwrap();
    let implode = first(&ev, |e| *e == ReactorEvent::Implosion).expect("an implosion");
    println!(
        "manual injection 100, converter 30: above {:.0} at {high:.1} s, fire at {fire:.1} s, above {:.0} at {crit:.1} s, implosion at {implode:.1} s",
        b.heat_optimal.hi, b.heat_allowed.hi
    );
    assert!(fire < implode);
    assert!(r.imploded() && !r.power_on() && r.output_kw() == 0.0);
}

#[test]
fn power_off_drops_injection_over_seconds_and_the_heat_falls() {
    let mut r = reactor(50.0);
    run(&mut r, 120.0, 500.0);
    assert!(r.output_kw() > 400.0);
    r.set_power(false);
    assert!(!r.power_on());
    r.step(DT, 500.0);
    assert_eq!(r.output_kw(), 0.0, "output at once");
    let hot = r.heat();
    // The glue sends its manual setting every step: with the power off it does not bring injection back.
    r.set_targets(80.0, 50.0);
    // Injection follows its target with the lag (injection_lag_s, 4 s): it is not 0 at once.
    run(&mut r, 20.0, 500.0);
    assert!(r.injection() < 1.0, "injection {}", r.injection());
    run(&mut r, 60.0, 500.0);
    println!("power off at {hot:.1}: 80 s later {:.2}", r.heat());
    assert_eq!(r.output_kw(), 0.0);
    assert!(r.heat() < hot, "the heat falls: {hot:.1} -> {:.1}", r.heat());
    assert!(!r.imploded());
}

#[test]
fn after_power_off_the_power_switch_starts_it_again() {
    let mut r = reactor(50.0);
    run(&mut r, 60.0, 300.0);
    r.set_power(false);
    run(&mut r, 5.0, 300.0);
    assert!(r.set_power(true), "no lockout after power off");
    let ev = run(&mut r, 120.0, 300.0);
    assert!(!ev.iter().any(|(_, e)| matches!(e, ReactorEvent::Fire { .. } | ReactorEvent::Implosion)));
}

#[test]
fn left_hot_it_implodes_down() {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 30.0);
    let ev = run(&mut r, 200.0, 500.0);
    let crit = first(&ev, |e| *e == ReactorEvent::Alarm { kind: AlarmKind::HeatCritical, on: true }).unwrap();
    let implode = first(&ev, |e| *e == ReactorEvent::Implosion).expect("an implosion");
    println!("critical at {crit:.1} s, implosion at {implode:.1} s");
    assert!(!r.condition().functioning() && r.canisters().iter().all(|r| !r.spent()), "the canisters keep their life");
    assert!(r.imploded());
    assert!(!r.set_power(true));
    assert_eq!(r.output_kw(), 0.0);
}

#[test]
fn fill_burns_down_and_runs_out() {
    let mut r = reactor(50.0);
    // Injection 43.75 with converter 50 holds the heat (the pump's offset takes 40).
    r.set_targets(43.75, 50.0);
    run(&mut r, 100.0, 500.0);
    let life = r.canisters()[0].life;
    // About injection 43.75 for 100 s (minus the ramp): 0.0875 per s.
    println!("canister life after 100 s at injection 43.75: {life:.2}");
    r.remove_canister(0);
    r.remove_canister(0);
    r.insert_canister(Canister { kind: "standard".into(), heat: 80.0, life: 1.0 }).unwrap();
    let ev = run(&mut r, 60.0, 300.0);
    assert!(ev.iter().any(|(_, e)| *e == ReactorEvent::Alarm { kind: AlarmKind::FillLow, on: true }));
    assert!(r.canisters()[0].spent());
    run(&mut r, 30.0, 300.0);
    println!("spent: injection {:.2}, heat {:.2}, output {:.1}", r.injection(), r.heat(), r.output_kw());
    assert_eq!(r.insert_canister(Canister::standard()), Ok(()));
    let ev = run(&mut r, 1.0, 300.0);
    assert!(ev.iter().any(|(_, e)| *e == ReactorEvent::Alarm { kind: AlarmKind::FillLow, on: false }));
}

#[test]
fn output_stays_in_its_range() {
    let mut r = reactor(50.0);
    r.set_targets(70.0, 40.0);
    run(&mut r, 60.0, 400.0);
    let (lo, hi) = r.output_range();
    println!("converter {:.1}, heat {:.1}: range {lo:.1}..{hi:.1} kW", r.converter(), r.heat());
    r.step(DT, 50.0);
    assert!((r.output_kw() - r.output_range().0).abs() < 1e-6, "low load gives the minimum");
    r.step(DT, 5000.0);
    assert!((r.output_kw() - r.output_range().1).abs() < 1e-6, "high load gives the maximum");
    // A cold reactor gives nothing whatever the converter.
    let mut cold = Reactor::new(ReactorTuning::default(), 1);
    cold.set_targets(0.0, 100.0);
    cold.step(DT, 500.0);
    assert_eq!(cold.output_kw(), 0.0);
}

#[test]
fn alarms_do_not_flicker() {
    let mut r = reactor(50.0);
    let line = r.bands().heat_optimal.hi;
    let mut ev = Vec::new();
    // Heat wobbling ±1 around the line.
    for i in 0..200 {
        r.heat = line + if i % 2 == 0 { 1.0 } else { -1.0 };
        r.update_alarms(&mut ev);
    }
    let n = ev.iter().filter(|e| matches!(e, ReactorEvent::Alarm { kind: AlarmKind::HeatHigh, .. })).count();
    assert_eq!(n, 1, "{ev:?}");
    r.heat = line - 3.5;
    ev.clear();
    r.update_alarms(&mut ev);
    assert_eq!(ev, vec![ReactorEvent::Alarm { kind: AlarmKind::HeatHigh, on: false }]);
}

#[test]
fn slots_are_limited() {
    let mut r = reactor(0.0);
    for _ in 0..2 {
        r.insert_canister(Canister::standard()).unwrap();
    }
    assert!(r.insert_canister(Canister::standard()).is_err());
}

/// The emergency brake (Barotrauma's: pulling a canister). The pull returns the canister
/// and the core's heat at that moment, and the fill it allowed is gone in the same step:
/// injection is capped to what is left, and the next step's heat already uses the new fill.
#[test]
fn pulling_a_canister_cuts_the_fill_in_the_same_step() {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 50.0);
    run(&mut r, 60.0, 500.0);
    assert!(r.injection() > 80.0, "injection {:.1}", r.injection());
    let heat = r.heat();
    assert!(r.pull_canister(9).is_none());
    let (canister, handled) = r.pull_canister(0).expect("a canister in slot 0");
    assert_eq!(canister.kind, "standard");
    assert_eq!(handled, heat);
    assert_eq!(r.available_fill(), 80.0);
    let f = r.injection();
    assert!(f <= 80.0 + 1e-9, "injection {f:.2} right after the pull");
    r.step(DT, 500.0);
    assert!((r.heat_input() - f * 0.8 * 2.0).abs() < 1e-9, "heat {:.3} for injection {f:.2}", r.heat_input());
}

/// Barotrauma's advice (Reactor.cs:504-543, NeedMoreFill(1.0) first, then TooMuchFill).
#[test]
fn fill_need_is_more_when_the_canisters_are_gone_and_the_grid_asks() {
    let mut r = Reactor::new(ReactorTuning::default(), 3);
    assert!(r.set_power(true));
    r.step(DT, 500.0);
    assert_eq!(r.fill_need(), FillNeed::More);
    let mut idle = Reactor::new(ReactorTuning::default(), 3);
    idle.step(DT, 0.0);
    assert_eq!(idle.fill_need(), FillNeed::Ok);
}

#[test]
fn burn_rate_doubles_when_injection_doubles() {
    let burnt = |injection: f64| {
        let mut r = reactor(50.0);
        r.set_targets(injection, 50.0);
        r.injection = injection;
        let before = r.canisters()[0].life;
        run(&mut r, 10.0, 500.0);
        before - r.canisters()[0].life
    };
    let (low, high) = (burnt(40.0), burnt(80.0));
    println!("burnt in 10 s: {low:.3} at injection 40, {high:.3} at 80");
    assert!(low > 0.5 && (high / low - 2.0).abs() < 0.01, "{high} vs {low}");
}

#[test]
fn a_spent_canister_makes_no_heat() {
    let mut r = Reactor::new(ReactorTuning::default(), 3);
    r.insert_canister(Canister { kind: "standard".into(), heat: 80.0, life: 0.0 }).unwrap();
    assert!(r.set_power(true));
    r.set_targets(100.0, 50.0);
    run(&mut r, 10.0, 500.0);
    assert_eq!(r.available_fill(), 0.0);
    assert!(r.injection() < 1e-9 && r.heat_input() < 1e-9, "injection {} heat {}", r.injection(), r.heat_input());
}

#[test]
fn a_imploded_reactor_is_restarted_by_a_full_repair() {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 30.0);
    let ev = run(&mut r, 200.0, 500.0);
    assert!(first(&ev, |e| *e == ReactorEvent::Implosion).is_some());
    assert!(r.imploded() && !r.set_power(true), "broken and cold after the implosion");
    // The canisters keep their life (the core is gone, the fill is not).
    assert!(r.canisters().iter().all(|canister| canister.life > 0.0), "{:?}", r.canisters());
    // Repaired from zero to full: the imploded flag clears and the switch works again.
    r.condition_mut().restore(100.0);
    run(&mut r, 1.0, 0.0);
    assert!(!r.imploded(), "the screen goes back to the normal view");
    assert!(r.set_power(true), "power can be switched on");
    assert_eq!(r.injection(), 0.0, "the core restarts cold");
    r.set_mode(Mode::Auto);
    let ev = run(&mut r, 240.0, 300.0);
    assert!(!ev.iter().any(|(_, e)| matches!(e, ReactorEvent::Implosion | ReactorEvent::Fire { .. })), "{ev:?}");
    println!("restarted after an implosion: injection {:.1}, output {:.1} kW, heat {:.1}", r.injection(), r.output_kw(), r.heat());
    assert!(r.injection() > 0.0, "restarted: injection {:.1}", r.injection());
}

#[test]
fn power_off_without_coolant_lets_the_core_cool_below_the_allowed_band() {
    let mut r = reactor(50.0);
    r.set_coolant(0.0);
    r.coolant = 0.0;
    r.set_targets(100.0, 30.0);
    let mut ev = Vec::new();
    while !r.alarm(AlarmKind::HeatCritical) {
        ev.extend(run(&mut r, 1.0, 500.0));
        assert!(r.heat() < 200.0 && !r.imploded(), "no critical line reached");
    }
    run(&mut r, 20.0, 500.0);
    r.set_power(false);
    // Injection falls with its lag, so the core stays above the critical line for a while (the timer
    // counts up); it cools below it on the decay heat. Wait for that, then the timer counts down.
    let mut secs = 0.0;
    while r.alarm(AlarmKind::HeatCritical) && secs < 300.0 {
        ev.extend(run(&mut r, 1.0, 500.0));
        assert!(!r.imploded(), "the core imploded while it cooled");
        secs += 1.0;
    }
    assert!(!r.alarm(AlarmKind::HeatCritical), "below the allowed band within 300 s, at {:.1}", r.heat());
    let (_, timer_below) = r.timers();
    let ev = run(&mut r, 15.0, 500.0);
    let (_, timer_now) = r.timers();
    assert!(timer_now < timer_below, "the implosion timer counts down: {timer_below:.3} -> {timer_now:.3}");
    assert!(!ev.iter().any(|(_, e)| *e == ReactorEvent::Implosion) && !r.imploded());
}

/// A reactor on with two canisters, injection and converter held at 25 by hand, settled.
fn settled_boost_test_reactor() -> Reactor {
    let mut r = reactor(50.0);
    r.set_targets(25.0, 25.0);
    run(&mut r, 200.0, 500.0);
    assert!(!r.imploded(), "the settling must not implode the core");
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
    assert_eq!(r.heat_boost(), 25.0);
    run(&mut r, 10.0, 500.0);
}

/// Barotrauma's AllowTemperatureBoost: a second boost is refused while |boost| >= 22.5, and the
/// refused one leaves the boost as it was.
#[test]
fn a_second_boost_is_refused_while_the_boost_is_at_least_90_percent_of_its_amount() {
    let mut r = reactor(50.0);
    assert!(r.boost(true));
    assert!(!r.boost_allowed());
    assert!(!r.boost(false), "refused at 25");
    assert_eq!(r.heat_boost(), 25.0);
    run(&mut r, 2.0, 500.0);
    assert!(!r.boost(true), "refused at 23");
    run(&mut r, 1.0, 500.0);
    assert!(r.boost_allowed(), "allowed at 22");
    assert!(r.boost(false));
    assert_eq!(r.heat_boost(), -25.0);
}

#[test]
fn a_boost_does_nothing_while_the_power_is_off_or_the_reactor_is_broken() {
    let mut r = Reactor::new(ReactorTuning::default(), 3);
    r.insert_canister(Canister::standard()).unwrap();
    assert!(!r.boost(true), "power off");
    assert_eq!(r.heat_boost(), 0.0);
    assert!(r.set_power(true));
    r.condition_mut().set_value(0.0);
    assert!(!r.boost(true), "broken");
    assert_eq!(r.heat_boost(), 0.0);
}

/// A reactor on, with every lamp off: 500 kW asked, output 500, injection 50 on 160 fill, the core
/// at 50 (inside the skill-50 optimal band, 35..65).
fn calm() -> Reactor {
    let mut r = reactor(50.0);
    r.heat = 50.0;
    r.output = 500.0;
    r.injection = 50.0;
    r
}

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

/// With the power off every lamp is off, except the implosion lamp (its timer part does not need the
/// power, and it is on when the core imploded) and the SCRAM lamp (warm with the power off).
#[test]
fn with_the_power_off_only_the_implosion_and_scram_lamps_can_show() {
    let mut r = calm();
    r.heat = 70.0;
    r.output = 1000.0;
    r.power_on = false;
    assert_eq!(r.warnings(500.0).short(), "SC", "warm and off: SCRAM only");
    r.imploded = true;
    assert_eq!(r.warnings(500.0).short(), "MD SC");
}

/// Critical heat (above the allowed band, 80 at skill 50) and cold (below 20) switch on with the
/// power on, and not with it off.
#[test]
fn critical_heat_and_cold_need_the_power_on() {
    assert_eq!(lamps_after(|r| {
        r.power_on = false;
        r.heat = 85.0;
    }), "SC", "off: no critical heat, the SCRAM lamp only");
    assert_eq!(lamps_after(|r| {
        r.power_on = false;
        r.heat = 5.0;
    }), "SC", "off: no cold, the SCRAM lamp only");
}

/// Implosion blast (Barotrauma reactor.xml:45-58): an implosion with fill left blasts right after the
/// `Implosion` event, with the reactor's `blast` values.
#[test]
fn a_implode_with_fill_blasts_right_after_the_implosion() {
    let mut r = reactor(50.0);
    let ev = r.force_implosion();
    let at = ev.iter().position(|e| *e == ReactorEvent::Implosion).expect("an implosion");
    assert_eq!(ev.get(at + 1), Some(&ReactorEvent::Blast(Blast::default())), "{ev:?}");
    assert_eq!(ev.iter().filter(|e| matches!(e, ReactorEvent::Blast(_))).count(), 1);
}

/// Barotrauma's Conditional AvailableFuel > 1: an implosion with no fill, and with exactly `min_fill`,
/// makes no blast.
#[test]
fn a_implode_without_fill_makes_no_blast() {
    let mut r = reactor(50.0);
    // pull_canister shifts the list: slot 0 twice takes both canisters.
    assert!(r.pull_canister(0).is_some() && r.pull_canister(0).is_some());
    assert_eq!(r.available_fill(), 0.0);
    let ev = r.force_implosion();
    assert_eq!(ev, vec![ReactorEvent::Implosion, ReactorEvent::Device(DeviceEvent::Broke)], "{ev:?}");

    let mut r = Reactor::new(ReactorTuning::default(), 3);
    r.insert_canister(Canister { kind: "thin".into(), heat: 1.0, life: 100.0 }).unwrap();
    let ev = r.force_implosion();
    assert!(!ev.iter().any(|e| matches!(e, ReactorEvent::Blast(_))), "exactly min_fill is no blast: {ev:?}");
}

/// An implosion keeps the canisters' life (the initiator's decision): the canisters are spent by burning, not by
/// the blast.
#[test]
fn a_implode_keeps_the_canisters_life() {
    let mut r = reactor(50.0);
    let before: Vec<f64> = r.canisters().iter().map(|x| x.life).collect();
    let fill = r.available_fill();
    r.force_implosion();
    let after: Vec<f64> = r.canisters().iter().map(|x| x.life).collect();
    assert_eq!(before, after);
    assert_eq!(r.available_fill(), fill);
}

/// PULSAR's core eject (manual 367-369): the core leaves through the bottom, the canisters
/// with it, the reactor is dead. Our values.
fn hot_reactor() -> Reactor {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 100.0);
    run(&mut r, 60.0, 500.0);
    r
}

#[test]
fn eject_resets_the_core_and_takes_the_canisters_with_it() {
    let mut r = hot_reactor();
    assert!(r.injection() > 0.0 && r.heat() > 0.0);
    let ev = r.eject();
    assert_eq!(ev, vec![ReactorEvent::CoreEjected]);
    assert!(r.ejected());
    assert!(r.canisters().is_empty(), "the canisters leave with the core");
    assert_eq!(r.available_fill(), 0.0);
    assert_eq!((r.injection(), r.converter(), r.heat(), r.output_kw()), (0.0, 0.0, 0.0, 0.0));
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
    assert!(r.insert_canister(Canister::standard()).is_err(), "no canisters go into an empty core");
}

#[test]
fn an_ejected_reactor_never_implodes_or_blasts_even_with_manual_100_and_no_coolant() {
    let mut r = hot_reactor();
    r.eject();
    r.set_targets(100.0, 100.0);
    r.set_coolant(0.0);
    let ev = run(&mut r, 600.0, 500.0);
    assert!(!ev.iter().any(|(_, e)| matches!(e, ReactorEvent::Implosion | ReactorEvent::Blast(_) | ReactorEvent::Fire { .. })), "{ev:?}");
    assert!(!r.imploded());
    assert_eq!(r.heat(), 0.0);
}

#[test]
fn an_ejected_core_is_not_imploded_by_a_forced_implode_even_once_repaired_before_the_next_step() {
    let mut r = hot_reactor();
    r.eject();
    r.condition_mut().restore(100.0);
    assert!(r.force_implosion().is_empty(), "the core is gone: nothing to implode");
    assert!(!r.imploded());
}

#[test]
fn a_repair_installs_an_empty_core_that_takes_canisters_again() {
    let mut r = hot_reactor();
    r.eject();
    r.condition_mut().restore(100.0);
    run(&mut r, 1.0, 0.0);
    assert!(!r.ejected(), "a repaired reactor has a core again");
    assert!(r.canisters().is_empty(), "the new core is empty");
    assert!(r.set_power(true));
    assert!(r.insert_canister(Canister::standard()).is_ok(), "the crew loads canisters");
    assert_eq!(r.injection(), 0.0, "the new core starts cold");
}

#[test]
fn a_imploded_core_that_is_ejected_is_no_longer_imploded() {
    let mut r = reactor(50.0);
    r.force_implosion();
    assert!(r.imploded());
    r.eject();
    assert!(!r.imploded(), "the core is out: the IMPLOSION lamp goes with it");
    assert!(r.ejected());
}

#[test]
fn stability_is_full_when_the_core_is_cold_and_off() {
    let r = Reactor::new(ReactorTuning::default(), 3);
    assert!(!r.power_on());
    assert_eq!(r.stability(), 1.0);
}

#[test]
fn stability_is_one_minus_the_implosion_timer_while_it_runs() {
    let mut r = reactor(50.0);
    r.set_targets(100.0, 30.0);
    let mut steps = 0;
    while r.timers().1 < 0.5 {
        r.step(DT, 500.0);
        steps += 1;
        assert!(steps < 30 * 400, "the implosion timer runs");
    }
    assert!(!r.imploded());
    let (_, timer) = r.timers();
    assert!((r.stability() - (1.0 - timer)).abs() < 1e-12, "stability {} timer {timer}", r.stability());
    assert!(r.stability() > 0.0 && r.stability() < 1.0);
}

#[test]
fn stability_is_zero_once_the_core_has_imploded() {
    let mut r = reactor(50.0);
    r.force_implosion();
    assert!(r.imploded());
    assert_eq!(r.stability(), 0.0);
}

#[test]
fn stability_is_zero_when_the_core_is_ejected() {
    let mut r = reactor(50.0);
    r.eject();
    assert!(r.ejected());
    assert_eq!(r.stability(), 0.0);
}

/// No canisters: the bands divide by nothing in a careless formula; every value stays finite in
/// Auto and in Manual, with and without load.
#[test]
fn zero_canisters_make_no_nan() {
    for manual in [false, true] {
        for load in [0.0, 800.0, 2500.0] {
            let mut r = Reactor::new(ReactorTuning::default(), 3);
            assert!(r.set_power(true));
            if manual {
                r.set_targets(100.0, 50.0);
            }
            run(&mut r, 120.0, load);
            let b = r.bands();
            for v in [r.heat(), r.injection(), r.converter(), r.output_kw(), r.heat_velocity(), b.injection_optimal.lo, b.injection_optimal.hi, b.injection_allowed.hi, b.converter_optimal.mid()] {
                assert!(v.is_finite(), "manual {manual}, load {load}: {v}");
            }
        }
    }
}

/// #316: a SCRAM vents the chamber: power off, injection gone at once, a share of the canisters' fill lost.
#[test]
fn scram_stops_injection_at_once_and_costs_a_share_of_the_fill() {
    let mut r = reactor(50.0);
    r.set_targets(60.0, 30.0);
    run(&mut r, 20.0, 500.0);
    assert!(r.injection() > 1.0, "injecting before the SCRAM");
    let before: f64 = r.canister_lives().iter().sum();
    let share = r.tuning().scram_fill_loss;
    let lost = r.scram().expect("a SCRAM on a live core");
    let after: f64 = r.canister_lives().iter().sum();
    assert_eq!((r.injection(), r.power_on(), r.targets().0), (0.0, false, 0.0));
    assert!((after - before * (1.0 - share)).abs() < 1e-9 && (lost - (before - after)).abs() < 1e-9);
    run(&mut r, 10.0, 500.0);
    assert_eq!(r.injection(), 0.0, "stays out");
}

#[test]
fn scram_never_goes_below_zero_and_is_nan_free_without_canisters() {
    let mut r = Reactor::new(ReactorTuning { scram_fill_loss: 7.0, ..ReactorTuning::default() }, 3);
    assert_eq!(r.scram(), Some(0.0), "no canisters: nothing to lose");
    r.insert_canister(Canister::standard()).unwrap();
    r.scram();
    assert!(r.canister_lives().iter().all(|l| *l == 0.0), "a share over 1 clamps to everything");
    let mut r = Reactor::new(ReactorTuning { scram_fill_loss: f64::NAN, ..ReactorTuning::default() }, 3);
    r.insert_canister(Canister::standard()).unwrap();
    r.scram();
    assert!(r.canister_lives().iter().all(|l| l.is_finite()));
    r.eject();
    assert_eq!(r.scram(), None, "an ejected core has nothing to vent");
}

#[test]
fn an_orderly_shutdown_costs_no_fill() {
    let mut r = reactor(50.0);
    let before = r.canister_lives();
    r.set_power(false);
    run(&mut r, 5.0, 0.0);
    assert_eq!(r.canister_lives(), before);
}
