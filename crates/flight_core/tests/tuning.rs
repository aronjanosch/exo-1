//! Ground tuning as data (#20, #92): the shipped file, the parser's rejections and the curve.
use flight_core::{Curve, GroundTuning, Interp};
use glam::DVec2;

const SHIP: &str = include_str!("../../../content/tuning/ship.json");

#[test]
fn shipped_file_equals_default() {
    assert_eq!(GroundTuning::from_json(SHIP).unwrap(), GroundTuning::default());
}

fn edited(from: &str, to: &str) -> String {
    assert!(SHIP.contains(from), "fixture text {from:?} not in ship.json");
    SHIP.replacen(from, to, 1)
}

#[test]
fn rejects_unknown_field() {
    let e = GroundTuning::from_json(&edited("\"linear_decay\"", "\"linear_decay_x\": 1.0, \"linear_decay\"")).unwrap_err();
    assert!(e.contains("linear_decay_x"), "{e}");
}

#[test]
fn rejects_missing_field() {
    let e = GroundTuning::from_json(&edited("\"linear_decay\": 3.0,", "")).unwrap_err();
    assert!(e.contains("linear_decay"), "{e}");
}

#[test]
fn rejects_curve_with_one_point() {
    let e = GroundTuning::from_json(&edited("[[0, 0.85], [0.5, 1.0], [1.0, 0.8]]", "[[0, 0.85]]")).unwrap_err();
    assert!(e.contains("rate_over_speed") && e.contains("2 points"), "{e}");
}

#[test]
fn rejects_unsorted_curve() {
    let e = GroundTuning::from_json(&edited("[[0, 0.85], [0.5, 1.0], [1.0, 0.8]]", "[[1, 0.85], [0.5, 1.0], [0, 0.8]]")).unwrap_err();
    assert!(e.contains("rate_over_speed") && e.contains("ascending"), "{e}");
}

#[test]
fn rejects_non_string_comment() {
    let e = GroundTuning::from_json(&SHIP.replacen("\"_comment\": \"", "\"_comment\": 1, \"x\": \"", 1)).unwrap_err();
    assert!(e.contains("_comment"), "{e}");
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
        let e = GroundTuning::from_json(&edited("\"landing_slope_limit\": 35.0", &format!("\"landing_slope_limit\": {bad}"))).unwrap_err();
        assert!(e.contains("landing_slope_limit"), "{e}");
    }
}

/// #106 point 5: speeds and decays the step divides by or scales with must be positive and finite.
#[test]
fn rejects_zero_or_negative_values() {
    let positive = ["cruise_speed", "linear_decay", "angular_decay"];
    let not_negative: [&str; 0] = [];
    let value = |name: &str| {
        let at = SHIP.find(&format!("\"{name}\": ")).unwrap_or_else(|| panic!("{name} not in ship.json"));
        let rest = &SHIP[at..];
        rest[..rest.find(',').unwrap()].to_string()
    };
    for name in positive.iter().chain(&not_negative) {
        let bad: &[&str] = if positive.contains(name) { &["0.0", "-1.0"] } else { &["-1.0"] };
        for b in bad {
            let e = GroundTuning::from_json(&edited(&value(name), &format!("\"{name}\": {b}"))).unwrap_err();
            assert!(e.contains(name), "{name} = {b}: {e}");
        }
    }
}

/// JSON has no NaN or infinity, but tuning built in code goes through the same check.
#[test]
fn rejects_non_finite_values_built_in_code() {
    let t = GroundTuning { linear_decay: f64::NAN, ..GroundTuning::default() };
    assert!(t.validate().unwrap_err().contains("linear_decay"));
    let t = GroundTuning { angular_decay: f64::INFINITY, ..GroundTuning::default() };
    assert!(t.validate().unwrap_err().contains("angular_decay"));
}
