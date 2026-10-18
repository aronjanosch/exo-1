mod sc_common;
use flight_core::FlightInput;
use flight_core::sc::{Master, ModeCmds, ScShip};
use glam::{DVec2, DVec3};
use sc_common::*;

fn assert_flight_budget(ship: &ScShip, body: &flight_core::BodyState, out: &flight_core::sc::ScOut, env: &impl flight_core::PlanetEnv) {
    let thrust = body.rot.inverse() * out.force / ship.mass();
    let density = env.density_at(body.pos);
    let scale = 1.0 + (ship.tuning.air.thrust_share - 1.0) * density;
    let axes = ship.thrust_box(scale, 0.0);
    assert!(out.lin_vel.is_finite() && out.ang_vel.is_finite());
    assert!((axes.clamp(thrust) - thrust).length() < 1e-7, "thrust outside physical axes: {thrust:?}");
    assert!(thrust.length() <= flight_core::limits::G_SAFE_MAX_G * flight_core::limits::G0 + 1e-7,
        "combined thrust exceeds G-safe: {thrust:?}");
    assert!(ship.modes.g_safe && ship.modes.proximity && ship.tuning.linear.anti_drift);
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
fn accelerating_after_rotating_on_the_spot_does_not_pause_a_backward_loop() {
    let mut baseline = 0;
    for variant in ["baseline", "g_safe_off", "anti_drift_off", "grav_comp_off", "proximity_off", "gravity_zero", "density_zero", "vacuum", "vacuum_anti_drift_off"] {
        let mut ship = ScShip::new(tuning());
        let mut env = Air { density: 0.3, ..Default::default() };
        match variant {
            "g_safe_off" => ship.modes.g_safe = false,
            "anti_drift_off" => ship.tuning.linear.anti_drift = false,
            "grav_comp_off" => ship.modes.grav_comp = false,
            "proximity_off" => ship.modes.proximity = false,
            "gravity_zero" => env.g = 0.0,
            "density_zero" => env.density = 0.0,
            "vacuum" => { env.density = 0.0; env.g = 0.0; }
            "vacuum_anti_drift_off" => { env.density = 0.0; env.g = 0.0; ship.tuning.linear.anti_drift = false; }
            _ => {}
        }
        let mut body = body_at(DVec3::Y * 10000.0);
        let mut input = FlightInput { turn: DVec2::NEG_X, piloted: true, ..Default::default() };
        // Unlike the cruise-speed loop test, start by spinning at rest, then add thrust.
        fly(&mut ship, &mut body, &input, &ModeCmds::default(), &env, 1.0);
        input.thrust = DVec3::NEG_Z;
        let mut down_min = f64::INFINITY;
        let (mut pause_ticks, mut longest_pause) = (0, 0);
        let mut worst = (0.0, 0.0, 0.0, false);
        for tick in 0..480 {
            let out = ship.step(&body, &input, &ModeCmds::default(), &env, DT);
            let pitch = -(body.rot.inverse() * out.ang_vel).x;
            let nose_down = (body.rot * DVec3::NEG_Z).dot(DVec3::NEG_Y);
            if nose_down > 0.9 {
                down_min = down_min.min(pitch);
                pause_ticks = if pitch < 0.1 { pause_ticks + 1 } else { 0 };
                if pause_ticks > longest_pause {
                    longest_pause = pause_ticks;
                    worst = (tick as f64 * DT, body.lin_vel.length(), body.pos.y, ship.status.rate_capped);
                }
            } else {
                pause_ticks = 0;
            }
            body.lin_vel = out.lin_vel;
            body.ang_vel = out.ang_vel;
            body.integrate(DT);
        }
        eprintln!("{variant}: downward min {down_min:.5} rad/s, pause {:.3}s, (time, speed, height, rate_capped) {worst:?}", longest_pause as f64 * DT);
        if variant == "baseline" { baseline = longest_pause; }
    }
    assert!(baseline < 6, "looping pauses nose-down for {:.3}s after accelerating from rest", baseline as f64 * DT);
}

#[test]
fn sustained_throttle_and_pitch_do_not_lock_rotation_at_any_attitude() {
    let mut failures = Vec::new();
    for pitch_sign in [-1.0, 1.0] {
        for spin_seconds in [1.0, 3.0, 6.0] {
            for deflection in [0.3, 0.6, 1.0] {
                for (density, comstab) in [(0.0, true), (0.0, false), (0.3, true), (0.3, false), (1.0, true), (1.0, false)] {
                    let mut ship = ScShip::new(tuning());
                    ship.modes.comstab = comstab;
                    let env = Air { density, ..Default::default() };
                    let mut body = body_at(DVec3::Y * 10000.0);
                    let mut input = FlightInput { turn: DVec2::X * pitch_sign * deflection, piloted: true, ..Default::default() };
                    fly(&mut ship, &mut body, &input, &ModeCmds::default(), &env, spin_seconds);
                    input.thrust = DVec3::NEG_Z;
                    let (mut pause, mut longest_pause) = (0, 0);
                    let mut stalled = (0.0, 0.0, 0.0);
                    for tick in 0..3600 {
                        let out = ship.step(&body, &input, &ModeCmds::default(), &env, DT);
                        assert_flight_budget(&ship, &body, &out, &env);
                        let pitch = (body.rot.inverse() * out.ang_vel).x * pitch_sign;
                        pause = if tick > 120 && pitch < 0.1 * deflection && body.lin_vel.length() > 5.0 { pause + 1 } else { 0 };
                        if pause > longest_pause {
                            longest_pause = pause;
                            stalled = (tick as f64 * DT, body.lin_vel.length(), (body.rot * DVec3::NEG_Z).dot(DVec3::NEG_Y));
                        }
                        body.lin_vel = out.lin_vel;
                        body.ang_vel = out.ang_vel;
                        body.integrate(DT);
                        if body.pos.y < 50.0 { break; }
                    }
                    if longest_pause >= 30 {
                        if failures.len() < 8 {
                            eprintln!("rotation lock: sign {pitch_sign}, spin {spin_seconds}s, deflection {deflection}, density {density}, comstab {comstab}, pause {:.2}s, (time, speed, nose_down) {stalled:?}", longest_pause as f64 * DT);
                        }
                        failures.push((pitch_sign, spin_seconds, deflection, density, longest_pause));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "sustained throttle locks rotation in {} cases; first {:?}", failures.len(), failures.first());
}

#[test]
fn spherical_planet_loops_do_not_lock_rotation_with_held_throttle() {
    use flight_core::{Field, PlanetEnv};
    struct Planet(Field);
    impl PlanetEnv for Planet {
        fn to_planet(&self, world: DVec3) -> DVec3 { world }
        fn radius(&self) -> f64 { 5000.0 }
        fn height_at(&self, _: DVec3) -> f64 { 0.0 }
        fn field(&self) -> &Field { &self.0 }
    }
    let env = Planet(Field::default());
    let mut failures = Vec::new();
    for height in [200.0, 400.0, 800.0, 1500.0] {
        for sign in [-1.0, 1.0] {
            for spin in [1.0, 3.0, 6.0] {
                for deflection in [0.3, 0.6, 1.0] {
                    let mut ship = ScShip::new(tuning());
                    let mut body = body_at(DVec3::Y * (env.radius() + height));
                    let mut input = FlightInput { turn: DVec2::X * sign * deflection, piloted: true, ..Default::default() };
                    fly(&mut ship, &mut body, &input, &ModeCmds::default(), &env, spin);
                    input.thrust = DVec3::NEG_Z;
                    let (mut pause, mut longest_pause) = (0, 0);
                    let mut worst = (0.0, 0.0, 0.0);
                    for tick in 0..3600 {
                        let out = ship.step(&body, &input, &ModeCmds::default(), &env, DT);
                        assert_flight_budget(&ship, &body, &out, &env);
                        let pitch = (body.rot.inverse() * out.ang_vel).x * sign;
                        pause = if tick > 120 && pitch < 0.1 * deflection && body.lin_vel.length() > 5.0 { pause + 1 } else { 0 };
                        if pause > longest_pause {
                            longest_pause = pause;
                            worst = (tick as f64 * DT, body.lin_vel.length(), body.pos.length() - env.radius());
                        }
                        body.lin_vel = out.lin_vel;
                        body.ang_vel = out.ang_vel;
                        body.integrate(DT);
                        if body.pos.length() < env.radius() + 5.0 { break; }
                    }
                    if longest_pause >= 30 {
                        if failures.len() < 8 { eprintln!("spherical lock: height {height}, sign {sign}, spin {spin}, deflection {deflection}, pause {:.2}s, (time, speed, altitude) {worst:?}", longest_pause as f64 * DT); }
                        failures.push((height, sign, spin, deflection, longest_pause));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "spherical flight locks rotation in {} cases; first {:?}", failures.len(), failures.first());
}

#[test]
fn continuing_a_turn_uses_cross_thrust_before_additional_forward_thrust() {
    let mut ship = ScShip::new(instant());
    let mut body = body_at(DVec3::Y * 10000.0);
    body.lin_vel = DVec3::NEG_Z * 60.0;
    body.ang_vel = DVec3::NEG_X * 0.2;
    let input = FlightInput { thrust: DVec3::NEG_Z, turn: DVec2::NEG_X, piloted: true, ..Default::default() };
    let out = ship.step(&body, &input, &ModeCmds::default(), &Space::default(), DT);
    let acceleration = (out.lin_vel - body.lin_vel) / DT;
    let ongoing_curve = body.ang_vel.cross(body.lin_vel);
    assert!(acceleration.y <= ongoing_curve.y + 1e-8,
        "the ongoing loop must receive its curve thrust, not just a token share: {acceleration:?}, curve {ongoing_curve:?}");
    assert!(-acceleration.z < flight_core::limits::G_SAFE_MAX_G * 9.81 - 1e-6,
        "forward thrust must leave room for the loop: {acceleration:?}");
    assert!(acceleration.length() <= flight_core::limits::G_SAFE_MAX_G * 9.81 + 1e-8);
}

#[test]
fn automatic_braking_serves_the_ongoing_curve_before_longitudinal_deceleration() {
    use glam::DQuat;
    for speed_sign in [-1.0, 1.0] {
        for rot in [DQuat::IDENTITY, DQuat::from_rotation_y(0.7) * DQuat::from_rotation_z(0.4)] {
            let mut ship = ScShip::new(instant());
            let mut body = body_at(DVec3::Y * 10000.0);
            body.rot = rot;
            body.lin_vel = rot * DVec3::Z * (60.0 * speed_sign);
            body.ang_vel = rot * DVec3::NEG_X * 0.2;
            let input = FlightInput { turn: DVec2::NEG_X, piloted: true, ..Default::default() };
            let out = ship.step(&body, &input, &ModeCmds::default(), &Space::default(), DT);
            let acceleration = (out.lin_vel - body.lin_vel) / DT;
            let curve = body.ang_vel.cross(body.lin_vel);
            let delivered = acceleration.dot(curve.normalize());
            assert!(delivered >= curve.length() - 1e-8,
                "automatic braking starved the ongoing curve: delivered {delivered:.3}, required {:.3}, acceleration {acceleration:?}", curve.length());
            assert!(acceleration.dot(body.lin_vel) < 0.0, "the ship must still brake");
            assert!(acceleration.length() <= flight_core::limits::G_SAFE_MAX_G * flight_core::limits::G0 + 1e-8);
            assert!(!ship.status.braking, "this is gas release, not the separate brake");
        }
    }
}

#[test]
fn releasing_throttle_during_a_turn_keeps_delivering_curve_thrust() {
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::Y * 10000.0);
    body.lin_vel = DVec3::NEG_Z * 60.0;
    let mut input = FlightInput { thrust: DVec3::NEG_Z, turn: DVec2::NEG_X * 0.4, piloted: true, ..Default::default() };
    fly(&mut ship, &mut body, &input, &ModeCmds::default(), &Space::default(), 2.0);
    input.thrust = DVec3::ZERO;
    let curve = body.ang_vel.cross(body.lin_vel);
    let out = ship.step(&body, &input, &ModeCmds::default(), &Space::default(), DT);
    let acceleration = (out.lin_vel - body.lin_vel) / DT;
    let delivered = acceleration.dot(curve.normalize());
    eprintln!("gas release: curve demand {:.3}, delivered {delivered:.3}, speed {:.3} -> {:.3}", curve.length(), body.lin_vel.length(), out.lin_vel.length());
    assert!(delivered >= curve.length() - 1e-8, "gas release starved ongoing curve thrust");
    assert!(out.lin_vel.length() < body.lin_vel.length(), "automatic braking still reduces speed");
}

#[test]
fn releasing_pitch_while_holding_throttle_does_not_leave_rotation_stuck() {
    for comstab in [true, false] {
        let mut ship = ScShip::new(tuning());
        ship.modes.comstab = comstab;
        let mut body = body_at(DVec3::Y * 10000.0);
        let env = Air { density: 0.3, ..Default::default() };
        let mut input = FlightInput { thrust: DVec3::NEG_Z, turn: DVec2::NEG_X, piloted: true, ..Default::default() };
        fly(&mut ship, &mut body, &input, &ModeCmds::default(), &env, 8.0);
        input.turn = DVec2::ZERO;
        fly(&mut ship, &mut body, &input, &ModeCmds::default(), &env, 8.0);
        assert!(body.ang_vel.length() < 0.001, "comstab {comstab}: release left angular velocity {:?}", body.ang_vel);
    }
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
