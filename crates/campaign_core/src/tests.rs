use super::*;
use serde_json::Value;

// Rng (#274)

#[test]
fn splitmix_of_zero_matches_the_reference_values() {
    let mut r = Rng::new(0);
    assert_eq!(r.next_u64(), 0xE220_A839_7B1D_CDAF);
    assert_eq!(r.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    assert_eq!(r.next_u64(), 0x06C4_5D18_8009_454F);
}

#[test]
fn same_seed_gives_the_same_sequence_and_other_seeds_differ() {
    let a: Vec<u64> = (0..8).map(|_| Rng::new(7).next_u64()).collect();
    let mut x = Rng::new(7);
    let mut y = Rng::new(7);
    for _ in 0..8 {
        assert_eq!(x.next_u64(), y.next_u64());
    }
    assert!(a.iter().all(|v| *v == a[0]), "a fresh Rng starts the same");
    assert_ne!(Rng::new(7).next_u64(), Rng::new(8).next_u64());
}

#[test]
fn unit_stays_in_zero_to_one() {
    let mut r = Rng::new(42);
    for _ in 0..10_000 {
        let u = r.unit();
        assert!((0.0..1.0).contains(&u), "{u}");
    }
}

#[test]
fn count_is_inclusive_at_both_ends() {
    let mut r = Rng::new(3);
    let mut seen = [false; 5];
    for _ in 0..1000 {
        let n = r.count(2, 4);
        assert!((2..=4).contains(&n), "{n}");
        seen[n as usize] = true;
    }
    assert!(seen[2] && seen[3] && seen[4]);
    for _ in 0..100 {
        assert_eq!(r.count(3, 3), 3);
    }
}

#[test]
fn direction_is_a_unit_vector_and_averages_to_zero() {
    let mut r = Rng::new(9);
    let mut mean = glam::DVec3::ZERO;
    for _ in 0..10_000 {
        let d = r.direction();
        assert!((d.length() - 1.0).abs() < 1e-12);
        mean += d;
    }
    mean /= 10_000.0;
    for axis in [mean.x, mean.y, mean.z] {
        assert!(axis.abs() < 0.05, "{axis}");
    }
}

#[test]
fn derived_streams_differ_by_key() {
    assert_ne!(Rng::derive(1, 1).next_u64(), Rng::derive(1, 2).next_u64());
    assert_eq!(Rng::derive(1, 1).next_u64(), Rng::derive(1, 1).next_u64());
}

#[test]
fn pick_of_an_empty_slice_is_none() {
    let mut r = Rng::new(1);
    let empty: [u8; 0] = [];
    assert!(r.pick(&empty).is_none());
    assert!([1, 2, 3].contains(r.pick(&[1, 2, 3]).unwrap()));
}

// Data (#274)

const PLANET_SET: &str = include_str!("../../../content/campaign/planet_set.json");
const NAMES: &str = include_str!("../../../content/campaign/names.json");
const HARBOUR: &str = include_str!("../../../content/campaign/template_harbour.json");
const NORMAL: &str = include_str!("../../../content/campaign/template_normal.json");
const HOSTILE: &str = include_str!("../../../content/campaign/template_hostile.json");

fn set() -> PlanetSet {
    PlanetSet::from_json(PLANET_SET).expect("planet_set.json")
}

fn template_with(json: &str, edit: impl FnOnce(&mut Value)) -> Result<Template, String> {
    let mut v: Value = serde_json::from_str(json).unwrap();
    edit(&mut v);
    Template::from_json(&v.to_string())
}

#[test]
fn shipped_planet_set_parses_and_its_recipes_have_files() {
    let s = set();
    assert!(!s.recipes.is_empty());
    for r in &s.recipes {
        let path = format!("{}/../../content/planet/{}.json", env!("CARGO_MANIFEST_DIR"), r.id);
        assert!(std::fs::metadata(&path).is_ok(), "missing {path}");
    }
}

#[test]
fn shipped_names_parse() {
    let n = Names::from_json(NAMES).expect("names.json");
    assert!(!n.planet.is_empty() && !n.station.is_empty());
}

#[test]
fn shipped_templates_parse_match_their_kind_and_pass_check_set() {
    let s = set();
    for (json, kind) in [
        (HARBOUR, SystemKind::Harbour),
        (NORMAL, SystemKind::Normal),
        (HOSTILE, SystemKind::Hostile),
    ] {
        let t = Template::from_json(json).expect("template");
        assert_eq!(t.kind, kind);
        t.check_set(&s).expect("recipes in set");
    }
}

#[test]
fn planet_set_rejects_a_duplicate_id() {
    let mut v: Value = serde_json::from_str(PLANET_SET).unwrap();
    let first = v["recipes"][0].clone();
    v["recipes"].as_array_mut().unwrap().push(first);
    assert!(PlanetSet::from_json(&v.to_string()).is_err());
}

#[test]
fn names_reject_a_duplicate_entry() {
    let mut v: Value = serde_json::from_str(NAMES).unwrap();
    let first = v["planet"][0].clone();
    v["planet"].as_array_mut().unwrap().push(first);
    assert!(Names::from_json(&v.to_string()).is_err());
}

#[test]
fn template_rejects_an_unknown_field() {
    assert!(template_with(NORMAL, |v| v["extra"] = Value::from(1)).is_err());
}

#[test]
fn template_rejects_a_reversed_range() {
    let err = template_with(NORMAL, |v| v["planets"]["count"] = Value::from(vec![3, 2])).unwrap_err();
    assert!(err.contains("planets.count"), "{err}");
}

#[test]
fn harbour_needs_its_station() {
    assert!(template_with(HARBOUR, |v| v["stations"]["count"] = Value::from(vec![0, 1])).is_err());
}

#[test]
fn hostile_needs_an_enemy() {
    assert!(template_with(HOSTILE, |v| v["enemy"] = Value::Null).is_err());
}

#[test]
fn station_count_stays_at_most_one() {
    assert!(template_with(HOSTILE, |v| v["stations"]["count"] = Value::from(vec![0, 2])).is_err());
}

#[test]
fn template_rejects_an_empty_or_duplicate_recipe_list() {
    assert!(template_with(NORMAL, |v| v["planets"]["recipes"] = Value::from(Vec::<String>::new())).is_err());
    assert!(template_with(NORMAL, |v| v["planets"]["recipes"] = Value::from(vec!["hearth", "hearth"])).is_err());
}

#[test]
fn check_set_rejects_an_unknown_recipe() {
    let t = template_with(NORMAL, |v| v["planets"]["recipes"] = Value::from(vec!["nowhere"])).unwrap();
    assert!(t.check_set(&set()).is_err());
}

#[test]
fn system_name_is_deterministic_and_varies() {
    let n = Names::from_json(NAMES).unwrap();
    assert_eq!(n.system_name(5), n.system_name(5));
    let mut distinct: Vec<String> = (0..200u64).map(|s| n.system_name(s)).collect();
    distinct.sort();
    distinct.dedup();
    assert!(distinct.len() >= 20, "{}", distinct.len());
}
