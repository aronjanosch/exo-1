//! Ship tuning as data (#20): the shipped file, the parser's rejections and the curve.
use flight_core::{Curve, Interp, ShipTuning};
use glam::DVec2;

const SHIP: &str = include_str!("../../../content/tuning/ship.json");

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
    let e = ShipTuning::from_json(&edited("[[0, 0.25], [1, 1]]", "[[0, 0.25]]")).unwrap_err();
    assert!(e.contains("ramp_curve") && e.contains("2 points"), "{e}");
}

#[test]
fn rejects_unsorted_curve() {
    let e = ShipTuning::from_json(&edited("[[0, 0.25], [1, 1]]", "[[1, 0.25], [0, 1]]")).unwrap_err();
    assert!(e.contains("ramp_curve") && e.contains("ascending"), "{e}");
}

#[test]
fn rejects_non_string_comment() {
    let e = ShipTuning::from_json(&SHIP.replacen("\"_comment\": \"", "\"_comment\": 1, \"x\": \"", 1)).unwrap_err();
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
        let e = ShipTuning::from_json(&edited("\"landing_slope_limit\": 35.0", &format!("\"landing_slope_limit\": {bad}"))).unwrap_err();
        assert!(e.contains("landing_slope_limit"), "{e}");
    }
}

/// #106 point 5: speeds and decays the step divides by or scales with must be positive and finite;
/// the rest at least finite and not negative.
#[test]
fn rejects_zero_negative_or_non_finite_values() {
    let positive = ["cruise_speed", "boost_speed_forward", "boost_speed_backward", "linear_decay", "angular_decay"];
    let not_negative = ["drag_k", "linear_ramp_time", "angular_ramp_time", "decouple_time"];
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
    let t = ShipTuning { angular_decay: f64::INFINITY, ..ShipTuning::default() };
    assert!(t.validate().unwrap_err().contains("angular_decay"));
}
