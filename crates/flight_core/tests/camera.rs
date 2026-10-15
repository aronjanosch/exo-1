use flight_core::limits::G0;
use flight_core::camera::{CameraFx, CameraTuning, FxInput};
use glam::{DVec2, DVec3};

const DT: f64 = 1.0 / 60.0;

/// Ship space: forward is -Z.
fn fwd_g(g: f64) -> DVec3 {
    DVec3::new(0.0, 0.0, -g * G0)
}

/// Inputs of a flight step at speed `v` with the given felt acceleration and boost.
fn flying(v: f64, accel: DVec3, boost: bool) -> FxInput {
    FxInput { speed: v, accel, boost, shake_scale: 1.0, dt: DT, ..FxInput::default() }
}

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
    let mut fx = CameraFx::default();
    fx.step(&t, 0.1, DVec2::ZERO, 0.1, false, DT);
    fx.step(&t, 0.1, DVec2::ZERO, 0.1, true, DT);
    assert_eq!(fx.bumps, 0);
}

/// Values that make the bump unbounded or NaN are refused.
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

/// Switch off: FOV, look-ahead and bump are the same as the view-only step, and the new effects
/// are zero, whatever the ship does.
#[test]
fn switch_off_keeps_the_old_effects_and_zeroes_the_new() {
    let t = CameraTuning::default();
    let mut old = CameraFx::default();
    let mut off = CameraFx::default();
    off.enabled = false;
    for k in 0..900 {
        let (speed, turn, approach, grounded) = match k {
            0..=299 => (50.0 + k as f64 * 0.2, DVec2::new(0.0, -2.0), 0.0, false),
            300..=309 => (5.0, DVec2::ZERO, 3.0, false),
            310 => (3.0, DVec2::ZERO, 3.0, true),
            _ => (0.0, DVec2::ZERO, 0.0, true),
        };
        old.step(&t, speed, turn, approach, grounded, DT);
        let mut i = flying(speed, fwd_g(3.0), k < 300);
        (i.turn, i.approach, i.grounded, i.thrust) = (turn, approach, grounded, 1.0);
        off.step_with(&t, &i);
        assert_eq!((off.fov_deg, off.streak, off.look, off.bumps), (old.fov_deg, old.streak, old.look, old.bumps), "step {k}");
        assert_eq!(off.bump(&t), old.bump(&t), "step {k}");
        assert_eq!((off.trauma, off.shake_offset, off.shake_angle, off.lag, off.g_fov), (0.0, DVec3::ZERO, DVec2::ZERO, DVec3::ZERO, 0.0), "step {k}");
    }
    assert_eq!(off.bumps, 1, "the touchdown still bumps");
}

#[test]
fn shake_stays_bounded_and_repeats_with_the_same_seed() {
    let t = CameraTuning::default();
    let run = || {
        let mut fx = CameraFx::default();
        let mut out = Vec::new();
        for k in 0..600 {
            fx.step_with(&t, &flying(100.0, DVec3::ZERO, k < 300));
            out.push((fx.shake_offset, fx.shake_angle));
        }
        out
    };
    let (a, b) = (run(), run());
    assert!(a == b, "same inputs, same offsets");
    let max_off = a.iter().map(|(o, _)| o.length()).fold(0.0, f64::max);
    let max_ang = a.iter().map(|(_, r)| r.abs().max_element()).fold(0.0, f64::max);
    println!("peak shake offset {max_off:.4} m (bound {}), angle {:.3} deg (bound {})", t.shake_max_offset, max_ang.to_degrees(), t.shake_max_angle_deg);
    assert!(max_off <= t.shake_max_offset + 1e-12 && max_off > 0.01, "{max_off}");
    assert!(max_ang <= t.shake_max_angle_deg.to_radians() + 1e-12 && max_ang > 0.0);
}

#[test]
fn shake_setting_zero_gives_no_shake_and_cabin_gets_less() {
    let t = CameraTuning::default();
    let mut fx = CameraFx::default();
    for _ in 0..120 {
        fx.step_with(&t, &FxInput { shake_scale: 0.0, ..flying(100.0, DVec3::ZERO, true) });
    }
    assert!(fx.trauma > 0.3 && fx.shake_offset == DVec3::ZERO && fx.shake_angle == DVec2::ZERO);
    let (mut chase, mut cabin) = (CameraFx::default(), CameraFx::default());
    for _ in 0..120 {
        chase.step_with(&t, &flying(100.0, DVec3::ZERO, true));
        cabin.step_with(&t, &FxInput { cabin: true, ..flying(100.0, DVec3::ZERO, true) });
    }
    assert_eq!(chase.trauma, cabin.trauma, "the same trauma");
    assert!(cabin.shake_offset.length() <= t.shake_max_offset * t.shake_scale_cabin + 1e-12);
    assert!(cabin.g_fov == 0.0 && cabin.lag == DVec3::ZERO, "the walker has no lag and no G field of view");
}

/// Gravity and ground contact are not felt: the input is zero on the ground (the caller's rule),
/// and a steady velocity gives no lag at all.
#[test]
fn steady_flight_gives_no_lag() {
    let t = CameraTuning::default();
    let mut fx = CameraFx::default();
    for _ in 0..600 {
        fx.step_with(&t, &flying(100.0, DVec3::ZERO, false));
    }
    assert!(fx.lag.length() < 1e-9 && fx.g_fov == 0.0);
}

#[test]
fn rejects_bad_spring_and_shake_values() {
    let cam = include_str!("../../../content/tuning/camera.json");
    for (from, to) in [("\"lag_damping\": 5.6", "\"lag_damping\": -5.6"), ("\"shake_decay\": 0.5", "\"shake_decay\": -0.5"), ("\"lag_max\": 1.5", "\"lag_max\": -1.5")] {
        assert!(cam.contains(from), "{from}");
        let e = CameraTuning::from_json(&cam.replacen(from, to, 1)).unwrap_err();
        assert!(e.contains(from.split('"').nth(1).unwrap()), "{e}");
    }
}
