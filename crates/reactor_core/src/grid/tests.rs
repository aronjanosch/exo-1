//! Rules for the power grid: shares, priorities and minimum ratios, with numbers.
use super::*;

#[test]
fn a_breaker_off_asks_for_nothing_and_its_group_is_dark() {
    let t = GridTuning::default();
    let mut g = Grid::default();
    g.set_breaker(Group::Lights, false);
    assert!(!g.breaker(Group::Lights));
    assert_eq!(g.group(Group::Lights).demand_kw, t.lights.demand_kw, "the demand is the device's, the breaker is the switch");
    let without_lights = t.engines.demand_kw + t.life_support.demand_kw;
    assert!((g.request_kw() - without_lights).abs() < 1e-9);
    let left = g.distribute(1500.0);
    assert!((left - (1500.0 - without_lights)).abs() < 1e-9, "left {left}");
    assert_eq!(g.factor(Group::Lights), 0.0);
    assert!(!g.supply(Group::Lights).on);
    assert_eq!(g.factor(Group::Engines), 1.0);
    assert_eq!(g.factor(Group::LifeSupport), 1.0);
    g.set_breaker(Group::Lights, true);
    assert_eq!(g.request_kw(), without_lights + t.lights.demand_kw);
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
    g.distribute(700.0);
    assert!(!g.supply(Group::Engines).on);
    assert_eq!(g.factor(Group::Engines), 0.0);
    assert_eq!(g.factor(Group::Lights), 1.0);
}

#[test]
fn equal_priority_shares_in_proportion() {
    let mut g = Grid::default();
    g.set_priority(Group::Lights, 2);
    g.distribute(400.0 + 440.0);
    assert!((g.supply(Group::Engines).ratio - 0.5).abs() < 1e-9);
    assert!((g.supply(Group::Lights).ratio - 0.5).abs() < 1e-9);
}

#[test]
fn a_limit_caps_the_request_and_the_factor_is_delivered_over_demand() {
    let t = GridTuning::default();
    let mut g = Grid::default();
    g.set_limit(Group::Engines, 0.5);
    assert_eq!(g.limit(Group::Engines), 0.5);
    let limited = t.engines.demand_kw * 0.5;
    assert_eq!(g.request_of(Group::Engines), limited, "the demand times the limit");
    assert_eq!(g.group(Group::Engines).demand_kw, t.engines.demand_kw, "the demand is the device's");
    let requested = limited + t.life_support.demand_kw + t.lights.demand_kw;
    assert_eq!(g.request_kw(), requested);
    let left = g.distribute(1000.0);
    assert!((left - (1000.0 - requested)).abs() < 1e-9, "left {left}");
    assert_eq!(g.supply(Group::Engines).request_kw, limited);
    assert_eq!(g.supply(Group::Engines).delivered_kw, limited);
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
fn lowering_the_engines_limit_lets_the_lowest_priority_keep_its_minimum_ratio() {
    let mut g = Grid::default();
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

#[test]
fn the_r40_groups_start_idle_and_draw_nothing() {
    let t = GridTuning::default();
    let g = Grid::default();
    for x in [Group::Shields, Group::Weapons, Group::JumpDrive] {
        assert_eq!(g.group(x).demand_kw, 0.0, "{x:?} starts at 0 kW");
        assert!(g.breaker(x));
    }
    let room_groups = t.engines.demand_kw + t.life_support.demand_kw + t.lights.demand_kw;
    assert_eq!(g.request_kw(), room_groups, "the load is the three room groups' only");
}
