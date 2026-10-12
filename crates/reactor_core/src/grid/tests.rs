//! Rules of #286 for the power grid, with numbers.
use super::*;
use crate::reactor::{FuelRod, Mode, ReactorTuning};
use device_core::DeviceTuning;

fn show(g: &Grid, supply: f64) {
    let line: Vec<String> = Group::ALL.iter().map(|&x| format!("{x:?} {:.2}", g.factor(x))).collect();
    println!("supply {supply:.0} kW: {}", line.join(", "));
}

#[test]
fn full_supply_runs_every_group_at_its_limit() {
    let mut g = Grid::default();
    assert_eq!(g.request_kw(), 1000.0);
    g.set_limit(Group::Engines, 0.3);
    assert_eq!(g.request_kw(), 720.0);
    let left = g.distribute(1000.0);
    show(&g, 1000.0);
    assert!((left - 280.0).abs() < 1e-9);
    assert!((g.factor(Group::Engines) - 0.3).abs() < 1e-9);
    for x in [Group::Shields, Group::Weapons, Group::LifeSupport, Group::Aux] {
        assert_eq!(g.factor(x), 1.0);
    }
}

#[test]
fn brownout_sheds_low_priority_first() {
    let mut g = Grid::default();
    // 600 kW: life support (prio 4) full, engines and shields (prio 3) share 500 of 650, weapons
    // and aux get nothing.
    g.distribute(600.0);
    show(&g, 600.0);
    assert_eq!(g.factor(Group::LifeSupport), 1.0);
    let share = 500.0 / 650.0;
    assert!((g.supply(Group::Engines).ratio - share).abs() < 1e-9);
    assert!((g.factor(Group::Shields) - share).abs() < 1e-9);
    assert!(g.supply(Group::Engines).on, "slower, not off");
    assert_eq!(g.factor(Group::Weapons), 0.0);
    assert_eq!(g.factor(Group::Aux), 0.0);
    assert!(!g.supply(Group::Aux).on);
}

#[test]
fn below_the_minimum_ratio_a_group_switches_off_and_frees_its_power() {
    let mut g = Grid::default();
    // 300 kW: after life support 200 are left for 650 asked (0.31 < 0.5). Engines (the bigger
    // one) drop out, shields get 200 of 250.
    g.distribute(300.0);
    show(&g, 300.0);
    assert!(!g.supply(Group::Engines).on && g.factor(Group::Engines) == 0.0);
    assert!((g.factor(Group::Shields) - 0.8).abs() < 1e-9);
    // Raising the engines' priority turns it around.
    g.set_priority(Group::Engines, 5);
    g.distribute(300.0);
    show(&g, 300.0);
    assert!((g.factor(Group::Engines) - 0.75).abs() < 1e-9);
    assert_eq!(g.factor(Group::LifeSupport), 0.0, "200 ÷ 0 left");
}

#[test]
fn reactor_feeds_the_grid() {
    let mut r = Reactor::new(ReactorTuning::default(), DeviceTuning::default(), 5);
    r.insert_rod(FuelRod::standard()).unwrap();
    r.set_operator_skill(50.0);
    assert!(r.set_power(true));
    assert_eq!(r.mode(), Mode::Auto);
    let mut g = Grid::default();
    g.set_limit(Group::Weapons, 0.0);
    g.set_limit(Group::Engines, 0.5);
    for _ in 0..(240.0 * 30.0) as usize {
        run(&mut r, &mut g, 1.0 / 30.0);
    }
    show(&g, r.output_kw());
    assert!((r.output_kw() - 600.0).abs() < 20.0, "{}", r.output_kw());
    assert!(g.factor(Group::Engines) > 0.48 && g.factor(Group::LifeSupport) > 0.97);
}
