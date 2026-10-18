//! One test binary: each file in tests/ would link Bevy again.
mod common;

mod collision_t1;
mod contacts;
mod headless_ticks;
mod mouse_look;
mod out_dirs;
mod scenario_camera_g;
mod scenario_coolant;
mod scenario_coupled_loop;
mod scenario_rods;
mod scenario_blast;
mod scenario_crate_budget;
mod scenario_crate_carry;
mod scenario_crate_hull;
mod scenario_crate_lock;
mod scenario_crate_ramp;
mod scenario_crate_ride;
mod scenario_crate_unload;
mod scenario_crate_wake_stream;
mod scenario_town;
mod scenario_daynight;
mod scenario_dev_area;
mod scenario_items;
mod scenario_dev_menu;
mod scenario_displays;
mod scenario_first_person_settings;
mod scenario_help;
mod scenario_engine_room;
mod scenario_extinguisher;
mod scenario_fire;
mod scenario_fire_grid;
mod scenario_repair;
mod scenario_rescue;
mod scenario_hud;
mod scenario_foreign;
mod scenario_foreign_warp;
mod scenario_full;
mod scenario_interact;
mod scenario_licence;
mod scenario_look;
mod scenario_map;
mod scenario_radius;
mod scenario_reload;
mod scenario_savefile;
mod scenario_sc_flight_hud;
mod scenario_ship_ambush;
mod scenario_site;
mod scenario_slope_landing;
mod scenario_space;
mod scenario_swap;
mod items_net;
mod session;

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
