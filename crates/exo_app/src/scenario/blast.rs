//! #300 (lane r19): the meltdown blast reaches the players. The core melts with fuel in it (the
//! harness's forced melt; the blast follows the melt in the same event list); the walker two metres
//! from the reactor takes burn and radiation, the local screen flashes, and three fires start round
//! the core. (A walker at 25 m takes nothing: the falloff is a unit test in `dev_area::blast`; the
//! scenario has one body.) Without a window; the walker is set down at the reactor (test setup).
use super::*;
use crate::dev_area::blast::{fire_points, BlastFlash, PendingBlasts};
use crate::dev_area::reactor::{route, ReactorSim};
use crate::dev_area::engine_room::{EngineAir, REACTOR_AT};
use crate::dev_area::reactor::ReactorCounts;
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

pub(super) fn blast_steps(s: &mut Vec<Step>) {
    // The walker's body and the dev area are there; the walker stands two metres north of the reactor.
    s.push(Box::new(|w, c| {
        begin(w, c, "blast: the walker's body and the dev area");
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && w.resource::<HealthState>().bodies.contains_key(&LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "blast: the dev area and the walker's body are there".into());
        let feet = ground(w, [REACTOR_AT[0], REACTOR_AT[1] + 2.0], 0.0);
        place_walker(w, feet);
        end(w, c, "two metres from the reactor".into());
        true
    }));

    // The melt: the harness's forced melt (`force_meltdown`, the timer does not run out in a script),
    // with the reactor's fuel in it. The events go the way `reactor_step` sends them: counted by
    // `route`, the blast queued for `reactor_blast` (the next tick).
    s.push(run("blast: the core melts with fuel in it", 5.0, |w, c| {
        if c.t == 0.0 {
            c.v.insert("burn0", strength(w, "burn"));
            c.v.insert("rad0", strength(w, "radiation"));
            let events = w.resource_mut::<ReactorSim>().room.reactor_mut().force_meltdown();
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
        check(c, k.meltdowns == 1 && k.blasts == 1, format!("blast: one melt and one blast ({} / {})", k.meltdowns, k.blasts));
        Some("melted".into())
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
        let air = w.resource::<EngineAir>();
        let missing = fire_points().iter().filter(|p| !air.deck.fires().iter().any(|f| (f.pos.x - p[0]).hypot(f.pos.y - p[1]) < 0.6)).count();
        check(c, missing == 0, format!("blast: a fire at each of the three points ({missing} missing)"));
        Some(format!("{} fires", air.deck.fires().len()))
    }));
}
