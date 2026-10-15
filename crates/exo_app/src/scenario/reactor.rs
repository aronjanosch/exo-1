//! The reactor runs on Auto and stays calm. From the dev faults screen (next to
//! the test console): Pump break takes the temperature past the allowed band and the ship's alarm
//! goes Warn, then Danger; Reset reactor brings it back calm; Load drop on Auto overshoots above
//! the optimal band for several seconds. Then Reset again and a second Load drop; the operator
//! takes over at the readout on the engine room's wall (MAN) and lowers fission by clicks until
//! the temperature is back in the optimal band. Then SCRAM at the readout cuts the
//! output and the machines' screens show NO POWER; POWER on brings it back; the pump's OFF button
//! lets the temperature climb; the junction box's LIGHTS breaker takes 80 kW off the load. There is
//! no automatic shutdown. Without a window; the walker is set down at each screen (test setup) and
//! uses it with F, the clicks go through the displays. BOOST UP on the RX STATUS screen raises the
//! temperature boost.
use super::*;
use crate::dev_area::engine_room::ShipAlarm;
use crate::dev_area::reactor::{ReactorCounts, ReactorSim};
use crate::dev_area::engine_room::{COOLANT_PUMP_AT, JUNCTION_BOX_AT};
use crate::dev_area::reactor_screen::{reactor_status, DOOR_ID, FAULTS_ID, JUNCTION_ID, PUMP_ID, READOUT_ID, STATUS_ID};
use crate::dev_area::DevArea;
use super::{ship_frame_of, with_player};
use super::dev_area::tap_until;
use crate::displays::{aim_at, world_point_of, ActiveDisplay, DisplayPanel, Panels};
use crate::interact::Interaction;
use crate::items::Items;
use bevy::math::DVec3;
use display_core::PanelId;
use item_core::wire::{Alarm, ReactorAsk};
use reactor_core::Group;

/// The display entity with this panel id.
fn display(w: &mut World, id: PanelId) -> DisplayPanel {
    w.query::<&DisplayPanel>().iter(w).find(|d| d.id == id).cloned().unwrap_or_else(|| panic!("no display {id:?}"))
}

fn ground_at(w: &mut World, p: [f64; 2]) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, 0.0).expect("the dev area is there").0
}

fn sim(w: &World) -> &ReactorSim {
    w.resource::<ReactorSim>()
}

fn counts(w: &World) -> ReactorCounts {
    *w.resource::<ReactorCounts>()
}

/// Whether a screen shows a control with this action (a screen with NO POWER shows none).
fn screen_has(w: &World, id: PanelId, action: &str) -> bool {
    w.resource::<Panels>().get(&id).is_some_and(|p| p.find(action).is_some())
}

/// The pump's mode buttons (a pump screen with power shows all three).
const PUMP_BUTTONS: &str = "pump_high";

fn alarm(w: &World) -> Alarm {
    w.resource::<ShipAlarm>().level
}

/// The temperature and the optimal band (low, high).
fn temp_now(w: &World) -> (f64, [f64; 2]) {
    let r = sim(w).room.reactor();
    (r.temperature(), [r.bands().temp_optimal.lo, r.bands().temp_optimal.hi])
}

/// One click on a widget of a display (the cursor goes there, the click is for this frame).
fn click_now(w: &mut World, id: PanelId, action: &str) {
    let d = display(w, id);
    let at = world_point_of(w, &d, action, None).unwrap_or_else(|| panic!("no widget '{action}'"));
    aim_at(w, at);
    w.resource_mut::<Controls>().click = true;
}

/// A click on a widget, then `wait` seconds.
fn click_step(id: PanelId, action: &'static str, wait: f64) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            click_now(w, id.clone(), action);
            return false;
        }
        c.t >= wait
    })
}

/// Looks at a point up or down as well as across (`face_towards` keeps the view level): the
/// screens low on a machine are seen from above.
fn pitch_towards(w: &mut World, target: DVec3) {
    let f = ship_frame_of(w);
    with_player(w, |p| {
        let up = p.world_up(f);
        let eye = p.world_pos(f) + up * crate::walker::EYE_HEIGHT;
        let d = target - eye;
        let v = d.dot(up);
        let h = (d - up * v).length();
        p.pitch = v.atan2(h);
    });
}

/// Stand the walker at a point on the ground (test setup) facing a display, then use it with F.
pub(super) fn use_display(s: &mut Vec<Step>, name: &'static str, id: PanelId, stand: [f64; 2]) {
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, &format!("reactor: use the {name}"));
            if active(w).using().is_some() {
                tap(w, KeyCode::KeyF);
                return false;
            }
        }
        if c.t < 0.3 {
            return false;
        }
        let at = ground_at(w, stand);
        place_walker(w, at);
        let target = display(w, id.clone()).centre;
        face_towards(w, target);
        pitch_towards(w, target);
        true
    }));
    s.push(Box::new(move |w, c| {
        if c.t < 0.5 {
            return false;
        }
        let p = w.resource::<Interaction>().prompt.clone();
        if p.contains(name) {
            return true;
        }
        if c.t >= 2.0 {
            check(c, false, format!("reactor: the prompt names {name} within 2 s (it says {p:?})"));
            return true;
        }
        false
    }));
    s.push(tap_until(KeyCode::KeyF, name, 1.0, |w| active(w).using().is_some() && w.resource::<Controls>().free_cursor));
}

fn active(w: &World) -> ActiveDisplay {
    *w.resource::<ActiveDisplay>()
}

/// Waits `secs` (the step checks nothing), then runs `then` on the last frame.
fn settle(secs: f64, what: &'static str) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, what);
            return false;
        }
        if c.t < secs {
            return false;
        }
        let (t, band) = temp_now(w);
        let k = counts(w);
        check(c, alarm(w) == Alarm::None && t > band[0] && t < band[1], format!("{what}: calm, temperature {t:.1} in {band:?}, alarm {:?}", alarm(w)));
        check(c, k.meltdowns == 0, format!("{what}: no meltdown ({k:?})"));
        end(w, c, format!("{t:.1}"));
        true
    })
}

pub(super) fn reactor_steps(s: &mut Vec<Step>) {
    // Auto, calm: the cold room warms up on Auto (about a minute), then 60 s with the power on.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "reactor: Auto runs calm");
            return false;
        }
        if c.t < 120.0 {
            return false;
        }
        let (t, band) = temp_now(w);
        let k = counts(w);
        check(c, !sim(w).manual && sim(w).room.reactor().power_on(), "reactor: on Auto with the power on".into());
        check(c, t > band[0] && t < band[1], format!("reactor: the temperature {t:.1} is in the optimal band {band:?}"));
        check(c, alarm(w) == Alarm::None && k.fires == 0 && k.meltdowns == 0, format!("reactor: calm ({k:?}, alarm {:?})", alarm(w)));
        check(c, w.resource::<Items>().reactor.is_some(), "reactor: the snapshot is there".into());
        end(w, c, format!("{t:.1}"));
        true
    }));

    // The dev faults screen: Pump break, Warn, then Danger.
    use_display(s, "Dev faults", FAULTS_ID, [-17.5, -12.2]);
    s.push(click_step(FAULTS_ID, "pump_break", 0.2));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "reactor: Pump break goes Warn, then Danger");
            return false;
        }
        match alarm(w) {
            Alarm::Warn => {
                c.v.entry("warn").or_insert(c.t);
            }
            Alarm::Danger => {
                let (t, _) = temp_now(w);
                let allowed = sim(w).room.reactor().bands().temp_allowed.hi;
                check(c, c.v.contains_key("warn"), "reactor: Warn came before Danger".into());
                check(c, t > allowed, format!("reactor: Danger at {:.1} s with the temperature {t:.1} past the allowed band {allowed:.0}", c.t));
                end(w, c, format!("Danger at {:.1} s", c.t));
                return true;
            }
            Alarm::None => {}
        }
        // Without coolant the core runs over the allowed band in about 10 s (the core's own run), Auto
        // cannot stop it. The limit is generous: the scenario's clock is wall time on a busy machine.
        if c.t > 60.0 {
            check(c, false, format!("reactor: no Danger within 60 s (alarm {:?})", alarm(w)));
            return true;
        }
        false
    }));

    // The overheat has started a fire at the core (after 20 s above the optimal band). A reset does
    // not put it out and it would burn the new reactor, so the test puts it out with the spray.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "reactor: the fire at the core is sprayed out");
            return false;
        }
        let mut air = w.resource_mut::<crate::dev_area::engine_room::EngineAir>();
        let at: Vec<DVec3> = air.deck.fires().iter().map(|f| f.pos).collect();
        for p in at {
            air.deck.spray(p, 0.5);
        }
        let left = air.deck.fires().len();
        if left == 0 || c.t > 5.0 {
            check(c, left == 0, format!("reactor: the fire is out ({left} left)"));
            end(w, c, format!("{left} left"));
            return true;
        }
        false
    }));
    // Reset reactor: fresh, calm after the warm-up.
    s.push(click_step(FAULTS_ID, "reset", 0.2));
    s.push(settle(120.0, "reactor: Reset reactor brings it back calm"));

    // Load drop on Auto: above the optimal band for several seconds.
    s.push(click_step(FAULTS_ID, "load_drop", 0.2));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "reactor: Load drop on Auto overshoots above the optimal band");
            c.v.insert("above", 0.0);
            return false;
        }
        let (t, band) = temp_now(w);
        if t > band[1] {
            *c.v.entry("above").or_insert(0.0) += c.dt;
        }
        // Done when it has been above the band for 5 s (the core's run: about 13 s above it), at
        // most 60 s.
        let above = c.v.get("above").copied().unwrap_or(0.0);
        if above < 5.0 && c.t < 60.0 {
            return false;
        }
        check(c, above >= 5.0, format!("reactor: above the optimal band {above:.1} s after the Load drop (Auto)"));
        check(c, counts(w).meltdowns == 0, "reactor: the Load drop does not melt the reactor".into());
        end(w, c, format!("{above:.1} s above"));
        true
    }));

    // Second pass: reset, then a Load drop, and the operator takes over while it is above.
    s.push(click_step(FAULTS_ID, "reset", 0.2));
    s.push(settle(120.0, "reactor: a second reset is calm"));
    s.push(click_step(FAULTS_ID, "load_drop", 0.2));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "reactor: wait for the overshoot");
            return false;
        }
        let (t, band) = temp_now(w);
        if t > band[1] {
            end(w, c, format!("{t:.1} above {:.0}", band[1]));
            return true;
        }
        if c.t > 60.0 {
            check(c, false, "reactor: no overshoot within 60 s".into());
            return true;
        }
        false
    }));

    // The junction box's breakers. The Load drop above switched ENGINES and LIGHTS off, so: LIGHTS on
    // (+80 kW), LIGHTS off (-80 kW), ENGINES on (its demand, 800 kW at full work). The load is the grid's request.
    use_display(s, "JUNCTION BOX", JUNCTION_ID, [JUNCTION_BOX_AT[0], JUNCTION_BOX_AT[1] + 2.0]);
    s.push(click_until(JUNCTION_ID, "br_lights", None, "reactor: the LIGHTS breaker on takes 80 kW onto the load", |w| sim(w).room.grid().breaker(Group::Lights)));
    s.push(load_step("reactor: LIGHTS on: the load is the grid's demand (lights 80 kW on)"));
    s.push(click_until(JUNCTION_ID, "br_lights", None, "reactor: the LIGHTS breaker off takes 80 kW off the load", |w| !sim(w).room.grid().breaker(Group::Lights)));
    s.push(load_step("reactor: LIGHTS off: the load is the grid's demand (lights 80 kW off)"));
    s.push(click_until(JUNCTION_ID, "br_engines", None, "reactor: the ENGINES breaker on takes its demand onto the load", |w| sim(w).room.grid().breaker(Group::Engines)));
    s.push(load_step("reactor: ENGINES on: the load is the grid's demand (engines 800 kW at full work, wandering)"));

    // The readout on the engine room's wall: MAN, then the fission slider to 30 %.
    use_display(s, "REACTOR", READOUT_ID, [-11.8, -17.7]);
    s.push(click_until(READOUT_ID, "mode", None, "reactor: the operator switches to MAN from the readout", |w| sim(w).manual));
    s.push(click_until(READOUT_ID, "fis_set", Some(0.3), "reactor: fission set to 30 % on the slider brings the temperature back", |w| (sim(w).fission - 30.0).abs() < 1.0));
    s.push(Box::new(|w, c| {
        if c.t < 15.0 {
            return false;
        }
        let (t, band) = temp_now(w);
        check(c, t < band[1] && t > band[0] - 10.0, format!("reactor: the temperature {t:.1} is back under the optimal band's top {:.0} after 15 s on MAN", band[1]));
        check(c, sim(w).manual && counts(w).meltdowns == 0, "reactor: on MAN, no meltdown".into());
        end(w, c, format!("{t:.1}"));
        true
    }));

    // SCRAM at the readout: fission and the power off at once, the output 0, the machines dark.
    s.push(click_until(READOUT_ID, "scram", None, "reactor: SCRAM from the readout cuts the output and the power", |w| !sim(w).room.reactor().power_on()));
    s.push(Box::new(|w, c| {
        if c.t < 3.0 {
            return false;
        }
        let r = sim(w).room.reactor();
        let (power, output, fission) = (r.power_on(), r.output_kw(), r.fission());
        let powered = w.resource::<Items>().reactor.is_some_and(|n| n.powered);
        check(c, !power && output == 0.0 && fission == 0.0, format!("reactor: SCRAM: power {power}, output {output:.1}, fission {fission:.1}"));
        check(c, !powered, "reactor: the machines have no power after SCRAM".into());
        check(c, !screen_has(w, PUMP_ID, PUMP_BUTTONS) && !screen_has(w, JUNCTION_ID, "br_lights") && !screen_has(w, DOOR_ID, "br_lights"), "reactor: the pump, junction and door screens show NO POWER".into());
        check(c, screen_has(w, READOUT_ID, "power") && screen_has(w, READOUT_ID, "scram"), "reactor: the readout still has POWER".into());
        end(w, c, "scrammed".into());
        true
    }));

    // POWER on at the readout: the reactor runs again, the machines have power.
    s.push(click_until(READOUT_ID, "power", None, "reactor: POWER on at the readout brings the reactor back", |w| sim(w).room.reactor().power_on()));
    s.push(Box::new(|w, c| {
        if c.t < 30.0 {
            return false;
        }
        let r = sim(w).room.reactor();
        let (power, output) = (r.power_on(), r.output_kw());
        let powered = w.resource::<Items>().reactor.is_some_and(|n| n.powered);
        check(c, power && output > 100.0 && powered, format!("reactor: back on: power {power}, output {output:.1}, powered {powered}"));
        check(c, screen_has(w, PUMP_ID, PUMP_BUTTONS) && screen_has(w, DOOR_ID, "br_lights"), "reactor: the screens have their controls again".into());
        end(w, c, format!("{output:.0} kW"));
        true
    }));

    // The coolant pump's OFF button on its screen: no flow, and the temperature climbs.
    use_display(s, "COOLANT PUMP", PUMP_ID, [COOLANT_PUMP_AT[0], COOLANT_PUMP_AT[1] + 2.0]);
    s.push(click_until(PUMP_ID, "pump_off", None, "reactor: the pump switched off climbs the temperature", |w| !sim(w).room.pump().on()));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            c.v.insert("t0", temp_now(w).0);
            return false;
        }
        // The pump spins down and the coolant coasts on: wait until the flow has gone (at most 90 s).
        let flow = w.resource::<Items>().reactor.map_or(1.0, |n| n.flow);
        if flow >= 0.05 && c.t < 90.0 {
            return false;
        }
        let (t, _) = temp_now(w);
        let t0 = c.v.get("t0").copied().unwrap_or(t);
        let flow = w.resource::<Items>().reactor.map(|n| (n.pump_on, n.flow));
        check(c, matches!(flow, Some((false, f)) if f < 0.05), format!("reactor: pump off, no flow ({flow:?})"));
        let r = sim(w).room.reactor();
        check(c, t > t0 + 5.0, format!("reactor: the temperature climbs without the pump: {t0:.1} -> {t:.1} (fission {:.1}, turbine {:.1}, targets {:?}, manual {}, power {}, output {:.1})", r.fission(), r.turbine(), r.targets(), sim(w).manual, r.power_on(), r.output_kw()));
        end(w, c, format!("{t0:.1} -> {t:.1}"));
        true
    }));
    s.push(click_until(PUMP_ID, "pump_high", None, "reactor: the pump back on", |w| sim(w).room.pump().on()));

    // The RX STATUS screen beside the readout: BOOST UP (the walker uses the screen) raises the
    // temperature boost to its amount (25, past 20).
    use_display(s, "RX STATUS", STATUS_ID, [-9.6, -17.6]);
    s.push(click_until(STATUS_ID, "boost_up", None, "reactor: BOOST UP on the status screen raises the boost", |w| sim(w).room.reactor().temperature_boost() > 20.0));

    // PULSAR's core eject: ARM EJECT opens the cover, EJECT inside its window throws the core
    // out. Then nothing follows: no melt, no blast, the readout says NO CORE, the rods are gone. The
    // overheat above has broken the old core (its status screen is dark), so a fresh room comes first
    // (the dev faults' Reset, as the host applies it).
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "reactor: a fresh core for the eject");
            let k = counts(w);
            c.v.insert("ejects0", k.ejects as f64);
            c.v.insert("melts0", k.meltdowns as f64);
            c.v.insert("blasts0", k.blasts as f64);
            w.resource_mut::<Items>().ask_reactor(ReactorAsk::Reset);
            return false;
        }
        // One second for the reset to be applied and the snapshot to show the new room.
        if c.t < 1.0 {
            return false;
        }
        let ok = w.resource::<Items>().reactor.is_some_and(|n| n.powered && n.output_kw > 0.0);
        if !ok && c.t < 10.0 {
            return false;
        }
        check(c, ok, "reactor: the fresh core runs on the load".into());
        end(w, c, "fresh core".into());
        true
    }));
    // One step for both presses: the button is ARM EJECT while the cover is shut and EJECT while it is
    // open, so each retry (half a second) clicks what the snapshot shows. The window is 5 s.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "reactor: ARM EJECT, then EJECT inside the window, on the status screen");
            return false;
        }
        if sim(w).room.reactor().ejected() {
            end(w, c, "ejected".into());
            return true;
        }
        if c.t > 20.0 {
            check(c, false, "reactor: the core was not ejected within 20 s".into());
            end(w, c, "timed out".into());
            return true;
        }
        let last = c.v.get("last").copied().unwrap_or(-1.0);
        if last >= 0.0 && c.t < last + 0.5 {
            return false;
        }
        let armed = w.resource::<Items>().reactor.is_some_and(|n| n.eject_armed);
        let action = if armed { "eject" } else { "arm_eject" };
        let d = display(w, STATUS_ID);
        if let Some(at) = world_point_of(w, &d, action, None) {
            aim_at(w, at);
            w.resource_mut::<Controls>().click = true;
            c.v.insert("last", c.t);
        }
        false
    }));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "reactor: after the eject nothing follows");
            return false;
        }
        if c.t < 10.0 {
            return false;
        }
        let k = *w.resource::<ReactorCounts>();
        let (e0, m0, b0) = (c.v["ejects0"] as u32, c.v["melts0"] as u32, c.v["blasts0"] as u32);
        check(c, k.ejects == e0 + 1 && k.meltdowns == m0 && k.blasts == b0, format!("reactor: one eject, no melt, no blast ({} / {} / {})", k.ejects - e0, k.meltdowns - m0, k.blasts - b0));
        let n = w.resource::<Items>().reactor;
        check(c, reactor_status(n.as_ref()).0 == "NO CORE", format!("reactor: the readout says NO CORE ({:?})", reactor_status(n.as_ref()).0));
        check(c, sim(w).room.reactor().rods().is_empty() && !sim(w).room.reactor().melted(), "reactor: the rods went with the core, and nothing melted".into());
        end(w, c, format!("{} eject", k.ejects - e0));
        true
    }));
}

/// The grid's demand of the groups whose breakers are on (what the load is, from the room).
fn grid_demand(w: &World) -> f64 {
    let g = sim(w).room.grid();
    Group::ALL.iter().filter(|&&x| g.breaker(x)).map(|&x| g.group(x).demand_kw).sum()
}

/// Waits until the snapshot's load is the grid's demand (the snapshot comes a few times a second,
/// and the engines' demand wanders, so a small tolerance), at most 5 s.
fn load_step(what: &'static str) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, what);
            return false;
        }
        let want = grid_demand(w);
        let load = w.resource::<Items>().reactor.map(|n| n.load_kw).unwrap_or(-1.0);
        let close = (load - want).abs() <= 0.02 * want + 0.5;
        if !close && c.t < 5.0 {
            return false;
        }
        check(c, close, format!("{what}: load {load:.0} kW, grid demand {want:.0} kW"));
        end(w, c, format!("load {load:.0} kW"));
        true
    })
}

/// Clicks an action until `done` holds, a retry each second, five at most (a click can be lost
/// to another system taking the press). `frac` aims at that fraction across the widget (a slider).
pub(super) fn click_until(id: PanelId, action: &'static str, frac: Option<f64>, what: &'static str, done: fn(&World) -> bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, what);
            c.v.insert("tries", 0.0);
            return false;
        }
        if done(w) {
            end(w, c, format!("{} tries", c.v.get("tries").copied().unwrap_or(0.0) as u32));
            return true;
        }
        let last = c.v.get("last").copied().unwrap_or(-1.0);
        if last >= 0.0 && c.t < last + 1.0 {
            return false;
        }
        let tries = c.v.get("tries").copied().unwrap_or(0.0);
        if tries >= 5.0 {
            check(c, false, format!("{what}: not done after 5 clicks"));
            return true;
        }
        let d = display(w, id.clone());
        let Some(at) = world_point_of(w, &d, action, frac) else {
            let n = w.resource::<Items>().reactor;
            check(c, false, format!("{what}: no widget '{action}' (snapshot {n:?})"));
            return true;
        };
        aim_at(w, at);
        w.resource_mut::<Controls>().click = true;
        c.v.insert("tries", tries + 1.0);
        c.v.insert("last", c.t);
        false
    })
}
