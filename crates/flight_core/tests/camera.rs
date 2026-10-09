//! #27: camera effects from speed, turn and touchdown.
use flight_core::camera::{CameraFx, CameraTuning};
use glam::DVec2;

const DT: f64 = 1.0 / 60.0;

#[test]
fn shipped_file_equals_default() {
    assert_eq!(CameraTuning::from_json(include_str!("../../../content/tuning/camera.json")).unwrap(), CameraTuning::default());
}

#[test]
fn fov_and_streaks_rise_with_speed() {
    let t = CameraTuning::default();
    let mut fx = CameraFx::default();
    fx.step(&t, 0.0, DVec2::ZERO, 0.0, false, DT);
    let (fov0, s0) = (fx.fov_deg, fx.streak);
    fx.step(&t, 400.0, DVec2::ZERO, 0.0, false, DT);
    assert!(fx.fov_deg > fov0 + 5.0 && fx.streak > s0, "{fov0} -> {}, {s0} -> {}", fx.fov_deg, fx.streak);
    assert_eq!(s0, 0.0);
}

#[test]
fn look_ahead_leads_the_turn_capped_and_eases_back() {
    let t = CameraTuning::default();
    let mut fx = CameraFx::default();
    for _ in 0..120 {
        fx.step(&t, 50.0, DVec2::new(0.0, -2.5), 0.0, false, DT);
    }
    assert!((fx.look.y + t.look_ahead_max_yaw_deg.to_radians()).abs() < 1e-3, "right turn: view right, capped: {}", fx.look.y);
    fx.step(&t, 50.0, DVec2::ZERO, 0.0, false, DT);
    assert!(fx.look.y.abs() < t.look_ahead_max_yaw_deg.to_radians(), "eases, no snap");
    for _ in 0..120 {
        fx.step(&t, 50.0, DVec2::new(0.05, 0.0), 0.0, false, DT);
    }
    assert!(fx.look.length() < 1e-3, "inside the dead zone it settles at the centre: {}", fx.look);
}

#[test]
fn touchdown_bumps_once_and_dies_out() {
    let t = CameraTuning::default();
    let mut fx = CameraFx::default();
    // Approaching at 2 m/s; the contact flag comes a step after the solver cut the approach.
    fx.step(&t, 2.0, DVec2::ZERO, 2.0, false, DT);
    fx.step(&t, 0.1, DVec2::ZERO, 0.1, false, DT);
    assert_eq!(fx.bumps, 0);
    fx.step(&t, 0.1, DVec2::ZERO, 0.1, true, DT);
    assert_eq!(fx.bumps, 1);
    let peak = (0..30).map(|_| {
        fx.step(&t, 0.0, DVec2::ZERO, 0.0, true, DT);
        fx.bump(&t)
    }).fold(0.0, f64::max);
    assert!(peak > 0.05 && peak <= t.bump_max, "{peak}");
    for _ in 0..120 {
        fx.step(&t, 0.0, DVec2::ZERO, 0.0, true, DT);
    }
    assert!(fx.bump(&t) < 1e-3);
    assert_eq!(fx.bumps, 1, "resting does not bump again");
}

/// #110 point 4: a pull-up near the ground without contact does not bump; a touch does, also
/// when the centre is high (a corner on a steep slope).
#[test]
fn bump_on_contact_not_on_a_pull_up() {
    let t = CameraTuning::default();
    let mut fx = CameraFx::default();
    fx.step(&t, 10.0, DVec2::ZERO, 5.0, false, DT);
    fx.step(&t, 10.0, DVec2::ZERO, 0.0, false, DT);
    for _ in 0..30 {
        fx.step(&t, 10.0, DVec2::ZERO, -3.0, false, DT);
    }
    assert_eq!(fx.bumps, 0, "pull-up without contact");
    fx.step(&t, 3.0, DVec2::ZERO, 3.0, false, DT);
    fx.step(&t, 3.0, DVec2::ZERO, 3.0, true, DT);
    assert_eq!(fx.bumps, 1, "contact");
    // A slow touch (below the threshold) does not bump.
    let mut fx = CameraFx::default();
    fx.step(&t, 0.1, DVec2::ZERO, 0.1, false, DT);
    fx.step(&t, 0.1, DVec2::ZERO, 0.1, true, DT);
    assert_eq!(fx.bumps, 0);
}

/// #106 point 6: values that make the bump unbounded or NaN are refused.
#[test]
fn rejects_bad_values() {
    let cam = include_str!("../../../content/tuning/camera.json");
    for (from, to) in [
        ("\"bump_damping\": 6.0", "\"bump_damping\": -1.0"),
        ("\"bump_frequency\": 3.0", "\"bump_frequency\": -3.0"),
        ("\"bump_max\": 0.5", "\"bump_max\": -0.5"),
        ("\"bump_per_speed\": 0.08", "\"bump_per_speed\": -0.08"),
        ("\"look_ahead_ease_time\": 0.3", "\"look_ahead_ease_time\": -0.3"),
        ("\"look_ahead_gain\": 0.25", "\"look_ahead_gain\": -0.25"),
        ("\"bump_threshold\": 0.3", "\"bump_threshold\": -0.3"),
    ] {
        assert!(cam.contains(from), "{from}");
        let e = CameraTuning::from_json(&cam.replacen(from, to, 1)).unwrap_err();
        let name = from.split('"').nth(1).unwrap();
        assert!(e.contains(name), "{to}: {e}");
    }
}

#[test]
fn rejects_non_finite_values_built_in_code() {
    let t = CameraTuning { chase_offset: [0.0, f64::NAN, 17.0], ..CameraTuning::default() };
    assert!(t.validate().unwrap_err().contains("chase_offset"));
    let t = CameraTuning { bump_damping: f64::INFINITY, ..CameraTuning::default() };
    assert!(t.validate().unwrap_err().contains("bump_damping"));
}
