mod sc_common;
use flight_core::FlightInput;
use flight_core::sc::{Master, ModeCmds, ScShip};
use glam::{DVec2, DVec3};
use sc_common::*;

#[test]
fn g_safe_caps_main_diagonal_and_boost_thrust_at_four_total_g() {
    for stick in [DVec3::NEG_Z, DVec3::new(1.0, 1.0, -1.0).normalize()] {
        let mut ship = ScShip::new(tuning());
        ship.modes.master = Master::Nav;
        let mut body = body_at(DVec3::ZERO);
        let input = FlightInput {
            thrust: stick,
            boost: true,
            piloted: true,
            ..Default::default()
        };
        let mut peak: f64 = 0.0;
        for _ in 0..120 {
            let out = ship.step(&body, &input, &ModeCmds::default(), &Space::default(), DT);
            let g = out.force.length() / ship.mass() / 9.81;
            peak = peak.max(g);
            assert!(g <= 4.0 + 1e-9, "stick {stick:?}: engine load {g:.3} g");
            body.lin_vel = out.lin_vel;
            body.ang_vel = out.ang_vel;
            body.integrate(DT);
        }
        assert!(
            peak > 3.9,
            "main/combined thrust reaches the 4 g ceiling: {peak:.3}"
        );
    }
}

#[test]
fn gravity_is_not_added_to_the_g_readout_but_hover_thrust_is_counted() {
    let env = Air {
        density: 0.0,
        ..Default::default()
    };
    let mut ship = ScShip::new(tuning());
    ship.modes.coupled = false;
    ship.modes.coupling = 0.0;
    ship.modes.grav_comp = false;
    let mut body = body_at(DVec3::Y * 400.0);
    let input = FlightInput {
        piloted: true,
        ..Default::default()
    };
    fly(
        &mut ship,
        &mut body,
        &input,
        &ModeCmds::default(),
        &env,
        1.0,
    );
    assert!(body.lin_vel.y < -9.0);
    assert!(
        ship.status.felt_g < 1e-9,
        "free fall does not add planetary gravity to felt g"
    );
    ship.modes.grav_comp = true;
    body.lin_vel = DVec3::ZERO;
    fly(
        &mut ship,
        &mut body,
        &input,
        &ModeCmds::default(),
        &env,
        1.0,
    );
    assert!(
        (ship.status.felt_g - 1.0).abs() < 0.01,
        "hover support is thruster force, not direct gravity"
    );
}

#[test]
fn enabling_g_safe_also_limits_existing_boost_output_immediately() {
    let mut ship = ScShip::new(tuning());
    ship.modes.g_safe = false;
    ship.modes.master = Master::Nav;
    let mut body = body_at(DVec3::ZERO);
    let input = FlightInput {
        thrust: DVec3::NEG_Z,
        boost: true,
        piloted: true,
        ..Default::default()
    };
    fly(
        &mut ship,
        &mut body,
        &input,
        &ModeCmds::default(),
        &Space::default(),
        1.0,
    );
    assert!(ship.status.felt_g > 4.0);
    let out = ship.step(
        &body,
        &input,
        &ModeCmds {
            g_safe: true,
            ..Default::default()
        },
        &Space::default(),
        DT,
    );
    assert!(out.force.length() / ship.mass() / 9.81 <= 4.0 + 1e-9);
}

#[test]
fn looping_does_not_pause_rotation_while_pointing_down() {
    let mut failures = Vec::new();
    for height in [400.0, 10000.0] {
        for proximity in [false, true] {
            for anti_drift in [true, false] {
                let mut ship = ScShip::new(tuning());
                ship.tuning.linear.anti_drift = anti_drift;
                ship.modes.proximity = proximity;
                let mut body = body_at(DVec3::Y * height);
                body.lin_vel = DVec3::NEG_Z * 90.0;
                let env = Air::default();
                let input = FlightInput {
                    thrust: DVec3::NEG_Z,
                    turn: DVec2::X,
                    piloted: true,
                    ..Default::default()
                };
                let mut angle: f64 = 0.0;
                let mut down_min: f64 = f64::INFINITY;
                let mut samples = 0;
                for _ in 0..1800 {
                    let out = ship.step(&body, &input, &ModeCmds::default(), &env, DT);
                    let pitch = (body.rot.inverse() * out.ang_vel).x;
                    angle += pitch * DT;
                    let phase = angle.to_degrees().rem_euclid(360.0);
                    if angle.to_degrees() > 180.0 && (225.0..315.0).contains(&phase) {
                        down_min = down_min.min(pitch);
                        samples += 1;
                    }
                    body.lin_vel = out.lin_vel;
                    body.ang_vel = out.ang_vel;
                    body.integrate(DT);
                }
                eprintln!(
                    "looping: height {height}, proximity {proximity}, anti_drift {anti_drift}, rotated {:.1} degrees, downward min pitch {down_min:.4} rad/s ({samples} ticks)",
                    angle.to_degrees()
                );
                if samples == 0 || down_min < 0.1 {
                    failures.push((height, proximity, down_min, samples));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "rotation pauses in descending quadrant: {failures:?}"
    );
}

#[test]
fn anti_drift_does_not_drop_curve_thrust_when_the_hold_opposes_the_turn() {
    use flight_core::limits::Dirs;
    use flight_core::sc::{Modes, StepState, linear};
    use glam::DQuat;
    let t = tuning().linear;
    let env = Air {
        density: 0.0,
        ..Default::default()
    };
    let input = FlightInput {
        thrust: DVec3::NEG_Z * 0.6,
        piloted: true,
        ..Default::default()
    };
    let modes = Modes {
        g_safe: false,
        proximity: false,
        ..Default::default()
    };
    let e = linear::Env {
        thrust_box: Dirs::splat(20.0),
        boost: 0.0,
        braking: false,
        air_accel: DVec3::ZERO,
        passive_drag: DVec3::ZERO,
        cap_scale: 1.0,
        drift: DVec3::ZERO,
    };
    // Inverted: gravity is +Y in ship space, so its compensation opposes +Y turning thrust.
    let cross_at_limit = (20.0 + env.g) / t.decay;
    let mut outputs = Vec::new();
    for offset in [-0.001, 0.001] {
        let mut body = body_at(DVec3::Y * 10000.0);
        body.rot = DQuat::from_rotation_z(std::f64::consts::PI);
        body.lin_vel = body.rot * DVec3::new(0.0, -(cross_at_limit + offset), -90.0);
        let f = StepState::new(&body, &env, DT);
        outputs.push(
            linear::step(
                &mut linear::LinearState::default(),
                &f,
                &input,
                &modes,
                &e,
                &t,
            )
            .accel
            .y,
        );
    }
    eprintln!(
        "curve thrust across saturation: {:.3} -> {:.3} m/s²",
        outputs[0], outputs[1]
    );
    assert!(
        outputs[1] >= outputs[0] - 0.1,
        "anti-drift dropped available curve thrust: {outputs:?}"
    );
    assert!(outputs[1] <= 20.0 + 1e-9);
}
