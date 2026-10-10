//! The SC model's linear law (round 5, #195): the cap that refuses thrust, the brake along the
//! velocity, anti-drift, the strafe taper, gravity compensation, G-safe, comstab, the master modes,
//! the speed limiter, proximity and landing mode. Numbers are printed; values are placeholders.
mod sc_common;
use flight_core::sc::{ModeCmds, ScShip};
use flight_core::{BodyState, FlightInput, PlanetEnv};
use glam::{DQuat, DVec3};
use sc_common::*;

const G0: f64 = 9.81;

/// A piloted input with the thrust, boost and brake as given.
fn input(t: DVec3, boost: bool, brake: bool) -> FlightInput {
    FlightInput { thrust: t, boost, brake, piloted: true, ..FlightInput::default() }
}

/// One step as the app does it; returns the world acceleration the step gave the ship.
fn step(ship: &mut ScShip, body: &mut BodyState, i: &FlightInput, c: &ModeCmds, env: &impl PlanetEnv) -> DVec3 {
    let out = ship.step(body, i, c, env, DT);
    let a = (out.lin_vel - body.lin_vel) / DT;
    body.lin_vel = out.lin_vel;
    body.ang_vel = out.ang_vel;
    body.integrate(DT);
    a
}

#[test]
fn a3_the_cap_refuses_thrust_above_it() {
    let space = Space::default();
    // Decoupled, so nothing damps the thrust along the stick and the cap is what holds the speed
    // (coupled, the goal already sits inside the cap). D to the sideways cap, then decouple and
    // full W with boost.
    let peak = |refuse: bool| {
        let mut t = tuning();
        t.linear.refuse_thrust = refuse;
        t.modes.decouple_time = 0.0;
        let mut ship = ScShip::new(t);
        let mut body = body_at(DVec3::ZERO);
        fly(&mut ship, &mut body, &thrust(DVec3::X), &ModeCmds::default(), &space, 8.0);
        let mut peak: f64 = 0.0;
        for i in 0..(3.0 / DT) as usize {
            let c = if i == 0 { ModeCmds { decoupled: true, ..Default::default() } } else { ModeCmds::default() };
            step(&mut ship, &mut body, &input(DVec3::NEG_Z, true, false), &c, &space);
            peak = peak.max(body.lin_vel.length());
        }
        peak
    };
    let (on, off) = (peak(true), peak(false));
    let cap = tuning().linear.scm.boost_forward;
    println!("A3 decoupled: peak with refusal {on:.2} m/s, without {off:.2} m/s, boost cap {cap}");
    assert!(on <= cap + 0.5, "peak {on:.2} above the cap {cap} + 0.5");
    assert!(off > on + 5.0, "without refusal the peak is higher: {off:.2} vs {on:.2}");
}

#[test]
fn brake_along_the_velocity_keeps_the_heading_and_stops() {
    let space = Space::default();
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::ZERO);
    fly(&mut ship, &mut body, &thrust(DVec3::new(1.0, 0.0, -1.0)), &ModeCmds::default(), &space, 8.0);
    let h0 = body.lin_vel.normalize();
    println!("B2: cruise speed {:.2} m/s", body.lin_vel.length());
    let (mut max_turn, mut stopped_at) = (0.0_f64, None);
    for i in 0..(10.0 / DT) as usize {
        step(&mut ship, &mut body, &input(DVec3::ZERO, false, true), &ModeCmds::default(), &space);
        let s = body.lin_vel.length();
        if s > 1.0 {
            max_turn = max_turn.max(body.lin_vel.normalize().angle_between(h0).to_degrees());
        }
        if s < 0.05 && stopped_at.is_none() {
            stopped_at = Some(i as f64 * DT);
        }
    }
    println!("B2: heading turn until 1 m/s {max_turn:.4} deg, stopped after {stopped_at:?} s");
    assert!(max_turn < 0.5, "heading turned {max_turn:.3} deg");
    assert!(stopped_at.is_some(), "the brake stops the ship");
    assert_eq!(body.lin_vel, DVec3::ZERO, "finished braking leaves no residual velocity");
}

#[test]
fn brake_holds_the_height_on_the_air_planet() {
    let env = Air::default();
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::new(0.0, 100.0, 0.0));
    fly(&mut ship, &mut body, &thrust(DVec3::new(1.0, 0.0, -1.0)), &ModeCmds::default(), &env, 8.0);
    let mut worst: f64 = 0.0;
    for _ in 0..(10.0 / DT) as usize {
        step(&mut ship, &mut body, &input(DVec3::ZERO, false, true), &ModeCmds::default(), &env);
        worst = worst.max((body.pos.y - 100.0).abs());
    }
    println!("B2 air: speed after brake {:.2} m/s, worst height change {worst:.3} m", body.lin_vel.length());
    assert!(worst < 1.0, "height moved {worst:.3} m under the brake");
    assert_eq!(body.lin_vel, DVec3::ZERO, "finished braking in air leaves no residual velocity");
}

#[test]
fn anti_drift_kills_the_velocity_across_the_goal_first() {
    let space = Space::default();
    let drift = |anti: bool| {
        let mut t = tuning();
        t.linear.anti_drift = anti;
        let mut ship = ScShip::new(t);
        let mut body = body_at(DVec3::ZERO);
        fly(&mut ship, &mut body, &thrust(DVec3::X), &ModeCmds::default(), &space, 8.0);
        // W and A together (a diagonal goal): the sideways velocity has to die across it. On a
        // straight W goal the across rate is the same with and without anti-drift (the across
        // axis is at its limit either way), so the diagonal is where it shows.
        let stick = DVec3::new(-1.0, 0.0, -1.0);
        let goal = stick.normalize();
        let mut sum = 0.0;
        for _ in 0..(6.0 / DT) as usize {
            step(&mut ship, &mut body, &thrust(stick), &ModeCmds::default(), &space);
            let v = body.lin_vel;
            sum += (v - v.dot(goal) * goal).length() * DT;
        }
        sum
    };
    let (on, off) = (drift(true), drift(false));
    println!("anti-drift: summed velocity across the goal {on:.1} m (on), {off:.1} m (off)");
    assert!(on < off, "anti-drift on {on:.1} m, off {off:.1} m");
}

#[test]
fn release_from_the_boost_cap_bleeds_without_a_plateau() {
    let space = Space::default();
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::ZERO);
    // The boost drains in 3 s; 2.5 s of it builds speed past the cruise cap.
    fly(&mut ship, &mut body, &input(DVec3::NEG_Z, true, false), &ModeCmds::default(), &space, 2.5);
    assert!(body.lin_vel.length() > 155.0, "boosted above the cruise cap first: {:.1}", body.lin_vel.length());
    let mut speeds = Vec::new();
    for _ in 0..(12.0 / DT) as usize {
        step(&mut ship, &mut body, &FlightInput::default(), &ModeCmds::default(), &space);
        speeds.push(body.lin_vel.length());
    }
    let monotonic = speeds.windows(2).all(|w| w[1] <= w[0] + 1e-9);
    let window = (0.5 / DT) as usize;
    let plateau = (0..speeds.len() - window).find(|&i| speeds[i] > 1.0 && (speeds[i + window] - speeds[i]).abs() < 0.1);
    println!("release: {:.1} -> {:.1} m/s in 12 s, monotonic {monotonic}, plateau at {plateau:?}", speeds[0], speeds[speeds.len() - 1]);
    assert!(monotonic, "speed rose during the release");
    assert!(speeds[speeds.len() - 1] < 1.0, "the release stops the ship: {:.2}", speeds[speeds.len() - 1]);
    assert!(plateau.is_none(), "a plateau above 1 m/s at step {plateau:?}");
}

#[test]
fn strafe_tapers_with_forward_speed_under_boost_only() {
    let space = Space::default();
    // The sideways acceleration of one step with D and the given velocity.
    let side = |v: DVec3, boost: bool| {
        let mut ship = ScShip::new(instant());
        let mut body = BodyState { lin_vel: v, ..body_at(DVec3::ZERO) };
        step(&mut ship, &mut body, &input(DVec3::X, boost, false), &ModeCmds::default(), &space).x
    };
    let at_rest = side(DVec3::ZERO, true);
    let boosted_cap = side(DVec3::new(0.0, 0.0, -150.0), true);
    let unboosted_cap = side(DVec3::new(0.0, 0.0, -150.0), false);
    // Full: the thrusters' side, or G-safe's 4 g if that is lower.
    let full = (tuning().ship.thrust.right / tuning().ship.mass).min(tuning().linear.g_limit.right * G0);
    println!("strafe: boosted at rest {at_rest:.2}, boosted at the cruise cap {boosted_cap:.2}, unboosted at the cap {unboosted_cap:.2}, full right {full:.2}");
    assert!(boosted_cap < at_rest * 0.9, "boosted strafe at the cap {boosted_cap:.2} vs at rest {at_rest:.2}");
    assert!((unboosted_cap - full).abs() < 1e-3, "unboosted strafe at the cap {unboosted_cap:.3}, full {full:.3}");
}

#[test]
fn gravity_compensation_off_falls_with_g_and_on_again_holds() {
    let env = Air::default();
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::new(0.0, 500.0, 0.0));
    // The first step taps H: compensation off.
    fly(&mut ship, &mut body, &FlightInput::default(), &ModeCmds { grav_comp: true, ..Default::default() }, &env, 1.0);
    assert!(!ship.status.grav_comp);
    let (fallen, speed) = (500.0 - body.pos.y, -body.lin_vel.y);
    let g = G0;
    println!("H off, 1 s: fell {fallen:.3} m (g/2 = {:.3}), speed {speed:.3} (g = {g})", g / 2.0);
    assert!((fallen - g / 2.0).abs() < 0.02 * g / 2.0, "fell {fallen:.3} m");
    assert!((speed - g).abs() < 0.02 * g, "speed {speed:.3}");

    fly(&mut ship, &mut body, &FlightInput::default(), &ModeCmds { grav_comp: true, ..Default::default() }, &env, 3.0);
    assert!(ship.status.grav_comp);
    println!("H on, 3 s later: vertical speed {:.3} m/s", body.lin_vel.y);
    assert!(body.lin_vel.y.abs() < 1.0, "vertical speed {:.3} after compensation on", body.lin_vel.y);
}

#[test]
fn decoupled_at_speed_without_input_keeps_the_velocity() {
    let space = Space::default();
    let mut t = tuning();
    t.modes.decouple_time = 0.0;
    let mut ship = ScShip::new(t);
    let mut body = body_at(DVec3::ZERO);
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &space, 5.0);
    let v0 = body.lin_vel;
    fly(&mut ship, &mut body, &FlightInput::default(), &ModeCmds { decoupled: true, ..Default::default() }, &space, 3.0);
    let dv = (body.lin_vel - v0).length();
    println!("decoupled at {:.2} m/s, 3 s without input: dv {dv:.2e}", v0.length());
    assert!(!ship.status.coupled);
    assert!(dv < 1e-6, "velocity changed by {dv:e}");
}

#[test]
fn g_safe_keeps_the_forward_felt_g_under_the_limit_in_a_boost() {
    let space = Space::default();
    let peak = |g_safe: bool| {
        let mut ship = ScShip::new(tuning());
        let mut body = body_at(DVec3::ZERO);
        // The first step taps F8 when G-safe is to go off.
        let first = if g_safe { ModeCmds::default() } else { ModeCmds { g_safe: true, ..Default::default() } };
        let mut peak: f64 = 0.0;
        for i in 0..(3.0 / DT) as usize {
            let c = if i == 0 { first } else { ModeCmds::default() };
            let a = step(&mut ship, &mut body, &input(DVec3::NEG_Z, true, false), &c, &space);
            peak = peak.max(-a.z);
        }
        (peak, ship.tuning.linear.g_limit.forward * G0)
    };
    let (on, limit) = peak(true);
    let (off, _) = peak(false);
    println!("G-safe: forward felt {on:.2} m/s2 on, {off:.2} off, limit {limit:.2}");
    assert!(on <= limit + 1e-6, "felt {on:.2} above {limit:.2}");
    assert!(off > on + 1.0, "off {off:.2} not above on {on:.2}");
}

/// Slip angle (nose against velocity) at the end of a 90 degree yaw over 1 s with W held.
fn yaw_slip(lag: f64, comstab: bool) -> f64 {
    let space = Space::default();
    let mut t = tuning();
    t.linear.comstab_off_lag = lag;
    let mut ship = ScShip::new(t);
    let mut body = body_at(DVec3::ZERO);
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &space, 5.0);
    // The nose turns by hand; the velocity is the flight model's.
    let first = if comstab { ModeCmds::default() } else { ModeCmds { comstab: true, ..Default::default() } };
    let rate = std::f64::consts::FRAC_PI_2;
    for i in 0..(1.0 / DT).round() as usize {
        let c = if i == 0 { first } else { ModeCmds::default() };
        let out = ship.step(&body, &thrust(DVec3::NEG_Z), &c, &space, DT);
        body.lin_vel = out.lin_vel;
        body.rot = DQuat::from_rotation_y(rate * (i + 1) as f64 * DT);
        body.integrate(DT);
    }
    let nose = body.rot * DVec3::NEG_Z;
    body.lin_vel.normalize().angle_between(nose).to_degrees()
}

#[test]
fn comstab_off_slips_wider_in_a_yaw() {
    // The placeholder lag (0.6 s) gives a 1 to 2 degree difference only: the box, not the goal,
    // limits the turn in a second (numbers printed). The mechanism is checked with a 3 s lag; the
    // lag is TODO(initiator).
    let (on_default, off_default) = (yaw_slip(0.6, true), yaw_slip(0.6, false));
    let (on, off) = (yaw_slip(3.0, true), yaw_slip(3.0, false));
    println!("comstab: slip at the end of a 90 deg yaw, lag 0.6 s: {on_default:.2} on, {off_default:.2} off; lag 3 s: {on:.2} on, {off:.2} off");
    assert!(off > on, "lag 3 s: off {off:.2} deg not above on {on:.2} deg");
}

#[test]
fn turning_at_rest_launches_along_the_nose_with_comstab_on_or_off() {
    let env = Space::default();
    for comstab in [true, false] {
        for speed in [0.0, 0.004] {
            let mut ship = ScShip::new(tuning());
            ship.modes.comstab = comstab;
            let mut body = body_at(DVec3::ZERO);
            body.lin_vel = DVec3::NEG_Z * speed;
            // Turn on the spot without thrust, including a nonzero speed rounded to HUD 0.00.
            for i in 0..120 {
                body.rot = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2 * (i + 1) as f64 / 120.0);
                fly(&mut ship, &mut body, &input(DVec3::ZERO, false, false), &ModeCmds::default(), &env, DT);
            }
            let nose = body.rot * DVec3::NEG_Z;
            let start = body.pos;
            fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &env, 1.0);
            let across = body.lin_vel - nose * body.lin_vel.dot(nose);
            let travel = body.pos - start;
            let across_travel = travel - nose * travel.dot(nose);
            assert!(body.lin_vel.dot(nose) > 20.0, "launch accelerates along the nose");
            assert!(across.length() < 0.001 && across_travel.length() < 0.001,
                "comstab {comstab}, initial speed {speed}: velocity {across:?}, travel {across_travel:?} across the nose");
        }
    }
}

#[test]
fn comstab_off_after_braking_and_turning_does_not_remember_the_old_heading() {
    let env = Space::default();
    let mut ship = ScShip::new(tuning());
    ship.modes.comstab = false;
    let mut body = body_at(DVec3::ZERO);
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &env, 5.0);
    for _ in 0..600 {
        step(&mut ship, &mut body, &input(DVec3::ZERO, false, true), &ModeCmds::default(), &env);
        if body.lin_vel == DVec3::ZERO {
            break;
        }
    }
    assert_eq!(body.lin_vel, DVec3::ZERO, "braking reaches exact rest within 10 s");
    for i in 0..120 {
        body.rot = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2 * (i + 1) as f64 / 120.0);
        fly(&mut ship, &mut body, &input(DVec3::ZERO, false, false), &ModeCmds::default(), &env, DT);
    }
    let nose = body.rot * DVec3::NEG_Z;
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &env, 1.0);
    assert!(body.lin_vel.dot(nose) > 20.0);
    assert!(body.lin_vel.angle_between(nose).to_degrees() < 0.01, "velocity {:?} follows the nose {nose:?}", body.lin_vel);
}

#[test]
fn slow_decoupled_motion_is_not_snapped_to_rest_without_braking() {
    let env = Space::default();
    let mut ship = ScShip::new(tuning());
    ship.modes.coupled = false;
    ship.modes.coupling = 0.0;
    let mut body = body_at(DVec3::ZERO);
    body.lin_vel = DVec3::NEG_Z * 0.004;
    let velocity = body.lin_vel;
    fly(&mut ship, &mut body, &input(DVec3::ZERO, false, false), &ModeCmds::default(), &env, 2.0);
    assert_eq!(body.lin_vel, velocity, "slow unbraked motion stays physical");
}

#[test]
fn nav_reaches_above_the_scm_cap_and_bleeds_back_to_it() {
    let space = Space::default();
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::ZERO);
    let scm = tuning().linear.scm.cruise;
    let c = ModeCmds { master: true, ..Default::default() };
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &c, &space, 15.0);
    let nav = body.lin_vel.length();
    println!("NAV: speed {nav:.2} m/s after 15 s (SCM cap {scm})");
    assert!(nav > scm + 50.0, "NAV speed {nav:.1}");
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds { master: true, ..Default::default() }, &space, 20.0);
    let back = body.lin_vel.length();
    println!("back to SCM: speed {back:.2} m/s after 20 s");
    assert!((back - scm).abs() < 1.0, "bled to {back:.2}, SCM cap {scm}");
}

#[test]
fn limiter_at_half_holds_half_the_cap() {
    let space = Space::default();
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::ZERO);
    let c = ModeCmds { limiter_steps: -5, ..Default::default() };
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &c, &space, 10.0);
    let half = tuning().linear.scm.cruise * 0.5;
    println!("limiter {:.2}: speed {:.2} m/s, half cap {half}", ship.modes.limiter, body.lin_vel.length());
    assert!((ship.modes.limiter - 0.5).abs() < 1e-9);
    assert!((body.lin_vel.length() - half).abs() < 1.0, "speed {:.2}", body.lin_vel.length());
}

/// Holds Ctrl (down) from 200 m with the given proximity switch. Returns (lowest clearance seen
/// before the ground, speed when the height first drops under 1 m, the step it reached the ground).
fn dive(proximity: bool) -> (f64, f64, Option<f64>) {
    let env = Air::default();
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::new(0.0, 200.0, 0.0));
    let first = if proximity { ModeCmds::default() } else { ModeCmds { proximity: true, ..Default::default() } };
    let (mut low, mut speed_near_ground, mut ground_at) = (f64::MAX, None::<f64>, None);
    for i in 0..(60.0 / DT) as usize {
        let c = if i == 0 { first } else { ModeCmds::default() };
        step(&mut ship, &mut body, &input(DVec3::NEG_Y, false, false), &c, &env);
        if body.pos.y > 0.0 {
            low = low.min(body.pos.y);
        }
        if body.pos.y < 1.0 && speed_near_ground.is_none() {
            speed_near_ground = Some(body.lin_vel.length());
        }
        if body.pos.y <= 0.0 {
            ground_at = Some(i as f64 * DT);
            break;
        }
    }
    (low, speed_near_ground.unwrap_or(f64::NAN), ground_at)
}

#[test]
fn proximity_stops_the_dive_above_the_ground() {
    let (low, touch, _) = dive(true);
    println!("proximity on: lowest clearance {low:.3} m, speed under 1 m {touch:.3} m/s");
    assert!(low > 0.0, "reached the ground: clearance {low:.3}");
    assert!(touch < 3.0, "touched down at {touch:.3} m/s");
}

#[test]
fn without_proximity_the_dive_reaches_the_ground_fast() {
    let (_, touch, ground) = dive(false);
    println!("proximity off: reached the ground at {ground:?} s, speed under 1 m {touch:.2} m/s");
    assert!(ground.is_some(), "never reached the ground");
    assert!(touch > 10.0, "speed {touch:.2} at the ground");
}

#[test]
fn landing_mode_caps_the_speed_along_the_ground_near_the_ground() {
    let env = Air::default();
    let run = |landing: bool| {
        let mut ship = ScShip::new(tuning());
        let mut body = body_at(DVec3::new(0.0, 3.0, 0.0));
        let first = if landing { ModeCmds { landing: true, ..Default::default() } } else { ModeCmds::default() };
        let mut peak: f64 = 0.0;
        for i in 0..(6.0 / DT) as usize {
            let c = if i == 0 { first } else { ModeCmds::default() };
            step(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &c, &env);
            peak = peak.max(DVec3::new(body.lin_vel.x, 0.0, body.lin_vel.z).length());
        }
        peak
    };
    let (on, off) = (run(true), run(false));
    println!("landing at 3 m: ground speed peak {on:.2} m/s on, {off:.2} m/s off");
    let cap = tuning().linear.landing.speed;
    assert!(on <= cap + 0.5, "ground speed {on:.2} in landing mode (cap {cap})");
    assert!(off > cap + 0.5, "without landing mode the speed is not capped: {off:.2}");
}

/// Anti-drift changes nothing on a straight path: W from rest accelerates as with it off (the
/// across speed is float noise there, not drift).
#[test]
fn anti_drift_leaves_a_straight_burn_alone() {
    let speed_after = |anti: bool| {
        let mut t = tuning();
        t.linear.anti_drift = anti;
        let mut ship = ScShip::new(t);
        let mut body = body_at(DVec3::new(0.0, 300.0, 0.0));
        let air = Air { density: 0.0, ..Air::default() };
        fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &Space::default(), 1.0);
        let space = body.lin_vel.length();
        let mut ship = ScShip::new(ship.tuning.clone());
        let mut body = body_at(DVec3::new(0.0, 300.0, 0.0));
        fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &air, 1.0);
        (space, body.lin_vel.length())
    };
    let (on, off) = (speed_after(true), speed_after(false));
    println!("straight W 1 s, space and under gravity: {on:.3?} m/s with anti-drift, {off:.3?} without");
    assert!((on.0 - off.0).abs() < 0.01 * off.0 && (on.1 - off.1).abs() < 0.01 * off.1, "{on:?} vs {off:?}");
}
