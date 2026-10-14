//! The slice's picture check: the engine room and the walker's HUD, shot from fixed viewpoints
//! into the scenario's out dir as `slice_<letter>_<what>.png` (with a window; headless it runs the
//! same states without shots). Run with `--start=dev_area --dev-area`. The steps set the states
//! the pictures need: the reactor warms up on Auto, the corridor and the room are viewed, each
//! screen is viewed from 1.2 m, the pump breaks and the readout goes HOT, SCRAM and the NO POWER
//! screens, POWER back on, a fire at the reactor is sprayed with the extinguisher held, the wrench
//! repairs the broken pump (the ring), and the walker stands on the hot plate, in the no-air booth
//! and at the glowing barrel for the HUD's burn, low oxygen and radiation.
use super::items::{collect, items, outside, tap_then, DELAY};
use super::*;
use crate::controls::Controls;
use crate::crew_hud::Vitals;
use crate::dev_area::engine_room::{CORRIDOR, COOLANT_PUMP_AT, DOOR_AT, JUNCTION_BOX_AT, REACTOR_AT};
use crate::dev_area::hazards::{GLOW_BARREL, HOT_PLATE, NO_AIR};
use crate::dev_area::reactor::ReactorSim;
use crate::dev_area::reactor_screen::{DOOR_ID, FAULTS_ID, JUNCTION_ID, PUMP_ID, READOUT_ID};
use crate::dev_area::{DevArea, ENGINEERING};
use crate::displays::{aim_at, world_point_of, DisplayPanel, Panels};
use crate::items::Items;
use crate::repair::RepairPlay;
use display_core::PanelId;
use item_core::wire::{ReactorAsk, ReactorFault};
use std::path::{Path, PathBuf};
use walker_core::Frame as WalkFrame;

const DIGITS: [KeyCode; 8] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];

/// Where the camera looks: a point on the ground (area metres, height above it) or a screen's centre.
#[derive(Clone)]
enum Look {
    Ground([f64; 2], f64),
    Screen(PanelId),
}

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

fn screen_centre(w: &mut World, id: &PanelId) -> DVec3 {
    w.query::<&DisplayPanel>().iter(w).find(|d| &d.id == id).map(|d| d.centre).unwrap_or_else(|| panic!("no display {id:?}"))
}

fn target_of(w: &mut World, look: &Look) -> DVec3 {
    match look {
        Look::Ground(p, h) => ground(w, *p, *h),
        Look::Screen(id) => screen_centre(w, id),
    }
}

/// Turns the walker on foot to a point: level heading, then the pitch (the screens low on a
/// machine are seen from above). World frame, as the repair scenario does it.
fn aim(w: &mut World, target: DVec3) {
    with_player(w, |p| {
        let up = p.world_up(WalkFrame::IDENTITY);
        let eye = p.world_pos(WalkFrame::IDENTITY) + up * crate::walker::EYE_HEIGHT;
        let d = target - eye;
        let h = d - up * d.dot(up);
        if h.length() > 1e-9 {
            p.w.forward = h.normalize();
        }
        p.pitch = d.dot(up).atan2(h.length());
    });
}

/// A window screenshot into the out dir under the exact name (headless: nothing).
fn snap(w: &mut World, dir: &Path, windowed: bool, name: &str) {
    if !windowed {
        return;
    }
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    let _ = std::fs::create_dir_all(dir);
    let path: PathBuf = dir.join(format!("{name}.png"));
    w.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
}

/// The same, once per tag: a step runs every tick, the capture only on the first.
fn snap_once(c: &mut Ctx, w: &mut World, dir: &Path, windowed: bool, tag: &'static str) {
    if !c.v.contains_key(tag) {
        c.v.insert(tag, 1.0);
        snap(w, dir, windowed, tag);
    }
}

fn sim(w: &World) -> &ReactorSim {
    w.resource::<ReactorSim>()
}

fn readout_status(w: &World) -> Option<String> {
    w.resource::<Panels>().get(&READOUT_ID).and_then(|p| p.plate.clone()).and_then(|p| p.status).map(|s| s.0)
}

/// The walker stands at a point facing a look target; on the first tick it is placed and turned,
/// then (after `settle` s) it takes the picture and holds for the screenshot to be taken.
fn view(name: &'static str, stand: [f64; 2], look: Look, shot: Option<&'static str>, dir: PathBuf, windowed: bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            let at = ground(w, stand, 0.0);
            place_walker(w, at);
            return false;
        }
        let target = target_of(w, &look);
        aim(w, target);
        if c.t < 0.5 {
            return false;
        }
        if let Some(tag) = shot {
            snap_once(c, w, &dir, windowed, tag);
        }
        if c.t < 0.9 {
            return false;
        }
        end(w, c, shot.unwrap_or("view").to_string());
        true
    })
}

/// Waits `secs` of game time (the step checks nothing), with a note of the reactor's temperature.
fn wait_for(name: &'static str, secs: f64) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            return false;
        }
        if c.t < secs {
            return false;
        }
        let t = sim(w).room.reactor().temperature();
        end(w, c, format!("{t:.1} C"));
        true
    })
}

/// Clicks a widget of a screen from where the walker stands (the cursor goes there, one click).
fn click_widget(id: PanelId, action: &'static str, name: &'static str) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            let d = w.query::<&DisplayPanel>().iter(w).find(|d| d.id == id).cloned().expect("the display is there");
            let at = world_point_of(w, &d, action, None).unwrap_or_else(|| panic!("no widget '{action}'"));
            aim_at(w, at);
            w.resource_mut::<Controls>().click = true;
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        end(w, c, action.to_string());
        true
    })
}

/// Selects the inventory slot that holds an item (as the repair scenario does), then waits.
fn select_item(name: &'static str, def: &'static str) -> Step {
    let mut slot = 0usize;
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == def)).unwrap_or(0);
            tap(w, DIGITS[slot]);
            return false;
        }
        if c.t < 0.3 || items(w).client.view.selected != slot {
            return c.t > 2.0;
        }
        end(w, c, format!("slot {}", slot + 1));
        true
    })
}

pub(super) fn slice_look_steps(s: &mut Vec<Step>, out_dir: &Path, windowed: bool) {
    let dir = out_dir.to_path_buf();
    let shot_view = |name: &'static str, stand: [f64; 2], look: Look, tag: &'static str| view(name, stand, look, Some(tag), dir.clone(), windowed);

    // Warm-up on Auto (the reactor's cold start takes about a minute).
    s.push(wait_for("slice: the reactor warms up on Auto", 90.0));

    // a. The corridor, looking at the engine-room door and its screen (the door shut).
    s.push(shot_view("slice a: corridor at the door", [CORRIDOR.centre[0], -6.3], Look::Ground(DOOR_AT, 1.5), "slice_a_corridor_door"));

    // b. Inside the room from the door: overview with the reactor, pump, junction box and screens.
    s.push(shot_view("slice b: the room from the door", [DOOR_AT[0], -12.4], Look::Ground([-13.6, -17.0], 1.0), "slice_b_room_overview"));

    // c. Close-ups at about 1.2 m, each screen in front of its machine or wall. The normal states first.
    s.push(shot_view("slice c: reactor readout, normal", [-11.8, -17.76], Look::Screen(READOUT_ID), "slice_c_readout_normal"));
    s.push(shot_view("slice c: coolant pump screen", [COOLANT_PUMP_AT[0], -15.75], Look::Screen(PUMP_ID), "slice_c_pump_screen"));
    s.push(shot_view("slice c: junction box screen", [JUNCTION_BOX_AT[0], -17.78], Look::Screen(JUNCTION_ID), "slice_c_junction_screen"));
    s.push(shot_view("slice c: door screen", [DOOR_AT[0] - 0.24, -10.0], Look::Screen(DOOR_ID), "slice_c_door_screen"));
    s.push(shot_view("slice c: dev faults screen", [-17.5, -12.24], Look::Screen(FAULTS_ID), "slice_c_faults_screen"));

    // The pump breaks from the dev faults screen (the dev button's fault), the reactor heats; the
    // readout is shot once it reads HOT, then SCRAM at the readout.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice c: the pump breaks (dev fault), the readout heats");
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::PumpBreak));
            let at = ground(w, [-11.8, -17.76], 0.0);
            place_walker(w, at);
            return false;
        }
        let rt = screen_centre(w, &READOUT_ID);
        aim(w, rt);
        if readout_status(w).as_deref() == Some("HOT") || c.t >= 60.0 {
            end(w, c, format!("status {:?} after {:.1} s", readout_status(w), c.t));
            return true;
        }
        false
    }));
    s.push(Box::new(move |w, c| {
        if c.t < 0.5 {
            return false;
        }
        snap_once(c, w, &dir, windowed, "slice_c_readout_hot");
        c.t > 1.0
    }));
    // (Fields and checks as the reactor scenario: SCRAM at the readout cuts the power.)
    s.push(click_widget(READOUT_ID, "scram", "slice: SCRAM at the readout"));
    s.push(wait_for("slice: the machines go dark after SCRAM", 3.0));

    // d. After SCRAM: the room and the pump screen (NO POWER, lights off).
    let dir_d = out_dir.to_path_buf();
    s.push(view("slice d: the room after SCRAM", [DOOR_AT[0], -12.4], Look::Ground([-13.6, -17.0], 1.0), Some("slice_d_scram_room"), dir_d.clone(), windowed));
    s.push(view("slice d: the pump screen after SCRAM", [COOLANT_PUMP_AT[0], -15.75], Look::Screen(PUMP_ID), Some("slice_d_scram_pump_screen"), dir_d, windowed));

    // Back to POWER at the readout, then the reactor settles before the fire and the repair.
    s.push(view("slice: stand at the readout for POWER", [-11.8, -17.76], Look::Screen(READOUT_ID), None, out_dir.to_path_buf(), windowed));
    s.push(click_widget(READOUT_ID, "power", "slice: POWER on at the readout"));
    s.push(wait_for("slice: the reactor runs again", 30.0));

    // e. A fire at the reactor, from 3 m; then the extinguisher held and sprayed at it.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice e: the table and the extinguisher");
            w.resource_mut::<Items>().delay = DELAY;
            let at = ground(w, [ENGINEERING.centre[0] - 3.0, 1.6], 0.0);
            place_walker(w, at);
            return false;
        }
        if outside(w, "extinguisher").is_none() && c.t < 5.0 {
            return false;
        }
        end(w, c, "at the table".into());
        true
    }));
    s.extend(collect("extinguisher", "Extinguisher"));
    s.push(tap_then("slice e: 1 takes the extinguisher into the hand", KeyCode::Digit1, |w| items(w).in_hand(0).is_some_and(|s| s.item.kind.as_str() == "extinguisher")));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice e: the dev fault starts a fire at the reactor");
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::Fire));
            return false;
        }
        c.t >= 6.0
    }));
    let fire_dir = out_dir.to_path_buf();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice e: the fire at 3 m");
            let at = ground(w, [REACTOR_AT[0], REACTOR_AT[1] + 3.0], 0.0);
            place_walker(w, at);
        }
        let target = ground(w, REACTOR_AT, 0.3);
        aim(w, target);
        if c.t < 0.5 {
            return false;
        }
        snap_once(c, w, &fire_dir, windowed, "slice_e_fire_3m");
        c.t >= 0.9
    }));
    let spray_dir = out_dir.to_path_buf();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice e: the extinguisher sprayed at the fire from 3 m");
        }
        let target = ground(w, REACTOR_AT, 0.3);
        aim(w, target);
        w.resource_mut::<Controls>().primary_held = true;
        if c.t < 1.2 {
            return false;
        }
        snap_once(c, w, &spray_dir, windowed, "slice_e_spray");
        if c.t < 1.6 {
            return false;
        }
        w.resource_mut::<Controls>().primary_held = false;
        end(w, c, "sprayed 1.6 s".into());
        true
    }));

    // f. The repair ring: the wrench in hand at the broken pump (the dev fault breaks it again).
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice f: the extinguisher down, the table and the wrench");
            w.resource_mut::<Items>().delay = DELAY;
            tap(w, KeyCode::KeyG);
            let at = ground(w, [ENGINEERING.centre[0] - 3.0, 1.6], 0.0);
            place_walker(w, at);
            return false;
        }
        if c.t < 1.0 {
            return false;
        }
        end(w, c, "at the table".into());
        true
    }));
    s.extend(collect("wrench", "Wrench"));
    s.push(select_item("slice f: the wrench is selected", "wrench"));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice f: the dev fault breaks the coolant pump");
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::PumpBreak));
            return false;
        }
        c.t >= 1.0
    }));
    let ring_dir = out_dir.to_path_buf();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice f: the repair ring on the broken pump");
            let at = ground(w, [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 1.4], 0.0);
            place_walker(w, at);
        }
        let target = ground(w, COOLANT_PUMP_AT, 0.6);
        aim(w, target);
        if c.t == 0.5 {
            w.resource_mut::<Controls>().click = true;
        }
        if c.t < 1.5 {
            return false;
        }
        let ring = w.resource::<RepairPlay>().0.is_some();
        snap_once(c, w, &ring_dir, windowed, "slice_f_repair_ring");
        if c.t < 1.9 {
            return false;
        }
        end(w, c, format!("ring {ring}"));
        true
    }));

    // g. The HUD: a burn and catching fire on the hot plate, low oxygen in the no-air booth,
    // radiation at the glowing barrel (the vitals lines).
    let hud_dir = out_dir.to_path_buf();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice g: the hot plate, burn and catching fire");
            let at = ground(w, HOT_PLATE.centre, 0.0);
            place_walker(w, at);
            return false;
        }
        let target = ground(w, [HOT_PLATE.centre[0], HOT_PLATE.centre[1] + 3.0], 1.2);
        aim(w, target);
        // Catching fire takes about 4.5 s on the plate (health scenario); the picture at 5 s.
        let burning = w.resource::<Vitals>().burning;
        if c.t < 5.0 {
            return false;
        }
        snap_once(c, w, &hud_dir, windowed, "slice_g_burn_fire");
        if c.t < 5.5 {
            return false;
        }
        end(w, c, format!("burning {burning}"));
        true
    }));
    let hud_dir2 = out_dir.to_path_buf();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice g: the no-air booth, low oxygen");
            let at = ground(w, NO_AIR.centre, 0.0);
            place_walker(w, at);
            return false;
        }
        let target = ground(w, [NO_AIR.centre[0], NO_AIR.centre[1] + 3.0], 1.2);
        aim(w, target);
        if c.t < 6.0 {
            return false;
        }
        let o2 = w.resource::<Vitals>().oxygen;
        snap_once(c, w, &hud_dir2, windowed, "slice_g_low_o2");
        if c.t < 6.5 {
            return false;
        }
        end(w, c, format!("oxygen {o2:?}"));
        true
    }));
    let hud_dir3 = out_dir.to_path_buf();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice g: the glowing barrel, radiation");
            let at = ground(w, GLOW_BARREL.centre, 0.0);
            place_walker(w, at);
            return false;
        }
        let target = ground(w, [GLOW_BARREL.centre[0], GLOW_BARREL.centre[1] + 3.0], 1.2);
        aim(w, target);
        if c.t < 8.0 {
            return false;
        }
        snap_once(c, w, &hud_dir3, windowed, "slice_g_radiation");
        if c.t < 8.5 {
            return false;
        }
        end(w, c, "radiation".into());
        true
    }));
}
