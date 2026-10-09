use super::*;

const CONTENT: &str = include_str!("../../../content/daynight/daynight.json");

fn sun(axis: [f64; 3]) -> PlanetSun {
    PlanetSun { todo: None, day_length_s: 1000.0, axis, look: "x".into() }
}

/// A star direction at `decl` deg above the equator of the axis, `lon` deg round it from SPAWN_UP's meridian.
fn star_at(s: &PlanetSun, decl: f64, lon: f64) -> DVec3 {
    let a = s.axis();
    let m = (SPAWN_UP - a * a.dot(SPAWN_UP)).normalize();
    let (d, l) = (decl.to_radians(), lon.to_radians());
    DQuat::from_axis_angle(a, l) * m * d.cos() + a * d.sin()
}

const STAR_POS: [f64; 3] = [-120000000.0, 84000000.0, 24000000.0];

fn shipped_to_star(d: &DayNight, recipe: &str) -> DVec3 {
    let sys: serde_json::Value = serde_json::from_str(include_str!("../../../content/system/system.json")).unwrap();
    let p = sys["planets"].as_array().unwrap().iter().find(|p| p["recipe"] == recipe).unwrap();
    let c = DVec3::from_array(serde_json::from_value(p["centre"].clone()).unwrap());
    let star = DVec3::from_array(serde_json::from_value(sys["star"]["position"].clone()).unwrap());
    d.check_geometry(recipe, c, star).unwrap();
    to_star(star, c)
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
        let ts = shipped_to_star(&d, p);
        assert!(s.time_for_elevation(SPAWN_UP, ts, 0.0, true, 0.0).is_some(), "{p}: the sun sets at spawn");
        let night = s.time_for_hour(SPAWN_UP, ts, 0.0, 0.0).unwrap();
        assert!(s.elevation_deg(SPAWN_UP, ts, night) < -6.0, "{p}: midnight is dark");
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
    assert!(broken(&|v| v["planets"]["hearth"]["day_length_s"] = (-5.0).into()).is_err());
    assert!(broken(&|v| v["planets"]["hearth"]["axis"] = serde_json::json!([1.0, "x", 0.0])).is_err());
    assert!(broken(&|v| v["planets"]["hearth"]["start_hour"] = 10.0.into()).is_err(), "derived now, not configurable");
    assert!(broken(&|v| v["space_blend"]["near_m"] = 500000.0.into()).unwrap_err().contains("space_blend"));
    assert!(broken(&|v| v["looks"]["temperate"]["keys"][0]["sun_color"] = serde_json::json!([1.0, -1.0, 0.0])).is_err());
    assert!(broken(&|v| v["looks"]["temperate"]["keys"] = serde_json::json!([])).is_err());
    assert!(broken(&|v| v["planets"]["hearth"]["extra"] = 1.into()).is_err());
    assert!(broken(&|v| v["looks"]["temperate"]["keys"][1]["elevation_deg"] = (-30.0).into()).unwrap_err().contains("sorted"));
    assert!(broken(&|v| v["looks"]["temperate"]["keys"][0]["sun_lux"] = (-1.0).into()).is_err());
}

#[test]
fn inputs_that_would_give_nan_are_errors_not_panics() {
    // day_length_s that overflows, an axis that overflows: from_json says so instead of NaN later.
    for bad in [
        CONTENT.replace("\"day_length_s\": 1200.0", "\"day_length_s\": 1e999"),
        CONTENT.replace("\"day_length_s\": 1200.0", "\"day_length_s\": null"),
        CONTENT.replace("\"axis\": [0.0, 0.25, 1.0]", "\"axis\": [0.0, 0.0, 0.0]"),
        CONTENT.replace("\"ambient\": 100.0", "\"ambient\": 1e999"),
    ] {
        assert!(DayNight::from_json(&bad).is_err());
    }
    assert!(DayNight::from_json("").is_err() && DayNight::from_json("{}").is_err());
}

#[test]
fn geometry_checks_catch_star_on_centre_and_axis_at_star() {
    let d = DayNight::from_json(CONTENT).unwrap();
    let c = DVec3::new(1.0, 2.0, 3.0);
    assert!(d.check_geometry("hearth", c, c).unwrap_err().contains("centre"));
    assert!(d.check_geometry("hearth", c, DVec3::new(f64::NAN, 0.0, 0.0)).is_err());
    let (s, _) = d.planet("hearth").unwrap();
    assert!(d.check_geometry("hearth", c, c + s.axis() * 1e8).unwrap_err().contains("axis"));
    assert!(d.check_geometry("nowhere", c, c + DVec3::X * 1e8).is_err());
    assert!(d.check_geometry("hearth", c, c + DVec3::X * 1e8).is_ok());
}

#[test]
fn declination_and_spawn_hour_come_from_the_geometry() {
    let s = sun([0.0, 0.25, 1.0]);
    for (decl, lon) in [(15.0, 0.0), (-5.0, 40.0), (60.0, -100.0), (0.0, 170.0)] {
        let ts = star_at(&s, decl, lon);
        assert!((s.declination_deg(ts) - decl).abs() < 1e-9);
        // The star `lon` deg past the spawn meridian (turning with time) at clock 0: hour angle +lon.
        let want = (12.0 + lon / 360.0 * 24.0).rem_euclid(24.0);
        let got = s.local_hour(SPAWN_UP, ts, 0.0).unwrap();
        let diff = (got - want + 12.0).rem_euclid(24.0) - 12.0;
        assert!(diff.abs() < 1e-9, "decl {decl} lon {lon}: hour {got}, want {want}");
        assert!((s.sun_dir(ts, 0.0) - ts).length() < 1e-12, "clock 0 is the star's own direction");
    }
}

#[test]
fn sun_turns_once_per_day_around_the_axis() {
    let s = sun([0.3, 0.2, 1.0]);
    let ts = star_at(&s, 20.0, 50.0);
    let a = s.axis();
    let s0 = s.sun_dir(ts, 0.0);
    assert!((s.sun_dir(ts, s.day_length_s) - s0).length() < 1e-9, "a full turn after day_length_s");
    assert!((s.sun_dir(ts, s.day_length_s / 2.0) - s0).length() > 0.5, "half a day is elsewhere");
    for t in [0.0, 123.0, 500.0, 999.0] {
        let d = s.sun_dir(ts, t);
        assert!((d.length() - 1.0).abs() < 1e-12);
        assert!((d.dot(a).asin().to_degrees() - 20.0).abs() < 1e-9, "declination holds at t={t}");
    }
    // A quarter day turns the sun's equator projection by 90 degrees.
    let eq = |v: DVec3| (v - a * a.dot(v)).normalize();
    let q = eq(s.sun_dir(ts, 0.0)).dot(eq(s.sun_dir(ts, 250.0)));
    assert!(q.abs() < 1e-9);
}

#[test]
fn local_hour_advances_24_hours_per_day() {
    let s = sun([0.0, 0.25, 1.0]);
    let ts = star_at(&s, 15.0, 30.0);
    let h0 = s.local_hour(SPAWN_UP, ts, 0.0).unwrap();
    let h1 = s.local_hour(SPAWN_UP, ts, s.day_length_s / 24.0 * 3.0).unwrap();
    assert!((h1 - (h0 + 3.0)).abs() < 1e-9, "{h0} -> {h1}");
}

#[test]
fn noon_is_the_highest_sun_and_midnight_the_lowest() {
    let s = sun([0.0, 0.25, 1.0]);
    let ts = star_at(&s, 15.0, 30.0);
    let up = DVec3::new(0.3, 0.9, 0.1).normalize();
    let noon = s.time_for_hour(up, ts, 12.0, 0.0).unwrap();
    let midnight = s.time_for_hour(up, ts, 0.0, 0.0).unwrap();
    let e_noon = s.elevation_deg(up, ts, noon);
    let e_mid = s.elevation_deg(up, ts, midnight);
    for i in 0..200 {
        let t = i as f64 * 5.0;
        let e = s.elevation_deg(up, ts, t);
        assert!(e <= e_noon + 1e-9 && e >= e_mid - 1e-9, "t={t}: {e} outside {e_mid}..{e_noon}");
    }
    // Noon elevation: 90 - |latitude - declination|.
    let lat = s.axis().dot(up).asin().to_degrees();
    assert!((e_noon - (90.0 - (lat - 15.0).abs())).abs() < 1e-9);
}

#[test]
fn time_for_hour_is_the_first_time_at_or_after_from() {
    let s = sun([0.0, 0.0, 1.0]);
    let ts = star_at(&s, 0.0, -90.0); // 6:00 at spawn at clock 0
    assert!((s.local_hour(SPAWN_UP, ts, 0.0).unwrap() - 6.0).abs() < 1e-9);
    let t = s.time_for_hour(SPAWN_UP, ts, 12.0, 0.0).unwrap();
    assert!((t - 250.0).abs() < 1e-9, "6:00 to 12:00 is a quarter day, got {t}");
    let t2 = s.time_for_hour(SPAWN_UP, ts, 12.0, t + 1.0).unwrap();
    assert!((t2 - (t + 1000.0)).abs() < 1e-6, "next noon a day later, got {t2}");
}

#[test]
fn time_for_elevation_finds_dusk_and_dawn() {
    let s = sun([0.0, 0.25, 1.0]);
    let ts = star_at(&s, 15.0, 30.0);
    let up = SPAWN_UP;
    let dusk = s.time_for_elevation(up, ts, -2.0, true, 0.0).unwrap();
    let dawn = s.time_for_elevation(up, ts, -2.0, false, 0.0).unwrap();
    assert!((s.elevation_deg(up, ts, dusk) + 2.0).abs() < 1e-6);
    assert!((s.elevation_deg(up, ts, dawn) + 2.0).abs() < 1e-6);
    // Setting in the evening: a moment later the sun is lower; rising at dawn: higher.
    assert!(s.elevation_deg(up, ts, dusk + 1.0) < -2.0);
    assert!(s.elevation_deg(up, ts, dawn + 1.0) > -2.0);
    assert!(s.local_hour(up, ts, dusk).unwrap() > 12.0 && s.local_hour(up, ts, dawn).unwrap() < 12.0);
}

#[test]
fn polar_day_has_no_sunset() {
    // Axis through the spawn point's neighbourhood and a high sun: it circles without setting.
    let s = sun([0.0, 1.0, 0.1]);
    let ts = star_at(&s, 30.0, 0.0);
    assert!(s.time_for_elevation(SPAWN_UP, ts, 0.0, true, 0.0).is_none());
    assert!(s.elevation_deg(SPAWN_UP, ts, 0.0) > 0.0);
}

#[test]
fn light_is_the_planets_sun_near_and_the_stars_true_direction_far() {
    let d = DayNight::from_json(CONTENT).unwrap();
    let b = d.space_blend;
    let star = DVec3::from_array(STAR_POS);
    let planets = [("hearth", DVec3::ZERO), ("cinder", DVec3::new(12.5e6, 0.0, 0.0))];
    let far = DVec3::new(6.0e6, 3.0e6, -2.0e6);
    for t in [0.0, 311.0, 1777.0] {
        let dirs: Vec<DVec3> = planets
            .iter()
            .map(|(r, c)| {
                let (s, _) = d.planet(r).unwrap();
                let near = *c + DVec3::Y * 5000.0;
                let local = s.light_dir(&b, star, *c, near, t);
                assert!((local - s.sun_dir(to_star(star, *c), t)).length() < 1e-12, "{r}: near the planet its own sun");
                s.light_dir(&b, star, *c, far, t)
            })
            .collect();
        let truth = to_star(star, far);
        for (r, dir) in dirs.iter().enumerate() {
            assert!((*dir - truth).length() < 1e-12, "planet {r} at t={t}: far light is the star's direction");
        }
    }
}

#[test]
fn light_blend_is_continuous_unit_and_monotone() {
    let b = SpaceBlend { near_m: 1000.0, far_m: 5000.0 };
    let (local, star) = (DVec3::X, DVec3::new(-1.0, 0.0, 0.0001).normalize()); // nearly opposite
    let mut prev = 0.0;
    for i in 0..=100 {
        let dist = 500.0 + i as f64 * 60.0;
        let d = b.mix(local, star, dist);
        assert!((d.length() - 1.0).abs() < 1e-9 && d.is_finite());
        let ang = d.angle_between(local);
        assert!(ang >= prev - 1e-9 && ang - prev < 0.1, "dist {dist}: jump {}", ang - prev);
        prev = ang;
    }
    assert_eq!(b.mix(local, star, 1000.0), local);
    assert_eq!(b.mix(local, star, 5000.0), star);
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
