use std::path::Path;

use content_core::File;
use health_core::*;

fn shipped() -> HealthTuning {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/gameplay/health/health.json");
    let text = std::fs::read_to_string(&p).unwrap();
    HealthTuning::load(&[File::new(HEALTH_PATH, text)]).unwrap_or_else(|e| panic!("{e:#?}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn air(oxygen: f64) -> Env {
    Env { oxygen, hot: false, glow: false, fire: 0.0, rod_per_s: 0.0, vacuum: false, suited: false }
}

fn run(h: &mut Health, t: &HealthTuning, secs: f64, env: Env) -> Vec<HealthEvent> {
    let mut ev = Vec::new();
    for _ in 0..(secs * 10.0).round() as usize {
        ev.extend(h.step(t, 0.1, env));
    }
    ev
}

#[test]
fn radiation_below_25_does_not_burn_and_decays_slowly() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "radiation", 20.0);
    run(&mut h, &t, 2.0, air(1.0));
    assert_eq!(h.strength("burn"), 0.0, "below 25 no burn");
}

#[test]
fn a_radioactive_item_near_the_body_builds_radiation_per_second() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    let near = Env { oxygen: 1.0, hot: false, glow: false, fire: 0.0, rod_per_s: 1.0, vacuum: false, suited: false };
    run(&mut h, &t, 3.0, near);
    let mut far = Health::new(&t, 2);
    run(&mut far, &t, 3.0, Env { rod_per_s: 0.0, ..near });
    assert_eq!(far.strength("radiation"), 0.0, "out of reach, nothing builds");
}

#[test]
fn death_at_the_threshold_and_unconscious_first() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    // Burn 200 takes 200 share of the maximum: vitality -100, the dead line.
    h.add(&t, "burn", 200.0);
    let seen = run(&mut h, &t, 0.1, air(1.0));
    assert_eq!(h.state(), Life::Dead);
    assert_eq!(seen, vec![HealthEvent::Unconscious, HealthEvent::Died { cause: "burn".into() }], "unconscious first, then dead: {seen:?}");
    assert!(h.vitality() <= -100.0 + 1e-9, "dead at minus the maximum: {}", h.vitality());
    let burn = h.strength("burn");
    h.add(&t, "burn", 10.0);
    assert_eq!(h.strength("burn"), burn, "the dead take nothing");
}

#[test]
fn respawn_clears_the_body() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "burn", 90.0);
    h.add(&t, "radiation", 150.0);
    run(&mut h, &t, 1.0, Env { oxygen: 0.0, hot: true, glow: false, fire: 0.0, rod_per_s: 0.0, vacuum: false, suited: false });
    h.respawn(&t);
    assert_eq!(h.state(), Life::Conscious);
    assert_eq!(h.vitality(), t.max_vitality);
    assert_eq!(h.buildup(), 0.0);
    assert!(!h.burning());
    assert_eq!(h.max_share(&t), 1.0);
    assert_eq!(h.mobility(&t), Mobility { walk: 1.0, sprint: true });
    for id in ["burn", "radiation", "oxygen_low", "headache"] {
        assert_eq!(h.strength(id), 0.0, "{id}");
    }
}

#[test]
fn the_dead_take_nothing_and_respawn_time_is_shipped() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "burn", 200.0);
    run(&mut h, &t, 0.1, air(1.0));
    assert_eq!(h.state(), Life::Dead);
    let burn = h.strength("burn");
    h.add(&t, "burn", 50.0);
    assert_eq!(h.strength("burn"), burn, "the dead take nothing");
    assert_eq!(h.treat(&t, "deglow", 50.0).unwrap().reduced, 0.0);
    let ev = run(&mut h, &t, 2.0, Env { oxygen: 1.0, hot: true, glow: true, fire: 0.0, rod_per_s: 0.0, vacuum: false, suited: false });
    assert!(ev.is_empty() && h.buildup() == 0.0, "the dead do not burn or glow");
}

#[test]
fn rng_is_seeded_and_in_range() {
    let mut a = Rng::new(9);
    let mut b = Rng::new(9);
    for _ in 0..100 {
        let x = a.next_f64();
        assert_eq!(x, b.next_f64());
        assert!((0.0..1.0).contains(&x));
    }
}

#[test]
fn the_aim_hits_a_capsule_in_front_within_reach_only() {
    let up = [0.0, 1.0, 0.0];
    let feet = [0.0, 0.0, -1.5];
    let eye = [0.0, 1.7, 0.0];
    let fwd = [0.0, 0.0, -1.0];
    let hit = capsule_hit(eye, fwd, 2.0, feet, up, 1.8, 0.4).expect("in front, within reach");
    assert!((hit - 1.1).abs() < 0.06, "the near side of a 0.4 m capsule 1.5 m away: {hit}");
    assert!(capsule_hit(eye, fwd, 2.0, [0.0, 0.0, -2.8], up, 1.8, 0.4).is_none(), "beyond the reach");
    assert!(capsule_hit(eye, fwd, 2.0, [1.0, 0.0, -1.5], up, 1.8, 0.4).is_none(), "a metre to the side");
    assert!(capsule_hit(eye, [0.0, 0.0, 1.0], 2.0, feet, up, 1.8, 0.4).is_none(), "behind");
}

#[test]
fn unconscious_at_zero_and_awake_again_above() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "burn", 110.0);
    let seen = run(&mut h, &t, 0.1, air(1.0));
    assert_eq!(seen, vec![HealthEvent::Unconscious], "unconscious once, not dead: {seen:?}");
    assert_eq!(h.state(), Life::Unconscious);
    assert!(h.vitality() < 0.0 && h.vitality() > -100.0, "below zero, above the dead line: {}", h.vitality());
    h.treat(&t, "bandage", 100.0).expect("a bandage");
    assert_eq!(h.state(), Life::Conscious, "awake above zero: vitality {}", h.vitality());
    assert!(h.vitality() > 0.0);
}

/// A body that is unconscious from oxygen low `s` alone (no other harm).
fn unconscious_at(t: &HealthTuning, s: f64) -> Health {
    let mut h = Health::new(t, 7);
    h.add(t, "oxygen_low", s);
    h.step(t, 0.0, air(1.0));
    assert_eq!(h.state(), Life::Unconscious, "oxygen low {s} is unconscious");
    h
}

/// One CPR step of `dt` seconds.
fn pump(h: &mut Health, t: &HealthTuning, dt: f64, skill: f64, air_ok: bool) -> Vec<HealthEvent> {
    h.cpr(t, dt, true, skill, air_ok)
}

#[test]
fn the_unconscious_line_is_below_the_cpr_floor() {
    // Bisect the oxygen low at which a body with nothing else wrong is unconscious: the CPR's floor
    // must sit above it, so CPR alone stabilises the body and never wakes it.
    let t = shipped();
    let (mut lo, mut hi) = (100.0, 200.0);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        let mut h = Health::new(&t, 7);
        h.add(&t, "oxygen_low", mid);
        h.step(&t, 0.0, air(1.0));
        if h.state() == Life::Unconscious {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    assert!(t.cpr.stabilize_floor > hi, "the floor {} sits above the line {hi}", t.cpr.stabilize_floor);
}

#[test]
fn cpr_does_nothing_in_bad_air_and_nothing_when_nobody_pumps() {
    let t = shipped();
    let mut h = unconscious_at(&t, 150.0);
    for _ in 0..20 {
        pump(&mut h, &t, 0.1, 50.0, false);
    }
    assert!(close(h.strength("oxygen_low"), 150.0), "no relief in bad air: {}", h.strength("oxygen_low"));
    assert!(close(h.cpr_seconds(), 2.0), "the time still counts: {}", h.cpr_seconds());
    h.cpr(&t, 0.1, false, 50.0, true);
    assert_eq!(h.cpr_seconds(), 0.0, "nobody pumps: the count is reset");
    assert!(close(h.strength("oxygen_low"), 150.0));
}

#[test]
fn a_revive_roll_wakes_the_body_after_two_seconds_and_lowers_the_oxygen_low() {
    let t = shipped();
    let mut h = unconscious_at(&t, t.cpr.stabilize_floor);
    let mut woke_at = None;
    for i in 1..=1200 {
        let ev = pump(&mut h, &t, 0.1, 50.0, true);
        if ev.contains(&HealthEvent::Woke) {
            woke_at = Some(i as f64 * 0.1);
            break;
        }
    }
    let at = woke_at.expect("the revive roll wakes it within 120 s (seeded)");
    assert!(at >= t.cpr.revive_after_s - 1e-6, "no revive before {} s of CPR: woke at {at:.1}", t.cpr.revive_after_s);
    assert_eq!(h.state(), Life::Conscious);
    assert!(h.strength("oxygen_low") <= t.cpr.revive_low_max + 1e-9, "the revive leaves at most {}: {}", t.cpr.revive_low_max, h.strength("oxygen_low"));
    assert!(h.vitality() > 0.0, "awake with vitality {}", h.vitality());
}

// Vacuum (r41, the suit): in vacuum the body takes decompression at Barotrauma's pressure rate,
// unsuited; a suit protects; the air in the room means nothing there.

fn in_vacuum(suited: bool) -> Env {
    Env { oxygen: 0.0, hot: false, glow: false, fire: 0.0, rod_per_s: 0.0, vacuum: true, suited }
}

/// Seconds (in 0.1 s steps) until the body is dead in this environment, or 100 s.
fn seconds_to_death(t: &HealthTuning, env: Env) -> f64 {
    let mut h = Health::new(t, 1);
    let mut steps = 0;
    while h.state() != Life::Dead && steps < 1000 {
        h.step(t, 0.1, env);
        steps += 1;
    }
    steps as f64 * 0.1
}

#[test]
fn unsuited_in_vacuum_dies_in_about_ten_seconds() {
    let t = shipped();
    let s = seconds_to_death(&t, in_vacuum(false));
    assert!((9.5..=10.5).contains(&s), "dead after {s} s, about 10 s wanted");
    let mut h = Health::new(&t, 1);
    let seen = run(&mut h, &t, 6.0, in_vacuum(false));
    assert!(seen.contains(&HealthEvent::Unconscious), "unconscious at about 5 s: {seen:?}");
    assert_eq!(h.state(), Life::Unconscious, "not dead yet at 6 s");
    let seen = run(&mut h, &t, 5.0, in_vacuum(false));
    assert_eq!(seen, vec![HealthEvent::Died { cause: "decompression".into() }], "the cause is the vacuum");
}

#[test]
fn suited_in_vacuum_takes_nothing() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    // The suit's tank gives air, so the oxygen is full; the vacuum is the only thing around.
    run(&mut h, &t, 60.0, Env { oxygen: 1.0, ..in_vacuum(true) });
    assert_eq!(h.strength("decompression"), 0.0);
    assert_eq!(h.state(), Life::Conscious);
    assert!(close(h.vitality(), t.max_vitality), "vitality {}", h.vitality());
}

#[test]
fn air_means_nothing_in_vacuum_and_bad_air_is_not_vacuum() {
    let t = shipped();
    let mut empty = Health::new(&t, 1);
    run(&mut empty, &t, 3.0, in_vacuum(false));
    let mut full = Health::new(&t, 1);
    run(&mut full, &t, 3.0, Env { oxygen: 1.0, ..in_vacuum(false) });
    assert!(close(empty.strength("decompression"), full.strength("decompression")), "the room's air does not change the damage");
    assert!(close(empty.strength("decompression"), 30.0), "10 a second for 3 s: {}", empty.strength("decompression"));
    // Bad air without vacuum is the slow suffocation: no decompression, the oxygen low builds.
    let mut stuffy = Health::new(&t, 1);
    run(&mut stuffy, &t, 3.0, air(0.0));
    assert_eq!(stuffy.strength("decompression"), 0.0);
    assert!(stuffy.strength("oxygen_low") > 0.0);
}

#[test]
fn decompression_heals_back_in_air() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    run(&mut h, &t, 3.0, in_vacuum(false));
    let hurt = h.strength("decompression");
    run(&mut h, &t, 10.0, air(1.0));
    assert!(h.strength("decompression") < hurt, "it heals in air");
}
