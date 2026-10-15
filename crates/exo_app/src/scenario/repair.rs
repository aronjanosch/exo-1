//! Repair at the machine, with the ring off (repair by holding).
use super::items::{collect, items, outside};
use walker_core::Frame as WalkFrame;
use super::*;
use crate::controls::Controls;
use crate::dev_area::engine_room::{COOLANT_PUMP_AT, JUNCTION_BOX_AT};
use crate::dev_area::{DevArea, ENGINEERING};
use crate::items::Items;
use crate::dev_area::reactor::ReactorSim;
use crate::repair::{RepairHurt, RepairPlay, RepairSessions};
use item_core::wire::{RepairAsk, RepairDevice, RepairTool, ReactorAsk, ReactorFault, ReactorNews};

const DIGITS: [KeyCode; 8] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

fn pump(w: &World) -> f64 {
    items(w).reactor.as_ref().map_or(1.0, |r: &ReactorNews| r.pump.share)
}

fn junction(w: &World) -> f64 {
    items(w).reactor.as_ref().map_or(1.0, |r: &ReactorNews| r.junction.share)
}

fn ring(w: &World) -> Option<&crate::repair::Play> {
    w.resource::<RepairPlay>().0.as_ref()
}

fn sessions(w: &World) -> usize {
    w.resource::<RepairSessions>().sessions.len()
}

/// The host's hits in the one session, 0 without one (the ring is off: none come).
fn hits(w: &World) -> u32 {
    w.resource::<RepairSessions>().sessions.first().map_or(0, |h| h.hits)
}

/// The walker's eye looks at a machine's middle (area point, 0.6 m up). Aimed in the world frame:
/// on foot the walker's forward and pitch are world values (the shared `look_at` turns in the ship's).
fn look(w: &mut World, at: [f64; 2]) {
    let target = ground(w, at, 0.6);
    with_player(w, |p| {
        let up = p.world_up(WalkFrame::IDENTITY);
        let eye = p.world_pos(WalkFrame::IDENTITY) + up * crate::walker::EYE_HEIGHT;
        let d = target - eye;
        let h = d - up * d.dot(up);
        if h.length() > 1e-9 {
            p.w.forward = h.normalize();
        }
        p.pitch = d.dot(up).atan2(h.length());
    });
}

fn lmb(w: &mut World) {
    w.resource_mut::<Controls>().click = true;
}

fn burn(w: &World) -> f64 {
    w.resource::<crate::health::HealthState>().bodies.get(&crate::items::LOCAL).map_or(0.0, |b| b.health.strength("burn"))
}

/// The walker's stun, s, as the client's ring has it (0 without a session).
fn stun(w: &World) -> f64 {
    ring(w).map_or(0.0, |p| p.stun)
}

/// Knocks the junction box or the pump down to `share` of full, on the host's room (the dev
/// console has no fault for it): the click test below half needs the machine below half.
fn knock_junction(w: &mut World, share: f64) {
    let mut sim = w.resource_mut::<ReactorSim>();
    let cond = sim.room.junction_mut().condition_mut();
    let max = cond.tuning().max;
    cond.damage((1.0 - share) * max);
}

fn knock_pump(w: &mut World, share: f64) {
    let mut sim = w.resource_mut::<ReactorSim>();
    let cond = sim.room.pump_mut().condition_mut();
    let max = cond.tuning().max;
    cond.damage((1.0 - share) * max);
}

/// A step with its own clock: `body` runs each tick and returns the note when it is done; the
/// step fails after `limit` seconds.
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

fn select(name: &'static str, def: &'static str) -> Step {
    let mut slot = 0;
    run(name, 2.0, move |w, c| {
        if c.t == 0.0 {
            slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == def)).unwrap_or(0);
            tap(w, DIGITS[slot]);
            return None;
        }
        let ok = items(w).client.view.selected == slot;
        if !ok {
            return None;
        }
        check(c, true, format!("{name}: slot {} selected", slot + 1));
        Some(format!("slot {}", slot + 1))
    })
}

pub(super) fn repair_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        begin(w, c, "repair: the table and the host's body for the walker");
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && outside(w, "wrench").is_some() && w.resource::<crate::health::HealthState>().bodies.contains_key(&crate::items::LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "repair: the table and the host's body for the walker are there".into());
        let p = ground(w, [ENGINEERING.centre[0] - 3.0, 1.6], 0.0);
        place_walker(w, p);
        end(w, c, "in front of the table".into());
        true
    }));
    s.extend(collect("wrench", "Wrench"));
    s.extend(collect("screwdriver", "Screwdriver"));
    s.push(select("repair: the wrench is selected", "wrench"));

    s.push(run("repair: the dev fault breaks the coolant pump", 5.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::PumpBreak));
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        let share = pump(w);
        c.v.insert("share0", share);
        Some(format!("share {share:.2}"))
    }));

    s.push(run("repair: the walker stands at the pump", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 1.4], 0.0);
            place_walker(w, feet);
        }
        look(w, COOLANT_PUMP_AT);
        if c.t < 0.5 {
            return None;
        }
        Some("at the pump".into())
    }));

    s.push(select("repair: the screwdriver is selected", "screwdriver"));
    s.push(run("repair: the screwdriver at the pump does nothing", 3.0, |w, c| {
        look(w, COOLANT_PUMP_AT);
        if c.t < 0.3 && !c.v.contains_key("sent_wrong") {
            return None;
        }
        if !c.v.contains_key("sent_wrong") {
            lmb(w);
            c.v.insert("sent_wrong", 1.0);
            return None;
        }
        if c.t < 1.5 {
            return None;
        }
        w.resource_mut::<Controls>().click = false;
        let share = pump(w);
        let (play, host) = (ring(w).is_none(), sessions(w) == 0);
        check(c, play && host, "repair: no session starts with the wrong tool".into());
        // The broken pump wears on its own; what matters is that nothing repairs it.
        check(c, share <= c.v["share0"] + 1e-9, format!("repair: the pump is not repaired ({:.3} to {share:.3})", c.v["share0"]));
        Some("nothing happens".into())
    }));

    s.push(select("repair: the wrench is selected again", "wrench"));
    s.push(run("repair: the wrench at the pump starts a session", 3.0, |w, c| {
        look(w, COOLANT_PUMP_AT);
        if c.t < 0.3 && !c.v.contains_key("sent_wrench") {
            return None;
        }
        if !c.v.contains_key("sent_wrench") {
            lmb(w);
            c.v.insert("sent_wrench", 1.0);
            return None;
        }
        if c.t < 0.8 {
            return None;
        }
        let started = ring(w).is_some() && sessions(w) == 1;
        let still_wrench = items(w).client.view.slot(items(w).client.view.selected).is_some_and(|s| s.item.kind.as_str() == "wrench");
        check(c, started, "repair: the wrench starts a session, host and client".into());
        check(c, still_wrench, "repair: the wrench stays in the hand (LMB does not throw or use it)".into());
        Some("session on".into())
    }));

    s.push(run("repair: a broken pump rises at once by holding (no ring, no hits)", 4.0, |w, c| {
        look(w, COOLANT_PUMP_AT);
        let zones = ring(w).map_or(0, |p| p.session.ring.zones.len()) as f64;
        c.v.insert("zones", c.v.get("zones").copied().unwrap_or(0.0).max(zones));
        if c.t < 2.5 {
            return None;
        }
        check(c, c.v["zones"] == 0.0, "repair: no ring (no zones) with the minigame off".into());
        check(c, hits(w) == 0, "repair: no hits (none come with the ring off)".into());
        Some("held".into())
    }));

    s.push(run("repair: the pump rises at the normal speed while held", 5.0, |w, c| {
        look(w, COOLANT_PUMP_AT);
        if c.t < 0.2 {
            c.v.insert("rise0", pump(w));
            return None;
        }
        if c.t < 3.0 {
            return None;
        }
        let rise = pump(w) - c.v["rise0"];
        Some(format!("rise {rise:.3}"))
    }));

    // straight after, so the step watches for the session's end with the pump near full, having been full.
    s.push(run("repair: the pump is repaired to full and the session ends", 90.0, |w, c| {
        look(w, COOLANT_PUMP_AT);
        let share = pump(w);
        let peak = c.v.get("peak").copied().unwrap_or(0.0).max(share);
        c.v.insert("peak", peak);
        if peak >= 0.999 && ring(w).is_none() && sessions(w) == 0 {
            w.resource_mut::<Controls>().click = false;
            return Some(format!("full at {:.1} s", c.t));
        }
        None
    }));

    s.push(run("repair: a surge takes the junction box to 70 %", 5.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::Surge));
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        let share = junction(w);
        c.v.insert("junction0", share);
        Some(format!("share {share:.2}"))
    }));
    s.push(select("repair: the screwdriver is selected for the junction box", "screwdriver"));
    s.push(run("repair: the walker walks to the junction box", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, [JUNCTION_BOX_AT[0], JUNCTION_BOX_AT[1] + 1.2], 0.0);
            place_walker(w, feet);
        }
        look(w, JUNCTION_BOX_AT);
        if c.t < 0.5 {
            return None;
        }
        Some("at the junction box".into())
    }));
    s.push(run("repair: the screwdriver starts a session on the junction box with no ring", 6.0, |w, c| {
        look(w, JUNCTION_BOX_AT);
        if c.t < 0.3 && !c.v.contains_key("sent_junction") {
            return None;
        }
        if !c.v.contains_key("sent_junction") {
            lmb(w);
            c.v.insert("sent_junction", 1.0);
            return None;
        }
        if c.t < 0.6 {
            return None;
        }
        if c.t > 0.6 && c.t < 3.6 {
            lmb(w);
        }
        if c.t < 3.6 {
            return None;
        }
        let no_ring = ring(w).is_some_and(|p| p.session.ring.zones.is_empty() && p.session.ring.pointer_deg == 0.0);
        check(c, no_ring, "repair: no zones and no pointer at 70 %".into());
        check(c, hits(w) == 0, "repair: LMB counts for nothing without the minigame".into());
        Some("no ring, slow repair".into())
    }));
    s.push(run("repair: F ends the session and keeps the progress", 2.0, |w, c| {
        if c.t == 0.0 {
            let share = junction(w);
            c.v.insert("before_f", share);
            tap(w, KeyCode::KeyF);
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        let (ended, host) = (ring(w).is_none(), sessions(w) == 0);
        check(c, ended && host, "repair: F ends the session, host and client".into());
        check(c, junction(w) >= c.v["before_f"], "repair: the progress is kept".into());
        Some("ended".into())
    }));
    s.push(run("repair: the slow repair takes the junction box to full", 60.0, |w, c| {
        look(w, JUNCTION_BOX_AT);
        if ring(w).is_none() && sessions(w) == 0 && c.t < 1.0 && !c.v.contains_key("again") {
            lmb(w);
            c.v.insert("again", 1.0);
            return None;
        }
        if junction(w) >= 1.0 {
            let full_at = *c.v.entry("full_at2").or_insert(c.t);
            if c.t - full_at < 0.5 {
                return None;
            }
            check(c, ring(w).is_none() && sessions(w) == 0, "repair: the session ends at full".into());
            return Some(format!("full at {:.1} s", full_at));
        }
        None
    }));

    // so no session starts, and the refusal reaches the client's flag on the next tick (the
    // client reads it before it asks again).
    s.push(run("repair: a refused start (the wrong tool at a full box) is told to the client", 3.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Items>().repair_asks.push((crate::items::LOCAL, RepairAsk::Start { device: RepairDevice::JunctionBox, tool: RepairTool::Wrench }));
            return None;
        }
        let told = w.resource::<Items>().repair_done;
        if !told && c.t < 0.5 {
            return None;
        }
        check(c, told, "repair: the refusal reaches the client (its ring ends)".into());
        check(c, sessions(w) == 0 && ring(w).is_none(), "repair: a refused start opens no session".into());
        Some("refused".into())
    }));

    s.push(run("repair: a dev fault knocks the junction box below half", 3.0, |w, c| {
        if c.t == 0.0 {
            knock_junction(w, 0.4);
            return None;
        }
        if c.t < 0.5 || junction(w) >= 0.5 {
            return None;
        }
        Some(format!("junction {:.2}", junction(w)))
    }));
    s.push(select("repair: the screwdriver is selected for the click", "screwdriver"));
    s.push(run("repair: a click at the junction box below half does nothing (no ring, no burn, no stun)", 12.0, |w, c| {
        look(w, JUNCTION_BOX_AT);
        if !c.v.contains_key("j_started") {
            if c.t < 0.3 {
                return None;
            }
            lmb(w);
            c.v.insert("j_started", 1.0);
            return None;
        }
        if !c.v.contains_key("j_click") {
            if sessions(w) == 0 {
                return None;
            }
            c.v.insert("j_burn0", burn(w));
            c.v.insert("j_hurt0", w.resource::<RepairHurt>().at.unwrap_or(-1.0));
            c.v.insert("j_share0", junction(w));
            c.v.insert("j_click", c.t);
            lmb(w);
            return None;
        }
        if c.t - c.v["j_click"] < 2.0 {
            return None;
        }
        let burn_now = burn(w);
        let burn0 = c.v["j_burn0"];
        check(c, burn_now <= burn0 + 1e-9, format!("repair: the click burns nothing ({burn0:.2} to {burn_now:.2})"));
        check(c, stun(w) == 0.0, "repair: the click does not stun".into());
        check(c, ring(w).is_some_and(|p| p.session.ring.zones.is_empty()), "repair: no zones at the junction box".into());
        check(c, junction(w) > c.v["j_share0"], format!("repair: the box still rises by holding ({:.3} to {:.3})", c.v["j_share0"], junction(w)));
        Some(format!("burn {burn0:.2} to {burn_now:.2}"))
    }));

    s.push(run("repair: the fault knocks the coolant pump below half", 3.0, |w, c| {
        if c.t == 0.0 {
            knock_pump(w, 0.4);
            return None;
        }
        if c.t < 0.5 || pump(w) >= 0.5 {
            return None;
        }
        Some(format!("pump {:.2}", pump(w)))
    }));
    s.push(select("repair: the wrench is selected for the knock", "wrench"));
    s.push(run("repair: the walker stands at the pump for the knock", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 1.4], 0.0);
            place_walker(w, feet);
        }
        look(w, COOLANT_PUMP_AT);
        if c.t < 0.5 {
            return None;
        }
        Some("at the pump".into())
    }));
    s.push(run("repair: a click at the pump below half does nothing (no ring, no stun, no clank)", 12.0, |w, c| {
        look(w, COOLANT_PUMP_AT);
        if !c.v.contains_key("p_started") {
            if c.t < 0.3 {
                return None;
            }
            lmb(w);
            c.v.insert("p_started", 1.0);
            return None;
        }
        if !c.v.contains_key("p_click") {
            if sessions(w) == 0 {
                return None;
            }
            c.v.insert("p_burn0", burn(w));
            c.v.insert("p_hurt0", w.resource::<RepairHurt>().at.unwrap_or(-1.0));
            c.v.insert("p_share0", pump(w));
            c.v.insert("p_click", c.t);
            lmb(w);
            return None;
        }
        if c.t - c.v["p_click"] < 2.0 {
            return None;
        }
        let burn_now = burn(w);
        let burn0 = c.v["p_burn0"];
        check(c, burn_now <= burn0 + 1e-9, format!("repair: the click burns nothing ({burn0:.2} to {burn_now:.2})"));
        check(c, stun(w) == 0.0, "repair: the click does not stun".into());
        check(c, ring(w).is_some_and(|p| p.session.ring.zones.is_empty()), "repair: no zones at the pump".into());
        check(c, pump(w) > c.v["p_share0"], format!("repair: the pump still rises by holding ({:.3} to {:.3})", c.v["p_share0"], pump(w)));
        Some(format!("burn {burn0:.2} to {burn_now:.2}"))
    }));
}
