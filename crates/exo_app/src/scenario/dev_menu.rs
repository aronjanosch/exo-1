//! Scenario `dev-menu` (round 5): F10 opens the dev menu (dev builds); 1 grants the flight
//! licence (the seat takes the pilot), 2 adds the credits, 3 refills the boost, 4 and 5 hold day
//! or night, 6 and 7 jump to day or night with the cycle running; F10 closes it.
//! Starts without the licences, like a new player.
use crate::daynight::{DayClock, Sun};
use crate::dev::{DevMenu, DAY_HOUR, MONEY, NIGHT_HOUR};
use crate::gameplay::{Gameplay, HOST};
use crate::scenario::{check, tap, wait, with_ship, Step};
use bevy::prelude::*;

fn press(k: KeyCode) -> Step {
    Box::new(move |w, _| {
        tap(w, k);
        true
    })
}

pub fn dev_menu_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        let gp = w.resource::<Gameplay>();
        c.v.insert("money0", gp.progress.wallet() as f64);
        check(c, !gp.may_pilot(HOST), "start: no flight licence".into());
        with_ship(w, |s| s.sc.drive.boost.charge = 0.1);
        true
    }));
    s.push(press(KeyCode::Digit1));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        check(c, !w.resource::<Gameplay>().may_pilot(HOST), "1 with the menu closed does nothing".into());
        true
    }));
    s.push(press(KeyCode::F10));
    s.push(Box::new(|w, c| {
        check(c, w.resource::<DevMenu>().open, "F10 opens the dev menu".into());
        true
    }));
    s.push(press(KeyCode::Digit1));
    s.push(press(KeyCode::Digit2));
    s.push(press(KeyCode::Digit3));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let gp = w.resource::<Gameplay>();
        let (pilot, gained) = (gp.may_pilot(HOST), gp.progress.wallet() as f64 - c.v["money0"]);
        check(c, pilot, "1: the flight licence, the seat takes the pilot".into());
        check(c, gained == MONEY as f64, format!("2: +{gained} credits (want {MONEY})"));
        let b = with_ship(w, |s| s.sc.drive.boost.charge);
        check(c, b > 0.99, format!("3: boost full ({b:.2})"));
        true
    }));
    // The day: 4 always day, 5 always night (the clock stands), 6 and 7 jump and the cycle runs.
    for (key, hour, stands, what) in [(KeyCode::Digit4, DAY_HOUR, true, "4: always day"), (KeyCode::Digit5, NIGHT_HOUR, true, "5: always night"), (KeyCode::Digit6, DAY_HOUR, false, "6: jump to day"), (KeyCode::Digit7, NIGHT_HOUR, false, "7: jump to night")] {
        s.push(press(key));
        s.push(Box::new(|w, c| {
            c.v.insert("t0", w.resource::<DayClock>().t);
            true
        }));
        s.push(wait(1.0));
        s.push(Box::new(move |w, c| {
            let clock = *w.resource::<DayClock>();
            let h = w.resource::<Sun>().hour.unwrap_or(f64::NAN);
            // Within 0.5 h of the hour (a second of the cycle, if it runs, moves it a little).
            let off = ((h - hour + 12.0).rem_euclid(24.0) - 12.0).abs();
            let moved = clock.t - c.v["t0"];
            check(c, off < 0.5 && (if stands { clock.rate == 0.0 && moved == 0.0 } else { clock.rate == 1.0 && moved > 0.0 }), format!("{what}: local hour {h:.2} (want {hour}), rate {}, clock moved {moved:.2} s", clock.rate));
            true
        }));
    }
    s.push(press(KeyCode::F10));
    s.push(Box::new(|w, c| {
        check(c, !w.resource::<DevMenu>().open, "F10 closes it".into());
        true
    }));
}
