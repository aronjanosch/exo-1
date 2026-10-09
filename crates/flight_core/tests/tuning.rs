//! Ship tuning as data (#20): the shipped file, the parser's rejections and the curve.
use flight_core::{smoothstep, Curve, Interp, ShipTuning};
use glam::DVec2;

const SHIP: &str = include_str!("../../../content/tuning/ship.json");

/// The `forward_speed_at` of `main` before #20, kept here as the reference.
fn old_forward_speed_at(c: &[DVec2], clearance: f64) -> f64 {
    for i in 1..c.len() {
        let (lo, hi) = (c[i - 1], c[i]);
        if clearance <= hi.x {
            return lo.y + (hi.y - lo.y) * smoothstep(lo.x, hi.x, clearance);
        }
    }
    c[c.len() - 1].y
}

#[test]
fn shipped_file_equals_default() {
    assert_eq!(ShipTuning::from_json(SHIP).unwrap(), ShipTuning::default());
}

fn edited(from: &str, to: &str) -> String {
    assert!(SHIP.contains(from), "fixture text {from:?} not in ship.json");
    SHIP.replacen(from, to, 1)
}

#[test]
fn rejects_unknown_field() {
    let e = ShipTuning::from_json(&edited("\"drag_k\"", "\"drag_kk\": 1.0, \"drag_k\"")).unwrap_err();
    assert!(e.contains("drag_kk"), "{e}");
}

#[test]
fn rejects_missing_field() {
    let e = ShipTuning::from_json(&edited("\"drag_k\": 0.0005,", "")).unwrap_err();
    assert!(e.contains("drag_k"), "{e}");
}

#[test]
fn rejects_curve_with_one_point() {
    let e = ShipTuning::from_json(&edited("[[30, 45], [150, 60], [600, 150], [1200, 350]]", "[[30, 45]]")).unwrap_err();
    assert!(e.contains("forward_speed_curve") && e.contains("2 points"), "{e}");
}

#[test]
fn rejects_unsorted_curve() {
    let e = ShipTuning::from_json(&edited("[[30, 45], [150, 60]", "[[150, 45], [30, 60]")).unwrap_err();
    assert!(e.contains("forward_speed_curve") && e.contains("ascending"), "{e}");
}

#[test]
fn rejects_non_string_comment() {
    let e = ShipTuning::from_json(&SHIP.replacen("\"_comment\": \"", "\"_comment\": 1, \"x\": \"", 1)).unwrap_err();
    assert!(e.contains("_comment"), "{e}");
}

#[test]
fn smooth_curve_reproduces_forward_speed_at() {
    let t = ShipTuning::default();
    let pts = t.forward_speed_curve.points.clone();
    let mut x = -50.0;
    while x < 1500.0 {
        assert_eq!(t.forward_speed_curve.eval(x), old_forward_speed_at(&pts, x), "at {x}");
        x += 7.3;
    }
    for p in &pts {
        assert_eq!(t.forward_speed_curve.eval(p.x), p.y);
    }
}

#[test]
fn linear_identity() {
    let c = Curve::new(Interp::Linear, vec![DVec2::ZERO, DVec2::ONE]).unwrap();
    for x in [0.0, 0.1, 0.25, 0.5, 0.9, 1.0] {
        assert_eq!(c.eval(x), x);
    }
    assert_eq!(c.eval(-1.0), 0.0);
    assert_eq!(c.eval(2.0), 1.0);
}

#[test]
fn curve_rejects_non_finite() {
    assert!(Curve::new(Interp::Linear, vec![DVec2::ZERO, DVec2::new(1.0, f64::NAN)]).is_err());
}

#[test]
fn rejects_landing_slope_limit_out_of_range() {
    for bad in ["-1.0", "91.0"] {
        let e = ShipTuning::from_json(&edited("\"landing_slope_limit\": 35.0", &format!("\"landing_slope_limit\": {bad}"))).unwrap_err();
        assert!(e.contains("landing_slope_limit"), "{e}");
    }
}

/// #106 point 5: times, accelerations and speeds the step divides by or scales with must be
/// positive and finite; the rest at least finite and not negative.
#[test]
fn rejects_zero_negative_or_non_finite_values() {
    let positive = [
        "thrust_accel", "boost_factor", "turn_rate", "roll_rate", "assisted_accel", "assisted_braking", "assisted_boost_accel",
        "assisted_acceleration_time", "assisted_braking_time", "release_braking", "release_braking_time", "velocity_response_time",
        "thrust_response_time", "assisted_reverse_speed", "assisted_strafe_speed", "assisted_vertical_speed", "assisted_boost_speed_factor",
    ];
    let not_negative = ["drag_k", "landing_sink_factor", "linear_ramp_time", "angular_ramp_time", "decouple_time"];
    let value = |name: &str| {
        let at = SHIP.find(&format!("\"{name}\": ")).unwrap_or_else(|| panic!("{name} not in ship.json"));
        let rest = &SHIP[at..];
        rest[..rest.find(',').unwrap()].to_string()
    };
    for name in positive.iter().chain(&not_negative) {
        let bad: &[&str] = if positive.contains(name) { &["0.0", "-1.0"] } else { &["-1.0"] };
        for b in bad {
            let e = ShipTuning::from_json(&edited(&value(name), &format!("\"{name}\": {b}"))).unwrap_err();
            assert!(e.contains(name), "{name} = {b}: {e}");
        }
    }
}

/// JSON has no NaN or infinity, but tuning built in code goes through the same check.
#[test]
fn rejects_non_finite_values_built_in_code() {
    let t = ShipTuning { drag_k: f64::NAN, ..ShipTuning::default() };
    assert!(t.validate().unwrap_err().contains("drag_k"));
    let t = ShipTuning { velocity_response_time: f64::INFINITY, ..ShipTuning::default() };
    assert!(t.validate().unwrap_err().contains("velocity_response_time"));
}
