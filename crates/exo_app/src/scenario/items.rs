//! Items at the dev area's item table, through the item request path.
use super::*;
use crate::cargo::Crate;
use crate::dev_area::{dev::item_table, DevArea, DEV};
use crate::interact::Interaction;
use crate::items::{slot_at, slot_rect, HudWindow, InventoryUi, Items, LooseItem};
use item_core::{Action, Reject, Request};

/// One-way delay of every item message in this scenario (s).
pub(super) const DELAY: f64 = 0.075;

pub(super) fn items(w: &World) -> &Items {
    w.resource::<Items>()
}

pub(super) fn prompt(w: &World) -> String {
    w.resource::<Interaction>().prompt.clone()
}

/// Loose items of a kind: entity, host id, world position, frame-local body.
fn loose_of(w: &mut World, def: &str) -> Vec<(Entity, Option<u64>, DVec3, bool)> {
    let f = ship_frame_of(w);
    let mut q = w.query::<(Entity, &LooseItem, &Crate)>();
    q.iter(w)
        .filter(|(_, li, _)| li.def == def)
        .map(|(e, li, c)| (e, li.id, crate::cargo::crate_world(c, &f).0, c.ship.is_some()))
        .collect()
}

pub(super) fn outside(w: &mut World, def: &str) -> Option<(Entity, Option<u64>, DVec3)> {
    loose_of(w, def).into_iter().find(|x| !x.3).map(|(e, id, p, _)| (e, id, p))
}

pub(super) fn look_at(w: &mut World, target: DVec3) {
    face_towards(w, target);
    let f = ship_frame_of(w);
    with_player(w, |p| {
        let eye = p.world_pos(f) + p.world_up(f) * crate::walker::EYE_HEIGHT;
        let d = target - eye;
        let up = p.world_up(f);
        p.pitch = d.dot(up).atan2((d - up * d.dot(up)).length());
    });
}

pub(super) fn collect(def: &'static str, name: &'static str) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            let Some((_, _, at)) = outside(w, def) else {
                check(c, false, format!("items: a {def} lies there"));
                return true;
            };
            look_at(w, at);
            c.v.insert("n0", items(w).client.view.count(def) as f64);
            c.v.insert("out0", loose_of(w, def).iter().filter(|x| !x.3).count() as f64);
            true
        }),
        wait(0.1),
        Box::new(move |w, c| {
            let p = prompt(w);
            check(c, p.contains(&format!("pick up {name}")), format!("items: the prompt offers the {def} ({p:?})"));
            tap(w, KeyCode::KeyF);
            true
        }),
        Box::new(move |w, c| {
            // The step after the tap: the fixed step ran, the host has not answered yet.
            let n0 = c.v["n0"] as u32;
            let (view, confirmed) = (items(w).client.view.count(def), items(w).client.confirmed.count(def));
            if c.t == 0.0 {
                return false;
            }
            check(c, view > n0 && confirmed == n0, format!("items: the {def} shows in the bar at once (view {view}, confirmed {confirmed})"));
            true
        }),
        wait(2.5 * DELAY),
        Box::new(move |w, c| {
            let confirmed = items(w).client.confirmed.count(def);
            let pending = items(w).client.pending.len();
            // The picked one is gone from the floor; a second one of the kind may still lie there (two plain rods on the rack).
            let left = loose_of(w, def).iter().filter(|x| !x.3).count() as f64 >= c.v["out0"];
            check(c, confirmed > c.v["n0"] as u32 && pending == 0 && !left, format!("items: the host confirmed the {def} (confirmed {confirmed}, in flight {pending}, still lying there {left})"));
            true
        }),
    ]
}

/// looks at it. The item messages take 75 ms each way.
pub(super) fn at_the_table(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        begin(w, c, "items: the table and the cabin");
        w.resource_mut::<Items>().delay = DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && ["extinguisher", "wrench", "bandage"].iter().all(|d| !loose_of(w, d).is_empty());
        if !ready && c.t < 3.0 {
            return false;
        }
        let in_cabin = loose_of(w, item_table::IN_CABIN.0).iter().filter(|x| x.3).count();
        let on_table = item_table::ON_TABLE.iter().filter(|(d, ..)| outside(w, d).is_some()).count();
        check(c, on_table == item_table::ON_TABLE.len() && in_cabin == 1, format!("items: every item of the table lies there ({on_table}), a wrench in the cabin ({in_cabin})"));
        end(w, c, "ready".into());
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let pl = planet(w);
        let p = DEV.point([item_table::AT[0], item_table::AT[1] - 1.3]);
        let (at, _) = w.resource::<DevArea>().ground(&pl, p, 0.5).expect("dev area");
        place_walker(w, at);
        begin(w, c, "items: collect at the table");
        true
    }));
    s.push(stand_still_short());
    s.push(Box::new(|w, _| {
        let pl = planet(w);
        let p = DEV.point(item_table::AT);
        let (top, _) = w.resource::<DevArea>().ground(&pl, p, item_table::SIZE[1]).expect("dev area");
        look_at(w, top);
        true
    }));
    s.push(wait(0.2));
}

pub(super) fn items_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    at_the_table(s);
    s.push(shot_step("table"));
    s.extend(collect("wrench", "Wrench"));
    s.extend(collect("extinguisher", "Extinguisher"));
    s.extend(collect("bandage", "Bandage"));
    s.push(Box::new(|w, c| {
        let v = &items(w).client.view;
        let names: Vec<String> = (0..8).map(|i| v.slot(i).map_or("-".into(), |s| format!("{}x{}", s.item.kind, s.count))).collect();
        check(c, v.slot(0).is_some_and(|s| s.item.kind.as_str() == "wrench") && v.slot(1).is_some_and(|s| s.item.kind.as_str() == "extinguisher") && v.slot(2).is_some_and(|s| s.count == 4),
            format!("items: the wrench in the hand, the rest in the next free slots ({})", names.join(" ")));
        end(w, c, "three collected".into());
        true
    }));
    s.push(tap_then("items: 2 takes the extinguisher into the hand", KeyCode::Digit2, |w| {
        let it = items(w);
        it.client.view.selected == 1 && it.in_hand(0).is_some_and(|s| s.item.kind.as_str() == "extinguisher") && it.client.view.held_hands(&it.defs) == 2
    }));
    s.push(shot_step("held"));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "items: G drops the extinguisher");
            tap(w, KeyCode::KeyG);
            return false;
        }
        let ghost = outside(w, "extinguisher");
        check(c, ghost.is_some_and(|g| g.1.is_none()) && items(w).client.view.count("extinguisher") == 0, format!("items: it lies there at once, not yet numbered ({:?})", ghost.map(|g| g.1)));
        true
    }));
    s.push(wait(1.5));
    s.push(Box::new(|w, c| {
        let pl = planet(w);
        let g = outside(w, "extinguisher");
        let agl = g.map(|g| pl.above_ground(g.2)).unwrap_or(f64::NAN);
        let count = loose_of(w, "extinguisher").len();
        check(c, g.is_some_and(|g| g.1.is_some()) && count == 1 && agl < 0.4, format!("items: the host numbered it, one extinguisher, on the ground ({agl:.2} m up)"));
        end(w, c, "dropped".into());
        true
    }));
    s.push(tap_then("items: 1 takes the wrench into the hand", KeyCode::Digit1, |w| items(w).in_hand(0).is_some_and(|s| s.item.kind.as_str() == "wrench")));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "items: R throws the wrench");
            c.p.insert("from", player_world(w));
            with_player(w, |p| p.pitch = 0.15);
            tap(w, KeyCode::KeyR);
            return false;
        }
        if c.t < 2.0 {
            return false;
        }
        let from = c.p["from"];
        let d = outside(w, "wrench").map(|x| (x.2 - from).length()).unwrap_or(0.0);
        check(c, items(w).client.view.count("wrench") == 0, format!("items: the wrench left the hand ({d:.1} m away)"));
        end(w, c, format!("thrown {d:.1} m"));
        true
    }));
    s.push(Box::new(|w, c| {
        let Some((_, _, at)) = outside(w, "extinguisher") else {
            check(c, false, "items: the extinguisher lies there".into());
            return true;
        };
        begin(w, c, "items: two players reach for the extinguisher");
        look_at(w, at);
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            let id = outside(w, "extinguisher").and_then(|x| x.1).expect("numbered");
            let now = w.resource::<Time>().elapsed_secs_f64();
            w.resource_mut::<Items>().remote(now, Action { player: 2, seq: 1, req: Request::Collect { item: id } });
            tap(w, KeyCode::KeyF);
            return false;
        }
        if !c.v.contains_key("seen") {
            c.v.insert("seen", 1.0);
            let n = items(w).client.view.count("extinguisher");
            check(c, n == 1, format!("items: the local player sees it in the bar at once ({n})"));
            return false;
        }
        if c.t < 3.0 * DELAY {
            return false;
        }
        let it = items(w);
        let rolled = it.client.view.count("extinguisher") == 0 && it.client.rejected.last().is_some_and(|r| r.1 == Reject::Gone);
        let other = it.host.inventory(2).count("extinguisher");
        let lying = loose_of(w, "extinguisher").len();
        check(c, rolled && other == 1 && lying == 0, format!("items: the other player has it ({other}), the local bar rolled back ({rolled}), nothing lies there ({lying})"));
        end(w, c, "conflict resolved".into());
        true
    }));
    s.push(tap_then("items: Tab opens the inventory", KeyCode::Tab, |w| w.resource::<InventoryUi>().open && w.resource::<Controls>().free_cursor));
    s.push(shot_step("inventory"));
    s.push(drag(2, 5));
    s.push(Box::new(|w, c| {
        if c.t < 3.0 * DELAY {
            return false;
        }
        let v = &items(w).client.confirmed;
        check(c, v.slot(5).is_some_and(|s| s.item.kind.as_str() == "bandage" && s.count == 4) && v.slot(2).is_none(), "items: the drag moved the bandages to slot 6, confirmed".into());
        true
    }));
    s.push(tap_then("items: Tab closes it", KeyCode::Tab, |w| !w.resource::<InventoryUi>().open && !w.resource::<Controls>().free_cursor));
    s.push(Box::new(|w, c| {
        begin(w, c, "items: take off with a wrench on the cabin floor");
        let x = loose_of(w, "wrench").into_iter().find(|x| x.3).map(|x| x.0).expect("the cabin wrench");
        c.v.insert("cabin", x.to_bits() as f64);
        let pos = w.get::<Crate>(x).unwrap().body.pos;
        c.p.insert("rest", pos);
        put_at_seat(w);
        true
    }));
    s.push(wait(0.3));
    s.extend(sit());
    s.push(hold_until("items: climb to 150 m", &[KeyCode::Space, KeyCode::ShiftLeft], 90.0, |w| above_ground(w) > 150.0));
    s.push(hold_until("items: full thrust forward", &[KeyCode::KeyW, KeyCode::ShiftLeft], 6.0, |_| false));
    s.push(hold_until("items: brake", &[KeyCode::KeyX], 20.0, |w| ship_vel(w).length() < 0.5));
    s.push(Box::new(|w, c| {
        let e = Entity::from_bits(c.v["cabin"] as u64);
        let cr = w.get::<Crate>(e).unwrap();
        let moved = (cr.body.pos - c.p["rest"]).length();
        check(c, cr.ship.is_some() && moved < 0.05, format!("items: the cabin wrench stayed on the floor (moved {:.1} mm, in the cabin {})", moved * 1000.0, cr.ship.is_some()));
        true
    }));
}

pub(super) fn tap_then(what: &'static str, k: KeyCode, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            tap(w, k);
            return false;
        }
        if done(w) {
            check(c, true, format!("{what} ({:.2} s)", c.t));
            return true;
        }
        if c.t >= 1.0 {
            check(c, false, format!("{what}: not within 1 s"));
            return true;
        }
        false
    })
}

fn drag(from: usize, to: usize) -> Step {
    Box::new(move |w, c| {
        let win = w.resource::<HudWindow>().0;
        let bar = w.resource::<crate::displays::DisplayLook>().0.hud.bar.clone();
        let centre = |i: usize| {
            let (at, size) = slot_rect(i, true, win, &bar);
            at + Vec2::splat(size * 0.5)
        };
        let (a, b) = (centre(from), centre(to));
        let mut ctl = w.resource_mut::<Controls>();
        if c.t == 0.0 {
            ctl.pointer = Some(a);
            ctl.pointer_down = true;
            return false;
        }
        if c.t < 0.1 {
            ctl.pointer = Some(b);
            return false;
        }
        ctl.pointer_down = false;
        check(c, slot_at(b, true, win, &bar) == Some(to), format!("items: drag from slot {} to slot {}", from + 1, to + 1));
        true
    })
}

fn stand_still_short() -> Step {
    Box::new(|w, c| with_player(w, |p| p.w.vel.length()) < 0.05 && c.t > 0.2 || c.t > 2.0)
}
