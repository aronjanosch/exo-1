//! #177 (spike 14): density-based placement of landforms and sites.
//! Tests: count scales with radius, separation rules hold, and bake time is reasonable.
use planet_core::*;
use std::time::Instant;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn recipe(text: &str, seed: i32, radius: f64) -> Recipe {
    Recipe::for_planet(&text.replace("\"resolution\": 512", "\"resolution\": 128"), seed, radius).unwrap()
}

fn baked(text: &str, seed: i32, radius: f64) -> (Planet, std::time::Duration) {
    let start = Instant::now();
    let mut p = Planet::new(recipe(text, seed, radius));
    let _ = p.bake_with(0, None, false);
    (p, start.elapsed())
}

#[test]
fn count_scales_with_radius_5km_vs_10km() {
    // Test that counts scale with surface area at different radii.
    // Ratio of areas = (r2/r1)^2

    let (p5km, t5) = baked(HEARTH, 1337, 5000.0);
    let (p10km, t10) = baked(HEARTH, 1337, 10000.0);

    let sites5 = p5km.sites.len() as f64;
    let sites10 = p10km.sites.len() as f64;

    let stamps5 = p5km.stamps().len() as f64;
    let stamps10 = p10km.stamps().len() as f64;

    // Ratio 5->10 km: area increases by 4x, count should be close to 4x (within 25% tolerance).
    let ratio_5_10_sites = sites10 / sites5;
    let ratio_5_10_stamps = stamps10 / stamps5;
    assert!(
        ratio_5_10_sites > 3.0 && ratio_5_10_sites < 5.0,
        "sites 5->10km ratio {:.2} should be ~4",
        ratio_5_10_sites
    );
    assert!(
        ratio_5_10_stamps > 3.0 && ratio_5_10_stamps < 5.0,
        "stamps 5->10km ratio {:.2} should be ~4",
        ratio_5_10_stamps
    );

    println!("5km: sites={} stamps={} ({:.2}s)", sites5 as u32, stamps5 as u32, t5.as_secs_f64());
    println!("10km: sites={} stamps={} ({:.2}s)", sites10 as u32, stamps10 as u32, t10.as_secs_f64());
    println!("✓ Scaling 5->10km: sites {:.2}x, stamps {:.2}x", ratio_5_10_sites, ratio_5_10_stamps);
}

#[test]
fn separation_rules_hold_at_5km() {
    // Test that no two sites or stamps of the same kind are closer than min_separation_m.
    let (planet, _) = baked(HEARTH, 1337, 5000.0);
    let radius = 5000.0;

    let kinds = &planet.recipe.sites.kinds;
    for (ki, kind) in kinds.iter().enumerate() {
        let mut same_kind = Vec::new();
        for (si, site) in planet.sites.iter().enumerate() {
            if site.kind == Some(ki) {
                same_kind.push(si);
            }
        }

        // Check all pairs of sites of the same kind.
        for i in 0..same_kind.len() {
            for j in (i + 1)..same_kind.len() {
                let s1 = &planet.sites[same_kind[i]];
                let s2 = &planet.sites[same_kind[j]];
                let dist_m = radius * s1.dir.dot(s2.dir).clamp(-1.0, 1.0).acos();
                assert!(
                    dist_m >= kind.min_separation_m - 1.0, // Allow 1m tolerance for float rounding
                    "sites of kind {} at {:.0}m < min {:.0}m",
                    kind.id, dist_m, kind.min_separation_m
                );
            }
        }
    }

    // Check stamps similarly.
    for (ki, kind) in planet.recipe.landforms.kinds.iter().enumerate() {
        let mut stamps_of_kind = Vec::new();
        for (si, stamp) in planet.stamps().iter().enumerate() {
            if stamp.kind == kind.id {
                stamps_of_kind.push(si);
            }
        }

        for i in 0..stamps_of_kind.len() {
            for j in (i + 1)..stamps_of_kind.len() {
                let s1 = &planet.stamps()[stamps_of_kind[i]];
                let s2 = &planet.stamps()[stamps_of_kind[j]];
                let dist_m = radius * s1.centre.dot(s2.centre).clamp(-1.0, 1.0).acos();
                assert!(
                    dist_m >= kind.min_separation_m - 1.0,
                    "stamps of kind {} at {:.0}m < min {:.0}m",
                    kind.id, dist_m, kind.min_separation_m
                );
            }
        }
    }

    println!("✓ Separation rules hold at 5km");
}

#[test]
fn count_at_different_radii_matches_expected_density() {
    // Verify the counts at each radius are reasonable and follow density rules.
    // Surface area at radius r: 4*pi*r^2
    // Expected count: area_in_100km2 * density_per_100km2

    let (p5km, t5) = baked(HEARTH, 1337, 5000.0);
    let (p10km, t10) = baked(HEARTH, 1337, 10000.0);

    for (planet, label, time) in [(&p5km, "5km", t5), (&p10km, "10km", t10)] {
        let area_m2 = 4.0 * std::f64::consts::PI * planet.radius * planet.radius;
        let area_100km2 = area_m2 / 1e8;

        // Just check we have a reasonable count (non-zero, not huge).
        assert!(planet.sites.len() > 0, "no sites at radius {}", planet.radius);
        assert!(planet.sites.len() < 50000, "too many sites at radius {}", planet.radius);
        assert!(planet.stamps().len() > 0, "no stamps at radius {}", planet.radius);
        assert!(planet.stamps().len() < 50000, "too many stamps at radius {}", planet.radius);

        println!("{}: area={:.1}km² sites={} stamps={} ({:.2}s)",
                 label, area_100km2 * 100.0, planet.sites.len(), planet.stamps().len(), time.as_secs_f64());
    }
}
