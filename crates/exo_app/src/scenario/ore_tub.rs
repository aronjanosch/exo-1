//! The ore tub (#327): the walker has ore, opens the standing tub with F and fills it through the
//! container path; picking the tub up closes the window and the host holds its rack, so an Open is
//! refused while it is carried; a full tub carried a few metres is slower than an empty one over
//! the same time; its load survives a save and a load; the object budget leaves tubs alone; set
//! down, its ore goes into Crate B and nothing is lost or doubled. Every step polls for its
//! outcome with a limit in simulated seconds. (The airlock cycle is a panel drive of its own in
//! `scenario_airlock`; the tub is not carried through it here.)
use super::items::{items, look_at, tap_then, DELAY};
use super::*;
use crate::cargo::Crate;
use crate::container_window::ContainerWindow;
use crate::dev_area::layout::STORAGE_ROOM;
use crate::dev_area::storage::CRATE_B;
use crate::dev_area::{DevArea, OPEN_GROUND};
use crate::grab::{Grab, Held, HoldError};
use crate::interact::Interaction;
use crate::items::{InventoryUi, Items, LOCAL};
use crate::ore_tub::{Tub, CARRIER, SPOTS, TUB_A, TUB_B};
use item_core::container::ItemKind;
use item_core::{Reject, Request};

const ORE: &str = "grumblite";
/// Chunks that fill a tub: six places of three.
const FULL: u32 = 18;
/// Where the carrying runs start and which way they go (storage room metres): east along the row
/// clear of the crates, the cabinets and both tubs' homes.
const START: [f64; 2] = [-2.0, -0.6];
const GOAL: [f64; 2] = [2.0, -0.6];

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

fn tub_e(w: &mut World, rack: u32) -> Entity {
    let mut q = w.query::<(Entity, &Tub)>();
    q.iter(w).find(|(_, t)| t.rack == rack).map(|(e, _)| e).expect("the tub is there")
}

fn tub_at(w: &mut World, rack: u32) -> DVec3 {
    let e = tub_e(w, rack);
    w.get::<Crate>(e).unwrap().body.pos
}

/// Puts a tub down somewhere (a scenario hook: the crate's body wins over the physics body).
fn put_tub(w: &mut World, rack: u32, at: DVec3) {
    let e = tub_e(w, rack);
    let mut c = w.get_mut::<Crate>(e).unwrap();
    c.body.pos = at;
    c.body.vel = DVec3::ZERO;
    c.body.wake();
}

fn home_of(w: &mut World, rack: u32) -> DVec3 {
    let at = SPOTS.iter().find(|s| s.0 == rack).unwrap().1;
    ground(w, STORAGE_ROOM.point(at), 0.21)
}

fn in_rack(w: &World, rack: u32) -> u32 {
    items(w).host.racks.get(&rack).map_or(0, |c| c.count(&ItemKind::new(ORE)))
}

/// The ore anywhere: on the floor, in the bar, in the tubs, in Crate B. A chunk may never be lost
/// or doubled.
fn ore_total(w: &World) -> u32 {
    let it = items(w);
    let loose = it.host.loose.values().filter(|l| l.stack.item.kind.as_str() == ORE).map(|l| l.stack.count).sum::<u32>();
    loose + it.host.inventory(LOCAL).count(ORE) + in_rack(w, TUB_A) + in_rack(w, TUB_B) + in_rack(w, CRATE_B)
}

fn held_tub(w: &World) -> Option<Entity> {
    w.resource::<Grab>().held.map(|h| h.crate_e)
}

fn window(w: &World) -> Option<u32> {
    w.resource::<ContainerWindow>().rack
}

/// A step that polls `done`, with a limit in simulated seconds.
fn until(what: &'static str, limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, what);
            return false;
        }
        if done(w) {
            end(w, c, format!("{:.1} s", c.t));
            return true;
        }
        if c.t >= limit {
            check(c, false, format!("{what}: not within {limit} s"));
            return true;
        }
        false
    })
}

/// Moves every stack of ore one request at a time (`into`: bar to the rack, else rack to the bar),
/// the next one once the host has answered the last.
fn move_ore(what: &'static str, rack: u32, into: bool) -> Step {
    until(what, 15.0, move |w| {
        if !items(w).client.pending.is_empty() {
            return false;
        }
        let now = w.resource::<Time>().elapsed_secs_f64();
        let next = if into {
            (0..8).find(|i| items(w).client.view.slot(*i).is_some_and(|s| s.item.kind.as_str() == ORE)).map(|slot| Request::Store { slot, rack })
        } else {
            items(w).client.racks.get(&rack).and_then(|c| (0..c.len()).find(|i| c.slot(*i).is_some_and(|s| s.item.kind.as_str() == ORE))).map(|rack_slot| Request::Fetch { rack, rack_slot })
        };
        match next {
            None => true,
            Some(req) => {
                let r = w.resource_mut::<Items>().request(now, req.clone());
                if r.is_err() {
                    println!("ore_tub: {req:?} refused: {r:?}");
                }
                false
            }
        }
    })
}

/// Walks east for 1.8 s with the tub of `rack` in both hands (it starts 1 m ahead, the walker at
/// `START`) and keeps how far the walker got between 0.8 s and 1.8 s as `key`: the same run for
/// a full and for an empty tub.
fn carry_run(rack: u32, other: u32, key: &'static str) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            begin(w, c, &format!("ore_tub: the tub {rack} carried east"));
            let at = home_of(w, other);
            put_tub(w, other, at);
            let from = ground(w, START, 0.3);
            let to = ground(w, GOAL, 0.3);
            place_walker(w, from);
            face_towards(w, to);
            let ahead = ground(w, [START[0] + 1.0, START[1]], 0.21);
            put_tub(w, rack, ahead);
            true
        }),
        wait(0.8),
        Box::new(move |w, _| {
            let at = tub_at(w, rack);
            look_at(w, at);
            true
        }),
        wait(0.1),
        tap_then("ore_tub: the carry key takes the tub in both hands", KeyCode::KeyZ, move |w| held_tub(w) == Some(tub_e(w, rack))),
        Box::new(|w, _| {
            let to = ground(w, GOAL, 0.3);
            face_towards(w, to);
            true
        }),
        wait(1.0),
        Box::new(move |w, c| {
            if c.t == 0.0 {
                keys(w, &[KeyCode::KeyW], true);
            }
            if c.t >= 0.8 && !c.p.contains_key("from") {
                let p = player_world(w);
                c.p.insert("from", p);
            }
            if c.t >= 1.8 {
                keys(w, &[KeyCode::KeyW], false);
                let from = c.p.remove("from").unwrap();
                let p = player_world(w);
                let d = (p - from).length();
                c.v.insert(key, d);
                check(c, held_tub(w) == Some(tub_e(w, rack)), "ore_tub: the tub is still in the hands after the walk".into());
                end(w, c, format!("{d:.2} m in 1 s"));
                return true;
            }
            false
        }),
        // F sets it down.
        tap_then("ore_tub: F sets the tub down", KeyCode::KeyF, |w| held_tub(w).is_none()),
        wait(0.5),
    ]
}

pub(super) fn ore_tub_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    // Eighteen chunks in the bar; the walker in front of tub A.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "ore_tub: ore in the bar, the walker at the first tub");
            w.resource_mut::<Items>().delay = DELAY;
        }
        let ready = w.resource::<DevArea>().frame.is_some()
            && items(w).client.racks.contains_key(&TUB_A)
            && items(w).client.racks.contains_key(&CRATE_B)
            && w.query::<&Tub>().iter(w).count() == 2;
        if !ready {
            if c.t > 5.0 {
                check(c, false, "ore_tub: the area and both tubs are there".into());
                return true;
            }
            return false;
        }
        let now = w.resource::<Time>().elapsed_secs_f64();
        let ore = items(w).defs.get(ORE).unwrap().stack_of(3);
        for _ in 0..FULL / 3 {
            let _ = w.resource_mut::<Items>().give(now, LOCAL, ore.clone());
        }
        let at = ground(w, STORAGE_ROOM.point([-1.4, 0.6]), 0.5);
        place_walker(w, at);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let at = tub_at(w, TUB_A);
        look_at(w, at);
        c.v.insert("ore0", ore_total(w) as f64);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let p = w.resource::<Interaction>().prompt.clone();
        check(c, p.contains("Ore tub") && p.contains("carry"), format!("ore_tub: the prompt offers to open and to carry ({p:?})"));
        check(c, c.v["ore0"] == FULL as f64, format!("ore_tub: eighteen chunks in the bar ({})", c.v["ore0"]));
        true
    }));
    s.push(tap_then("ore_tub: F opens the standing tub", KeyCode::KeyF, |w| window(w) == Some(TUB_A) && w.resource::<InventoryUi>().open && items(w).client.holder(TUB_A) == Some(LOCAL)));

    // Fill it through the container path.
    s.push(move_ore("ore_tub: the ore goes into the tub", TUB_A, true));
    s.push(Box::new(|w, c| {
        let (bar, tub, all) = (items(w).host.inventory(LOCAL).count(ORE), in_rack(w, TUB_A), ore_total(w));
        check(c, bar == 0 && tub == FULL && all == FULL, format!("ore_tub: the tub holds all {FULL}, the bar none (bar {bar}, tub {tub}, all {all})"));
        true
    }));
    s.push(tap_then("ore_tub: F closes the window", KeyCode::KeyF, |w| window(w).is_none()));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        check(c, items(w).host.locks.get(&TUB_A).is_none(), "ore_tub: the host let go of the tub".into());
        let at = tub_at(w, TUB_A);
        look_at(w, at);
        true
    }));
    s.push(wait(0.2));

    // Picked up: no window, the host holds the rack, an Open is refused for everyone.
    s.push(tap_then("ore_tub: the carry key takes the full tub", KeyCode::KeyZ, |w| held_tub(w) == Some(tub_e(w, TUB_A))));
    s.push(until("ore_tub: the host holds the carried tub", 2.0, |w| items(w).host.locks.get(&TUB_A) == Some(&CARRIER) && items(w).client.holder(TUB_A) == Some(CARRIER)));
    s.push(Box::new(|w, c| {
        check(c, window(w).is_none(), "ore_tub: no window while the tub is carried".into());
        let now = w.resource::<Time>().elapsed_secs_f64();
        let r = w.resource_mut::<Items>().request(now, Request::Open { rack: TUB_A });
        check(c, r == Err(Reject::InUse), format!("ore_tub: an Open on the carried tub is refused ({r:?})"));
        let p = w.resource::<Interaction>().prompt.clone();
        check(c, !p.contains("Ore tub"), format!("ore_tub: the prompt does not offer it while carried ({p:?})"));
        true
    }));
    s.push(tap_then("ore_tub: F sets it down", KeyCode::KeyF, |w| held_tub(w).is_none()));
    s.push(until("ore_tub: the host gives the tub back", 2.0, |w| items(w).host.locks.get(&TUB_A).is_none() && items(w).client.holder(TUB_A).is_none()));

    // Someone picks it up while the window is open: the window closes, the lock changes hands.
    s.push(Box::new(|w, _| {
        let at = tub_at(w, TUB_A);
        look_at(w, at);
        true
    }));
    s.push(wait(0.2));
    s.push(tap_then("ore_tub: F opens it again", KeyCode::KeyF, |w| window(w) == Some(TUB_A) && items(w).client.holder(TUB_A) == Some(LOCAL)));
    s.push(Box::new(|w, c| {
        begin(w, c, "ore_tub: lifted with the window open");
        let e = tub_e(w, TUB_A);
        w.resource_mut::<Grab>().held = Some(Held { crate_e: e, reach: grab_core::Reach::Hands, dist: 0.8, breaker: Default::default(), hands: 2, mass: 25.0, yaw: 0.0, err: HoldError::default() });
        true
    }));
    s.push(until("ore_tub: the window closes and the carrier holds the tub", 2.0, |w| window(w).is_none() && !w.resource::<InventoryUi>().open && items(w).host.locks.get(&TUB_A) == Some(&CARRIER)));
    s.push(Box::new(|w, _| {
        w.resource_mut::<Grab>().release();
        true
    }));
    s.push(until("ore_tub: the tub is free again", 2.0, |w| items(w).host.locks.get(&TUB_A).is_none()));

    // The load slows the carrier: the full tub (A), then the empty one (B), the same run.
    s.extend(carry_run(TUB_A, TUB_B, "full"));
    s.extend(carry_run(TUB_B, TUB_A, "empty"));
    s.push(Box::new(|w, c| {
        let (full, empty) = (c.v["full"], c.v["empty"]);
        let share = grab_core::load_speed_share(18 / 3, 6, crate::ore_tub::FULL_SPEED_SHARE);
        check(c, full.is_finite() && empty.is_finite() && empty > 0.5, format!("ore_tub: the empty tub walks ({empty:.2} m in 1 s)"));
        check(c, full < empty, format!("ore_tub: the full tub walks slower than the empty one ({full:.2} m against {empty:.2} m in 1 s, wanted ratio {share:.2}, got {:.2})", full / empty));
        let _ = w;
        true
    }));

    // Saved and loaded: the load stays with the tub.
    s.push(Box::new(|w, c| {
        begin(w, c, "ore_tub: the load survives a save and a load");
        let now = w.resource::<Time>().elapsed_secs_f64();
        check(c, in_rack(w, TUB_A) == FULL, format!("ore_tub: the tub is full before the save ({})", in_rack(w, TUB_A)));
        let r = crate::savefile::save_now(w);
        check(c, r.is_ok(), format!("ore_tub: saved to the file ({r:?})"));
        w.resource_mut::<Items>().add_container(now, TUB_A, crate::ore_tub::contents());
        check(c, in_rack(w, TUB_A) == 0, "ore_tub: the tub is emptied after the save".into());
        let dir = w.resource::<crate::savefile::SaveDir>().0.clone();
        let text = crate::savefile::read_slot(&dir, crate::savefile::SLOT).ok().flatten().unwrap_or_default();
        match gameplay_core::save::Envelope::from_json(&text) {
            Ok(env) => {
                w.insert_resource(crate::savefile::PendingLoad(env));
            }
            Err(e) => check(c, false, format!("ore_tub: the file reads back ({e})")),
        }
        true
    }));
    s.push(until("ore_tub: the loaded tub has its ore back", 3.0, |w| in_rack(w, TUB_A) == FULL));
    s.push(Box::new(|w, c| {
        check(c, in_rack(w, TUB_B) == 0 && ore_total(w) == FULL, format!("ore_tub: the other tub is empty and nothing is doubled (all {})", ore_total(w)));
        true
    }));

    // The object budget leaves tubs alone: thirty plain crates over the cap, the tubs stay.
    s.push(Box::new(|w, c| {
        begin(w, c, "ore_tub: the object budget keeps the tubs");
        let table = w.resource::<crate::cargo::Crates>().0.clone();
        let cap = w.resource::<crate::cargo::ObjectBudget>().0.crates.cap;
        let pl = planet(w);
        c.v.insert("despawned0", w.resource::<crate::cargo::CargoStats>().despawned as f64);
        for i in 0..cap + 6 {
            let at = w.resource::<DevArea>().ground(&pl, [OPEN_GROUND[0] + (i % 8) as f64 * 1.3, OPEN_GROUND[1] + (i / 8) as f64 * 1.3], 0.0).unwrap().0;
            super::cargo::ground_crate(w, "small", at);
        }
        let _ = table;
        true
    }));
    s.push(until("ore_tub: the budget removes crates", 5.0, |w| w.resource::<crate::cargo::CargoStats>().despawned > 0));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let alive = w.query::<&Tub>().iter(w).count();
        let removed = w.resource::<crate::cargo::CargoStats>().despawned as f64 - c.v["despawned0"];
        check(c, alive == 2 && in_rack(w, TUB_A) == FULL, format!("ore_tub: both tubs stand after the budget removed {removed} crates ({alive} tubs)"));
        true
    }));

    // Set down at home, open, and the ore goes into Crate B.
    s.push(Box::new(|w, c| {
        begin(w, c, "ore_tub: the ore goes into Crate B");
        for rack in [TUB_A, TUB_B] {
            let at = home_of(w, rack);
            put_tub(w, rack, at);
        }
        let at = ground(w, STORAGE_ROOM.point([-1.4, 0.6]), 0.5);
        place_walker(w, at);
        true
    }));
    s.push(wait(0.6));
    s.push(Box::new(|w, _| {
        let at = tub_at(w, TUB_A);
        look_at(w, at);
        true
    }));
    s.push(wait(0.2));
    s.push(tap_then("ore_tub: F opens the full tub", KeyCode::KeyF, |w| window(w) == Some(TUB_A) && items(w).client.holder(TUB_A) == Some(LOCAL)));
    s.push(move_ore("ore_tub: the ore comes out into the bar", TUB_A, false));
    s.push(Box::new(|w, c| {
        let (bar, tub) = (items(w).host.inventory(LOCAL).count(ORE), in_rack(w, TUB_A));
        check(c, bar == FULL && tub == 0 && ore_total(w) == FULL, format!("ore_tub: the tub is empty, the bar has it all (bar {bar}, tub {tub}, all {})", ore_total(w)));
        true
    }));
    s.push(tap_then("ore_tub: F closes the tub", KeyCode::KeyF, |w| window(w).is_none()));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let at = ground(w, STORAGE_ROOM.point([0.0, -0.6]), 0.5);
        place_walker(w, at);
        let now = w.resource::<Time>().elapsed_secs_f64();
        let r = w.resource_mut::<Items>().request(now, Request::Open { rack: CRATE_B });
        check(c, r.is_ok(), format!("ore_tub: Crate B opens ({r:?})"));
        true
    }));
    s.push(wait(3.0 * DELAY));
    s.push(move_ore("ore_tub: the ore goes into Crate B", CRATE_B, true));
    s.push(Box::new(|w, c| {
        let (bar, tub, krate, all) = (items(w).host.inventory(LOCAL).count(ORE), in_rack(w, TUB_A) + in_rack(w, TUB_B), in_rack(w, CRATE_B), ore_total(w));
        check(c, bar == 0 && tub == 0 && krate == FULL && all == FULL, format!("ore_tub: tub 0, crate {krate}, bar {bar}, nothing lost or doubled (all {all})"));
        check(c, items(w).client.pending.is_empty(), "ore_tub: the client agrees".into());
        let now = w.resource::<Time>().elapsed_secs_f64();
        let _ = w.resource_mut::<Items>().request(now, Request::Close { rack: CRATE_B });
        end(w, c, "moved".into());
        true
    }));
    s.push(wait(0.3));

    // A picture of the two tubs from inside the doorway.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "ore_tub: the tubs from the middle of the room");
            let at = ground(w, STORAGE_ROOM.point([0.0, -0.9]), 0.5);
            place_walker(w, at);
            let look = ground(w, STORAGE_ROOM.point([0.0, 1.8]), 0.3);
            look_at(w, look);
        }
        c.t >= 0.8
    }));
    s.push(shot_step("ore-tubs"));
    // The picture is written a few frames after the request.
    s.push(wait(0.5));
}
