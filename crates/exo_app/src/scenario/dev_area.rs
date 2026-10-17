//! The dev area: level ground and the walker's start.
use super::*;
use crate::dev_area::{DevArea, HALF_EXTENT};

/// Area coordinates (m east, m north of the area's centre) of a world point.
fn area_xy(w: &World, p: DVec3) -> [f64; 2] {
    let pl = planet(w);
    let f = w.resource::<DevArea>().frame.expect("dev area frame");
    let d = pl.up(p) - f.dir;
    [d.dot(f.east) * pl.radius, d.dot(f.north) * pl.radius]
}

pub(crate) fn tap_until(k: KeyCode, what: &'static str, limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            tap(w, k);
            return false;
        }
        if done(w) {
            check(c, true, format!("{what} ({:.2} s)", c.t));
            return true;
        }
        if c.t >= limit {
            check(c, false, format!("{what}: not within {limit} s"));
            return true;
        }
        false
    })
}

pub(super) fn dev_area_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        begin(w, c, "dev area: the level rectangle");
        let Some(frame) = w.resource::<DevArea>().frame else {
            check(c, false, "dev area: the place 'dev_area' is on the planet".into());
            return true;
        };
        let pl = planet(w);
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for i in -10..=10 {
            for j in -6..=6 {
                let (p, _) = frame.ground(&pl, i as f64 * HALF_EXTENT[0] / 10.0, j as f64 * HALF_EXTENT[1] / 10.0, 0.0);
                let h = (p - pl.centre).length() - pl.radius;
                (lo, hi) = (lo.min(h), hi.max(h));
            }
        }
        let p = player_world(w);
        let xy = area_xy(w, p);
        let (grounded, agl) = (with_player(w, |p| p.w.grounded), pl.above_ground(p));
        check(c, xy[0].abs() < HALF_EXTENT[0] && xy[1].abs() < HALF_EXTENT[1] && grounded && agl.abs() < 0.1,
            format!("dev area: the walker starts on it (at {:.0} m east, {:.0} m north, {agl:.3} m above the ground, grounded {grounded})", xy[0], xy[1]));
        end(w, c, format!("level within {:.3} m", hi - lo));
        true
    }));
    s.push(shot_step("start"));
}
