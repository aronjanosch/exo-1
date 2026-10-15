//! Seed-rolled variation inside an archetype (#281).
use planet_core::*;
use std::path::PathBuf;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");
const CINDER: &str = include_str!("../../../content/planet/cinder.json");

fn recipe(text: &str, seed: i32, radius: f64) -> Recipe {
    Recipe::for_planet(&text.replace("\"resolution\": 512", "\"resolution\": 128"), seed, radius).unwrap()
}

fn spec(text: &str) -> VariationSpec {
    Recipe::from_json(text).unwrap().variation.expect("the recipe has a variation block")
}

#[test]
fn same_seed_same_tweaks_and_every_value_stays_in_its_range() {
    for text in [HEARTH, CINDER] {
        let sp = spec(text);
        let authored = sp.authored_seed;
        let mut distinct = std::collections::BTreeSet::new();
        for seed in 1..400 {
            let a = recipe(text, seed, 30_000.0).tweaks.unwrap();
            let b = recipe(text, seed, 5_000.0).tweaks.unwrap();
            assert_eq!(a, b, "seed {seed}: the radius does not change the tweaks");
            assert_eq!(a, Tweaks::roll(&sp, seed));
            if Some(seed) != authored {
                assert!(a.within(&sp), "seed {seed}: {a:?} outside {sp:?}");
                distinct.insert(a.hash());
            }
        }
        assert!(distinct.len() > 390, "seeds give different tweaks ({} of 399)", distinct.len());
    }
}

#[test]
fn the_authored_seed_keeps_the_recipe_as_written() {
    let r = recipe(HEARTH, 1337, 5000.0);
    let t = r.tweaks.unwrap();
    assert_eq!((t.climate_base, t.moisture_bias, t.palette_shift_deg), (0.0, 0.0, 0.0));
    assert!((r.sea_level.land_fraction - 0.66).abs() < 1e-12);
    assert_eq!(r.macro_.temperature.base, 1.0);
}

#[test]
fn tweaks_reach_the_recipe_fields() {
    let r = recipe(HEARTH, 77, 30_000.0);
    let t = r.tweaks.unwrap();
    assert_eq!(r.macro_.temperature.base, 1.0 + t.climate_base);
    assert_eq!(r.macro_.moisture_bias, t.moisture_bias);
    assert!((r.sea_level.land_fraction - (1.0 - t.sea_share)).abs() < 1e-12);
    let plain = Recipe::for_planet(HEARTH, 1337, 30_000.0).unwrap();
    assert_ne!(plain.biomes[0].palette.ground, r.biomes[0].palette.ground, "the palette is shifted");
}

#[test]
fn a_bad_variation_block_is_refused() {
    let bad = HEARTH.replacen("\"sea_share\": [0.26, 0.44]", "\"sea_share\": [0.5, 0.3]", 1);
    assert_ne!(bad, HEARTH);
    assert!(Recipe::from_json(&bad).is_err_and(|e| e.contains("variation.sea_share")));
}

fn dir(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("tweaks-cache").join(name);
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn the_bake_is_deterministic_and_the_cache_key_follows_the_tweaks() {
    let d = dir("key");
    let bake = |r: Recipe| {
        let mut p = Planet::new(r);
        let st = p.bake_with(0, Some(&d), false);
        (p, st)
    };
    let (a, sa) = bake(recipe(HEARTH, 77, 5000.0));
    let (b, sb) = bake(recipe(HEARTH, 77, 5000.0));
    assert!(!sa.coarse_from_cache && sb.coarse_from_cache);
    assert_eq!(a.coarse.hash(), b.coarse.hash());
    assert_eq!(a.sea, b.sea);
    let mut again = Planet::new(recipe(HEARTH, 77, 5000.0));
    again.bake(0);
    assert_eq!(a.coarse.hash(), again.coarse.hash(), "a bake without the cache gives the same layers");

    // Same recipe text, seed and radius, one tweak changed: another key, no stale read.
    let mut r = recipe(HEARTH, 77, 5000.0);
    r.tweaks.as_mut().unwrap().moisture_bias += 0.01;
    r.macro_.moisture_bias += 0.01;
    assert!(!bake(r).1.coarse_from_cache, "a changed tweak rebakes");
    let t = recipe(HEARTH, 77, 5000.0).tweaks.unwrap();
    let key = |t: &Tweaks| coarse::cache_key(1, 77, 5000.0, 128, 0, t.hash());
    for f in [|t: &mut Tweaks| t.climate_base += 0.01, |t: &mut Tweaks| t.sea_share += 0.01, |t: &mut Tweaks| t.moisture_bias += 0.01, |t: &mut Tweaks| t.palette_shift_deg += 1.0] {
        let mut u = t;
        f(&mut u);
        assert_ne!(key(&t), key(&u));
    }
}

#[test]
fn places_bind_to_an_instance() {
    let place = |extra: &str| -> Place { serde_json::from_str(&format!("{{ \"id\": \"p\", \"planet\": \"hearth\", \"lat_deg\": 80.0, \"lon_deg\": 0.0 {extra} }}")).unwrap() };
    let authored = place("");
    let named = place(", \"seed\": 9");
    assert!(authored.binds_to("hearth", 1337, Some(1337)));
    assert!(!authored.binds_to("hearth", 9, Some(1337)), "a generated planet gets no authored place");
    assert!(named.binds_to("hearth", 9, Some(1337)) && !named.binds_to("hearth", 1337, Some(1337)));
    assert!(!authored.binds_to("cinder", 1337, Some(1337)));
    assert!(authored.binds_to("hearth", 5, None), "a recipe without an authored seed: as before");
}

#[test]
fn look_spots_face_somewhere_on_varied_planets() {
    // A varied planet can have a basin whose centre is dry land: the spot stands on the centre, its facing must not be zero.
    for text in [HEARTH, CINDER] {
        for seed in 1..=8 {
            let mut p = Planet::new(recipe(text, seed, 5000.0));
            p.bake(0);
            for kind in ["basin", "rim", "plateau", "crater", "canyon", "mesa", "spire", "caldera", "signature"] {
                if let Some(s) = p.spot(kind) {
                    let (d, f) = (s.dir, s.facing);
                    assert!(f.length() > 0.99 && f.length() < 1.01 && f.dot(d).abs() < 1e-6, "seed {seed} {kind}: facing {f:?}");
                }
            }
        }
    }
}
