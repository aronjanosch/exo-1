//! #63: atlas and look spots.
use planet_core::*;

fn hearth() -> Planet {
    let r = Recipe::for_planet(include_str!("../../../content/planet/hearth.json"), 1337, 5000.0).unwrap();
    let mut p = Planet::new(r);
    p.bake(0);
    p
}

#[test]
fn atlas_has_every_layer_at_its_size() {
    let p = hearth();
    let a = p.atlas(128, 0);
    assert_eq!((a.width, a.height), (128, 64));
    assert_eq!(a.layers.len(), AtlasLayer::ALL.len());
    for (_, rgb) in &a.layers {
        assert_eq!(rgb.len(), 128 * 64 * 3);
    }
}

#[test]
fn viewpoints_parse_and_every_spot_exists_on_hearth() {
    let v = Viewpoints::from_json(include_str!("../../../content/look/viewpoints.json")).unwrap();
    let p = hearth();
    // Landform kinds are per planet (Hearth has no caldera); every other spot must exist.
    let optional = ["crater", "canyon", "mesa", "spire", "caldera"];
    for vp in v.viewpoints.iter().filter(|vp| vp.spot != "orbit") {
        let Some(s) = p.spot(&vp.spot) else {
            assert!(optional.contains(&vp.spot.as_str()), "no spot {}", vp.spot);
            continue;
        };
        assert!((s.dir.length() - 1.0).abs() < 1e-9 && s.facing.dot(s.dir).abs() < 1e-6, "{}", vp.spot);
    }
    assert!(Viewpoints::from_json(r#"{"sun_elevation_deg":1,"sun_azimuth_deg":1,"atlas_width":8,"viewpoints":[{"id":"x","spot":"nowhere","height_m":1}]}"#).is_err());
}

// ---------- local map (#166): a north-up picture of the ground around a point ----------

#[test]
fn a_local_map_has_its_size_and_north_is_up() {
    let p = hearth();
    let c = v3(0.0, 1.0, 0.0);
    let px = 64;
    let img = p.local_map(c, 800.0, px);
    assert_eq!(img.len(), px * px * 3);
    assert_eq!(img, p.local_map(c, 800.0, px), "the same map twice");
    // The pixel centre is the centre; a pixel higher up is further north, one to the right east.
    let at = |x: usize, y: usize| planet_core::look::local_map_dir(c, p.radius, 800.0, px, x, y);
    let (e, n) = planet_core::look::tangent_frame(c);
    let mid = px / 2;
    assert!((at(mid, mid) - c).length() < 800.0 / p.radius / px as f64 * 1.5, "the middle is the centre");
    assert!((at(mid, 4) - c).dot(n) > 0.0 && (at(mid, 4) - c).dot(e).abs() < 4e-3, "up is north");
    assert!((at(px - 4, mid) - c).dot(e) > 0.0 && (at(px - 4, mid) - c).dot(n).abs() < 4e-3, "right is east");
    // The metres are metres: the map's edge is its radius from the centre.
    let edge = at(px - 1, mid);
    let d = p.radius * c.dot(edge).clamp(-1.0, 1.0).acos();
    assert!((d - 800.0 * (1.0 - 1.0 / px as f64)).abs() < 800.0 / px as f64 * 1.5, "{d}");
}

#[test]
fn local_offsets_are_the_inverse_of_the_map_directions() {
    let p = hearth();
    let c = v3(0.0, 1.0, 0.0);
    let px = 64;
    for (x, y) in [(10, 20), (50, 12), (32, 32), (60, 58)] {
        let d = planet_core::look::local_map_dir(c, p.radius, 800.0, px, x, y);
        let (east, north) = planet_core::look::local_offset(c, p.radius, d);
        // Pixel centre in metres from the map's centre.
        let (ex, ny) = ((x as f64 + 0.5 - px as f64 / 2.0) * 800.0 / (px as f64 / 2.0), (px as f64 / 2.0 - y as f64 - 0.5) * 800.0 / (px as f64 / 2.0));
        assert!((east - ex).abs() < 0.5 && (north - ny).abs() < 0.5, "({x},{y}): {east:.1}, {north:.1} vs {ex:.1}, {ny:.1}");
    }
    let (e0, n0) = planet_core::look::local_offset(c, p.radius, c);
    assert!(e0.abs() < 1e-6 && n0.abs() < 1e-6);
}

#[test]
fn water_is_blue_in_a_local_map_and_land_is_not() {
    let p = hearth();
    // A map over a wide stretch contains both on Hearth near some coast; find one by sampling the atlas.
    let a = p.atlas(128, 0);
    let _ = a;
    let wet = (0..200).map(|i| planet_core::look::pixel_dir((i * 7) % 128, 5 + (i * 3) % 50, 128, 64)).find(|d| p.sample(*d).water_depth > 1.0).expect("water somewhere");
    let dry = (0..200).map(|i| planet_core::look::pixel_dir((i * 11) % 128, 5 + (i * 5) % 50, 128, 64)).find(|d| p.sample(*d).water_depth == 0.0 && p.sample(*d).height_above_sea > 20.0).expect("dry land somewhere");
    let px = |d| {
        let img = p.local_map(d, 50.0, 8);
        let k = (4 * 8 + 4) * 3;
        [img[k], img[k + 1], img[k + 2]]
    };
    let w = px(wet);
    let l = px(dry);
    assert!(w[2] > w[0] && w[2] > w[1], "water is blue-ish: {w:?}");
    assert!(!(l[2] > l[0] && l[2] > l[1] + 20), "land is not blue: {l:?}");
}
