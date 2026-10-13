//! health_core (#295): burn, oxygen, radiation, treatments, death and respawn, on the shipped file.
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

/// Steps `secs` in the given air (fixed 0.1 s steps).
fn run(h: &mut Health, t: &HealthTuning, secs: f64, oxygen: f64) -> Vec<HealthEvent> {
    let mut ev = Vec::new();
    for _ in 0..(secs * 10.0).round() as usize {
        ev.extend(h.step(t, 0.1, Env { oxygen }));
    }
    ev
}

#[test]
fn burn_reduces_vitality_by_its_bands() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    assert_eq!(h.vitality(), 100.0);
    h.add(&t, "burn", 30.0);
    assert!(close(h.vitality(), 70.0), "burn 30 is the first band's end: {}", h.vitality());
    h.add(&t, "burn", 15.0);
    assert!(close(h.vitality(), 55.0), "burn 45 is halfway through the second band: {}", h.vitality());
    h.add(&t, "burn", 1000.0);
    assert!(close(h.strength("burn"), 200.0), "capped at the affliction's max");
    assert_eq!(h.add(&t, "poison", 1.0), false, "no such affliction any more");
}

#[test]
fn bandage_lowers_burn() {
    let t = shipped();
    let mut h = Health::new(&t, 7);
    h.add(&t, "burn", 60.0);
    // Skill 50 is above the bandage's 40, so every roll succeeds.
    let r = h.treat(&t, "bandage", 50.0).unwrap();
    assert!(r.success);
    assert!(close(h.strength("burn"), 60.0 - 15.0 * t.skill_factor(50.0)), "burn {}", h.strength("burn"));
    assert!(h.treat(&t, "medkit", 50.0).is_none(), "an unknown treatment is None");
}

#[test]
fn deglow_lowers_radiation() {
    let t = shipped();
    let mut h = Health::new(&t, 3);
    h.add(&t, "radiation", 100.0);
    let before = h.vitality();
    h.treat(&t, "deglow", 50.0).unwrap();
    assert!(h.strength("radiation") < 100.0 - 39.0, "radiation {}", h.strength("radiation"));
    assert!(h.vitality() > before, "less radiation, more vitality");
}

#[test]
fn bad_air_raises_oxygen_low_and_good_air_lowers_it() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    run(&mut h, &t, 2.0, 0.0);
    assert!(close(h.strength("oxygen_low"), 10.0), "5 per second for 2 s: {}", h.strength("oxygen_low"));
    run(&mut h, &t, 1.0, 1.0);
    assert!(close(h.strength("oxygen_low"), 0.0), "10 per second of fresh air clears it: {}", h.strength("oxygen_low"));
}

#[test]
fn death_at_the_threshold_and_unconscious_first() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    // Radiation 200 takes 150, burn 100 takes 100: vitality -150, past the -100 line.
    h.add(&t, "radiation", 200.0);
    h.add(&t, "burn", 100.0);
    let mut seen = Vec::new();
    for _ in 0..200 {
        seen.extend(h.step(&t, 0.1, Env { oxygen: 1.0 }));
        if h.state() == Life::Dead {
            break;
        }
    }
    assert!(seen.contains(&HealthEvent::Unconscious), "unconscious first: {seen:?}");
    assert_eq!(h.state(), Life::Dead);
    assert!(matches!(seen.last(), Some(HealthEvent::Died { cause }) if cause == "radiation"), "{seen:?}");
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
    h.add(&t, "radiation", 120.0);
    run(&mut h, &t, 1.0, 0.0);
    h.respawn(&t);
    assert_eq!(h.state(), Life::Conscious);
    assert_eq!(h.vitality(), t.max_vitality);
    assert_eq!(h.strength("burn"), 0.0);
    assert_eq!(h.strength("radiation"), 0.0);
    assert_eq!(h.strength("oxygen_low"), 0.0);
}

#[test]
fn the_dead_take_nothing_and_respawn_time_is_shipped() {
    let t = shipped();
    assert!(close(t.respawn_s, 5.0));
    let mut h = Health::new(&t, 1);
    h.add(&t, "radiation", 200.0);
    h.add(&t, "burn", 100.0);
    run(&mut h, &t, 1.0, 1.0);
    assert_eq!(h.state(), Life::Dead);
    let burn = h.strength("burn");
    h.add(&t, "burn", 50.0);
    assert_eq!(h.strength("burn"), burn, "the dead take nothing");
    assert!(h.treat(&t, "deglow", 50.0).unwrap().reduced == 0.0);
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
