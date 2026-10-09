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
    fx.step(&t, 2.0, DVec2::ZERO, 2.0, true, DT);
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
