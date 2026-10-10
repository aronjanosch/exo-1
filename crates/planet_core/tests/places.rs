//! #129: hand-placed places on the planet, their ground and their pads.
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn place(json: &str) -> Place {
    serde_json::from_str(json).unwrap()
}

const PAD: &str = r#"{ "id": "drip_rock", "planet": "hearth", "lat_deg": 86.0, "lon_deg": 90.0,
  "edits": [ { "type": "flatten", "order": 0, "radius_m": 18.0, "rolloff_m": 20.0 } ],
  "pads": [ { "id": "main", "at": [0.0, 0.0], "radius_m": 15.0 }, { "id": "side", "at": [10.0, 0.0], "radius_m": 4.0 } ] }"#;

fn hearth(places: Vec<Place>) -> Result<Planet, String> {
    let mut p = Planet::new(Recipe::for_planet(HEARTH, 1337, 5000.0).unwrap());
    p.set_places(places)?;
    p.bake_checked(0)?;
    Ok(p)
}

fn dist(p: &Planet, a: V3, b: V3) -> f64 {
    p.radius * a.dot(b).clamp(-1.0, 1.0).acos()
}

#[test]
fn a_place_flattens_its_pad_and_generated_sites_keep_away() {
    let pl = place(PAD);
    let p = hearth(vec![pl.clone()]).unwrap();
    let pad = p.pad("drip_rock", "main").unwrap();
    assert!(dist(&p, pad.centre.normalized(), pl.dir()) < 1e-6);
    // Flat within the flatten radius: heights along two diameters differ by millimetres.
    let (e, n) = planet_core::look::tangent_frame(pad.up);
    let mut hs = Vec::new();
    for t in [e, n, e * -1.0, n * -1.0] {
        for m in [0.0, 5.0, 10.0, 15.0] {
            hs.push(p.height_at(planet_core::look::walk(pad.up, t, m, p.radius)));
        }
    }
    let (lo, hi) = hs.iter().fold((f64::MAX, f64::MIN), |(a, b), h| (a.min(*h), b.max(*h)));
    assert!(hi - lo < 0.05, "pad not flat: {:.3} m", hi - lo);
    assert!(pad.centre.length() - (p.radius + p.height_at(pad.up)) < 1e-6, "centre on the ground");
    let s = p.sites.iter().find(|s| s.id == "drip_rock").unwrap();
    for o in p.sites.iter().filter(|o| o.kind.is_some()) {
        assert!(dist(&p, o.dir, s.dir) >= s.reach_m, "site {} inside the place", o.id);
    }
    assert!(p.site_pieces(p.sites.iter().position(|s| s.id == "drip_rock").unwrap()).is_empty(), "a place has no kit of its own");
}

#[test]
fn a_pad_off_the_centre_sits_where_the_place_turns_it() {
    let p = hearth(vec![place(PAD)]).unwrap();
    let (main, side) = (p.pad("drip_rock", "main").unwrap(), p.pad("drip_rock", "side").unwrap());
    assert!((dist(&p, main.up, side.up) - 10.0).abs() < 1e-3);
    let turned = PAD.replace("\"lon_deg\": 90.0,", "\"lon_deg\": 90.0, \"turn_deg\": 90.0,");
    let q = hearth(vec![place(&turned)]).unwrap();
    let side_t = q.pad("drip_rock", "side").unwrap();
    assert!((dist(&q, main.up, side_t.up) - 10.0).abs() < 1e-3);
    assert!((dist(&q, side.up, side_t.up) - 10.0 * 2f64.sqrt()).abs() < 1e-2, "a quarter turn moves the side pad by 10 √2 m");
    assert!(p.pad("drip_rock", "nope").is_none() && p.pad("nowhere", "main").is_none());
}

#[test]
fn places_are_deterministic() {
    let (a, b) = (hearth(vec![place(PAD)]).unwrap(), hearth(vec![place(PAD)]).unwrap());
    assert_eq!(a.sites.len(), b.sites.len());
    for (x, y) in a.sites.iter().zip(&b.sites) {
        assert_eq!((&x.id, x.dir), (&y.id, y.dir));
    }
}

#[test]
fn a_place_in_water_fails_the_bake() {
    // lat 80, lon 0 lies in water on Hearth (seed 1337).
    let wet = PAD.replace("\"lat_deg\": 86.0, \"lon_deg\": 90.0", "\"lat_deg\": 80.0, \"lon_deg\": 0.0");
    let e = hearth(vec![place(&wet)]).err().unwrap();
    assert!(e.contains("place drip_rock") && e.contains("water"), "{e}");
}

#[test]
fn place_checks() {
    let bad = PAD.replace("\"radius_m\": 4.0", "\"radius_m\": 0.0");
    assert!(hearth(vec![place(&bad)]).err().unwrap().contains("pads.radius_m"));
    let twice = PAD.replace("\"id\": \"side\"", "\"id\": \"main\"");
    assert!(hearth(vec![place(&twice)]).err().unwrap().contains("pads.id"));
    assert!(serde_json::from_str::<Place>(&PAD.replace("\"turn_deg\"", "\"x\"").replace("\"lon_deg\": 90.0,", "\"lon_deg\": 90.0, \"colour\": 1,")).is_err());
}

// ---------- near (#170): metres from another place, so distances survive a radius change ----------

const NEAR: &str = r#"{ "id": "post_box", "planet": "hearth",
  "near": { "place": "drip_rock", "east_m": 120.0, "north_m": -160.0 },
  "edits": [ { "type": "flatten", "order": 0, "radius_m": 10.0, "rolloff_m": 10.0 } ],
  "pads": [ { "id": "main", "at": [0.0, 0.0], "radius_m": 8.0 } ] }"#;


#[test]
fn a_near_place_sits_so_many_metres_from_its_reference_at_any_radius() {
    // The position is resolved by set_places, so no bake is needed (a bigger planet may fail its
    // biome quotas for reasons of its own); the pads of the baked planet are checked at 5000 m.
    for radius in [3000.0, 5000.0, 6500.0, 20000.0] {
        let mut p = Planet::new(Recipe::for_planet(HEARTH, 1337, radius).unwrap());
        p.set_places(vec![place(PAD), place(NEAR)]).unwrap();
        let (a, b) = (p.places[0].dir(), p.places[1].dir());
        let d = dist(&p, a, b);
        assert!((d - 200.0).abs() < 0.05, "radius {radius}: {d:.3} m, want 200 (120 east, 160 south)");
        let (east, north) = planet_core::look::tangent_frame(a);
        let off = b - a;
        let (e_m, n_m) = (off.dot(east) * radius, off.dot(north) * radius);
        assert!((e_m - 120.0).abs() < 0.5 && (n_m + 160.0).abs() < 0.5, "radius {radius}: east {e_m:.2}, north {n_m:.2}");
    }
    let p = hearth(vec![place(PAD), place(NEAR)]).unwrap();
    let (a, b) = (p.pad("drip_rock", "main").unwrap(), p.pad("post_box", "main").unwrap());
    assert!((dist(&p, a.up, b.up) - 200.0).abs() < 0.05);
}

#[test]
fn a_near_place_still_flattens_its_pad_and_sites_keep_away() {
    let p = hearth(vec![place(PAD), place(NEAR)]).unwrap();
    let s = p.sites.iter().find(|s| s.id == "post_box").unwrap();
    for o in p.sites.iter().filter(|o| o.kind.is_some()) {
        assert!(dist(&p, o.dir, s.dir) >= s.reach_m);
    }
    let pad = p.pad("post_box", "main").unwrap();
    assert!(pad.centre.length() - (p.radius + p.height_at(pad.up)) < 1e-6);
}

#[test]
fn near_checks() {
    // Unknown reference, a reference that is itself near, both ways at once, neither.
    let unknown = NEAR.replace("\"drip_rock\"", "\"nowhere\"");
    let e = hearth(vec![place(PAD), place(&unknown)]).err().unwrap();
    assert!(e.contains("post_box") && e.contains("near") && e.contains("nowhere"), "{e}");
    let chain = NEAR.replace("\"id\": \"post_box\"", "\"id\": \"second\"").replace("\"place\": \"drip_rock\"", "\"place\": \"post_box\"");
    let e = hearth(vec![place(PAD), place(NEAR), place(&chain)]).err().unwrap();
    assert!(e.contains("second") && e.contains("near") && e.contains("absolute"), "{e}");
    let both = NEAR.replace("\"planet\": \"hearth\",", "\"planet\": \"hearth\", \"lat_deg\": 80.0, \"lon_deg\": 0.0,");
    let e = hearth(vec![place(PAD), place(&both)]).err().unwrap();
    assert!(e.contains("post_box") && e.contains("either"), "{e}");
    let neither = NEAR.replace("\"near\": { \"place\": \"drip_rock\", \"east_m\": 120.0, \"north_m\": -160.0 },", "");
    let e = hearth(vec![place(PAD), place(&neither)]).err().unwrap();
    assert!(e.contains("post_box") && e.contains("lat_deg") && e.contains("near"), "{e}");
    assert!(serde_json::from_str::<Place>(&NEAR.replace("\"east_m\"", "\"east\"")).is_err());
}
