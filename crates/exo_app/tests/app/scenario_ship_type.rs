//! The named type goes through the real spawn and a Controls-driven flight scenario.
use crate::common;
use avian3d::prelude::Mass;
use bevy::prelude::Name;
use exo_app::{ship::Ship, Options};

#[test]
fn scenario_ship_type_kackfass_1_owns_its_identity_and_physics() {
    let o = Options { scenario: Some("sc-body".into()), headless: true, out_dir: common::out_dir("ship-type"), ..Default::default() };
    let mut app = common::run_scenario(&o);
    let world = app.world_mut();
    let mut query = world.query::<(&Ship, &Mass, &Name)>();
    let (ship, mass, name) = query.single(world).unwrap();
    assert_eq!(ship.model.id, "kackfass_1");
    assert_eq!(ship.model.name, "Kackfass-1");
    assert_eq!(name.as_str(), "Kackfass-1");
    assert!((mass.0 as f64 - ship.sc.mass()).abs() < 0.01);
    assert_eq!(ship.sc.tuning.ship.thrust.forward, 162000.0);
    let hud = world.resource::<exo_app::hud::HudReadout>();
    assert!(hud.texts[0].contains("Kackfass-1"));
}
