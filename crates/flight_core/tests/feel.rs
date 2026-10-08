//! Sprint 2 feel: input ramp (#25), decoupled damping (#26), virtual stick.
use flight_core::*;
use glam::{DQuat, DVec2, DVec3};

const DT: f64 = 1.0 / 60.0;

/// No planet influence: deep space with a far-away planet.
struct Space(Field);
impl PlanetEnv for Space {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world + DVec3::Y * 1e7
    }
    fn radius(&self) -> f64 {
        5000.0
    }
    fn height_at(&self, _dir: DVec3) -> f64 {
        0.0
    }
    fn field(&self) -> &Field {
        &self.0
    }
}

fn piloted(thrust: DVec3) -> FlightInput {
    FlightInput { thrust, piloted: true, ..Default::default() }
}

#[test]
fn ramp_reaches_full_deflection_after_the_ramp_time() {
    let t = ShipTuning::default();
    for (time, input, idx) in [(t.linear_ramp_time, piloted(DVec3::NEG_Z), 2), (t.angular_ramp_time, FlightInput { roll: 1.0, piloted: true, ..Default::default() }, 3)] {
        let mut r = InputRamp::default();
        let mut ticks = 0;
        loop {
            ticks += 1;
            r.apply(&input, &t, DT);
            if r.out[idx].abs() >= 1.0 {
                break;
            }
            assert!(r.out[idx].abs() >= t.ramp_curve.eval(0.0) * 0.999, "starts at the curve's floor");
        }
        assert_eq!(ticks, (time / DT).round() as i32, "full after {time} s");
        // Release starts over.
        r.apply(&FlightInput { piloted: true, ..Default::default() }, &t, DT);
        assert_eq!(r.out[idx], 0.0);
    }
}

#[test]
fn ramp_reversal_starts_over_and_small_input_is_not_raised() {
    let t = ShipTuning::default();
    let mut r = InputRamp::default();
    for _ in 0..60 {
        r.apply(&piloted(DVec3::X), &t, DT);
    }
    assert_eq!(r.out[0], 1.0);
    r.apply(&piloted(DVec3::NEG_X), &t, DT);
    assert!(r.out[0] < 0.0 && r.out[0] > -0.5, "reversal ramps again: {}", r.out[0]);
    for _ in 0..60 {
        r.apply(&piloted(DVec3::X * 0.1), &t, DT);
    }
    assert!((r.out[0] - 0.1).abs() < 1e-12);
}

#[test]
fn stick_deflection() {
    let mut s = VirtualStick::default();
    let (dz, max) = (0.02, 0.2);
    s.push(DVec2::new(0.01, 0.0), max);
    assert_eq!(s.deflection(dz, max, None), DVec2::ZERO, "inside the dead zone");
    s.push(DVec2::new(0.1, 0.0), max);
    let d = s.deflection(dz, max, None);
    assert!((d.y + (0.11 - dz) / (max - dz)).abs() < 1e-12, "mouse right yaws right: {d}");
    s.push(DVec2::new(5.0, 0.0), max);
    assert!((s.offset.length() - max).abs() < 1e-12, "offset capped at the max angle");
    assert!((s.deflection(dz, max, None).y + 1.0).abs() < 1e-12);
    let mut up = VirtualStick::default();
    up.push(DVec2::new(0.0, -0.2), max);
    assert!(up.deflection(dz, max, None).x > 0.99, "mouse up lifts the nose");
}

#[test]
fn decoupled_keeps_gliding_and_coupled_brakes() {
    let env = Space(Field::default());
    // `level0`: coupling at the start (0 = the blend is already done).
    let run = |coupled: bool, level0: f64| {
        let mut c = ShipController::default();
        c.coupled = coupled;
        c.coupling = level0;
        let mut b = BodyState { lin_vel: DVec3::NEG_Z * 60.0, rot: DQuat::IDENTITY, ..Default::default() };
        let mut levels = Vec::new();
        for i in 0..(6.0 / DT) as usize {
            let (v, w) = c.step(&b, &piloted(DVec3::ZERO), &env, DT);
            b.lin_vel = v;
            b.ang_vel = w;
            b.integrate(DT);
            if i == (2.0 / DT) as usize - 1 {
                levels.push(c.coupling);
            }
        }
        (b.lin_vel.length(), levels, c.coupling)
    };
    let (coupled_speed, _, level) = run(true, 1.0);
    assert_eq!(level, 1.0);
    assert!(coupled_speed < 30.0, "coupled release brakes: {coupled_speed}");
    let (_, half, level) = run(false, 1.0);
    assert!((half[0] - 0.5).abs() < 0.01, "half blended after 2 of 4 s: {}", half[0]);
    assert_eq!(level, 0.0);
    let (glide, _, _) = run(false, 0.0);
    assert!((glide - 60.0).abs() < 1e-6, "decoupled keeps gliding: {glide} m/s");
}

#[test]
fn decoupled_thrust_only_along_input() {
    let env = Space(Field::default());
    let mut c = ShipController::default();
    c.coupled = false;
    c.coupling = 0.0;
    // Sliding sideways at 20 m/s, pushing forward: the sideways speed stays.
    let mut b = BodyState { lin_vel: DVec3::X * 20.0, ..Default::default() };
    for _ in 0..120 {
        let (v, _) = c.step(&b, &piloted(DVec3::NEG_Z), &env, DT);
        b.lin_vel = v;
        b.integrate(DT);
    }
    assert!((b.lin_vel.x - 20.0).abs() < 1e-9, "no damping across the input: {}", b.lin_vel);
    assert!(b.lin_vel.z < -20.0, "thrust forward: {}", b.lin_vel);
}
