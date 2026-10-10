//! Boost as a capacitor (#90): drain, recharge after a delay, start threshold, strength curve.
use flight_core::{BoostCapacitor, BoostCapacitorTuning, Curve, Interp};
use glam::DVec2;

const DT: f64 = 1.0 / 60.0;

fn tuning() -> BoostCapacitorTuning {
    BoostCapacitorTuning {
        drain_time: 3.0,
        recharge_time: 6.0,
        recharge_delay: 1.0,
        start_charge: 0.2,
        strength_curve: Curve::new(Interp::Linear, vec![DVec2::new(0.0, 0.0), DVec2::new(1.0, 1.0)]).unwrap(),
        restart_while_held: false,
    }
}

/// Steps `secs` seconds with boost held or not; returns the last strength.
fn run(c: &mut BoostCapacitor, t: &BoostCapacitorTuning, want: bool, secs: f64) -> f64 {
    let mut s = 0.0;
    for _ in 0..(secs / DT).round() as usize {
        s = c.step(want, t, DT);
    }
    s
}

#[test]
fn starts_full_and_drains_in_the_drain_time() {
    let t = tuning();
    let mut c = BoostCapacitor::default();
    assert_eq!(c.charge, 1.0);
    run(&mut c, &t, true, 1.5);
    assert!((c.charge - 0.5).abs() < 0.02, "half drained after half the drain time: {}", c.charge);
    assert!(c.active);
    run(&mut c, &t, true, 1.6);
    assert_eq!(c.charge, 0.0);
    assert!(!c.active, "empty ends the boost");
}

#[test]
fn strength_follows_the_curve_and_is_zero_when_empty() {
    let t = tuning();
    let mut c = BoostCapacitor::default();
    let s = run(&mut c, &t, true, 1.5);
    assert!((s - c.charge).abs() < 0.02, "linear curve: strength {s} ~ charge {}", c.charge);
    let s = run(&mut c, &t, true, 2.0);
    assert_eq!(s, 0.0, "empty: no boost");
    let s = run(&mut c, &t, false, 0.5);
    assert_eq!(s, 0.0, "released: no boost");
}

#[test]
fn recharges_after_the_delay_in_the_recharge_time() {
    let t = tuning();
    let mut c = BoostCapacitor::default();
    run(&mut c, &t, true, 3.5);
    assert_eq!(c.charge, 0.0);
    // Empty at 3.0 s: the delay counts from the last use, so 0.5 s of it are gone.
    run(&mut c, &t, false, 0.4);
    assert_eq!(c.charge, 0.0, "nothing before the delay");
    run(&mut c, &t, false, 0.1 + 3.0);
    assert!((c.charge - 0.5).abs() < 0.02, "half after half the recharge time: {}", c.charge);
    run(&mut c, &t, false, 3.5);
    assert_eq!(c.charge, 1.0, "full, not above");
}

#[test]
fn every_use_restarts_the_delay() {
    let t = tuning();
    let mut c = BoostCapacitor::default();
    run(&mut c, &t, true, 1.5);
    run(&mut c, &t, false, 0.8);
    run(&mut c, &t, true, 0.1);
    let before = c.charge;
    run(&mut c, &t, false, 0.8);
    assert_eq!(c.charge, before, "a tap restarts the delay");
}

#[test]
fn needs_the_start_charge_but_runs_until_empty() {
    let t = tuning();
    let mut c = BoostCapacitor::with_charge(0.1);
    assert_eq!(run(&mut c, &t, true, 0.1), 0.0, "below the start charge it does not start");
    assert!(!c.active);
    let mut c = BoostCapacitor::with_charge(0.25);
    run(&mut c, &t, true, 0.3);
    assert!(c.active && c.charge < t.start_charge && c.charge > 0.0, "started above, runs on below: {}", c.charge);
}

#[test]
fn held_on_empty_recharges_and_starts_again_at_the_start_charge() {
    // TODO(initiator): no fresh press needed (issue #90, question 4), or a new press (#104 point 9).
    let t = BoostCapacitorTuning { restart_while_held: true, ..tuning() };
    let mut c = BoostCapacitor::default();
    run(&mut c, &t, true, 3.5);
    assert!(!c.active);
    // Empty at 3.0 s, delay 1 s, then 0.2 of 6 s = 1.2 s: starts at 5.2 s.
    run(&mut c, &t, true, 1.6);
    assert!(!c.active, "not yet: {}", c.charge);
    run(&mut c, &t, true, 0.2);
    assert!(c.active, "starts again at the start charge");
}

#[test]
fn no_drain_time_is_the_old_speed_stage() {
    let t = BoostCapacitorTuning { drain_time: 0.0, ..tuning() };
    let mut c = BoostCapacitor::default();
    assert_eq!(run(&mut c, &t, true, 30.0), 1.0);
    assert_eq!(c.charge, 1.0);
    assert_eq!(run(&mut c, &t, false, 1.0), 0.0);
}

#[test]
fn step_curve_cuts_off_instead_of_weakening() {
    let t = BoostCapacitorTuning { strength_curve: Curve::new(Interp::Linear, vec![DVec2::new(0.0, 1.0), DVec2::new(1.0, 1.0)]).unwrap(), ..tuning() };
    let mut c = BoostCapacitor::default();
    assert_eq!(run(&mut c, &t, true, 2.9), 1.0, "full strength until empty");
    assert_eq!(run(&mut c, &t, true, 0.2), 0.0);
}

#[test]
fn tuning_rejects_bad_values() {
    let ok = tuning();
    assert!(ok.validate().is_ok());
    for (bad, what) in [
        (BoostCapacitorTuning { drain_time: -1.0, ..tuning() }, "drain_time"),
        (BoostCapacitorTuning { recharge_time: 0.0, ..tuning() }, "recharge_time"),
        (BoostCapacitorTuning { recharge_delay: -0.1, ..tuning() }, "recharge_delay"),
        (BoostCapacitorTuning { start_charge: 1.5, ..tuning() }, "start_charge"),
    ] {
        let e = bad.validate().unwrap_err();
        assert!(e.contains(what), "{e}");
    }
}

/// #104 point 9: holding boost through an empty capacitor gives no more pulses; a new press
/// boosts again once the start charge is back (`restart_while_held: false`).
#[test]
fn after_empty_a_new_press_is_needed() {
    let t = tuning();
    let mut c = BoostCapacitor::default();
    run(&mut c, &t, true, 3.1);
    assert_eq!(c.charge, 0.0);
    let mut most: f64 = 0.0;
    for _ in 0..(8.0 / DT) as usize {
        most = most.max(c.step(true, &t, DT));
    }
    assert_eq!(most, 0.0, "held through empty: no weak pulses");
    assert!(c.charge > t.start_charge, "it recharged meanwhile: {}", c.charge);
    run(&mut c, &t, false, DT);
    assert!(run(&mut c, &t, true, DT) > 0.0, "a new press boosts");
}

/// Tapping the brake while holding boost through empty does not count as a new press.
#[test]
fn the_brake_does_not_rearm_an_empty_boost() {
    let t = tuning();
    let mut c = BoostCapacitor::default();
    for _ in 0..(3.1 / DT) as usize {
        c.step_braking(true, false, &t, DT);
    }
    for _ in 0..(3.0 / DT) as usize {
        c.step_braking(true, false, &t, DT);
    }
    assert!(c.charge > t.start_charge);
    c.step_braking(true, true, &t, DT);
    assert_eq!(c.step_braking(true, false, &t, DT), 0.0, "still needs a new press");
    c.step_braking(false, false, &t, DT);
    assert!(c.step_braking(true, false, &t, DT) > 0.0);
}
