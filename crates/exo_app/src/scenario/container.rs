//! The container window (#310) at the storage crate: open with F, click and drag stacks through the
//! host's authority (nothing lost or duplicated), refusals, Esc, the lock against a second player,
//! walking away.
use super::*;
use crate::container_window::{place_centre, ContainerSpecs, ContainerWindow, Place};
use crate::dev_area::storage::{CRATE, ROW_Y};
use crate::dev_area::{DevArea, STORAGE};
use crate::interact::Interaction;
use crate::items::{HudWindow, InventoryUi, Items};
use item_core::{Action, Request};

const DELAY: f64 = 0.075;
const MINE: u32 = 1;
const OTHER: u32 = 2;

fn items(w: &World) -> &Items {
    w.resource::<Items>()
}

fn cw(w: &World) -> &ContainerWindow {
    w.resource::<ContainerWindow>()
}

/// The player's three items anywhere: in the bar and in the crate (nothing may be lost or
/// duplicated). The crate's own stock (fuel rod, bandages, canisters) is not counted.
fn mine(w: &World) -> u32 {
    let it = items(w);
    ["wrench", "extinguisher", "suit"].iter().map(|k| it.host.inventory(MINE).count(k) + in_crate(w, k)).sum()
}

fn in_bar(w: &World, kind: &str) -> u32 {
    items(w).host.inventory(MINE).count(kind)
}

fn bar_slot_of(w: &World, kind: &str) -> Option<usize> {
    (0..8).find(|i| items(w).client.view.slot(*i).is_some_and(|s| s.item.kind.as_str() == kind))
}

fn in_crate(w: &World, kind: &str) -> u32 {
    items(w).host.racks.get(&CRATE).map_or(0, |c| c.count(&item_core::container::ItemKind::new(kind)))
}

fn centre(w: &World, p: Place) -> Vec2 {
    let n = items(w).client.racks.get(&CRATE).map_or(0, |c| c.len());
    let cols = w.resource::<ContainerSpecs>().get(CRATE).map_or(4, |s| s.cols);
    place_centre(w.resource::<HudWindow>().0, n, cols, &w.resource::<crate::displays::DisplayLook>().0.hud.bar, p)
}

/// The first place of the crate holding a kind.
fn place_of(w: &World, kind: &str) -> Option<usize> {
    let c = items(w).client.racks.get(&CRATE)?;
    (0..c.len()).find(|i| c.slot(*i).is_some_and(|s| s.item.kind.as_str() == kind))
}

fn click_at(what: &'static str, at: fn(&World) -> Option<Place>) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            let Some(p) = at(w) else {
                check(c, false, format!("container: {what}: the place exists"));
                return true;
            };
            let pos = centre(w, p);
            pointer_at(w, Some(pos), true);
            return false;
        }
        if c.t < 0.1 {
            return false;
        }
        w.resource_mut::<Controls>().pointer_down = false;
        c.t > 3.0 * DELAY + 0.1
    })
}

fn pointer_at(w: &mut World, p: Option<Vec2>, down: bool) {
    let mut ctl = w.resource_mut::<Controls>();
    ctl.pointer = p;
    ctl.pointer_down = down;
}

pub(super) fn container_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        begin(w, c, "container: the crate and a bar with bandages");
        w.resource_mut::<Items>().delay = DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && items(w).client.racks.contains_key(&CRATE);
        if !ready {
            if c.t > 5.0 {
                check(c, false, "container: the crate stands there".into());
                return true;
            }
            return false;
        }
        let now = w.resource::<Time>().elapsed_secs_f64();
        let defs = items(w).defs.clone();
        for (id, n) in [("wrench", 1), ("extinguisher", 1), ("suit", 1)] {
            let _ = w.resource_mut::<Items>().give(now, MINE, defs.get(id).unwrap().stack_of(n));
        }
        let pl = planet(w);
        let (at, _) = w.resource::<DevArea>().ground(&pl, STORAGE.point([3.5, ROW_Y - 1.5]), 0.5).expect("dev area");
        place_walker(w, at);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let mut q = w.query::<(&crate::dev_area::DevBox, &avian3d::prelude::Position)>();
        let at = q.iter(w).find(|(b, _)| b.id == "store_crate").map(|(_, p)| p.0).expect("the crate box");
        super::items::look_at(w, at);
        c.v.insert("mine", mine(w) as f64);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let p = w.resource::<Interaction>().prompt.clone();
        check(c, p.contains("Supply crate"), format!("container: the prompt offers the crate ({p:?})"));
        check(c, c.v["mine"] == 3.0, format!("container: three items to move ({})", c.v["mine"]));
        true
    }));
    s.push(super::items::tap_then("container: F opens the window", KeyCode::KeyF, |w| cw(w).rack == Some(CRATE) && w.resource::<InventoryUi>().open && items(w).client.holder(CRATE) == Some(MINE)));
    s.push(wait(3.0 * DELAY));
    s.push(Box::new(|w, c| {
        check(c, items(w).host.locks.get(&CRATE) == Some(&MINE), "container: the host holds the crate for the player".into());
        true
    }));

    // Click: the bar's wrench goes into the crate, a click on the crate's wrench brings it back.
    s.push(click_at("click the wrench in the bar", |w| bar_slot_of(w, "wrench").map(Place::Bar)));
    s.push(Box::new(|w, c| {
        let (bar, held) = (in_bar(w, "wrench"), in_crate(w, "wrench"));
        check(c, bar == 0 && held == 1 && mine(w) == 3, format!("container: a click moved the stack into the crate (bar {bar}, crate {held})"));
        true
    }));
    s.push(click_at("click the wrench in the crate", |w| place_of(w, "wrench").map(Place::Box)));
    s.push(Box::new(|w, c| {
        let (bar, held) = (in_bar(w, "wrench"), in_crate(w, "wrench"));
        check(c, bar == 1 && held == 0 && mine(w) == 3, format!("container: a click moved it back (bar {bar}, crate {held})"));
        true
    }));

    // Drag a bar item onto a chosen place; an item that does not go in comes back (extinguisher is
    // a medium tool: it goes in the crate, so refuse by a full place instead: a bandage onto the
    // wrench's place).
    s.push(Box::new(|w, c| {
        let slot = bar_slot_of(w, "extinguisher").expect("the extinguisher");
        let from = centre(w, Place::Bar(slot));
        pointer_at(w, Some(from), true);
        begin(w, c, "container: drag the extinguisher to place 5");
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        let to = centre(w, Place::Box(4));
        pointer_at(w, Some(to), true);
        if c.t < 0.1 {
            return false;
        }
        check(c, cw(w).dragging() && cw(w).over.as_ref().is_some_and(|(p, why)| *p == Place::Box(4) && why.is_none()), format!("container: the place accepts ({:?})", cw(w).over));
        pointer_at(w, Some(to), false);
        true
    }));
    s.push(wait(3.0 * DELAY + 0.1));
    s.push(Box::new(|w, c| {
        let on_five = items(w).host.racks[&CRATE].slot(4).is_some_and(|s| s.item.kind.as_str() == "extinguisher");
        check(c, on_five && in_bar(w, "extinguisher") == 0 && mine(w) == 3, "container: the extinguisher lies on place 5, not in the bar".into());
        end(w, c, "dragged".into());
        true
    }));

    // A refused drop changes nothing: the suit does not go into a crate for small and medium items.
    s.push(Box::new(|w, c| {
        begin(w, c, "container: a refused drop returns the item");
        let slot = bar_slot_of(w, "suit").expect("the suit");
        pointer_at(w, Some(centre(w, Place::Bar(slot))), true);
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        pointer_at(w, Some(centre(w, Place::Box(8))), true);
        if c.t < 0.1 {
            return false;
        }
        let refused = cw(w).over.as_ref().is_some_and(|(p, why)| *p == Place::Box(8) && *why == Some(item_core::Reject::NotAllowed));
        check(c, refused, format!("container: the place refuses ({:?})", cw(w).over));
        pointer_at(w, Some(centre(w, Place::Box(8))), false);
        true
    }));
    s.push(wait(3.0 * DELAY + 0.1));
    s.push(Box::new(|w, c| {
        check(c, in_bar(w, "suit") == 1 && mine(w) == 3, "container: the suit is still in the bar".into());
        check(c, cw(w).flash.is_some(), "container: the refusal shows its reason".into());
        true
    }));

    // Esc cancels a drag, a second Esc closes; closing mid-drag loses nothing.
    s.push(Box::new(|w, c| {
        begin(w, c, "container: Esc cancels a drag");
        let slot = bar_slot_of(w, "wrench").expect("the wrench");
        pointer_at(w, Some(centre(w, Place::Bar(slot))), true);
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        pointer_at(w, Some(centre(w, Place::Box(7))), true);
        if c.t < 0.1 {
            return false;
        }
        check(c, cw(w).dragging(), "container: a drag is under way".into());
        tap(w, KeyCode::Escape);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        pointer_at(w, Some(centre(w, Place::Box(7))), false);
        check(c, cw(w).rack == Some(CRATE) && !cw(w).dragging() && mine(w) == 3 && in_bar(w, "wrench") == 1, "container: Esc cancelled the drag, the window stays, nothing moved".into());
        true
    }));
    s.push(wait(0.2));

    // The lock: a second player cannot open nor move while the window is open.
    s.push(Box::new(|w, c| {
        begin(w, c, "container: a second player is locked out");
        let now = w.resource::<Time>().elapsed_secs_f64();
        let defs = items(w).defs.clone();
        w.resource_mut::<Items>().host.give(OTHER, defs.get("bandage").unwrap().stack_of(2)).expect("room");
        let mut it = w.resource_mut::<Items>();
        it.remote(now, Action { player: OTHER, seq: 1, req: Request::Open { rack: CRATE } });
        it.remote(now, Action { player: OTHER, seq: 2, req: Request::Store { slot: 0, rack: CRATE } });
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let it = items(w);
        check(c, it.host.locks.get(&CRATE) == Some(&MINE), "container: the lock stays with the first player".into());
        check(c, it.host.inventory(OTHER).count("bandage") == 2, "container: the second player's store was refused".into());
        end(w, c, "locked out".into());
        true
    }));

    // F closes and frees the lock; the other player can open now.
    s.push(super::items::tap_then("container: F closes the window", KeyCode::KeyF, |w| cw(w).rack.is_none() && !w.resource::<InventoryUi>().open));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        check(c, items(w).host.locks.is_empty(), "container: closing frees the crate".into());
        let now = w.resource::<Time>().elapsed_secs_f64();
        w.resource_mut::<Items>().remote(now, Action { player: OTHER, seq: 3, req: Request::Open { rack: CRATE } });
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        check(c, items(w).host.locks.get(&CRATE) == Some(&OTHER), "container: the second player holds it now".into());
        let p = w.resource::<Interaction>().prompt.clone();
        check(c, p.contains("in use"), format!("container: the first player's prompt says in use ({p:?})"));
        true
    }));
    s.push(super::items::tap_then("container: F on a crate in use opens nothing", KeyCode::KeyF, |w| cw(w).rack.is_none()));
    s.push(Box::new(|w, _| {
        let now = w.resource::<Time>().elapsed_secs_f64();
        w.resource_mut::<Items>().remote(now, Action { player: OTHER, seq: 4, req: Request::Close { rack: CRATE } });
        true
    }));
    s.push(wait(0.3));

    // Closing mid-drag loses nothing and frees the crate.
    s.push(super::items::tap_then("container: F opens it again", KeyCode::KeyF, |w| cw(w).rack == Some(CRATE)));
    s.push(Box::new(|w, c| {
        begin(w, c, "container: closing mid-drag");
        let slot = bar_slot_of(w, "wrench").expect("the wrench");
        pointer_at(w, Some(centre(w, Place::Bar(slot))), true);
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        pointer_at(w, Some(centre(w, Place::Box(9))), true);
        if c.t < 0.1 {
            return false;
        }
        check(c, cw(w).dragging(), "container: a drag is under way".into());
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        pointer_at(w, None, false);
        check(c, cw(w).rack.is_none() && !cw(w).dragging() && items(w).host.locks.is_empty(), "container: the window closed, the drag is gone, the crate is free".into());
        check(c, mine(w) == 3 && in_bar(w, "wrench") == 1, "container: nothing moved".into());
        end(w, c, "closed mid-drag".into());
        true
    }));

    // Walking away closes the window and frees the lock.
    s.push(super::items::tap_then("container: F opens it a third time", KeyCode::KeyF, |w| cw(w).rack == Some(CRATE)));
    s.push(Box::new(|w, c| {
        begin(w, c, "container: walking away closes the window");
        let pl = planet(w);
        let (at, _) = w.resource::<DevArea>().ground(&pl, STORAGE.point([3.5, ROW_Y - 9.0]), 0.5).expect("dev area");
        place_walker(w, at);
        true
    }));
    s.push(wait(0.4));
    s.push(Box::new(|w, c| {
        check(c, cw(w).rack.is_none() && items(w).host.locks.is_empty(), "container: the window closed and the lock is free".into());
        check(c, mine(w) == 3 && items(w).host.inventory(OTHER).count("bandage") == 2, "container: nothing lost or duplicated".into());
        end(w, c, "done".into());
        true
    }));
}
