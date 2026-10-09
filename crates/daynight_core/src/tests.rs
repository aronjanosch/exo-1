use super::*;

const CONTENT: &str = include_str!("../../../content/daynight/daynight.json");

fn sun(axis: [f64; 3], decl: f64, start_hour: f64) -> PlanetSun {
    PlanetSun { todo: None, day_length_s: 1000.0, axis, declination_deg: decl, start_hour, look: "x".into() }
}

fn key(el: f64, sun_lux: f64, night_lux: f64, ambient: f64) -> LightKey {
    LightKey {
        elevation_deg: el,
        sun_lux,
        sun_color: [1.0; 3],
        night_lux,
        night_color: [1.0; 3],
        ambient,
        ambient_color: [1.0; 3],
        sky_color: [0.0; 3],
        fog_tint: [1.0; 3],
        fog_density: 1.0,
    }
}

#[test]
fn shipped_content_parses_and_has_both_planets() {
    let d = DayNight::from_json(CONTENT).unwrap();
    for p in ["hearth", "cinder"] {
        let (s, look) = d.planet(p).unwrap();
        assert!(s.day_length_s > 0.0);
        assert!(!look.keys.is_empty());
    }
    assert!(d.planet("nowhere").is_err());
}

#[test]
fn shipped_planets_have_day_and_night_at_spawn() {
    let d = DayNight::from_json(CONTENT).unwrap();
    for p in ["hearth", "cinder"] {
        let (s, _) = d.planet(p).unwrap();
        assert!(s.time_for_elevation(SPAWN_UP, 0.0, true, 0.0).is_some(), "{p}: the sun sets at spawn");
        let night = s.time_for_hour(SPAWN_UP, 0.0, 0.0).unwrap();
        assert!(s.elevation_deg(SPAWN_UP, night) < -6.0, "{p}: midnight is dark");
    }
}

#[test]
fn rejects_unknown_look_unsorted_keys_and_bad_values() {
    let d: serde_json::Value = serde_json::from_str(CONTENT).unwrap();
    let broken = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v = d.clone();
        f(&mut v);
        DayNight::from_json(&v.to_string())
    };
    assert!(broken(&|v| v["planets"]["hearth"]["look"] = "nope".into()).unwrap_err().contains("unknown look"));
    assert!(broken(&|v| v["planets"]["hearth"]["day_length_s"] = 0.0.into()).is_err());
    assert!(broken(&|v| v["planets"]["hearth"]["axis"] = serde_json::json!([0.0, 0.0, 0.0])).is_err());
    assert!(broken(&|v| v["planets"]["hearth"]["start_hour"] = 24.0.into()).is_err());
    assert!(broken(&|v| v["planets"]["hearth"]["extra"] = 1.into()).is_err());
    assert!(broken(&|v| v["looks"]["temperate"]["keys"][1]["elevation_deg"] = (-30.0).into()).unwrap_err().contains("sorted"));
    assert!(broken(&|v| v["looks"]["temperate"]["keys"][0]["sun_lux"] = (-1.0).into()).is_err());
}

#[test]
fn start_hour_is_the_spawn_hour_at_clock_zero() {
    for h in [0.0, 6.0, 10.0, 12.0, 18.5, 23.9] {
        let s = sun([0.0, 0.25, 1.0], 15.0, h);
        let got = s.local_hour(SPAWN_UP, 0.0).unwrap();
        let diff = (got - h + 12.0).rem_euclid(24.0) - 12.0;
        assert!(diff.abs() < 1e-9, "start {h}: local hour {got}");
    }
}

#[test]
fn sun_turns_once_per_day_around_the_axis() {
    let s = sun([0.3, 0.2, 1.0], 20.0, 7.0);
    let a = s.axis();
    let s0 = s.sun_dir(0.0);
    assert!((s.sun_dir(s.day_length_s) - s0).length() < 1e-9, "a full turn after day_length_s");
    assert!((s.sun_dir(s.day_length_s / 2.0) - s0).length() > 0.5, "half a day is elsewhere");
    for t in [0.0, 123.0, 500.0, 999.0] {
        let d = s.sun_dir(t);
        assert!((d.length() - 1.0).abs() < 1e-12);
        assert!((d.dot(a).asin().to_degrees() - 20.0).abs() < 1e-9, "declination holds at t={t}");
    }
    // A quarter day turns the sun's equator projection by 90 degrees.
    let eq = |v: DVec3| (v - a * a.dot(v)).normalize();
    let q = eq(s.sun_dir(0.0)).dot(eq(s.sun_dir(250.0)));
    assert!(q.abs() < 1e-9);
}

#[test]
fn local_hour_advances_24_hours_per_day() {
    let s = sun([0.0, 0.25, 1.0], 15.0, 10.0);
    let h0 = s.local_hour(SPAWN_UP, 0.0).unwrap();
    let h1 = s.local_hour(SPAWN_UP, s.day_length_s / 24.0 * 3.0).unwrap();
    assert!((h1 - (h0 + 3.0)).abs() < 1e-9, "{h0} -> {h1}");
}

#[test]
fn noon_is_the_highest_sun_and_midnight_the_lowest() {
    let s = sun([0.0, 0.25, 1.0], 15.0, 10.0);
    let up = DVec3::new(0.3, 0.9, 0.1).normalize();
    let noon = s.time_for_hour(up, 12.0, 0.0).unwrap();
    let midnight = s.time_for_hour(up, 0.0, 0.0).unwrap();
    let e_noon = s.elevation_deg(up, noon);
    let e_mid = s.elevation_deg(up, midnight);
    for i in 0..200 {
        let t = i as f64 * 5.0;
        let e = s.elevation_deg(up, t);
        assert!(e <= e_noon + 1e-9 && e >= e_mid - 1e-9, "t={t}: {e} outside {e_mid}..{e_noon}");
    }
    // Noon elevation: 90 - |latitude - declination|.
    let lat = s.axis().dot(up).asin().to_degrees();
    assert!((e_noon - (90.0 - (lat - 15.0).abs())).abs() < 1e-9);
}

#[test]
fn time_for_hour_is_the_first_time_at_or_after_from() {
    let s = sun([0.0, 0.0, 1.0], 0.0, 6.0);
    let t = s.time_for_hour(SPAWN_UP, 12.0, 0.0).unwrap();
    assert!((t - 250.0).abs() < 1e-9, "6:00 to 12:00 is a quarter day, got {t}");
    let t2 = s.time_for_hour(SPAWN_UP, 12.0, t + 1.0).unwrap();
    assert!((t2 - (t + 1000.0)).abs() < 1e-6, "next noon a day later, got {t2}");
}

#[test]
fn time_for_elevation_finds_dusk_and_dawn() {
    let s = sun([0.0, 0.25, 1.0], 15.0, 10.0);
    let up = SPAWN_UP;
    let dusk = s.time_for_elevation(up, -2.0, true, 0.0).unwrap();
    let dawn = s.time_for_elevation(up, -2.0, false, 0.0).unwrap();
    assert!((s.elevation_deg(up, dusk) + 2.0).abs() < 1e-6);
    assert!((s.elevation_deg(up, dawn) + 2.0).abs() < 1e-6);
    // Setting in the evening: a moment later the sun is lower; rising at dawn: higher.
    assert!(s.elevation_deg(up, dusk + 1.0) < -2.0);
    assert!(s.elevation_deg(up, dawn + 1.0) > -2.0);
    assert!(s.local_hour(up, dusk).unwrap() > 12.0 && s.local_hour(up, dawn).unwrap() < 12.0);
}

#[test]
fn polar_day_has_no_sunset() {
    // Axis through the spawn point's neighbourhood and a high sun: it circles without setting.
    let s = sun([0.0, 1.0, 0.1], 30.0, 0.0);
    assert!(s.time_for_elevation(SPAWN_UP, 0.0, true, 0.0).is_none());
    assert!(s.elevation_deg(SPAWN_UP, 0.0) > 0.0);
}

#[test]
fn look_interpolates_and_clamps() {
    let look = Look { todo: None, keys: vec![key(-10.0, 0.0, 100.0, 50.0), key(0.0, 1000.0, 0.0, 150.0), key(30.0, 9000.0, 0.0, 400.0)] };
    assert_eq!(look.sample(-90.0).sun_lux, 0.0);
    assert_eq!(look.sample(-90.0).night_lux, 100.0);
    assert_eq!(look.sample(90.0).sun_lux, 9000.0);
    let mid = look.sample(15.0);
    assert!((mid.sun_lux - 5000.0).abs() < 1e-9 && (mid.ambient - 275.0).abs() < 1e-9);
    assert_eq!(mid.elevation_deg, 15.0);
    assert_eq!(look.sample(0.0).sun_lux, 1000.0, "exactly on a key");
}

#[test]
fn shipped_looks_are_brighter_at_noon_than_dusk_than_night() {
    let d = DayNight::from_json(CONTENT).unwrap();
    for (id, look) in &d.looks {
        let b = |e: f64| look.sample(e).ground_brightness();
        let (noon, dusk) = (b(60.0), b(-2.0));
        // The night light comes from the antisolar direction: brightest when the sun is lowest.
        let night = (18..=90).map(|e| b(-(e as f64))).fold(0.0, f64::max);
        assert!(noon > dusk && dusk > night, "{id}: noon {noon}, dusk {dusk}, night {night}");
        assert!(night > 0.0, "{id}: the night light keeps the night above black");
        for e in [-10.0, -2.0, -0.1, 0.0] {
            assert_eq!(look.sample(e).sun_lux, 0.0, "{id}: no sun under the horizon ({e} deg)");
        }
    }
}
