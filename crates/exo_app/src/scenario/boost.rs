//! Scenario `boost-hud` (#90, #91): the boost capacitor drains, cuts out and recharges, and the
//! minimal HUD shows it with speed and altitude, all driven through `Controls`.
use crate::hud::HudReadout;
use crate::scenario::{above_ground, altitude, begin, check, end, hold_until, keys, put_at_seat, ship_vel, sit, with_ship, Ctx, Step};
use bevy::prelude::*;

fn readout(w: &World) -> HudReadout {
    w.resource::<HudReadout>().clone()
}

fn cap(w: &mut World) -> flight_core::BoostCapacitorTuning {
    with_ship(w, |s| s.sc.tuning.drive.boost_capacitor.clone())
}

/// The number in a text such as "ALT 450 m" or "123.4 m/s".
fn number(t: &str) -> Option<f64> {
    t.split_whitespace().find_map(|p| p.parse().ok())
}

/// Speed and altitude in the HUD match the ship; no debug words in any permanent element.
fn check_readout(w: &mut World, c: &mut Ctx, when: &str) {
    let r = readout(w);
    let (v, agl_below) = (ship_vel(w).length(), w.resource::<crate::tuning::Tuning>().hud.agl_below);
    // AGL (height above the terrain under the ship) near the ground, ALT above.
    let agl = above_ground(w);
    let (word, alt) = if agl < agl_below { ("AGL", agl) } else { ("ALT", altitude(w)) };
    let shown_v = number(&r.texts[1]).unwrap_or(f64::NAN);
    let shown_alt = if r.texts[2].starts_with(word) { number(&r.texts[2]).unwrap_or(f64::NAN) } else { f64::NAN };
    // One fixed step apart at most (the readout runs after the step, the check before the next).
    check(c, (shown_v - v).abs() <= 0.5 + 0.02 * v, format!("hud {when}: speed {:?} for {v:.1} m/s", r.texts[1]));
    check(c, (shown_alt - alt).abs() <= 1.0 + 0.03 * v, format!("hud {when}: altitude {:?} for {word} {alt:.0} m", r.texts[2]));
    let debug = ["ms", "chunk", "patch", "limit", "grounded", "coupling", "rescue", "(H)", "(L)"];
    let bad: Vec<_> = r.texts.iter().filter(|t| debug.iter().any(|d| t.contains(d))).collect();
    check(c, bad.is_empty(), format!("hud {when}: no debug words in {:?}", r.texts));
}

pub fn boost_hud_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(Box::new(|w, c| {
        let r = readout(w);
        check(c, r.texts[0].starts_with("SHIP SC") && r.gauge == Some(1.0) && r.texts[3] == "BOOST 100 %" && r.boost_mode == "CAPACITOR", format!("hud seated: {:?}, gauge {:?}, {}", r.texts, r.gauge, r.boost_mode));
        true
    }));
    // Climb without boost, so the meter is still full.
    s.push(hold_until("climb to 300 m above ground", &[KeyCode::Space], 90.0, |w| above_ground(w) > 300.0));
    s.push(hold_until("cruise", &[KeyCode::KeyW], 8.0, |_| false));
    s.push(Box::new(|w, c| {
        check_readout(w, c, "cruise");
        let charge = with_ship(w, |s| s.sc.drive.boost.charge);
        check(c, charge == 1.0, format!("capacitor full before the boost: {charge:.3}"));
        check(c, readout(w).texts[2].starts_with("AGL"), format!("hud at 300 m above ground: AGL ({:?})", readout(w).texts[2]));
        // Above the threshold ALT: lowered under the ship for one step instead of a long climb.
        let keep = w.resource::<crate::tuning::Tuning>().hud.agl_below;
        c.v.insert("agl_below", keep);
        w.resource_mut::<crate::tuning::Tuning>().hud.agl_below = 100.0;
        true
    }));
    s.push(Box::new(|w, c| {
        check_readout(w, c, "threshold lowered to 100 m");
        let r = readout(w);
        check(c, r.texts[2].starts_with("ALT"), format!("hud above the threshold: ALT ({:?})", r.texts[2]));
        w.resource_mut::<crate::tuning::Tuning>().hud.agl_below = c.v["agl_below"];
        true
    }));
    // Hold W + Shift until the meter is empty: it lasts the drain time, the limit is raised while
    // it runs and drops back although Shift is still held.
    s.push(Box::new(|w, c| {
        let (limit, active, charge) = with_ship(w, |s| (s.sc.status.cap, s.sc.drive.boost.active, s.sc.drive.boost.charge));
        if c.t == 0.0 {
            begin(w, c, "boost until empty");
            c.v.insert("limit0", limit);
            c.v.insert("max_limit", limit);
            c.v.insert("seen_boosting", 0.0);
            keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], true);
            return false;
        }
        let max = c.v["max_limit"].max(limit);
        c.v.insert("max_limit", max);
        let r = readout(w);
        if c.t >= 1.0 && !c.v.contains_key("checked_1s") {
            c.v.insert("checked_1s", 1.0);
            // The drain is the SC capacitor's drain time (20 s), so one second takes 1/drain of it.
            let want = 1.0 - 1.0 / cap(w).drain_time;
            check(c, r.boosting && r.gauge.is_some_and(|g| (g - want).abs() < 0.01), format!("hud after 1 s of boost: {:?}, gauge {:?} (want {want:.2}), boosting {}", r.texts[3], r.gauge, r.boosting));
            check_readout(w, c, "boosting");
        }
        let drain = cap(w).drain_time;
        if (!active && charge == 0.0) || c.t > drain + 2.0 {
            c.v.insert("empty_at", c.t);
            let l0 = c.v["limit0"];
            end(w, c, format!("empty after {:.2} s (drain time {drain}), limit {l0:.0} -> max {max:.0} m/s", c.t));
            check(c, (c.t - drain).abs() <= 0.1, format!("boost: empty after {:.2} s, drain time {drain} s", c.t));
            check(c, max > 1.3 * l0, format!("boost: limit {l0:.0} -> {max:.0} m/s while charged"));
            return true;
        }
        false
    }));
    s.push(Box::new(|w, c| {
        // Shift still held on the empty meter: no boost, the limit falls back.
        if c.t == 0.0 {
            begin(w, c, "boost held on empty");
        }
        if c.t >= 0.8 {
            let (limit, strength) = with_ship(w, |s| (s.sc.status.cap, s.sc.status.boost_active as u8 as f64));
            // The air thins with the climb during the boost, so the plain cap is compared with the
            // boosted peak, not with the cap at the start.
            let peak = c.v.get("max_limit").copied().unwrap_or(limit);
            let r = readout(w);
            end(w, c, format!("limit {limit:.0} m/s (boost peak {peak:.0}), strength {strength:.2}"));
            check(c, strength == 0.0 && limit < 0.7 * peak, format!("boost empty: strength {strength:.2}, limit {limit:.0} m/s (boost peak {peak:.0})"));
            check(c, !r.boosting && r.texts[3].starts_with("BOOST") && !r.ready, format!("hud empty: {:?}, boosting {}, ready {}", r.texts[3], r.boosting, r.ready));
            keys(w, &[KeyCode::ShiftLeft], false);
            return true;
        }
        false
    }));
    // Released: nothing during the rest of the delay (it counts from the last use), then full in
    // the recharge time.
    s.push(Box::new(|w, c| {
        let t = cap(w);
        let charge = with_ship(w, |s| s.sc.drive.boost.charge);
        if c.t == 0.0 {
            begin(w, c, "recharge");
            // 0.8 s after empty, Shift released: the recharge began at the delay (0.2 s) while held.
            let at_release = (0.8 - t.recharge_delay) / t.recharge_time;
            check(c, (charge - at_release).abs() < 0.005, format!("recharge: charge {charge:.4} at release, want {at_release:.4} (the delay ran while held)"));
        }
        // 0.8 s of the delay passed while Shift was still held on empty.
        let since_empty = c.t + 0.8;
        if charge >= 1.0 || c.t > t.recharge_delay + t.recharge_time + 2.0 {
            let full = since_empty;
            let r = readout(w);
            end(w, c, format!("full after {full:.2} s"));
            check(c, (full - t.recharge_delay - t.recharge_time).abs() <= 0.15, format!("recharge: full {full:.2} s after empty (delay + recharge {} s)", t.recharge_delay + t.recharge_time));
            check(c, r.texts[3] == "BOOST 100 %" && r.ready, format!("hud recharged: {:?}", r.texts[3]));
            check_readout(w, c, "recharged");
            keys(w, &[KeyCode::KeyW], false);
            return true;
        }
        false
    }));
    // Half the meter, then X with Shift still held: the brake uses no charge (TODO(initiator), #90).
    s.push(hold_until("boost to half the meter", &[KeyCode::KeyW, KeyCode::ShiftLeft], 10.0, |w| with_ship(w, |s| s.sc.drive.boost.charge) <= 0.5));
    s.push(Box::new(|w, c| {
        let charge = with_ship(w, |s| s.sc.drive.boost.charge);
        if c.t == 0.0 {
            begin(w, c, "firm brake with Shift held");
            c.v.insert("charge0", charge);
            c.v.insert("min", charge);
            keys(w, &[KeyCode::KeyX, KeyCode::ShiftLeft], true);
            return false;
        }
        let min = c.v["min"].min(charge);
        c.v.insert("min", min);
        if ship_vel(w).length() < 0.5 || c.t > 15.0 {
            keys(w, &[KeyCode::KeyX, KeyCode::ShiftLeft], false);
            let c0 = c.v["charge0"];
            let r = readout(w);
            end(w, c, format!("{:.1} s, charge {c0:.3}, lowest {min:.3}", c.t));
            check(c, min >= c0 - 1e-9 && min < 0.6, format!("the brake used no charge: {c0:.3} at the start, lowest {min:.3}"));
            check(c, !r.boosting, format!("hud braking: not boosting ({:?})", r.texts[3]));
            check_readout(w, c, "stopped");
            return true;
        }
        false
    }));
    s.push(hold_until("firm brake", &[KeyCode::KeyX], 20.0, |w| ship_vel(w).length() < 0.5));
}
