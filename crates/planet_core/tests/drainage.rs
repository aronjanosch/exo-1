use planet_core::drainage::Mouth;
use planet_core::look::{tangent_frame, walk};
use planet_core::*;
use std::sync::OnceLock;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");
/// The coarse layer stores heights in tenths of a metre: a stored level is within half of that.
const QUANT: f64 = 0.06;
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
fn water_flows_downhill() {
    let (p, _) = hearth();
    // The river's vertex on its drawn curve (smoothed and meandered off the grid vertex).
    let on = |i: usize| p.river_idx.curve_point(&p.coarse.rivers, p.radius, i, 0.0).normalized();
    let mut cut_deeper = 0;
    for (i, rv) in p.coarse.rivers.iter().enumerate() {
        let at = on(i);
        let ground = p.base_height_at(at);
        // At its bed, never above; lower where a nearby river's curve cuts deeper (the lowest cut
        // wins), which only a few vertices next to another river see.
        assert!(ground < rv.bed_m + 0.1, "the ground at a river vertex is its bed: {ground} vs {}", rv.bed_m);
        cut_deeper += (ground < rv.bed_m - rv.depth_m) as usize;
        assert!(rv.level_m > rv.bed_m);
        assert!(level_at(p, at).is_some_and(|l| l > ground), "the river is wet at {at:?}");
        if let Mouth::River(j) = rv.next {
            let n = &p.coarse.rivers[j as usize];
            assert!(n.bed_m <= rv.bed_m && n.level_m <= rv.level_m, "bed or water rises downstream");
            let down = p.base_height_at(on(j as usize));
            assert!(down <= ground.max(rv.bed_m) + 0.15, "the ground rises downstream: {} -> {} (beds {} -> {})", ground, down, rv.bed_m, n.bed_m);
            assert!(n.catchment_km2 >= rv.catchment_km2, "water gets lost downstream");
        }
    }
    let n = p.coarse.rivers.len();
    println!("{cut_deeper} of {n} river vertices lie in a deeper river's cut");
    assert!(cut_deeper * 20 < n, "{cut_deeper} of {n} river vertices lie in another river's deeper cut");
}

#[test]
fn lakes_sit_in_sinks() {
    let (p, _) = hearth();
    assert!(!p.coarse.lakes.is_empty());
    for l in &p.coarse.lakes {
        let deep = p.base_height_at(l.deepest);
        assert!(l.level_m - deep >= l.depth_m - 0.05 && l.depth_m > 0.0, "the deepest point lies {} m under the level", l.level_m - deep);
        assert!(level_at(p, l.deepest).is_some_and(|w| (w - l.level_m).abs() < QUANT), "the lake is flat at its level");
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
                    let river = p.coarse.rivers.iter().any(|r| (r.dir - d).length() * p.radius < 30.0);
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
    for start in 0..p.coarse.rivers.len() {
        let mut at = start;
        for _ in 0..=p.coarse.rivers.len() {
            match p.coarse.rivers[at].next {
                Mouth::River(j) => at = j as usize,
                Mouth::Sea => break,
                Mouth::Lake(_) => break,
            }
        }
        let end = &p.coarse.rivers[at];
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
                    let l = &p.coarse.lakes[id as usize];
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
    assert!(!a.coarse.rivers.is_empty());
    assert_eq!(a.coarse, b.coarse, "the macro image (cut and water) is the same on any thread count");
    assert_eq!(a.coarse.rivers.len(), b.coarse.rivers.len());
    for (x, y) in a.coarse.rivers.iter().zip(&b.coarse.rivers) {
        assert_eq!((x.dir, x.bed_m, x.level_m, x.next), (y.dir, y.bed_m, y.level_m, y.next));
    }
    assert_eq!(a.coarse.lakes.len(), b.coarse.lakes.len());
    for (x, y) in a.coarse.lakes.iter().zip(&b.coarse.lakes) {
        assert_eq!((x.deepest, x.level_m, x.area_m2), (y.deepest, y.level_m, y.area_m2));
    }
    assert!(a.coarse.rivers.len() != c.coarse.rivers.len() || a.coarse.rivers.iter().zip(&c.coarse.rivers).any(|(x, y)| x.dir != y.dir), "another seed, other rivers");
}

#[test]
fn nothing_grows_or_builds_in_the_water() {
    let (p, _) = hearth();
    for s in &p.sites {
        assert!(p.sample(s.dir).water_depth == 0.0, "site {} stands in water", s.id);
    }
    let mut checked = 0;
    for rv in p.coarse.rivers.iter().filter(|r| r.catchment_km2 > 1.0).step_by(20) {
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
        p.coarse.lakes.iter().filter(|l| l.outlet.is_none()).count(),
        st.drainage_ms
    );
    assert!(st.river_length_km > 0.0, "a dry planet still drains somewhere");
    assert!(p.coarse.lakes.iter().any(|l| l.outlet.is_none()), "a dry climate keeps lakes that never spill");
    for l in &p.coarse.lakes {
        let deep = p.base_height_at(l.deepest);
        assert!(l.level_m > deep && level_at(&p, l.deepest).is_some_and(|w| (w - l.level_m).abs() < QUANT), "a lake is wet at its level");
    }
    for start in 0..p.coarse.rivers.len() {
        let mut at = start;
        for _ in 0..=p.coarse.rivers.len() {
            match p.coarse.rivers[at].next {
                Mouth::River(j) => at = j as usize,
                _ => break,
            }
        }
        assert!(!matches!(p.coarse.rivers[at].next, Mouth::River(_)), "river {start} runs in a circle");
    }
}

