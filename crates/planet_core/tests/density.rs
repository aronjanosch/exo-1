//! #177 (spike 14): landforms and sites by density rules. The count grows with the surface at a
//! fixed rule, no two of a kind stand closer than their separation, and the bake stays small.
use planet_core::*;
use std::time::Instant;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn baked(radius: f64) -> (Planet, f64) {
    let text = HEARTH.replace("\"resolution\": 512", "\"resolution\": 128");
    let t = Instant::now();
    let mut p = Planet::new(Recipe::for_planet(&text, 1337, radius).unwrap());
    p.bake_with(0, None, false);
    (p, t.elapsed().as_secs_f64())
}

#[test]
fn counts_grow_with_the_surface_at_a_fixed_rule() {
    let rows: Vec<(f64, usize, usize, f64)> = [5000.0, 10_000.0, 20_000.0, 30_000.0]
        .iter()
        .map(|&r| {
            let (p, secs) = baked(r);
            let sites = p.sites.iter().filter(|s| s.kind.is_some()).count();
            println!("radius {r}: {sites} sites, {} landforms, bake {secs:.1} s", p.stamps().len());
            (r, sites, p.stamps().len(), secs)
        })
        .collect();
    let (base_sites, base_stamps) = (rows[0].1 as f64, rows[0].2 as f64);
    for &(r, sites, stamps, _) in &rows[1..] {
        let area = (r / 5000.0) * (r / 5000.0);
        let (rs, rl) = (sites as f64 / base_sites / area, stamps as f64 / base_stamps / area);
        assert!((0.75..=1.25).contains(&rs), "sites at {r}: {sites} is {rs:.2} x the area's share");
        assert!((0.75..=1.25).contains(&rl), "landforms at {r}: {stamps} is {rl:.2} x the area's share");
    }
}

#[test]
fn no_two_of_a_kind_closer_than_their_separation_at_every_radius() {
    for radius in [5000.0, 10_000.0, 20_000.0] {
        let (p, _) = baked(radius);
        let dist = |a: V3, b: V3| radius * a.dot(b).clamp(-1.0, 1.0).acos();
        for (ki, kind) in p.recipe.sites.kinds.iter().enumerate() {
            let of: Vec<&Site> = p.sites.iter().filter(|s| s.kind == Some(ki)).collect();
            for (i, a) in of.iter().enumerate() {
                for b in &of[i + 1..] {
                    assert!(dist(a.dir, b.dir) >= kind.min_separation_m - 0.01, "{radius}: two {} sites {:.0} m apart (rule {})", kind.id, dist(a.dir, b.dir), kind.min_separation_m);
                }
            }
        }
        let stamps = p.stamps();
        for kind in &p.recipe.landforms.kinds {
            let of: Vec<&PlacedStamp> = stamps.iter().filter(|s| s.kind == kind.id).collect();
            for (i, a) in of.iter().enumerate() {
                for b in &of[i + 1..] {
                    assert!(dist(a.centre, b.centre) >= kind.min_separation_m - 0.01, "{radius}: two {} landforms {:.0} m apart (rule {})", kind.id, dist(a.centre, b.centre), kind.min_separation_m);
                }
            }
        }
    }
}

#[test]
fn the_bake_stays_small_at_30_km() {
    let (p, secs) = baked(30_000.0);
    println!("30 km: {} sites, {} landforms, bake {secs:.1} s (debug build, shared machine)", p.sites.len(), p.stamps().len());
    assert!(secs < 120.0, "{secs} s");
}
