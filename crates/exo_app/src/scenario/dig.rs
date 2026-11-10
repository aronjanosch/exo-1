//! The dig run (#324): the walker cuts a chunk of the first vein with the welder (LMB held through
//! `Controls`), picks it up, puts it into Crate B of the storage room through the container path
//! (nothing lost, nothing doubled), and without a suit loses breath inside the dig site but not
//! outside it. Every step polls for its outcome with a limit in simulated seconds.
use super::items::{collect, items, look_at, tap_then, DELAY};
use super::*;
use crate::controls::Controls;
use crate::crew_hud::Vitals;
use crate::dev_area::dig::{DigCfg, DigState};
use crate::dev_area::layout::STORAGE_ROOM;
use crate::dev_area::storage::CRATE_B as CRATE;
use crate::dev_area::{DevArea, DIG_SITE, OPEN_GROUND};
use crate::items::{Items, LOCAL};
use item_core::container::ItemKind;
use item_core::Request;

const ORE: &str = "grumblite";

fn lmb(w: &mut World, down: bool) {
    w.resource_mut::<Controls>().primary_held = down;
}

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

/// The first vein's place in the area frame.
fn vein_at(w: &World) -> [f64; 2] {
    DIG_SITE.point(w.resource::<DigCfg>().0.veins[0].at)
}

fn chunks(w: &World) -> u32 {
    w.resource::<DigState>().veins[0].chunks_left
}

/// The ore anywhere: on the floor, in the bar, in the crate. A chunk may never be lost or doubled.
fn ore_total(w: &World) -> u32 {
    let it = items(w);
    let loose = it.host.loose.values().filter(|l| l.stack.item.kind.as_str() == ORE).map(|l| l.stack.count).sum::<u32>();
    loose + it.host.inventory(LOCAL).count(ORE) + in_crate(w)
}

fn in_crate(w: &World) -> u32 {
    items(w).host.racks.get(&CRATE).map_or(0, |c| c.count(&ItemKind::new(ORE)))
}

fn oxygen(w: &World) -> Option<f64> {
    w.resource::<Vitals>().oxygen
}

/// A step that polls `done`, with a limit in simulated seconds.
pub(super) fn until(what: &'static str, limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, what);
            return false;
        }
        if done(w) {
            end(w, c, format!("{:.1} s", c.t));
            return true;
        }
        if c.t >= limit {
            check(c, false, format!("{what}: not within {limit} s"));
            return true;
        }
        false
    })
}

pub(super) fn dig_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "dig: the welder in the hand at the first vein");
            w.resource_mut::<Items>().delay = DELAY;
        }
        let ready = w.resource::<DevArea>().frame.is_some() && items(w).client.racks.contains_key(&CRATE) && items(w).defs.get("welder").is_some();
        if !ready {
            if c.t > 5.0 {
                check(c, false, "dig: the area and the crate are there".into());
                return true;
            }
            return false;
        }
        let now = w.resource::<Time>().elapsed_secs_f64();
        let welder = items(w).defs.get("welder").unwrap().stack_of(1);
        let _ = w.resource_mut::<Items>().give(now, LOCAL, welder);
        let v = vein_at(w);
        let at = ground(w, [v[0] - 1.6, v[1]], 0.5);
        place_walker(w, at);
        true
    }));
    s.push(wait(0.5));
    s.push(tap_then("dig: 1 takes the welder into the hand", KeyCode::Digit1, |w| items(w).in_hand(0).is_some_and(|s| s.item.kind.as_str() == "welder")));

    // Hold LMB on the vein: the spot heats up (a picture of the glow), then a chunk lies on the ground.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "dig: LMB held on the vein heats the spot");
            let v = vein_at(w);
            let at = ground(w, v, 0.6);
            look_at(w, at);
            c.v.insert("chunks0", chunks(w) as f64);
            check(c, ore_total(w) == 0, format!("dig: no ore anywhere yet ({})", ore_total(w)));
            lmb(w, true);
            return false;
        }
        let heat = w.resource::<DigState>().veins[0].heat;
        let news = items(w).dig.news.as_ref().map(|n| (n.cuts.len(), n.veins[0].heat));
        if heat > 0.7 {
            check(c, news.is_some_and(|n| n.0 == 1 && n.1 > 500), format!("dig: the spot heats up while it is held (heat {heat:.2}, news {news:?})"));
            end(w, c, format!("{:.1} s", c.t));
            return true;
        }
        if c.t >= 6.0 {
            lmb(w, false);
            check(c, false, format!("dig: the spot did not heat in 6 s ({heat:.2})"));
            return true;
        }
        false
    }));
    s.push(shot_step("dig-cut"));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "dig: LMB held on the vein cuts a chunk");
        }
        if super::items::outside(w, ORE).is_some() {
            lmb(w, false);
            let left = chunks(w);
            check(c, left as f64 == c.v["chunks0"] - 1.0, format!("dig: the vein has one chunk less ({} to {left})", c.v["chunks0"]));
            check(c, ore_total(w) == 1, format!("dig: one chunk of ore exists ({})", ore_total(w)));
            end(w, c, format!("{:.1} s", c.t));
            return true;
        }
        if c.t >= 8.0 {
            lmb(w, false);
            check(c, false, "dig: no chunk within 8 s".into());
            return true;
        }
        false
    }));
    s.push(until("dig: letting go cools the spot", 6.0, |w| w.resource::<DigState>().veins[0].heat == 0.0 && items(w).dig.news.as_ref().is_some_and(|n| n.cuts.is_empty())));
    s.push(wait(1.0));

    // Pick it up by hand.
    s.extend(collect(ORE, "Grumblite"));
    s.push(Box::new(|w, c| {
        let (bar, total) = (items(w).host.inventory(LOCAL).count(ORE), ore_total(w));
        check(c, bar == 1 && total == 1, format!("dig: the chunk is in the bar (bar {bar}, all {total})"));
        true
    }));

    // Into Crate B of the storage room.
    s.push(Box::new(|w, c| {
        begin(w, c, "dig: the chunk goes into Crate B");
        let at = ground(w, STORAGE_ROOM.point([0.0, -0.6]), 0.5);
        place_walker(w, at);
        let now = w.resource::<Time>().elapsed_secs_f64();
        let r = w.resource_mut::<Items>().request(now, Request::Open { rack: CRATE });
        check(c, r.is_ok(), format!("dig: the crate opens ({r:?})"));
        true
    }));
    s.push(wait(3.0 * DELAY));
    s.push(Box::new(|w, c| {
        check(c, items(w).host.locks.get(&CRATE) == Some(&LOCAL), "dig: the host holds the crate for the walker".into());
        let slot = (0..8).find(|i| items(w).client.view.slot(*i).is_some_and(|s| s.item.kind.as_str() == ORE));
        let Some(slot) = slot else {
            check(c, false, "dig: the chunk is in the bar".into());
            return true;
        };
        let now = w.resource::<Time>().elapsed_secs_f64();
        let r = w.resource_mut::<Items>().request(now, Request::Store { slot, rack: CRATE });
        check(c, r.is_ok(), format!("dig: the store is accepted ({r:?})"));
        true
    }));
    s.push(wait(3.0 * DELAY + 0.2));
    s.push(Box::new(|w, c| {
        let (bar, held, total) = (items(w).host.inventory(LOCAL).count(ORE), in_crate(w), ore_total(w));
        check(c, bar == 0 && held == 1 && total == 1, format!("dig: the crate holds one, the bar none, nothing lost or doubled (bar {bar}, crate {held}, all {total})"));
        check(c, items(w).client.pending.is_empty() && items(w).client.confirmed.count(ORE) == 0, "dig: the client agrees".into());
        let now = w.resource::<Time>().elapsed_secs_f64();
        let _ = w.resource_mut::<Items>().request(now, Request::Close { rack: CRATE });
        end(w, c, "stored".into());
        true
    }));
    s.push(wait(0.3));

    // The air: outside the bounds nothing happens; inside, without a suit, the breath goes.
    s.push(Box::new(|w, c| {
        begin(w, c, "dig: outside the site the breath is fine");
        let at = ground(w, OPEN_GROUND, 0.3);
        place_walker(w, at);
        true
    }));
    s.push(until("dig: the breath is back to full outside", 20.0, |w| oxygen(w).is_none_or(|o| o >= 0.99)));
    s.push(Box::new(|w, c| {
        if c.t < 4.0 {
            return false;
        }
        let o2 = oxygen(w);
        check(c, o2.is_none_or(|o| o >= 0.99), format!("dig: still full after 4 s outside ({o2:?})"));
        end(w, c, format!("{o2:?}"));
        true
    }));
    s.push(Box::new(|w, c| {
        begin(w, c, "dig: inside the site without a suit");
        let at = ground(w, [DIG_SITE.centre[0], DIG_SITE.centre[1] - 3.0], 0.3);
        place_walker(w, at);
        true
    }));
    s.push(until("dig: the breath drops", 15.0, |w| oxygen(w).is_some_and(|o| o < 0.9)));
    s.push(Box::new(|w, c| {
        let o2 = oxygen(w);
        check(c, o2.is_some_and(|o| o < 1.0), format!("dig: the breath shows on the HUD ({o2:?})"));
        let at = ground(w, OPEN_GROUND, 0.3);
        place_walker(w, at);
        true
    }));
    // A picture of the site from its edge: the haze, the veins (one a chunk smaller).
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "dig: the site from its edge");
            let at = ground(w, [DIG_SITE.centre[0] + 1.0, DIG_SITE.centre[1] - DIG_SITE.half[1] - 3.5], 0.3);
            place_walker(w, at);
            let v = ground(w, DIG_SITE.point([0.5, 1.5]), 0.8);
            look_at(w, v);
        }
        c.t >= 0.8
    }));
    s.push(shot_step("dig-site"));
}
