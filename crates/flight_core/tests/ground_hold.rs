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
        let got = GroundRules::default().ground_slope(&env, env.ground(0.0)).to_degrees();
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

/// Settles a ship onto the slope with Ctrl held from just above the ground, falling at 1 m/s; returns
/// how far it moved sideways from the touchdown spot.
fn settle(c: &mut GroundRules, env: &Slope, secs: f64, friction: f64) -> (f64, BodyState) {
    let up = env.ground(0.0).normalize();
    let mut b = BodyState { pos: env.ground(0.0) + up * 0.05, lin_vel: -up, ..Default::default() };
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
fn at_rest_it_returns_to_its_spot_until_thrust() {
    let env = Slope::new(20.0);
    let mut c = GroundRules::default();
    let (_, mut b) = settle(&mut c, &env, 3.0, 0.5);
    let rest = c.ground_hold.and_then(|h| h.rest).expect("at rest");
    b.pos += DVec3::new(0.03, 0.01, -0.02);
    let none = FlightInput { grounded: true, piloted: true, ..Default::default() };
    let (v, _) = c.step(&b, &none, &env, DT);
    assert!((b.pos + v * DT - rest).length() < 1e-9, "back to the spot: {v}");
    let (v, _) = c.step(&b, &FlightInput { grounded: false, ..down() }, &env, DT);
    assert!((b.pos + v * DT - rest).length() < 1e-9, "held without contact: {v}");
    // Thrust up lets go; the lift is the SC model's (the ground rules do not fly the ship).
    let mut up = none;
    up.thrust = DVec3::Y;
    c.step(&BodyState { pos: rest, ..b }, &up, &env, DT);
    assert!(c.ground_hold.is_none(), "thrust ends the hold");
}

#[test]
fn sideways_input_also_ends_the_hold() {
    let env = Slope::new(10.0);
    for thrust in [DVec3::X, DVec3::NEG_Z, DVec3::Z] {
        let mut c = GroundRules::default();
        let (_, b) = settle(&mut c, &env, 2.0, 0.5);
        assert!(c.ground_hold.is_some());
        c.step(&b, &FlightInput { thrust, grounded: true, piloted: true, ..Default::default() }, &env, DT);
        assert!(c.ground_hold.is_none(), "{thrust} ends the hold");
    }
}

#[test]
fn moved_far_away_or_released_lets_go() {
    let env = Slope::new(10.0);
    let mut c = GroundRules::default();
    let (_, b) = settle(&mut c, &env, 2.0, 0.5);
    assert!(c.ground_hold.is_some());
    // Teleported 40 m along the ground (a scenario does this): no pull back.
    let far = BodyState { pos: env.ground(40.0) + DVec3::Y * 20.0, lin_vel: DVec3::ZERO, ..b };
    let none = FlightInput { piloted: true, ..Default::default() };
    let (v, _) = c.step(&far, &none, &env, DT);
    assert!(c.ground_hold.is_none() && v.length() < 1.0, "lets go: {v}");
    settle(&mut c, &env, 2.0, 0.5);
    assert!(c.ground_hold.is_some());
    c.release();
    assert!(c.ground_hold.is_none(), "the SC model flying the ship (or gravity compensation off) lets go");
}

/// A settling hold whose contact stays lost lets go (the ground fell away); a
/// flicker of one step does not (see `at_rest_it_returns_to_its_spot_until_thrust`), and a resting
/// hold keeps the ship on its spot without contact (the physics drops a contact without push).
#[test]
fn contact_lost_while_settling_ends_the_hold() {
    let env = Slope::new(10.0);
    let mut c = GroundRules::default();
    let b = BodyState { pos: env.ground(0.0), ..Default::default() };
    c.step(&b, &down(), &env, DT);
    assert!(c.ground_hold.is_some_and(|h| h.rest.is_none()), "settling");
    let none = FlightInput { grounded: false, piloted: true, ..Default::default() };
    for _ in 0..(GroundRules::GROUND_HOLD_RELEASE_TIME / DT).ceil() as usize + 1 {
        c.step(&b, &none, &env, DT);
    }
    assert!(c.ground_hold.is_none(), "contact lost for {} s lets go", GroundRules::GROUND_HOLD_RELEASE_TIME);
    let mut c = GroundRules::default();
    let (_, b) = settle(&mut c, &env, 3.0, 0.5);
    assert!(c.ground_hold.and_then(|h| h.rest).is_some());
    for _ in 0..120 {
        c.step(&b, &none, &env, DT);
    }
    assert!(c.ground_hold.is_some(), "resting: held without contact");
}

#[test]
fn lifted_straight_up_it_is_not_pulled_back() {
    let env = Slope::new(10.0);
    let mut c = GroundRules::default();
    let (_, b) = settle(&mut c, &env, 3.0, 0.5);
    assert!(c.ground_hold.and_then(|h| h.rest).is_some());
    let lifted = BodyState { pos: b.pos + b.pos.normalize() * 20.0, lin_vel: DVec3::ZERO, ..b };
    let (v, _) = c.step(&lifted, &FlightInput { grounded: true, piloted: true, ..Default::default() }, &env, DT);
    assert!(c.ground_hold.is_none() && v.length() < 5.0, "lets go instead of 1200 m/s down: {v}");
}

#[test]
fn grounded_with_down_input_settles_without_sliding() {
    let env = Slope::new(0.0);
    let mut c = GroundRules::default();
    let b = BodyState { pos: env.ground(0.0), ..Default::default() };
    let up = b.pos.normalize();
    let mut v = DVec3::X * 1.6;
    for _ in 0..30 {
        v = c.step(&BodyState { lin_vel: v, ..b }, &down(), &env, DT).0;
    }
    assert!((v - up * v.dot(up)).length() < 1e-9, "no sideways speed on the ground: {v}");
    assert!(v.dot(up) < 0.0 && v.dot(up) > -0.6, "gentle settle: {v}");
    for _ in 0..120 {
        v = c.step(&BodyState { lin_vel: v, ..b }, &down(), &env, DT).0;
        v -= up * v.dot(up).min(0.0);
    }
    let pushed = c.step(&BodyState { lin_vel: v, ..b }, &down(), &env, DT).0;
    assert!(pushed.length() < 1e-3, "resting, no push: {pushed}");
}
