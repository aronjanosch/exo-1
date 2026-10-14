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
