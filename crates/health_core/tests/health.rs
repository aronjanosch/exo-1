//! Health (#254, #287): vitality from afflictions, bands, recovery, oxygen, unconsciousness and
//! death, treatments with skill, overdose, the lab's doses, respawn, and the shipped file.
use std::path::Path;

use content_core::File;
use gameplay_core::{CommodityId, EffectId, ProductSpec};
use health_core::*;
use production_core::devices::{Dose, Syringe};
use production_core::skill::Dice;

fn shipped_files() -> Vec<File> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/gameplay");
    let mut out = Vec::new();
    for d in std::fs::read_dir(&root).unwrap() {
        let d = d.unwrap().path();
        if !d.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&d).unwrap() {
            let f = f.unwrap().path();
            out.push(File::new(f.strip_prefix(&root).unwrap().to_str().unwrap(), std::fs::read_to_string(&f).unwrap()));
        }
    }
    out
}

fn shipped() -> HealthTuning {
    HealthTuning::load(&shipped_files()).unwrap_or_else(|e| panic!("{e:#?}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Steps `secs` in fresh air.
fn run(h: &mut Health, t: &HealthTuning, secs: f64) -> Vec<HealthEvent> {
    let mut ev = Vec::new();
    for _ in 0..(secs * 10.0).round() as usize {
        ev.extend(h.step(t, 0.1, Env { oxygen: 1.0 }));
    }
    ev
}

#[test]
fn vitality_is_max_minus_the_bands_of_each_affliction() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    assert_eq!(h.vitality(), 100.0);
    h.add(&t, "burn", Some(Part::Torso), 30.0);
    assert!(close(h.vitality(), 70.0), "burn 30 is the first band's end: {}", h.vitality());
    h.add(&t, "burn", Some(Part::Torso), 15.0);
    assert!(close(h.vitality(), 55.0), "burn 45 is halfway through the second band: {}", h.vitality());
    h.add(&t, "bleeding", Some(Part::Arms), 20.0);
    assert!(close(h.vitality(), 53.0), "bleeding 20 adds its own band: {}", h.vitality());
    assert!(close(h.strength("burn"), 45.0) && close(h.strength_on("burn", Part::Torso), 45.0));
    h.add(&t, "burn", Some(Part::Torso), 1000.0);
    assert!(close(h.strength("burn"), 200.0), "capped at the affliction's max");
}

#[test]
fn a_limb_affliction_without_a_part_goes_to_the_torso_and_body_wide_ones_have_none() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "burn", None, 10.0);
    h.add(&t, "poison", Some(Part::Head), 10.0);
    assert!(close(h.strength_on("burn", Part::Torso), 10.0));
    assert!(h.afflictions().iter().any(|a| a.id == "poison" && a.part.is_none()));
    assert!(!h.add(&t, "nope", None, 5.0), "an unknown affliction is not added");
}

#[test]
fn bleeding_and_statuses_heal_by_themselves_burns_slowly() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "bleeding", Some(Part::Legs), 10.0);
    h.add(&t, "giggly", None, 5.0);
    h.add(&t, "burn", Some(Part::Arms), 10.0);
    run(&mut h, &t, 10.0);
    assert!(close(h.strength("bleeding"), 9.0), "bleeding -0.1/s: {}", h.strength("bleeding"));
    assert_eq!(h.strength("giggly"), 0.0, "the status wore off");
    assert!(h.afflictions().iter().all(|a| a.id != "giggly"), "an affliction at zero is removed");
    assert!(close(h.strength("burn"), 9.5), "burn -0.05/s: {}", h.strength("burn"));
}

#[test]
fn bad_air_raises_oxygen_low_until_unconscious_then_dead() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    let mut events = Vec::new();
    let mut secs = 0.0;
    while h.state() != Life::Dead && secs < 400.0 {
        events.extend(h.step(&t, 0.1, Env { oxygen: 0.1 }));
        secs += 0.1;
        if close(secs, 10.0) {
            assert!(close(h.strength("oxygen_low"), 50.0), "5 per second in bad air: {}", h.strength("oxygen_low"));
            assert_eq!(h.vitality(), 100.0, "no harm below 50");
        }
    }
    let down = events.iter().position(|e| *e == HealthEvent::Unconscious).expect("goes down first");
    let died = events.iter().position(|e| matches!(e, HealthEvent::Died { .. })).expect("then dies");
    assert!(down < died);
    assert_eq!(events[died], HealthEvent::Died { cause: "oxygen_low".into() });
    assert!(h.vitality() <= -100.0 + 1e-6);
    // 50 at 5/s, 125 (vitality 0) at 25 s, then 1/s unconscious to 200: about 100 s.
    assert!((secs - 100.0).abs() < 2.0, "dead after {secs:.1} s");
    let before = h.clone();
    run(&mut h, &t, 5.0);
    assert_eq!(h, before, "the dead do not change");
}

#[test]
fn fresh_air_brings_oxygen_back() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "oxygen_low", None, 80.0);
    run(&mut h, &t, 5.0);
    assert!(close(h.strength("oxygen_low"), 30.0), "10 per second back: {}", h.strength("oxygen_low"));
}

#[test]
fn a_fracture_slows_walking_and_blocks_sprinting() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    assert_eq!(h.mobility(&t), Mobility { walk: 1.0, sprint: true, conscious: true });
    h.add(&t, "fracture", Some(Part::Legs), 5.0);
    assert!(h.mobility(&t).sprint, "a crack below 10 does not stop a sprint");
    h.add(&t, "fracture", Some(Part::Legs), 20.0);
    assert_eq!(h.mobility(&t), Mobility { walk: 0.5, sprint: false, conscious: true });
    h.add(&t, "burn", None, 200.0);
    assert!(!h.mobility(&t).conscious, "200 burn takes all the vitality");
}

#[test]
fn a_bandage_reduces_by_its_table_scaled_by_skill_and_spreads_over_parts() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "bleeding", Some(Part::Arms), 10.0);
    h.add(&t, "bleeding", Some(Part::Legs), 40.0);
    let r = h.treat(&t, "bandage", Treater { skill: 40.0, potency: 1.0, success: None }).unwrap();
    assert!(r.success, "at the required skill it always works");
    // 30 x skill factor (0.75 + 0.5 x 0.4 = 0.95) = 28.5: 10 to the arms (all of it), 18.5 to the legs.
    assert!(close(r.reduced, 28.5), "{r:?}");
    assert_eq!(h.strength_on("bleeding", Part::Arms), 0.0);
    assert!(close(h.strength_on("bleeding", Part::Legs), 21.5), "{}", h.strength_on("bleeding", Part::Legs));
    assert!(h.treat(&t, "nope", Treater { skill: 40.0, potency: 1.0, success: None }).is_none());
}

#[test]
fn more_skill_heals_more_and_a_low_skill_fails_sometimes() {
    let t = shipped();
    let treat = |skill: f64, seed: u64| {
        let mut h = Health::new(&t, seed);
        h.add(&t, "burn", Some(Part::Torso), 100.0);
        let r = h.treat(&t, "bandage", Treater { skill, potency: 1.0, success: None }).unwrap();
        (r.success, 100.0 - h.strength("burn"))
    };
    assert!(treat(100.0, 1).1 > treat(40.0, 1).1, "skill scales the reduction");
    let fails = (0..200).filter(|s| !treat(0.0, *s).0).count();
    assert!((55..=105).contains(&fails), "skill 0 against 40 fails about 40 %: {fails} of 200");
}

#[test]
fn treating_what_is_not_there_may_poison() {
    let t = shipped();
    let mut poisoned = 0;
    for seed in 0..100 {
        let mut h = Health::new(&t, seed);
        let r = h.treat(&t, "medkit", Treater { skill: 50.0, potency: 1.0, success: Some(true) }).unwrap();
        assert_eq!(r.reduced, 0.0);
        if let Some(p) = r.overdose {
            poisoned += 1;
            assert!(close(h.strength("poison"), p) && p > 0.0);
        }
    }
    assert!((30..=70).contains(&poisoned), "about half of the overdoses poison: {poisoned}");
}

#[test]
fn a_syringe_dose_treats_and_adds_its_statuses() {
    let t = shipped();
    let mut h = Health::new(&t, 3);
    h.add(&t, "burn", Some(Part::Arms), 80.0);
    let mut y = Syringe {
        product: ProductSpec { base: CommodityId::new("blorp_paste"), effects: [EffectId::new("cooling"), EffectId::new("giggly")].into_iter().collect(), quality: 1.0 },
        doses: 2,
    };
    let files = shipped_files();
    let kernel = gameplay_core::Content::load(&files, &["small", "medium", "large"]).unwrap();
    let tun = production_core::DeviceTuning::load(&files, &kernel).unwrap();
    let mut patient = Patient::new(&mut h, &t, 100.0);
    let d: Dose = y.inject(&mut patient, 100.0, &tun, &mut Dice::new(1)).unwrap();
    assert!(d.success);
    let last = patient.last.clone().unwrap();
    // cooling 60 x skill factor 1.25 x strength 1.0 = 75.
    assert!(close(last.reduced, 75.0), "{last:?}");
    assert!(close(h.strength("burn"), 5.0), "{}", h.strength("burn"));
    assert!(close(h.strength("giggly"), 40.0), "the giggles: {}", h.strength("giggly"));
}

#[test]
fn respawn_clears_the_body_and_costs_a_share_of_the_skills() {
    let t = shipped();
    let mut h = Health::new(&t, 1);
    h.add(&t, "burn", None, 150.0);
    let ev = h.step(&t, 0.1, Env { oxygen: 1.0 });
    assert!(ev.contains(&HealthEvent::Unconscious), "{ev:?} at vitality {}", h.vitality());
    h.add(&t, "poison", None, 100.0);
    let ev = h.step(&t, 0.1, Env { oxygen: 1.0 });
    assert!(ev.iter().any(|e| matches!(e, HealthEvent::Died { cause } if cause == "burn")), "{ev:?}");
    assert_eq!(h.respawn(&t), HealthEvent::Respawn { skills_lost: 0.1 });
    assert_eq!((h.state(), h.vitality(), h.afflictions().len()), (Life::Conscious, 100.0, 0));
}

#[test]
fn the_shipped_file_is_checked() {
    let good = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/gameplay").join(HEALTH_PATH)).unwrap();
    let load = |s: &str| HealthTuning::load(&[File::new(HEALTH_PATH, s)]);
    let bad = good.replacen("\"fail_factor\": 0.75, \"reduce\": { \"bleeding\": 30.0", "\"fail_factor\": 0.75, \"reduce\": { \"blooding\": 30.0", 1);
    let e = load(&bad).unwrap_err();
    assert!(e.iter().any(|m| m.contains("treatments[0].reduce") && m.contains("blooding")), "{e:?}");
    let bad = good.replacen("{ \"to\": 60.0, \"vitality\": [0.3, 0.6]", "{ \"to\": 20.0, \"vitality\": [0.3, 0.6]", 1);
    let e = load(&bad).unwrap_err();
    assert!(e.iter().any(|m| m.contains("afflictions[0].bands[1].to")), "{e:?}");
    assert!(load(&good.replacen('{', "{ \"colour\": 1,", 1)).is_err(), "unknown fields are errors");
    assert!(HealthTuning::load(&[]).unwrap_err()[0].contains("missing"));
    let t = shipped();
    assert!(t.affliction("burn").is_some() && t.treatment("cooling").is_some());
}
