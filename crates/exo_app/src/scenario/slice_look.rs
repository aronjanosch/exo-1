//! The slice's picture check: the engine room, the walker's HUD and this round's effects, shot from
//! fixed viewpoints into the scenario's out dir as `slice_<letter>_<what>.png` (with a window;
//! headless it runs the same states and the same checks without shots). Run with
//! `--start=dev_area --dev-area`. The steps set the states the pictures need, and check them:
//! the reactor warms up on Auto, the corridor and the room are viewed, each screen is viewed from
//! 1.2 m, the pump breaks and the readout goes HOT, the critical lamp lights, SCRAM (the walker
//! uses the readout, the click is retried until the reactor is off), 2 s of dark, then POWER on; a
//! fire at the reactor is sprayed from 2.5 m; the pump breaks again (smoke), the wrench starts the
//! hold repair (the minigame is off: the pump rises while held); the reactor on manual at 100 % with
//! the engines and lights off overvolts the junction box (sparks); the walker stands
//! inside the hot plate (burn, edge), the no-air booth (dark edge) and the glowing barrel (grain).
use super::items::{collect, items, outside, tap_then, DELAY};
use super::reactor::{click_until, use_display};
use super::*;
use crate::controls::Controls;
use crate::crew_hud::Vitals;
use crate::dev_area::engine_room::{CORRIDOR, COOLANT_PUMP_AT, DOOR_AT, JUNCTION_BOX_AT, REACTOR_AT};
use crate::dev_area::hazards::{GLOW_BARREL, HOT_PLATE, NO_AIR};
use crate::dev_area::reactor::ReactorSim;
use crate::dev_area::reactor_screen::{DOOR_ID, FAULTS_ID, JUNCTION_ID, PUMP_ID, READOUT_ID};
use crate::dev_area::{DevArea, ENGINEERING};
use crate::displays::{ActiveDisplay, DisplayPanel, Panels};
use crate::effects::{self, Smoke};
use crate::items::Items;
use crate::repair::{RepairPlay, RepairSessions};
use display_core::PanelId;
use item_core::wire::{BreakerGroup, FireNew, ReactorAsk, ReactorFault};
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

fn active(w: &World) -> ActiveDisplay {
    *w.resource::<ActiveDisplay>()
}

/// The pump's share of full condition (1 without a snapshot).
fn pump_share(w: &World) -> f64 {
    items(w).reactor.as_ref().map_or(1.0, |r| r.pump.share)
}

fn overvolt(w: &World) -> bool {
    items(w).reactor.as_ref().is_some_and(|r| r.overvolt)
}

fn machines_powered(w: &World) -> bool {
    items(w).reactor.as_ref().is_some_and(|r| r.powered)
}

/// Why the machines are dark or lit on this snapshot: the rule is the junction box working and the
/// reactor's output above 0 (no steam at 0 °C, no output).
fn power_why(w: &World) -> String {
    let room = &sim(w).room;
    let reac = room.reactor();
    format!(
        "power: reactor on {}, temp {:.1}, output {:.1} kW, junction {}, machines powered {}",
        reac.power_on(),
        reac.temperature(),
        reac.output_kw(),
        if room.junction().condition().functioning() { "working" } else { "broken" },
        machines_powered(w)
    )
}

fn fires(w: &World) -> Vec<FireNew> {
    items(w).fire.as_ref().map(|f| f.fires.clone()).unwrap_or_default()
}

fn sprays(w: &World) -> usize {
    items(w).fire.as_ref().map_or(0, |f| f.sprays.len())
}

fn ring(w: &World) -> Option<&crate::repair::Play> {
    w.resource::<RepairPlay>().0.as_ref()
}

fn sessions(w: &World) -> usize {
    w.resource::<RepairSessions>().sessions.len()
}

/// The screen's edges now (the HUD's burn, bad air and radiation alphas).
fn edges(w: &World) -> effects::ScreenFx {
    let v = w.resource::<Vitals>();
    effects::screen_fx(v.burning, v.oxygen, v.radiation, w.resource::<Time>().elapsed_secs_f64())
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

/// Waits `secs` of game time, with a note of the reactor's temperature.
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

/// Waits `secs`, then checks `ok` (the step's states are the checks).
fn wait_check(name: &'static str, secs: f64, what: &'static str, ok: fn(&World) -> bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            return false;
        }
        if c.t < secs {
            return false;
        }
        check(c, ok(w), what.to_string());
        end(w, c, format!("{:.1} s", c.t));
        true
    })
}

/// Waits until `done` holds (a PASS with the time), or fails at `limit` seconds.
fn until(name: &'static str, limit: f64, what: &'static str, done: fn(&World) -> bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            return false;
        }
        if done(w) {
            check(c, true, format!("{what} ({:.1} s)", c.t));
            end(w, c, format!("{:.1} s", c.t));
            return true;
        }
        if c.t >= limit {
            check(c, false, format!("{what}: not within {limit} s"));
            end(w, c, "timed out".into());
            return true;
        }
        false
    })
}

/// Takes the walker out of a display it uses (F leaves), then waits for the hands to be free.
fn leave_display(name: &'static str) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            if active(w).using().is_some() {
                tap(w, KeyCode::KeyF);
            }
            return false;
        }
        if c.t < 0.3 {
            return false;
        }
        end(w, c, "free".into());
        true
    })
}

/// Selects the inventory slot that holds an item (as the repair scenario does), and checks it.
fn select_item(name: &'static str, def: &'static str) -> Step {
    let mut slot = 0usize;
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == def)).unwrap_or(0);
            tap(w, DIGITS[slot]);
            return false;
        }
        if c.t < 0.3 {
            return false;
        }
        let ok = items(w).client.view.selected == slot;
        if !ok && c.t < 2.0 {
            return false;
        }
        check(c, ok, format!("{name}: slot {} selected", slot + 1));
        end(w, c, format!("slot {}", slot + 1));
        true
    })
}

/// True once per tag: a check in a step that runs every tick is made on the first tick only.
fn first(c: &mut Ctx, tag: &'static str) -> bool {
    if c.v.contains_key(tag) {
        return false;
    }
    c.v.insert(tag, 1.0);
    true
}

/// The critical lamp: the walker 2.5 m north of the reactor, the lamp shot in a lit half-second of
/// its 1 Hz blink. Fails when the reactor is not critical within `limit` s.
fn critical_lamp(dir: PathBuf, windowed: bool, limit: f64) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice c: the reactor heats past its allowed band (critical lamp)");
            let at = ground(w, [REACTOR_AT[0], REACTOR_AT[1] + 2.5], 0.0);
            place_walker(w, at);
            return false;
        }
        let target = ground(w, REACTOR_AT, 1.8);
        aim(w, target);
        let critical = effects::is_critical(items(w).reactor.as_ref());
        if !critical {
            if c.t >= limit {
                check(c, false, format!("reactor: critical within {limit} s (temperature {:.1})", sim(w).room.reactor().temperature()));
                end(w, c, "not critical".into());
                return true;
            }
            return false;
        }
        let lit = effects::critical_lamp_lit(true, w.resource::<Time>().elapsed_secs_f64());
        if c.t < 0.5 || !lit {
            return false;
        }
        check(c, critical, "reactor: the critical lamp is lit".into());
        snap_once(c, w, &dir, windowed, "slice_c_critical_lamp");
        end(w, c, format!("critical after {:.1} s", c.t));
        true
    })
}

/// Aims the walker at the coolant pump (the repair's reach is 1.8 m; the walker stands 1.4 m off).
fn at_pump(w: &mut World, lift: f64) {
    let target = ground(w, COOLANT_PUMP_AT, lift);
    aim(w, target);
}

pub(super) fn slice_look_steps(s: &mut Vec<Step>, out_dir: &Path, windowed: bool) {
    let dir = out_dir.to_path_buf();
    let (hot_dir, booth_dir, barrel_dir, junction_dir) = (out_dir.to_path_buf(), out_dir.to_path_buf(), out_dir.to_path_buf(), out_dir.to_path_buf());
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

    // The pump breaks from the dev faults button (the ask), the reactor heats; the readout is shot
    // once it reads HOT.
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
            check(c, readout_status(w).as_deref() == Some("HOT"), format!("slice c: the readout reads HOT (it says {:?})", readout_status(w)));
            end(w, c, format!("status {:?} after {:.1} s", readout_status(w), c.t));
            return true;
        }
        false
    }));
    let hot_readout_dir = dir.clone();
    s.push(Box::new(move |w, c| {
        if c.t < 0.5 {
            return false;
        }
        snap_once(c, w, &hot_readout_dir, windowed, "slice_c_readout_hot");
        c.t > 1.0
    }));

    // The critical lamp on the reactor's top (above the allowed band), then SCRAM at the readout:
    // the walker uses the readout, the click is retried until the reactor is off.
    s.push(critical_lamp(dir.clone(), windowed, 60.0));
    let critical_dir = dir.clone();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice c: the readout while critical");
            let at = ground(w, [-11.8, -17.76], 0.0);
            place_walker(w, at);
            return false;
        }
        let rt = screen_centre(w, &READOUT_ID);
        aim(w, rt);
        if c.t < 0.5 {
            return false;
        }
        if first(c, "critical_readout_checked") {
            check(c, effects::is_critical(items(w).reactor.as_ref()), "slice c: the readout is critical for its shot".into());
        }
        snap_once(c, w, &critical_dir, windowed, "slice_c_readout_critical");
        if c.t < 0.9 {
            return false;
        }
        end(w, c, "critical readout".into());
        true
    }));
    use_display(s, "REACTOR", READOUT_ID, [-11.8, -17.7]);
    s.push(click_until(READOUT_ID, "scram", None, "slice: SCRAM at the readout cuts the power", |w| !sim(w).room.reactor().power_on()));
    s.push(wait_check("slice: 2 s after SCRAM, the lights are dark", 2.0, "slice: the machines have no power after SCRAM", |w| !machines_powered(w) && !sim(w).room.reactor().power_on()));
    s.push(leave_display("slice: leave the readout after SCRAM"));

    // d. After SCRAM: the room and the pump screen (NO POWER, lights off).
    let dir_d = out_dir.to_path_buf();
    s.push(view("slice d: the room after SCRAM", [DOOR_AT[0], -12.4], Look::Ground([-13.6, -17.0], 1.0), Some("slice_d_scram_room"), dir_d.clone(), windowed));
    s.push(view("slice d: the pump screen after SCRAM", [COOLANT_PUMP_AT[0], -15.75], Look::Screen(PUMP_ID), Some("slice_d_scram_pump_screen"), dir_d, windowed));

    // POWER on at the readout, then the reactor settles before the fire and the repair.
    use_display(s, "REACTOR", READOUT_ID, [-11.8, -17.7]);
    s.push(click_until(READOUT_ID, "power", None, "slice: POWER on at the readout brings the reactor back", |w| sim(w).room.reactor().power_on()));
    s.push(leave_display("slice: leave the readout after POWER"));
    s.push(until("slice: the reactor runs again", 60.0, "slice: the reactor on and the machines powered", |w| sim(w).room.reactor().power_on() && machines_powered(w)));

    // e. The table and the extinguisher, then a fire at the reactor: the walker 2.5 m north of it,
    // the extinguisher held and sprayed at it (the steam where it lands).
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
        if c.t < 6.0 {
            return false;
        }
        let f = fires(w);
        check(c, !f.is_empty(), format!("slice e: a fire exists ({} fires)", f.len()));
        let start = crate::dev_area::engine_room::ignition_at(reactor_core::Source::Reactor);
        let at_reactor = |p: [f64; 2]| (p[0] - start[0]).hypot(p[1] - start[1]) < 1.0;
        check(c, f.iter().all(|x| at_reactor(x.pos)), format!("slice e: the fire starts in front of the reactor (within 1 m of {:?}: {:?})", start, f.iter().map(|x| x.pos).collect::<Vec<_>>()));
        end(w, c, format!("{} fires", f.len()));
        true
    }));
    let fire_dir = out_dir.to_path_buf();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice e: the fire at 2.5 m");
            let at = ground(w, [REACTOR_AT[0], REACTOR_AT[1] + 2.5], 0.0);
            place_walker(w, at);
        }
        let target = ground(w, REACTOR_AT, 0.3);
        aim(w, target);
        if c.t < 0.5 {
            return false;
        }
        snap_once(c, w, &fire_dir, windowed, "slice_e_fire_2_5m");
        c.t >= 0.9
    }));
    let spray_dir = out_dir.to_path_buf();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice e: the extinguisher sprayed at the fire from 2.5 m");
        }
        let target = ground(w, REACTOR_AT, 0.3);
        aim(w, target);
        w.resource_mut::<Controls>().primary_held = true;
        if c.t < 1.2 {
            return false;
        }
        if first(c, "spray_checked") {
            check(c, sprays(w) > 0, "slice e: a spray is on the fire (the steam comes where it lands)".into());
        }
        snap_once(c, w, &spray_dir, windowed, "slice_e_spray_steam");
        if c.t < 1.6 {
            return false;
        }
        w.resource_mut::<Controls>().primary_held = false;
        end(w, c, "sprayed 1.6 s".into());
        true
    }));

    // f. The repair ring: the extinguisher down, the wrench from the table, the dev fault breaks the
    // coolant pump again; smoke above it, then the wrench's hold session (the pump rises while held).
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
        if c.t < 1.0 {
            return false;
        }
        check(c, pump_share(w) < 0.5, format!("slice f: the pump is below half condition ({:.2})", pump_share(w)));
        end(w, c, format!("pump {:.2}", pump_share(w)));
        true
    }));
    let smoke_dir = out_dir.to_path_buf();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice f: smoke above the broken pump");
            let feet = ground(w, [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 1.4], 0.0);
            place_walker(w, feet);
        }
        at_pump(w, 0.9);
        if c.t < 0.5 {
            return false;
        }
        let level = effects::smoke_level(pump_share(w));
        if first(c, "smoke_checked") {
            check(c, level != Smoke::None, format!("slice f: the pump smokes below half condition ({:?} at {:.2})", level, pump_share(w)));
        }
        snap_once(c, w, &smoke_dir, windowed, "slice_f_pump_smoke");
        if c.t < 0.8 {
            return false;
        }
        end(w, c, format!("{level:?}; {}", power_why(w)));
        true
    }));
    // The wrench at the pump: LMB starts the session (retried every 0.5 s until the ring is on,
    // the first click may come before the snapshot says the pump is below full).
    let ring_dir = out_dir.to_path_buf();
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice f: the repair ring on the broken pump");
            c.v.insert("tries", 0.0);
            c.v.insert("last", -1.0);
        }
        at_pump(w, 0.6);
        if ring(w).is_some() {
            // The hold repair: the minigame is off at this condition, so the ring has no zones, and
            // the pump rises while the session is held.
            let t0 = *c.v.entry("ring_t0").or_insert(c.t);
            let s0 = *c.v.entry("ring_share0").or_insert(pump_share(w));
            if c.t - t0 >= 1.0 {
                snap_once(c, w, &ring_dir, windowed, "slice_f_repair_ring");
            }
            if c.t - t0 < 1.5 {
                return false;
            }
            if first(c, "ring_checked") {
                check(c, sessions(w) == 1, "slice f: the repair session runs, host and client".into());
                check(c, pump_share(w) > s0 + 0.02, format!("slice f: the pump rises while held ({s0:.3} to {:.3})", pump_share(w)));
            }
            end(w, c, format!("held, pump {:.2}", pump_share(w)));
            return true;
        }
        let (tries, last) = (c.v["tries"], c.v["last"]);
        if c.t >= 0.5 && c.t >= last + 0.5 && tries < 6.0 {
            w.resource_mut::<Controls>().click = true;
            c.v.insert("tries", tries + 1.0);
            c.v.insert("last", c.t);
        }
        if c.t >= 5.0 {
            check(c, false, format!("slice f: no ring within 5 s ({} clicks)", tries as u32));
            end(w, c, "no ring".into());
            return true;
        }
        false
    }));

    // h. The engines and the lights off, the reactor on manual at 100 % fission and turbine (the core
    // harness's overvolt_turbine_max, the manual set at 10 s): the supply runs above the demand, so
    // the junction box is overvolted and throws sparks (the stand is in the room, 1.2 m off the box).
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice h: the engines and lights off, the reactor at 100 % on manual");
            let mut items = w.resource_mut::<Items>();
            items.ask_reactor(ReactorAsk::SetBreaker(BreakerGroup::Engines, false));
            items.ask_reactor(ReactorAsk::SetBreaker(BreakerGroup::Lights, false));
            let at = ground(w, [JUNCTION_BOX_AT[0], JUNCTION_BOX_AT[1] + 1.2], 0.0);
            place_walker(w, at);
            return false;
        }
        let target = ground(w, JUNCTION_BOX_AT, 0.5);
        aim(w, target);
        if c.t >= 10.0 && first(c, "manual_asked") {
            let mut items = w.resource_mut::<Items>();
            items.ask_reactor(ReactorAsk::SetManual(true));
            items.ask_reactor(ReactorAsk::SetFission(100.0));
            items.ask_reactor(ReactorAsk::SetTurbine(100.0));
        }
        if !overvolt(w) && c.t < 30.0 {
            return false;
        }
        if first(c, "overvolt_checked") {
            check(c, overvolt(w), format!("slice h: the junction box is overvolted ({})", overvolt(w)));
        }
        if c.t < 1.0 {
            return false;
        }
        snap_once(c, w, &junction_dir, windowed, "slice_h_sparks_1");
        if c.t < 2.6 {
            return false;
        }
        snap_once(c, w, &junction_dir, windowed, "slice_h_sparks_2");
        if c.t < 3.0 {
            return false;
        }
        end(w, c, "overvolted".into());
        true
    }));

    // g. The HUD. The walker stands inside each zone (the effect needs it), 1 m short of the middle,
    // and looks past it at the storage strip's north wall (the edge of the area, y 20 m).
    let wall = |z: [f64; 2]| [z[0], z[1] + 3.5];
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice g: the hot plate, burn and catching fire");
            let at = ground(w, [HOT_PLATE.centre[0], HOT_PLATE.centre[1] - 1.0], 0.0);
            place_walker(w, at);
            return false;
        }
        let target = ground(w, wall(HOT_PLATE.centre), 1.5);
        aim(w, target);
        // Catching fire takes about 4.5 s on the plate (health scenario): the picture after it.
        if !w.resource::<Vitals>().burning && c.t < 8.0 {
            return false;
        }
        let burning = w.resource::<Vitals>().burning;
        if first(c, "burn_checked") {
            check(c, burning, format!("slice g: on fire on the hot plate ({burning}) after {:.1} s", c.t));
            check(c, edges(w).burn > 0.0, "slice g: the burn's screen edge is on".into());
        }
        if c.t < 0.5 {
            return false;
        }
        snap_once(c, w, &hot_dir, windowed, "slice_g_burn_fire");
        end(w, c, format!("burning {burning}"));
        true
    }));
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice g: the no-air booth, low oxygen");
            let at = ground(w, [NO_AIR.centre[0], NO_AIR.centre[1] - 1.0], 0.0);
            place_walker(w, at);
            return false;
        }
        let target = ground(w, wall(NO_AIR.centre), 1.5);
        aim(w, target);
        let o2 = w.resource::<Vitals>().oxygen;
        if !o2.is_some_and(|o| o < 0.5) && c.t < 30.0 {
            return false;
        }
        if first(c, "o2_checked") {
            check(c, o2.is_some_and(|o| o < 0.5), format!("slice g: the air runs out in the booth ({o2:?})"));
            check(c, edges(w).vignette > 0.0 && edges(w).dark > 0.0, "slice g: the dark edge of low oxygen is on".into());
        }
        if c.t < 0.5 {
            return false;
        }
        snap_once(c, w, &booth_dir, windowed, "slice_g_low_o2");
        end(w, c, format!("oxygen {o2:?}"));
        true
    }));
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "slice g: the glowing barrel, radiation");
            let at = ground(w, [GLOW_BARREL.centre[0], GLOW_BARREL.centre[1] - 1.0], 0.0);
            place_walker(w, at);
            return false;
        }
        let target = ground(w, wall(GLOW_BARREL.centre), 1.5);
        aim(w, target);
        let rad = w.resource::<Vitals>().radiation;
        if rad <= 0.25 && c.t < 40.0 {
            return false;
        }
        if first(c, "rad_checked") {
            check(c, rad > 0.25, format!("slice g: radiation above a quarter at the barrel ({rad:.2})"));
            check(c, edges(w).grain > 0.0, "slice g: the radiation grain is on".into());
        }
        if c.t < 0.5 {
            return false;
        }
        snap_once(c, w, &barrel_dir, windowed, "slice_g_radiation");
        end(w, c, format!("radiation {rad:.2}"));
        true
    }));
}
