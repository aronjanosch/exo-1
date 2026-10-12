use super::*;
use glam::DVec3;
use serde_json::Value;
use std::collections::HashSet;
use warp_core::{DriveConfig, PlanetDef};

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

// Generator (#274)

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
fn same_seed_gives_the_same_system_and_other_seeds_differ() {
    let g = generator();
    let g2 = generator();
    for t in all_templates() {
        for seed in 0..SEEDS {
            let a = format!("{:?}", gen_at(&g, &t, seed));
            assert_eq!(a, format!("{:?}", gen_at(&g, &t, seed)));
            assert_eq!(a, format!("{:?}", gen_at(&g2, &t, seed)));
        }
        let distinct: HashSet<String> = (0..50).map(|s| format!("{:?}", gen_at(&g, &t, s))).collect();
        assert!(distinct.len() >= 45, "{} distinct of 50", distinct.len());
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
            assert!(in_range(dist([0.0; 3], out.system.star.position), t.star.distance_m), "seed {seed}");
            for (i, p) in planets.iter().enumerate() {
                assert!(in_range(p.radius, r.radius_m), "seed {seed}");
                assert!(in_range(p.atmosphere_height, r.atmosphere_height_m), "seed {seed}");
                assert!(r.recipes.contains(&p.recipe), "seed {seed}: {}", p.recipe);
                let recipe = s.get(&p.recipe).unwrap();
                assert!((p.obstruction_radius - (p.radius + recipe.terrain_m)).abs() < 1e-9, "seed {seed}");
                assert!(p.color.iter().all(|c| (0.0..=1.0).contains(c)), "seed {seed}");
                if i > 0 {
                    let prev = planets[i - 1].centre;
                    assert!(in_range(dist(prev, p.centre), r.gap_m), "seed {seed}: consecutive planets");
                }
                for q in &planets[..i] {
                    assert!(dist(q.centre, p.centre) >= r.gap_m[0] - 1e-6, "seed {seed}: planets too close");
                }
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
        assert!((2..=4).contains(&n.system.planets.len()), "seed {seed}");
        assert!(n.enemy.is_none(), "seed {seed}");
        assert_eq!(n.sites.len(), 1, "seed {seed}");

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

#[test]
fn the_shipped_templates_together_use_every_recipe_of_the_set() {
    let g = generator();
    let mut used: HashSet<String> = HashSet::new();
    for t in all_templates() {
        for seed in 0..SEEDS {
            for p in gen_at(&g, &t, seed).system.planets {
                used.insert(p.recipe);
            }
        }
    }
    for r in &set().recipes {
        assert!(used.contains(&r.id), "recipe {} never generated", r.id);
    }
}

// Pinned: a change here changes every saved campaign (the map is regenerated from its seed).
#[test]
fn normal_seed_42_is_pinned() {
    let out = gen_at(&generator(), &template(NORMAL), 42);
    assert_eq!(out.name, "Trundle Drift");
    assert_eq!(out.kind, SystemKind::Normal);
    let planets: Vec<(&str, &str, i32)> =
        out.system.planets.iter().map(|p| (p.name.as_str(), p.recipe.as_str(), p.seed)).collect();
    assert_eq!(planets, [("Mumble", "cinder", 801_617), ("Sprocket", "cinder", 544_030)]);
}

#[test]
fn layout_retries_and_says_when_there_is_no_room() {
    let mut v: Value = serde_json::from_str(NORMAL).unwrap();
    v["planets"]["count"] = Value::from(vec![8, 8]);
    v["planets"]["gap_m"] = Value::from(vec![1.0e7, 1.0e7]);
    let t = Template::from_json(&v.to_string()).expect("eight planets pass validate");
    let g = generator();
    let mut ok = 0;
    for seed in 0..100 {
        match g.generate(&t, seed, &open_sky) {
            Ok(out) => {
                ok += 1;
                let planets = &out.system.planets;
                assert_eq!(planets.len(), 8, "seed {seed}");
                for (i, p) in planets.iter().enumerate() {
                    for q in &planets[..i] {
                        assert!(dist(q.centre, p.centre) >= 1.0e7 - 1e-6, "seed {seed}: planets too close");
                    }
                }
            }
            Err(e) => assert!(e.contains("no room for planet"), "seed {seed}: {e}"),
        }
    }
    assert!(ok > 0, "no seed found a layout");
}

// Campaign map (#275)

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
    (0..count).map(|seed| gen_map(seed, &rules, &names)).collect()
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
        assert_eq!(gen_map(seed, &rules, &names), gen_map(seed, &rules, &names), "seed {seed}");
    }
}

#[test]
fn map_different_seeds_give_different_maps() {
    let all = maps(50);
    let distinct = all.iter().enumerate().filter(|(i, m)| !all[..*i].contains(*m)).count();
    assert!(distinct >= 45, "{distinct} distinct maps of 50");
}

#[test]
fn map_generates_and_checks_for_every_seed() {
    let (rules, names) = (map_rules(), map_names());
    for seed in 0..MAP_SEEDS {
        let m = gen_map(seed, &rules, &names);
        m.check(&rules).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
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
            assert!((2..=3).contains(&n.forward.len()), "seed {}: {:?}", m.seed, n);
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
        for n in m.nodes.iter().filter(|n| n.column == 1) {
            assert_eq!(n.kind, SystemKind::Normal, "seed {}", m.seed);
        }
        for n in m.nodes.iter().filter(|n| !m.is_last(n.id)) {
            let way_out = m.forward(n.id).iter().any(|t| m.node(*t).unwrap().kind != SystemKind::Hostile);
            assert!(way_out, "seed {}: {:?} has no safe way on", m.seed, n.id);
        }
        let harbours = m.nodes.iter().filter(|n| n.kind == SystemKind::Harbour).count() as u32;
        assert!((2..=rules.max_harbours).contains(&harbours), "seed {}: {harbours} harbours", m.seed);
        assert!(m.nodes.iter().any(|n| n.kind == SystemKind::Hostile), "seed {}", m.seed);
    }
}

#[test]
fn map_sizes_for_the_shipped_rules() {
    for m in maps(MAP_SEEDS) {
        assert!((7..=8).contains(&m.nodes.len()), "seed {}: {} nodes", m.seed, m.nodes.len());
        assert_eq!(m.column_count(), 4, "seed {}", m.seed);
        assert_eq!(m.nodes.iter().filter(|n| n.column == 3).count(), 2, "seed {}", m.seed);
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
fn map_danger_stays_in_the_kind_range_and_grows_with_the_column() {
    let (mut first, mut first_n, mut last, mut last_n) = (0u32, 0u32, 0u32, 0u32);
    for m in maps(MAP_SEEDS) {
        for n in &m.nodes {
            let (lo, hi) = match n.kind {
                SystemKind::Harbour => (0, 0),
                SystemKind::Normal => (1, 3),
                SystemKind::Hostile => (3, 5),
            };
            assert!((lo..=hi).contains(&n.danger), "seed {}: {:?} danger {}", m.seed, n.kind, n.danger);
            if n.column == 1 {
                first += u32::from(n.danger);
                first_n += 1;
            }
            if m.is_last(n.id) {
                last += u32::from(n.danger);
                last_n += 1;
            }
        }
    }
    let (mean_first, mean_last) = (f64::from(first) / f64::from(first_n), f64::from(last) / f64::from(last_n));
    assert!(mean_last > mean_first, "column 1 mean {mean_first}, last column mean {mean_last}");
}

#[test]
fn map_names_and_system_seeds_are_unique() {
    for m in maps(MAP_SEEDS) {
        let names: HashSet<&str> = m.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names.len(), m.nodes.len(), "seed {}: names repeat", m.seed);
        let seeds: HashSet<u64> = m.nodes.iter().map(|n| n.seed).collect();
        assert_eq!(seeds.len(), m.nodes.len(), "seed {}: system seeds repeat", m.seed);
    }
}

#[test]
fn map_all_kinds_occur_in_the_middle_columns() {
    let mut seen = HashSet::new();
    for m in maps(MAP_SEEDS) {
        seen.extend(m.nodes.iter().filter(|n| (1..=3).contains(&n.column)).map(|n| n.kind));
    }
    for kind in SystemKind::ALL {
        assert!(seen.contains(&kind), "{kind:?} never occurs");
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
    let from = m.nodes.iter().find(|n| n.column == 1).unwrap().forward.clone();
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
        let (p, x) = m.nodes.iter().filter(|n| n.forward.len() == 3).find_map(|p| {
            p.forward.iter().copied().find(|x| m.back(*x).len() == 1).map(|x| (p.id, x))
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
    assert!(rules_from(|v| v["columns"][1] = serde_json::json!([1, 3])).is_err(), "column below forward[0]");
    assert!(rules_from(|v| v["columns"][0] = serde_json::json!([1, 2])).is_err(), "column 0 not [1, 1]");
    assert!(rules_from(|v| v["max_nodes"] = serde_json::json!(6)).is_err(), "max_nodes below the minimums");
    assert!(rules_from(|v| v["max_harbours"] = serde_json::json!(1)).is_err(), "max_harbours below 2");
    assert!(
        rules_from(|v| v["weights"] = serde_json::json!({"harbour": 0, "normal": 0, "hostile": 0})).is_err(),
        "all weights zero"
    );
    assert!(rules_from(|v| v["danger"]["normal"] = serde_json::json!([1, 6])).is_err(), "danger above 5");
    assert!(rules_from(|v| v["bogus"] = serde_json::json!(1)).is_err(), "unknown field");
}
