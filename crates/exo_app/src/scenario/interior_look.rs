//! The look of the mini ship's interior (#328 follow-up): with the reactor running and every junction box passing, the lamps are at
//! full level (headless: the news says so); with a window, one picture from a corner of the engine room, the corridor, the storage
//! room, the equipment room and the airlock, then JB-2 blown and the corridor and the storage room again, for the contrast.
use super::items::{items, look_at};
use super::*;
use crate::dev_area::engineering::junction_boxes::box_passes;
use crate::dev_area::ship::{ReactorSim, ShipEvents};
use crate::dev_area::DevArea;
use reactor_core::director::DirectorTuning;
use reactor_core::Director;

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

/// The walker stands at `from` (area metres) and looks at `to` at `lift` metres above the floor.
fn view(w: &mut World, from: [f64; 2], to: [f64; 2], lift: f64) {
    let feet = ground(w, from, 0.0);
    place_walker(w, feet);
    let target = ground(w, to, lift);
    look_at(w, target);
}

fn lights(w: &World) -> f64 {
    items(w).reactor.as_ref().map_or(0.0, |r| r.lights_factor)
}

/// A picture: put the walker there, give the lamps' easing a second, shoot.
fn picture(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step, tag: &'static str, from: [f64; 2], to: [f64; 2], lift: f64) {
    s.push(Box::new(move |w, c| {
        view(w, from, to, lift);
        c.t >= 1.2
    }));
    s.push(shot_step(tag));
    s.push(wait(0.5));
}

pub(super) fn interior_look_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "interior_look: the reactor runs warm, the lamps are at full level");
            w.resource_mut::<ReactorSim>().director = Director::new(&DirectorTuning { enabled: false, ..Default::default() }, 1);
        }
        let ready = w.resource::<DevArea>().frame.is_some() && lights(w) >= 0.99;
        if !ready && c.t < 30.0 {
            return false;
        }
        check(c, ready, format!("interior_look: the lights factor is full with the reactor running ({:.2})", lights(w)));
        for i in 0..3 {
            let passes = box_passes(items(w).reactor.as_ref(), i);
            check(c, passes, format!("interior_look: junction box {} passes power", i + 1));
        }
        end(w, c, format!("lights {:.2}", lights(w)));
        true
    }));
    picture(s, shot_step, "engine-room", [-18.2, -18.2], [-9.3, -14.5], 1.2);
    picture(s, shot_step, "corridor", [-18.5, -9.2], [3.0, -9.2], 1.4);
    picture(s, shot_step, "storage-room", [-1.9, -11.6], [3.4, -15.6], 0.8);
    picture(s, shot_step, "equipment-room", [4.1, -8.3], [9.8, -11.4], 1.0);
    picture(s, shot_step, "airlock", [12.0, -8.9], [10.4, -10.6], 1.0);
    // Frame time (a window run with `--perf --no-vsync`): the lit corridor seen from its west end, held for a few seconds.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "interior_look: frame time in the lit corridor");
        }
        view(w, [-18.5, -9.2], [3.0, -9.2], 1.4);
        if c.t < 4.0 {
            return false;
        }
        end(w, c, "measured".into());
        true
    }));
    // JB-2 blown: the corridor, the storage room and the rest of its part go dark.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "interior_look: JB-2 blown");
            let events = w.resource_mut::<ReactorSim>().room.blow_junction(1, false);
            w.resource_mut::<ShipEvents>().0.extend(events);
        }
        if c.t < 1.0 {
            return false;
        }
        let passes = box_passes(items(w).reactor.as_ref(), 1);
        check(c, !passes, "interior_look: the news says JB-2 no longer passes".into());
        end(w, c, "dark".into());
        true
    }));
    picture(s, shot_step, "corridor-dark", [-18.5, -9.2], [3.0, -9.2], 1.4);
    picture(s, shot_step, "storage-room-dark", [-1.9, -11.6], [3.4, -15.6], 0.8);
}
