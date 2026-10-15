//! Rules for the power grid: shares, priorities and minimum ratios, with numbers.
use super::*;

#[test]
fn default_request_is_the_1280_kw_load() {
    let g = Grid::default();
    assert_eq!(g.request_kw(), 1280.0);
}

#[test]
fn a_breaker_off_asks_for_nothing_and_its_group_is_dark() {
    let mut g = Grid::default();
    g.set_breaker(Group::Lights, false);
    assert!(!g.breaker(Group::Lights));
    assert_eq!(g.group(Group::Lights).demand_kw, 80.0, "the demand is the device's, the breaker is the switch");
    assert!((g.request_kw() - 1200.0).abs() < 1e-9);
    let left = g.distribute(1500.0);
    assert!((left - 300.0).abs() < 1e-9, "left {left}");
    assert_eq!(g.factor(Group::Lights), 0.0);
    assert!(!g.supply(Group::Lights).on);
    assert_eq!(g.factor(Group::Engines), 1.0);
    assert_eq!(g.factor(Group::LifeSupport), 1.0);
    g.set_breaker(Group::Lights, true);
    assert_eq!(g.request_kw(), 1280.0);
}

#[test]
fn all_breakers_off_is_no_load() {
    let mut g = Grid::default();
    for x in Group::ALL {
        g.set_breaker(x, false);
    }
    assert_eq!(g.request_kw(), 0.0);
    assert_eq!(g.distribute(300.0), 300.0, "nothing is taken");
}

#[test]
fn brownout_sheds_low_priority_first() {
    let mut g = Grid::default();
    // 850 kW: life support (prio 3) full, engines (prio 2) get the 450 left for 800 asked
    // (0.56, above their 0.5), lights (prio 1) nothing.
    g.distribute(850.0);
    assert_eq!(g.factor(Group::LifeSupport), 1.0);
    assert!((g.supply(Group::Engines).ratio - 450.0 / 800.0).abs() < 1e-9);
    assert!(g.supply(Group::Engines).on, "slower, not off");
    assert_eq!(g.factor(Group::Lights), 0.0);
    assert!(!g.supply(Group::Lights).on);
}

#[test]
fn below_the_minimum_ratio_a_group_switches_off_and_frees_its_power() {
    let mut g = Grid::default();
    // 700 kW: life support takes 400, 300 are left for 800 asked (0.375 < 0.5). Engines drop out
    // and the 300 go to lights (80 asked).
    g.distribute(700.0);
    assert!(!g.supply(Group::Engines).on);
    assert_eq!(g.factor(Group::Engines), 0.0);
    assert_eq!(g.factor(Group::Lights), 1.0);
}

#[test]
fn equal_priority_shares_in_proportion() {
    let mut g = Grid::default();
    g.set_priority(Group::Lights, 2);
    // Engines 800 and lights 80 share the 440 kW left after life support: ratio 440 / 880 each.
    g.distribute(400.0 + 440.0);
    assert!((g.supply(Group::Engines).ratio - 0.5).abs() < 1e-9);
    assert!((g.supply(Group::Lights).ratio - 0.5).abs() < 1e-9);
}

#[test]
fn a_limit_of_one_is_the_default_and_changes_nothing() {
    let mut g = Grid::default();
    for x in Group::ALL {
        assert_eq!(g.limit(x), 1.0);
    }
    assert_eq!(g.request_kw(), 1280.0);
    g.distribute(850.0);
    assert_eq!(g.factor(Group::LifeSupport), 1.0);
    assert!((g.supply(Group::Engines).ratio - 450.0 / 800.0).abs() < 1e-9);
}

#[test]
fn a_limit_caps_the_request_and_the_factor_is_delivered_over_demand() {
    let mut g = Grid::default();
    g.set_limit(Group::Engines, 0.5);
    assert_eq!(g.limit(Group::Engines), 0.5);
    assert_eq!(g.request_of(Group::Engines), 400.0, "800 demand times 0.5");
    assert_eq!(g.group(Group::Engines).demand_kw, 800.0, "the demand is the device's");
    assert_eq!(g.request_kw(), 880.0);
    let left = g.distribute(1000.0);
    assert!((left - 120.0).abs() < 1e-9, "left {left}");
    assert_eq!(g.supply(Group::Engines).request_kw, 400.0);
    assert_eq!(g.supply(Group::Engines).delivered_kw, 400.0);
    assert_eq!(g.supply(Group::Engines).ratio, 1.0, "full delivery of the limited request");
    assert!((g.factor(Group::Engines) - 0.5).abs() < 1e-9, "scales the efficiency");
}

#[test]
fn a_limit_is_clamped_to_zero_one() {
    let mut g = Grid::default();
    g.set_limit(Group::Lights, 1.5);
    assert_eq!(g.limit(Group::Lights), 1.0);
    g.set_limit(Group::Lights, -0.2);
    assert_eq!(g.limit(Group::Lights), 0.0);
}

#[test]
fn a_limit_of_zero_switches_the_group_off() {
    let mut g = Grid::default();
    g.set_limit(Group::Lights, 0.0);
    assert_eq!(g.request_of(Group::Lights), 0.0);
    assert_eq!(g.request_kw(), 1200.0);
    let left = g.distribute(1500.0);
    assert!((left - 300.0).abs() < 1e-9, "left {left}");
    assert!(!g.supply(Group::Lights).on);
    assert_eq!(g.factor(Group::Lights), 0.0);
    assert_eq!(g.factor(Group::Engines), 1.0);
}

#[test]
fn a_short_reactor_with_limits_at_one_sheds_by_priority() {
    let mut g = Grid::default();
    g.distribute(850.0);
    assert_eq!(g.factor(Group::LifeSupport), 1.0);
    assert!(g.supply(Group::Engines).on);
    assert!(!g.supply(Group::Lights).on, "the lowest priority goes first");
}

#[test]
fn lowering_the_engines_limit_lets_the_lowest_priority_keep_its_minimum_ratio() {
    let mut g = Grid::default();
    // 850 kW, limits 1: engines take the 450 left, lights (prio 1) are shed.
    g.set_limit(Group::Engines, 0.5);
    // Engines ask 400: life support 400 and engines 400 are served in full, the 50 left go to
    // lights (80 asked, share 0.625 >= its 0.3 minimum), so lights stay on.
    g.distribute(850.0);
    assert_eq!(g.factor(Group::Engines), 0.5);
    assert!(g.supply(Group::Lights).on);
    assert!((g.supply(Group::Lights).ratio - 50.0 / 80.0).abs() < 1e-9);
}

#[test]
fn lowering_the_lights_limit_does_not_save_the_lights() {
    let mut g = Grid::default();
    // Lights are the lowest priority: their own limit only shrinks what they ask, and the engines
    // (prio 2) still take everything they can before lights see a kW.
    g.set_limit(Group::Lights, 0.5);
    g.distribute(850.0);
    assert!(!g.supply(Group::Lights).on);
    assert_eq!(g.factor(Group::Engines), 450.0 / 800.0);
}
