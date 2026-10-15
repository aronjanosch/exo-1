//! Rules for the weapons' capacitor: recharge from the WEAPONS group, one shot per energy, the reload.
use super::*;

const DT: f64 = 1.0;

fn guns() -> Weapons {
    Weapons::new(WeaponsTuning::default(), 1)
}

/// A gun that jams on every shot (the jam chance is the tuning's; this one is 1).
fn jammy() -> Weapons {
    let mut w = Weapons::new(WeaponsTuning { jam_chance: 1.0, ..WeaponsTuning::default() }, 1);
    w.step(1000.0, 1.0);
    w
}

/// Shots at `skill` over `shots` shots with no heat (so only jams stop the gun), seeded. Each
/// jam is cleared by its full hold, as a player would, and counted.
fn jams_in(skill: f64, shots: u32, seed: u64) -> u32 {
    let tuning = WeaponsTuning { heat_per_shot: 0.0, ..WeaponsTuning::default() };
    let mut w = Weapons::new(tuning, seed).with_skill(skill);
    let mut jams = 0;
    for _ in 0..shots {
        // 0.25 s at full power: one shot's energy back, and the reload runs out.
        w.step(0.25, 1.0);
        assert!(w.fire(), "the gun is ready every shot");
        if w.jammed() {
            jams += 1;
            assert!(w.clear_jam(3.0));
        }
    }
    jams
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

#[test]
fn heat_rises_per_shot_and_cools_over_time() {
    let mut w = full();
    assert_eq!(w.heat(), 0.0);
    assert!(w.fire());
    assert!((w.heat() - 0.18).abs() < 1e-12, "one shot heats 0.18: {}", w.heat());
    w.step(1.0, 0.0);
    assert!((w.heat() - 0.10).abs() < 1e-9, "a second cools 0.08: {}", w.heat());
    w.step(10.0, 0.0);
    assert_eq!(w.heat(), 0.0, "cold is the floor");
}

#[test]
fn overheat_locks_at_the_line_and_lets_go_only_below_the_lower_one() {
    let mut w = full();
    // Shots with the capacitor kept full (power 1 at each step), until the line is crossed.
    while !w.overheated() {
        assert!(w.fire());
        w.step(0.25, 1.0);
    }
    assert!(!w.fire(), "overheated: refused");
    w.step(2.25, 1.0); // about 0.8: over the lower line, so still locked
    assert!(w.heat() > 0.5, "still above the unlock line: {}", w.heat());
    assert!(!w.fire(), "hysteresis: 0.8 is not cool enough");
    while w.heat() >= 0.5 {
        w.step(0.05, 0.0);
    }
    assert!(w.heat() < 0.5);
    assert!(w.fire(), "below the lower line the lock lets go");
    assert!(!w.overheated());
}

#[test]
fn a_jam_blocks_fire_until_cleared() {
    let mut w = jammy();
    assert!(w.fire(), "the jammed shot still goes");
    assert!(w.jammed());
    w.step(1.0, 1.0); // reload over, the gun still jammed
    assert!(!w.fire(), "a jam blocks fire");
    assert!(!w.clear_jam(2.0), "2 s of hold is not enough");
    assert!(w.jammed());
    assert!(w.clear_jam(1.0), "3 s of hold clears it");
    assert!(!w.jammed());
    assert!(w.fire(), "a cleared gun fires again");
}

#[test]
fn a_release_starts_the_clearing_over_and_no_jam_has_nothing_to_clear() {
    let mut w = guns();
    assert!(!w.clear_jam(5.0), "no jam, nothing to clear");
    let mut w = jammy();
    assert!(w.fire());
    assert!(!w.clear_jam(2.0));
    w.stop_clearing();
    assert!(!w.clear_jam(2.0), "released: the count starts over");
    assert!(w.jammed());
}

#[test]
fn a_higher_skill_lowers_the_jams_and_full_skill_has_none() {
    let at_zero = jams_in(0.0, 2000, 7);
    let at_quarter = jams_in(25.0, 2000, 7);
    assert!(at_zero > 0, "skill 0 jams now and then");
    assert!(at_quarter < at_zero, "skill 25: {at_quarter} jams, skill 0: {at_zero}");
    assert_eq!(jams_in(50.0, 2000, 7), 0, "at the required skill the jam chance is 0");
    assert_eq!(jams_in(0.0, 2000, 7), at_zero, "the same seed gives the same jams");
}
