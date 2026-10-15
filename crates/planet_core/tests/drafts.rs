//! Draft archetypes (#282, `content/planet/draft/`): they parse, validate and bake. Not in the planet set.
use planet_core::*;
use std::path::PathBuf;

fn drafts() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../content/planet/draft");
    let mut v: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    v.sort();
    v
}

#[test]
fn every_draft_parses_and_is_marked_as_a_draft() {
    let all = drafts();
    assert!(all.len() >= 2, "drafts present");
    for (id, text) in &all {
        let r = Recipe::from_json(text).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert!(r.comment.starts_with("DRAFT (#282)"), "{id}: the comment marks it as a draft");
        assert!(r.variation.is_some(), "{id}: has variation ranges");
        let planet_set = include_str!("../../../content/campaign/planet_set.json");
        assert!(!planet_set.contains(&format!("\"{id}\"")), "{id} is not in the planet set");
    }
}

#[test]
fn every_draft_bakes_with_a_few_seeds() {
    for (id, text) in drafts() {
        for seed in [11, 22, 33] {
            let r = Recipe::for_planet(&text.replace("\"resolution\": 512", "\"resolution\": 128"), seed, 30_000.0).unwrap();
            assert!(r.tweaks.unwrap().within(r.variation.as_ref().unwrap()), "{id} {seed}");
            let mut p = Planet::new(r);
            assert!(p.placement_error().is_none(), "{id} {seed}: {:?}", p.placement_error());
            p.bake(0);
            assert!(p.sea.is_finite(), "{id} {seed}");
        }
    }
}

#[test]
#[ignore = "30 km bake at full resolution, slow: prints the time per draft"]
fn drafts_bake_at_30_km_full_resolution() {
    for (id, text) in drafts() {
        let mut p = Planet::new(Recipe::for_planet(&text, 11, 30_000.0).unwrap());
        let t = std::time::Instant::now();
        let st = p.bake(0);
        println!("{id}: bake {:.1} s (coarse {:.0} ms, sites {:.0} ms)", t.elapsed().as_secs_f64(), st.coarse_ms, st.sites_ms);
    }
}
