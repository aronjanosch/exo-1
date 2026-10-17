//! The reactor room's physical parts (#316): SCRAM cover and button, core eject cover and lever, and the canister
//! slots, all through the look-and-F path (`Controls` taps, the view aimed at the part), no screens.
use super::*;
use super::items::{items, look_at};
use crate::controls_kit::Machine;
use crate::dev_area::engineering::canisters::canister_stack;
use crate::dev_area::engineering::reactor_room::{self as rr, ReactorRoom};
use crate::dev_area::layout::REACTOR_FRONT_AT;
use crate::dev_area::ship::{ReactorCounts, ReactorSim};
use crate::dev_area::DevArea;
use crate::items::{Items, LOCAL};
use item_core::wire::ReactorAsk;

const DIGITS: [KeyCode; 8] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];

fn part_at(w: &mut World, name: &str) -> DVec3 {
    let mut q = w.query::<(&ReactorRoom, &Machine)>();
    let (r, m) = q.single(w).expect("the reactor room is there");
    r.part_at(rr::part_index(name).unwrap(), m)
}

fn cover_open(w: &mut World, name: &str) -> bool {
    let mut q = w.query::<&Machine>();
    q.single(w).expect("the reactor room is there").controls.is_open(name)
}

fn seated(w: &mut World, name: &str) -> bool {
    let mut q = w.query::<&Machine>();
    q.single(w).expect("the reactor room is there").controls.seated(name).is_some()
}

/// Puts the walker's feet at a spot of the model's floor (model metres, glTF axes).
fn stand_at(w: &mut World, model: [f64; 3]) {
    let mut q = w.query::<&ReactorRoom>();
    let feet = q.single(w).expect("the reactor room is there").world(model);
    place_walker(w, feet);
}

fn aim(w: &mut World, name: &str) {
    let at = part_at(w, name);
    look_at(w, at);
}

fn sim(w: &World) -> &ReactorSim {
    w.resource::<ReactorSim>()
}

fn lives(w: &World) -> f64 {
    sim(w).room.reactor().canister_lives().iter().sum()
}

/// A step that runs `body` each tick until it answers `Some(note)`, with a time limit.
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

/// Aims at the part, taps F once at the start and lets `after` s pass before `done` reads the result.
fn press(name: &'static str, part: &'static str, after: f64, mut done: impl FnMut(&mut World, &mut Ctx) -> String + Send + Sync + 'static) -> Step {
    run(name, after + 3.0, move |w, c| {
        aim(w, part);
        if c.t == 0.0 {
            // The prompt names the part looked at (the cover when the part is under a shut cover).
            tap(w, KeyCode::KeyF);
            return None;
        }
        (c.t >= after).then(|| done(w, c))
    })
}

pub(super) fn reactor_room_steps(s: &mut Vec<Step>) {
    s.push(run("reactor room: the room and the walker in front of it", 6.0, |w, c| {
        let ready = w.resource::<DevArea>().frame.is_some() && w.query::<&ReactorRoom>().iter(w).count() == 1;
        if !ready {
            return None;
        }
        let pl = planet(w);
        let feet = w.resource::<DevArea>().ground(&pl, REACTOR_FRONT_AT, 0.0).expect("the dev area is there").0;
        place_walker(w, feet);
        check(c, true, "reactor room: the machine stands in the engine room".into());
        Some("at the reactor".into())
    }));
    s.push(run("reactor room: settle", 1.0, |w, c| {
        aim(w, "button_scram");
        (c.t >= 0.5).then(|| "settled".into())
    }));

    // A canister in a slot feeds the sim. The feed bay is on the model's +X flank, out of reach from the front.
    s.push(run("reactor room: the walker goes to the feed bay", 2.0, |w, c| {
        if c.t == 0.0 {
            stand_at(w, [2.4, 0.0, 0.0]);
        }
        aim(w, "slot_2");
        (c.t >= 0.5).then(|| "at the bay".into())
    }));
    // A canister in a slot feeds the sim. The reactor starts with two; one leaves, one comes back from the hands through slot 2.
    s.push(run("reactor room: one canister out, one in the hands", 3.0, |w, c| {
        if c.t == 0.0 {
            let canister = w.resource_mut::<ReactorSim>().room.reactor_mut().remove_canister(0).expect("two canisters at the start");
            c.v.insert("fill0", sim(w).room.reactor().available_fill());
            let def = items(w).defs.get("antimatter_canister").cloned().expect("the canister item");
            let now = 0.0;
            let ok = w.resource_mut::<Items>().give(now, LOCAL, canister_stack(&def, canister.life)).is_ok();
            check(c, ok, "reactor room: the canister is in the bar".into());
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        let slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == "antimatter_canister")).unwrap_or(0);
        if items(w).client.view.selected != slot {
            tap(w, DIGITS[slot]);
            return None;
        }
        check(c, seated(w, "slot_1") && !seated(w, "slot_2"), "reactor room: slot 1 shows the canister left, slot 2 is free".into());
        Some("canister selected".into())
    }));
    s.push(press("reactor room: F on slot 2 loads the canister", "slot_2", 0.8, |w, c| {
        check(c, sim(w).room.reactor().canisters().len() == 2, format!("reactor room: the reactor holds two canisters ({})", sim(w).room.reactor().canisters().len()));
        check(c, sim(w).room.reactor().available_fill() > c.v["fill0"], "reactor room: the fill rose, the sim uses the canister".into());
        check(c, items(w).client.view.count("antimatter_canister") == 0, "reactor room: the canister left the bar".into());
        check(c, seated(w, "slot_1") && seated(w, "slot_2"), "reactor room: both slots show a canister".into());
        "loaded".into()
    }));

    s.push(run("reactor room: the walker goes back to the front", 2.0, |w, c| {
        if c.t == 0.0 {
            stand_at(w, [0.0, 0.0, 2.6]);
        }
        aim(w, "button_scram");
        (c.t >= 0.5).then(|| "at the console".into())
    }));

    // SCRAM: the button answers only with its cover open, and it vents the chamber.
    s.push(run("reactor room: injection runs (manual 60)", 25.0, |w, c| {
        if c.t == 0.0 {
            let mut i = w.resource_mut::<Items>();
            i.ask_reactor(ReactorAsk::SetManual(true));
            i.ask_reactor(ReactorAsk::SetInjection(60.0));
            return None;
        }
        (sim(w).room.reactor().injection() > 5.0).then(|| {
            c.v.insert("lives0", lives(w));
            format!("injection {:.1}", sim(w).room.reactor().injection())
        })
    }));
    s.push(run("reactor room: a press on the button with the cover shut does nothing", 2.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Press(1));
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        check(c, w.resource::<ReactorCounts>().scrams == 0 && sim(w).room.reactor().power_on() && sim(w).room.reactor().injection() > 5.0, "reactor room: shut cover, no SCRAM".into());
        Some("covered".into())
    }));
    s.push(press("reactor room: F on the covered button opens the cover", "button_scram", 0.5, |w, c| {
        check(c, cover_open(w, "cover_scram"), "reactor room: the SCRAM cover is open".into());
        check(c, items(w).reactor.is_some_and(|r| r.scram_cover), "reactor room: the news show the cover open".into());
        "open".into()
    }));
    s.push(press("reactor room: F on the button SCRAMs", "button_scram", 0.6, |w, c| {
        let r = sim(w).room.reactor();
        check(c, w.resource::<ReactorCounts>().scrams == 1, "reactor room: one SCRAM".into());
        check(c, r.injection() == 0.0 && !r.power_on(), format!("reactor room: injection stopped at once ({:.2}), power off", r.injection()));
        let (before, after) = (c.v["lives0"], lives(w));
        let share = sim(w).room.reactor().tuning().scram_fill_loss;
        check(c, after < before && (after - before * (1.0 - share)).abs() < before * 0.01, format!("reactor room: the SCRAM cost its share of the fill ({before:.1} -> {after:.1})"));
        check(c, !cover_open(w, "cover_scram"), "reactor room: the machine closed the SCRAM cover".into());
        "scrammed".into()
    }));
    s.push(run("reactor room: it stays off", 2.0, |w, c| {
        if c.t < 1.0 {
            return None;
        }
        check(c, sim(w).room.reactor().injection() == 0.0, "reactor room: no injection a second later".into());
        Some("off".into())
    }));

    // Core eject: only with the cover open.
    s.push(run("reactor room: a pull with the eject cover shut does nothing", 2.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Press(3));
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        check(c, !sim(w).room.reactor().ejected(), "reactor room: shut cover, the core stays".into());
        Some("covered".into())
    }));
    s.push(press("reactor room: F on the lever under the shut cover opens the cover", "lever_eject", 0.5, |w, c| {
        check(c, cover_open(w, "cover_eject") && !sim(w).room.reactor().ejected(), "reactor room: the eject cover is open, the core is still in".into());
        "open".into()
    }));
    s.push(press("reactor room: F on the lever ejects the core", "lever_eject", 0.6, |w, c| {
        check(c, sim(w).room.reactor().ejected() && sim(w).room.reactor().canisters().is_empty(), "reactor room: the core is out, the canisters with it".into());
        check(c, items(w).reactor.is_some_and(|r| r.ejected && r.eject_cover), "reactor room: the news show it".into());
        "ejected".into()
    }));
}
