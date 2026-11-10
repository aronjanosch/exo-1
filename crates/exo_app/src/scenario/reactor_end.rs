//! The reactor's bad end (#325): the dev menu makes the core critical; thrown out in time, it implodes outside and hurts the
//! walker near it and spares the one beyond the safe distance, with no blast and no hull damage; left aboard, it ends the game.
use super::*;
use crate::dev::DevMenu;
use crate::dev_area::engineering::outside_core::{distance_to_core, CORE_OUT_AT};
use crate::dev_area::hull::ShipAlarm;
use crate::dev_area::ship::{ReactorCounts, ReactorSim, ShipEvents};
use crate::dev_area::DevArea;
use crate::gameover::GameOver;
use crate::health::HealthState;
use crate::items::{Items, LOCAL};
use bevy::math::DVec3;
use item_core::wire::{Alarm, ReactorAsk};

/// The second player's id (a remote walker, no network).
const FAR: u32 = 2;

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

fn counts(w: &World) -> ReactorCounts {
    *w.resource::<ReactorCounts>()
}

fn burn(w: &World, id: u32) -> f64 {
    w.resource::<HealthState>().bodies.get(&id).map_or(0.0, |b| b.health.strength("burn"))
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

fn critical(name: &'static str) -> Step {
    run(name, 10.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<DevMenu>().critical_reactor = true;
            return None;
        }
        let timer = w.resource::<ReactorSim>().room.reactor().timers().1;
        let eta = w.resource::<Items>().reactor.and_then(|n| n.implosion_eta_s);
        let danger = w.resource::<ShipAlarm>().level == Alarm::Danger;
        (timer >= 0.7 && eta.is_some() && danger).then(|| {
            format!("timer {timer:.2}, eta {:.0} s, Danger alarm", eta.unwrap_or(0.0))
        })
    })
}

pub(super) fn reactor_end_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        begin(w, c, "reactor end: the walker's body and the dev area");
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && w.resource::<HealthState>().bodies.contains_key(&LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "reactor end: the dev area and the walker's body are there".into());
        check(c, w.resource::<ReactorSim>().room.reactor().power_on(), "reactor end: the reactor runs".into());
        end(w, c, "ready".into());
        true
    }));

    // (a) Critical, thrown out: the near walker burns, the far one does not.
    s.push(critical("reactor end: the dev menu makes the core critical, the countdown runs"));
    s.push(run("reactor end: two walkers, one near the ejected core's spot, one beyond the safe distance", 2.0, |w, c| {
        let safe = w.resource::<crate::dev_area::ship::ReactorConfig>().0.reactor.blast.safe_distance_m;
        let near = [CORE_OUT_AT[0], CORE_OUT_AT[1] + 3.0];
        let far = [CORE_OUT_AT[0] + safe + 5.0, CORE_OUT_AT[1]];
        check(c, distance_to_core(near) < safe * 0.5 && distance_to_core(far) > safe, "reactor end: the spots are inside half the safe distance and beyond it".into());
        let feet = ground(w, near, 0.0);
        place_walker(w, feet);
        let at = ground(w, far, 0.1);
        let up = planet(w).up(at);
        w.spawn((crate::net_live::RemoteWalker { owner: FAR }, crate::origin::WorldPose { pos: at, rot: walker_core::look_rot(DVec3::X, up) }, Transform::default(), Visibility::default()));
        w.resource_mut::<HealthState>().body(FAR);
        Some("placed".into())
    }));
    s.push(run("reactor end: the eject sends the countdown outside", 5.0, |w, c| {
        if c.t == 0.0 {
            c.v.insert("near0", burn(w, LOCAL));
            c.v.insert("far0", burn(w, FAR));
            let events = w.resource_mut::<ReactorSim>().room.eject_core();
            w.resource_mut::<ShipEvents>().0.extend(events);
            return None;
        }
        let n = w.resource::<Items>().reactor?;
        (n.ejected && n.outside_eta_s.is_some()).then(|| {
            check(c, n.outside_eta_s.is_some_and(|e| e > 0.0 && e < 120.0), "reactor end: the news carry the outside countdown".into());
            check(c, n.implosion_eta_s == n.outside_eta_s, "reactor end: the screen's countdown is the outside one".into());
            format!("{:.0} s left outside", n.outside_eta_s.unwrap_or(0.0))
        })
    }));
    s.push(run("reactor end: the core implodes outside", 90.0, |w, c| {
        (counts(w).core_implosions == 1).then(|| {
            let k = counts(w);
            check(c, k.implosions == 0 && k.blasts == 0, format!("reactor end: no implosion and no blast aboard ({} / {})", k.implosions, k.blasts));
            "imploded outside".into()
        })
    }));
    s.push(run("reactor end: the near walker is hurt, the far one is not, the hull is whole, the game goes on", 3.0, |w, c| {
        if c.t < 0.5 {
            return None;
        }
        let (near, far) = (burn(w, LOCAL), burn(w, FAR));
        check(c, near > c.v["near0"], format!("reactor end: the near walker burns ({:.1} -> {near:.1})", c.v["near0"]));
        check(c, far <= c.v["far0"] + 1e-9, format!("reactor end: the far walker is spared ({:.1} -> {far:.1})", c.v["far0"]));
        check(c, w.resource::<ReactorSim>().room.hull().intact(), "reactor end: no hull damage".into());
        check(c, !w.contains_resource::<GameOver>(), "reactor end: no game over for the outside implosion".into());
        check(c, w.resource::<crate::dev_area::engineering::blast::BlastFlash>().0.is_some(), "reactor end: the flash shows".into());
        Some(format!("near {near:.0}, far {far:.0}"))
    }));

    // (b) Critical, left aboard: game over for the crew.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "reactor end: a fresh room, running");
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Reset);
            // The eject dropped the stand's lever and toggles (the core was gone): the stand matches the fresh warm room again.
            let mut q = w.query_filtered::<&mut crate::controls_kit::Machine, With<crate::dev_area::engineering::reactor_room::ReactorRoom>>();
            if let Ok(mut m) = q.single_mut(w) {
                m.controls = crate::dev_area::engineering::reactor_room::machine().controls;
            }
            return false;
        }
        let ok = w.resource::<ReactorSim>().room.reactor().power_on();
        if !ok && c.t < 20.0 {
            return false;
        }
        check(c, ok, "reactor end: the reactor runs again after the reset".into());
        end(w, c, "running".into());
        true
    }));
    s.push(critical("reactor end: critical again"));
    s.push(run("reactor end: left aboard, the core implodes and the game is over", 90.0, |w, c| {
        let over = w.contains_resource::<GameOver>();
        over.then(|| {
            let k = counts(w);
            check(c, k.implosions == 1, format!("reactor end: one implosion aboard ({})", k.implosions));
            "game over".into()
        })
    }));
}
