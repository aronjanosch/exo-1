//! Fuel rods by hand. The corridor's rod rack holds two standard rods and a hot
//! one; the walker takes the hot rod (F: the only one of its kind), it is selected, LMB held on the reactor with it loads it (the
//! rods in the reactor go from two to three, the rod is gone from the hand). The dev drain makes
//! the core hot; with empty hands and LMB held on the reactor the brake pulls the rod with the most
//! life into the hand (the reactor's available fuel falls, the hand burns once). G drops it.
//! Without a window; the walker is set down at each place (test setup).
use super::*;
use crate::controls::Controls;
use crate::dev_area::coolant::HOLD_S;
use crate::dev_area::engine_room::{CORRIDOR, REACTOR_AT};
use crate::dev_area::hazards::{GLOW_BARREL, SPICY_AT};
use crate::dev_area::reactor::ReactorSim;
use crate::dev_area::rods::RACK_AT;
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

fn rods_in_reactor(w: &World) -> usize {
    w.resource::<ReactorSim>().room.reactor().rods().len()
}

fn available(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().available_fuel()
}

fn longest_life(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().rods().iter().map(|r| r.life).fold(0.0, f64::max)
}

fn temp(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().temperature()
}

fn hot_above(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().bands().temp_optimal.hi
}

fn radiation(w: &World) -> f64 {
    w.resource::<HealthState>().bodies.get(&LOCAL).map_or(0.0, |b| b.health.strength("radiation"))
}

fn burn(w: &World) -> f64 {
    w.resource::<HealthState>().bodies.get(&LOCAL).map_or(0.0, |b| b.health.strength("burn"))
}

const DIGITS: [KeyCode; 8] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];

/// A step that runs `body` each frame until it returns a note (then ends) or the limit passes.
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

pub(super) fn rods_steps(s: &mut Vec<Step>) {
    // The rack: the walker beside it in the walkway, the rods on it (two standard, one hot).
    s.push(Box::new(|w, c| {
        begin(w, c, "rods: the rack and the host's body for the walker");
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && outside(w, "fuel_rod_hot").is_some() && w.resource::<HealthState>().bodies.contains_key(&LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "rods: the rack and the walker's body are there".into());
        let p = ground(w, CORRIDOR.point([RACK_AT[0] + 0.35, RACK_AT[1]]), 0.0);
        place_walker(w, p);
        end(w, c, "at the rack".into());
        true
    }));
    s.extend(collect("fuel_rod_hot", "Hot fuel rod"));
    s.push(run("rods: the rod is in the bar", 2.0, |w, c| {
        let n = items(w).client.view.count("fuel_rod_hot");
        check(c, n == 1, format!("rods: one rod in the bar ({n})"));
        Some(format!("{n} in the bar"))
    }));
    s.push(select("rods: the rod is selected", Some("fuel_rod_hot")));

    // Load: the walker at the reactor's front, the look on it, LMB held.
    s.push(run("rods: the walker stands at the reactor", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, [REACTOR_AT[0], REACTOR_AT[1] + 1.4], 0.0);
            place_walker(w, feet);
        }
        let target = ground(w, REACTOR_AT, 1.0);
        look_at(w, target);
        if c.t < 0.5 {
            return None;
        }
        Some("at the reactor".into())
    }));
    s.push(run("rods: LMB held on the reactor loads the rod", HOLD_S + 4.0, {
        move |w, c| {
            if c.t == 0.0 {
                c.v.insert("n0", rods_in_reactor(w) as f64);
                let target = ground(w, REACTOR_AT, 1.0);
                look_at(w, target);
                w.resource_mut::<Controls>().primary_held = true;
                return None;
            }
            let target = ground(w, REACTOR_AT, 1.0);
            look_at(w, target);
            if c.t < HOLD_S + 1.0 {
                return None;
            }
            w.resource_mut::<Controls>().primary_held = false;
            let n = rods_in_reactor(w);
            check(c, n == c.v["n0"] as usize + 1, format!("rods: the reactor has one rod more ({} -> {n})", c.v["n0"]));
            check(c, items(w).client.view.count("fuel_rod_hot") == 0, "rods: the rod is gone from the bar".into());
            Some(format!("{n} rods"))
        }
    }));

    // The brake: the core heats up without coolant (the dev drain), empty hands, LMB held.
    s.push(run("rods: the dev drain heats the core", 2.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::DrainCoolant));
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        Some("drained".into())
    }));
    s.push(run("rods: the core passes the optimal band", 60.0, |w, c| {
        if temp(w) <= hot_above(w) {
            return None;
        }
        check(c, true, format!("rods: the core is hot ({:.1} > {:.1})", temp(w), hot_above(w)));
        Some(format!("{:.1} degrees", temp(w)))
    }));
    s.push(select("rods: the hands are empty", None));
    s.push(run("rods: the walker stands at the reactor with empty hands", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, [REACTOR_AT[0], REACTOR_AT[1] + 1.4], 0.0);
            place_walker(w, feet);
        }
        let target = ground(w, REACTOR_AT, 1.0);
        look_at(w, target);
        if c.t < 0.5 {
            return None;
        }
        Some("at the reactor".into())
    }));
    s.push(run("rods: LMB held on the reactor pulls the rod with the most life", HOLD_S + 4.0, |w, c| {
        if c.t == 0.0 {
            c.v.insert("p0", rods_in_reactor(w) as f64);
            c.v.insert("fuel0", available(w));
            c.v.insert("life0", longest_life(w));
            c.v.insert("burn0", burn(w));
            let target = ground(w, REACTOR_AT, 1.0);
            look_at(w, target);
            w.resource_mut::<Controls>().primary_held = true;
            return None;
        }
        let target = ground(w, REACTOR_AT, 1.0);
        look_at(w, target);
        if c.t < HOLD_S + 1.0 {
            return None;
        }
        w.resource_mut::<Controls>().primary_held = false;
        let n = rods_in_reactor(w);
        check(c, n + 1 == c.v["p0"] as usize, format!("rods: one rod less in the reactor ({} -> {n})", c.v["p0"]));
        check(c, items(w).client.view.count("fuel_rod_hot") == 1, "rods: the pulled rod is in the hand".into());
        Some(format!("{n} rods"))
    }));
    s.push(run("rods: the pulled rod keeps its life and the hand burns", 3.0, |w, c| {
        if c.t < 0.5 {
            return None;
        }
        let slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == "fuel_rod_hot")).unwrap_or(0);
        let life = items(w).client.view.slot(slot).and_then(|s| s.item.condition).unwrap_or(-1.0);
        // The most life there was when the hold began (the rods burn a little meanwhile).
        check(c, (life - c.v["life0"]).abs() < 2.0, format!("rods: the pulled rod keeps its life ({life:.1}, the most there was {:.1})", c.v["life0"]));
        check(c, burn(w) > c.v["burn0"], format!("rods: the hand burns once ({:.1})", burn(w)));
        check(c, available(w) < c.v["fuel0"], format!("rods: the available fuel falls ({:.0} -> {:.0})", c.v["fuel0"], available(w)));
        Some(format!("life {life:.0}"))
    }));
    s.push(select("rods: the pulled rod is selected", Some("fuel_rod_hot")));
    s.push(run("rods: G drops the rod", 3.0, |w, c| {
        if c.t == 0.0 {
            tap(w, KeyCode::KeyG);
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        check(c, items(w).client.view.count("fuel_rod_hot") == 0, "rods: the hand is empty after G".into());
        check(c, outside(w, "fuel_rod_hot").is_some(), "rods: the rod lies on the floor".into());
        Some("dropped".into())
    }));

    // The spicy rod: it lies on the pad by the glowing barrel. The walker stands east of it,
    // outside the barrel's glow, takes it (F), and holds it 3 s: the radiation rises by about 3.
    s.push(run("rods: the spicy rod lies by the barrel", 8.0, |w, c| {
        if outside(w, "fuel_rod_spicy").is_none() {
            return None;
        }
        check(c, true, "rods: the spicy rod lies on the floor".into());
        Some("there".into())
    }));
    s.push(run("rods: the walker stands east of the spicy rod, out of the glow", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, GLOW_BARREL.point([SPICY_AT[0] + 0.95, SPICY_AT[1]]), 0.0);
            place_walker(w, feet);
        }
        let rod = ground(w, GLOW_BARREL.point(SPICY_AT), 0.3);
        look_at(w, rod);
        if c.t < 0.5 {
            return None;
        }
        Some("beside the rod".into())
    }));
    s.extend(collect("fuel_rod_spicy", "Spicy fuel rod"));
    s.push(run("rods: the spicy rod is in the bar and the walker holds it", 3.0, |w, c| {
        if c.t < 0.5 {
            return None;
        }
        let n = items(w).client.view.count("fuel_rod_spicy");
        check(c, n == 1, format!("rods: one spicy rod in the bar ({n})"));
        Some("held".into())
    }));
    s.push(run("rods: holding the spicy rod 3 s raises the radiation by about 3", 6.0, |w, c| {
        if c.t == 0.0 {
            c.v.insert("rad0", radiation(w));
            return None;
        }
        if c.t < 3.0 {
            return None;
        }
        let rose = radiation(w) - c.v["rad0"];
        check(c, (2.5..=3.5).contains(&rose), format!("rods: the radiation rose by about 3 in 3 s ({rose:.2})"));
        Some(format!("+{rose:.2}"))
    }));
}
