//! Scenario `dev-menu` (round 5): F10 opens the dev menu (dev builds); 1 grants the flight
//! licence (the seat takes the pilot), 2 adds the credits, 3 refills the boost; F10 closes it.
//! Starts without the licences, like a new player.
use crate::dev::{DevMenu, MONEY};
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
        with_ship(w, |s| {
            s.ctl.boost.charge = 0.1;
            s.sc.drive.boost.charge = 0.1;
        });
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
        let (a, b) = with_ship(w, |s| (s.ctl.boost.charge, s.sc.drive.boost.charge));
        check(c, a > 0.99 && b > 0.99, format!("3: boost full ({a:.2}, {b:.2})"));
        true
    }));
    s.push(press(KeyCode::F10));
    s.push(Box::new(|w, c| {
        check(c, !w.resource::<DevMenu>().open, "F10 closes it".into());
        true
    }));
}
