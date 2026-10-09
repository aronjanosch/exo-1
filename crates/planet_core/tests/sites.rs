//! #70: site kinds placed by the global pass, ground edits, kit pieces.
use planet_core::recipe::{Edit, SiteCategory};
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn hearth() -> Planet {
    let mut p = Planet::new(Recipe::for_planet(HEARTH, 1337, 5000.0).unwrap());
    p.bake_checked(0).unwrap();
    p
}

fn dist(p: &Planet, a: V3, b: V3) -> f64 {
    p.radius * a.dot(b).clamp(-1.0, 1.0).acos()
}

#[test]
fn placement_is_deterministic_and_respects_separation_filters_and_budgets() {
    let (p, q) = (hearth(), hearth());
    assert_eq!(p.sites.len(), q.sites.len());
    for (a, b) in p.sites.iter().zip(&q.sites) {
        assert_eq!((&a.id, a.dir, a.yaw), (&b.id, b.dir, b.yaw));
    }
    let kinds = &p.recipe.sites.kinds;
    for (i, a) in p.sites.iter().enumerate() {
        let k = &kinds[a.kind.unwrap()];
        for b in &p.sites[i + 1..] {
            let kb = &kinds[b.kind.unwrap()];
            let want = if a.kind == b.kind { k.min_separation_m.max(k.min_separation_all_m) } else { k.min_separation_all_m.max(kb.min_separation_all_m) };
            assert!(dist(&p, a.dir, b.dir) >= want - 1e-6, "{} and {} too close", a.id, b.id);
        }
        // Filters on the noise ground at the centre (the edits come after placement).
        let base = p.base_height_at(a.dir) - p.sea;
        assert!(base >= k.height_above_sea_m[0] && base <= k.height_above_sea_m[1], "{} at {base:.1} m", a.id);
        if let Some(b) = &k.biomes {
            let face = face_of(a.dir);
            let (fa, fb) = sphere_to_face_ab(face, a.dir);
            let (h, f) = p.base_height_ab(face, fa, fb, a.dir);
            let row = p.biome_for(h - p.sea, &f, p.stamp_height(a.dir).1);
            assert!(b.contains(&row), "{} in biome row {row}", a.id);
        }
        for bud in &p.recipe.sites.budgets {
            if bud.category == a.category {
                let n = p.sites.iter().filter(|s| s.category == bud.category && dist(&p, s.dir, a.dir) < bud.radius_m).count();
                assert!(n as u32 <= bud.max, "budget {:?}: {n} within {} m", bud.category, bud.radius_m);
            }
        }
    }
    assert!(p.sites.iter().filter(|s| s.category == SiteCategory::Landmark).count() >= 1);
}

#[test]
fn edits_change_the_height_only_inside_their_reach() {
    let p = hearth();
    for s in &p.sites {
        let (e, n) = planet_core::look::tangent_frame(s.dir);
        for k in 0..16 {
            let a = k as f64 / 16.0 * std::f64::consts::TAU;
            let t = e * a.cos() + n * a.sin();
            for extra in [1.0, 10.0, 40.0] {
                let d = planet_core::look::walk(s.dir, t, s.reach_m + extra, p.radius);
                // Neighbouring sites' edits may reach here.
                if p.sites.iter().any(|o| o.dir != s.dir && dist(&p, o.dir, d) < o.reach_m + 1.0) {
                    continue;
                }
                let diff = (p.height_at(d) - p.base_height_at(d)).abs();
                assert!(diff < 1e-3, "{}: {diff:.4} m changed {extra} m outside the reach", s.id);
            }
        }
        // A flatten levels the middle to the ground height at the centre (less the dish).
        let k = &p.recipe.sites.kinds[s.kind.unwrap()];
        if let [Edit::Smooth { .. }, Edit::Flatten { dish_m, .. }] | [Edit::Flatten { dish_m, .. }] = k.edits.as_slice() {
            let mid = planet_core::look::walk(s.dir, e, 1.0, p.radius);
            assert!((p.height_at(mid) - (s.ground_m - dish_m)).abs() < 0.05, "{} not flat", s.id);
        }
    }
}

#[test]
fn kit_pieces_neither_float_nor_get_buried() {
    let p = hearth();
    let mut n = 0;
    for i in 0..p.sites.len() {
        for piece in p.site_pieces(i) {
            // The base point sits on the edited ground, sunk by its own sink depth.
            let ground = p.height_at(piece.pos.normalized());
            let base = piece.pos.length() - p.radius;
            assert!((base - (ground - piece.sink_m)).abs() < 1e-6, "{} off the ground", piece.prop);
            assert!(piece.sink_m < piece.scale * 3.0 + 0.5, "{} buried", piece.prop);
            n += 1;
        }
    }
    assert!(n > 100, "only {n} pieces");
}
