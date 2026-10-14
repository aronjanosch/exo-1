//! #297, slice S3b: the reactor runs on Auto and stays calm. From the dev faults screen (next to
//! the test console): Pump break takes the temperature past the allowed band and the ship's alarm
//! goes Warn, then Danger; Reset reactor brings it back calm; Load drop on Auto overshoots above
//! the optimal band for several seconds. Then Reset again and a second Load drop; the operator
//! takes over at the readout on the engine room's wall (MAN) and lowers fission by clicks until
//! the temperature is back in the optimal band. Without a window; the walker is set down at each
//! screen (test setup) and uses it with F, the clicks go through the displays.
use super::*;
use crate::dev_area::engine_room::{ShipAlarm, ROOM};
use crate::dev_area::reactor::{ReactorCounts, ReactorSim};
use crate::dev_area::reactor_screen::{FAULTS_ID, READOUT_ID};
use crate::dev_area::{DevArea, LAB};
use super::dev_area::tap_until;
use crate::displays::{aim_at, world_point_of, ActiveDisplay, DisplayPanel};
use crate::interact::Interaction;
use crate::items::Items;
use bevy::math::DVec3;
use display_core::PanelId;
use item_core::wire::Alarm;

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

fn alarm(w: &World) -> Alarm {
    w.resource::<ShipAlarm>().level
}

/// The temperature now and the optimal band (low, high).
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

/// Stand the walker at a point on the ground (test setup) facing a display, then use it with F.
fn use_display(s: &mut Vec<Step>, name: &'static str, id: PanelId, stand: [f64; 2]) {
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
        check(c, k.shutdowns == 0 && k.meltdowns == 0, format!("{what}: no shutdown or meltdown ({k:?})"));
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
        check(c, alarm(w) == Alarm::None && k.fires == 0 && k.shutdowns == 0 && k.meltdowns == 0, format!("reactor: calm ({k:?}, alarm {:?})", alarm(w)));
        check(c, w.resource::<Items>().reactor.is_some(), "reactor: the snapshot is there".into());
        end(w, c, format!("{t:.1}"));
        true
    }));

    // The dev faults screen: Pump break, Warn, then Danger.
    use_display(s, "Dev faults", FAULTS_ID, LAB.point([1.9, -4.5 - 1.2]));
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
                check(c, c.v.contains_key("warn"), "reactor: Warn came before Danger".into());
                check(c, t > 80.0, format!("reactor: Danger at {:.1} s with the temperature {t:.1} past the allowed band", c.t));
                end(w, c, format!("Danger at {:.1} s", c.t));
                return true;
            }
            Alarm::None => {}
        }
        if c.t > 8.0 {
            check(c, false, format!("reactor: no Danger within 8 s (alarm {:?})", alarm(w)));
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
        if c.t < 25.0 {
            return false;
        }
        let above = c.v.get("above").copied().unwrap_or(0.0);
        check(c, above >= 5.0, format!("reactor: above the optimal band {above:.1} s after the Load drop (Auto)"));
        check(c, counts(w).shutdowns == 0, "reactor: the Load drop does not shut the reactor down".into());
        end(w, c, format!("{above:.1} s above"));
        true
    }));

    // Second round: reset, then a Load drop, and the operator takes over while it is above.
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
        if c.t > 20.0 {
            check(c, false, "reactor: no overshoot within 20 s".into());
            return true;
        }
        false
    }));

    // The readout on the engine room's wall: MAN, then fission down by clicks until it is back.
    use_display(s, "Reactor", READOUT_ID, [ROOM.centre[0] + 3.0, ROOM.centre[1]]);
    s.push(click_step(READOUT_ID, "mode", 0.3));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "reactor: the operator switches to MAN from the readout");
            return false;
        }
        check(c, sim(w).manual, "reactor: the readout's button puts the reactor on manual".into());
        end(w, c, "manual".into());
        true
    }));
    s.push(Box::new(|w, c| {
        // Each click lowers fission by 5, 2.5 s apart (the temperature answers in seconds), until
        // the temperature is 3 degrees under the optimal band's top (or 10 clicks).
        if c.t == 0.0 {
            begin(w, c, "reactor: fission lowered from the readout brings the temperature back");
            c.v.insert("clicks", 0.0);
            return false;
        }
        let (t, band) = temp_now(w);
        let n = c.v.get("clicks").copied().unwrap_or(0.0);
        if t < band[1] - 3.0 {
            end(w, c, format!("{t:.1} after {n:.0} clicks"));
            return true;
        }
        if n >= 10.0 {
            check(c, false, format!("reactor: still {t:.1} after 10 clicks"));
            return true;
        }
        if c.t >= 2.5 * n {
            click_now(w, READOUT_ID, "fis_dn");
            c.v.insert("clicks", n + 1.0);
        }
        false
    }));
    s.push(Box::new(|w, c| {
        if c.t < 15.0 {
            return false;
        }
        let (t, band) = temp_now(w);
        check(c, t < band[1] && t > band[0] - 10.0, format!("reactor: the temperature {t:.1} is back under the optimal band's top {:.0} after 15 s on MAN", band[1]));
        check(c, sim(w).manual && counts(w).shutdowns == 0, "reactor: on MAN with no shutdown".into());
        end(w, c, format!("{t:.1}"));
        true
    }));
}
