//! The pump's coolant tank: the drain, the refill by hand and E-COOL.
use super::*;
use crate::controls::Controls;
use crate::dev_area::engineering::coolant::{HOLD_S, RACK_AT};
use crate::dev_area::{layout::{COOLANT_PUMP_AT, CORRIDOR}};
use crate::dev_area::reactor::ReactorSim;
use crate::dev_area::reactor_screen::PUMP_ID;
use crate::displays::Panels;
use crate::dev_area::DevArea;
use super::items::{collect, items, look_at, outside};
use crate::items::Items;
use bevy::math::DVec3;
use item_core::wire::{ReactorAsk, ReactorFault};

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

fn tank(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.pump().tank()
}

fn flow(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.flow()
}

fn temp(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().temperature()
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

fn select(name: &'static str, def: &'static str) -> Step {
    let mut slot = 0;
    run(name, 2.0, move |w, c| {
        if c.t == 0.0 {
            slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == def)).unwrap_or(0);
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

pub(super) fn coolant_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        begin(w, c, "coolant: the rack and the host's body for the walker");
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && outside(w, "coolant_canister").is_some() && w.resource::<crate::health::HealthState>().bodies.contains_key(&crate::items::LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "coolant: the rack and the walker's body are there".into());
        let p = ground(w, CORRIDOR.point([RACK_AT[0] - 0.8, RACK_AT[1]]), 0.0);
        place_walker(w, p);
        end(w, c, "at the rack".into());
        true
    }));
    s.extend(collect("coolant_canister", "Coolant canister"));
    s.push(run("coolant: the stack of three is in the bar", 2.0, |w, _| (items(w).client.view.count("coolant_canister") == 3).then(|| "three canisters".into())));
    s.push(select("coolant: the canisters are selected", "coolant_canister"));

    s.push(run("coolant: the dev drain empties the tank", 3.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::DrainCoolant));
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        check(c, tank(w) == 0.0 && flow(w) == 0.0, format!("coolant: the tank is empty and no flow ({} %, {:.2})", tank(w), flow(w)));
        let panel = w.resource::<Panels>().get(&PUMP_ID).map(|p| format!("{p:?}")).unwrap_or_default();
        check(c, panel.contains("EMPTY"), "coolant: the pump screen says EMPTY".into());
        c.v.insert("t0", temp(w));
        Some(format!("tank {:.0} %", tank(w)))
    }));
// Short on purpose: 20 s above the optimal band starts a fire at the reactor.
    s.push(wait(6.0));
    s.push(run("coolant: the core climbs without coolant", 2.0, |w, c| {
        let (t0, t1) = (c.v["t0"], temp(w));
        Some(format!("{t0:.1} -> {t1:.1}"))
    }));

    s.push(run("coolant: the walker stands at the pump", 3.0, |w, c| {
        if c.t == 0.0 {
            let feet = ground(w, [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 1.4], 0.0);
            place_walker(w, feet);
        }
        let target = ground(w, COOLANT_PUMP_AT, 0.6);
        look_at(w, target);
        if c.t < 0.5 {
            return None;
        }
        Some("at the pump".into())
    }));
    s.push(run("coolant: LMB held on the pump refills the tank", HOLD_S + 4.0, |w, c| {
        if c.t == 0.0 {
            let target = ground(w, COOLANT_PUMP_AT, 0.6);
            look_at(w, target);
            w.resource_mut::<Controls>().primary_held = true;
            return None;
        }
        let target = ground(w, COOLANT_PUMP_AT, 0.6);
        look_at(w, target);
        if c.t < HOLD_S + 1.0 {
            return None;
        }
        w.resource_mut::<Controls>().primary_held = false;
        check(c, flow(w) > 0.0, format!("coolant: the flow comes back ({:.2})", flow(w)));
        Some(format!("tank {:.0} %", tank(w)))
    }));
    s.push(run("coolant: one canister is used up", 3.0, |w, c| {
        if c.t < 0.5 {
            return None;
        }
        let n = items(w).client.view.count("coolant_canister");
        check(c, n == 2, format!("coolant: two canisters are left ({n})"));
        Some(format!("{n} left"))
    }));
    s.push(run("coolant: E-COOL takes its tank cost and starts the cooldown", 3.0, |w, c| {
        if c.t == 0.0 {
            c.v.insert("tank", tank(w));
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::EmergencyCool);
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        let (before, now) = (c.v["tank"], tank(w));
        let (cost, ready) = {
            let room = &w.resource::<ReactorSim>().room;
            (room.tuning().ecool_tank_cost, room.ecool_ready_in_s())
        };
        // The coolant use of the half second is taken off too: a tolerance of a point.
        check(c, (before - now - cost).abs() < 1.0, format!("coolant: E-COOL takes {cost} % of the tank ({before:.1} -> {now:.1})"));
        Some(format!("tank {now:.0} %, ready in {ready:.0} s"))
    }));
}
