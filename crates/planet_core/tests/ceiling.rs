//! #177 (spike 14): ceilings first. With a ceiling the tallest terrain stays below the share of the
//! atmosphere height it is given, at every radius; without one the planet is as before.
use planet_core::ceilings::TERRAIN_SHARE;
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn baked(radius: f64, ceiling: Option<f64>) -> Planet {
    // Terrain only: no site placement (that is what takes the time at 300 km).
    let text = HEARTH.replace("\"resolution\": 512", "\"resolution\": 128").replace("\"candidates\": 30000", "\"candidates\": 0");
    let mut p = Planet::new(Recipe::for_planet(&text, 1337, radius).unwrap());
    if let Some(c) = ceiling {
        p.ceiling_m = c;
    }
    p.bake_with(0, None, false);
    p
}

#[test]
fn the_tallest_terrain_stays_below_its_share_of_the_atmosphere() {
    for radius in [5000.0, 30_000.0, 100_000.0, 300_000.0] {
        let c = Ceilings::for_radius(radius);
        let p = baked(radius, Some(c.terrain_max));
        let tallest = p.height_range.1;
        println!("radius {radius}: tallest {tallest:.1} m, ceiling {:.1} m (atmosphere {:.0} m)", c.terrain_max, c.atmosphere_height);
        assert!(tallest < c.terrain_max, "{radius}: {tallest} m above the radius, ceiling {} m", c.terrain_max);
        assert!(tallest <= c.atmosphere_height * TERRAIN_SHARE);
        assert!(c.obstruction_radius > radius + tallest, "a path keeps out of the terrain");
        // A walked line never exceeds it either.
        let mut d = v3(0.1, 0.9, 0.4).normalized();
        let t = look::tangent_frame(d).0;
        for k in 0..2000 {
            d = look::walk(d, t, 40.0, radius).normalized();
            assert!(p.height_at(d) < c.terrain_max, "{k}");
        }
    }
}

#[test]
fn without_a_ceiling_the_height_is_untouched_and_the_bend_is_monotone() {
    let free = baked(5000.0, None);
    let capped = baked(5000.0, Some(150.0));
    assert!(free.height_range.1 > 150.0, "Hearth's relief is above 150 m: {}", free.height_range.1);
    let d = v3(0.3, 0.8, 0.5).normalized();
    assert!((free.soft_ceiling(40.0) - 40.0).abs() < 1e-12);
    let mut last = f64::MIN;
    for h in (0..600).map(|i| i as f64) {
        let c = capped.soft_ceiling(h);
        assert!(c >= last && c < 150.0, "{h} -> {c}");
        last = c;
    }
    let _ = d;
}
