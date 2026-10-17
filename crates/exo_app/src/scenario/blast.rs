//! The implosion blast reaches a walker two metres off, not one at 25 m (a unit test covers the falloff).
use super::*;
use crate::dev_area::engineering::blast::{fire_points, BlastFlash, PendingBlasts};
use crate::dev_area::ship::{route, ReactorSim};
use crate::dev_area::{hull::ShipAir, layout::REACTOR_FRONT_AT};
use crate::dev_area::ship::ReactorCounts;
use crate::dev_area::DevArea;
use crate::health::HealthState;
use crate::items::{Items, LOCAL};
use bevy::math::DVec3;
use reactor_core::{ReactorEvent, RoomEvent};

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

fn counts(w: &World) -> ReactorCounts {
    *w.resource::<ReactorCounts>()
}

fn strength(w: &World, id: &str) -> f64 {
    w.resource::<HealthState>().bodies.get(&LOCAL).map_or(0.0, |b| b.health.strength(id))
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

pub(super) fn blast_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        begin(w, c, "blast: the walker's body and the dev area");
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && w.resource::<HealthState>().bodies.contains_key(&LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "blast: the dev area and the walker's body are there".into());
        let feet = ground(w, REACTOR_FRONT_AT, 0.0);
        place_walker(w, feet);
        end(w, c, "two metres from the reactor".into());
        true
    }));

    s.push(run("blast: the core implodes with fill in it", 5.0, |w, c| {
        if c.t == 0.0 {
            c.v.insert("burn0", strength(w, "burn"));
            c.v.insert("rad0", strength(w, "radiation"));
            let events = w.resource_mut::<ReactorSim>().room.reactor_mut().force_implosion();
            let mut k = counts(w);
            for e in events {
                if let ReactorEvent::Blast(b) = e {
                    w.resource_mut::<PendingBlasts>().0.push(b);
                }
                route(RoomEvent::Reactor(e), &mut k);
            }
            *w.resource_mut::<ReactorCounts>() = k;
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        let k = counts(w);
        check(c, k.implosions == 1 && k.blasts == 1, format!("blast: one implode and one blast ({} / {})", k.implosions, k.blasts));
        Some("imploded".into())
    }));
    s.push(run("blast: the walker two metres off takes burn and radiation", 1.0, |w, c| {
        let (burn, rad) = (strength(w, "burn"), strength(w, "radiation"));
        check(c, burn > c.v["burn0"], format!("blast: the burn rises ({:.1} -> {burn:.1})", c.v["burn0"]));
        check(c, rad > c.v["rad0"] && rad > 0.0, format!("blast: the radiation rises ({:.1} -> {rad:.1})", c.v["rad0"]));
        Some(format!("burn {burn:.0}, radiation {rad:.0}"))
    }));
    s.push(run("blast: the local screen flashes", 1.0, |w, c| {
        check(c, w.resource::<BlastFlash>().0.is_some(), "blast: the local flash is on".into());
        Some("flash".into())
    }));
    s.push(run("blast: three fires start round the core", 1.0, |w, c| {
        let air = w.resource::<ShipAir>();
        let missing = fire_points().iter().filter(|p| !air.deck.fires().iter().any(|f| (f.pos.x - p[0]).hypot(f.pos.y - p[1]) < 0.6)).count();
        check(c, missing == 0, format!("blast: a fire at each of the three points ({missing} missing)"));
        Some(format!("{} fires", air.deck.fires().len()))
    }));
}
