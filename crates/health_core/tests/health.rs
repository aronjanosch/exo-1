//! health_core (#295): heat, fire and burn in ticks, radiation, nausea and mobility, oxygen,
//! headache, treatments, death and respawn, on the shipped file.
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
    Env { oxygen, hot: false, glow: false }
}

fn hot() -> Env {
    Env { oxygen: 1.0, hot: true, glow: false }
}

/// Steps `secs` in 0.1 s steps with the given surroundings.
fn run(h: &mut Health, t: &HealthTuning, secs: f64, env: Env) -> Vec<HealthEvent> {
    let mut ev = Vec::new();
    for _ in 0..(secs * 10.0).round() as usize {
        ev.extend(h.step(t, 0.1, env));
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
    assert!(!h.add(&t, "poison", 1.0), "no such affliction");
}

#[test]
fn burn_levels_follow_the_bands() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    assert_eq!(h.burn_level(&t), 0);
    h.add(&t, "burn", 30.0);
    assert_eq!(h.burn_level(&t), 1, "30 is still level I");
    h.add(&t, "burn", 1.0);
    assert_eq!(h.burn_level(&t), 2, "above 30 is level II");
    h.add(&t, "burn", 30.0);
    assert_eq!(h.burn_level(&t), 3, "above 60 is level III");
}

#[test]
fn buildup_fills_in_three_seconds_and_cools_in_two() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    run(&mut h, &t, 2.9, hot());
    assert!(h.buildup() > 0.9 && h.buildup() < 1.0, "almost full at 2.9 s: {}", h.buildup());
    run(&mut h, &t, 0.2, hot());
    assert!(close(h.buildup(), 1.0), "full after 3 s, capped: {}", h.buildup());
    run(&mut h, &t, 1.0, air(1.0));
    assert!(close(h.buildup(), 0.5), "half cooled after 1 of 2 s: {}", h.buildup());
    run(&mut h, &t, 1.5, air(1.0));
    assert!(close(h.buildup(), 0.0), "empty after 2 s, not below zero: {}", h.buildup());
}

#[test]
fn the_hot_plate_burns_lightly_per_tick_from_the_first_tick() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    run(&mut h, &t, 1.0, hot());
    assert!(close(h.strength("burn"), 1.5 - 0.075), "1.5 per tick less the band's heal: {}", h.strength("burn"));
    assert!(!h.burning(), "one second of heat is no fire");
}

#[test]
fn standing_too_long_sets_fire_and_the_burn_grows_fast() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    let mut changed = Vec::new();
    let mut diff = std::collections::BTreeMap::new();
    let mut last = 0.0;
    for i in 1..=50 {
        h.step(&t, 0.1, hot());
        let b = h.strength("burn");
        if b != last {
            changed.push(i);
            diff.insert(i, b - last);
            last = b;
        }
    }
    assert_eq!(changed, vec![10, 20, 30, 40, 50], "burn changes only on the ticks");
    assert!(close(diff[&20], 1.5 - 0.075), "heat only before the fire: {}", diff[&20]);
    assert!(close(diff[&40], 1.5 + 6.0 - 0.075), "on fire from the full buildup at 3 s: {}", diff[&40]);
    assert!(h.burning());
}

#[test]
fn fire_drains_over_four_seconds_after_leaving_the_heat() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    run(&mut h, &t, 3.0, hot());
    assert!(h.burning() && close(h.fire(), 1.0));
    run(&mut h, &t, 2.0, air(1.0));
    assert!(h.burning(), "still burning after 2 s out of the heat");
    assert!((h.fire() - 0.5).abs() < 0.02, "half drained: {}", h.fire());
    let before = h.strength("burn");
    run(&mut h, &t, 1.0, air(1.0));
    assert!(h.strength("burn") > before, "the burn keeps growing while it drains");
    run(&mut h, &t, 1.5, air(1.0));
    assert!(!h.burning() && h.fire() == 0.0, "out after 4 s");
}

#[test]
fn radiation_above_25_burns_per_tick_by_band_and_costs_no_vitality() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "radiation", 100.0);
    assert_eq!(h.vitality(), 100.0, "radiation costs no vitality itself");
    run(&mut h, &t, 1.0, air(1.0));
    assert!(close(h.strength("burn"), 2.0 - 0.075), "100 radiation burns 2.0 per tick (band 100..150): {}", h.strength("burn"));
    assert!(close(h.strength("radiation"), 100.0 - 0.2), "above 25 it decays 0.2 per tick: {}", h.strength("radiation"));
}

#[test]
fn radiation_below_25_does_not_burn_and_decays_slowly() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "radiation", 20.0);
    run(&mut h, &t, 2.0, air(1.0));
    assert_eq!(h.strength("burn"), 0.0, "below 25 no burn");
    assert!(close(h.strength("radiation"), 20.0 - 2.0 * 0.07), "below 25 it decays 0.07 per tick: {}", h.strength("radiation"));
}

#[test]
fn the_glow_builds_radiation_per_tick() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    let glow = Env { oxygen: 1.0, hot: false, glow: true };
    run(&mut h, &t, 0.9, glow);
    assert_eq!(h.strength("radiation"), 0.0, "no tick yet");
    run(&mut h, &t, 0.1, glow);
    assert!(close(h.strength("radiation"), 8.0 - 0.07), "one tick of glow adds 8 (and decays a little): {}", h.strength("radiation"));
}

#[test]
fn nausea_follows_radiation_above_50_and_slows_the_walk() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "radiation", 50.0);
    assert_eq!(h.nausea(&t), 0.0, "nothing up to 50 radiation");
    assert_eq!(h.mobility(&t), Mobility { walk: 1.0, sprint: true });
    h.add(&t, "radiation", 50.0);
    assert!(close(h.nausea(&t), 100.0 / 3.0), "(100 - 50) / 150 of the scale: {}", h.nausea(&t));
    assert_eq!(h.mobility(&t), Mobility { walk: 0.85, sprint: true }, "nausea slows the walk a little");
    assert_eq!(h.vitality(), 100.0, "nausea costs no vitality");
    h.add(&t, "radiation", 50.0);
    assert!(close(h.nausea(&t), 200.0 / 3.0));
    assert_eq!(h.mobility(&t), Mobility { walk: 0.7, sprint: false }, "above nausea 50: slower, no sprint");
}

#[test]
fn deglow_lowers_radiation_by_100_and_the_nausea_with_it() {
    let t = shipped();
    let mut h = Health::new(&t, 3);
    h.add(&t, "radiation", 150.0);
    assert!(h.nausea(&t) > 50.0);
    let r = h.treat(&t, "deglow", 50.0).unwrap();
    assert!(r.success);
    assert!(close(h.strength("radiation"), 50.0), "150 less 100: {}", h.strength("radiation"));
    assert!(close(r.reduced, 100.0));
    assert_eq!(h.nausea(&t), 0.0, "at 50 radiation the nausea is gone");
    assert_eq!(h.mobility(&t), Mobility { walk: 1.0, sprint: true });
}

#[test]
fn bandage_lowers_burn_by_15() {
    let t = shipped();
    let mut h = Health::new(&t, 7);
    h.add(&t, "burn", 60.0);
    // Skill 50 is above the bandage's 40, so every roll succeeds, and skill 50 counts 1.0.
    let r = h.treat(&t, "bandage", 50.0).unwrap();
    assert!(r.success);
    assert!(close(h.strength("burn"), 45.0), "burn {}", h.strength("burn"));
    assert!(h.treat(&t, "medkit", 50.0).is_none(), "an unknown treatment is None");
}

#[test]
fn bad_air_raises_oxygen_low_per_tick_and_good_air_lowers_it() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    run(&mut h, &t, 2.0, air(0.0));
    assert!(close(h.strength("oxygen_low"), 10.0), "5 per tick for 2 ticks: {}", h.strength("oxygen_low"));
    run(&mut h, &t, 1.0, air(1.0));
    assert!(close(h.strength("oxygen_low"), 0.0), "10 per tick of fresh air clears it: {}", h.strength("oxygen_low"));
}

#[test]
fn headache_after_20_s_of_oxygen_low_above_50_lowers_the_maximum() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    // Above 50 from the 11th tick on; the 20 s count starts there, so at 25 s there is none yet.
    run(&mut h, &t, 25.0, air(0.0));
    assert_eq!(h.strength("headache"), 0.0, "not yet at 25 s: the count began at about 11 s");
    run(&mut h, &t, 8.0, air(0.0));
    // Set to full at about 31 s, healed by 0.5 per tick since: still near 100.
    assert!(h.strength("headache") > 97.0 && h.strength("headache") <= 100.0, "headache at 33 s: {}", h.strength("headache"));
    let s = h.strength("headache");
    assert!(close(h.max_share(&t), 1.0 - 0.2 * s / 100.0), "the maximum drops by 20 % at full strength: {}", h.max_share(&t));
    let lost = t.affliction("oxygen_low").unwrap().vitality_share(h.strength("oxygen_low"));
    assert!(close(h.vitality(), 100.0 * (h.max_share(&t) - lost)), "vitality is the lower maximum less the afflictions: {}", h.vitality());
}

#[test]
fn headache_heals_by_half_a_point_per_tick_and_scales_the_maximum() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "headache", 100.0);
    assert!(close(h.max_share(&t), 0.8));
    assert_eq!(h.vitality(), 80.0, "no vitality lost to the headache itself, the maximum is 80");
    run(&mut h, &t, 10.0, air(1.0));
    assert!(close(h.strength("headache"), 95.0), "10 ticks at 0.5: {}", h.strength("headache"));
    assert!(close(h.max_share(&t), 1.0 - 0.2 * 0.95));
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
    run(&mut h, &t, 1.0, Env { oxygen: 0.0, hot: true, glow: false });
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
    assert!(close(t.respawn_s, 5.0));
    let mut h = Health::new(&t, 1);
    h.add(&t, "burn", 200.0);
    run(&mut h, &t, 0.1, air(1.0));
    assert_eq!(h.state(), Life::Dead);
    let burn = h.strength("burn");
    h.add(&t, "burn", 50.0);
    assert_eq!(h.strength("burn"), burn, "the dead take nothing");
    assert_eq!(h.treat(&t, "deglow", 50.0).unwrap().reduced, 0.0);
    let ev = run(&mut h, &t, 2.0, Env { oxygen: 1.0, hot: true, glow: true });
    assert!(ev.is_empty() && h.buildup() == 0.0, "the dead do not burn or glow");
}

#[test]
fn the_shipped_file_names_every_affliction_the_code_uses() {
    let t = shipped();
    for id in ["burn", "oxygen_low", "radiation", "headache"] {
        assert!(t.affliction(id).is_some(), "{id}");
    }
    assert!(close(t.tick_s, 1.0));
    assert!(close(t.heat.burn_per_tick, 1.5));
    assert!(close(t.burning.fill_s, 3.0) && close(t.burning.cool_s, 2.0) && close(t.burning.drain_s, 4.0) && close(t.burning.burn_per_tick, 6.0));
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
