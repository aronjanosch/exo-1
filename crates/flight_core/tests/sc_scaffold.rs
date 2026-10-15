mod sc_common;
use flight_core::sc::{ModeCmds, ScShip};
use glam::DVec3;
use sc_common::*;

/// A step the caller does not fly (the ship rests on the ground, the ground rules fly it) reads no
/// thrust: the thruster sound and the HUD take the status, not the last flown step's.
#[test]
fn a_skipped_step_reads_no_thrust() {
    let mut ship = ScShip::new(tuning());
    let mut body = body_at(DVec3::ZERO);
    fly(&mut ship, &mut body, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &Space::default(), 1.0);
    let safe_share = 4.0 * 9.81 / (ship.tuning.ship.thrust.forward / ship.mass());
    assert!((ship.status.thrust_share.z.abs() - safe_share).abs() < 0.01, "flown at the G-safe ceiling: {:?}", ship.status.thrust_share);
    ship.skip_step(1.0 / 60.0);
    assert_eq!(ship.status.thrust_share, DVec3::ZERO);
    assert_eq!((ship.status.felt_g, ship.status.saturated, ship.status.braking), (0.0, false, false));
}
