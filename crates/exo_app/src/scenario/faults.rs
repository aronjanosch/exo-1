//! Faults (#328): a junction box breaks, its part of the ship goes dark, a blown box refuses repair until a fuse is fitted, then the
//! repair brings the lights back; and the director on a short first wait opens exactly one incident at a time.
use super::items::{collect, items, outside, prompt};
use super::*;
use crate::dev_area::engineering::junction_boxes::box_passes;
use crate::dev_area::equipment::box_power;
use crate::dev_area::fire::{FireGrid, FireModel};
use crate::dev_area::hull::ShipAir;
use crate::dev_area::layout::{box_of_point, junction_centre, junction_front, BRIDGE, CORRIDOR, EQUIPMENT, JUNCTION_BOX_SIZE, JUNCTION_LIFT, ROOM};
use crate::dev_area::ship::{fire_count, situation_of, ReactorCounts, ReactorSim, ShipEvents};
use crate::dev_area::{dev::item_table, DevArea, DEV};
use crate::items::{Items, LOCAL};
use crate::repair::{RepairPlay, RepairSessions};
use item_core::wire::{RepairAsk, RepairDevice, RepairTool};
use reactor_core::director::DirectorTuning;
use reactor_core::Director;
use walker_core::Frame as WalkFrame;

const DIGITS: [KeyCode; 8] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];
/// The box the scenario breaks: JB-2, the corridor's.
const BOX: usize = 1;

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

fn look_box(w: &mut World) {
    let target = ground(w, junction_centre(BOX), JUNCTION_LIFT + JUNCTION_BOX_SIZE[1] * 0.5);
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

fn fuses(w: &World) -> u32 {
    items(w).client.view.count("fuse")
}

fn news_passes(w: &World, i: usize) -> bool {
    box_passes(items(w).reactor.as_ref(), i)
}

fn blown(w: &World, i: usize) -> bool {
    w.resource::<ReactorSim>().room.junction(i).blown()
}

fn run(name: &'static str, limit: f64, mut body: impl FnMut(&mut World, &mut Ctx) -> Option<String> + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
        }
        if let Some(note) = body(w, c) {
            end(w, c, note);
            return true;
        }
        if c.t > limit {
            check(c, false, format!("{name}: not done within {limit} s"));
            end(w, c, "timed out".into());
            return true;
        }
        false
    })
}

fn select(name: &'static str, def: &'static str) -> Step {
    let mut slot = 0;
    run(name, 2.0, move |w, c| {
        if c.t == 0.0 {
            slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == def)).unwrap_or(0);
            tap(w, DIGITS[slot]);
            return None;
        }
        (items(w).client.view.selected == slot).then(|| {
            check(c, items(w).client.view.slot(slot).is_some_and(|s| s.item.kind.as_str() == def), format!("{name}: slot {} holds the {def}", slot + 1));
            format!("slot {}", slot + 1)
        })
    })
}

fn open_incidents(w: &World) -> bool {
    let sim = w.resource::<ReactorSim>();
    let fires = fire_count(w.resource::<ShipAir>(), w.resource::<FireGrid>(), w.resource::<FireModel>());
    situation_of(sim, fires).busy()
}

pub(super) fn faults_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        begin(w, c, "faults: the table and the host's body for the walker");
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && outside(w, "screwdriver").is_some() && w.resource::<crate::health::HealthState>().bodies.contains_key(&LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "faults: the table and the host's body for the walker are there".into());
        check(c, !w.resource::<ReactorSim>().director.enabled(), "faults: the scenario starts with the director off".into());
        let p = ground(w, DEV.point([item_table::AT[0], item_table::AT[1] - 1.3]), 0.0);
        place_walker(w, p);
        end(w, c, "in front of the table".into());
        true
    }));
    s.extend(collect("screwdriver", "Screwdriver"));
    s.push(select("faults: the screwdriver is selected", "screwdriver"));

    // (a) JB-2 breaks: its part of the ship goes dark, the others stay.
    s.push(run("faults: JB-2 breaks (sparks); the corridor, the equipment room and the airlock go dark, the engine room and the bridge do not", 6.0, |w, c| {
        if c.t == 0.0 {
            c.v.insert("sparks0", w.resource::<ReactorCounts>().sparks as f64);
            let events = w.resource_mut::<ReactorSim>().room.blow_junction(BOX, false);
            w.resource_mut::<ShipEvents>().0.extend(events);
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        check(c, blown(w, BOX), "faults: JB-2 is blown on the host".into());
        check(c, w.resource::<Items>().reactor.is_some_and(|n| n.junction_blown[BOX] && !n.junction_blown[0] && !n.junction_blown[2]), "faults: the news say JB-2 is blown, the others not".into());
        check(c, w.resource::<ReactorCounts>().sparks as f64 > c.v["sparks0"], "faults: the break threw sparks".into());
        let corridor = box_of_point(CORRIDOR.centre);
        let equipment = box_of_point(EQUIPMENT.centre);
        let engine = box_of_point(ROOM.centre);
        let bridge = box_of_point(BRIDGE.centre);
        check(c, corridor == BOX && equipment == BOX && !news_passes(w, corridor) && !news_passes(w, equipment), "faults: the corridor's and the equipment room's lights are out (emergency glow)".into());
        check(c, engine == 0 && news_passes(w, engine) && bridge == 2 && news_passes(w, bridge), "faults: the engine room's and the bridge's lights are on".into());
        check(c, !box_power(&w.resource::<ReactorSim>()), "faults: the canister boxes get no power from a dead JB-2".into());
        Some("dark corridor".into())
    }));

    // (b) The screwdriver is refused at the blown box.
    s.push(run("faults: the walker goes to JB-2", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, junction_front(BOX, 1.4), 0.0);
            place_walker(w, feet);
        }
        look_box(w);
        (c.t > 0.5).then(|| "at JB-2".into())
    }));
    // The look: JB-2 blown, the corridor in its emergency glow (a picture only in a window run; further back the view is a bare wall).
    s.push(run("faults: and close to the box again", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, junction_front(BOX, 1.4), 0.0);
            place_walker(w, feet);
        }
        look_box(w);
        (c.t > 0.6).then(|| "close".into())
    }));
    s.push(shot_step("faults-jb2-dark-near"));
    s.push(wait(0.5));
    s.push(run("faults: a repair try with the screwdriver is refused (no session, no ring), before and at the host", 6.0, |w, c| {
        look_box(w);
        if c.t < 0.3 {
            return None;
        }
        if !c.v.contains_key("lmb") {
            w.resource_mut::<Controls>().click = true;
            c.v.insert("lmb", c.t);
            return None;
        }
        if c.t - c.v["lmb"] < 0.5 {
            return None;
        }
        if !c.v.contains_key("ask") {
            check(c, w.resource::<RepairPlay>().0.is_none() && w.resource::<RepairSessions>().sessions.is_empty(), "faults: LMB at the blown box starts no repair".into());
            w.resource_mut::<Items>().repair_asks.push((LOCAL, RepairAsk::Start { device: RepairDevice::JunctionBox(BOX), tool: RepairTool::Screwdriver }));
            c.v.insert("ask", c.t);
            return None;
        }
        let told = w.resource::<Items>().repair_done;
        if !told && c.t - c.v["ask"] < 0.5 {
            return None;
        }
        check(c, told && w.resource::<RepairSessions>().sessions.is_empty(), "faults: the host refuses a start on the blown box".into());
        check(c, blown(w, BOX), "faults: still blown".into());
        let hint = prompt(w);
        check(c, hint.contains("JB-2") && hint.contains("needs a fuse"), format!("faults: the prompt at the box says a fuse is needed ('{hint}')"));
        Some("refused".into())
    }));

    // (c) A fuse goes in; the repair follows and the lights are back.
    s.push(run("faults: two fuses for the walker; with one selected the prompt says fit fuse", 4.0, |w, c| {
        if c.t == 0.0 {
            let now = w.resource::<Time>().elapsed_secs_f64();
            let mut it = w.resource_mut::<Items>();
            let stack = it.defs.get("fuse").expect("fuse.json").stack_of(2);
            assert!(it.give(now, LOCAL, stack).is_ok(), "the fuses fit in the bar");
            return None;
        }
        look_box(w);
        if c.t < 0.3 {
            return None;
        }
        if !c.v.contains_key("sel") {
            let slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == "fuse")).unwrap_or(0);
            tap(w, DIGITS[slot]);
            c.v.insert("sel", slot as f64);
            return None;
        }
        if items(w).client.view.selected != c.v["sel"] as usize || c.t < 0.8 {
            return None;
        }
        check(c, fuses(w) == 2, format!("faults: two fuses in hand ({})", fuses(w)));
        let hint = prompt(w);
        check(c, hint.contains("fit fuse") && hint.contains("JB-2") && !hint.contains("needs a fuse"), format!("faults: the prompt with a fuse says fit fuse ('{hint}')"));
        Some("fuse selected".into())
    }));
    s.push(run("faults: F fits the fuse: the box is no longer blown, one fuse is used up, the lights stay out until repaired", 6.0, |w, c| {
        look_box(w);
        if c.t == 0.0 {
            tap(w, KeyCode::KeyF);
            return None;
        }
        if blown(w, BOX) || c.t < 0.3 {
            return (c.t > 4.0).then(|| "no fuse fitted".into());
        }
        if c.t < 1.0 {
            return None;
        }
        check(c, fuses(w) == 1, format!("faults: exactly one fuse used up ({} left)", fuses(w)));
        check(c, !blown(w, BOX) && !w.resource::<ReactorSim>().room.junction(BOX).condition().functioning(), "faults: fitted but still broken".into());
        check(c, w.resource::<ReactorSim>().room.junction(BOX).repair_allowed().is_ok(), "faults: the repair is allowed now".into());
        check(c, !news_passes(w, BOX), "faults: the corridor is still dark".into());
        Some("fitted".into())
    }));
    s.push(select("faults: the screwdriver is selected again", "screwdriver"));
    s.push(run("faults: the screwdriver repairs the box; the lights and the canister boxes are back", 150.0, |w, c| {
        look_box(w);
        if w.resource::<RepairPlay>().0.is_none() && w.resource::<RepairSessions>().sessions.is_empty() && c.t > 0.5 && (c.t - c.v.get("lmb").copied().unwrap_or(-9.0)) > 1.0 && !news_passes(w, BOX) {
            w.resource_mut::<Controls>().click = true;
            c.v.insert("lmb", c.t);
            return None;
        }
        if !news_passes(w, BOX) {
            return None;
        }
        let corridor = box_of_point(CORRIDOR.centre);
        check(c, news_passes(w, corridor), "faults: the corridor's lights are back".into());
        check(c, box_power(&w.resource::<ReactorSim>()), "faults: the canister boxes have power again".into());
        check(c, fuses(w) == 1, format!("faults: the repair used no further fuse ({})", fuses(w)));
        Some(format!("repaired at {:.0} s", c.t))
    }));
    s.push(run("faults: a fuse on a box that is not blown is refused and the fuse stays", 3.0, |w, c| {
        if c.t == 0.0 {
            // As a second player at the same box would: the load names a box that is sound.
            let slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == "fuse")).unwrap_or(0);
            let now = w.resource::<Time>().elapsed_secs_f64();
            let _ = w.resource_mut::<Items>().request(now, item_core::Request::Load { slot, into: BOX as u8 });
            return None;
        }
        if c.t < 1.5 {
            return None;
        }
        check(c, fuses(w) == 1, format!("faults: the fuse came back ({})", fuses(w)));
        check(c, !blown(w, BOX), "faults: nothing changed at the box".into());
        Some("fuse kept".into())
    }));

    // (d) The director: one incident at a time.
    s.push(run("faults: the director on a short wait opens exactly one incident, and no second while it is open", 60.0, |w, c| {
        if c.t == 0.0 {
            let tuning = DirectorTuning { enabled: true, first_s: 3.0, min_interval_s: 4.0, max_interval_s: 6.0, fire_chance: 0.0 };
            w.resource_mut::<ReactorSim>().director = Director::new(&tuning, 17);
            c.v.insert("n0", (w.resource::<ReactorCounts>().sparks + w.resource::<ReactorCounts>().fires) as f64);
            c.v.insert("busy", 0.0);
            return None;
        }
        let k = w.resource::<ReactorCounts>();
        let n = (k.sparks + k.fires) as f64 - c.v["n0"];
        // A new incident may only start in a tick that began calm.
        let was_busy = c.v["busy"] > 0.0;
        if n > c.v.get("seen").copied().unwrap_or(0.0) {
            check(c, !was_busy, format!("faults: incident {n} started in a calm tick (none was open)"));
            c.v.insert("seen", n);
            c.v.insert("first_at", c.t);
        }
        c.v.insert("busy", open_incidents(w) as u8 as f64);
        let Some(first) = c.v.get("first_at").copied() else {
            if c.t > 10.0 {
                check(c, false, "faults: no incident within 10 s of a 3 s wait".into());
                return Some("none".into());
            }
            return None;
        };
        if c.t - first < 25.0 {
            return None;
        }
        // 25 s (more than the longest wait) after the first: while it is still open no second one came.
        if open_incidents(w) {
            check(c, n == 1.0, format!("faults: exactly one incident while it is open ({n})"));
        }
        check(c, n >= 1.0, "faults: an incident opened".into());
        Some(format!("{n} incident(s)"))
    }));
}
