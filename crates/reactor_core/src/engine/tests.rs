use super::*;

const FULL_KW: f64 = 800.0;

/// An engine whose force has reached `target` (one step of a whole second closes the gap).
fn settled(target: f64) -> Engine {
    let mut e = Engine::new();
    e.set_target(target);
    e.step(1.0, 1.0);
    e
}

#[test]
fn engine_demand_at_half_throttle_is_the_full_demand_to_the_power_one_and_a_half() {
    let e = settled(50.0);
    assert!((e.demand_kw(FULL_KW) - 800.0 * 0.5_f64.powf(1.5)).abs() < 1e-9);
    assert!((e.demand_kw(FULL_KW) - 282.8).abs() < 0.1, "about 283 kW");
    assert_eq!(settled(100.0).demand_kw(FULL_KW), 800.0);
    assert_eq!(settled(-100.0).demand_kw(FULL_KW), 800.0, "reverse draws the same");
}

#[test]
fn engine_force_follows_the_target_at_ten_per_second() {
    let mut e = Engine::new();
    e.set_target(0.0);
    e.step(0.01, 1.0);
    assert!((e.force() - 90.0).abs() < 1e-9, "a tenth of the gap per hundredth of a second");
    for _ in 0..200 {
        e.step(0.01, 1.0);
    }
    assert!(e.force().abs() < 1e-6, "reaches the target: {}", e.force());
}

#[test]
fn engine_has_no_thrust_below_min_voltage() {
    let e = settled(100.0);
    assert_eq!(e.thrust(MIN_VOLTAGE - 0.01), 0.0);
    assert!(e.thrust(MIN_VOLTAGE) > 0.0);
    assert_eq!(e.thrust(1.0), 100.0, "full force at full power and condition");
}

#[test]
fn engine_low_condition_lowers_thrust() {
    let mut worn = settled(100.0);
    worn.set_condition(0.0);
    let mut sound = settled(100.0);
    sound.set_condition(1.0);
    assert!(worn.thrust(1.0) < sound.thrust(1.0));
    assert_eq!(worn.thrust(1.0), 50.0, "half thrust at zero condition");
}
