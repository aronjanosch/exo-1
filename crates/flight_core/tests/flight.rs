//! The planetary field (gravity, influence by altitude) on the game's planet model, and the cabin's
//! LAG state (#110). The controller's own checks are the SC model's (`sc_*.rs`) and the ground rules'
//! (`ground_hold.rs`).
use flight_core::*;
use glam::DVec3;

fn check(ok: bool, msg: String) {
    assert!(ok, "{msg}");
}

/// The game's planet model (centre, radius 5000, default field).
struct MainModel {
    centre: DVec3,
    field: Field,
}

impl PlanetEnv for MainModel {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world - self.centre
    }
    fn radius(&self) -> f64 {
        5000.0
    }
    fn height_at(&self, _dir: DVec3) -> f64 {
        0.0
    }
    fn field(&self) -> &Field {
        &self.field
    }
}

#[test]
fn field_gravity_and_boundaries() {
    let mut model = MainModel { centre: DVec3::ZERO, field: Field::default() };
    for altitude in [-10.0, 0.0, 600.0, 1200.0] {
        let g = model.gravity_at(DVec3::Y * (5000.0 + altitude));
        check(g.distance(DVec3::NEG_Y * 9.81) < 0.0001, format!("full 9.81 m/s² gravity at {altitude:.0} m"));
    }
    check((model.field_strength_at(DVec3::Y * 8600.0) - 0.5).abs() < 0.0001, "half planetary influence at 3600 m".into());
    for boundary in [1200.0, 6000.0] {
        let below = model.field_strength_at(DVec3::Y * (5000.0 + boundary - 1.0));
        let above = model.field_strength_at(DVec3::Y * (5000.0 + boundary + 1.0));
        check((below - above).abs() < 0.00001, format!("smooth field boundary at {boundary:.0} m"));
    }
    for altitude in [6000.0, 7000.0, 20000.0] {
        let g = model.gravity_at(DVec3::Y * (5000.0 + altitude));
        check(g.abs().max_element() < 1e-5, format!("zero gravity at {altitude:.0} m"));
    }
    let sample = DVec3::Y * 8600.0;
    let expected = model.gravity_at(sample);
    model.centre = DVec3::new(-10000.0, 2000.0, 10000.0);
    check(
        model.gravity_at(sample + model.centre).distance(expected) < 0.0001,
        "field and gravity are invariant under origin shifts".into(),
    );
}

/// LAG: off while landed, comes up over 1 s after take-off, goes down after landing; G only
/// works while landed.
#[test]
fn lag_follows_landing_and_the_manual_switch() {
    let dt = 1.0 / 60.0;
    let mut lag = Lag::default();
    let run = |lag: &mut Lag, grounded: bool, clearance: f64, speed: f64, secs: f64| {
        for _ in 0..(secs / dt).round() as usize {
            lag.step(grounded, clearance, speed, dt);
        }
    };
    run(&mut lag, true, 0.1, 0.0, 2.0);
    assert_eq!((lag.landed, lag.level), (true, 0.0));
    lag.toggle();
    run(&mut lag, true, 0.1, 0.0, 1.0);
    assert!(lag.manual_on && lag.level == 1.0, "G switches it on while landed");
    lag.toggle();
    run(&mut lag, true, 0.1, 0.0, 1.0);
    assert_eq!(lag.level, 0.0, "and off again");
    // Lifting off low is still landed (hysteresis), above 2 m the ship flies.
    run(&mut lag, false, 1.8, 3.0, 1.0);
    assert!(lag.landed);
    run(&mut lag, false, 2.5, 3.0, 0.5);
    assert!(!lag.landed && (lag.level - 0.5).abs() < 0.02, "half way after 0.5 s: {}", lag.level);
    run(&mut lag, false, 2.5, 3.0, 0.6);
    assert_eq!(lag.level, 1.0);
    lag.toggle();
    assert!(lag.is_on() && !lag.manual_on, "G does nothing in flight");
    // #110 point 1: hovering still and low without contact is flying.
    run(&mut lag, false, 1.4, 0.0, 1.0);
    assert!(!lag.landed && lag.level == 1.0, "hovering at 1.4 m is not landed");
    // Touching down slowly: landed, the field goes down.
    run(&mut lag, true, 1.0, 0.1, 1.1);
    assert_eq!((lag.landed, lag.level), (true, 0.0));
    // Parked with its centre over a dip deeper than 2 m: the hull touches, still landed.
    let mut dip = Lag { landed: false, level: 1.0, ..Lag::default() };
    run(&mut dip, true, 3.0, 0.0, 1.1);
    assert_eq!((dip.landed, dip.level), (true, 0.0), "contact counts, not the clearance under the centre");
    // Mix: half way the direction is half turned, the strength stays.
    let half = Lag { level: 0.5, ..Lag::default() };
    let g = half.gravity(DVec3::X, DVec3::new(0.0, -9.81, 0.0));
    let d = 9.81 / 2f64.sqrt();
    assert!((g - DVec3::new(-d, -d, 0.0)).length() < 1e-9, "{g:?}");
    // Upside down half way: still full strength, not cancelled.
    let g = half.gravity(DVec3::NEG_Y, DVec3::new(0.0, -9.81, 0.0));
    assert!((g.length() - 9.81).abs() < 1e-9, "{g:?}");
    assert_eq!(Lag::full().level, 1.0);
}
