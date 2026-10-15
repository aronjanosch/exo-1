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

