//! #295, health: at the dev area's table a bandage is collected; the walker stands on the hot plate
//! and catches a light burn per tick, then the buildup fills and the walker is on fire (the burn
//! rises in steps, once per tick); off the plate, the bandage takes 1.5 s with LMB held on oneself
//! (let go after 0.5 s, nothing is used; held 1.5 s, the burn drops and one less is in the stack);
//! the walker stands in the no-air booth until the HUD shows the air, with its O2 line labelled, then
//! in the glowing barrel's zone, where nausea shows and the walk slows, until dead; five seconds
//! later the walker is back on the pad with full health. Without a window. Run with
//! `--start=dev_area`.
use super::items::{collect, items, outside, tap_then};
use super::*;
use crate::controls::Controls;
use crate::crew_hud::{CrewHud, Vitals};
use crate::dev_area::hazards::{GLOW_BARREL, HOT_PLATE, NO_AIR};
use crate::dev_area::{area_of, DevArea, ENGINEERING, PAD_MAIN};
use crate::health::{HealthState, OwnMobility, OwnVitals, UseHold};
use crate::items::{Items, LOCAL};
use health_core::Life;

/// The walker's burn and radiation on the host, whether it is on fire, and whether it is down.
fn burn(w: &World) -> f64 {
    w.resource::<HealthState>().bodies.get(&LOCAL).map_or(0.0, |b| b.health.strength("burn"))
}
fn radiation(w: &World) -> f64 {
    w.resource::<HealthState>().bodies.get(&LOCAL).map_or(0.0, |b| b.health.strength("radiation"))
}
fn on_fire(w: &World) -> bool {
    w.resource::<HealthState>().bodies.get(&LOCAL).is_some_and(|b| b.health.burning())
}
fn dead(w: &World) -> bool {
    w.resource::<HealthState>().bodies.get(&LOCAL).is_some_and(|b| b.health.state() == Life::Dead)
}
fn down(w: &World) -> Option<f64> {
    w.resource::<OwnVitals>().last.and_then(|v| v.down)
}

/// LMB down (a press starts a use) or up, as a player's hand does it with the mouse captured.
fn lmb(w: &mut World, down: bool) {
    let mut c = w.resource_mut::<Controls>();
    c.primary_held = down;
    if down {
        c.click = true;
    }
}

/// A point on the ground at area metres.
fn ground_at(w: &mut World, p: [f64; 2]) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, 0.0).expect("the dev area is there").0
}

/// The walker's place in the area, in metres.
fn area_of_walker(w: &mut World) -> [f64; 2] {
    let pl = planet(w);
    let f = w.resource::<DevArea>().frame.expect("the dev area is there");
    let at = player_world(w);
    area_of(&f, &pl, at)
}

pub(super) fn health_steps(s: &mut Vec<Step>) {
    // The table: stand in front of it, the bandages on it.
    s.push(Box::new(|w, c| {
        begin(w, c, "health: the table and the health state");
        // The item messages take the same 75 ms each way as in the items scenario.
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && outside(w, "bandage").is_some() && w.resource::<HealthState>().bodies.contains_key(&LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "health: the table and the host's body for the walker are there".into());
        let p = ground_at(w, [ENGINEERING.centre[0] - 3.0, 1.6]);
        place_walker(w, p);
        end(w, c, "in front of the table".into());
        true
    }));
    s.extend(collect("bandage", "Bandage"));
    s.push(tap_then("health: 1 takes the bandage into the hand", KeyCode::Digit1, |w| items(w).in_hand(0).is_some_and(|s| s.item.kind.as_str() == "bandage")));

    // The hot plate: a light burn per tick from the first tick; the buildup fills in 3 s, then the
    // walker is on fire. The burn changes once per tick (a step per second), never between.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "health: onto the hot plate");
            let p = ground_at(w, HOT_PLATE.centre);
            place_walker(w, p);
            c.v.insert("last", burn(w));
            c.v.insert("steps", 0.0);
            return false;
        }
        let b = burn(w);
        if (b - c.v["last"]).abs() > 1e-9 {
            c.v.insert("steps", c.v["steps"] + 1.0);
            c.v.insert("last", b);
        }
        if c.t < 4.5 {
            return false;
        }
        let shown = w.resource::<Vitals>().burning;
        let steps = c.v["steps"];
        check(c, on_fire(w) && burn(w) > 10.0 && shown, format!("health: 4.5 s on the hot plate: on fire ({}), burn {:.1}, shown on the HUD ({shown})", on_fire(w), burn(w)));
        check(c, (3.0..=5.0).contains(&steps), format!("health: the burn rose in steps, once per tick: {steps} changes in 4.5 s"));
        end(w, c, format!("burn {:.1}", burn(w)));
        true
    }));

    // Off the plate, the bandage on oneself: LMB held for 1.5 s. Let go after 0.5 s: nothing is
    // used. Pressed again and held: the ring is filling at 1.0 s, and the bandage is used at 1.5 s.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "health: off the plate, LMB held on the bandage");
            let p = ground_at(w, [ENGINEERING.centre[0] - 3.0, 1.6]);
            place_walker(w, p);
            c.v.insert("n0", items(w).client.confirmed.count("bandage") as f64);
            lmb(w, true);
            return false;
        }
        let n0 = c.v["n0"] as u32;
        // Let go at 0.5 s; the next step (a frame later) checks nothing was used, then presses again.
        if !c.v.contains_key("let_go") {
            if c.t < 0.5 {
                return false;
            }
            lmb(w, false);
            c.v.insert("let_go", c.t);
            return false;
        }
        if !c.v.contains_key("again") {
            if c.t - c.v["let_go"] < 0.2 {
                return false;
            }
            let (n, hold) = (items(w).client.confirmed.count("bandage"), w.resource::<UseHold>().0.is_some());
            check(c, n == n0 && !hold, format!("health: LMB let go after 0.5 s: nothing used (bandages {n0} to {n}), no hold left: {}", !hold));
            c.v.insert("again", c.t);
            lmb(w, true);
            return false;
        }
        let since = c.t - c.v["again"];
        if !c.v.contains_key("ring") {
            if since < 1.0 {
                return false;
            }
            let (n, p) = (items(w).client.confirmed.count("bandage"), w.resource::<UseHold>().progress());
            check(c, n == n0 && p.is_some_and(|p| p > 0.5 && p < 1.0), format!("health: LMB held 1.0 s: the ring reads {p:?}, not used yet"));
            c.v.insert("ring", 1.0);
            return false;
        }
        if since < 1.7 {
            return false;
        }
        lmb(w, false);
        let n1 = items(w).client.confirmed.count("bandage");
        check(c, n1 + 1 == n0, format!("health: LMB held 1.5 s: the bandage is used (bandages {n0} to {n1})"));
        check(c, w.resource::<UseHold>().0.is_none(), "health: the hold is over after the use".into());
        end(w, c, "used".into());
        true
    }));

    // The no-air booth: oxygen low shows on the HUD; out again before it costs health.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "health: into the no-air booth");
            let p = ground_at(w, NO_AIR.centre);
            place_walker(w, p);
            return false;
        }
        if c.t < 4.0 {
            return false;
        }
        let o2 = w.resource::<Vitals>().oxygen;
        check(c, o2.is_some_and(|o| o < 1.0), format!("health: the booth's air shows on the HUD ({o2:?})"));
        // The HUD's lines, top to bottom: burn, radiation, oxygen, health; the O2 line has its label.
        let lines = w.resource::<CrewHud>().vitals.clone();
        let kinds: Vec<_> = lines.iter().map(|l| l.kind).collect();
        let o2_label = lines.iter().find(|l| l.kind == display_core::hud::VitalKind::Oxygen).map(|l| l.label());
        check(c, kinds.last() == Some(&display_core::hud::VitalKind::Health) && kinds.contains(&display_core::hud::VitalKind::Oxygen) && o2_label.as_deref().is_some_and(|l| l.ends_with("O2")), format!("health: the HUD lines {kinds:?}, the O2 line reads {o2_label:?}"));
        end(w, c, format!("{o2:?}"));
        true
    }));

    // The glowing barrel: radiation, nausea and the slower walk, until dead; the respawn starts.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "health: into the glowing barrel's zone");
            let p = ground_at(w, GLOW_BARREL.centre);
            place_walker(w, p);
            return false;
        }
        if !c.v.contains_key("sick") && radiation(w) > 60.0 {
            c.v.insert("sick", 1.0);
            let (sick, m) = (w.resource::<Vitals>().sick, w.resource::<OwnMobility>().0);
            check(c, sick && m.walk < 1.0, format!("health: nausea at radiation {:.0}: SICK on the HUD ({sick}), walk {:.2}", radiation(w), m.walk));
        }
        if !dead(w) && c.t < 120.0 {
            return false;
        }
        check(c, dead(w) && down(w).is_some(), format!("health: dead by radiation ({:.0}) after {:.1} s, the respawn counts ({:?} s)", radiation(w), c.t, down(w)));
        end(w, c, "dead".into());
        true
    }));

    // Five seconds later: back on the pad, full health.
    s.push(Box::new(|w, c| {
        if c.t < 2.0 {
            return false;
        }
        if !c.v.contains_key("counted") {
            c.v.insert("counted", 1.0);
            let left = down(w).unwrap_or(-1.0);
            check(c, (2.0..=3.2).contains(&left), format!("health: 2 s after the death the countdown reads {left:.1} s"));
        }
        if down(w).is_some() && c.t < 6.0 {
            return false;
        }
        begin(w, c, "health: respawn on the pad");
        let at = area_of_walker(w);
        let (dx, dy) = (at[0] - PAD_MAIN[0], at[1] - PAD_MAIN[1]);
        let v = w.resource::<Vitals>().clone();
        let m = w.resource::<OwnMobility>().0;
        check(c, down(w).is_none() && !dead(w), format!("health: up again after {:.1} s", c.t));
        check(c, (dx * dx + dy * dy).sqrt() < 2.0, format!("health: on the pad, {:.1} m from it", (dx * dx + dy * dy).sqrt()));
        check(
            c,
            v.health == 1.0 && v.max_share == 1.0 && v.burn_level == 0 && !v.burning && v.buildup == 0.0 && v.radiation == 0.0 && !v.sick && radiation(w) == 0.0 && burn(w) == 0.0,
            format!("health: full health, no burn, no radiation, no nausea ({:.2})", v.health),
        );
        check(c, m.walk == 1.0 && m.sprint, "health: the walk is back to full, sprint too".into());
        check(c, items(w).client.confirmed.count("bandage") == 3, format!("health: the items stay, one bandage less ({} left)", items(w).client.confirmed.count("bandage")));
        end(w, c, format!("{:.1} s", c.t));
        true
    }));
}
