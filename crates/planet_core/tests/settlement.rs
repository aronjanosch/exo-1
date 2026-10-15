use planet_core::recipe::SiteCategory;
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn baked(radius: f64) -> Planet {
    // Resolution 128 as in the density test: the placement and the edits are what is tested.
    let text = HEARTH.replace("\"resolution\": 512", "\"resolution\": 128");
    let mut p = Planet::new(Recipe::for_planet(&text, 1337, radius).unwrap());
    p.bake_checked(0).unwrap();
    p
}

fn settlements(p: &Planet) -> Vec<usize> {
    (0..p.sites.len()).filter(|&i| p.sites[i].category == SiteCategory::Settlement).collect()
}

fn plain_rule(p: &Planet) -> (f64, f64) {
    let kind = p.recipe.sites.kinds.iter().find(|k| k.id == "settlement").expect("a settlement kind");
    let rule = kind.plain.as_ref().expect("settlements need a plain rule");
    (rule.radius_m, rule.max_rise_m)
}

#[test]
#[ignore = "100 km planet, slow: runs with --run-ignored all before main"]
fn a_100_km_planet_has_its_settlements_on_plains() {
    let p = baked(100_000.0);
    let ids = settlements(&p);
    let (span, max_rise) = plain_rule(&p);
    for &i in &ids {
        let s = &p.sites[i];
        let rise = p.plain_rise_m(s.dir, span);
        assert!(rise <= max_rise, "{} at {:.1} m rise over its span (rule {max_rise:.1} m)", s.id, rise);
    }
}

#[test]
#[ignore = "100 km planet, slow: runs with --run-ignored all before main"]
fn a_landing_pad_is_level_within_10_m() {
    let p = baked(100_000.0);
    for &i in &settlements(&p) {
        let s = &p.sites[i];
        let (e, n) = look::tangent_frame(s.dir);
        let mut lo = p.height_at(s.dir);
        let mut hi = lo;
        for r in [2.0, 5.0, 10.0] {
            for k in 0..8 {
                let a = k as f64 / 8.0 * std::f64::consts::TAU;
                let d = look::walk(s.dir, e * a.cos() + n * a.sin(), r, p.radius);
                let h = p.height_at(d);
                lo = lo.min(h);
                hi = hi.max(h);
            }
        }
        assert!(hi - lo < 0.3, "{}: pad varies {:.3} m within 10 m", s.id, hi - lo);
    }
}

#[test]
#[ignore = "100 km planet, slow: runs with --run-ignored all before main"]
fn a_settlement_kit_stands_on_the_level_ground() {
    let p = baked(100_000.0);
    for &i in &settlements(&p) {
        let pieces = p.site_pieces(i);
        assert!(pieces.iter().any(|c| c.prop == "landing_pad"), "{}: no landing pad", p.sites[i].id);
        assert!(pieces.iter().any(|c| c.prop == "city/beacon_spire"), "{}: no beacon", p.sites[i].id);
        for piece in &pieces {
            let ground = p.height_at(piece.pos.normalized());
            let base = piece.pos.length() - p.radius;
            assert!((base - (ground - piece.sink_m)).abs() < 1e-6, "{} off the ground", piece.prop);
            assert!(piece.sink_m < piece.scale * 3.0 + 0.5, "{} buried", piece.prop);
        }
    }
}
