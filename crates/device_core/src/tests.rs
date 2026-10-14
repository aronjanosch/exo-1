//! Rules of #286 for device condition, with numbers.
use super::*;

const DT: f64 = 1.0 / 60.0;

fn dev() -> Condition {
    Condition::new(DeviceTuning::default(), 7)
}

/// A device that never fails a repair roll, for timing tests.
fn safe() -> Condition {
    Condition::new(DeviceTuning { fail_chance: 0.0, ..DeviceTuning::default() }, 7)
}

/// Holds repair until done; returns seconds and the outcome that ended it.
fn hold(c: &mut Condition, skill: f64, tool: Tool) -> (f64, RepairOutcome) {
    let mut t = 0.0;
    loop {
        let o = c.repair_step(DT, skill, tool);
        t += DT;
        if matches!(o, RepairOutcome::Done { .. } | RepairOutcome::Full) || t > 120.0 {
            return (t, o);
        }
    }
}

#[test]
fn thresholds_and_factor() {
    let mut c = dev();
    assert!(c.functioning() && !c.needs_repair());
    assert_eq!(c.factor(), 1.0);
    assert_eq!(c.damage(55.0), Some(DeviceEvent::NeedsRepair));
    assert!(c.needs_repair() && c.functioning());
    // 45: a quarter of the way down from 50 to 20.
    assert!((c.factor() - (0.5 + 0.5 * 25.0 / 30.0)).abs() < 1e-9, "factor {}", c.factor());
    assert_eq!(c.damage(30.0), Some(DeviceEvent::Broke));
    assert!(!c.functioning());
    assert_eq!(c.factor(), 0.0);
}

#[test]
fn repair_time_falls_with_skill() {
    let mut times = Vec::new();
    for skill in [0.0, 50.0, 100.0] {
        let mut c = safe();
        c.set_value(0.0);
        let (t, o) = hold(&mut c, skill, Tool::WRENCH);
        assert!(matches!(o, RepairOutcome::Done { .. }), "{o:?}");
        times.push(t);
    }
    println!("full repair from 0, s at skill 0/50/100: {times:.2?}");
    assert!((times[0] - 25.0).abs() < 0.1 && (times[1] - 15.0).abs() < 0.1 && (times[2] - 5.0).abs() < 0.1);
    let mut c = safe();
    c.set_value(0.0);
    let (bare, _) = hold(&mut c, 100.0, Tool::HANDS);
    assert!(bare > times[2] * 1.9, "bare hands {bare:.2} s");
}

#[test]
fn repaired_device_works_at_condition_factor() {
    let mut c = safe();
    c.set_value(10.0);
    assert!(!c.functioning());
    // Repair at skill 50 (15 s from 0 to full, 6.67 per s) until it works again.
    while !c.functioning() {
        let o = c.repair_step(DT, 50.0, Tool::WRENCH);
        assert!(matches!(o, RepairOutcome::Working { .. }), "{o:?}");
    }
    let f = c.factor();
    println!("works again at condition {:.2}, factor {f:.3}", c.value());
    assert!(f > 0.49 && f < 0.52);
    // Up to 35: half-way from broken to threshold.
    while c.value() < 35.0 {
        c.repair_step(DT, 50.0, Tool::WRENCH);
    }
    assert!((c.factor() - 0.75).abs() < 0.01, "{}", c.factor());
    let (_, o) = hold(&mut c, 50.0, Tool::WRENCH);
    assert!(matches!(o, RepairOutcome::Done { .. }));
    assert_eq!(c.factor(), 1.0);
}

#[test]
fn wear_only_after_delay() {
    let mut c = dev();
    let delay = c.wear_delay_left();
    println!("wear delay {delay:.1} s");
    assert!((120.0..=720.0).contains(&delay));
    // Idle: no wear and the delay does not run.
    for _ in 0..600 {
        c.deteriorate(1.0, 0.0);
    }
    assert_eq!(c.value(), 100.0);
    assert_eq!(c.wear_delay_left(), delay);
    // In use: nothing until the delay is over.
    let mut t = 0.0;
    while c.wear_delay_left() > 0.0 {
        c.deteriorate(1.0, 1.0);
        t += 1.0;
        assert_eq!(c.value(), 100.0, "worn at {t} s, before the delay");
    }
    for _ in 0..100 {
        c.deteriorate(1.0, 1.0);
    }
    assert!((c.value() - (100.0 - 12.5)).abs() < 1e-9, "{}", c.value());
    // Normal wear stops at the floor.
    for _ in 0..2000 {
        c.deteriorate(1.0, 1.0);
    }
    assert_eq!(c.value(), 50.0);
}

#[test]
fn overload_wears_at_once() {
    let mut c = dev();
    assert!(c.wear_delay_left() > 10.0);
    for _ in 0..10 {
        c.deteriorate(1.0, 1.5);
    }
    // 0.125 x 4 x 0.5 per s for 10 s.
    assert!((c.value() - 97.5).abs() < 1e-9, "{}", c.value());
}

#[test]
fn failure_costs_condition_and_locks() {
    let mut fails = 0;
    let mut lost_total = 0.0;
    for seed in 0..200 {
        let mut c = Condition::new(DeviceTuning::default(), seed);
        c.set_value(30.0);
        if let RepairOutcome::Failed { lost } = c.repair_step(DT, 0.0, Tool::WRENCH) {
            fails += 1;
            lost_total += lost;
            assert!(matches!(c.repair_step(DT, 0.0, Tool::WRENCH), RepairOutcome::Locked { .. }));
        }
    }
    println!("skill 0: {fails}/200 attempts failed");
    assert!((40..80).contains(&fails), "{fails}");
    assert_eq!(lost_total, 5.0 * fails as f64);
    // At the required skill no attempt fails.
    for seed in 0..200 {
        let mut c = Condition::new(DeviceTuning::default(), seed);
        c.set_value(30.0);
        assert!(matches!(c.repair_step(DT, 50.0, Tool::WRENCH), RepairOutcome::Working { .. }));
    }
}

#[test]
fn skill_grows_only_from_real_repairs() {
    let mut c = safe();
    c.set_value(30.0);
    let (_, o) = hold(&mut c, 40.0, Tool::WRENCH);
    let RepairOutcome::Done { skill_gain: gain } = o else { panic!("{o:?}") };
    println!("skill gain at 40: {gain:.3}");
    assert!((gain - 40.0 / 40f64.powf(1.5)).abs() < 1e-9);
    // Topping up a device that did not need repair teaches nothing.
    c.set_value(80.0);
    let (_, o) = hold(&mut c, 40.0, Tool::WRENCH);
    assert_eq!(o, RepairOutcome::Done { skill_gain: 0.0 });
    assert!(skill_gain(40.0, 0.0) <= 2.0 && skill_gain(40.0, 90.0) < skill_gain(40.0, 20.0));
}

#[test]
fn normal_wear_never_breaks_the_device_even_with_a_low_floor() {
    // A floor below the broken limit is clamped: wear alone stops at the broken limit.
    let t = DeviceTuning { wear_floor: 10.0, ..DeviceTuning::default() };
    let mut c = Condition::new(t, 7);
    let mut events = Vec::new();
    for _ in 0..20_000 {
        events.extend(c.deteriorate(1.0, 1.0));
    }
    assert!(c.functioning(), "broke by wear at {}", c.value());
    assert_eq!(c.value(), t.broken_below);
    assert!(!events.contains(&DeviceEvent::Broke), "{events:?}");
}

#[test]
fn overload_still_breaks_the_device() {
    let mut c = Condition::new(DeviceTuning { wear_floor: 50.0, ..DeviceTuning::default() }, 7);
    let mut events = Vec::new();
    for _ in 0..600 {
        events.extend(c.deteriorate(1.0, 2.0));
    }
    assert!(!c.functioning());
    assert!(events.contains(&DeviceEvent::Broke), "{events:?}");
}

#[test]
fn restore_reports_works_again_and_redraws_the_wear_delay() {
    let mut c = safe();
    c.set_value(15.0);
    assert!(!c.functioning());
    // Each rise is one step; crossing the broken limit gives Works once.
    assert_eq!(c.restore(4.0), None);
    assert_eq!(c.restore(2.0), Some(DeviceEvent::Works));
    assert_eq!(c.restore(1.0), None);
    // A full repair (by rise) draws a new delay.
    c.set_value(0.0);
    c.wear_delay = 0.0;
    let o = c.repair_rise(1000.0, 50.0, Tool::WRENCH);
    assert_eq!(o, Some(DeviceEvent::Works));
    assert_eq!(c.value(), 100.0);
    assert!((120.0..=720.0).contains(&c.wear_delay_left()), "{}", c.wear_delay_left());
}

#[test]
fn the_screwdriver_repairs_slower_than_the_wrench() {
    let c = safe();
    assert!(c.repair_seconds(50.0, Tool::SCREWDRIVER) > c.repair_seconds(50.0, Tool::WRENCH));
    let mut w = safe();
    let mut s = safe();
    w.set_value(10.0);
    s.set_value(10.0);
    for _ in 0..5 {
        w.repair_rise(1.0, 50.0, Tool::WRENCH);
        s.repair_rise(1.0, 50.0, Tool::SCREWDRIVER);
    }
    assert!(w.value() > s.value(), "wrench {} screwdriver {}", w.value(), s.value());
}

#[test]
fn holding_repairs_a_broken_device_all_the_way() {
    let mut c = safe();
    c.set_value(0.0);
    let (t, o) = hold(&mut c, 50.0, Tool::WRENCH);
    assert!(c.functioning() && c.fraction() >= 1.0, "{o:?}");
    assert!(t < 60.0, "the hold takes about a repair time: {t:.1} s");
}
