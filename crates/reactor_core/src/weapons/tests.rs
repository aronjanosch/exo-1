//! Rules for the weapons' capacitor: recharge from the WEAPONS group, one shot per energy, the reload.
use super::*;

const DT: f64 = 1.0;

fn guns() -> Weapons {
    Weapons::new(WeaponsTuning::default())
}

/// A capacitor filled to full with full power.
fn full() -> Weapons {
    let mut w = guns();
    w.step(1000.0, 1.0);
    w
}

#[test]
fn recharge_scales_with_the_power_the_group_gets() {
    let mut half = guns();
    half.step(DT, 0.5);
    let mut all = guns();
    all.step(DT, 1.0);
    assert!((all.stored_kws() - 400.0 * DT).abs() < 1e-9, "full power: {}", all.stored_kws());
    assert!((half.stored_kws() - 200.0 * DT).abs() < 1e-9, "half power: {}", half.stored_kws());
}

#[test]
fn no_power_no_recharge() {
    let mut w = guns();
    w.step(100.0, 0.0);
    assert_eq!(w.stored_kws(), 0.0);
    assert_eq!(w.shots(), 0);
    assert!(!w.fire(), "an empty gun with no power does not fire");
}

#[test]
fn a_shot_takes_one_shot_energy_and_starts_the_reload() {
    let mut w = full();
    assert_eq!(w.shots(), 6);
    let before = w.stored_kws();
    assert!(w.fire());
    assert!((w.stored_kws() - (before - 100.0)).abs() < 1e-9, "took {}", before - w.stored_kws());
    assert_eq!(w.shots(), 5);
    assert!((w.reload_left() - 0.25).abs() < 1e-12, "reload {}", w.reload_left());
}

#[test]
fn a_shot_is_refused_while_reloading() {
    let mut w = full();
    assert!(w.fire());
    let stored = w.stored_kws();
    assert!(!w.fire(), "the reload is running");
    assert_eq!(w.stored_kws(), stored, "a refused shot takes nothing");
    w.step(0.3, 0.0);
    assert_eq!(w.reload_left(), 0.0);
    assert!(w.fire(), "the reload is over");
}

#[test]
fn a_shot_is_refused_without_one_shot_of_energy() {
    let mut w = guns();
    w.step(0.2, 1.0); // 80 kW·s: under one shot of 100 kW·s
    assert_eq!(w.shots(), 0);
    assert!(!w.fire());
    assert_eq!(w.reload_left(), 0.0, "a refused shot starts no reload");
}

#[test]
fn the_capacitor_holds_six_shots_and_no_more() {
    let mut w = full();
    assert!((w.stored_kws() - 600.0).abs() < 1e-9, "capacity {}", w.stored_kws());
    w.step(1000.0, 1.0);
    assert!((w.stored_kws() - 600.0).abs() < 1e-9, "overfilled to {}", w.stored_kws());
}

#[test]
fn demand_is_one_while_not_full_and_zero_when_full() {
    let mut w = guns();
    assert_eq!(w.demand_share(), 1.0);
    w.step(1000.0, 1.0);
    assert_eq!(w.demand_share(), 0.0);
    assert!(w.fire());
    assert_eq!(w.demand_share(), 1.0, "a shot makes room, the group asks again");
}

#[test]
fn a_full_capacitor_takes_no_more_power() {
    let mut w = full();
    let stored = w.stored_kws();
    w.step(DT, 1.0);
    assert_eq!(w.stored_kws(), stored);
}
