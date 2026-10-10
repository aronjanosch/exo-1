//! The SC model's scaffold (round 5): the tuning files load, the ship holds its height with
//! gravity compensation and falls without it, and full forward thrust reaches the SCM cap.
mod sc_common;
use flight_core::sc::{ModeCmds, ScShip};
use flight_core::FlightInput;
use glam::DVec3;
use sc_common::*;

#[test]
fn tuning_files_load() {
    let t = tuning();
    assert!(t.ship.mass > 0.0);
}

#[test]
fn gravity_compensation_holds_and_off_falls() {
    let env = Air { density: 0.0, ..Air::default() };
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::new(0.0, 500.0, 0.0));
    fly(&mut ship, &mut body, &FlightInput { piloted: true, ..Default::default() }, &ModeCmds::default(), &env, 3.0);
    assert!((body.pos.y - 500.0).abs() < 0.05, "hover: height {:.3}", body.pos.y);

    fly(&mut ship, &mut body, &FlightInput { piloted: true, ..Default::default() }, &ModeCmds { grav_comp: true, ..Default::default() }, &env, 1.0);
    assert!(!ship.status.grav_comp);
    // The scaffold's coupled damping still fights the fall; lane `sc-linear` makes it ballistic.
    let fallen = 500.0 - body.pos.y;
    assert!(fallen > 1.0, "1 s without compensation sinks: {fallen:.2} m");
}

#[test]
fn forward_reaches_the_scm_cap_in_space() {
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::ZERO);
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &Space::default(), 10.0);
    let cap = ship.tuning.linear.scm.cruise;
    assert!((body.lin_vel.length() - cap).abs() < 1.0, "speed {:.2}, cap {cap}", body.lin_vel.length());
    assert!(ship.status.thrust_share.z.abs() < 0.05, "at the cap the thrusters rest: {:?}", ship.status.thrust_share);
}

/// A step the caller does not fly (the ship rests on the ground, the ground rules fly it) reads no
/// thrust: the thruster sound and the HUD take the status, not the last flown step's.
#[test]
fn a_skipped_step_reads_no_thrust() {
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::ZERO);
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &Space::default(), 1.0);
    assert!(ship.status.thrust_share.z.abs() > 0.5, "flown: {:?}", ship.status.thrust_share);
    ship.skip_step(1.0 / 60.0);
    assert_eq!(ship.status.thrust_share, DVec3::ZERO);
    assert_eq!((ship.status.felt_g, ship.status.saturated, ship.status.braking), (0.0, false, false));
}
