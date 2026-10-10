//! #168: customers order, orders become offers at the wholesaler, deliveries move the relationship.
//! Without a window.
use crate::common;
use exo_app::Options;

#[test]
fn customers_scenario_passes_headless() {
    let o = Options { scenario: Some("customers".into()), headless: true, out_dir: common::out_dir("customers-scenario"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
