//! The pump's coolant: it cools by default, the dev drain empties the tank, the core climbs without coolant, E-COOL.
//! (The refill by hand at the pump's housing is gone with the housing; the tank is refilled by a host hook here.)
use super::*;
use crate::dev_area::ship::ReactorSim;
use crate::items::Items;
use item_core::wire::{ReactorAsk, ReactorFault};

fn tank(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.pump().tank()
}

fn flow(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.flow()
}

fn heat(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().heat()
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

pub(super) fn coolant_steps(s: &mut Vec<Step>) {
    s.push(run("coolant: the pump runs and cools by default", 6.0, |w, c| {
        let ready = w.resource::<crate::dev_area::DevArea>().frame.is_some();
        if !ready || c.t < 1.0 {
            return None;
        }
        let pump = &w.resource::<ReactorSim>().room;
        check(c, pump.pump().on() && pump.flow() > 0.0 && pump.pump().tank() > 0.0, format!("coolant: the pump is on with flow {:.2} and {:.0} % in the tank", pump.flow(), pump.pump().tank()));
        Some(format!("tank {:.0} %", tank(w)))
    }));

    s.push(run("coolant: the dev drain empties the tank", 3.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Trigger(ReactorFault::DrainCoolant));
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        check(c, tank(w) == 0.0 && flow(w) == 0.0, format!("coolant: the tank is empty and no flow ({} %, {:.2})", tank(w), flow(w)));
        c.v.insert("t0", heat(w));
        Some(format!("tank {:.0} %", tank(w)))
    }));
// Short on purpose: 20 s above the optimal band starts a fire at the reactor.
    s.push(wait(6.0));
    s.push(run("coolant: the core climbs without coolant", 2.0, |w, c| {
        let (t0, t1) = (c.v["t0"], heat(w));
        Some(format!("{t0:.1} -> {t1:.1}"))
    }));

    s.push(run("coolant: a refilled tank brings the flow back", 3.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<ReactorSim>().room.pump_mut().refill(50.0);
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        check(c, flow(w) > 0.0, format!("coolant: the flow comes back ({:.2})", flow(w)));
        Some(format!("tank {:.0} %", tank(w)))
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
