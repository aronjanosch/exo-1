//! Rules for the power grid: shares, priorities and minimum ratios, with numbers.
use super::*;

#[test]
fn default_request_is_the_500_kw_load() {
    let g = Grid::default();
    assert_eq!(g.request_kw(), 500.0);
}

#[test]
fn a_breaker_off_asks_for_nothing_and_its_group_is_dark() {
    let mut g = Grid::default();
    g.set_breaker(Group::Lights, false);
    assert!(!g.breaker(Group::Lights));
    assert_eq!(g.group(Group::Lights).demand_kw, 50.0, "the demand is the device's, the breaker is the switch");
    assert!((g.request_kw() - 450.0).abs() < 1e-9);
    let left = g.distribute(1000.0);
    assert!((left - 550.0).abs() < 1e-9, "left {left}");
    assert_eq!(g.factor(Group::Lights), 0.0);
    assert!(!g.supply(Group::Lights).on);
    assert_eq!(g.factor(Group::Engines), 1.0);
    assert_eq!(g.factor(Group::LifeSupport), 1.0);
    g.set_breaker(Group::Lights, true);
    assert_eq!(g.request_kw(), 500.0);
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
    // 300 kW: life support (prio 3) full, engines (prio 2) get the 200 left for 350 asked
    // (0.57, above their 0.5), lights (prio 1) nothing.
    g.distribute(300.0);
    assert_eq!(g.factor(Group::LifeSupport), 1.0);
    assert!((g.supply(Group::Engines).ratio - 200.0 / 350.0).abs() < 1e-9);
    assert!(g.supply(Group::Engines).on, "slower, not off");
    assert_eq!(g.factor(Group::Lights), 0.0);
    assert!(!g.supply(Group::Lights).on);
}

#[test]
fn below_the_minimum_ratio_a_group_switches_off_and_frees_its_power() {
    let mut g = Grid::default();
    // 150 kW: life support takes 100, 50 are left for 350 asked (0.14 < 0.5). Engines drop out
    // and the 50 go to lights.
    g.distribute(150.0);
    assert!(!g.supply(Group::Engines).on);
    assert_eq!(g.factor(Group::Engines), 0.0);
    assert_eq!(g.factor(Group::Lights), 1.0);
}

#[test]
fn equal_priority_shares_in_proportion() {
    let mut g = Grid::default();
    g.set_priority(Group::Lights, 2);
    // Engines 350 and lights 50 share the 300 kW left after life support: ratio 300 / 400 each.
    g.distribute(100.0 + 300.0);
    assert!((g.supply(Group::Engines).ratio - 0.75).abs() < 1e-9);
    assert!((g.supply(Group::Lights).ratio - 0.75).abs() < 1e-9);
}
