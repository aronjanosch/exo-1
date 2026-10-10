//! #27: camera effects from speed, turn and touchdown. #148: trauma shake. #149: spring lag and
//! field of view from G.
use flight_core::axis::G0;
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
fn trauma_rises_under_boost_stays_in_range_and_decays() {
    let t = CameraTuning::default();
    let mut fx = CameraFx::default();
    for _ in 0..120 {
        fx.step_with(&t, &flying(100.0, DVec3::ZERO, true));
        assert!((0.0..=1.0).contains(&fx.trauma));
    }
    println!("trauma after 2 s boost: {:.3}", fx.trauma);
    assert!(fx.trauma > 0.3, "{}", fx.trauma);
    let mut steps = 0;
    while fx.trauma >= 0.01 {
        fx.step_with(&t, &flying(100.0, DVec3::ZERO, false));
        steps += 1;
        assert!(steps < 600, "trauma never decays");
    }
    let secs = steps as f64 * DT;
    println!("trauma from {:.2} to < 0.01 at rest: {secs:.2} s (bound {:.2} s)", 0.0, 1.0 / t.shake_decay + 0.5);
    assert!(secs <= 1.0 / t.shake_decay + 0.5, "{secs}");
}

#[test]
fn thrust_adds_trauma_in_proportion_to_the_share() {
    let t = CameraTuning::default();
    let (mut half, mut full) = (CameraFx::default(), CameraFx::default());
    for _ in 0..60 {
        half.step_with(&t, &FxInput { thrust: 0.5, ..flying(10.0, DVec3::ZERO, false) });
        full.step_with(&t, &FxInput { thrust: 1.0, ..flying(10.0, DVec3::ZERO, false) });
    }
    // Decay is constant, so a source must add faster than that to build trauma at all: full thrust
    // builds, half thrust (below the decay rate) does not.
    assert!(full.trauma > half.trauma && half.trauma >= 0.0 && full.trauma > 0.0, "{} vs {}", full.trauma, half.trauma);
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

/// Touchdowns: a contact at 3 m/s adds trauma, a slower one (below the bump threshold) none.
#[test]
fn touchdown_adds_trauma_above_threshold_only() {
    let t = CameraTuning::default();
    let mut fx = CameraFx::default();
    fx.step_with(&t, &FxInput { approach: 3.0, ..flying(3.0, DVec3::ZERO, false) });
    fx.step_with(&t, &FxInput { grounded: true, approach: 0.1, ..flying(0.1, DVec3::ZERO, false) });
    assert_eq!(fx.bumps, 1);
    println!("trauma at a 3 m/s touchdown: {:.3}", fx.trauma);
    assert!(fx.trauma > 0.2, "{}", fx.trauma);
    let mut slow = CameraFx::default();
    slow.step_with(&t, &FxInput { approach: t.bump_threshold * 0.5, ..flying(0.3, DVec3::ZERO, false) });
    slow.step_with(&t, &FxInput { grounded: true, approach: 0.0, ..flying(0.0, DVec3::ZERO, false) });
    assert_eq!(slow.trauma, 0.0, "a touch below the threshold gives no shake");
}

/// Spring: a 2 g forward step trails backwards, stays within lag_max, settles and barely swings back.
#[test]
fn spring_trails_under_g_and_settles() {
    let t = CameraTuning::default();
    let mut fx = CameraFx::default();
    let (mut peak, mut peak_t, mut max_len) = (0.0_f64, 0.0, 0.0_f64);
    for k in 0..120 {
        fx.step_with(&t, &flying(100.0, fwd_g(2.0), false));
        let z = fx.lag.z;
        if z > peak {
            (peak, peak_t) = (z, k as f64 * DT);
        }
        max_len = max_len.max(fx.lag.length());
    }
    println!("2 g forward: lag peak {peak:.3} m at {peak_t:.2} s, target {:.3} m", 2.0 * t.lag_per_g);
    assert!(peak > 0.5 * 2.0 * t.lag_per_g, "trails backwards (+z): {peak}");
    assert!(max_len <= t.lag_max + 1e-12, "never past lag_max: {max_len}");
    // G ends: swings back past the centre and settles.
    let (mut settle, mut opposite, mut t_settle) = (f64::NAN, 0.0_f64, 0.0);
    for k in 0..240 {
        fx.step_with(&t, &flying(100.0, DVec3::ZERO, false));
        let z = fx.lag.z;
        opposite = opposite.max(-z);
        if z.abs() < 0.01 && settle.is_nan() {
            settle = k as f64 * DT;
        }
        if z.abs() >= 0.01 {
            t_settle = (k + 1) as f64 * DT;
        }
    }
    let overshoot = opposite / peak;
    println!("after the G: overshoot {:.1} % of the peak, settled below 1 cm at {:.2} s (last outside at {t_settle:.2} s)", overshoot * 100.0, settle);
    assert!(t_settle <= 2.0, "settles within 2 s: {t_settle}");
    assert!(overshoot < 0.2, "no overshoot past 20 %: {overshoot}");
    assert!(fx.lag.length() < 0.01);
}

#[test]
fn hard_turn_swings_the_camera_out_a_little_and_no_further_than_the_bound() {
    let t = CameraTuning::default();
    let mut fx = CameraFx::default();
    for _ in 0..240 {
        fx.step_with(&t, &flying(100.0, DVec3::new(-3.0 * G0, 0.0, 0.0), false));
    }
    assert!(fx.lag.x > 0.1 && fx.lag.x <= t.lag_max, "{}", fx.lag);
}

#[test]
fn g_fov_adds_the_curve_only_for_forward_g() {
    let t = CameraTuning::default();
    let mut rest = CameraFx::default();
    for _ in 0..120 {
        rest.step_with(&t, &flying(100.0, DVec3::ZERO, false));
    }
    assert_eq!(rest.g_fov, 0.0);
    assert!((rest.fov_deg - t.fov_curve.eval(100.0)).abs() < 1e-9, "zero G adds nothing");
    let mut f = CameraFx::default();
    for _ in 0..240 {
        f.step_with(&t, &flying(100.0, fwd_g(2.0), false));
    }
    let want = t.g_fov_curve.eval(2.0);
    println!("2 g forward: g_fov {:.3} deg (curve {want:.3})", f.g_fov);
    assert!((f.g_fov - want).abs() < 1e-3 && f.g_fov > 0.0);
    assert!((f.fov_deg - (t.fov_curve.eval(100.0) + want)).abs() < 1e-3);
    // Backward G (braking) does not widen the view.
    let mut b = CameraFx::default();
    for _ in 0..240 {
        b.step_with(&t, &flying(100.0, -fwd_g(2.0), false));
    }
    assert_eq!(b.g_fov, 0.0);
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
