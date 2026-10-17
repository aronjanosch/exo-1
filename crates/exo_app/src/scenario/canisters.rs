//! Antimatter canisters by hand: load and brake pull (the plain canister; the hot and spicy canisters are parked, #306 Parkplatz).
use super::*;
use crate::controls::Controls;
use crate::dev_area::engineering::coolant::HOLD_S;
use crate::dev_area::{layout::{REACTOR_AIM_LIFT, REACTOR_AT, REACTOR_FRONT_AT, ROOM}};
use crate::dev_area::ship::ReactorSim;
use crate::dev_area::engineering::canisters::RACK_AT;
use crate::dev_area::DevArea;
use super::items::{collect, items, look_at, outside};
use crate::health::HealthState;
use crate::items::{Items, LOCAL};
use bevy::math::DVec3;
use item_core::wire::{ReactorAsk, ReactorFault};

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

fn canisters_in_reactor(w: &World) -> usize {
    w.resource::<ReactorSim>().room.reactor().canisters().len()
}

fn available(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().available_fill()
}

fn longest_life(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().canisters().iter().map(|r| r.life).fold(0.0, f64::max)
}

fn heat(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().heat()
}

fn hot_above(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().bands().heat_optimal.hi
}

#[allow(dead_code)] // parked (#306 Parkplatz): the spicy canister steps
fn radiation(w: &World) -> f64 {
    w.resource::<HealthState>().bodies.get(&LOCAL).map_or(0.0, |b| b.health.strength("radiation"))
}

fn burn(w: &World) -> f64 {
    w.resource::<HealthState>().bodies.get(&LOCAL).map_or(0.0, |b| b.health.strength("burn"))
}

const DIGITS: [KeyCode; 8] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];

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

/// Selects the slot that holds the item (or the empty slot when `def` is None: the hands empty).
fn select(name: &'static str, def: Option<&'static str>) -> Step {
    let mut slot = 0;
    run(name, 2.0, move |w, c| {
        if c.t == 0.0 {
            slot = match def {
                Some(d) => (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == d)).unwrap_or(0),
                None => (0..8).find(|&i| items(w).client.view.slot(i).is_none()).unwrap_or(0),
            };
            tap(w, DIGITS[slot]);
            return None;
        }
        if items(w).client.view.selected != slot {
            return None;
        }
        check(c, true, format!("{name}: slot {} selected", slot + 1));
        Some(format!("slot {}", slot + 1))
    })
}

pub(super) fn canisters_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        begin(w, c, "canisters: the rack and the host's body for the walker");
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && outside(w, "antimatter_canister").is_some() && w.resource::<HealthState>().bodies.contains_key(&LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "canisters: the rack and the walker's body are there".into());
        let p = ground(w, ROOM.point([RACK_AT[0], RACK_AT[1] - 0.8]), 0.0);
        place_walker(w, p);
        end(w, c, "at the rack".into());
        true
    }));
    s.extend(collect("antimatter_canister", "Antimatter canister"));
    s.push(run("canisters: the canister is in the bar", 2.0, |w, c| {
        let n = items(w).client.view.count("antimatter_canister");
        check(c, n == 1, format!("canisters: one canister in the bar ({n})"));
        Some(format!("{n} in the bar"))
    }));
    s.push(select("canisters: the canister is selected", Some("antimatter_canister")));

    s.push(run("canisters: the walker stands at the reactor", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, REACTOR_FRONT_AT, 0.0);
            place_walker(w, feet);
        }
        let target = ground(w, REACTOR_AT, REACTOR_AIM_LIFT);
        look_at(w, target);
        if c.t < 0.5 {
            return None;
        }
        Some("at the reactor".into())
    }));
    s.push(run("canisters: LMB held on the reactor loads the canister", HOLD_S + 4.0, {
        move |w, c| {
            if c.t == 0.0 {
                // The two slots start full: take one out so there is room to load.
                w.resource_mut::<ReactorSim>().room.reactor_mut().remove_canister(0);
                c.v.insert("n0", canisters_in_reactor(w) as f64);
                let target = ground(w, REACTOR_AT, REACTOR_AIM_LIFT);
                look_at(w, target);
                w.resource_mut::<Controls>().primary_held = true;
                return None;
            }
            let target = ground(w, REACTOR_AT, REACTOR_AIM_LIFT);
            look_at(w, target);
            if c.t < HOLD_S + 1.0 {
                return None;
            }
            w.resource_mut::<Controls>().primary_held = false;
            let n = canisters_in_reactor(w);
            check(c, n == c.v["n0"] as usize + 1, format!("canisters: the reactor has one canister more ({} -> {n})", c.v["n0"]));
            check(c, items(w).client.view.count("antimatter_canister") == 0, "canisters: the canister is gone from the bar".into());
            Some(format!("{n} canisters"))
        }
    }));

    s.push(run("canisters: the dev drain heats the core", 2.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::DrainCoolant));
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        Some("drained".into())
    }));
    s.push(run("canisters: the core passes the optimal band", 60.0, |w, c| {
        if heat(w) <= hot_above(w) {
            return None;
        }
        check(c, true, format!("canisters: the core is hot ({:.1} > {:.1})", heat(w), hot_above(w)));
        Some(format!("{:.1} degrees", heat(w)))
    }));
    s.push(select("canisters: the hands are empty", None));
    s.push(run("canisters: the walker stands at the reactor with empty hands", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, REACTOR_FRONT_AT, 0.0);
            place_walker(w, feet);
        }
        let target = ground(w, REACTOR_AT, REACTOR_AIM_LIFT);
        look_at(w, target);
        if c.t < 0.5 {
            return None;
        }
        Some("at the reactor".into())
    }));
    s.push(run("canisters: LMB held on the reactor pulls the canister with the most life", HOLD_S + 4.0, |w, c| {
        if c.t == 0.0 {
            c.v.insert("p0", canisters_in_reactor(w) as f64);
            c.v.insert("fill0", available(w));
            c.v.insert("life0", longest_life(w));
            c.v.insert("burn0", burn(w));
            let target = ground(w, REACTOR_AT, REACTOR_AIM_LIFT);
            look_at(w, target);
            w.resource_mut::<Controls>().primary_held = true;
            return None;
        }
        let target = ground(w, REACTOR_AT, REACTOR_AIM_LIFT);
        look_at(w, target);
        if c.t < HOLD_S + 1.0 {
            return None;
        }
        w.resource_mut::<Controls>().primary_held = false;
        let n = canisters_in_reactor(w);
        check(c, n + 1 == c.v["p0"] as usize, format!("canisters: one canister less in the reactor ({} -> {n})", c.v["p0"]));
        check(c, items(w).client.view.count("antimatter_canister") == 1, "canisters: the pulled canister is in the hand".into());
        Some(format!("{n} canisters"))
    }));
    s.push(run("canisters: the pulled canister keeps its life and the hand burns", 3.0, |w, c| {
        if c.t < 0.5 {
            return None;
        }
        let slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == "antimatter_canister")).unwrap_or(0);
        let life = items(w).client.view.slot(slot).and_then(|s| s.item.condition).unwrap_or(-1.0);
        // The most life there was when the hold began (the canisters burn a little meanwhile).
        check(c, (life - c.v["life0"]).abs() < 2.0, format!("canisters: the pulled canister keeps its life ({life:.1}, the most there was {:.1})", c.v["life0"]));
        check(c, burn(w) > c.v["burn0"], format!("canisters: the hand burns once ({:.1})", burn(w)));
        check(c, available(w) < c.v["fill0"], format!("canisters: the available fill falls ({:.0} -> {:.0})", c.v["fill0"], available(w)));
        Some(format!("life {life:.0}"))
    }));
    s.push(select("canisters: the pulled canister is selected", Some("antimatter_canister")));
    s.push(run("canisters: G drops the canister", 3.0, |w, c| {
        if c.t == 0.0 {
            tap(w, KeyCode::KeyG);
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        check(c, items(w).client.view.count("antimatter_canister") == 0, "canisters: the hand is empty after G".into());
        check(c, outside(w, "antimatter_canister").is_some(), "canisters: the canister lies on the floor".into());
        Some("dropped".into())
    }));

    // Parked (#306 Parkplatz): the spicy canister by the glow barrel is not placed, so its steps (collect, 3 s radiation) are off.
}
