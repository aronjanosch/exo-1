//! Boost as a capacitor (#90): drain, recharge after a delay, start threshold, strength curve.
use flight_core::{BodyState, BoostCapacitor, BoostCapacitorTuning, Curve, Field, FlightInput, Interp, PlanetEnv, ShipController, ShipTuning};
use glam::{DVec2, DVec3};

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

struct Space(Field);
impl PlanetEnv for Space {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world
    }
    fn radius(&self) -> f64 {
        5000.0
    }
    fn height_at(&self, _dir: DVec3) -> f64 {
        0.0
    }
    fn field(&self) -> &Field {
        &self.0
    }
}

/// Assist off in space: forward acceleration over one second with boost held from the start.
fn accel(ship: &mut ShipController, brake: bool) -> f64 {
    let env = Space(Field::default());
    let mut body = BodyState { pos: DVec3::new(0.0, 1e6, 0.0), ..Default::default() };
    let input = FlightInput { thrust: DVec3::NEG_Z, boost: true, brake, piloted: brake, ..Default::default() };
    let v0 = body.lin_vel;
    for _ in 0..60 {
        let (v, w) = ship.step(&body, &input, &env, DT);
        body.lin_vel = v;
        body.ang_vel = w;
        body.integrate(DT);
    }
    (body.lin_vel - v0).length()
}

#[test]
fn the_controller_drains_while_boosting_and_boost_weakens() {
    let mut ship = ShipController::new(ShipTuning::default());
    ship.hover_assist = false;
    // The pilot's G tolerance would cap the full boost.
    ship.tuning.g_safety.enabled = false;
    let full = accel(&mut ship, false);
    assert!(ship.boost.charge < 0.9, "one second of boost drains: {}", ship.boost.charge);
    ship.boost.charge = 0.3;
    let weak = accel(&mut ship, false);
    assert!(weak < full * 0.8, "weaker at low charge: {weak:.1} vs {full:.1} m/s");
    // Empty: plain thrust.
    ship.boost.charge = 0.0;
    ship.boost.active = false;
    let plain = accel(&mut ship, false);
    let want = ship.tuning.accel.forward;
    assert!((plain - want).abs() < 0.05 * want, "empty: plain thrust {plain:.2} m/s per s, want {want}");
}

#[test]
fn the_brake_does_not_drain() {
    // TODO(initiator): issue #90, question 5.
    let mut ship = ShipController::new(ShipTuning::default());
    accel(&mut ship, true);
    assert_eq!(ship.boost.charge, 1.0);
}

#[test]
fn the_dev_switch_flies_the_speed_stage_and_back() {
    let mut ship = ShipController::new(ShipTuning::default());
    ship.hover_assist = false;
    ship.boost_stage = true;
    let first = accel(&mut ship, false);
    for _ in 0..4 {
        accel(&mut ship, false);
    }
    let fifth = accel(&mut ship, false);
    assert_eq!(ship.boost.charge, 1.0, "the speed stage uses no charge");
    assert!((fifth - first).abs() < 0.02 * first, "full boost after 5 s: {fifth:.1} vs {first:.1} m/s");
    ship.boost_stage = false;
    accel(&mut ship, false);
    assert!(ship.boost.charge < 0.9, "back on the capacitor it drains: {}", ship.boost.charge);
}

/// #104 point 8: the speed stage (F6) does not refill an empty capacitor.
#[test]
fn the_speed_stage_leaves_the_charge_alone() {
    let mut c = BoostCapacitor::with_charge(0.0);
    assert_eq!(c.stage(true), 1.0);
    assert_eq!(c.stage(false), 0.0);
    assert_eq!(c.charge, 0.0, "F6 does not refill");
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
