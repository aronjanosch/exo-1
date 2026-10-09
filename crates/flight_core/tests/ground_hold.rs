//! Ground hold (#92): a ship set down below the slope limit keeps its spot until thrust.
use flight_core::*;
use glam::DVec3;

const DT: f64 = 1.0 / 60.0;
const R: f64 = 5000.0;

/// A planet whose ground near the top (+Y) is a plane rising towards +X at `slope` radians.
struct Slope {
    slope: f64,
    field: Field,
}

impl Slope {
    fn new(deg: f64) -> Slope {
        Slope { slope: deg.to_radians(), field: Field::default() }
    }
    /// Normal of the plane at the top (planet frame, centre at the origin).
    fn normal(&self) -> DVec3 {
        DVec3::new(-self.slope.sin(), self.slope.cos(), 0.0)
    }
    /// A point on the ground at `x` metres along the slope's rise.
    fn ground(&self, x: f64) -> DVec3 {
        let dir = DVec3::new(x, R, 0.0).normalize();
        dir * (R + self.height_at(dir))
    }
}

impl PlanetEnv for Slope {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world
    }
    fn radius(&self) -> f64 {
        R
    }
    fn height_at(&self, dir: DVec3) -> f64 {
        // Where the ray along `dir` meets the plane through (0, R, 0).
        let t = R * self.normal().y / dir.dot(self.normal());
        t - R
    }
    fn field(&self) -> &Field {
        &self.field
    }
}

fn down() -> FlightInput {
    FlightInput { thrust: DVec3::NEG_Y, grounded: true, piloted: true, ..Default::default() }
}

fn sideways(v: DVec3, up: DVec3) -> DVec3 {
    v - up * v.dot(up)
}

#[test]
fn measures_the_slope_under_the_ship() {
    for deg in [0.0, 10.0, 25.0, 40.0] {
        let env = Slope::new(deg);
        let got = ShipController::default().ground_slope(&env, env.ground(0.0)).to_degrees();
        assert!((got - deg).abs() < 0.1, "{deg} deg: measured {got:.3}");
    }
}

/// A point hull on the plane, with what a contact does in one step: no push into the ground
/// (the rest of the push turns along it) and friction against the slide.
fn contact(env: &Slope, b: &mut BodyState, friction: f64) -> bool {
    let n = env.normal();
    let gap = (b.pos - env.ground(0.0)).dot(n);
    let into = b.lin_vel.dot(n) + gap.max(0.0) / DT;
    if into < 0.0 {
        b.lin_vel -= n * into;
        let along = b.lin_vel - n * b.lin_vel.dot(n);
        let l = along.length();
        if l > 0.0 {
            b.lin_vel -= along / l * l.min(-into * friction);
        }
    }
    b.integrate(DT);
    (b.pos - env.ground(0.0)).dot(n) < 1e-6
}

/// Settles a ship onto the slope with Ctrl held from just above the ground; returns how far it
/// moved sideways from the touchdown spot.
fn settle(c: &mut ShipController, env: &Slope, secs: f64, friction: f64) -> (f64, BodyState) {
    let mut b = BodyState { pos: env.ground(0.0) + DVec3::Y * 0.05, ..Default::default() };
    let mut grounded = false;
    let mut touch = None;
    let mut most: f64 = 0.0;
    for _ in 0..(secs / DT) as usize {
        let (v, _) = c.step(&b, &FlightInput { grounded, ..down() }, env, DT);
        b.lin_vel = v;
        grounded = contact(env, &mut b, friction);
        if grounded && touch.is_none() {
            touch = Some(b.pos);
        }
        if let Some(t) = touch {
            most = most.max(sideways(b.pos - t, b.pos.normalize()).length());
        }
    }
    (most, b)
}

#[test]
fn set_down_on_a_slope_it_keeps_the_touchdown_spot() {
    // 33 degrees, as `full` lands on, and a contact without friction: each step it turns part of
    // the settle push down the slope.
    let env = Slope::new(33.0);
    let mut held = ShipController::default();
    let (moved, b) = settle(&mut held, &env, 5.0, 0.0);
    let hold = held.ground_hold.expect("held");
    let rest = hold.rest.expect("at rest after 5 s");
    // Each step the contact pushes it a few millimetres down the slope; the hold takes it back.
    assert!(moved < 0.005, "moved {moved:.4} m while settling");
    assert!(sideways(rest - hold.at, rest.normalize()).length() < 0.005, "rests on the touchdown spot: {:?}", rest - hold.at);
    assert!((b.pos - rest).length() < 1e-9, "and stays there: {:?}", b.pos - rest);
    // Without the hold (limit 0) the same contact slides it down the slope until the push ends
    // (1.5 cm here; 0.55 m in `full`, where the hull tipped for 12 s and the push went on).
    let mut free = ShipController::new(ShipTuning { landing_slope_limit: 0.0, ..ShipTuning::default() });
    let (slid, _) = settle(&mut free, &env, 5.0, 0.0);
    assert!(free.ground_hold.is_none() && slid > 0.01, "without the hold it slides: {slid:.4} m");
}

#[test]
fn at_rest_it_returns_to_its_spot_until_thrust() {
    let env = Slope::new(20.0);
    let mut c = ShipController::default();
    let (_, mut b) = settle(&mut c, &env, 3.0, 0.5);
    let rest = c.ground_hold.and_then(|h| h.rest).expect("at rest");
    // A push moved it 3 cm (another contact, a bump): it goes back within one step.
    b.pos += DVec3::new(0.03, 0.01, -0.02);
    let none = FlightInput { grounded: true, piloted: true, ..Default::default() };
    let (v, _) = c.step(&b, &none, &env, DT);
    assert!((b.pos + v * DT - rest).length() < 1e-9, "back to the spot: {v}");
    // Contact lost (a flicker) and Ctrl held: still held.
    let (v, _) = c.step(&b, &FlightInput { grounded: false, ..down() }, &env, DT);
    assert!((b.pos + v * DT - rest).length() < 1e-9, "held without contact: {v}");
    // Thrust up lets go and lifts.
    let mut up = none;
    up.thrust = DVec3::Y;
    let mut b = BodyState { pos: rest, ..b };
    for _ in 0..60 {
        let (v, _) = c.step(&b, &up, &env, DT);
        b.lin_vel = v;
        b.integrate(DT);
    }
    assert!(c.ground_hold.is_none(), "thrust ends the hold");
    assert!((b.pos - rest).dot(rest.normalize()) > 0.5, "and lifts: {:?}", b.pos - rest);
}

#[test]
fn sideways_input_also_ends_the_hold() {
    let env = Slope::new(10.0);
    for thrust in [DVec3::X, DVec3::NEG_Z, DVec3::Z] {
        let mut c = ShipController::default();
        let (_, b) = settle(&mut c, &env, 2.0, 0.5);
        assert!(c.ground_hold.is_some());
        c.step(&b, &FlightInput { thrust, grounded: true, piloted: true, ..Default::default() }, &env, DT);
        assert!(c.ground_hold.is_none(), "{thrust} ends the hold");
    }
}

#[test]
fn steeper_than_the_limit_is_not_held() {
    let env = Slope::new(40.0);
    let mut c = ShipController::default();
    let b = BodyState { pos: env.ground(0.0), ..Default::default() };
    c.step(&b, &down(), &env, DT);
    assert!(c.ground_hold.is_none(), "40 degrees, limit {}", c.tuning.landing_slope_limit);
    let env = Slope::new(30.0);
    c.step(&b, &down(), &env, DT);
    assert!(c.ground_hold.is_some(), "30 degrees is held");
}

#[test]
fn moved_far_away_or_assist_off_lets_go() {
    let env = Slope::new(10.0);
    let mut c = ShipController::default();
    let (_, b) = settle(&mut c, &env, 2.0, 0.5);
    assert!(c.ground_hold.is_some());
    // Teleported 40 m along the ground (a scenario does this): no pull back.
    let far = BodyState { pos: env.ground(40.0) + DVec3::Y * 20.0, lin_vel: DVec3::ZERO, ..b };
    let none = FlightInput { piloted: true, ..Default::default() };
    let (v, _) = c.step(&far, &none, &env, DT);
    assert!(c.ground_hold.is_none() && v.length() < 1.0, "lets go: {v}");
    let (_, b) = settle(&mut c, &env, 2.0, 0.5);
    assert!(c.ground_hold.is_some());
    c.hover_assist = false;
    c.step(&b, &down(), &env, DT);
    assert!(c.ground_hold.is_none(), "assist off lets go");
}
