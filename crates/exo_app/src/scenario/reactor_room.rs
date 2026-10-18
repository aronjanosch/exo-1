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
use super::reactor::{click_until, use_display};

const DIGITS: [KeyCode; 8] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];

fn part_at(w: &mut World, name: &str) -> DVec3 {
    let mut q = w.query::<(&ReactorRoom, &Machine)>();
    let (r, m) = q.single(w).expect("the reactor room is there");
    r.part_at(rr::part_index(name).unwrap(), m)
}

fn cover_open(w: &mut World, name: &str) -> bool {
    let mut q = w.query_filtered::<&Machine, With<ReactorRoom>>();
    q.single(w).expect("the reactor room is there").controls.is_open(name)
}

fn seated(w: &mut World, name: &str) -> bool {
    let mut q = w.query_filtered::<&Machine, With<ReactorRoom>>();
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

/// Slot `slot` (0 is slot_1) of the sim's reactor holds a canister.
fn seated_in_sim(w: &World, slot: usize) -> bool {
    sim(w).room.reactor().slot_taken(slot)
}

/// An antimatter canister with `life` goes into the bar.
fn give_canister(w: &mut World, life: f64) {
    let def = items(w).defs.get("antimatter_canister").cloned().expect("the canister item");
    let ok = w.resource_mut::<Items>().give(0.0, LOCAL, canister_stack(&def, life)).is_ok();
    assert!(ok, "the canister fits the bar");
}

/// The words on the supply screen (`screen_supply`).
fn supply_words(w: &World) -> Vec<String> {
    use display_core::{Shape, Widget};
    let panels = w.resource::<crate::displays::Panels>();
    let Some(p) = panels.get(&crate::dev_area::engineering::reactor_room::SUPPLY_ID) else { return Vec::new() };
    p.layout().iter().flat_map(|l| match l.widget {
        Widget::Canvas { shapes, .. } => shapes.iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect::<Vec<_>>(),
        _ => Vec::new(),
    }).collect()
}

/// Selects the bar slot of a canister (one key tap per tick); true once it is selected.
fn select_canister(w: &mut World) -> bool {
    let slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == "antimatter_canister")).unwrap_or(0);
    if items(w).client.view.selected != slot {
        tap(w, DIGITS[slot]);
        return false;
    }
    true
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

/// A picture (window runs only) of the model from where the walker stands, aimed at a point of the model.
fn view(tag: &'static str, model: [f64; 3], shot_step: &dyn Fn(&'static str) -> Step) -> Vec<Step> {
    let mut left = 1.0;
    vec![
        Box::new(move |w, c| {
            let at = {
                let mut q = w.query::<&ReactorRoom>();
                q.single(w).expect("the reactor room is there").world(model)
            };
            look_at(w, at);
            left -= c.dt;
            left <= 0.0
        }),
        shot_step(tag),
    ]
}

pub(super) fn reactor_room_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
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

    s.extend(view("reactor-01-front", [0.0, 1.8, 0.0], shot_step));
    // A canister in a slot feeds the sim. The feed bay is on the model's +X flank, out of reach from the front.
    s.push(run("reactor room: the walker goes to the feed bay", 2.0, |w, c| {
        if c.t == 0.0 {
            stand_at(w, [2.4, 0.0, 0.0]);
        }
        aim(w, "slot_2");
        (c.t >= 0.5).then(|| "at the bay".into())
    }));
    // F loads the slot LOOKED AT, F with empty hands pulls the slot looked at (#316). Both canisters leave the reactor; one
    // (life 90) goes to the hands and is loaded at slot 2 while slot 1 is free.
    s.push(run("reactor room: both canisters out, one in the hands", 3.0, |w, c| {
        if c.t == 0.0 {
            let mut r = w.resource_mut::<ReactorSim>();
            let canister = r.room.reactor_mut().remove_canister(0).expect("two canisters at the start");
            r.room.reactor_mut().remove_canister(0).expect("two canisters at the start");
            c.v.insert("fill0", sim(w).room.reactor().available_fill());
            check(c, !seated_in_sim(w, 0) && !seated_in_sim(w, 1), "reactor room: the reactor is empty".into());
            let _ = canister;
            give_canister(w, 90.0);
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        select_canister(w).then(|| "canister selected".into())
    }));
    s.push(press("reactor room: F on slot 2 loads slot 2, slot 1 stays empty", "slot_2", 0.8, |w, c| {
        check(c, seated_in_sim(w, 1) && !seated_in_sim(w, 0), "reactor room: the canister sits in slot 2 of the sim, slot 1 is empty".into());
        check(c, sim(w).room.reactor().available_fill() > c.v["fill0"], "reactor room: the fill rose, the sim uses the canister".into());
        check(c, items(w).client.view.count("antimatter_canister") == 0, "reactor room: the canister left the bar".into());
        check(c, seated(w, "slot_2") && !seated(w, "slot_1"), "reactor room: only slot 2 shows a canister".into());
        "loaded".into()
    }));
    s.push(run("reactor room: a second canister (life 60) in the hands", 3.0, |w, c| {
        if c.t == 0.0 {
            give_canister(w, 60.0);
            return None;
        }
        (c.t >= 0.3 && select_canister(w)).then(|| "selected".into())
    }));
    s.push(press("reactor room: F on the taken slot 2 is refused, the canister stays in hand", "slot_2", 0.8, |w, c| {
        check(c, items(w).client.view.count("antimatter_canister") == 1, "reactor room: the canister is still in the bar".into());
        check(c, sim(w).room.reactor().canisters().len() == 1 && seated_in_sim(w, 1), "reactor room: slot 2 still holds its own canister, nothing else went in".into());
        check(c, (sim(w).room.reactor().canisters()[0].life - 90.0).abs() < 1.0, "reactor room: slot 2's canister is the one that was there".into());
        "refused".into()
    }));
    s.push(press("reactor room: F on slot 1 loads the second canister there", "slot_1", 0.8, |w, c| {
        check(c, seated_in_sim(w, 0) && seated_in_sim(w, 1), "reactor room: both slots of the sim hold one".into());
        check(c, items(w).client.view.count("antimatter_canister") == 0, "reactor room: the bar is empty".into());
        check(c, seated(w, "slot_1") && seated(w, "slot_2"), "reactor room: both slots show a canister".into());
        "both loaded".into()
    }));
    // Slot 2 holds more life (90) than slot 1 (60): F with empty hands at slot 1 takes slot 1's, not the most life.
    s.push(press("reactor room: F with empty hands on slot 1 pulls slot 1's canister, not the one with most life", "slot_1", 0.8, |w, c| {
        check(c, !seated_in_sim(w, 0) && seated_in_sim(w, 1), "reactor room: slot 1 is empty, slot 2 still holds its canister".into());
        let held = (0..8).filter_map(|i| items(w).client.view.slot(i)).find(|s| s.item.kind.as_str() == "antimatter_canister").and_then(|s| s.item.condition);
        check(c, held.is_some_and(|l| (l - 60.0).abs() < 1.0), format!("reactor room: the hands hold the 60 life canister ({held:?})"));
        check(c, supply_words(w).iter().any(|t| t == "EMPTY") && supply_words(w).iter().any(|t| t == "SEATED"), format!("reactor room: the supply screen shows slot 1 EMPTY and slot 2 SEATED ({:?})", supply_words(w)));
        "pulled slot 1".into()
    }));
    s.push(run("reactor room: the pulled canister is selected", 3.0, |w, c| (c.t >= 0.3 && select_canister(w)).then(|| "selected".into())));
    s.push(press("reactor room: F loads it back into slot 1", "slot_1", 0.8, |w, c| {
        check(c, seated_in_sim(w, 0) && seated_in_sim(w, 1), "reactor room: both slots hold a canister again".into());
        "loaded back".into()
    }));
    s.extend(view("reactor-02-bay-loaded", [1.4, 1.1, 0.0], shot_step));

    s.push(run("reactor room: the walker goes back to the front", 2.0, |w, c| {
        if c.t == 0.0 {
            stand_at(w, [0.0, 0.0, 2.6]);
        }
        aim(w, "button_scram");
        (c.t >= 0.5).then(|| "at the console".into())
    }));

    main_screen_steps(s, shot_step);

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
    s.extend(view("reactor-03-scram-cover-open", [0.5, 1.0, 1.7], shot_step));
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
    s.extend(view("reactor-04-scrammed", [0.0, 1.8, 0.0], shot_step));
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
    s.extend(view("reactor-05-ejected", [0.0, 1.2, 0.0], shot_step));
    s.push(wait(1.0));
}

/// The main screen (#317): F on it, MANUAL and the injection slider move the sim's set point, AUTO makes the converter slider a
/// no-op, F leaves. The shot (window runs) is the screen as the engineer sees it.
fn main_screen_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    use crate::dev_area::engineering::reactor_room::{MAIN_ID, MAIN_NAME};
    use display_core::reactor_main::{AUTO, CONVERTER, INJECTION, MANUAL};
    use crate::displays::ActiveDisplay;
    use crate::controls::Controls;
    use_display(s, MAIN_NAME, MAIN_ID, REACTOR_FRONT_AT);
    s.push(click_until(MAIN_ID, MANUAL, None, "reactor room: MANUAL on the main screen", |w| sim(w).manual));
    s.push(click_until(MAIN_ID, INJECTION, Some(0.7), "reactor room: the injection slider sets 70 on the main screen", |w| (sim(w).injection - 70.0).abs() < 1.5));
    s.push(wait(2.0));
    s.push(shot_step("reactor-06-main-manual"));
    s.push(click_until(MAIN_ID, AUTO, None, "reactor room: AUTO on the main screen", |w| !sim(w).manual));
    s.push(run("reactor room: in AUTO the converter slider does nothing", 4.0, |w, c| {
        let d = w.query::<&crate::displays::DisplayPanel>().iter(w).find(|d| d.id == MAIN_ID).cloned().expect("the main screen");
        if c.t == 0.0 {
            c.v.insert("conv0", sim(w).converter);
            let at = crate::displays::world_point_of(w, &d, CONVERTER, Some(0.1)).expect("the converter slider");
            crate::displays::aim_at(w, at);
            w.resource_mut::<Controls>().click = true;
            return None;
        }
        (c.t >= 1.5).then(|| {
            check(c, (sim(w).converter - c.v["conv0"]).abs() < 1e-9 && !sim(w).manual, format!("reactor room: the AUTO converter stays {:.1}", sim(w).converter));
            "ignored".into()
        })
    }));
    s.push(shot_step("reactor-07-main-auto"));
    s.push(run("reactor room: F leaves the main screen", 3.0, |w, c| {
        if c.t == 0.0 {
            tap(w, KeyCode::KeyF);
            return None;
        }
        (*w.resource::<ActiveDisplay>() == ActiveDisplay::Idle).then(|| "left".into())
    }));
}
