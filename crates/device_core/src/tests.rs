use super::*;

const DT: f64 = 1.0 / 60.0;

fn dev() -> Condition {
    Condition::new(DeviceTuning::default(), 7)
}

fn safe() -> Condition {
    Condition::new(DeviceTuning { fail_chance: 0.0, ..DeviceTuning::default() }, 7)
}

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
fn wear_only_after_delay() {
    let mut c = dev();
    let delay = c.wear_delay_left();
    println!("wear delay {delay:.1} s");
    for _ in 0..600 {
        c.deteriorate(1.0, 0.0);
    }
    assert_eq!(c.value(), 100.0);
    assert_eq!(c.wear_delay_left(), delay);
    let mut t = 0.0;
    while c.wear_delay_left() > 0.0 {
        c.deteriorate(1.0, 1.0);
        t += 1.0;
        assert_eq!(c.value(), 100.0, "worn at {t} s, before the delay");
    }
    for _ in 0..100 {
        c.deteriorate(1.0, 1.0);
    }
    for _ in 0..2000 {
        c.deteriorate(1.0, 1.0);
    }
}

#[test]
fn overload_wears_at_once() {
    let mut c = dev();
    assert!(c.wear_delay_left() > 10.0);
    for _ in 0..10 {
        c.deteriorate(1.0, 1.5);
    }
    // 0.125 x 4 x 0.5 per s for 10 s.
    assert!(c.value() < 100.0, "overload wears at once: {}", c.value());
}

#[test]
fn failure_costs_condition_and_locks() {
    let mut fails = 0;
    for seed in 0..200 {
        let mut c = Condition::new(DeviceTuning::default(), seed);
        c.set_value(30.0);
        if let RepairOutcome::Failed { .. } = c.repair_step(DT, 0.0, Tool::WRENCH) {
            fails += 1;
            assert!(matches!(c.repair_step(DT, 0.0, Tool::WRENCH), RepairOutcome::Locked { .. }));
        }
    }
    println!("skill 0: {fails}/200 attempts failed");
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
fn holding_repairs_a_broken_device_all_the_way() {
    let mut c = safe();
    c.set_value(0.0);
    let (_, o) = hold(&mut c, 50.0, Tool::WRENCH);
    assert!(c.functioning() && c.fraction() >= 1.0, "{o:?}");
}
