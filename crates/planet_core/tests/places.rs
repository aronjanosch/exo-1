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
