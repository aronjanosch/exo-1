use super::*;
use glam::DVec3;
use serde_json::Value;
use std::cell::Cell;
use std::collections::HashSet;
use warp_core::{DriveConfig, PlanetDef};

// Rng

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

// Data

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
fn system_name_is_deterministic() {
    let n = Names::from_json(NAMES).unwrap();
    assert_eq!(n.system_name(5), n.system_name(5));
}

// Generator

const SYSTEM: &str = include_str!("../../../content/system/system.json");
const SEEDS: u64 = 300;

fn drive() -> DriveConfig {
    warp_core::System::from_json(SYSTEM).expect("system.json").drive
}

fn generator() -> Generator {
    Generator::new(set(), Names::from_json(NAMES).unwrap(), drive())
}

fn template(json: &str) -> Template {
    Template::from_json(json).expect("template")
}

fn gen_at(g: &Generator, t: &Template, seed: u64) -> Generated {
    g.generate(t, seed, &open_sky).unwrap_or_else(|e| panic!("seed {seed}: {e}"))
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    DVec3::from_array(a).distance(DVec3::from_array(b))
}

fn in_range(v: f64, r: [f64; 2]) -> bool {
    v >= r[0] - 1e-6 && v <= r[1] + 1e-6
}

fn all_templates() -> [Template; 3] {
    [template(HARBOUR), template(NORMAL), template(HOSTILE)]
}

#[test]
fn same_seed_gives_the_same_system() {
    let g = generator();
    let g2 = generator();
    for t in all_templates() {
        for seed in 0..SEEDS {
            let a = format!("{:?}", gen_at(&g, &t, seed));
            assert_eq!(a, format!("{:?}", gen_at(&g, &t, seed)));
            assert_eq!(a, format!("{:?}", gen_at(&g2, &t, seed)));
        }
    }
}

#[test]
fn every_result_passes_system_validate() {
    let g = generator();
    for t in all_templates() {
        for seed in 0..SEEDS {
            let out = gen_at(&g, &t, seed);
            out.system.validate().unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            assert_eq!(out.system.ids().count(), out.system.planets.len());
            assert!(out.system.id(out.system.planets.len() as u32 - 1).is_some());
        }
    }
}

#[test]
fn planets_stay_inside_the_template_ranges() {
    let g = generator();
    let s = set();
    for t in all_templates() {
        let r = &t.planets;
        for seed in 0..SEEDS {
            let out = gen_at(&g, &t, seed);
            let planets = &out.system.planets;
            assert!((r.count[0] as usize..=r.count[1] as usize).contains(&planets.len()), "seed {seed}");
            assert_eq!(planets[0].centre, [0.0, 0.0, 0.0], "seed {seed}");
            for p in planets {
                assert!(in_range(p.radius, r.radius_m), "seed {seed}");
                assert!(in_range(p.atmosphere_height, r.atmosphere_height_m), "seed {seed}");
                assert!(r.recipes.contains(&p.recipe), "seed {seed}: {}", p.recipe);
                let recipe = s.get(&p.recipe).unwrap();
                assert!((p.obstruction_radius - (p.radius + recipe.terrain_m)).abs() < 1e-9, "seed {seed}");
                assert!(p.color.iter().all(|c| (0.0..=1.0).contains(c)), "seed {seed}");
            }
            let names: HashSet<&String> = planets.iter().map(|p| &p.name).collect();
            assert_eq!(names.len(), planets.len(), "seed {seed}: planet names repeat");
            assert!(planets.iter().all(|p| !p.name.is_empty()), "seed {seed}");
        }
    }
}

#[test]
fn stations_sites_and_enemy_sit_on_existing_planets_in_their_ranges() {
    let g = generator();
    for t in all_templates() {
        for seed in 0..SEEDS {
            let out = gen_at(&g, &t, seed);
            let n = out.system.planets.len();
            let planets = &out.system.planets;
            assert!((t.stations.count[0] as usize..=t.stations.count[1] as usize).contains(&out.stations.len()));
            assert!((t.sites.count[0] as usize..=t.sites.count[1] as usize).contains(&out.sites.len()));
            for st in &out.stations {
                let p = &planets[st.planet.index()];
                assert!(st.planet.index() < n, "seed {seed}");
                let alt = dist(st.position, p.centre) - p.radius - p.atmosphere_height;
                assert!(in_range(alt, t.stations.altitude_m), "seed {seed}: altitude {alt}");
            }
            let names: HashSet<&String> = out.stations.iter().map(|s| &s.name).collect();
            assert_eq!(names.len(), out.stations.len(), "seed {seed}: station names repeat");
            for site in &out.sites {
                assert!(site.planet.index() < n, "seed {seed}");
                assert!(t.sites.kinds.contains(&site.kind), "seed {seed}");
            }
            if let Some(e) = &out.enemy {
                assert!(e.planet.index() < n, "seed {seed}");
                let rule = t.enemy.as_ref().expect("enemy only where the template has one");
                let p = &planets[e.planet.index()];
                let alt = dist(e.position, p.centre) - p.radius - p.atmosphere_height;
                assert!(in_range(alt, rule.altitude_m), "seed {seed}: enemy altitude {alt}");
            }
        }
    }
}

#[test]
fn each_kind_has_its_shape() {
    let g = generator();
    let (harbour, normal, hostile) = (template(HARBOUR), template(NORMAL), template(HOSTILE));
    for seed in 0..SEEDS {
        let h = gen_at(&g, &harbour, seed);
        assert_eq!(h.kind, SystemKind::Harbour);
        assert_eq!(h.system.planets.len(), 1);
        assert_eq!(h.stations.len(), 1);
        assert!(h.sites.is_empty() && h.enemy.is_none() && h.events.is_empty(), "seed {seed}");

        let n = gen_at(&g, &normal, seed);
        assert_eq!(n.kind, SystemKind::Normal);
        assert!(n.enemy.is_none(), "seed {seed}");

        let x = gen_at(&g, &hostile, seed);
        assert_eq!(x.kind, SystemKind::Hostile);
        let e = x.enemy.as_ref().unwrap_or_else(|| panic!("seed {seed}: no enemy"));
        assert!(hostile.enemy.as_ref().unwrap().kinds.contains(&e.kind), "seed {seed}");
        assert!(x.events.iter().any(|ev| ev == "ambush"), "seed {seed}");
    }
}

#[test]
fn the_sky_hook_moves_the_star_and_a_refusing_hook_is_an_error() {
    let g = generator();
    let t = template(NORMAL);
    let west = |_: &PlanetDef, s: DVec3| s.x <= 0.0;
    for seed in 0..SEEDS {
        let a = g.generate(&t, seed, &west).unwrap();
        assert!(a.system.star.position().x <= 0.0, "seed {seed}");
        let b = g.generate(&t, seed, &west).unwrap();
        assert_eq!(format!("{a:?}"), format!("{b:?}"), "seed {seed}");
    }
    let err = g.generate(&t, 1, &|_: &PlanetDef, _: DVec3| false).unwrap_err();
    assert!(err.contains("star"), "{err}");
}

#[test]
fn a_recipe_outside_the_set_is_an_error() {
    let t = template_with(NORMAL, |v| v["planets"]["recipes"] = Value::from(vec!["nowhere"])).unwrap();
    let err = generator().generate(&t, 1, &open_sky).unwrap_err();
    assert!(err.contains("planets.recipes"), "{err}");
}

#[test]
fn a_name_pool_too_small_is_an_error() {
    let mut names = Names::from_json(NAMES).unwrap();
    names.planet.truncate(1);
    let g = Generator::new(set(), names, drive());
    let err = g.generate(&template(NORMAL), 1, &open_sky).unwrap_err();
    assert!(err.contains("names.planet") && err.contains("the template needs"), "{err}");

    let mut names = Names::from_json(NAMES).unwrap();
    names.station.clear();
    let g = Generator::new(set(), names, drive());
    let err = g.generate(&template(NORMAL), 1, &open_sky).unwrap_err();
    assert!(err.contains("names.station") && err.contains("the template needs"), "{err}");
}

// Orbit rings

fn star_of(out: &Generated) -> [f64; 3] {
    out.system.star.position
}

#[test]
fn ring_radii_start_in_the_first_ring_and_grow_by_the_ring_gap() {
    let g = generator();
    for t in all_templates() {
        let o = &t.orbits;
        for seed in 0..SEEDS {
            let out = gen_at(&g, &t, seed);
            let planets = &out.system.planets;
            let r: Vec<f64> = planets.iter().map(|p| dist(star_of(&out), p.centre)).collect();
            assert!(in_range(r[0], o.first_ring_m), "seed {seed}: first ring {}", r[0]);
            for i in 1..r.len() {
                let d = r[i] - r[i - 1];
                assert!(d > 0.0, "seed {seed}: rings do not grow");
                assert!(d >= o.ring_gap_m[0] - 1e-3 && d <= o.ring_gap_m[1] + 1e-3, "seed {seed}: gap {d}");
            }
        }
    }
}

#[test]
fn every_body_lies_in_one_plane_through_the_star_tilted_as_the_template_says() {
    let g = generator();
    for t in all_templates() {
        for seed in 0..SEEDS {
            let out = gen_at(&g, &t, seed);
            let planets = &out.system.planets;
            if planets.len() < 2 {
                continue;
            }
            let star = DVec3::from_array(star_of(&out));
            let n = (DVec3::from_array(planets[0].centre) - star)
                .cross(DVec3::from_array(planets[1].centre) - star)
                .normalize();
            for p in planets {
                let off = (DVec3::from_array(p.centre) - star).dot(n).abs();
                assert!(off < 1e-6, "seed {seed}: {} off the plane by {off}", p.name);
            }
            let tilt = n.dot(DVec3::Y).abs().acos().to_degrees();
            assert!(in_range(tilt, t.orbits.plane_tilt_deg), "seed {seed}: tilt {tilt}");
        }
    }
}

#[test]
fn planet_zero_is_at_the_origin_and_all_pairs_keep_their_distance() {
    let g = generator();
    for t in all_templates() {
        for seed in 0..SEEDS {
            let out = gen_at(&g, &t, seed);
            let planets = &out.system.planets;
            assert_eq!(planets[0].centre, [0.0; 3], "seed {seed}");
            for (i, p) in planets.iter().enumerate() {
                for q in &planets[..i] {
                    let d = dist(q.centre, p.centre);
                    assert!(d >= t.orbits.ring_gap_m[0] - 1e-6, "seed {seed}: {d} apart");
                    assert!(d > 2.0 * p.frame_radius, "seed {seed}: frame zones touch");
                }
            }
        }
    }
}

#[test]
fn a_hook_that_refuses_the_first_layouts_still_gets_one() {
    let g = generator();
    let t = template(NORMAL);
    let calls = Cell::new(0u32);
    let refuse_three = |_: &PlanetDef, _: DVec3| {
        let n = calls.get();
        calls.set(n + 1);
        n >= 3
    };
    let out = g.generate(&t, 7, &refuse_three).expect("the fourth layout passes");
    assert!(calls.get() >= 4 && out.system.planets.len() >= 2);
}

#[test]
fn a_hook_that_always_refuses_stops_after_bounded_tries() {
    let t = template(NORMAL);
    let calls = Cell::new(0u32);
    let refuse = |_: &PlanetDef, _: DVec3| {
        calls.set(calls.get() + 1);
        false
    };
    let err = generator().generate(&t, 1, &refuse).unwrap_err();
    assert!(err.contains("star"), "{err}");
    assert!(calls.get() <= 32 * 4, "{} calls", calls.get());
}

#[test]
fn a_ring_gap_inside_the_frame_zones_is_an_error() {
    let t = template_with(NORMAL, |v| v["orbits"]["ring_gap_m"] = Value::from(vec![1.0e6, 2.0e6])).unwrap();
    let err = generator().generate(&t, 1, &open_sky).unwrap_err();
    assert!(err.contains("orbits.ring_gap_m"), "{err}");
}

#[test]
fn template_rejects_bad_orbit_ranges() {
    let err = template_with(NORMAL, |v| v["orbits"]["first_ring_m"] = Value::from(vec![0.0, 5.0e7])).unwrap_err();
    assert!(err.contains("orbits.first_ring_m"), "{err}");
    let err = template_with(NORMAL, |v| v["orbits"]["plane_tilt_deg"] = Value::from(vec![10.0, 120.0])).unwrap_err();
    assert!(err.contains("orbits.plane_tilt_deg"), "{err}");
    let err = template_with(NORMAL, |v| v["orbits"]["ring_gap_m"] = Value::from(vec![3.0e7, 1.5e7])).unwrap_err();
    assert!(err.contains("orbits.ring_gap_m"), "{err}");
    let err = template_with(NORMAL, |v| v["orbits"]["ring_gap_m"] = Value::from(vec![0.0, 1.5e7])).unwrap_err();
    assert!(err.contains("orbits.ring_gap_m"), "{err}");
}

#[test]
fn eight_planets_fit_on_their_rings() {
    let t = template_with(NORMAL, |v| v["planets"]["count"] = Value::from(vec![8, 8])).expect("eight planets pass validate");
    let g = generator();
    for seed in 0..100 {
        let out = gen_at(&g, &t, seed);
        let planets = &out.system.planets;
        assert_eq!(planets.len(), 8, "seed {seed}");
        for (i, p) in planets.iter().enumerate() {
            for q in &planets[..i] {
                assert!(dist(q.centre, p.centre) >= t.orbits.ring_gap_m[0] - 1e-6, "seed {seed}: planets too close");
            }
        }
    }
}

// Campaign map

const MAP_RULES: &str = include_str!("../../../content/campaign/map.json");
const MAP_SEEDS: u64 = 500;

fn map_rules() -> MapRules {
    MapRules::from_json(MAP_RULES).expect("map.json")
}

fn map_names() -> Names {
    Names::from_json(NAMES).expect("names.json")
}

fn gen_map(seed: u64, rules: &MapRules, names: &Names) -> Map {
    Map::generate(seed, rules, names).unwrap_or_else(|e| panic!("seed {seed}: {e}"))
}

fn maps(count: u64) -> Vec<Map> {
    let (rules, names) = (map_rules(), map_names());
    (0..count)
        .map(|seed| gen_map(seed, &rules, &names))
        .collect()
}

fn rules_from(edit: impl FnOnce(&mut Value)) -> Result<MapRules, String> {
    let mut v: Value = serde_json::from_str(MAP_RULES).unwrap();
    edit(&mut v);
    MapRules::from_json(&v.to_string())
}

#[test]
fn map_same_seed_same_map() {
    let (rules, names) = (map_rules(), map_names());
    for seed in [0, 1, 42, u64::MAX] {
        assert_eq!(
            gen_map(seed, &rules, &names),
            gen_map(seed, &rules, &names),
            "seed {seed}"
        );
    }
}

#[test]
fn map_generates_and_checks_for_every_seed() {
    let (rules, names) = (map_rules(), map_names());
    for seed in 0..MAP_SEEDS {
        let m = gen_map(seed, &rules, &names);
        m.check(&rules)
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    }
}

#[test]
fn map_every_system_is_reachable_from_the_start() {
    for m in maps(MAP_SEEDS) {
        let mut seen = vec![Map::START];
        let mut todo = vec![Map::START];
        while let Some(id) = todo.pop() {
            for &next in m.forward(id) {
                if !seen.contains(&next) {
                    seen.push(next);
                    todo.push(next);
                }
            }
        }
        assert_eq!(seen.len(), m.nodes.len(), "seed {}", m.seed);
    }
}

#[test]
fn map_fans_out_two_or_three_into_the_next_column() {
    for m in maps(MAP_SEEDS) {
        let last = m.column_count() - 1;
        for n in &m.nodes {
            if n.column == last {
                assert!(n.forward.is_empty(), "seed {}: {:?}", m.seed, n);
                continue;
            }
            for t in &n.forward {
                assert_eq!(m.node(*t).unwrap().column, n.column + 1, "seed {}", m.seed);
            }
        }
    }
}

#[test]
fn map_type_mix_follows_the_rules() {
    let rules = map_rules();
    for m in maps(MAP_SEEDS) {
        assert_eq!(m.nodes[0].kind, SystemKind::Harbour, "seed {}", m.seed);
        for n in m.nodes.iter().filter(|n| !m.is_last(n.id)) {
            let way_out = m
                .forward(n.id)
                .iter()
                .any(|t| m.node(*t).unwrap().kind != SystemKind::Hostile);
            assert!(way_out, "seed {}: {:?} has no safe way on", m.seed, n.id);
        }
        let harbours = m
            .nodes
            .iter()
            .filter(|n| n.kind == SystemKind::Harbour)
            .count() as u32;
        assert!(
            (2..=rules.max_harbours).contains(&harbours),
            "seed {}: {harbours} harbours",
            m.seed
        );
    }
}

#[test]
fn map_back_agrees_with_the_forward_edges() {
    for m in maps(MAP_SEEDS) {
        assert!(m.back(Map::START).is_empty());
        let mut reverse: Vec<Vec<NodeId>> = vec![Vec::new(); m.nodes.len()];
        for n in &m.nodes {
            for t in &n.forward {
                reverse[t.0 as usize].push(n.id);
            }
        }
        for n in &m.nodes {
            let mut expected = reverse[n.id.0 as usize].clone();
            expected.sort();
            assert_eq!(m.back(n.id), expected, "seed {} node {:?}", m.seed, n.id);
        }
    }
}

#[test]
fn map_names_and_system_seeds_are_unique() {
    for m in maps(MAP_SEEDS) {
        let names: HashSet<&str> = m.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names.len(), m.nodes.len(), "seed {}: names repeat", m.seed);
        let seeds: HashSet<u64> = m.nodes.iter().map(|n| n.seed).collect();
        assert_eq!(
            seeds.len(),
            m.nodes.len(),
            "seed {}: system seeds repeat",
            m.seed
        );
    }
}

#[test]
fn map_check_rejects_broken_maps() {
    let (rules, names) = (map_rules(), map_names());
    let base = gen_map(42, &rules, &names);
    assert!(base.check(&rules).is_ok());

    let mut m = base.clone();
    let n = m.nodes.iter_mut().find(|n| n.column == 1).unwrap();
    n.kind = SystemKind::Hostile;
    n.danger = 4;
    assert!(m.check(&rules).is_err(), "hostile in column 1");

    let mut m = base.clone();
    m.nodes[0].forward.truncate(1);
    assert!(m.check(&rules).is_err(), "one forward target");

    let mut m = base.clone();
    let n = m.nodes.iter_mut().find(|n| n.column == 1).unwrap();
    n.kind = SystemKind::Harbour;
    n.danger = 0;
    assert!(m.check(&rules).is_err(), "second harbour in column 1");

    let mut m = base.clone();
    let from = m
        .nodes
        .iter()
        .find(|n| n.column == 1)
        .unwrap()
        .forward
        .clone();
    for t in from {
        let target = &mut m.nodes[t.0 as usize];
        target.kind = SystemKind::Hostile;
        target.danger = 4;
    }
    assert!(m.check(&rules).is_err(), "all forward targets hostile");

    let mut m = base.clone();
    m.nodes[1].name = m.nodes[2].name.clone();
    assert!(m.check(&rules).is_err(), "duplicate name");

    let found = (0..MAP_SEEDS).find_map(|seed| {
        let m = gen_map(seed, &rules, &names);
        let (p, x) = m
            .nodes
            .iter()
            .filter(|n| n.forward.len() == 3)
            .find_map(|p| {
                p.forward
                    .iter()
                    .copied()
                    .find(|x| m.back(*x).len() == 1)
                    .map(|x| (p.id, x))
            })?;
        Some((m, p, x))
    });
    let (mut m, p, x) = found.expect("a seed with a lonely edge");
    m.nodes[p.0 as usize].forward.retain(|t| *t != x);
    let err = m.check(&rules).expect_err("unreachable node");
    assert!(err.contains("reachable"), "{err}");
}

#[test]
fn map_rules_reject_bad_values() {
    assert!(rules_from(|_| {}).is_ok());
    assert!(
        rules_from(|v| v["columns"][1] = serde_json::json!([1, 3])).is_err(),
        "column below forward[0]"
    );
    assert!(
        rules_from(|v| v["columns"][0] = serde_json::json!([1, 2])).is_err(),
        "column 0 not [1, 1]"
    );
    assert!(
        rules_from(|v| v["max_nodes"] = serde_json::json!(6)).is_err(),
        "max_nodes below the minimums"
    );
    assert!(
        rules_from(|v| v["max_harbours"] = serde_json::json!(1)).is_err(),
        "max_harbours below 2"
    );
    assert!(
        rules_from(|v| v["weights"] = serde_json::json!({"harbour": 0, "normal": 0, "hostile": 0}))
            .is_err(),
        "all weights zero"
    );
    assert!(
        rules_from(|v| v["danger"]["normal"] = serde_json::json!([1, 6])).is_err(),
        "danger above 5"
    );
    assert!(
        rules_from(|v| v["bogus"] = serde_json::json!(1)).is_err(),
        "unknown field"
    );
}

// Progress

fn progress_walk(map: &Map, mut pick: impl FnMut(&[NodeId], usize) -> NodeId) -> Progress {
    let mut p = Progress::start(map);
    let mut jumps = 0;
    while !map.is_last(p.current) {
        let ts = p.targets(map, false);
        let to = pick(&ts, jumps);
        let before = p.visited.clone();
        p.jump(map, to, false).unwrap_or_else(|e| panic!("jump to {}: {e}", to.0));
        assert!(p.visited.len() == before.len() + 1, "one more visited");
        jumps += 1;
    }
    assert_eq!(jumps, map.column_count() as usize - 1);
    p
}

fn assert_visited_sane(p: &Progress) {
    assert!(p.visited.windows(2).all(|w| w[0] < w[1]), "{:?}", p.visited);
    assert!(p.visited.contains(&p.current), "{:?}", p.visited);
}

#[test]
fn progress_start_is_the_start_node_and_checks() {
    let (rules, names) = (map_rules(), map_names());
    for seed in [0, 1, 42] {
        let map = gen_map(seed, &rules, &names);
        let p = Progress::start(&map);
        assert_eq!(p.current, Map::START);
        assert_eq!(p.visited, vec![Map::START]);
        assert!(p.has_visited(Map::START));
        p.check(&map).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    }
}

#[test]
fn progress_walks_to_the_last_column_for_every_seed() {
    let (rules, names) = (map_rules(), map_names());
    for seed in 0..200 {
        let map = gen_map(seed, &rules, &names);
        let mut rng = Rng::new(seed ^ 0x57A1);
        let walks = [
            progress_walk(&map, |ts, _| ts[0]),
            progress_walk(&map, |ts, _| *ts.last().unwrap()),
            progress_walk(&map, |ts, _| *rng.pick(ts).unwrap()),
        ];
        for p in &walks {
            assert_visited_sane(p);
            p.check(&map).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
            assert!(map.is_last(p.current), "seed {seed}");
            assert!(p.targets(&map, false).is_empty(), "seed {seed}");
            for id in 0..map.nodes.len() {
                let mut q = p.clone();
                assert!(q.jump(&map, NodeId(id as u8), false).is_err(), "seed {seed}: jump from the end");
                assert_eq!(&q, p);
            }
        }
    }
}

#[test]
fn progress_illegal_jumps_are_errors_and_change_nothing() {
    let (rules, names) = (map_rules(), map_names());
    let map = gen_map(1, &rules, &names);
    let mut p = Progress::start(&map);
    let two_ahead = map.nodes.iter().find(|n| n.column == 2).unwrap().id;
    let unknown = NodeId(255);
    assert!(map.node(unknown).is_none());
    for to in [two_ahead, Map::START, unknown] {
        let before = p.clone();
        assert!(p.jump(&map, to, false).is_err(), "jump to {}", to.0);
        assert_eq!(p, before);
    }
    let first = p.targets(&map, false)[0];
    p.jump(&map, first, false).unwrap();
    let before = p.clone();
    assert!(p.jump(&map, Map::START, false).is_err(), "backward without allow_back");
    assert_eq!(p, before);
}

#[test]
fn progress_allow_back_reaches_the_node_you_came_from() {
    let (rules, names) = (map_rules(), map_names());
    let map = gen_map(1, &rules, &names);
    let mut p = Progress::start(&map);
    let first = p.targets(&map, false)[0];
    p.jump(&map, first, true).unwrap();
    let ts = p.targets(&map, true);
    assert!(ts.contains(&Map::START), "{ts:?}");
    let mut want: Vec<NodeId> = map.forward(first).iter().copied().chain(map.back(first)).collect();
    want.sort();
    want.dedup();
    assert_eq!(ts, want);
    assert!(ts.windows(2).all(|w| w[0] < w[1]));
    p.jump(&map, Map::START, true).unwrap();
    assert_eq!(p.current, Map::START);
    assert_eq!(p.visited, vec![Map::START, first]);
    assert_visited_sane(&p);
}

#[test]
fn progress_of_another_map_is_refused() {
    let (rules, names) = (map_rules(), map_names());
    let (a, b) = (gen_map(1, &rules, &names), gen_map(2, &rules, &names));
    let mut p = Progress::start(&a);
    let before = p.clone();
    let to = p.targets(&a, false)[0];
    assert!(p.jump(&b, to, false).is_err());
    assert_eq!(p, before);
    assert!(p.check(&b).is_err());
    assert!(p.check(&a).is_ok());
}

#[test]
fn progress_json_round_trip_and_keys() {
    let (rules, names) = (map_rules(), map_names());
    let map = gen_map(1, &rules, &names);
    let p = Progress::start(&map);
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(json, r#"{"seed":1,"current":0,"visited":[0]}"#);
    let back: Progress = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);
}

#[test]
fn progress_check_rejects_hand_edited_states() {
    let (rules, names) = (map_rules(), map_names());
    let map = gen_map(1, &rules, &names);
    let seed = map.seed;
    let s = Map::START;
    let other = NodeId(1);
    let bad = [
        Progress { seed, current: s, visited: vec![NodeId(3), s] },
        Progress { seed, current: s, visited: vec![s, s] },
        Progress { seed, current: other, visited: vec![s] },
        Progress { seed, current: s, visited: vec![s, NodeId(255)] },
        Progress { seed, current: other, visited: vec![other] },
    ];
    for p in &bad {
        assert!(p.check(&map).is_err(), "{p:?}");
    }
    assert!(Progress::start(&map).check(&map).is_ok());
}

#[test]
fn progress_loaded_after_two_jumps_keeps_going() {
    let (rules, names) = (map_rules(), map_names());
    let map = gen_map(7, &rules, &names);
    let mut p = Progress::start(&map);
    for _ in 0..2 {
        let to = p.targets(&map, false)[0];
        p.jump(&map, to, false).unwrap();
    }
    let json = serde_json::to_string(&p).unwrap();
    let mut loaded: Progress = serde_json::from_str(&json).unwrap();
    loaded.check(&map).unwrap();
    while !map.is_last(loaded.current) {
        let to = *loaded.targets(&map, false).last().unwrap();
        loaded.jump(&map, to, false).unwrap();
    }
    assert_eq!(loaded.visited.len(), map.column_count() as usize);
    assert_visited_sane(&loaded);
}
