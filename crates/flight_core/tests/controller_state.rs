//! Controller state that must not go stale: steps skipped during a quantum flight, assist off
//! (review 2026-10-09, #110 points 5 and 6).
use flight_core::*;
use glam::DVec3;

const DT: f64 = 1.0 / 60.0;
const R: f64 = 5000.0;

/// A smooth sphere of radius `R` at the origin.
struct Ball(Field);

impl PlanetEnv for Ball {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world
    }
    fn radius(&self) -> f64 {
        R
    }
    fn height_at(&self, _dir: DVec3) -> f64 {
        0.0
    }
    fn field(&self) -> &Field {
        &self.0
    }
}

/// #110 point 6: steps skipped while the drive holds the ship leave no planet-follow rate behind;
/// at rest afterwards the ship does not pitch.
#[test]
fn skipped_steps_leave_no_turn_behind() {
    let env = Ball(Field::default());
    let mut c = ShipController::default();
    let fast = BodyState { pos: DVec3::Y * (R + 200.0), lin_vel: DVec3::NEG_Z * 300.0, ..Default::default() };
    c.step(&fast, &FlightInput { piloted: true, ..Default::default() }, &env, DT);
    c.skip_step(DT);
    let rest = BodyState { lin_vel: DVec3::ZERO, ..fast };
    let (_, w) = c.step(&rest, &FlightInput { piloted: true, ..Default::default() }, &env, DT);
    // Only the follow rate of the little speed the step itself gives (stale: ~0.05 rad/s).
    assert!(w.length() < 1e-4, "no turn at rest after a skipped step: {w}");
}

/// The skipped step also lets the boost go and keeps the meter running.
#[test]
fn skipped_steps_release_the_boost() {
    let mut c = ShipController::default();
    c.boost_strength = 1.0;
    c.boost = BoostCapacitor::with_charge(0.5);
    c.boost.active = true;
    for _ in 0..(3.0 / DT) as usize {
        c.skip_step(DT);
    }
    assert!(!c.boost.active && c.boost_strength == 0.0);
    assert!(c.boost.charge > 0.5, "recharging: {}", c.boost.charge);
}

/// #110 point 5: with the hover assist off the clearance is still measured, the HUD limit is
/// zero and the settle timer starts over when the assist comes back.
#[test]
fn assist_off_keeps_the_readout_fresh() {
    let env = Ball(Field::default());
    let mut c = ShipController::default();
    let ground = BodyState { pos: DVec3::Y * R, ..Default::default() };
    let none = FlightInput { piloted: true, grounded: true, ..Default::default() };
    for _ in 0..60 {
        c.step(&ground, &none, &env, DT);
    }
    assert!(c.ground_time > 0.5);
    c.hover_assist = false;
    let high = BodyState { pos: DVec3::Y * (R + 300.0), ..Default::default() };
    c.step(&high, &FlightInput { grounded: false, ..none }, &env, DT);
    assert!((c.terrain_clearance - 300.0).abs() < 1e-6, "clearance {}", c.terrain_clearance);
    assert_eq!(c.forward_speed_limit, 0.0);
    assert_eq!(c.ground_time, 0.0);
}
