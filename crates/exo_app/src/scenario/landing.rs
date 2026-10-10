//! Scenario `slope-landing` (#92): set down on a slope below the slope limit, the ship stays on
//! its touchdown spot until thrust.
use crate::env::PlanetRes;
use crate::scenario::{above_ground, begin, check, end, hold_until, land, planet, put_at_seat, ship_frame_of, sit, teleport_ship, wait, with_ship, Step};
use crate::ship::basis_for_up;
use bevy::math::DVec3;
use bevy::prelude::*;
use flight_core::GroundRules;

/// Degrees below the slope limit the ground is searched for: steep, but held.
const BELOW_LIMIT: f64 = 5.0;
/// Grid spacing (m) and half extent (cells) of the search around the start.
const GRID: f64 = 25.0;
const CELLS: i32 = 40;

/// The spot nearest to `want` degrees of slope in a grid around `up`: on land, and even under the
/// hull (the slope 4 m to each side within 3 degrees of the centre's). Returns the ground's
/// direction and its slope in degrees.
fn find_slope(pl: &PlanetRes, ctl: &GroundRules, up: DVec3, want: f64) -> Option<(DVec3, f64)> {
    let ground = |dir: DVec3| pl.centre + dir * pl.surface(dir);
    let slope = |dir: DVec3| ctl.ground_slope(pl, ground(dir)).to_degrees();
    let (a, b) = up.any_orthonormal_pair();
    let mut best: Option<(DVec3, f64)> = None;
    for i in -CELLS..=CELLS {
        for j in -CELLS..=CELLS {
            let dir = (up * pl.radius + (a * i as f64 + b * j as f64) * GRID).normalize();
            if pl.surface(dir) - pl.radius < pl.sea + 1.0 {
                continue;
            }
            let s = slope(dir);
            if best.is_some_and(|(_, bs)| (bs - want).abs() <= (s - want).abs()) {
                continue;
            }
            let even = [a, -a, b, -b].into_iter().all(|side| (slope((dir * pl.radius + side * 4.0).normalize()) - s).abs() < 3.0);
            if even {
                best = Some((dir, s));
            }
        }
    }
    best
}

pub fn slope_landing_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(Box::new(|w, c| {
        begin(w, c, "slope-landing: find a slope below the limit");
        let pl = planet(w);
        let start = ship_frame_of(w).origin;
        let ctl = with_ship(w, |s| s.ground.clone());
        let limit = ctl.tuning.landing_slope_limit;
        let Some((dir, slope)) = find_slope(&pl, &ctl, pl.up(start), limit - BELOW_LIMIT) else {
            end(w, c, "no spot".into());
            check(c, false, "slope-landing: a slope to land on".into());
            return true;
        };
        // Level, 12 m above the highest ground under the hull.
        let rot = basis_for_up(dir);
        let top = [DVec3::ZERO, DVec3::new(2.3, 0.0, 4.2), DVec3::new(-2.3, 0.0, 4.2), DVec3::new(2.3, 0.0, -4.2), DVec3::new(-2.3, 0.0, -4.2)]
            .into_iter()
            .map(|corner| pl.surface(dir * pl.radius + rot * corner))
            .fold(f64::MIN, f64::max);
        teleport_ship(w, pl.centre + dir * (top + 12.0), rot);
        c.v.insert("slope", slope);
        let far = (dir * pl.radius).distance(pl.up(start) * pl.radius);
        end(w, c, format!("{slope:.1} deg, {far:.0} m from the start (limit {limit:.0} deg)"));
        check(c, slope > limit - BELOW_LIMIT - 3.0 && slope <= limit, format!("slope-landing: found a {slope:.1} deg slope (limit {limit:.0} deg)"));
        true
    }));
    // The collision ring builds the ground under the new spot.
    s.push(wait(1.0));
    s.push(land("slope-landing: land on the slope"));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "slope-landing: rest 10 s, no input");
            c.v.insert("drift", 0.0);
        }
        let f = ship_frame_of(w);
        let up = planet(w).up(f.origin);
        let d = f.origin - c.p["touch"];
        let drift = c.v["drift"].max((d - up * d.dot(up)).length());
        c.v.insert("drift", drift);
        if c.t >= 10.0 {
            let hold = with_ship(w, |s| s.ground.ground_hold);
            let tilt = (f.rot * DVec3::Y).angle_between(up).to_degrees();
            let slope = c.v["slope"];
            end(w, c, format!("held {}, at rest {}, tilt {tilt:.1} deg on a {slope:.1} deg slope, largest drift from touchdown {:.3} mm", hold.is_some(), hold.is_some_and(|h| h.rest.is_some()), drift * 1000.0));
            check(c, hold.is_some_and(|h| h.rest.is_some()), "slope-landing: the ship is held at rest".into());
            check(c, (tilt - slope).abs() < 5.0, format!("slope-landing: it rests on the slope (tilt {tilt:.1} deg, slope {slope:.1} deg)"));
            check(c, drift < crate::scenario::LANDING_SETTLE_M, format!("slope-landing: no drift after touchdown ({:.3} mm)", drift * 1000.0));
            return true;
        }
        false
    }));
    s.push(hold_until("slope-landing: thrust up", &[KeyCode::Space], 3.0, |w| above_ground(w) > 5.0));
    s.push(Box::new(|w, c| {
        let (hold, agl) = (with_ship(w, |s| s.ground.ground_hold), above_ground(w));
        check(c, hold.is_none() && agl > 5.0, format!("slope-landing: thrust lets go, the ship lifts ({agl:.1} m above ground)"));
        true
    }));
}
