mod sc_common;
use flight_core::ship_type::{HullKind, ShipModel, ShipType};
use flight_core::sc::{ModeCmds, ScShip};
use glam::DVec3;
use sc_common::*;

const MODEL: &str = include_str!("../../../content/ship/kackfass_1/type.json");

#[test]
fn kackfass_1_has_a_valid_identity_hull_and_complete_profile() {
    let model = ShipModel::from_json(MODEL).unwrap();
    assert_eq!(model.id, "kackfass_1");
    assert_eq!(model.name, "Kackfass-1");
    assert_eq!(model.hull, HullKind::Kackfass1);
    let ship_type = ShipType { model, flight: tuning() };
    assert_eq!(ship_type.flight.ship.mass, 2000.0);
    assert_eq!(ship_type.flight.ship.thrust.forward, 162000.0);
}

#[test]
fn type_identity_rejects_invalid_or_missing_fields() {
    for broken in [
        MODEL.replace("kackfass_1", "Bad ID"),
        MODEL.replace("Kackfass-1", " "),
        MODEL.replace("\"hull\": \"kackfass_1\"", "\"hull\": \"missing_hull\""),
        MODEL.replace("\"name\"", "\"naem\""),
    ] {
        assert!(ShipModel::from_json(&broken).is_err(), "accepted {broken}");
    }
}

#[test]
fn instances_use_their_own_type_profile_without_changing_other_ships() {
    let mut first = ScShip::new(tuning());
    let mut heavier_profile = first.tuning.clone();
    heavier_profile.ship.mass *= 2.0;
    let mut second = ScShip::new(heavier_profile);
    first.modes.g_safe = false;
    second.modes.g_safe = false;
    first.modes.coupled = false;
    first.modes.coupling = 0.0;
    second.modes.coupled = false;
    second.modes.coupling = 0.0;
    let mut a = body_at(DVec3::Y * 10000.0);
    let mut b = a.clone();
    fly(&mut first, &mut a, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &Space::default(), 1.0);
    fly(&mut second, &mut b, &thrust(DVec3::NEG_Z), &ModeCmds::default(), &Space::default(), 1.0);
    assert!(a.lin_vel.length() > b.lin_vel.length());
    assert_eq!(first.mass(), 2000.0);
    assert_eq!(second.mass(), 4000.0);
}
