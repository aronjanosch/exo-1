//! #177 (spike 14): rivers as polylines with width. On a planet whose coarse grid is far wider than
//! a river, the channel is still carved, continuous along the river and across chunk borders.
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn planet(radius: f64) -> Planet {
    let text = HEARTH.replace("\"resolution\": 512", "\"resolution\": 128");
    let mut p = Planet::new(Recipe::for_planet(&text, 1337, radius).unwrap());
    p.bake_with(0, None, false);
    p
}

/// Direction `m` metres from `d` along the tangent `t`.
fn at(d: V3, t: V3, m: f64, radius: f64) -> V3 {
    planet_core::look::walk(d, t, m, radius)
}

#[test]
fn a_river_is_carved_between_the_coarse_vertices_and_is_wet() {
    let p = planet(30_000.0);
    let big: Vec<_> = p.coarse.rivers.iter().enumerate().filter(|(_, r)| r.catchment_km2 > 3.0 && matches!(r.next, drainage::Mouth::River(_))).take(30).collect();
    assert!(big.len() >= 5, "a wet planet has rivers: {}", big.len());
    let (mut carved, mut wet) = (0, 0);
    for &(i, r) in &big {
        // The middle of the segment's curve (#235), and a point across the river, 6 half widths out.
        let on = |t: f64| p.river_idx.curve_point(&p.coarse.rivers, p.radius, i, t).normalized();
        let mid = on(0.5);
        let along = (on(0.55) - on(0.45)).normalized();
        let across = mid.cross(along).normalized();
        let side = at(mid, across, 6.0 * r.half_width_m, p.radius);
        let (h_mid, h_side) = (p.height_at(mid), p.height_at(side));
        if h_side - h_mid > 0.5 * r.depth_m {
            carved += 1;
        }
        let face = face_of(mid);
        let (a, b) = sphere_to_face_ab(face, mid);
        if p.water_level_ab(face, a, b).is_some_and(|l| l > h_mid) {
            wet += 1;
        }
    }
    assert!(carved * 10 >= big.len() * 8, "the channel lies below its banks: {carved} of {}", big.len());
    assert!(wet * 10 >= big.len() * 8, "and holds water: {wet} of {}", big.len());
}

#[test]
fn the_channel_has_no_seam_across_chunk_borders() {
    let p = planet(30_000.0);
    let r = p.coarse.rivers.iter().filter(|r| r.catchment_km2 > 3.0).max_by(|a, b| a.catchment_km2.total_cmp(&b.catchment_km2)).unwrap();
    let face = face_of(r.dir);
    let (a0, b0) = sphere_to_face_ab(face, r.dir);
    // Two chunks side by side at the river, one level deeper than the root: their shared edge is
    // a = a_edge. Heights at the shared vertices agree to float precision.
    let size = 0.0625;
    let ca = (a0 / size).floor() * size;
    let cb = (b0 / size).floor() * size;
    let left = p.build_chunk(face, ca - size, cb, size);
    let right = p.build_chunk(face, ca, cb, size);
    let m = M;
    // vertex (i, j) of a chunk is at face coordinates a0 + (i - 1) * step; left's i = GRID + 1 is
    // right's i = 1.
    let mut worst = 0.0f32;
    for j in 1..=GRID + 1 {
        let l = left.heights[j * m + GRID + 1];
        let rr = right.heights[j * m + 1];
        worst = worst.max((l - rr).abs());
    }
    assert!(worst < 1e-3, "the shared edge differs by {worst} m");
    // And no step along a line through the channel: neighbouring samples 0.25 m apart never differ
    // by more than the steepest bank the profile allows.
    let t = {
        let (e, n) = look::tangent_frame(r.dir);
        let _ = n;
        e
    };
    let mut prev = p.height_at(at(r.dir, t, -60.0, p.radius));
    let mut jump = 0.0f64;
    for k in 1..=480 {
        let h = p.height_at(at(r.dir, t, -60.0 + k as f64 * 0.25, p.radius));
        jump = jump.max((h - prev).abs());
        prev = h;
    }
    assert!(jump < 1.5, "a step of {jump} m in 0.25 m across the channel");
}

#[test]
fn a_river_bends_instead_of_turning_at_right_angles() {
    // #235: flow on the grid goes one of 8 ways; the drawn curve is smoothed, so along a long
    // river the turn between neighbouring pieces stays well below a right angle.
    let p = planet(30_000.0);
    let rivers = &p.coarse.rivers;
    let mut worst = 0.0f64;
    let mut pieces = 0;
    for (i, r) in rivers.iter().enumerate().filter(|(_, r)| r.catchment_km2 > 3.0) {
        let drainage::Mouth::River(j) = r.next else { continue };
        // Along the main stem only: a tributary meets the main river at an angle at a confluence.
        if rivers.iter().any(|o| o.next == drainage::Mouth::River(j) && o.catchment_km2 > r.catchment_km2) {
            continue;
        }
        // This segment and the next, so the turn at the vertex between them counts too.
        let on = |s: usize, t: f64| p.river_idx.curve_point(rivers, p.radius, s, t);
        let pts: Vec<V3> = (0..=8).map(|k| on(i, k as f64 / 8.0)).chain((1..=8).map(|k| on(j as usize, k as f64 / 8.0))).collect();
        for w in pts.windows(3) {
            let (a, b) = ((w[1] - w[0]).normalized(), (w[2] - w[1]).normalized());
            worst = worst.max(a.dot(b).clamp(-1.0, 1.0).acos().to_degrees());
            pieces += 1;
        }
    }
    assert!(pieces > 50, "{pieces} pieces");
    assert!(worst < 60.0, "the sharpest turn along a river is {worst:.0} degrees");
}
