//! Scenarios of milestone C (crates, grab, lock grid, budget). They drive the game through
//! `Controls` like the others; crates are placed by test hooks.
use crate::cargo::{cabin_floor_pos, crate_bundle, CargoStats, Crate, CrateWatch, Crates};
use crate::scenario::*;
use crate::ship::{basis_for_up, cabin_contains};
use bevy::math::DVec3;
use bevy::prelude::*;

/// Removes every crate (scenarios start from a known set).
pub(crate) fn clear_crates(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Crate>>().iter(w).collect();
    for e in es {
        w.despawn(e);
    }
}

/// Spawns a crate of `size` resting on the own cabin floor at (x, z), ship-local.
pub(crate) fn cabin_crate(w: &mut World, size: &str, x: f64, z: f64) -> Entity {
    let ship = ship_e(w);
    let t = w.resource::<Crates>().0.clone();
    let b = crate_bundle(&t, size, Some(ship), cabin_floor_pos(&t, size, x, z), DVec3::NEG_Z);
    w.spawn(b).id()
}

pub(crate) fn crate_of(w: &World, e: Entity) -> Option<&Crate> {
    w.get::<Crate>(e)
}

fn watch(w: &World) -> &CrateWatch {
    w.resource::<CrateWatch>()
}

/// #80: a test crate on the cabin floor through take-off, flight, warp and landing; a second one
/// pushed out over the ramp of the flying ship.
pub fn crate_ride_steps(s: &mut Vec<Step>, dir: &std::path::Path, windowed: bool) {
    s.push(Box::new(|w, c| {
        clear_crates(w);
        let e = cabin_crate(w, "small", 1.2, 1.5);
        let start = w.get::<Crate>(e).unwrap().body.pos;
        w.insert_resource(CrateWatch { e, start, max_drift: 0.0, left_cabin: false, ticks: 0 });
        begin(w, c, "crate-ride: test crate on the cabin floor");
        true
    }));
    s.push(wait(1.5));
    s.push(Box::new(|w, c| {
        let e = watch(w).e;
        let b = crate_of(w, e).unwrap().body.clone();
        let stats = w.resource::<CargoStats>();
        let note = format!("asleep {}, grounded {}, drift {:.1} mm, steps {} asleep-steps {}", b.asleep, b.grounded, watch(w).max_drift * 1000.0, stats.steps, stats.asleep);
        end(w, c, note);
        check(c, b.asleep && watch(w).max_drift < 0.01, format!("crate-ride: the crate rests and sleeps on the floor (drift {:.1} mm < 10 mm)", watch(w).max_drift * 1000.0));
        true
    }));
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(hold_until("crate-ride: take off, climb to 300 m above ground", &[KeyCode::Space, KeyCode::ShiftLeft], 120.0, |w| above_ground(w) > 300.0));
    s.push(hold_until("crate-ride: firm brake", &[KeyCode::KeyX], 20.0, |w| ship_vel(w).length() < 0.5));
    // Push a second crate out over the ramp while the ship flies forward.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "crate-ride: push a crate out over the ramp in flight");
            let e = cabin_crate(w, "small", 0.0, 3.4);
            c.v.insert("pushed", e.to_bits() as f64);
            c.v.insert("n0", w.resource::<CargoStats>().handovers.len() as f64);
            keys(w, &[KeyCode::KeyW], true);
        }
        let e = Entity::from_bits(c.v["pushed"] as u64);
        if c.t > 0.8 {
            keys(w, &[KeyCode::KeyW], false);
            // Shove it backwards, harder than friction (test hook: what a hand will do in #83).
            if let Some(mut cr) = w.get_mut::<Crate>(e) {
                cr.push = DVec3::new(0.0, 0.0, 8.0);
            }
        }
        let hs = &w.resource::<CargoStats>().handovers[c.v["n0"] as usize..];
        if let Some(h) = hs.iter().find(|h| h.crate_e == e && h.out).copied() {
            let jump = (h.after - h.before).length();
            let rel = (h.after - h.ship_vel).length();
            end(w, c, format!("hand-over at ship speed {:.2} m/s: world velocity before {:.3?}, after {:.3?}, crate relative to the ship {rel:.2} m/s", h.ship_vel.length(), h.before, h.after));
            check(c, jump < 1e-6 && rel < 5.0 && h.ship_vel.length() > 1.0, format!("crate-ride: the crate keeps the ship velocity at the hand-over (jump {jump:.2e} m/s, ship {:.2} m/s, relative {rel:.2} m/s)", h.ship_vel.length()));
            return true;
        }
        if c.t > 10.0 {
            check(c, false, "crate-ride: the pushed crate left the cabin within 10 s".into());
            keys(w, &[KeyCode::KeyW], false);
            return true;
        }
        false
    }));
    s.push(hold_until("crate-ride: firm brake", &[KeyCode::KeyX], 20.0, |w| ship_vel(w).length() < 0.5));
    s.extend(fly_to_space_and_back());
    s.push(warp_flight("crate-ride: warp Hearth -> Cinder", "crate-ride", Some(HEARTH), CINDER, Flight::Seated, dir.to_path_buf(), windowed));
    s.push(wait_drive_idle());
    s.push(Box::new(|w, c| {
        let held = w.resource::<CargoStats>().held;
        check(c, held > 0, format!("crate-ride: the crate was held to the ship during the warp ({held} crate steps)"));
        // Above Cinder's ground, level, for the landing (test hook like the warp scenario's).
        let pl = planet(w);
        let up = DVec3::Y;
        teleport_ship(w, pl.centre + up * (pl.surface(up) + 40.0), basis_for_up(up));
        true
    }));
    s.push(wait(1.0));
    s.push(land("crate-ride: land on Cinder"));
    s.push(wait(2.0));
    s.push(Box::new(|w, c| {
        let wt = watch(w);
        let (e, drift, left, ticks) = (wt.e, wt.max_drift, wt.left_cabin, wt.ticks);
        let ship = ship_e(w);
        let b = crate_of(w, e).map(|c| (c.ship, c.body.pos));
        let inside = b.is_some_and(|(s, p)| s == Some(ship) && cabin_contains(p, 0.0));
        let (v, agl) = (ship_vel(w).length(), above_ground(w));
        check(c, v < 0.1 && agl < 1.0, format!("crate-ride: landed on Cinder ({v:.2} m/s, {agl:.2} m above ground)"));
        check(c, inside && !left && drift < 0.3, format!("crate-ride: the test crate stayed in the cabin through take-off, flight, warp and landing (largest drift {drift:.3} m < 0.3 m over {ticks} ticks, ever outside: {left})"));
        true
    }));
}

/// Test hook: put the walker at rest in the own cabin at `pos` (ship space, feet).
pub(crate) fn walker_in_cabin(w: &mut World, pos: DVec3) {
    put_at_seat(w);
    with_player(w, |p| {
        p.w.pos = pos;
        p.w.halt();
    });
}

/// Turn the walker to look at a world point (heading and pitch).
pub(crate) fn look_at(w: &mut World, target: DVec3) {
    let f = ship_frame_of(w);
    with_player(w, |p| {
        let eye = p.world_pos(f) + p.world_up(f) * crate::walker::EYE_HEIGHT;
        let up = p.world_up(f);
        let (fwd, pitch) = walker_core::split_look(target - eye, up, p.w.forward);
        p.w.forward = if p.ship.is_some() { f.rot.inverse() * fwd } else { fwd };
        p.pitch = pitch;
    });
}

pub(crate) fn crate_world_pos(w: &mut World, e: Entity) -> DVec3 {
    let f = ship_frame_of(w);
    crate::cargo::crate_world(w.get::<Crate>(e).unwrap(), &f).0
}

fn prompt(w: &World) -> String {
    w.resource::<crate::interact::Interaction>().prompt.clone()
}

fn held(w: &World) -> Option<Entity> {
    w.resource::<crate::grab::Grab>().held.map(|h| h.crate_e)
}

/// #82: one tap (F) for a crate and for the seat; the HUD prompt says which.
pub fn interact_steps(s: &mut Vec<Step>) {
    use crate::interact::Target;
    s.push(Box::new(|w, c| {
        clear_crates(w);
        let e = cabin_crate(w, "small", 1.2, 1.5);
        c.v.insert("crate", e.to_bits() as f64);
        walker_in_cabin(w, DVec3::new(1.2, 0.32, 0.0));
        let at = crate_world_pos(w, e);
        look_at(w, at);
        begin(w, c, "interact: crate and seat with one tap");
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let e = Entity::from_bits(c.v["crate"] as u64);
        let p = prompt(w);
        let t = w.resource::<crate::interact::Interaction>().target;
        check(c, p == "[F] pick up the small crate" && t == Some(Target::Crate(e, grab_core::Reach::Hands)), format!("interact: the HUD prompt offers the crate: \"{p}\""));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let e = Entity::from_bits(c.v["crate"] as u64);
        let p = prompt(w);
        check(c, held(w) == Some(e) && p == "[F] set the small crate down  [R] throw", format!("interact: F picked the crate up, prompt now \"{p}\""));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        check(c, held(w).is_none(), "interact: F again set it down".into());
        let f = ship_frame_of(w);
        look_at(w, f.to_world(crate::ship::SEAT_POS));
        keys(w, &[KeyCode::KeyW], true);
        true
    }));
    s.push(Box::new(|w, c| {
        let near = with_player(w, |p| p.w.pos.distance(crate::ship::SEAT_POS) < 1.2);
        if near || c.t > 5.0 {
            keys(w, &[KeyCode::KeyW], false);
            return true;
        }
        false
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let p = prompt(w);
        check(c, p == "[F] sit", format!("interact: at the seat the prompt is \"{p}\""));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let seated = with_player(w, |p| p.seated);
        let p = prompt(w);
        let used: Vec<Target> = w.resource::<crate::interact::Interaction>().used.clone();
        let both = used.iter().any(|t| matches!(t, Target::Crate(..))) && used.contains(&Target::Seat);
        check(c, seated && p == "[F] stand up", format!("interact: the same F sat down, prompt now \"{p}\""));
        check(c, both, format!("interact: one tap served the crate and the seat ({used:?})"));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let seated = with_player(w, |p| p.seated);
        end(w, c, format!("seated after the last F: {seated}"));
        check(c, !seated, "interact: F stood up again".into());
        true
    }));
}
