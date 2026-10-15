//! Scenario dev-menu: the F10 dev menu's keys, from a new player's start.
use crate::daynight::DayClock;
use crate::dev::DevMenu;
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
        let pilot = w.resource::<Gameplay>().may_pilot(HOST);
        check(c, pilot, "1: the flight licence, the seat takes the pilot".into());
        let b = with_ship(w, |s| s.sc.drive.boost.charge);
        check(c, b > 0.99, format!("3: boost full ({b:.2})"));
        true
    }));
    for (key, stands, what) in [(KeyCode::Digit4, true, "4: always day"), (KeyCode::Digit5, true, "5: always night"), (KeyCode::Digit6, false, "6: jump to day"), (KeyCode::Digit7, false, "7: jump to night")] {
        s.push(press(key));
        s.push(Box::new(|w, c| {
            c.v.insert("t0", w.resource::<DayClock>().t);
            true
        }));
        s.push(wait(1.0));
        s.push(Box::new(move |w, c| {
            let clock = *w.resource::<DayClock>();
            let moved = clock.t - c.v["t0"];
            check(c, if stands { clock.rate == 0.0 && moved == 0.0 } else { clock.rate == 1.0 && moved > 0.0 }, format!("{what}: rate {}, clock moved {moved:.2} s", clock.rate));
            true
        }));
    }
    s.push(press(KeyCode::Digit8));
    s.push(wait(0.1));
    for (on, what) in [(true, "9: noclip on"), (false, "9 again: noclip off")] {
        s.push(press(KeyCode::Digit9));
        s.push(wait(0.1));
        s.push(Box::new(move |w, c| {
            let noclip = w.query::<&crate::walker::Player>().single(w).unwrap().noclip;
            check(c, noclip == on, format!("{what} (noclip {noclip})"));
            true
        }));
    }
    for (on, what) in [(true, "0: god mode on"), (false, "0 again: god mode off")] {
        s.push(press(KeyCode::Digit0));
        s.push(wait(0.1));
        s.push(Box::new(move |w, c| {
            let god = w.contains_resource::<crate::health::GodMode>();
            check(c, god == on, format!("{what} (god {god})"));
            true
        }));
    }
    s.push(press(KeyCode::F10));
    s.push(Box::new(|w, c| {
        check(c, !w.resource::<DevMenu>().open, "F10 closes it".into());
        true
    }));
}
