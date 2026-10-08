//! #69: landforms placed by budget: deterministic, separated, never over a site, retries with a
//! clear error, every kind somewhere.
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");
const CINDER: &str = include_str!("../../../content/planet/cinder.json");

fn baked(text: &str, seed: i32) -> Planet {
    let mut p = Planet::new(Recipe::for_planet(text, seed, 5000.0).unwrap());
    p.bake_checked(0).unwrap();
    p
}

#[test]
fn placement_is_deterministic_per_seed() {
    let a = Planet::new(Recipe::for_planet(HEARTH, 1337, 5000.0).unwrap()).stamps();
    let b = Planet::new(Recipe::for_planet(HEARTH, 1337, 5000.0).unwrap()).stamps();
    let c = Planet::new(Recipe::for_planet(HEARTH, 1338, 5000.0).unwrap()).stamps();
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!((&x.kind, x.centre, x.reach_m), (&y.kind, y.centre, y.reach_m));
    }
    assert!(a.iter().zip(&c).any(|(x, y)| x.centre != y.centre), "another seed, other places");
}

#[test]
fn separation_and_sites_are_respected() {
    for (text, seed) in [(HEARTH, 1337), (CINDER, 4242)] {
        let p = baked(text, seed);
        let r = Recipe::for_planet(text, seed, 5000.0).unwrap();
        let sep = |k: &str| r.landforms.kinds.iter().find(|x| x.id == k).unwrap().min_separation_m;
        let st = p.stamps();
        for (i, a) in st.iter().enumerate() {
            for b in &st[i + 1..] {
                let d = p.radius * a.centre.dot(b.centre).clamp(-1.0, 1.0).acos();
                assert!(d >= sep(&a.kind).max(sep(&b.kind)) - 1e-6, "{} and {} {d:.0} m apart", a.kind, b.kind);
            }
            for s in &p.sites {
                let d = p.radius * a.centre.dot(*s).clamp(-1.0, 1.0).acos();
                assert!(d >= a.reach_m, "site inside {} ({d:.0} m < {:.0} m)", a.kind, a.reach_m);
            }
        }
    }
}

#[test]
fn every_new_kind_is_on_a_planet_and_the_signatures_beat_the_relief() {
    let (h, c) = (baked(HEARTH, 1337), baked(CINDER, 4242));
    let all: Vec<PlacedStamp> = h.stamps().into_iter().chain(c.stamps()).collect();
    for shape in ["crater", "canyon", "mesa_field", "spire", "caldera"] {
        assert!(all.iter().any(|s| s.shape == shape), "no {shape}");
    }
    for p in [&h, &c] {
        let sig = p.stamps().into_iter().find(|s| s.signature).expect("a signature");
        assert!(sig.relief_m >= 200.0, "{} relief {:.0} m", sig.kind, sig.relief_m);
    }
}

#[test]
fn a_missed_count_retries_then_fails_clearly() {
    let text = HEARTH.replacen("\"count\": [2, 4], \"min_separation_m\": 900.0, \"where\": { \"elevation\": [-0.3, 1.0] }", "\"count\": [2, 4], \"min_separation_m\": 900.0, \"where\": { \"elevation\": [5.0, 6.0] }", 1);
    assert_ne!(text, HEARTH, "fixture");
    let mut p = Planet::new(Recipe::for_planet(&text, 1337, 5000.0).unwrap());
    let e = p.bake_checked(0).unwrap_err();
    assert!(e.contains("landform crater: placed 0 of at least 2 after 12 tries"), "{e}");
}
