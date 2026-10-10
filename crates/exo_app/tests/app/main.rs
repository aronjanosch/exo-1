//! Every integration test of the app in one binary (#186): each file in `tests/` is its own binary
//! that links Bevy (260 MB dynamic, about 900 MB static), so 27 of them cost 27 links per change.
//! nextest still runs each test in its own process. A new scenario test is a module here;
//! `perf.rs` stays its own binary for its nextest profile.
mod common;

mod collision_t1;
mod contacts;
mod headless_ticks;
mod mouse_look;
mod out_dirs;
mod scenario_boost_hud;
mod scenario_camera_g;
mod scenario_courier;
mod scenario_crate_budget;
mod scenario_crate_carry;
mod scenario_crate_hull;
mod scenario_crate_lock;
mod scenario_crate_ramp;
mod scenario_crate_ride;
mod scenario_crate_unload;
mod scenario_crate_wake_stream;
mod scenario_customers;
mod scenario_daynight;
mod scenario_deliver;
mod scenario_dev_menu;
mod scenario_first_person_settings;
mod scenario_flight;
mod scenario_help;
mod scenario_foreign;
mod scenario_foreign_warp;
mod scenario_full;
mod scenario_interact;
mod scenario_licence;
mod scenario_look;
mod scenario_map;
mod scenario_reload;
mod scenario_savefile;
mod scenario_sc_air;
mod scenario_sc_body;
mod scenario_sc_flight_hud;
mod scenario_sc_hud;
mod scenario_sc_linear;
mod scenario_sc_lift;
mod scenario_sc_turn;
mod scenario_site;
mod scenario_slope_landing;
mod scenario_space;
mod scenario_swap;
mod scenario_thruster_audio;
mod scenario_warp;
mod session;

/// The binary count does not creep back: `tests/` holds only this folder and `perf.rs`.
#[test]
fn tests_folder_holds_one_app_binary_and_perf() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut extra: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != "app" && n != "perf.rs")
        .collect();
    extra.sort();
    assert!(extra.is_empty(), "move {extra:?} into tests/app/ as a module of the one app test binary");
}
