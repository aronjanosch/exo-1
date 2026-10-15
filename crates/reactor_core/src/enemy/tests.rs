//! Rules for the enemy: the fire timer, the shield before the hull, Destroyed and Escaped, and the
//! seed's hits.
use super::*;
use crate::engine_room::EngineRoom;
use crate::hull::HitsTuning;
use crate::tuning::Tuning;

fn enemy() -> EnemyTuning {
    EnemyTuning::default()
}

fn encounter() -> Encounter {
    Encounter::new(&enemy(), &HitsTuning::default())
}

/// Steps `secs` in `dt` steps; the times at which the enemy fired.
fn fire_times(e: &mut Encounter, rng: &mut Rng, dt: f64, secs: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let steps = (secs / dt).round() as usize;
    for i in 1..=steps {
        if e.step(dt, rng).is_some() {
            out.push(i as f64 * dt);
        }
    }
    out
}

#[test]
fn fires_on_the_timer() {
    // The first shot after `first_shot_s`, then one every `fire_every_s`.
    let mut e = encounter();
    let times = fire_times(&mut e, &mut Rng::new(1), 0.5, 30.0);
    assert_eq!(times, vec![10.0, 18.0, 26.0], "{times:?}");
    assert_eq!(e.shots(), 3);
}

#[test]
fn shots_drain_the_shield_before_the_hull() {
    let mut e = encounter();
    // 600 points of shield: five shots of 120 leave the hull alone.
    for _ in 0..5 {
        e.take_shot(120.0);
    }
    assert_eq!(e.shield_fraction(), 0.0);
    assert_eq!(e.hull_fraction(), 1.0, "the shield takes the first 600 points");
    e.take_shot(120.0);
    assert!((e.hull_fraction() - 880.0 / 1000.0).abs() < 1e-9, "the sixth shot reaches the hull");
    assert_eq!(e.outcome(), Outcome::None);
}

#[test]
fn hull_at_zero_is_destroyed_and_it_does_not_fire_again() {
    let mut e = encounter();
    // 600 shield + 1000 hull = 1600 points: 13 shots (1560) leave 40 hull, the 14th ends it.
    for _ in 0..13 {
        e.take_shot(120.0);
    }
    assert_eq!(e.outcome(), Outcome::None);
    e.take_shot(120.0);
    assert_eq!(e.outcome(), Outcome::Destroyed);
    assert_eq!(e.hull_fraction(), 0.0);
    assert!(fire_times(&mut e, &mut Rng::new(1), 0.5, 200.0).is_empty(), "a destroyed ship does not fire");
    e.take_shot(120.0);
    assert_eq!(e.outcome(), Outcome::Destroyed, "the end stays the end");
}

#[test]
fn a_jump_during_the_encounter_is_escaped() {
    let mut e = encounter();
    e.escape();
    assert_eq!(e.outcome(), Outcome::Escaped);
    assert!(fire_times(&mut e, &mut Rng::new(1), 0.5, 60.0).is_empty(), "an escaped ship does not fire");
}

#[test]
fn a_jump_out_of_the_room_ends_the_encounter_as_escaped() {
    let mut room = EngineRoom::from_tuning(&Tuning::default(), 1);
    room.start_ambush();
    room.jump_mut().fill();
    assert!(room.jump_go(), "a full drive jumps");
    assert_eq!(room.encounter().map(|e| e.outcome()), Some(Outcome::Escaped));
}

#[test]
fn the_same_seed_gives_the_same_hits() {
    let run = |seed: u64| {
        let mut e = encounter();
        let mut rng = Rng::new(seed);
        let mut hits = Vec::new();
        for _ in 0..200 {
            if let Some(h) = e.step(0.5, &mut rng) {
                hits.push((h.at, h.structure));
            }
        }
        hits
    };
    let a = run(7);
    assert_eq!(a, run(7));
    assert!(a.len() >= 10 && a != run(8), "{a:?}");
}
