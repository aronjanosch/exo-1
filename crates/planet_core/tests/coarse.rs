//! #177 (spike 14): the coarse global layer. Same hash with one and many threads and across runs,
//! cached under a hash of recipe, seed, radius and code version.
use planet_core::*;
use std::path::PathBuf;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn recipe(text: &str, seed: i32, radius: f64) -> Recipe {
    Recipe::for_planet(&text.replace("\"resolution\": 512", "\"resolution\": 128"), seed, radius).unwrap()
}

fn baked(text: &str, seed: i32, radius: f64, threads: usize, cache: Option<&std::path::Path>) -> (Planet, BakeStats) {
    let mut p = Planet::new(recipe(text, seed, radius));
    let st = p.bake_with(threads, cache, false);
    (p, st)
}

fn dir(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("coarse-cache").join(name);
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn layer_and_chunk_hash_equal_on_one_and_many_threads_and_across_runs() {
    let (a, _) = baked(HEARTH, 1337, 5000.0, 1, None);
    let (b, _) = baked(HEARTH, 1337, 5000.0, 0, None);
    let (c, _) = baked(HEARTH, 1337, 5000.0, 3, None);
    assert!(!a.coarse.rivers.is_empty() && a.coarse.carve.iter().any(|&v| v != 0));
    assert_eq!(a.coarse.hash(), b.coarse.hash(), "1 thread and all cores");
    assert_eq!(a.coarse.hash(), c.coarse.hash(), "3 threads");
    assert_eq!(a.sea, b.sea);
    let other_seed = baked(HEARTH, 1338, 5000.0, 0, None).0;
    assert_ne!(a.coarse.hash(), other_seed.coarse.hash());

    // A chunk: the same bytes built alone, and built on four threads at once, on either planet.
    let one = a.build_chunk(2, -0.3, 0.1, 0.0625).hash();
    assert_eq!(one, b.build_chunk(2, -0.3, 0.1, 0.0625).hash());
    let many: Vec<u64> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..4).map(|_| s.spawn(|| a.build_chunk(2, -0.3, 0.1, 0.0625).hash())).collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert!(many.iter().all(|&h| h == one), "{many:x?} vs {one:x}");
}

#[test]
fn cache_loads_when_unchanged_and_rebakes_when_recipe_seed_radius_or_code_change() {
    let d = dir("keys");
    let (first, st) = baked(HEARTH, 1337, 5000.0, 0, Some(&d));
    assert!(!st.coarse_from_cache, "the first bake writes");
    assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1, "one file");
    let (second, st) = baked(HEARTH, 1337, 5000.0, 0, Some(&d));
    assert!(st.coarse_from_cache, "an unchanged recipe loads");
    assert_eq!(first.coarse, second.coarse, "the loaded layer is the baked one");
    assert_eq!(first.coarse.hash(), second.coarse.hash());
    assert_eq!(first.sea, second.sea);

    // The loaded planet is the baked planet: same ground, same chunks.
    assert_eq!(first.build_chunk(0, 0.2, -0.2, 0.125).hash(), second.build_chunk(0, 0.2, -0.2, 0.125).hash());

    let changed = HEARTH.replace("\"river_min_catchment_km2\": 2.0", "\"river_min_catchment_km2\": 3.0");
    assert_ne!(changed, HEARTH, "the test changes a drainage value");
    assert!(!baked(&changed, 1337, 5000.0, 0, Some(&d)).1.coarse_from_cache, "a changed recipe rebakes");
    assert!(!baked(HEARTH, 1338, 5000.0, 0, Some(&d)).1.coarse_from_cache, "another seed rebakes");
    assert!(!baked(HEARTH, 1337, 6000.0, 0, Some(&d)).1.coarse_from_cache, "another radius rebakes");
    assert_eq!(std::fs::read_dir(&d).unwrap().count(), 4, "each under its own key");
    assert!(baked(HEARTH, 1337, 5000.0, 0, Some(&d)).1.coarse_from_cache, "the first is still there");

    // Another code version: the key changes with it, and a file of an older version does not load.
    let key = coarse::cache_key(recipe(HEARTH, 1337, 5000.0).source_hash, 1337, 5000.0, 128);
    let mut bytes = first.coarse.encode(key);
    bytes[4] ^= 1;
    let sum = coarse::fnv(&bytes[..bytes.len() - 8], coarse::FNV_START);
    let len = bytes.len();
    bytes[len - 8..].copy_from_slice(&sum.to_le_bytes());
    assert!(coarse::Coarse::decode(&bytes, key).is_err_and(|e| e.contains("code version")));
}

#[test]
fn a_damaged_or_foreign_cache_file_is_ignored() {
    let d = dir("damaged");
    let (first, _) = baked(HEARTH, 1337, 5000.0, 0, Some(&d));
    let file = std::fs::read_dir(&d).unwrap().next().unwrap().unwrap().path();
    let mut bytes = std::fs::read(&file).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    std::fs::write(&file, &bytes).unwrap();
    let (again, st) = baked(HEARTH, 1337, 5000.0, 0, Some(&d));
    assert!(!st.coarse_from_cache, "a flipped byte fails the checksum");
    assert_eq!(first.coarse, again.coarse);
    std::fs::write(&file, &bytes[..mid]).unwrap();
    assert!(!baked(HEARTH, 1337, 5000.0, 0, Some(&d)).1.coarse_from_cache, "a truncated file");
    assert!(baked(HEARTH, 1337, 5000.0, 0, Some(&d)).1.coarse_from_cache, "and the rebake repaired it");
}
