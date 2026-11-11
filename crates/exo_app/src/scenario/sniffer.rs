//! The sniffer and the fizz pockets (#333): with the fixed seed the walker holds the sniffer; its signal rises from
//! 15 m to 6 m to 0.5 m from the first pocket; a hold of LMB away from the pocket opens nothing, a hold on it opens
//! the vent; F with an empty jar gives a fizz jar, three times, a fourth is refused, the pocket is spent and a new
//! hidden one lies elsewhere; two fizz jars make a full extinguisher at the fabricator. Every step polls for its
//! outcome with a limit in simulated seconds.
use super::dig::until;
use super::fabricator::{ask, fab, load_item, look_at_button, push, rack_count, says, stand_before};
use super::items::{items, look_at, prompt, DELAY};
use super::*;
use crate::controls::Controls;
use crate::dev_area::fabricator::{FAB_RACK, OUTPUT};
use crate::dev_area::sniffer::{PocketCfg, PocketHost, Sniffing, FIZZ_JAR, SNIFFER};
use crate::dev_area::DevArea;
use crate::items::{Items, LOCAL};
use dig_core::pocket::{signal_at, PocketState};
use item_core::Request;

const JAR: &str = "jar";

fn lmb(w: &mut World, down: bool) {
    w.resource_mut::<Controls>().primary_held = down;
}

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

/// The first pocket's place and state (the one the whole scenario works at).
fn pocket(w: &World) -> ([f64; 2], PocketState, u32) {
    let p = &w.resource::<PocketHost>().field.pockets()[0];
    (p.at, p.state, p.jars_left)
}

fn is_open(w: &World) -> bool {
    matches!(pocket(w).1, PocketState::Open { .. })
}

fn bar_count(w: &World, kind: &str) -> u32 {
    items(w).host.inventory(LOCAL).count(kind)
}

fn bar_slot(w: &World, kind: &str) -> Option<usize> {
    (0..8).find(|i| items(w).client.view.slot(*i).is_some_and(|s| s.item.kind.as_str() == kind))
}

fn give(w: &mut World, id: &str, n: u32) {
    let now = w.resource::<Time>().elapsed_secs_f64();
    let stack = items(w).defs.get(id).unwrap().stack_of(n);
    let _ = w.resource_mut::<Items>().give(now, LOCAL, stack);
}

/// The direction (unit, area metres) from the pocket along which `reach` metres still lie on free ground and keep the
/// farthest from the other pockets, so the signal there is the first pocket's.
fn lane(w: &World, reach: f64) -> [f64; 2] {
    let host = w.resource::<PocketHost>();
    let (me, margin) = (host.field.pockets()[0].at, host.field.config().margin_m);
    let others: Vec<[f64; 2]> = host.field.pockets().iter().skip(1).map(|p| p.at).collect();
    (0..16)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / 16.0;
            [a.cos(), a.sin()]
        })
        .filter(|d| host.field.ground().is_free([me[0] + d[0] * reach, me[1] + d[1] * reach], margin))
        .max_by(|a, b| {
            let far = |d: &[f64; 2]| others.iter().map(|o| ((me[0] + d[0] * reach - o[0]).powi(2) + (me[1] + d[1] * reach - o[1]).powi(2)).sqrt()).fold(f64::INFINITY, f64::min);
            far(a).total_cmp(&far(b))
        })
        .expect("a free lane from the pocket")
}

/// Stands `d` metres from the first pocket along its lane, looking at it.
fn stand_at(w: &mut World, d: f64) {
    let (at, dir) = (pocket(w).0, lane(w, 15.0));
    let p = ground(w, [at[0] + dir[0] * d, at[1] + dir[1] * d], 0.3);
    place_walker(w, p);
    let v = ground(w, at, 0.15);
    look_at(w, v);
}

/// A step that places the walker `d` metres out, waits a moment and checks the reading against the sim's.
fn read_at(what: &'static str, key: &'static str, d: f64) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            begin(w, c, what);
            stand_at(w, d);
            true
        }),
        wait(0.4),
        Box::new(move |w, c| {
            let s = w.resource::<Sniffing>().clone();
            let host = w.resource::<PocketHost>();
            let want = signal_at(&w.resource::<PocketCfg>().0.pocket, host.field.pockets().iter().map(|p| p.at), {
                let me = pocket(w).0;
                let dir = lane(w, 15.0);
                [me[0] + dir[0] * d, me[1] + dir[1] * d]
            });
            check(c, s.on, format!("{what}: the sniffer is on while it is the selected item"));
            check(c, (s.signal - want).abs() < 0.05, format!("{what}: the walker reads {:.3}, the pockets say {want:.3}", s.signal));
            c.v.insert(key, s.signal);
            end(w, c, format!("{:.3}", s.signal));
            true
        }),
    ]
}

pub(super) fn sniffer_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "sniffer: the sniffer and four empty jars in the bar");
            w.resource_mut::<Items>().delay = DELAY;
        }
        let ready = w.resource::<DevArea>().frame.is_some() && items(w).client.racks.contains_key(&FAB_RACK) && items(w).defs.get(SNIFFER).is_some();
        if !ready {
            if c.t > 5.0 {
                check(c, false, "sniffer: the area, the fabricator and the items are there".into());
                return true;
            }
            return false;
        }
        give(w, SNIFFER, 1);
        give(w, JAR, 4);
        let pockets = w.resource::<PocketHost>().field.pockets().len();
        check(c, pockets == 2, format!("sniffer: two hidden pockets lie in the field ({pockets})"));
        check(c, w.resource::<PocketHost>().field.pockets().iter().all(|p| p.state == PocketState::Hidden), "sniffer: every pocket is hidden".into());
        stand_at(w, 15.0);
        true
    }));
    s.push(wait(0.5));
    s.push(super::items::tap_then("sniffer: 1 takes the sniffer into the hand", KeyCode::Digit1, |w| items(w).in_hand(0).is_some_and(|s| s.item.kind.as_str() == SNIFFER)));

    // The signal rises as the walker comes closer.
    s.extend(read_at("sniffer: 15 m from the pocket", "s15", 15.0));
    s.extend(read_at("sniffer: 6 m from the pocket", "s6", 6.0));
    s.extend(read_at("sniffer: 0.5 m from the pocket", "s05", 0.5));
    s.push(Box::new(|w, c| {
        let (a, b, d) = (c.v["s15"], c.v["s6"], c.v["s05"]);
        check(c, a > 0.0 && a < b && b < d, format!("sniffer: the signal rises with each step closer ({a:.3} < {b:.3} < {d:.3})"));
        check(c, d >= 0.95, format!("sniffer: on the pocket it is past the buzz line ({d:.3})"));
        // A picture's place: where the bar stands about half (found on the pocket's own curve).
        let (cfg, at) = (w.resource::<PocketCfg>().0.pocket.clone(), pocket(w).0);
        let half = (1..180).map(|i| i as f64 * 0.1).min_by(|x, y| (signal_at(&cfg, [at], [at[0] + x, at[1]]) - 0.5).abs().total_cmp(&(signal_at(&cfg, [at], [at[0] + y, at[1]]) - 0.5).abs())).unwrap_or(3.0);
        c.v.insert("half_d", half);
        true
    }));

    // Looking at the bar about half full.
    s.push(Box::new(|w, c| {
        begin(w, c, "sniffer: a picture with the bar about half");
        let d = c.v["half_d"];
        stand_at(w, d);
        // The held item shows by itself; look a little to the side of the pocket so the hand is clear.
        true
    }));
    s.push(wait(0.8));
    s.push(shot_step("sniffer-held"));

    // A hold away from the pocket opens nothing.
    s.push(Box::new(|w, c| {
        begin(w, c, "sniffer: LMB held 3 s away from the pocket opens nothing");
        stand_at(w, 4.0);
        lmb(w, true);
        true
    }));
    s.push(Box::new(|w, c| {
        if c.t < 3.2 {
            return false;
        }
        lmb(w, false);
        check(c, !is_open(w) && pocket(w).1 == PocketState::Hidden, "sniffer: the pocket is still hidden".into());
        end(w, c, "nothing".into());
        true
    }));
    s.push(wait(0.5));

    // A hold on it opens the vent.
    s.push(Box::new(|w, c| {
        begin(w, c, "sniffer: LMB held on the pocket opens the vent");
        stand_at(w, 0.5);
        lmb(w, true);
        c.v.insert("t0", 0.0);
        false
    }));
    s.push(Box::new(|w, c| {
        if c.t > 0.5 && !c.v.contains_key("progress") {
            let sn = *w.resource::<Sniffing>();
            let tap = w.resource::<PocketHost>().field.pockets()[0].tap;
            check(c, sn.tapping && tap > 0.0 && tap < 2.0, format!("sniffer: the tap's progress runs on the host ({tap:.2} s)"));
            c.v.insert("progress", 1.0);
        }
        if is_open(w) {
            lmb(w, false);
            check(c, c.t >= 1.6, format!("sniffer: it took about tap_s of holding ({:.1} s)", c.t));
            end(w, c, format!("{:.1} s", c.t));
            return true;
        }
        if c.t >= 6.0 {
            lmb(w, false);
            check(c, false, "sniffer: the vent did not open within 6 s".into());
            return true;
        }
        false
    }));
    s.push(until("sniffer: the news say the vent puffs", 2.0, |w| items(w).dig.pockets.as_ref().is_some_and(|n| n.pockets[0].open_ds > 0)));

    // The vent from 3 m, puffing.
    s.push(Box::new(|w, c| {
        begin(w, c, "sniffer: the vent from 3 m");
        stand_at(w, 3.0);
        let top = ground(w, pocket(w).0, 0.35);
        look_at(w, top);
        true
    }));
    s.push(wait(1.8));
    s.push(shot_step("sniffer-vent"));

    // No jar in the hand: the hint says one is needed.
    s.push(Box::new(|w, c| {
        stand_at(w, 1.3);
        let top = ground(w, pocket(w).0, 0.15);
        look_at(w, top);
        if c.t < 0.4 {
            return false;
        }
        let p = prompt(w);
        check(c, p.contains("needs an empty jar"), format!("sniffer: without a jar the hint says so ({p:?})"));
        true
    }));
    s.push(Box::new(|w, c| {
        begin(w, c, "sniffer: 2 takes the jars into the hand");
        let slot = bar_slot(w, JAR).unwrap_or(1);
        tap(w, [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4][slot.min(3)]);
        true
    }));
    s.push(wait(0.5));
    for n in 1..=3u32 {
        let what: &'static str = ["sniffer: F fills the first jar", "sniffer: F fills the second jar", "sniffer: F fills the third jar"][n as usize - 1];
        s.push(Box::new(move |w, c| {
            let top = ground(w, pocket(w).0, 0.15);
            look_at(w, top);
            if c.t == 0.0 {
                begin(w, c, what);
                return false;
            }
            if c.t < 0.4 {
                return false;
            }
            if !c.v.contains_key("pressed") {
                let p = prompt(w);
                check(c, p.contains("fill jar"), format!("{what}: the hint offers it ({p:?})"));
                tap(w, KeyCode::KeyF);
                c.v.insert("pressed", 1.0);
                return false;
            }
            if bar_count(w, FIZZ_JAR) == n {
                check(c, bar_count(w, JAR) == 4 - n && bar_count(w, JAR) + bar_count(w, FIZZ_JAR) == 4, format!("sniffer: the jar became a fizz jar (jars {}, fizz {})", bar_count(w, JAR), bar_count(w, FIZZ_JAR)));
                end(w, c, format!("{:.1} s", c.t));
                return true;
            }
            if c.t >= 4.0 {
                check(c, false, format!("{what}: no fizz jar within 4 s"));
                return true;
            }
            false
        }));
        s.push(wait(0.3));
    }
    s.push(Box::new(|w, c| {
        begin(w, c, "sniffer: the pocket is spent and a new hidden one lies elsewhere");
        let (at, state, jars) = pocket(w);
        let filled = w.resource::<PocketHost>().filled;
        check(c, filled == 3, format!("sniffer: exactly three jars were filled ({filled})"));
        check(c, state == PocketState::Hidden && jars == 3, format!("sniffer: the successor is hidden with all its jars ({state:?}, {jars})"));
        c.p.insert("spent_at", ground(w, at, 0.0));
        true
    }));
    s.push(until("sniffer: the news show the vent gone", 2.0, |w| items(w).dig.pockets.as_ref().is_some_and(|n| n.pockets.iter().all(|p| p.open_ds == 0))));
    s.push(Box::new(|w, c| {
        let gap = w.resource::<PocketCfg>().0.pocket.min_gap_m;
        let (at, _, _) = pocket(w);
        let now = ground(w, at, 0.0);
        let d = now.distance(c.p["spent_at"]);
        check(c, d >= gap * 0.99, format!("sniffer: the new pocket lies {d:.1} m from the spent one (gap {gap} m)"));
        // A fourth fill: F finds no vent (the prompt is not the fill hint) and a stray ask for the new pocket gives the jar back.
        let p = prompt(w);
        check(c, !p.contains("fill jar"), format!("sniffer: no fill hint where the vent was ({p:?})"));
        tap(w, KeyCode::KeyF);
        let slot = bar_slot(w, JAR).unwrap_or(1);
        let r = ask(w, Request::Load { slot, into: 0 });
        check(c, r.is_ok(), format!("sniffer: the stray fill is sent ({r:?})"));
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let filled = w.resource::<PocketHost>().filled;
        check(c, filled == 3 && bar_count(w, FIZZ_JAR) == 3 && bar_count(w, JAR) == 1, format!("sniffer: the fourth is refused, the jar is still empty (filled {filled}, jars {}, fizz {})", bar_count(w, JAR), bar_count(w, FIZZ_JAR)));
        end(w, c, "refused".into());
        true
    }));

    // Two fizz jars at the fabricator make a full extinguisher; the third stays.
    s.push(Box::new(|w, c| {
        begin(w, c, "sniffer: the fabricator takes the fizz jars");
        stand_before(w);
        let slot = bar_slot(w, FIZZ_JAR);
        check(c, slot.is_some(), "sniffer: the fizz jars are in the bar".into());
        true
    }));
    s.push(wait(0.5));
    s.extend(load_item("sniffer: the fizz jars into the first input", FIZZ_JAR, 0));
    s.push(Box::new(|w, c| {
        check(c, rack_count(w, FIZZ_JAR) == 3, format!("sniffer: the input place took the fizz jars ({})", rack_count(w, FIZZ_JAR)));
        true
    }));
    s.push(until("sniffer: the screen names the recipe", 3.0, |w| says(w, "2 FIZZ JAR > EXTINGUISHER") && says(w, "PUSH TO START")));
    s.push(Box::new(|w, _| {
        look_at_button(w);
        true
    }));
    s.push(push("sniffer: F on the button starts the run"));
    s.push(until("sniffer: it runs", 3.0, |w| fab(w).fab.running()));
    s.push(until("sniffer: the extinguisher lies in the output place", 30.0, |w| rack_count(w, "extinguisher") == 1));
    s.push(Box::new(|w, c| {
        let out = items(w).host.racks.get(&FAB_RACK).and_then(|r| r.slot(OUTPUT)).cloned();
        let cond = out.as_ref().and_then(|s| s.item.condition);
        check(c, out.as_ref().is_some_and(|s| s.item.kind.as_str() == "extinguisher" && s.count == 1), "sniffer: one extinguisher is in the output place".into());
        check(c, cond == Some(100.0), format!("sniffer: it is a full one (condition {cond:?})"));
        check(c, rack_count(w, FIZZ_JAR) == 1, format!("sniffer: two fizz jars were used, one is left ({})", rack_count(w, FIZZ_JAR)));
        end(w, c, "made".into());
        true
    }));
}
