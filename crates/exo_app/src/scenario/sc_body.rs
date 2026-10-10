//! Scenario `sc-body` (round 5, #198): the SC body through the controls. F7 switches to the SC
//! model, the ship lifts to 400 m and hovers; W from the hover gives the forward thrust with its
//! spool and jerk (times to 50 % and to the peak of `thrust_share.z`); after a brake to rest, W
//! with boost gives the boosted forward thrust (times to 50 % and 95 % of its peak, the forward
//! part of the felt acceleration).
use crate::scenario::{begin, check, end, hold_until, keys, put_at_seat, ship_e, ship_vel, sit, tap, with_ship, Step};
use crate::ship::FlightModel;
use avian3d::prelude::Position;
use bevy::prelude::*;

/// m above the planet's centre-relative reference sphere (the terrain does not matter here).
fn height(w: &mut World) -> f64 {
    let e = ship_e(w);
    let p = w.get::<Position>(e).unwrap().0;
    let pl = crate::scenario::planet(w);
    (p - pl.centre).length() - pl.radius
}

/// The forward part of the felt acceleration (g) from the felt total, the hover's 1 g removed.
fn forward_g(felt: f64) -> f64 {
    (felt * felt - 1.0).max(0.0).sqrt()
}

/// The first time in a run at which column `col` reaches `level`.
fn first(rec: &[(f64, [f64; 2])], col: usize, level: f64) -> Option<f64> {
    rec.iter().find(|(_, v)| v[col] >= level).map(|(t, _)| *t)
}

fn fmt(t: Option<f64>) -> String {
    t.map_or("never".to_string(), |t| format!("{t:.2} s"))
}

/// Holds `ks` for `limit` s and records (time, [thrust share, forward g]) every step. At the end
/// it checks the times to 50 % and to the peak (share) or to 50 % and 95 % of the peak (g).
fn thrust_run(name: &'static str, ks: &'static [KeyCode], limit: f64, boost: bool) -> Step {
    let mut rec: Vec<(f64, [f64; 2])> = Vec::new();
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            keys(w, ks, true);
        }
        let (share, felt) = with_ship(w, |s| (s.sc.status.thrust_share.z.abs(), s.sc.status.felt_g));
        rec.push((c.t, [share, forward_g(felt)]));
        if c.t < limit {
            return false;
        }
        keys(w, ks, false);
        let peak_share = rec.iter().map(|(_, v)| v[0]).fold(0.0, f64::max);
        let peak_g = rec.iter().map(|(_, v)| v[1]).fold(0.0, f64::max);
        let spool = with_ship(w, |s| s.sc.tuning.drive.spool_delay.main);
        let note = if boost {
            let (t50, t95) = (first(&rec, 1, 0.5 * peak_g), first(&rec, 1, 0.95 * peak_g));
            check(c, peak_g > 0.0, format!("boost: peak forward {peak_g:.2} g, 50 % at {}, 95 % at {}", fmt(t50), fmt(t95)));
            format!("peak forward {peak_g:.2} g")
        } else {
            let (t50, t100) = (first(&rec, 0, 0.5), first(&rec, 0, 0.999 * peak_share));
            let ok = t50.is_some_and(|t| t > spool) && peak_share > 0.3;
            check(c, ok, format!("forward W: spool {spool:.2} s, 50 % at {}, peak share {peak_share:.3} at {}", fmt(t50), fmt(t100)));
            format!("peak share {peak_share:.3}")
        };
        end(w, c, note);
        true
    })
}

pub fn sc_body_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::F7);
        true
    }));
    s.push(Box::new(|w, c| {
        let now = with_ship(w, |s| s.model);
        check(c, now == FlightModel::Sc, format!("F7: model {now:?}"));
        true
    }));
    s.push(hold_until("lift to 400 m", &[KeyCode::Space], 90.0, |w| height(w) >= 400.0));
    s.push(Box::new(|w, c| {
        let h = height(w);
        check(c, h >= 400.0, format!("lift: {h:.1} m"));
        true
    }));
    // Released at 400 m the climb still carries speed: the hold brings it to rest first.
    s.push(hold_until("settle", &[], 30.0, |w| ship_vel(w).length() < 0.5));
    s.push(Box::new(|w, c| {
        c.v.insert("h0", height(w));
        true
    }));
    s.push(hold_until("hover 3 s", &[], 3.0, |_| false));
    s.push(Box::new(|w, c| {
        let drift = height(w) - c.v["h0"];
        check(c, drift.abs() < 1.0, format!("hover: {drift:+.2} m in 3 s"));
        true
    }));
    s.push(thrust_run("W from hover", &[KeyCode::KeyW], 3.0, false));
    s.push(hold_until("brake to rest", &[KeyCode::KeyX], 60.0, |w| ship_vel(w).length() < 1.0));
    s.push(hold_until("settle 2 s", &[], 2.0, |_| false));
    s.push(thrust_run("W with boost from hover", &[KeyCode::KeyW, KeyCode::ShiftLeft], 1.5, true));
    s.push(hold_until("brake to rest", &[KeyCode::KeyX], 60.0, |w| ship_vel(w).length() < 1.0));
}
