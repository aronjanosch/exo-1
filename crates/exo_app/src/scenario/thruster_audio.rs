//! Scenario `thruster-audio` (#150): the thruster sound layers follow the controls. A strafe
//! right raises the +x hiss most, a boost raises the roar and counts one boost start, and after
//! release the ship rests on the idle rumble with every other layer quiet.
use crate::scenario::{check, hold_until, put_at_seat, sit, Step};
use crate::ship::ThrusterLevels;
use bevy::prelude::*;
use flight_core::audio::{LayerLevels, ThrusterAudioTuning};

fn levels(w: &World) -> LayerLevels {
    w.resource::<ThrusterLevels>().0.levels()
}

pub fn thruster_audio_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(hold_until("strafe right 1 s", &[KeyCode::KeyD], 1.0, |_| false));
    s.push(Box::new(|w, c| {
        let l = levels(w);
        let largest = (0..6).max_by(|a, b| l.hiss[*a].total_cmp(&l.hiss[*b])).unwrap_or(0);
        check(c, largest == 0 && l.hiss[0] > 0.3, format!("strafe right: +x hiss the largest ({largest}), {:.2}", l.hiss[0]));
        true
    }));
    s.push(hold_until("boost 2 s", &[KeyCode::KeyW, KeyCode::ShiftLeft], 2.0, |_| false));
    s.push(Box::new(|w, c| {
        let l = levels(w);
        check(c, l.boost > 0.5, format!("boost: roar {:.2} after 2 s", l.boost));
        check(c, l.boost_starts == 1, format!("boost: one start, counted {}", l.boost_starts));
        true
    }));
    s.push(hold_until("release and rest 3 s", &[], 3.0, |_| false));
    s.push(Box::new(|w, c| {
        let l = levels(w);
        let idle = ThrusterAudioTuning::default().idle;
        let quiet = l.hiss.iter().all(|h| *h < 0.05) && l.boost < 0.05;
        check(c, quiet, format!("rest: hiss and roar below 0.05 (roar {:.3}, hiss {:.3?})", l.boost, l.hiss));
        check(c, (l.rumble - idle).abs() < 0.01, format!("rest: idle rumble {:.3} (idle {idle})", l.rumble));
        true
    }));
}
