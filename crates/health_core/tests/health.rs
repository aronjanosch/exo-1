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
    Env { oxygen, hot: false, glow: false, fire: 0.0, rod_per_s: 0.0 }
}

fn hot() -> Env {
    Env { oxygen: 1.0, hot: true, glow: false, fire: 0.0, rod_per_s: 0.0 }
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
    let glow = Env { oxygen: 1.0, hot: false, glow: true, fire: 0.0, rod_per_s: 0.0 };
    run(&mut h, &t, 0.9, glow);
    assert_eq!(h.strength("radiation"), 0.0, "no tick yet");
    run(&mut h, &t, 0.1, glow);
    assert!(close(h.strength("radiation"), 8.0 - 0.07), "one tick of glow adds 8 (and decays a little): {}", h.strength("radiation"));
}

#[test]
fn a_radioactive_item_near_the_body_builds_radiation_per_second() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    let near = Env { oxygen: 1.0, hot: false, glow: false, fire: 0.0, rod_per_s: 1.0 };
    run(&mut h, &t, 3.0, near);
    assert!(close(h.strength("radiation"), 3.0 - 3.0 * 0.07), "three seconds at 1 per second, less the decay: {}", h.strength("radiation"));
    let mut far = Health::new(&t, 2);
    run(&mut far, &t, 3.0, Env { rod_per_s: 0.0, ..near });
    assert_eq!(far.strength("radiation"), 0.0, "out of reach, nothing builds");
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
    run(&mut h, &t, 1.0, Env { oxygen: 0.0, hot: true, glow: false, fire: 0.0, rod_per_s: 0.0 });
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
    let ev = run(&mut h, &t, 2.0, Env { oxygen: 1.0, hot: true, glow: true, fire: 0.0, rod_per_s: 0.0 });
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

#[test]
fn each_injury_costs_its_vitality_share_of_the_maximum() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    assert_eq!(h.cost(&t, "burn"), 0.0, "no burn, no cost");
    assert_eq!(h.cost(&t, "oxygen_low"), 0.0, "no bad air, no cost");
    assert_eq!(h.cost(&t, "headache"), 0.0, "no headache, no cap lost");
    h.add(&t, "burn", 30.0);
    assert!(close(h.cost(&t, "burn"), 30.0), "burn 30 costs its first band's end: {}", h.cost(&t, "burn"));
    h.add(&t, "oxygen_low", 100.0);
    assert!(close(h.cost(&t, "oxygen_low"), 200.0 / 3.0), "oxygen 100 is a third into the second band: {}", h.cost(&t, "oxygen_low"));
    h.add(&t, "headache", 100.0);
    assert!(close(h.cost(&t, "headache"), 20.0), "a full headache takes the cap's 20 points: {}", h.cost(&t, "headache"));
    assert_eq!(h.cost(&t, "radiation"), 0.0, "radiation costs no vitality");
}

/// The burn of two seconds in one surrounding (the plate's buildup is not full before 3 s, so
/// only its own burn counts).
fn burn_after(t: &HealthTuning, env: Env) -> f64 {
    let mut h = Health::new(t, 5);
    run(&mut h, t, 2.0, env);
    h.strength("burn")
}

#[test]
fn a_fire_burns_by_its_amount_against_the_hot_plate() {
    let t = HealthTuning::load(&[File { path: "health/health.json".into(), text: include_str!("../../../content/gameplay/health/health.json").into() }]).unwrap_or_else(|e| panic!("{e:?}"));
    let plate = burn_after(&t, Env { oxygen: 1.0, hot: true, glow: false, fire: 0.0, rod_per_s: 0.0 });
    // A size 0.5 m fire (the dev button's): 10 x sqrt(0.5) per second on the body.
    let small = burn_after(&t, Env { oxygen: 1.0, hot: false, glow: false, fire: 10.0 * 0.5f64.sqrt(), rod_per_s: 0.0 });
    // A size 2 m fire: 10 x sqrt(2) per second.
    let big = burn_after(&t, Env { oxygen: 1.0, hot: false, glow: false, fire: 10.0 * 2f64.sqrt(), rod_per_s: 0.0 });
    println!("burn after 2 s: plate {plate:.1}, small fire {small:.1}, big fire {big:.1}");
    assert!(small < plate, "{small} vs {plate}");
    assert!(big > plate, "{big} vs {plate}");
    assert!(burn_after(&t, Env { oxygen: 1.0, hot: false, glow: false, fire: 0.0, rod_per_s: 0.0 }) == 0.0);
}

#[test]
fn the_low_air_case_costs_73_of_oxygen_and_the_burn_takes_the_rest_so_health_reads_zero() {
    // The HUD case: the air low (oxygen_low at 104.75: half the air left, its cost 73 points) and
    // the burn at level II (45 points). Together they are more than the whole maximum of 100, so
    // the vitality is below zero and the health reads 0 %. The health's bar is then all lost.
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "oxygen_low", 104.75);
    h.add(&t, "burn", 45.0);
    assert!(close(h.cost(&t, "oxygen_low"), 73.0), "oxygen {}", h.cost(&t, "oxygen_low"));
    assert!(close(h.cost(&t, "burn"), 45.0), "burn {}", h.cost(&t, "burn"));
    assert!(h.vitality() < 0.0, "vitality {}", h.vitality());
    assert!(close(h.max_share(&t), 1.0), "no headache at this air");
    let air_left = 1.0 - h.strength("oxygen_low") / 200.0;
    assert!((air_left - 0.476).abs() < 0.001, "air left {air_left}");
}

#[test]
fn unconscious_at_zero_and_awake_again_above() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    // Burn 110 is 110 points of the maximum: vitality -10, unconscious but not dead.
    h.add(&t, "burn", 110.0);
    let seen = run(&mut h, &t, 0.1, air(1.0));
    assert_eq!(seen, vec![HealthEvent::Unconscious], "unconscious once, not dead: {seen:?}");
    assert_eq!(h.state(), Life::Unconscious);
    assert!(h.vitality() < 0.0 && h.vitality() > -100.0, "below zero, above the dead line: {}", h.vitality());
    // A bandage at full skill takes 18.75 of burn: vitality is above zero again, awake.
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
    assert!((hi - 125.0).abs() < 0.01, "the line is 125 with the shipped numbers: {hi}");
    assert!(t.cpr.stabilize_floor > hi, "the floor {} sits above the line {hi}", t.cpr.stabilize_floor);
}

#[test]
fn cpr_takes_oxygen_low_off_per_second_in_good_air() {
    let t = shipped();
    let mut h = unconscious_at(&t, 150.0);
    // 1.5 s: before the first revive roll (after 2 s of CPR).
    for _ in 0..15 {
        pump(&mut h, &t, 0.1, 50.0, true);
    }
    // Relief 0.5 a second, plus the stabilisation above the floor (skill 50 x 0.02 = 1.0): 1.5 a second.
    assert!(close(h.strength("oxygen_low"), 147.75), "1.5 s of CPR at 150: {}", h.strength("oxygen_low"));
    assert_eq!(h.state(), Life::Unconscious, "still unconscious");
    assert!(close(h.cpr_seconds(), 1.5), "CPR has run 1.5 s: {}", h.cpr_seconds());
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
fn cpr_stabilises_at_the_floor_and_never_wakes_the_body_by_itself() {
    let t = shipped();
    let floor = t.cpr.stabilize_floor;
    let mut h = unconscious_at(&t, 150.0);
    let mut woke = false;
    for _ in 0..600 {
        let ev = pump(&mut h, &t, 0.1, 50.0, true);
        if ev.contains(&HealthEvent::Woke) {
            woke = true;
            break;
        }
        assert!(h.strength("oxygen_low") >= floor - 1e-9, "never below the floor while unconscious: {}", h.strength("oxygen_low"));
        assert_eq!(h.burn_level(&t), 0, "skill 50 costs no burn");
    }
    // The revive roll may wake it; the floor is what it holds without one.
    if !woke {
        assert!(close(h.strength("oxygen_low"), floor) || h.strength("oxygen_low") > floor, "held at the floor: {}", h.strength("oxygen_low"));
    }
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

#[test]
fn a_low_skill_rescuer_hurts_the_body_with_burn_each_pump() {
    let t = shipped();
    let mut h = unconscious_at(&t, t.cpr.stabilize_floor);
    // One whole pump (at 1 s), before the first revive roll (after 2 s of CPR).
    for _ in 0..15 {
        pump(&mut h, &t, 0.1, 30.0, true);
    }
    let want = (t.cpr.damage_below_skill - 30.0) * t.cpr.damage_per_point;
    assert!(close(h.strength("burn"), want), "one pump at skill 30 costs {want} burn: {}", h.strength("burn"));
}

#[test]
fn an_oxy_canister_takes_forty_off_the_oxygen_low() {
    let t = shipped();
    let mut h = unconscious_at(&t, 150.0);
    let r = h.treat(&t, "oxy", 50.0).expect("the oxy treatment");
    assert!(r.success && close(r.reduced, 40.0), "a canister at skill 50: {r:?}");
    assert!(close(h.strength("oxygen_low"), 110.0), "150 - 40: {}", h.strength("oxygen_low"));
    assert_eq!(h.state(), Life::Conscious, "the canister alone wakes the body (110 is above the line)");
}

#[test]
fn the_cpr_numbers_are_in_the_shipped_file_and_checked() {
    let t = shipped();
    assert!(t.cpr.start_s > 0.0 && t.cpr.revive_after_s == 2.0 && t.cpr.revive_low_max < t.cpr.stabilize_floor);
    assert!(t.treatment("oxy").is_some(), "the oxy canister is a treatment");
}

#[test]
fn a_treatment_suits_the_injuries_it_reduces() {
    // The bandage reduces the burn, the deglow the radiation; neither suits the oxygen, and an
    // unknown treatment suits nothing (#300: the medic view's highlight).
    let t = shipped();
    assert!(t.suits("bandage", "burn"));
    assert!(!t.suits("bandage", "radiation"));
    assert!(t.suits("deglow", "radiation"));
    assert!(!t.suits("deglow", "oxygen_low"));
    assert!(!t.suits("nothing", "burn"));
}
