//! Scenario `thruster-audio` (#150): the thruster sound layers follow the controls. A strafe
//! right raises the +x hiss most, a boost raises the roar and counts one boost start, and after
//! release the ship hovers: the SC model's thrusters hold it against gravity (a tilted ship holds
//! part of it sideways), so the rest is not silent (the axis model was, its thrust was the input).
use crate::scenario::{begin, check, end, hold_until, keys, planet, put_at_seat, ship_e, ship_vel, sit, Step};
use crate::ship::ThrusterLevels;
use avian3d::prelude::{LinearVelocity, Position, Rotation};
use bevy::math::DVec3;
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
        check(c, l.boost < 0.05, format!("rest: roar below 0.05 ({:.3})", l.boost));
        check(c, l.rumble >= idle, format!("rest: the hover thrust keeps the rumble at or above idle ({:.3}, idle {idle})", l.rumble));
        true
    }));
    // Brake (X) with no movement key: the thrusters fire against the forward motion (-z), so the
    // +z hiss and the rumble rise; at standstill they fall again.
    // Setup, a test hook like the teleports: the ship on the ground does not pick up speed from
    // thrust here (1.5 m/s after 2 s of W, 1.6 m/s with boost), so it is lifted 60 m and set
    // moving forward; lower, the 30 m/s hits the terrain within 0.2 s and the motion turns sideways.
    s.push(Box::new(|w, _| {
        let e = ship_e(w);
        let r = w.get::<Rotation>(e).unwrap().0;
        let p = w.get::<Position>(e).unwrap().0;
        let up = planet(w).up(p);
        w.get_mut::<Position>(e).unwrap().0 = p + up * 60.0;
        w.get_mut::<LinearVelocity>(e).unwrap().0 = r * DVec3::new(0.0, 0.0, -30.0);
        true
    }));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "brake");
            c.v.insert("max_hiss_z", 0.0);
            c.v.insert("max_rumble", 0.0);
            c.v.insert("max_other_hiss", 0.0);
            c.v.insert("max_speed", 0.0);
            keys(w, &[KeyCode::KeyX], true);
        }
        let l = levels(w);
        let speed = ship_vel(w).length();
        let ms = c.v["max_speed"].max(speed);
        c.v.insert("max_speed", ms);
        // While still moving: the brake is the only input, so its layers show the braking.
        if c.t > 0.2 && speed > 3.0 {
            let other = (0..6).filter(|i| *i != 4).map(|i| l.hiss[i]).fold(0.0, f64::max);
            let (hz, r, o) = (c.v["max_hiss_z"].max(l.hiss[4]), c.v["max_rumble"].max(l.rumble), c.v["max_other_hiss"].max(other));
            c.v.insert("max_hiss_z", hz);
            c.v.insert("max_rumble", r);
            c.v.insert("max_other_hiss", o);
        }
        if speed < 0.5 || c.t >= 25.0 {
            keys(w, &[KeyCode::KeyX], false);
            let (hz, r, o, ms) = (c.v["max_hiss_z"], c.v["max_rumble"], c.v["max_other_hiss"], c.v["max_speed"]);
            end(w, c, format!("speed {speed:.2} m/s (max {ms:.2}), hiss[+z] max {hz:.2}, rumble max {r:.2}"));
            let idle = ThrusterAudioTuning::default().idle;
            check(c, hz > 0.3 && hz > o, format!("brake: +z hiss (against forward motion) the largest ({hz:.2}, others {o:.2})"));
            check(c, r > idle + 0.1, format!("brake: rumble {r:.2} above idle {idle}"));
            return true;
        }
        false
    }));
    s.push(hold_until("rest 3 s after the brake", &[], 3.0, |_| false));
    s.push(Box::new(|w, c| {
        let l = levels(w);
        let idle = ThrusterAudioTuning::default().idle;
        check(c, l.boost < 0.05, format!("after the brake: roar below 0.05 ({:.3})", l.boost));
        check(c, l.rumble >= idle, format!("after the brake: the hover keeps the rumble at or above idle ({:.3}, idle {idle})", l.rumble));
        true
    }));
}
