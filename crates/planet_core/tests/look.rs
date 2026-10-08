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
    for vp in v.viewpoints.iter().filter(|vp| vp.spot != "orbit") {
        let s = p.spot(&vp.spot).unwrap_or_else(|| panic!("no spot {}", vp.spot));
        assert!((s.dir.length() - 1.0).abs() < 1e-9 && s.facing.dot(s.dir).abs() < 1e-6, "{}", vp.spot);
    }
    assert!(Viewpoints::from_json(r#"{"sun_elevation_deg":1,"sun_azimuth_deg":1,"atlas_width":8,"viewpoints":[{"id":"x","spot":"nowhere","height_m":1}]}"#).is_err());
}
