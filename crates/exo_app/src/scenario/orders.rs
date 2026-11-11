//! The dispatch chute (#334), with the fixed seed: the walker reads the first order off the slate; SEND on an empty
//! chute is refused (`done` stays 0, the lever springs back, the slate returns to the order); one item too few in the
//! hatch leaves the line unmet; the rest plus one extra meets it; SEND takes exactly the order's counts, the extra stays,
//! the next order differs; a save, a reset and a load bring back `done` and the same order. Then the next order (two lines)
//! is half filled for the picture. Every step polls for its outcome with a limit in simulated seconds.
use super::dig::until;
use super::items::{items, look_at, prompt, DELAY};
use super::*;
use crate::dev_area::dispatch::{DispatchCfg, DispatchMachine, DispatchState, DISPATCH_RACK as RACK, LEVER, SCREEN_ID};
use crate::dev_area::DevArea;
use crate::items::{Items, LOCAL};
use crate::controls_kit::Machine;
use display_core::{Shape, Widget};
use item_core::container::ItemKind;
use item_core::wire::DispatchFace;
use item_core::Request;
use production_core::fabricator::Amount;

fn machine(w: &mut World) -> DispatchMachine {
    let mut q = w.query::<&DispatchMachine>();
    *q.single(w).expect("the chute stands there")
}

fn st(w: &World) -> &DispatchState {
    w.resource::<DispatchState>()
}

fn order(w: &World) -> Vec<Amount> {
    st(w).book.current().lines.clone()
}

fn in_chute(w: &World, item: &str) -> u32 {
    items(w).host.racks.get(&RACK).map_or(0, |c| c.count(&ItemKind::new(item)))
}

fn bar_slot(w: &World, kind: &str) -> Option<usize> {
    (0..8).find(|i| items(w).client.view.slot(*i).is_some_and(|s| s.item.kind.as_str() == kind))
}

fn give(w: &mut World, id: &str, n: u32) {
    let now = w.resource::<Time>().elapsed_secs_f64();
    let stack = items(w).defs.get(id).unwrap().stack_of(n);
    let _ = w.resource_mut::<Items>().give(now, LOCAL, stack);
}

fn lever_index(w: &mut World) -> u8 {
    let mut q = w.query_filtered::<&Machine, With<DispatchMachine>>();
    q.single(w).ok().and_then(|m| m.controls.lever_index(LEVER)).unwrap_or(0)
}

/// The slate's shapes now: the words and how many ticks are drawn (a tick is a pair of strokes five pixels wide).
fn slate(w: &World) -> (Vec<String>, usize) {
    let panels = w.resource::<crate::displays::Panels>();
    let Some(p) = panels.get(&SCREEN_ID) else { return (Vec::new(), 0) };
    let mut words = Vec::new();
    let mut strokes = 0;
    for l in p.layout() {
        if let Widget::Canvas { shapes, .. } = l.widget {
            for s in shapes {
                match s {
                    Shape::Text { text, .. } | Shape::TextRight { text, .. } => words.push(text.clone()),
                    Shape::Line { width, .. } if *width == 5.0 => strokes += 1,
                    _ => {}
                }
            }
        }
    }
    (words, strokes / 2)
}

fn says(w: &World, word: &str) -> bool {
    slate(w).0.iter().any(|t| t == word)
}

fn stand_before(w: &mut World) {
    let at = machine(w).world([0.0, 0.1, 1.5]);
    place_walker(w, at);
}

fn look_at_lever(w: &mut World) {
    let at = machine(w).lever_at;
    look_at(w, at);
}

/// Aims at the lever, checks the prompt and taps F once; done when the lever is back at rest.
fn pull(what: &'static str) -> Step {
    let mut pulls0: Option<u32> = None;
    Box::new(move |w, c| {
        look_at_lever(w);
        if c.t == 0.0 {
            begin(w, c, what);
            return false;
        }
        if c.t < 0.2 {
            return false;
        }
        let Some(before) = pulls0 else {
            let p = prompt(w);
            check(c, p.contains("SEND lever"), format!("{what}: the prompt names the lever ({p:?})"));
            tap(w, KeyCode::KeyF);
            pulls0 = Some(st(w).pulls);
            return false;
        };
        if st(w).pulls == before {
            if c.t > 3.0 {
                check(c, false, format!("{what}: the pull reached the host"));
                return true;
            }
            return false;
        }
        lever_index(w) == 0 && c.t > 1.0
    })
}

/// The host puts goods straight into the chute (the picture prep; the main flow goes through the window).
fn fill(w: &mut World, lines: &[(String, u32)]) {
    let now = w.resource::<Time>().elapsed_secs_f64();
    let defs = items(w).defs.clone();
    w.resource_mut::<Items>().edit_rack(now, RACK, |c| {
        for (item, n) in lines {
            let stack = defs.get(item).unwrap().stack_of(*n);
            let _ = c.try_insert(&[], stack);
        }
    });
}

/// Stores the walker's stack of the order's first item in the hatch's place `rack_slot` through the container path.
fn store(what: &'static str, rack_slot: usize) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            begin(w, c, what);
            let now = w.resource::<Time>().elapsed_secs_f64();
            let r = w.resource_mut::<Items>().request(now, Request::Open { rack: RACK });
            check(c, r.is_ok(), format!("{what}: the hatch opens ({r:?})"));
            true
        }),
        wait(3.0 * DELAY),
        Box::new(move |w, c| {
            let kind = order(w)[0].item.clone();
            let Some(slot) = bar_slot(w, &kind) else {
                check(c, false, format!("{what}: {kind} in the bar"));
                return true;
            };
            let now = w.resource::<Time>().elapsed_secs_f64();
            let r = w.resource_mut::<Items>().request(now, Request::StoreAt { slot, rack: RACK, rack_slot });
            check(c, r.is_ok(), format!("{what}: the store is accepted ({r:?})"));
            true
        }),
        wait(3.0 * DELAY + 0.2),
        Box::new(move |w, c| {
            let now = w.resource::<Time>().elapsed_secs_f64();
            let _ = w.resource_mut::<Items>().request(now, Request::Close { rack: RACK });
            end(w, c, "stored".into());
            true
        }),
        wait(3.0 * DELAY),
    ]
}

pub(super) fn orders_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "orders: the chute, its rack and its slate");
            w.resource_mut::<Items>().delay = DELAY;
        }
        let ready = w.resource::<DevArea>().frame.is_some() && items(w).client.racks.contains_key(&RACK) && w.query::<&DispatchMachine>().iter(w).count() == 1 && items(w).dispatch.news.is_some();
        if !ready {
            if c.t > 5.0 {
                check(c, false, "orders: the chute and its rack are there".into());
                return true;
            }
            return false;
        }
        stand_before(w);
        let rack = items(w).client.racks.get(&RACK).cloned().unwrap();
        check(c, rack.len() == 6, format!("orders: six places in the hatch ({})", rack.len()));
        let first = order(w);
        c.v.insert("first_items", first.len() as f64);
        check(c, st(w).book.done() == 0 && !first.is_empty() && first.len() <= 3, format!("orders: the first order is {first:?}, done 0"));
        let cfg = w.resource::<DispatchCfg>().0.clone();
        let again = production_core::orders::OrderBook::new(&cfg, crate::dev_area::dispatch::SEED);
        check(c, again.current().lines == first, "orders: the fixed seed gives the same first order".into());
        true
    }));
    s.push(until("orders: the slate shows the first order and no tick", 3.0, |w| {
        let first = order(w);
        let (words, ticks) = slate(w);
        says(w, "ORDER 1") && ticks == 0 && first.iter().all(|l| words.iter().any(|t| t.starts_with(&format!("{} ", l.count))))
    }));
    s.push(Box::new(|w, c| {
        let words = slate(w).0;
        check(c, words.contains(&"DONE 0".to_string()), format!("orders: the footer says DONE 0 ({words:?})"));
        // Name check: every line names its item.
        let defs = items(w).defs.clone();
        for l in order(w) {
            let name = defs.get(&l.item).unwrap().name.to_uppercase();
            check(c, words.contains(&format!("{} {}", l.count, name)), format!("orders: the slate names {} {name}", l.count));
        }
        true
    }));

    // SEND on an empty chute: refused.
    s.push(pull("orders: SEND on an empty chute"));
    s.push(Box::new(|w, c| {
        check(c, st(w).book.done() == 0 && st(w).refusals == 1 && st(w).sends == 0, format!("orders: refused, done {} ({} refusals)", st(w).book.done(), st(w).refusals));
        check(c, lever_index(w) == 0, "orders: the lever sprang back to rest".into());
        end(w, c, "refused".into());
        true
    }));
    s.push(until("orders: the slate goes back to the order after the blink", 4.0, |w| st(w).face == DispatchFace::Order && says(w, "ORDER 1")));

    // One item too few, then the rest plus one extra, through the container path.
    s.push(Box::new(|w, c| {
        let first = order(w);
        let l = first[0].clone();
        c.v.insert("n", l.count as f64);
        give(w, &l.item, l.count - 1);
        true
    }));
    s.push(wait(0.3));
    s.extend(store("orders: one too few into the hatch", 0));
    s.push(Box::new(|w, c| {
        let l = order(w)[0].clone();
        check(c, in_chute(w, &l.item) == l.count - 1, format!("orders: {} of {} lie in the chute", in_chute(w, &l.item), l.count));
        true
    }));
    s.push(until("orders: the line shows unmet while one is missing", 3.0, |w| items(w).dispatch.news.as_ref().is_some_and(|n| n.lines.iter().all(|l| !l.met)) && slate(w).1 == 0));
    s.push(Box::new(|w, _| {
        let l = order(w)[0].clone();
        give(w, &l.item, 2);
        true
    }));
    s.push(wait(0.3));
    s.extend(store("orders: the rest plus one extra into the hatch", 1));
    s.push(until("orders: every line shows met", 3.0, |w| items(w).dispatch.news.as_ref().is_some_and(|n| !n.lines.is_empty() && n.lines.iter().all(|l| l.met)) && slate(w).1 == order(w).len()));
    s.push(Box::new(|w, c| {
        let first = order(w);
        c.v.insert("done_before", st(w).book.done() as f64);
        w.insert_resource(FirstOrder(first));
        true
    }));
    s.push(pull("orders: SEND on a complete chute"));
    s.push(until("orders: done is 1 and the slate flashes DONE", 3.0, |w| st(w).book.done() == 1 && says(w, "DONE") && says(w, "ORDER 1 SENT")));
    s.push(shot_step("orders-00-done"));
    s.push(Box::new(|w, c| {
        let first = w.resource::<FirstOrder>().0.clone();
        let l = &first[0];
        check(c, in_chute(w, &l.item) == 1, format!("orders: the chute holds exactly the extra ({} of {})", in_chute(w, &l.item), l.item));
        check(c, order(w) != first, format!("orders: the next order differs ({:?})", order(w)));
        check(c, st(w).sends == 1 && items(w).dispatch.news.as_ref().is_some_and(|n| n.done == 1), "orders: one send, the news say done 1".into());
        true
    }));
    s.push(until("orders: the slate shows order 2 after the flash", 4.0, |w| st(w).face == DispatchFace::Order && says(w, "ORDER 2") && says(w, "DONE 1")));

    // Save, reset, load.
    s.push(Box::new(|w, c| {
        begin(w, c, "orders: done and the order survive a save and a load");
        let r = crate::savefile::save_now(w);
        check(c, r.is_ok(), format!("orders: saved to the file ({r:?})"));
        c.v.insert("done", st(w).book.done() as f64);
        w.insert_resource(Saved(order(w)));
        let cfg = w.resource::<DispatchCfg>().0.clone();
        crate::dev_area::dispatch::apply_section(&mut w.resource_mut::<DispatchState>(), &cfg, None);
        check(c, st(w).book.done() == 0, "orders: reset to a fresh book".into());
        let dir = w.resource::<crate::savefile::SaveDir>().0.clone();
        let text = crate::savefile::read_slot(&dir, crate::savefile::SLOT).ok().flatten().unwrap_or_default();
        match gameplay_core::save::Envelope::from_json(&text) {
            Ok(env) => {
                w.insert_resource(crate::savefile::PendingLoad(env));
            }
            Err(e) => check(c, false, format!("orders: the file reads back ({e})")),
        }
        true
    }));
    s.push(until("orders: the loaded book has done 1", 3.0, |w| st(w).book.done() == 1));
    s.push(Box::new(|w, c| {
        let saved = w.resource::<Saved>().0.clone();
        check(c, order(w) == saved, format!("orders: the same current order after the load ({:?})", order(w)));
        end(w, c, "saved".into());
        true
    }));

    // The picture: send order 2 exactly, then fill order 3 (two lines) with one line met and one short.
    s.push(Box::new(|w, c| {
        begin(w, c, "orders: order 2 is sent, order 3 is half filled");
        let lines: Vec<(String, u32)> = order(w).iter().map(|l| (l.item.clone(), l.count)).collect();
        // The extra from the first send is still in the chute: it counts for a line of the same item.
        fill(w, &lines);
        true
    }));
    s.push(until("orders: the chute holds order 2", 3.0, |w| items(w).dispatch.news.as_ref().is_some_and(|n| !n.lines.is_empty() && n.lines.iter().all(|l| l.met))));
    s.push(pull("orders: SEND order 2"));
    s.push(until("orders: done is 2", 3.0, |w| st(w).book.done() == 2));
    s.push(until("orders: order 3 on the slate (two lines)", 4.0, |w| st(w).face == DispatchFace::Order && says(w, "ORDER 3")));
    s.push(Box::new(|w, c| {
        let o = order(w);
        check(c, o.len() == 2, format!("orders: order 3 has two lines ({o:?})"));
        // Line 1 in full, line 2 one short. What lies in the chute from before counts: top up exactly.
        let mut lines = Vec::new();
        for (i, l) in o.iter().enumerate() {
            let want = if i == 0 { l.count } else { l.count.saturating_sub(1) };
            let have = in_chute(w, &l.item);
            lines.push((l.item.clone(), want.saturating_sub(have)));
        }
        fill(w, &lines);
        stand_before(w);
        true
    }));
    s.push(until("orders: one line ticked, one not", 3.0, |w| slate(w).1 == 1));
    s.push(Box::new(|w, _| {
        let m = machine(w);
        let at = m.world([0.3, 1.0, 0.2]);
        look_at(w, at);
        true
    }));
    s.push(wait(0.5));
    s.push(shot_step("orders-01-half"));
    s.push(Box::new(|w, c| {
        let (words, ticks) = slate(w);
        check(c, ticks == 1 && words.contains(&"ORDER 3".to_string()) && words.contains(&"DONE 2".to_string()), format!("orders: the picture shows ORDER 3, DONE 2 and one tick ({words:?})"));
        end(w, c, "done".into());
        true
    }));
}

#[derive(Resource)]
struct FirstOrder(Vec<Amount>);

#[derive(Resource)]
struct Saved(Vec<Amount>);
