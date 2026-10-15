mod sc_common;
use flight_core::FlightInput;
use flight_core::sc::{Master, ModeCmds, ScShip};
use glam::{DVec2, DVec3};
use sc_common::*;

fn requested_rate(speed: f64, turn: DVec2, hold: DVec3, coupling: f64, g_safe: bool) -> DVec3 {
    use flight_core::limits::{Dirs, Rot};
    use flight_core::sc::{StepState, angular};
    let mut body = body_at(DVec3::ZERO);
    body.lin_vel = DVec3::NEG_Z * speed;
    let f = StepState::new(&body, &Space::default(), DT);
    let modes = flight_core::sc::Modes {
        coupling,
        g_safe,
        ..Default::default()
    };
    let env = angular::Env {
        accel_box: Rot {
            pitch: 1e6,
            yaw: 1e6,
            roll: 1e6,
        },
        boost: 0.0,
        cap: 150.0,
        thrust_box: Dirs {
            up: 100.0,
            down: 50.0,
            left: 100.0,
            right: 100.0,
            forward: 100.0,
            backward: 100.0,
        },
        linear_hold: hold,
        air_hold: DVec3::ZERO,
        level: false,
    };
    let t = tuning().angular;
    let input = FlightInput {
        turn,
        piloted: true,
        ..Default::default()
    };
    let out = angular::step(
        &mut angular::AngularState::default(),
        &f,
        &input,
        &modes,
        &env,
        &t,
    );
    DVec3::new(
        out.accel.x / (DT * t.natural_frequency.pitch.powi(2)),
        out.accel.y / (DT * t.natural_frequency.yaw.powi(2)),
        0.0,
    )
}

#[test]
fn turn_budget_accounts_for_speed_direction_and_gravity_hold() {
    let pitch = requested_rate(200.0, DVec2::X, DVec3::ZERO, 1.0, false).x;
    let faster = requested_rate(400.0, DVec2::X, DVec3::ZERO, 1.0, false).x;
    assert!((pitch - 100.0 / 200.0).abs() < 1e-9);
    assert!((faster - pitch / 2.0).abs() < 1e-9);
    let down = requested_rate(200.0, -DVec2::X, DVec3::ZERO, 1.0, false).x;
    assert!((down + 50.0 / 200.0).abs() < 1e-9);
    let holding = requested_rate(200.0, DVec2::X, DVec3::Y * 9.81, 1.0, false).x;
    assert!((holding - (100.0 - 9.81) / 200.0).abs() < 1e-9);
}

#[test]
fn g_safe_is_an_additional_limit_not_a_prerequisite_for_coordination() {
    let free = requested_rate(200.0, DVec2::Y, DVec3::ZERO, 1.0, false).y;
    let safe = requested_rate(200.0, DVec2::Y, DVec3::ZERO, 1.0, true).y;
    assert!((free - 100.0 / 200.0).abs() < 1e-9);
    assert!((safe - 4.0 * 9.81 / 200.0).abs() < 1e-9);
    let decoupled = requested_rate(200.0, DVec2::Y, DVec3::ZERO, 0.0, true).y;
    assert!(decoupled > free);
}

#[test]
fn coupled_full_yaw_does_not_leave_the_flight_path_behind() {
    let mut ship = ScShip::new(tuning());
    ship.modes.g_safe = false;
    ship.modes.proximity = false;
    ship.modes.master = Master::Nav;
    let mut body = body_at(DVec3::ZERO);
    body.lin_vel = DVec3::NEG_Z * ship.tuning.linear.nav.cruise;
    let input = FlightInput {
        thrust: DVec3::NEG_Z,
        turn: DVec2::new(0.0, 1.0),
        piloted: true,
        ..Default::default()
    };
    let env = Space::default();
    let mut worst: f64 = 0.0;
    for _ in 0..600 {
        fly(&mut ship, &mut body, &input, &ModeCmds::default(), &env, DT);
        worst = worst.max(
            (body.rot * DVec3::NEG_Z)
                .angle_between(body.lin_vel)
                .to_degrees(),
        );
    }
    eprintln!("coupled full yaw: maximum nose/path angle {worst:.2} degrees");
    // A playtest starting criterion, not a settled flight-balancing constant.
    assert!(
        worst < 15.0,
        "nose left the flight path: {worst:.2} degrees"
    );
}

#[test]
fn coupled_pitch_yaw_and_diagonal_stay_coordinated_with_and_without_g_safe() {
    for master in [Master::Scm, Master::Nav] {
        for g_safe in [false, true] {
            for turn in [DVec2::X, -DVec2::X, DVec2::Y, -DVec2::Y, DVec2::ONE] {
                let mut ship = ScShip::new(tuning());
                ship.modes.master = master;
                ship.modes.g_safe = g_safe;
                // Deep-space fixture has no physical ground to avoid.
                ship.modes.proximity = false;
                let mut body = body_at(DVec3::ZERO);
                body.lin_vel = DVec3::NEG_Z * ship.tuning.linear.caps(master).cruise;
                let input = FlightInput {
                    thrust: DVec3::NEG_Z,
                    turn,
                    piloted: true,
                    ..Default::default()
                };
                let mut worst: f64 = 0.0;
                for _ in 0..600 {
                    fly(
                        &mut ship,
                        &mut body,
                        &input,
                        &ModeCmds::default(),
                        &Space::default(),
                        DT,
                    );
                    worst = worst.max(
                        (body.rot * DVec3::NEG_Z)
                            .angle_between(body.lin_vel)
                            .to_degrees(),
                    );
                }
                assert!(
                    worst < 15.0,
                    "{master:?}, G-safe {g_safe}, turn {turn:?}: nose/path {worst:.2} degrees; local velocity {:?}, spin {:?}, g {}",
                    body.rot.inverse() * body.lin_vel,
                    body.rot.inverse() * body.ang_vel,
                    ship.status.felt_g
                );
            }
        }
    }
}

#[test]
fn decoupled_still_rotates_independently_of_the_flight_path() {
    let mut ship = ScShip::new(tuning());
    ship.modes.coupled = false;
    ship.modes.coupling = 0.0;
    let mut body = body_at(DVec3::ZERO);
    body.lin_vel = DVec3::NEG_Z * 150.0;
    let input = FlightInput {
        turn: DVec2::Y,
        piloted: true,
        ..Default::default()
    };
    fly(
        &mut ship,
        &mut body,
        &input,
        &ModeCmds::default(),
        &Space::default(),
        2.0,
    );
    assert!((body.lin_vel - DVec3::NEG_Z * 150.0).length() < 1e-9);
    assert!(
        (body.rot * DVec3::NEG_Z)
            .angle_between(body.lin_vel)
            .to_degrees()
            > 60.0
    );
    assert!(!ship.status.rate_capped);
}

#[test]
fn coupled_turns_remain_coordinated_with_boost_cargo_and_gravity() {
    for boost in [false, true] {
        for g_safe in [false, true] {
            let mut ship = ScShip::new(tuning());
            ship.modes.g_safe = g_safe;
            ship.set_cargo_mass(ship.mass() * 0.5);
            let mut body = body_at(DVec3::Y * 10000.0);
            body.lin_vel = DVec3::NEG_Z * 150.0;
            // Isolate the gravity hold; aerodynamic behavior has its own tests.
            let env = Air {
                density: 0.0,
                ..Default::default()
            };
            let input = FlightInput {
                thrust: DVec3::NEG_Z,
                turn: DVec2::ONE,
                boost,
                piloted: true,
                ..Default::default()
            };
            let mut worst: f64 = 0.0;
            for _ in 0..600 {
                fly(&mut ship, &mut body, &input, &ModeCmds::default(), &env, DT);
                worst = worst.max(
                    (body.rot * DVec3::NEG_Z)
                        .angle_between(body.lin_vel)
                        .to_degrees(),
                );
            }
            assert!(
                worst < 15.0,
                "boost {boost}, G-safe {g_safe}, cargo/gravity: angle {worst:.2} degrees"
            );
        }
    }
}

#[test]
fn low_speed_limiter_does_not_remove_automatic_hover() {
    for limiter in [0.1, 0.2, 1.0] {
        for turn in [DVec2::ZERO, DVec2::X, DVec2::Y, DVec2::ONE] {
            let mut ship = ScShip::new(tuning());
            ship.modes.limiter = limiter;
            let mut body = body_at(DVec3::Y * 400.0);
            let env = Air::default();
            let input = FlightInput {
                turn,
                piloted: true,
                ..Default::default()
            };
            fly(
                &mut ship,
                &mut body,
                &input,
                &ModeCmds::default(),
                &env,
                10.0,
            );
            let fall = 400.0 - body.pos.y;
            eprintln!("hover: limiter {limiter}, turn {turn:?}, height loss {fall:.2} m");
            assert!(
                fall < 2.0,
                "limiter {limiter}, turn {turn:?}: fell {fall:.2} m"
            );
        }
    }
}

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
fn sustained_yaw_uses_the_available_load_instead_of_halving_it() {
    for density in [0.0, 1.0] {
        let mut ship = ScShip::new(tuning());
        let mut body = body_at(DVec3::Y * 10000.0);
        body.lin_vel = DVec3::NEG_Z * 90.0;
        let env = Air {
            density,
            ..Default::default()
        };
        let input = FlightInput {
            thrust: DVec3::NEG_Z,
            turn: DVec2::Y,
            piloted: true,
            ..Default::default()
        };
        fly(
            &mut ship,
            &mut body,
            &input,
            &ModeCmds::default(),
            &env,
            10.0,
        );
        let out = ship.step(&body, &input, &ModeCmds::default(), &env, DT);
        let local = body.rot.inverse() * out.force / ship.mass() / 9.81;
        eprintln!(
            "sustained yaw: density {density}, engine g {local:?}, total {:.2} g",
            ship.status.felt_g
        );
        assert!(ship.status.felt_g <= 4.0 + 1e-9);
        assert!(
            local.x.abs() > 3.0,
            "curve thrust is not unnecessarily halved: {local:?}"
        );
    }
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
fn measure_g_safe_effect_with_the_shipped_thrusters() {
    for density in [0.0, 1.0] {
        for safe in [false, true] {
            let mut ship = ScShip::new(tuning());
            ship.modes.g_safe = safe;
            let mut body = body_at(DVec3::Y * 10000.0);
            body.lin_vel = DVec3::NEG_Z * 90.0;
            let env = Air {
                density,
                ..Default::default()
            };
            let input = FlightInput {
                thrust: DVec3::NEG_Z,
                turn: DVec2::Y,
                piloted: true,
                ..Default::default()
            };
            fly(
                &mut ship,
                &mut body,
                &input,
                &ModeCmds::default(),
                &env,
                10.0,
            );
            let out = ship.step(&body, &input, &ModeCmds::default(), &env, DT);
            let local = body.rot.inverse() * out.force / ship.mass() / 9.81;
            eprintln!(
                "G-safe {safe}, density {density}: local thrust {local:?} g, total {:.2} g, yaw {:.3} rad/s",
                ship.status.felt_g,
                (body.rot.inverse() * body.ang_vel).y
            );
            if safe {
                assert!(ship.status.felt_g <= 4.0 + 1e-9);
            }
        }
    }
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
