//! The coolant tank on the engine room wall (#331): it drains while the reactor runs, a coolant canister held at it (LMB, 3 s) raises
//! the level by the canister's share and is used up, a full tank refuses and keeps the canister, a dry tank is no flow and the core
//! climbs, a refill brings the flow back; E-COOL still takes its tank cost. With a window a picture of the half full tank.
use super::items::{items, look_at, prompt};
use super::*;
use crate::controls::Controls;
use crate::dev_area::engineering::coolant::HOLD_S;
use crate::dev_area::engineering::coolant_tank::{CoolantTank, TUBE_FOOT, TUBE_H};
use crate::dev_area::ship::ReactorSim;
use crate::items::{Items, LOCAL};
use item_core::wire::{ReactorAsk, ReactorFault};
use item_core::Request;

const CANISTER: &str = "coolant_canister";
const DIGITS: [KeyCode; 8] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];

fn tank(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.pump().tank()
}

fn set_tank(w: &mut World, pct: f64) {
    w.resource_mut::<ReactorSim>().room.pump_mut().set_tank(pct);
}

fn flow(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.flow()
}

fn heat(w: &World) -> f64 {
    w.resource::<ReactorSim>().room.reactor().heat()
}

fn canisters(w: &World) -> u32 {
    items(w).client.view.count(CANISTER)
}

fn the_tank(w: &mut World) -> Option<CoolantTank> {
    let mut q = w.query::<&CoolantTank>();
    q.iter(w).next().copied()
}

/// The glass's middle, what the view aims at.
fn glass(w: &mut World) -> DVec3 {
    the_tank(w).expect("the coolant tank stands there").world([0.0, TUBE_FOOT + TUBE_H * 0.5, 0.1])
}

/// Stands `out` metres in front of the tank.
fn stand_before(w: &mut World, out: f64) {
    let at = the_tank(w).expect("the coolant tank stands there").world([0.0, 0.0, out]);
    place_walker(w, at);
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

/// Holds LMB at the tank for `HOLD_S` and a little over, the walker looking at the glass, then lets go.
fn hold(name: &'static str) -> Step {
    run(name, HOLD_S + 4.0, |w, c| {
        let at = glass(w);
        look_at(w, at);
        if c.t == 0.0 {
            c.v.insert("tank0", tank(w));
            c.v.insert("n0", canisters(w) as f64);
            w.resource_mut::<Controls>().primary_held = true;
            return None;
        }
        if c.t < HOLD_S + 1.5 {
            return None;
        }
        w.resource_mut::<Controls>().primary_held = false;
        Some(format!("tank {:.1} -> {:.1} %, canisters {} -> {}", c.v["tank0"], tank(w), c.v["n0"], canisters(w)))
    })
}

pub(super) fn coolant_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(run("coolant: the tank, the walker's body and the news are there", 8.0, |w, c| {
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = the_tank(w).is_some() && w.resource::<crate::health::HealthState>().bodies.contains_key(&LOCAL) && items(w).reactor.is_some();
        if !ready || c.t < 1.0 {
            return None;
        }
        check(c, true, "coolant: the tank on the wall is there".into());
        Some("ready".into())
    }));

    // The reactor runs and the pump with it: the level goes down.
    s.push(run("coolant: the tank drains while the reactor runs", 40.0, |w, c| {
        if c.t == 0.0 {
            c.v.insert("t0", tank(w));
            check(c, flow(w) > 0.0, format!("coolant: the pump runs with flow {:.2}", flow(w)));
            return None;
        }
        let drop = c.v["t0"] - tank(w);
        if drop < 0.3 {
            return None;
        }
        check(c, tank(w) >= 0.0, format!("coolant: the tank drained by {drop:.2} % in {:.0} s", c.t));
        Some(format!("tank {:.1} %", tank(w)))
    }));

    // Half full, three canisters in the hand, in front of the tank: the picture and the hint.
    s.push(run("coolant: three canisters in the hand, the tank half full", 3.0, |w, c| {
        if c.t == 0.0 {
            set_tank(w, 50.0);
            let now = w.resource::<Time>().elapsed_secs_f64();
            let stack = items(w).defs.get(CANISTER).expect("coolant_canister").stack_of(3);
            let _ = w.resource_mut::<Items>().give(now, LOCAL, stack);
            stand_before(w, 4.0);
            return None;
        }
        let at = glass(w);
        look_at(w, at);
        if c.t < 1.0 {
            return None;
        }
        check(c, canisters(w) == 3, format!("coolant: three canisters in the bar ({})", canisters(w)));
        Some("stack of three".into())
    }));
    s.push(wait(0.5));
    s.push(shot_step("coolant-tank-half-4m"));
    s.push(wait(0.5));
    s.push({
        let mut slot = 0;
        run("coolant: the canisters are selected", 2.0, move |w, c| {
            if c.t == 0.0 {
                slot = (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == CANISTER)).unwrap_or(0);
                tap(w, DIGITS[slot]);
                return None;
            }
            if items(w).client.view.selected != slot {
                return None;
            }
            check(c, true, format!("coolant: slot {} selected", slot + 1));
            Some(format!("slot {}", slot + 1))
        })
    });
    s.push(run("coolant: the hint names the hold with a canister", 4.0, |w, c| {
        if c.t == 0.0 {
            stand_before(w, 1.4);
        }
        let at = glass(w);
        look_at(w, at);
        if c.t < 0.6 {
            return None;
        }
        let p = prompt(w);
        check(c, p.contains("fill coolant") && !p.contains('%'), format!("coolant: with a canister the hint says to hold ({p:?})"));
        Some(p)
    }));

    // The refill by the hold: half a tank more, one canister used up.
    s.push(hold("coolant: LMB held at the tank empties a canister into it"));
    s.push(run("coolant: the level rose by the canister's share and one canister is used up", 3.0, |w, c| {
        let share = w.resource::<crate::dev_area::ship::ReactorConfig>().0.room.coolant_canister;
        // 50 % plus a share, less what the reactor used in the seconds of the hold (a few per cent at most).
        check(c, tank(w) > 50.0 + share - 6.0 && tank(w) <= 100.0, format!("coolant: the tank rose by about {share} % ({:.1})", tank(w)));
        check(c, canisters(w) == 2, format!("coolant: two canisters are left ({})", canisters(w)));
        Some(format!("tank {:.1} %, {} canisters", tank(w), canisters(w)))
    }));

    // The cap: 80 % and one more canister fill it to the top and no more; a full tank refuses and keeps the canister.
    s.push(run("coolant: the tank at 80 %", 2.0, |w, c| {
        if c.t == 0.0 {
            set_tank(w, 80.0);
            return None;
        }
        Some("80 %".into())
    }));
    s.push(hold("coolant: LMB held at 80 %"));
    s.push(run("coolant: capped at full, one canister used", 3.0, |w, c| {
        check(c, tank(w) <= 100.0 && tank(w) > 97.0, format!("coolant: the tank is full and not over ({:.2})", tank(w)));
        check(c, canisters(w) == 1, format!("coolant: one canister is left ({})", canisters(w)));
        Some(format!("tank {:.1} %", tank(w)))
    }));
    s.push(run("coolant: a full tank refuses: nothing is used up, the hint says FULL", 8.0, |w, c| {
        if c.t == 0.0 {
            set_tank(w, 100.0);
            return None;
        }
        if c.t < 0.5 {
            return None;
        }
        let at = glass(w);
        look_at(w, at);
        if !c.v.contains_key("pressed") {
            let p = prompt(w);
            check(c, p.contains("FULL"), format!("coolant: the hint says FULL ({p:?})"));
            c.v.insert("pressed", c.t);
            w.resource_mut::<Controls>().primary_held = true;
            return None;
        }
        if c.t < c.v["pressed"] + HOLD_S + 1.5 {
            return None;
        }
        w.resource_mut::<Controls>().primary_held = false;
        check(c, canisters(w) == 1 && tank(w) > 97.0, format!("coolant: the canister is kept ({}), the tank stays full ({:.1})", canisters(w), tank(w)));
        Some("kept".into())
    }));
    // A use that reaches the host while the tank is full (two players, one tank): the canister goes back to the hand.
    s.push(run("coolant: a use that meets a full tank gives the canister back", 4.0, |w, c| {
        if c.t == 0.0 {
            let slot = items(w).client.view.selected;
            let now = w.resource::<Time>().elapsed_secs_f64();
            let _ = w.resource_mut::<Items>().request(now, Request::Use { slot, target: LOCAL });
            return None;
        }
        if c.t < 1.0 {
            return None;
        }
        check(c, canisters(w) == 1 && tank(w) > 97.0, format!("coolant: still one canister ({}), the tank full ({:.1})", canisters(w), tank(w)));
        Some(format!("{} canister", canisters(w)))
    }));

    // Dry: no flow, the core climbs; the last canister brings the flow back.
    s.push(run("coolant: a dry tank is no flow", 3.0, |w, c| {
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
    s.push(run("coolant: the core climbs without coolant", 15.0, |w, c| {
        if c.t == 0.0 {
            c.v.insert("t0", heat(w));
        }
        let (t0, t1) = (c.v["t0"], heat(w));
        if t1 < t0 + 0.5 {
            return None;
        }
        check(c, true, format!("coolant: the heat rises with no flow ({t0:.1} -> {t1:.1})"));
        Some(format!("{t0:.1} -> {t1:.1}"))
    }));
    s.push(hold("coolant: the last canister at the dry tank"));
    s.push(run("coolant: the flow is back and the canisters are gone", 3.0, |w, c| {
        check(c, flow(w) > 0.0 && tank(w) > 40.0, format!("coolant: the flow comes back ({:.2}), the tank at {:.1} %", flow(w), tank(w)));
        check(c, canisters(w) == 0, format!("coolant: no canister is left ({})", canisters(w)));
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
