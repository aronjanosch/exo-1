//! #72: rivers and lakes from drainage on the macro grid. On the baked planets the water runs
//! downhill, lakes sit in sinks, rivers end in the sea or a lake, the same seed gives the same
//! water. The algorithm's own cases on a flat test grid are in `src/drainage.rs`.
use planet_core::drainage::Mouth;
use planet_core::look::{tangent_frame, walk};
use planet_core::*;
use std::sync::OnceLock;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");
const CINDER: &str = include_str!("../../../content/planet/cinder.json");

fn baked(text: &str, seed: i32, threads: usize) -> (Planet, BakeStats) {
    let mut p = Planet::new(Recipe::for_planet(text, seed, 5000.0).unwrap());
    let st = p.bake_checked(threads).unwrap();
    (p, st)
}

static PLANET: OnceLock<(Planet, BakeStats)> = OnceLock::new();
fn hearth() -> &'static (Planet, BakeStats) {
    PLANET.get_or_init(|| baked(HEARTH, 1337, 0))
}

fn level_at(p: &Planet, d: V3) -> Option<f64> {
    let face = face_of(d);
    let (a, b) = sphere_to_face_ab(face, d);
    p.water_level_ab(face, a, b)
}

#[test]
fn hearth_has_rivers_and_lakes() {
    let (_, st) = hearth();
    println!(
        "rivers {} nodes, {:.1} km (longest {:.2} km, through lakes {:.2} km), {} to the sea, {} to a lake, largest catchment {:.1} km²; lakes {} ({:.2} % of the surface, largest {:.3} km², deepest {:.1} m); erosion max {:.1} m, mean {:.2} m; cut max {:.1} m; drainage {:.0} ms of {:.0} ms",
        st.river_nodes,
        st.river_length_km,
        st.longest_river_km,
        st.longest_waterway_km,
        st.rivers_to_sea,
        st.rivers_to_lake,
        st.largest_catchment_km2,
        st.lake_count,
        st.lake_area_share * 100.0,
        st.largest_lake_km2,
        st.deepest_lake_m,
        st.erosion_max_m,
        st.erosion_mean_m,
        st.carve_max_m,
        st.drainage_ms,
        st.bake_ms
    );
    println!("drainage steps (ms): {:?}", st.drainage_phases_ms);
    assert!(st.river_length_km > 20.0, "a wet planet has rivers");
    assert!(st.rivers_to_sea > 0 && st.lake_count > 0);
}

#[test]
fn water_flows_downhill() {
    let (p, _) = hearth();
    for rv in &p.rivers {
        let ground = p.base_height_at(rv.dir);
        assert!((ground - rv.bed_m).abs() < 0.05, "the ground at a river vertex is its bed: {ground} vs {}", rv.bed_m);
        assert!(rv.level_m > rv.bed_m);
        assert!(level_at(p, rv.dir).is_some_and(|l| l > ground), "the river is wet at {:?}", rv.dir);
        if let Mouth::River(j) = rv.next {
            let n = &p.rivers[j as usize];
            assert!(n.bed_m <= rv.bed_m && n.level_m <= rv.level_m, "bed or water rises downstream");
            assert!(p.base_height_at(n.dir) <= ground + 0.05, "the ground rises downstream");
            assert!(n.catchment_km2 >= rv.catchment_km2, "water gets lost downstream");
        }
    }
}

#[test]
fn lakes_sit_in_sinks() {
    let (p, _) = hearth();
    assert!(!p.lakes.is_empty());
    for l in &p.lakes {
        let deep = p.base_height_at(l.deepest);
        assert!(l.level_m - deep >= l.depth_m - 0.05 && l.depth_m > 0.0, "the deepest point lies {} m under the level", l.level_m - deep);
        assert!(level_at(p, l.deepest).is_some_and(|w| (w - l.level_m).abs() < 1e-3), "the lake is flat at its level");
        assert!(l.level_m > p.sea, "a lake above the sea");
        // Out from the deepest point the ground rises above the level before the water ends,
        // in every direction but along a river (the outlet's, one coming in).
        let (e, n) = tangent_frame(l.deepest);
        let mut leaks = 0;
        for k in 0..32 {
            let a = k as f64 * std::f64::consts::TAU / 32.0;
            let t = e * a.cos() + n * a.sin();
            let mut m = 0.0;
            loop {
                m += 5.0;
                let d = walk(l.deepest, t, m, p.radius);
                if level_at(p, d).is_none_or(|w| w <= p.base_height_at(d)) {
                    let river = p.rivers.iter().any(|r| (r.dir - d).length() * p.radius < 30.0);
                    if p.base_height_at(d) < l.level_m - 0.5 && !river {
                        leaks += 1;
                    }
                    break;
                }
                assert!(m < 20_000.0, "the lake never ends");
            }
        }
        assert!(leaks <= 4, "lake at {:.1} m leaks in {leaks} of 32 directions", l.level_m);
    }
}

#[test]
fn rivers_end_in_the_sea_or_a_lake() {
    let (p, _) = hearth();
    let near = |d: V3, f: &dyn Fn(V3) -> bool| (0..16).any(|k| {
        let (e, n) = tangent_frame(d);
        let a = k as f64 * std::f64::consts::TAU / 16.0;
        (1..=6).any(|s| f(walk(d, e * a.cos() + n * a.sin(), s as f64 * 5.0, p.radius)))
    });
    let (mut sea, mut lake) = (0, 0);
    for start in 0..p.rivers.len() {
        let mut at = start;
        for _ in 0..=p.rivers.len() {
            match p.rivers[at].next {
                Mouth::River(j) => at = j as usize,
                Mouth::Sea => break,
                Mouth::Lake(_) => break,
            }
        }
        let end = &p.rivers[at];
        match end.next {
            Mouth::Sea => {
                if at == start {
                    sea += 1;
                    assert!(near(end.dir, &|d| p.base_height_at(d) < p.sea), "a river ends at the sea, but no sea near {:?}", end.dir);
                }
            }
            Mouth::Lake(id) => {
                if at == start {
                    lake += 1;
                    let l = &p.lakes[id as usize];
                    // Wet water between the lake's level and the river's: a river may drop into the
                    // lake, or the lake may drown the river's last metres.
                    let (lo, hi) = (l.level_m.min(end.level_m) - 1e-3, l.level_m.max(end.level_m) + 1e-3);
                    let joins = |d: V3| level_at(p, d).is_some_and(|w| w >= lo && w <= hi && w > p.base_height_at(d));
                    assert!(near(end.dir, &joins), "a river ends at a lake, but no lake water near {:?}", end.dir);
                }
            }
            Mouth::River(_) => panic!("river {start} runs in a circle"),
        }
    }
    println!("river ends: {sea} at the sea, {lake} at a lake");
    assert!(sea > 0);
}

#[test]
fn same_seed_same_water() {
    let low = HEARTH.replace("\"resolution\": 512", "\"resolution\": 128");
    let (a, _) = baked(&low, 1337, 1);
    let (b, _) = baked(&low, 1337, 0);
    let (c, _) = baked(&low, 1338, 0);
    assert!(!a.rivers.is_empty());
    assert_eq!(a.macro_img, b.macro_img, "the macro image (cut and water) is the same on any thread count");
    assert_eq!(a.rivers.len(), b.rivers.len());
    for (x, y) in a.rivers.iter().zip(&b.rivers) {
        assert_eq!((x.dir, x.bed_m, x.level_m, x.next), (y.dir, y.bed_m, y.level_m, y.next));
    }
    assert_eq!(a.lakes.len(), b.lakes.len());
    for (x, y) in a.lakes.iter().zip(&b.lakes) {
        assert_eq!((x.deepest, x.level_m, x.area_m2), (y.deepest, y.level_m, y.area_m2));
    }
    assert!(a.rivers.len() != c.rivers.len() || a.rivers.iter().zip(&c.rivers).any(|(x, y)| x.dir != y.dir), "another seed, other rivers");
}

#[test]
fn nothing_grows_or_builds_in_the_water() {
    let (p, _) = hearth();
    for s in &p.sites {
        assert!(p.sample(s.dir).water_depth == 0.0, "site {} stands in water", s.id);
    }
    // Trees and shrubs keep out of lakes and rivers; scatter cells along the biggest rivers.
    let mut checked = 0;
    for rv in p.rivers.iter().filter(|r| r.catchment_km2 > 1.0).step_by(20) {
        let face = face_of(rv.dir);
        let (a, b) = sphere_to_face_ab(face, rv.dir);
        let size = 2.0 / 64.0;
        let (a0, b0) = (((a + 1.0) / size).floor() * size - 1.0, ((b + 1.0) / size).floor() * size - 1.0);
        // Entries whose height band starts above the water (the plants; rocks may lie under it).
        let dry: Vec<bool> = p.recipe.scatter.groups.iter().flat_map(|g| g.entries.iter()).map(|e| e.height_above_sea_m[0] > 0.0).collect();
        for storey in 0..3 {
            let cell = p.build_scatter(face, a0, b0, size, storey);
            for inst in &cell.instances {
                let d = (V3::from_arr(cell.center) + V3::from_arr(inst.pos.map(|x| x as f64))).normalized();
                let level = level_at(p, d);
                let wet = level.is_some_and(|w| w > p.base_height_at(d) + 0.05);
                assert!(!(dry[inst.entry as usize] && wet), "a plant in the water at {d:?}");
                if level.is_some() {
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 100, "only {checked} instances near the water");
}

#[test]
fn cinder_stays_dry_but_drains() {
    let (p, st) = baked(CINDER, 4242, 0);
    let hearth = &hearth().1;
    println!(
        "Cinder: largest catchment {:.2} km², rivers {:.1} km (longest {:.2} km, through lakes {:.2} km; {} to the sea, {} to a lake), lakes {} ({:.2} % of the surface, {} never spill), drainage {:.0} ms",
        st.largest_catchment_km2,
        st.river_length_km,
        st.longest_river_km,
        st.longest_waterway_km,
        st.rivers_to_sea,
        st.rivers_to_lake,
        st.lake_count,
        st.lake_area_share * 100.0,
        p.lakes.iter().filter(|l| l.outlet.is_none()).count(),
        st.drainage_ms
    );
    assert!(st.river_length_km > 0.0, "a dry planet still drains somewhere");
    assert!(st.river_length_km < hearth.river_length_km && st.lake_area_share < hearth.lake_area_share, "less water than on the wet planet");
    assert!(p.lakes.iter().any(|l| l.outlet.is_none()), "a dry climate keeps lakes that never spill");
    for l in &p.lakes {
        let deep = p.base_height_at(l.deepest);
        assert!(l.level_m > deep && level_at(&p, l.deepest).is_some_and(|w| (w - l.level_m).abs() < 1e-3), "a lake is wet at its level");
    }
    for start in 0..p.rivers.len() {
        let mut at = start;
        for _ in 0..=p.rivers.len() {
            match p.rivers[at].next {
                Mouth::River(j) => at = j as usize,
                _ => break,
            }
        }
        assert!(!matches!(p.rivers[at].next, Mouth::River(_)), "river {start} runs in a circle");
    }
}

/// Bake time with and without drainage, the best of five each, alternating (load from other
/// work spoils single runs). `cargo test -p planet_core --test drainage bake_time -- --ignored --nocapture`
#[test]
#[ignore]
fn bake_time() {
    let dry: String = {
        let i = HEARTH.find("  \"drainage\": {").unwrap();
        let j = HEARTH[i..].find("\n  },\n").unwrap() + i + "\n  },\n".len();
        format!("{}{}", &HEARTH[..i], &HEARTH[j..])
    };
    let (mut with, mut without, mut drain) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..5 {
        let (_, st) = baked(HEARTH, 1337, 0);
        with.push(st.bake_ms);
        drain.push(st.drainage_ms);
        let (_, st) = baked(&dry, 1337, 0);
        without.push(st.bake_ms);
    }
    let best = |v: &[f64]| v.iter().copied().fold(f64::MAX, f64::min);
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    println!(
        "Hearth bake without drainage: best {:.0} ms, median {:.0} ms; with: best {:.0} ms, median {:.0} ms (drainage best {:.0} ms)",
        best(&without),
        median(&mut without.clone()),
        best(&with),
        median(&mut with.clone()),
        best(&drain)
    );
}
