//! #295, health: at the dev area's table a bandage is collected; the walker stands on the hot plate
//! and burns; LMB uses the bandage on oneself (the burn drops, one less in the stack); the walker
//! stands in the no-air booth until the HUD shows the air, then in the glowing barrel's zone until
//! dead; five seconds later the walker is back on the pad with full health. Without a window.
//! Run with `--start=dev_area`.
use super::items::{collect, items, outside, tap_then};
use super::*;
use crate::controls::Controls;
use crate::crew_hud::Vitals;
use crate::dev_area::hazards::{GLOW_BARREL, HOT_PLATE, NO_AIR};
use crate::dev_area::{area_of, DevArea, ENGINEERING, PAD_MAIN};
use crate::health::{HealthState, OwnVitals};
use crate::items::{Items, LOCAL};
use health_core::Life;

/// The walker's burn and radiation on the host, and whether it is down.
fn burn(w: &World) -> f64 {
    w.resource::<HealthState>().bodies.get(&LOCAL).map_or(0.0, |b| b.health.strength("burn"))
}
fn radiation(w: &World) -> f64 {
    w.resource::<HealthState>().bodies.get(&LOCAL).map_or(0.0, |b| b.health.strength("radiation"))
}
fn dead(w: &World) -> bool {
    w.resource::<HealthState>().bodies.get(&LOCAL).is_some_and(|b| b.health.state() == Life::Dead)
}
fn down(w: &World) -> Option<f64> {
    w.resource::<OwnVitals>().last.and_then(|v| v.down)
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

    // The hot plate burns.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "health: onto the hot plate");
            let p = ground_at(w, HOT_PLATE.centre);
            place_walker(w, p);
            return false;
        }
        if c.t < 3.0 {
            return false;
        }
        let shown = w.resource::<Vitals>().burn.is_some();
        check(c, burn(w) > 20.0 && shown, format!("health: 3 s on the hot plate: burn {:.1}, shown on the HUD ({shown})", burn(w)));
        end(w, c, format!("burn {:.1}", burn(w)));
        true
    }));

    // LMB uses the bandage on oneself: the burn drops, one less in the stack.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "health: LMB uses the bandage");
            c.v.insert("burn0", burn(w));
            c.v.insert("n0", items(w).client.confirmed.count("bandage") as f64);
            w.resource_mut::<Controls>().click = true;
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        let (burn1, n1) = (burn(w), items(w).client.confirmed.count("bandage"));
        let (b0, n0) = (c.v["burn0"], c.v["n0"] as u32);
        check(c, n1 + 1 == n0 && burn1 < b0 - 5.0, format!("health: LMB on the plate: burn {b0:.1} to {burn1:.1}, bandages {n0} to {n1}"));
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
        end(w, c, format!("{o2:?}"));
        true
    }));

    // The glowing barrel: radiation until dead, and the respawn starts.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "health: into the glowing barrel's zone");
            let p = ground_at(w, GLOW_BARREL.centre);
            place_walker(w, p);
            return false;
        }
        if !dead(w) && c.t < 60.0 {
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
        check(c, down(w).is_none() && !dead(w), format!("health: up again after {:.1} s", c.t));
        check(c, (dx * dx + dy * dy).sqrt() < 2.0, format!("health: on the pad, {:.1} m from it", (dx * dx + dy * dy).sqrt()));
        check(c, v.health == 1.0 && v.burn.is_none() && v.radiation == 0.0 && radiation(w) == 0.0 && burn(w) == 0.0, format!("health: full health, no burn, no radiation ({:.2})", v.health));
        check(c, items(w).client.confirmed.count("bandage") == 3, format!("health: the items stay, one bandage less ({} left)", items(w).client.confirmed.count("bandage")));
        end(w, c, format!("{:.1} s", c.t));
        true
    }));
}
