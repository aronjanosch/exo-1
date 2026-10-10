//! #70: a walk reaches a site and the walker stands on its flattened ground.
use crate::common;
use exo_app::Options;

#[test]
fn site_walk_passes_headless() {
    let o = Options { scenario: Some("site-walk".into()), headless: true, out_dir: common::out_dir("site-walk"), ..Default::default() };
    let _ = common::run_scenario(&o);
}
