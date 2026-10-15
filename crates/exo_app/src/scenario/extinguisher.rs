//! The extinguisher's supply: wear, empty, refill at the corridor rack.
use super::items::{collect, items, look_at, tap_then, DELAY};
use super::*;
use crate::controls::Controls;
use crate::dev_area::layout::CORRIDOR;
use crate::dev_area::fire::Sprays;
use crate::dev_area::{DevArea, ENGINEERING};
use crate::items::{Items, LOCAL};

/// The rack's id in the dev area (`item_core::supply`'s file).
const RACK: u32 = 1;

fn lmb(w: &mut World, down: bool) {
    w.resource_mut::<Controls>().primary_held = down;
}

fn spraying(w: &World) -> bool {
    w.resource::<Sprays>().0.contains_key(&LOCAL)
}

fn held(w: &World) -> Option<f64> {
    let i = items(w);
    i.client.view.slot(i.client.view.selected)?.item.condition
}

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

pub(super) fn extinguisher_steps(s: &mut Vec<Step>) {
    super::items::at_the_table(s);
    s.extend(collect("extinguisher", "Extinguisher"));
    s.push(tap_then("extinguisher: 1 takes it into the hand", KeyCode::Digit1, |w| items(w).in_hand(0).is_some_and(|s| s.item.kind.as_str() == "extinguisher")));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "extinguisher: a new one is at 100 %");
            let at = ground(w, [ENGINEERING.centre[0] - 3.0, 4.0], 0.6);
            look_at(w, at);
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        check(c, held(w).is_some_and(|c| c > 99.5), format!("extinguisher: a new one is at 100 % ({:?})", held(w)));
        lmb(w, true);
        end(w, c, "spraying".into());
        true
    }));
    s.push(Box::new(|w, c| {
        if c.t < 3.0 {
            return false;
        }
        let c0 = held(w).unwrap_or(0.0);
        check(c, spraying(w), "extinguisher: the spray runs while LMB is held".into());
        lmb(w, false);
        end(w, c, format!("{c0:.1} %"));
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "extinguisher: empty, the spray does not work");
            let slot = items(w).host.inventories.get(&LOCAL).map_or(0, |i| i.selected);
            let now = w.resource::<Time>().elapsed_secs_f64();
            w.resource_mut::<Items>().wear(now, LOCAL, slot, 100.0);
            return false;
        }
        if c.t < DELAY + 0.3 {
            return false;
        }
        check(c, held(w) == Some(0.0), format!("extinguisher: empty on the client ({:?})", held(w)));
        lmb(w, true);
        end(w, c, "empty".into());
        true
    }));
    s.push(Box::new(|w, c| {
        if c.t < 1.5 {
            return false;
        }
        check(c, !spraying(w), "extinguisher: an empty one does not spray".into());
        lmb(w, false);
        end(w, c, "no spray".into());
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "extinguisher: the rack in the corridor");
            let at = ground(w, [CORRIDOR.centre[0] - 0.75, CORRIDOR.centre[1] - 2.2], 0.0);
            place_walker(w, at);
            let rack = ground(w, [CORRIDOR.centre[0] - 0.75, CORRIDOR.centre[1] - 1.0], 0.6);
            look_at(w, rack);
            return false;
        }
        if c.t < 0.6 {
            return false;
        }
        end(w, c, "at the rack".into());
        true
    }));
    s.push(tap_then("extinguisher: F at the rack swaps it", KeyCode::KeyF, |w| held(w) == Some(100.0)));
    s.push(Box::new(|w, c| {
        if c.t < DELAY + 0.3 {
            return false;
        }
        let racks = items(w).host.racks.get(&RACK).cloned();
        let empty_on_rack = racks.is_some_and(|r| (0..r.len()).any(|i| r.slot(i).and_then(|s| s.item.condition) == Some(0.0)));
        check(c, empty_on_rack, "extinguisher: the empty one hangs on the rack".into());
        end(w, c, "swapped".into());
        true
    }));

    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "extinguisher: the spray works again");
            let at = ground(w, [ENGINEERING.centre[0] - 3.0, 4.0], 0.6);
            look_at(w, at);
            lmb(w, true);
            return false;
        }
        if c.t < 2.0 {
            return false;
        }
        let c0 = held(w).unwrap_or(0.0);
        check(c, spraying(w), "extinguisher: the spray runs with the full one".into());
        lmb(w, false);
        end(w, c, format!("{c0:.1} %"));
        true
    }));
}
