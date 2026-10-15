//! Scenario dev-menu: the F10 dev menu's keys, from a new player's start.
use crate::daynight::{DayClock, Sun};
use crate::dev::{DevMenu, DAY_HOUR, JUMP_ABOVE_M, MONEY, NIGHT_HOUR};
use crate::env::{from_v3, PlanetRes};
use crate::ship::Ship;
use avian3d::prelude::Position;
use planet_core::recipe::SiteCategory;
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
    // 8: above the next settlement, or the menu says the planet has none (a small planet may not).
    s.push(press(KeyCode::Digit8));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        let ship = w.query_filtered::<&Position, With<Ship>>().single(w).unwrap().0;
        let planet = w.resource::<PlanetRes>();
        let last = w.resource::<DevMenu>().last.clone().unwrap_or_default();
        let settlements: Vec<_> = planet.pgen.sites.iter().filter(|s| s.category == SiteCategory::Settlement).collect();
        if settlements.is_empty() {
            check(c, last == "no settlement on this planet", format!("8 without settlements: \"{last}\""));
        } else {
            let up = (ship - planet.centre).normalize();
            let (near, s) = settlements.iter().map(|s| (from_v3(s.dir).dot(up).clamp(-1.0, 1.0).acos() * planet.radius, *s)).min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
            let above = (ship - planet.centre).length() - planet.radius - s.ground_m;
            check(c, near < 20.0 && (above - JUMP_ABOVE_M).abs() < 20.0, format!("8: {near:.1} m from {} along the ground, {above:.0} m above it (want {JUMP_ABOVE_M}); \"{last}\"", s.id));
        }
        true
    }));
    for (on, what) in [(true, "9: noclip on"), (false, "9 again: noclip off")] {
        s.push(press(KeyCode::Digit9));
        s.push(wait(0.1));
        s.push(Box::new(move |w, c| {
            let noclip = w.query::<&crate::walker::Player>().single(w).unwrap().noclip;
            check(c, noclip == on, format!("{what} (noclip {noclip})"));
            true
        }));
    }
    s.push(press(KeyCode::F10));
    s.push(Box::new(|w, c| {
        check(c, !w.resource::<DevMenu>().open, "F10 closes it".into());
        true
    }));
}
