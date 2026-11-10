//! The fabricator (#326): ore goes into the input places through the container path, the physical
//! button is pushed through `Controls` (F on the button, looked at), the product lies in the output
//! place and the inputs are gone. A load that makes nothing is refused. A power cut pauses a run and it
//! resumes when the power is back. Every step polls for its outcome with a limit in simulated seconds.
use super::dig::until;
use super::items::{items, look_at, prompt, tap_then, DELAY};
use super::*;
use crate::dev_area::fabricator::{FabCfg, FabMachine, FabState, FAB_RACK as RACK, INPUTS, OUTPUT};
use crate::dev_area::ship::ReactorSim;
use crate::dev_area::DevArea;
use crate::items::{Items, LOCAL};
use item_core::container::ItemKind;
use item_core::wire::FabMode;
use item_core::Request;

const ORE: &str = "grumblite";
const PART: &str = "fuse";

fn machine(w: &mut World) -> FabMachine {
    let mut q = w.query::<&FabMachine>();
    *q.single(w).expect("the fabricator stands there")
}

fn rack_count(w: &World, kind: &str) -> u32 {
    items(w).host.racks.get(&RACK).map_or(0, |c| c.count(&ItemKind::new(kind)))
}

fn in_input(w: &World, i: usize) -> u32 {
    items(w).host.racks.get(&RACK).and_then(|c| c.slot(i)).map_or(0, |s| s.count)
}

fn bar_slot(w: &World, kind: &str) -> Option<usize> {
    (0..8).find(|i| items(w).client.view.slot(*i).is_some_and(|s| s.item.kind.as_str() == kind))
}

/// Ore and parts anywhere: bar, rack. Nothing may be lost or doubled.
fn total(w: &World, kind: &str) -> u32 {
    items(w).host.inventory(LOCAL).count(kind) + rack_count(w, kind)
}

fn fab(w: &World) -> &FabState {
    w.resource::<FabState>()
}

fn left_s(w: &World) -> Option<f64> {
    fab(w).fab.progress().map(|(_, l)| l)
}

/// The words the screen shows now.
fn screen_words(w: &World) -> Vec<String> {
    use display_core::{Shape, Widget};
    let panels = w.resource::<crate::displays::Panels>();
    let Some(p) = panels.get(&crate::dev_area::fabricator::SCREEN_ID) else { return Vec::new() };
    p.layout().iter().flat_map(|l| match l.widget {
        Widget::Canvas { shapes, .. } => shapes.iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text.clone()) } else { None }).collect::<Vec<_>>(),
        _ => Vec::new(),
    }).collect()
}

fn says(w: &World, word: &str) -> bool {
    screen_words(w).iter().any(|t| t == word)
}

fn ask(w: &mut World, req: Request) -> Result<u32, item_core::Reject> {
    let now = w.resource::<Time>().elapsed_secs_f64();
    w.resource_mut::<Items>().request(now, req)
}

fn give(w: &mut World, id: &str, n: u32) {
    let now = w.resource::<Time>().elapsed_secs_f64();
    let stack = items(w).defs.get(id).unwrap().stack_of(n);
    let _ = w.resource_mut::<Items>().give(now, LOCAL, stack);
}

/// Stands in front of the machine, a little to its button's side.
fn stand_before(w: &mut World) {
    let at = machine(w).world([0.3, 0.3, 1.4]);
    place_walker(w, at);
}

fn look_at_button(w: &mut World) {
    let at = machine(w).button_at;
    look_at(w, at);
}

/// The hatch's middle, a place where F opens the container.
fn look_at_hatch(w: &mut World) {
    let at = machine(w).world([-0.2, 0.4, 0.4]);
    look_at(w, at);
}

/// Opens the container window's host lock, stores the bar's ore into one input, closes again.
fn load(what: &'static str, rack_slot: usize) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            begin(w, c, what);
            let r = ask(w, Request::Open { rack: RACK });
            check(c, r.is_ok(), format!("{what}: the hatch opens ({r:?})"));
            true
        }),
        wait(3.0 * DELAY),
        Box::new(move |w, c| {
            let Some(slot) = bar_slot(w, ORE) else {
                check(c, false, format!("{what}: ore in the bar"));
                return true;
            };
            let r = ask(w, Request::StoreAt { slot, rack: RACK, rack_slot });
            check(c, r.is_ok(), format!("{what}: the store is accepted ({r:?})"));
            true
        }),
        wait(3.0 * DELAY + 0.2),
        Box::new(move |w, c| {
            let _ = ask(w, Request::Close { rack: RACK });
            end(w, c, "loaded".into());
            true
        }),
        wait(3.0 * DELAY),
    ]
}

/// Aims at the button, checks the prompt and taps F once.
fn push(what: &'static str) -> Step {
    Box::new(move |w, c| {
        look_at_button(w);
        if c.t == 0.0 {
            begin(w, c, what);
            return false;
        }
        if c.t < 0.2 {
            return false;
        }
        if !c.v.contains_key(what) {
            let p = prompt(w);
            check(c, p.contains("Fabricator start"), format!("{what}: the prompt names the button ({p:?})"));
            tap(w, KeyCode::KeyF);
            c.v.insert(what, 1.0);
            return false;
        }
        c.t > 0.8
    })
}

pub(super) fn fabricator_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "fabricator: the machine and its rack");
            w.resource_mut::<Items>().delay = DELAY;
        }
        let ready = w.resource::<DevArea>().frame.is_some() && items(w).client.racks.contains_key(&RACK) && w.query::<&FabMachine>().iter(w).count() == 1;
        if !ready {
            if c.t > 5.0 {
                check(c, false, "fabricator: the machine and its rack are there".into());
                return true;
            }
            return false;
        }
        give(w, ORE, 2);
        stand_before(w);
        let rack = items(w).client.racks.get(&RACK).cloned().unwrap();
        check(c, rack.len() == 3 && rack.rule(OUTPUT).take_only, "fabricator: two input places and an output place that only gives".into());
        true
    }));
    s.push(wait(0.5));

    // F on the housing opens the hatch; the screen asks for ore. F again closes it.
    s.push(Box::new(|w, c| {
        look_at_hatch(w);
        if c.t == 0.0 {
            begin(w, c, "fabricator: F on the housing opens the hatch");
        }
        if c.t < 0.3 {
            return false;
        }
        if !c.v.contains_key("tapped_hatch") {
            let p = prompt(w);
            check(c, p.contains("Fabricator") && !p.contains("start"), format!("fabricator: the prompt offers the hatch ({p:?})"));
            check(c, says(w, "LOAD ORE"), format!("fabricator: the screen says LOAD ORE ({:?})", screen_words(w)));
            tap(w, KeyCode::KeyF);
            c.v.insert("tapped_hatch", 1.0);
            return false;
        }
        if w.resource::<crate::container_window::ContainerWindow>().rack == Some(RACK) {
            end(w, c, "open".into());
            return true;
        }
        if c.t > 2.0 {
            check(c, false, "fabricator: the window opened".into());
            return true;
        }
        false
    }));
    s.push(tap_then("fabricator: F closes the window", KeyCode::KeyF, |w| w.resource::<crate::container_window::ContainerWindow>().rack.is_none()));
    s.push(wait(0.3));

    // The output place takes nothing from a hand: the ore for place 3 is refused.
    s.push(Box::new(|w, c| {
        begin(w, c, "fabricator: nothing goes into the output place");
        let _ = ask(w, Request::Open { rack: RACK });
        true
    }));
    s.push(wait(3.0 * DELAY));
    s.push(Box::new(|w, c| {
        let slot = bar_slot(w, ORE).unwrap_or(0);
        let r = ask(w, Request::StoreAt { slot, rack: RACK, rack_slot: OUTPUT });
        check(c, r.is_err(), format!("fabricator: the output place refuses a hand ({r:?})"));
        let _ = ask(w, Request::Close { rack: RACK });
        true
    }));
    s.push(wait(3.0 * DELAY + 0.2));
    s.push(Box::new(|w, c| {
        check(c, rack_count(w, ORE) == 0 && total(w, ORE) == 2, format!("fabricator: nothing moved (rack {}, all {})", rack_count(w, ORE), total(w, ORE)));
        end(w, c, "refused".into());
        true
    }));

    // Run 1: two grumblite, the button, one fuse.
    s.extend(load("fabricator: 2 grumblite into the first input", 0));
    s.push(Box::new(|w, c| {
        check(c, in_input(w, 0) == 2 && total(w, ORE) == 2, format!("fabricator: both ore are in the input place ({})", in_input(w, 0)));
        true
    }));
    s.push(until("fabricator: the screen names the recipe", 3.0, |w| says(w, "2 GRUMBLITE > FUSE") && says(w, "PUSH TO START")));
    s.push(Box::new(|w, _| {
        let at = machine(w).world([0.1, 0.95, 0.4]);
        look_at(w, at);
        true
    }));
    s.push(wait(0.3));
    s.push(shot_step("fabricator-01-ready"));
    s.push(push("fabricator: F on the button starts the run"));
    s.push(until("fabricator: the run is going and the inputs are locked", 3.0, |w| fab(w).fab.running() && items(w).host.racks.get(&RACK).is_some_and(|c| INPUTS.iter().all(|i| c.rule(*i).locked))));
    s.push(Box::new(|w, c| {
        // Locked: no ore can be taken out or put in while it runs.
        let _ = ask(w, Request::Open { rack: RACK });
        c.v.insert("sent", 0.0);
        true
    }));
    s.push(wait(3.0 * DELAY));
    s.push(Box::new(|w, c| {
        let r = ask(w, Request::FetchTo { rack: RACK, rack_slot: 0, slot: 5 });
        let _ = r;
        let _ = ask(w, Request::Close { rack: RACK });
        c.v.insert("took", 1.0);
        true
    }));
    s.push(wait(3.0 * DELAY + 0.2));
    s.push(Box::new(|w, c| {
        check(c, in_input(w, 0) == 2 && total(w, ORE) == 2, format!("fabricator: the locked input kept its ore ({})", in_input(w, 0)));
        check(c, says(w, "FUSE") && screen_words(w).iter().any(|t| t.ends_with(" S")), format!("fabricator: the screen shows the product and the seconds ({:?})", screen_words(w)));
        check(c, items(w).fab.news.is_some_and(|n| n.mode == FabMode::Running), "fabricator: the news say it runs".into());
        let at = machine(w).world([0.1, 0.95, 0.4]);
        look_at(w, at);
        true
    }));
    s.push(wait(0.3));
    s.push(shot_step("fabricator-02-running"));
    s.push(until("fabricator: one fuse lies in the output place, the inputs are empty", 25.0, |w| rack_count(w, PART) == 1 && rack_count(w, ORE) == 0));
    s.push(Box::new(|w, c| {
        check(c, !fab(w).fab.running() && fab(w).runs == 1, format!("fabricator: one run, done ({} runs)", fab(w).runs));
        check(c, total(w, ORE) == 0 && total(w, PART) == 1, format!("fabricator: nothing lost or doubled (ore {}, fuse {})", total(w, ORE), total(w, PART)));
        check(c, items(w).host.racks.get(&RACK).is_some_and(|c| INPUTS.iter().all(|i| !c.rule(*i).locked)), "fabricator: the inputs are free again".into());
        true
    }));
    s.push(until("fabricator: the screen says TAKE FUSE", 3.0, |w| says(w, "TAKE") && says(w, "FUSE")));

    // Run 2: one grumblite is no recipe; the button refuses and nothing changes.
    s.push(Box::new(|w, _| {
        give(w, ORE, 1);
        true
    }));
    s.extend(load("fabricator: 1 grumblite into the first input", 0));
    s.push(until("fabricator: the screen says NO RECIPE... after the take is shown", 3.0, |w| items(w).fab.news.is_some_and(|n| n.mode == FabMode::Take)));
    s.push(Box::new(|w, c| {
        c.v.insert("refusals", fab(w).refusals as f64);
        true
    }));
    s.push(push("fabricator: F on the button with one grumblite"));
    s.push(Box::new(|w, c| {
        check(c, fab(w).refusals as f64 == c.v["refusals"] + 1.0, format!("fabricator: the button refused ({} refusals)", fab(w).refusals));
        check(c, !fab(w).fab.running() && in_input(w, 0) == 1 && rack_count(w, PART) == 1 && fab(w).runs == 1, "fabricator: nothing started, nothing changed".into());
        true
    }));

    // Run 3: the second grumblite, the button, a power cut in the middle, the power back.
    s.push(Box::new(|w, _| {
        give(w, ORE, 1);
        true
    }));
    s.extend(load("fabricator: the second grumblite", 0));
    s.push(push("fabricator: F on the button starts the second run"));
    s.push(until("fabricator: it runs", 3.0, |w| fab(w).fab.running()));
    s.push(until("fabricator: 3 s into the run", 6.0, |w| left_s(w).is_some_and(|l| l < 9.0)));
    s.push(Box::new(|w, c| {
        begin(w, c, "fabricator: the power is cut");
        w.resource_mut::<crate::dev::DevMenu>().cold_reactor = true;
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        c.v.insert("left", left_s(w).unwrap_or(-1.0));
        true
    }));
    s.push(wait(3.0));
    s.push(Box::new(|w, c| {
        let (a, b) = (c.v["left"], left_s(w).unwrap_or(-2.0));
        check(c, a > 0.0 && (a - b).abs() < 1e-9, format!("fabricator: no progress without power ({a:.2} s then {b:.2} s)"));
        check(c, says(w, "NO POWER") && items(w).fab.news.is_some_and(|n| n.mode == FabMode::NoPower), format!("fabricator: the screen says NO POWER ({:?})", screen_words(w)));
        check(c, fab(w).fab.running(), "fabricator: the run is paused, not lost".into());
        end(w, c, "paused".into());
        w.resource_mut::<ReactorSim>().room.warm_reactor();
        true
    }));
    s.push(until("fabricator: the run goes on when the power is back", 4.0, |w| left_s(w).is_some_and(|l| l < 8.9) && !says(w, "NO POWER")));
    s.push(until("fabricator: the second fuse is made", 20.0, |w| rack_count(w, PART) == 2 && rack_count(w, ORE) == 0));
    s.push(Box::new(|w, c| {
        check(c, fab(w).runs == 2 && total(w, PART) == 2 && total(w, ORE) == 0, format!("fabricator: two runs, two fuses, no ore left ({} runs, fuse {}, ore {})", fab(w).runs, total(w, PART), total(w, ORE)));
        let cfg = w.resource::<FabCfg>().0.recipes.len();
        check(c, cfg == 3, "fabricator: the three recipes are loaded".into());
        end(w, c, "done".into());
        true
    }));
}
