//! Scenarios of milestone C (crates, grab, lock grid, budget). They drive the game through
//! `Controls` like the others; crates are placed by test hooks.
use crate::cargo::{cabin_floor_pos, crate_bundle, CargoStats, Crate, CrateWatch, Crates};
use super::*;
use crate::ship::{basis_for_up, cabin_contains};
use bevy::math::DVec3;

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

/// Starts watching these crates (drift from where they are now, ever outside the cabin).
pub(crate) fn watch_crates(w: &mut World, es: &[Entity]) {
    let crates = es
        .iter()
        .map(|&e| crate::cargo::Watched { e, start: w.get::<Crate>(e).unwrap().body.pos, max_drift: 0.0, left_cabin: false })
        .collect();
    w.insert_resource(CrateWatch { crates, ticks: 0 });
}

/// #80: a test crate on the cabin floor through take-off, flight, warp and landing; a second one
/// pushed out over the ramp of the flying ship.
pub fn crate_ride_steps(s: &mut Vec<Step>, dir: &std::path::Path, windowed: bool) {
    s.push(Box::new(|w, c| {
        clear_crates(w);
        let e = cabin_crate(w, "small", 1.2, 1.5);
        watch_crates(w, &[e]);
        c.v.insert("watched", e.to_bits() as f64);
        begin(w, c, "crate-ride: test crate on the cabin floor");
        true
    }));
    s.push(wait(1.5));
    s.push(Box::new(|w, c| {
        let e = Entity::from_bits(c.v["watched"] as u64);
        let cr = crate_of(w, e).unwrap();
        let (asleep, locked) = (cr.body.asleep || cr.locked, cr.locked);
        let drift = watch(w).get(e).max_drift;
        let stats = w.resource::<CargoStats>();
        let note = format!("at rest {asleep}, locked {locked}, drift {:.1} mm, steps {} asleep-steps {}", drift * 1000.0, stats.steps, stats.asleep);
        end(w, c, note);
        // It rests on the plates, so it also locks and snaps to them (#84): up to half a plate
        // along each axis, 0.354 m on the diagonal.
        check(c, asleep && drift < 0.36, format!("crate-ride: the crate comes to rest on the floor (drift {:.1} mm, snapped to the plates: {locked})", drift * 1000.0));
        // Reset the watch: from here on the crate must not move.
        watch_crates(w, &[e]);
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
            // Keep flying forward (the ship's acceleration pushes loose crates back too, #84).
            // Shove it backwards, harder than friction (test hook: what a hand will do in #83).
            if let Some(mut cr) = w.get_mut::<Crate>(e) {
                cr.push = DVec3::new(0.0, 0.0, 8.0);
            }
        }
        let hs = &w.resource::<CargoStats>().handovers[c.v["n0"] as usize..];
        if let Some(h) = hs.iter().find(|h| h.crate_e == e && h.out).copied() {
            keys(w, &[KeyCode::KeyW], false);
            let jump = (h.after - h.before).length();
            let rel = (h.after - h.ship_vel).length();
            end(w, c, format!("hand-over at ship speed {:.2} m/s: world velocity before {:.3?}, after {:.3?}, crate relative to the ship {rel:.2} m/s", h.ship_vel.length(), h.before, h.after));
            // Relative: the 8 m/s push plus what the ship's acceleration took from under the crate
            // (the axis model pulls away harder than the classic one did: 11.2 m/s, #144).
            check(c, jump < 1e-6 && rel < 15.0 && h.ship_vel.length() > 1.0, format!("crate-ride: the crate keeps the ship velocity at the hand-over (jump {jump:.2e} m/s, ship {:.2} m/s, relative {rel:.2} m/s)", h.ship_vel.length()));
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
        let e = Entity::from_bits(c.v["watched"] as u64);
        let x = watch(w).get(e);
        let (drift, left, ticks) = (x.max_drift, x.left_cabin, watch(w).ticks);
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
        let t = w.resource::<crate::interact::Interaction>().target.clone();
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

/// Spawns a crate resting on the planet's ground at the world point `at` (planet frame).
pub(crate) fn ground_crate(w: &mut World, size: &str, at: DVec3) -> Entity {
    let pl = planet(w);
    let t = w.resource::<Crates>().0.clone();
    let up = pl.up(at);
    let h = t.get(size).expect("crate size").extents[1] * 0.5;
    let pos = pl.centre + up * (pl.surface(up) + h + 0.01);
    let fwd = up.any_orthonormal_vector();
    w.spawn(crate_bundle(&t, size, None, pos, fwd)).id()
}

fn crate_e(c: &Ctx, k: &'static str) -> Entity {
    Entity::from_bits(c.v[k] as u64)
}

/// Height of a crate's bottom above the planet's ground (m).
fn crate_above_ground(w: &mut World, e: Entity) -> f64 {
    let p = crate_world_pos(w, e);
    let half = w.get::<Crate>(e).unwrap().body.half.y;
    planet(w).above_ground(p) - half
}

/// Walker on open ground 25 m behind the parked ship, facing away from it, at rest.
fn walker_on_ground(w: &mut World) {
    let f = ship_frame_of(w);
    place_walker(w, f.to_world(DVec3::new(8.0, 0.0, 25.0)));
    let p = player_world(w);
    face_towards(w, p + (p - f.origin));
}

/// A point `d` m ahead of the walker on its heading, world space.
fn ahead(w: &mut World, d: f64) -> DVec3 {
    let f = ship_frame_of(w);
    let (p, fwd) = with_player(w, |pl| (pl.world_pos(f), if pl.ship.is_some() { f.rot * pl.w.forward } else { pl.w.forward }));
    p + fwd * d
}

/// Picks up a fresh `size` crate 1.3 m ahead, then walks 3 s holding W (and Shift); checks the
/// walking speed of this carry state and that the crate stays in the hands.
fn carry_walk(size: &'static str, want_speed: fn(&walker_core::WalkerConfig, &grab_core::GrabConfig) -> f64, try_jump: bool) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            clear_crates(w);
            walker_on_ground(w);
            let at = ahead(w, 1.3);
            let e = ground_crate(w, size, at);
            c.v.insert("crate", e.to_bits() as f64);
            begin(w, c, &format!("crate-carry: pick up and carry the {size} crate"));
            true
        }),
        wait(0.6),
        Box::new(|w, c| {
            let e = crate_e(c, "crate");
            let at = crate_world_pos(w, e);
            look_at(w, at);
            true
        }),
        wait(0.1),
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }),
        wait(1.0),
        // Look ahead before checking the lift: the hold follows the view, and looking down at a
        // crate on flat ground keeps a heavy one just off the floor (it passed on a 20 degree
        // slope; the drainage made the spawn flat, #72).
        Box::new(|w, _| {
            with_player(w, |p| p.pitch = 0.0);
            true
        }),
        wait(1.0),
        Box::new(move |w, c| {
            let e = crate_e(c, "crate");
            let lifted = crate_above_ground(w, e);
            check(c, held(w) == Some(e) && lifted > 0.15, format!("crate-carry: {size} crate held and lifted ({lifted:.2} m above ground)"));
            keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], true);
            c.v.insert("max_air", 0.0);
            true
        }),
        Box::new(move |w, c| {
            let p = player_world(w);
            if c.t > 1.5 && !c.p.contains_key("from") {
                c.p.insert("from", p);
                c.v.insert("t_from", c.t);
            }
            if try_jump && c.t > 2.0 && c.t < 2.4 {
                keys(w, &[KeyCode::Space], true);
            }
            if try_jump && c.t >= 2.4 {
                keys(w, &[KeyCode::Space], false);
            }
            let air = planet(w).above_ground(p);
            c.v.insert("max_air", c.v["max_air"].max(air));
            if c.t >= 3.0 {
                keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft, KeyCode::Space], false);
                let from = c.p.remove("from").unwrap();
                let up = planet(w).up(p);
                let d = p - from;
                let speed = (d - up * d.dot(up)).length() / (c.t - c.v["t_from"]);
                let want = want_speed(&w.resource::<crate::tuning::Tuning>().walker, &w.resource::<crate::tuning::Tuning>().grab);
                let e = crate_e(c, "crate");
                let still = held(w) == Some(e);
                end(w, c, format!("{size}: walking {speed:.2} m/s (want {want:.2}), crate still held {still}, highest feet {:.2} m", c.v["max_air"]));
                // The walker loses a little against its target on this ground (both states alike),
                // so the band is absolute; the states are 9 m/s apart.
                check(c, (speed - want).abs() < 1.0 && still, format!("crate-carry: carrying the {size} crate walks at {speed:.2} m/s (want {want:.2} +-1 m/s), crate still in the hands"));
                if try_jump {
                    check(c, c.v["max_air"] < 0.2, format!("crate-carry: no jump with both hands busy (feet at most {:.2} m above ground)", c.v["max_air"]));
                }
                tap(w, KeyCode::KeyF);
                return true;
            }
            false
        }),
        wait(0.3),
    ]
}

/// #83: hands and the grab tool.
pub fn crate_carry_steps(s: &mut Vec<Step>, dir: &std::path::Path, windowed: bool) {
    // One hand: no cost, sprint allowed.
    s.extend(carry_walk("small", |wc, _| wc.run_speed, false));
    // Two hands: slower, no sprint, no jump.
    s.extend(carry_walk("medium", |wc, gc| wc.walk_speed * gc.two_hand_speed_share, true));

    // The large crate does not lift with one holder.
    s.push(Box::new(|w, c| {
        clear_crates(w);
        walker_on_ground(w);
        let at = ahead(w, 1.9);
        let e = ground_crate(w, "large", at);
        c.v.insert("crate", e.to_bits() as f64);
        begin(w, c, "crate-carry: try to lift the large crate alone");
        true
    }));
    s.push(wait(0.6));
    s.push(Box::new(|w, c| {
        let e = crate_e(c, "crate");
        let at = crate_world_pos(w, e);
        look_at(w, at);
        c.v.insert("h0", crate_above_ground(w, e));
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        let p = prompt(w);
        check(c, p.contains("large crate"), format!("crate-carry: the prompt offers the large crate: \"{p}\""));
        tap(w, KeyCode::KeyF);
        c.v.insert("max_h", f64::MIN);
        true
    }));
    s.push(Box::new(|w, c| {
        let e = crate_e(c, "crate");
        let h = crate_above_ground(w, e);
        c.v.insert("max_h", c.v["max_h"].max(h));
        if c.t >= 2.5 {
            let rise = c.v["max_h"] - c.v["h0"];
            end(w, c, format!("large crate: highest bottom {:.3} m above ground (start {:.3} m), still held {}", c.v["max_h"], c.v["h0"], held(w).is_some()));
            check(c, rise < 0.1, format!("crate-carry: one holder cannot lift the large crate (rose {rise:.3} m)"));
            if held(w).is_some() {
                tap(w, KeyCode::KeyF);
            }
            return true;
        }
        false
    }));
    s.push(wait(0.3));

    // Throw the small crate.
    s.push(Box::new(|w, c| {
        clear_crates(w);
        walker_on_ground(w);
        let at = ahead(w, 1.3);
        let e = ground_crate(w, "small", at);
        c.v.insert("crate", e.to_bits() as f64);
        begin(w, c, "crate-carry: throw the small crate");
        true
    }));
    s.push(wait(0.6));
    s.push(Box::new(|w, c| {
        let e = crate_e(c, "crate");
        let at = crate_world_pos(w, e);
        look_at(w, at);
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        // Look 25 degrees up and throw.
        with_player(w, |p| p.pitch = 25f64.to_radians());
        let e = crate_e(c, "crate");
        c.p.insert("from", crate_world_pos(w, e));
        c.v.insert("n0", w.resource::<crate::grab::Grab>().throws.len() as f64);
        tap(w, KeyCode::KeyR);
        true
    }));
    s.push(Box::new(|w, c| {
        let e = crate_e(c, "crate");
        let pos = crate_world_pos(w, e);
        let (asleep, vel) = w.get::<Crate>(e).map(|c| (c.body.asleep, c.body.vel)).unwrap();
        if c.t > 0.3 && (asleep || vel.length() < 0.05) || c.t > 5.0 {
            let throws = w.resource::<crate::grab::Grab>().throws.clone();
            let thrown = throws.get(c.v["n0"] as usize).copied();
            let up = planet(w).up(pos);
            let d = pos - c.p["from"];
            let range = (d - up * d.dot(up)).length();
            let speed = thrown.map_or(0.0, |(_, v)| v.length());
            let cfg = w.resource::<crate::tuning::Tuning>().grab;
            let want = (cfg.throw_impulse / 15.0).min(cfg.throw_max_speed);
            let below = -crate_above_ground(w, e).min(0.0);
            end(w, c, format!("thrown at {speed:.2} m/s, landed {range:.2} m away after {:.2} s, below ground {below:.3} m", c.t));
            check(c, thrown.is_some_and(|(x, _)| x == e) && (speed - want).abs() < 0.5 && held(w).is_none(), format!("crate-carry: R throws the small crate at {speed:.2} m/s (want {want:.2})"));
            check(c, (2.0..12.0).contains(&range) && below < 0.05, format!("crate-carry: the thrown crate flew {range:.2} m and came to rest on the ground"));
            return true;
        }
        false
    }));

    // The grab tool pulls a crate from 8 m.
    s.push(Box::new(|w, c| {
        clear_crates(w);
        walker_on_ground(w);
        let at = ahead(w, 8.0);
        let e = ground_crate(w, "medium", at);
        c.v.insert("crate", e.to_bits() as f64);
        begin(w, c, "crate-carry: pull the medium crate with the grab tool from 8 m");
        true
    }));
    s.push(wait(0.6));
    s.push(Box::new(|w, c| {
        let e = crate_e(c, "crate");
        let at = crate_world_pos(w, e);
        look_at(w, at);
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        let p = prompt(w);
        let e = crate_e(c, "crate");
        let f = ship_frame_of(w);
        let eye = with_player(w, |pl| pl.world_pos(f) + pl.world_up(f) * crate::walker::EYE_HEIGHT);
        c.v.insert("d0", crate_world_pos(w, e).distance(eye));
        check(c, p == "[F] pull the medium crate (grab tool)", format!("crate-carry: from {:.1} m the prompt offers the tool: \"{p}\"", c.v["d0"]));
        tap(w, KeyCode::KeyF);
        true
    }));
    let dir = dir.join("crate-carry");
    s.push(Box::new(move |w, c| {
        // Windowed: the grab tool's beam mid-pull (night extra E3).
        if c.t >= 1.5 && !c.v.contains_key("beam_shot") {
            c.v.insert("beam_shot", 1.0);
            shot(w, c, &dir, windowed, "grab-beam");
        }
        if c.t < 4.0 {
            return false;
        }
        let e = crate_e(c, "crate");
        let f = ship_frame_of(w);
        let eye = with_player(w, |pl| pl.world_pos(f) + pl.world_up(f) * crate::walker::EYE_HEIGHT);
        let d = crate_world_pos(w, e).distance(eye);
        let want = w.resource::<crate::tuning::Tuning>().grab.tool_hold_distance;
        let h = crate_above_ground(w, e);
        end(w, c, format!("tool: crate from {:.2} m to {d:.2} m from the eye in 4 s, {h:.2} m above ground", c.v["d0"]));
        check(c, held(w) == Some(e) && (d - want).abs() < 0.6 && h > 0.2, format!("crate-carry: the tool pulled the crate from {:.1} m to {d:.2} m (hold distance {want} m) and holds it up", c.v["d0"]));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.3));
}

fn plates(w: &World) -> (usize, usize) {
    let g = w.resource::<crate::cargo::LockGrid>();
    (g.count(crate::cargo::Plate::Lit), g.count(crate::cargo::Plate::Blocked))
}

/// #84: one crate locked on the plates, one loose off them, one partly on (red). Hard
/// acceleration and a warp: the locked one does not move, the loose one slides and stays in the
/// cabin; grabbing unlocks.
pub fn crate_lock_steps(s: &mut Vec<Step>, dir: &std::path::Path, windowed: bool) {
    s.push(Box::new(|w, c| {
        clear_crates(w);
        // On the plates (locks), off them in front (loose), half on the right edge (blocked).
        let a = cabin_crate(w, "small", -0.95, 1.1);
        let b = cabin_crate(w, "small", 1.2, -2.0);
        let r = cabin_crate(w, "small", 1.6, 0.25);
        for (k, e) in [("a", a), ("b", b), ("r", r)] {
            c.v.insert(k, e.to_bits() as f64);
        }
        begin(w, c, "crate-lock: three crates set down in the cabin");
        true
    }));
    s.push(wait(1.5));
    s.push(Box::new(|w, c| {
        let [a, b, r] = ["a", "b", "r"].map(|k| crate_e(c, k));
        let locked = |w: &World, e| crate_of(w, e).unwrap().locked;
        let (lit, red) = plates(w);
        let pa = crate_of(w, a).unwrap().body.pos;
        end(w, c, format!("locked: a {}, b {}, r {}; plates lit {lit}, red {red}; a snapped to ({:.2}, {:.2})", locked(w, a), locked(w, b), locked(w, r), pa.x, pa.z));
        check(c, locked(w, a) && !locked(w, b) && !locked(w, r), "crate-lock: the crate fully on the plates locks, the others do not".into());
        check(c, lit == 1 && red >= 1, format!("crate-lock: one plate lit under the locked crate ({lit}), red under the one half on the grid ({red})"));
        watch_crates(w, &[a, b]);
        true
    }));
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(hold_until("crate-lock: take off, climb to 300 m above ground", &[KeyCode::Space, KeyCode::ShiftLeft], 120.0, |w| above_ground(w) > 300.0));
    s.push(hold_until("crate-lock: hard acceleration (boost forward)", &[KeyCode::KeyW, KeyCode::ShiftLeft], 6.0, |_| false));
    s.push(hold_until("crate-lock: firm brake", &[KeyCode::KeyX], 20.0, |w| ship_vel(w).length() < 0.5));
    s.push(hold_until("crate-lock: hard strafe right", &[KeyCode::KeyD, KeyCode::ShiftLeft], 3.0, |_| false));
    s.push(hold_until("crate-lock: firm brake", &[KeyCode::KeyX], 20.0, |w| ship_vel(w).length() < 0.5));
    s.push(Box::new(|w, c| {
        let [a, b] = ["a", "b"].map(|k| crate_e(c, k));
        let (xa, xb) = (watch(w).get(a), watch(w).get(b));
        let stops = w.resource::<CargoStats>().field_stops;
        check(c, xa.max_drift == 0.0, format!("crate-lock: the locked crate did not move under hard acceleration (drift {:.4} m)", xa.max_drift));
        check(c, xb.max_drift > 0.3 && !xb.left_cabin, format!("crate-lock: the loose crate slid {:.2} m and stayed in the cabin (ramp field stops so far: {stops})", xb.max_drift));
        true
    }));
    s.extend(fly_to_space_and_back());
    s.push(warp_flight("crate-lock: warp Hearth -> Cinder", "crate-lock", Some(HEARTH), CINDER, Flight::Seated, dir.to_path_buf(), windowed));
    s.push(wait_drive_idle());
    s.push(Box::new(|w, c| {
        let [a, b] = ["a", "b"].map(|k| crate_e(c, k));
        let (xa, xb) = (watch(w).get(a), watch(w).get(b));
        let ship = ship_e(w);
        let inside = |w: &World, e| crate_of(w, e).is_some_and(|c| c.ship == Some(ship));
        check(c, xa.max_drift == 0.0 && crate_of(w, a).unwrap().locked, format!("crate-lock: after the warp the locked crate is still locked and has not moved ({:.4} m)", xa.max_drift));
        let catches = w.resource::<CargoStats>().wall_catches;
        check(c, !xb.left_cabin && inside(w, b), format!("crate-lock: after the warp the loose crate is still in the cabin (largest drift {:.2} m, cabin safety net caught it {catches} times)", xb.max_drift));
        // Down on Cinder for the grab: ship level on the ground.
        let pl = planet(w);
        let up = DVec3::Y;
        teleport_ship(w, pl.centre + up * (pl.surface(up) + 40.0), crate::ship::basis_for_up(up));
        true
    }));
    s.push(wait(1.0));
    s.push(land("crate-lock: land on Cinder"));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let a = crate_e(c, "a");
        let pa = crate_of(w, a).unwrap().body.pos;
        walker_in_cabin(w, DVec3::new(pa.x, 0.32, pa.z - 1.4));
        let at = crate_world_pos(w, a);
        look_at(w, at);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let a = crate_e(c, "a");
        let locked = crate_of(w, a).unwrap().locked;
        let (lit, _) = plates(w);
        check(c, held(w) == Some(a) && !locked && lit == 0, format!("crate-lock: grabbing unlocks the crate (held {}, locked {locked}, plates lit {lit})", held(w) == Some(a)));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(2.0));
    s.push(Box::new(|w, c| {
        let a = crate_e(c, "a");
        let locked = crate_of(w, a).unwrap().locked;
        end(w, c, format!("set down again: locked {locked}"));
        check(c, locked, "crate-lock: set down on the plates again, it locks again".into());
        true
    }));
}

fn crate_count(w: &mut World) -> usize {
    w.query::<&Crate>().iter(w).count()
}

fn budget_row(w: &World) -> grab_core::budget::BudgetRow {
    w.resource::<crate::cargo::ObjectBudget>().0.crates
}

/// Spawns `n` small crates on the ground in rows 6 m ahead of the walker, one per tick from
/// `c.v["spawned"]` on; true when all are out.
fn spawn_rows(w: &mut World, c: &mut Ctx, n: usize) -> bool {
    let i = c.v.get("spawned").copied().unwrap_or(0.0) as usize;
    if i >= n {
        return true;
    }
    let f = ship_frame_of(w);
    let (p, fwd) = with_player(w, |pl| (pl.world_pos(f), pl.w.forward));
    let up = planet(w).up(p);
    let side = fwd.cross(up);
    let at = p + fwd * (6.0 + (i / 6) as f64 * 1.2) + side * ((i % 6) as f64 * 1.2 - 3.0);
    let e = ground_crate(w, "small", at);
    c.v.insert("spawned", (i + 1) as f64);
    if i == 0 {
        c.v.insert("first", e.to_bits() as f64);
    }
    c.v.insert("last", e.to_bits() as f64);
    false
}

/// #85: past the cap, sleeping at rest, a planet swap keeps the persistence cap, the timeout and
/// the distance rule.
pub fn crate_budget_steps(s: &mut Vec<Step>, dir: &std::path::Path, windowed: bool) {
    s.push(Box::new(|w, c| {
        clear_crates(w);
        walker_on_ground(w);
        c.v.remove("spawned");
        begin(w, c, "crate-budget: spawn 30 crates, cap 24");
        true
    }));
    s.push(Box::new(|w, c| {
        let n = budget_row(w).cap + 6;
        spawn_rows(w, c, n)
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        let cap = budget_row(w).cap;
        let n = crate_count(w);
        let (first, last) = (crate_e(c, "first"), crate_e(c, "last"));
        let (first_alive, last_alive) = (w.get::<Crate>(first).is_some(), w.get::<Crate>(last).is_some());
        let despawned = w.resource::<CargoStats>().despawned;
        end(w, c, format!("{n} crates alive after spawning {}, {despawned} removed; first spawned alive {first_alive}, last {last_alive}", cap + 6));
        check(c, n == cap && !first_alive && last_alive, format!("crate-budget: spawning past the cap keeps {n} crates (cap {cap}), the longest untouched went first"));
        true
    }));
    s.push(wait(2.0));
    s.push(Box::new(|w, c| {
        let asleep = w.query::<&Crate>().iter(w).filter(|c| c.body.asleep).count();
        let n = crate_count(w);
        c.v.insert("steps0", w.resource::<CargoStats>().steps as f64);
        check(c, asleep == n, format!("crate-budget: all {n} crates at rest sleep ({asleep})"));
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let steps = w.resource::<CargoStats>().steps as f64 - c.v["steps0"];
        check(c, steps == 0.0, format!("crate-budget: sleeping crates cost no steps ({steps} crate steps in 0.5 s)"));
        true
    }));
    // Leave them on Hearth: warp to Cinder.
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(warp_flight("crate-budget: warp Hearth -> Cinder", "crate-budget", Some(HEARTH), CINDER, Flight::Seated, dir.to_path_buf(), windowed));
    s.push(wait_drive_idle());
    s.push(Box::new(|w, c| {
        let n = crate_count(w);
        let keep = budget_row(w).persistence_cap;
        let frozen = w.resource::<CargoStats>().frozen;
        let on_hearth = w.query::<&Crate>().iter(w).filter(|c| c.planet == Some(HEARTH)).count();
        check(c, n == keep && on_hearth == keep && frozen > 0, format!("crate-budget: after the swap {n} crates are left on Hearth (persistence cap {keep}), frozen ({frozen} skipped steps)"));
        // Down on Cinder, walker outside.
        let pl = planet(w);
        let up = DVec3::Y;
        teleport_ship(w, pl.centre + up * (pl.surface(up) + 40.0), crate::ship::basis_for_up(up));
        true
    }));
    s.push(wait(1.0));
    s.push(land("crate-budget: land on Cinder"));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        walker_on_ground(w);
        c.v.remove("spawned");
        // Test hook: a short timeout.
        w.resource_mut::<crate::cargo::ObjectBudget>().0.crates.timeout_s = 2.0;
        begin(w, c, "crate-budget: timeout on Cinder");
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| spawn_rows(w, c, 3)));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let n = crate_count(w);
        check(c, n == budget_row(w).persistence_cap + 3, format!("crate-budget: three fresh crates on Cinder before the timeout ({n} in all)"));
        true
    }));
    s.push(wait(2.5));
    s.push(Box::new(|w, c| {
        let n = crate_count(w);
        let on_cinder = w.query::<&Crate>().iter(w).filter(|c| c.planet == Some(CINDER)).count();
        end(w, c, format!("{n} crates left, {on_cinder} on Cinder"));
        check(c, on_cinder == 0 && n == budget_row(w).persistence_cap, format!("crate-budget: untouched crates resting on Cinder went after the timeout; the {n} frozen on Hearth stay"));
        w.resource_mut::<crate::cargo::ObjectBudget>().0 = crate::cargo::ObjectBudget::default().0;
        true
    }));
    // A crate falling far above every player and ship goes (distance rule for moving crates).
    s.push(Box::new(|w, c| {
        let p = player_world(w);
        let up = planet(w).up(p);
        let e = ground_crate(w, "small", p);
        let far = budget_row(w).far_m + 500.0;
        w.get_mut::<Crate>(e).unwrap().body.pos = p + up * far;
        c.v.insert("far", e.to_bits() as f64);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let gone = w.get::<Crate>(crate_e(c, "far")).is_none();
        check(c, gone, format!("crate-budget: a crate falling {:.0} m above the walker went", budget_row(w).far_m + 500.0));
        true
    }));
}

/// Walk with keys held for `secs`, then stop (heading as set before).
fn walk_for(keys_held: &'static [KeyCode], secs: f64) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            keys(w, keys_held, true);
        }
        if c.t >= secs {
            keys(w, keys_held, false);
            return true;
        }
        false
    })
}

/// E1 (night extras): unload the parked ship by hand and load it again: a crate from the plates
/// down the ramp onto the ground, set down there, picked up again and carried back up onto the
/// plates, where it locks. Once with the small crate, once with the medium one (two hands).
pub fn crate_unload_steps(s: &mut Vec<Step>) {
    for size in ["small", "medium"] {
        s.push(Box::new(move |w, c| {
            clear_crates(w);
            let e = cabin_crate(w, size, 0.0, 1.5);
            c.v.insert("crate", e.to_bits() as f64);
            walker_in_cabin(w, DVec3::new(0.0, 0.32, -0.6));
            begin(w, c, &format!("crate-unload: {size} crate down the ramp and back"));
            true
        }));
        s.push(wait(1.2));
        s.push(Box::new(move |w, c| {
            let e = crate_e(c, "crate");
            check(c, crate_of(w, e).unwrap().locked, format!("crate-unload: the {size} crate is locked on the plates"));
            let at = crate_world_pos(w, e);
            look_at(w, at);
            true
        }));
        s.push(wait(0.1));
        s.push(Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }));
        s.push(wait(0.8));
        // Out the back: face the ramp, walk down it and on 6 m.
        s.push(Box::new(|w, _| {
            let f = ship_frame_of(w);
            look_at(w, f.to_world(DVec3::new(0.0, 0.0, 14.0)));
            with_player(w, |p| p.pitch = -0.3);
            true
        }));
        s.push(walk_for(&[KeyCode::KeyW], if size == "small" { 3.0 } else { 4.5 }));
        s.push(wait(0.8));
        s.push(Box::new(move |w, c| {
            let e = crate_e(c, "crate");
            let (outside, held_it) = (with_player(w, |p| p.ship.is_none()), held(w) == Some(e));
            let in_planet = crate_of(w, e).is_some_and(|c| c.ship.is_none());
            let f = ship_frame_of(w);
            let behind = f.to_local(crate_world_pos(w, e)).z;
            check(c, outside && held_it && in_planet && behind > 6.6, format!("crate-unload: carried the {size} crate down the ramp ({behind:.1} m behind the ship's centre, walker outside {outside}, still held {held_it}, crate on the planet {in_planet})"));
            tap(w, KeyCode::KeyF);
            true
        }));
        s.push(wait(1.5));
        s.push(Box::new(move |w, c| {
            let e = crate_e(c, "crate");
            let h = crate_above_ground(w, e);
            let asleep = crate_of(w, e).unwrap().body.asleep;
            check(c, h.abs() < 0.35 && asleep, format!("crate-unload: set down, the {size} crate rests on the ground ({h:.2} m above it, asleep {asleep})"));
            // Pick it up again, facing it.
            let at = crate_world_pos(w, e);
            look_at(w, at);
            true
        }));
        s.push(wait(0.1));
        s.push(Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }));
        s.push(wait(0.8));
        // Back up the ramp into the cabin, onto the plates.
        s.push(Box::new(|w, _| {
            let f = ship_frame_of(w);
            look_at(w, f.to_world(DVec3::new(0.0, 1.5, -1.5)));
            with_player(w, |p| p.pitch = -0.3);
            true
        }));
        s.push(Box::new(move |w, c| {
            if c.t == 0.0 {
                keys(w, &[KeyCode::KeyW], true);
            }
            // Stop once the walker stands well inside: the crate ahead is then over the plates.
            let z = with_player(w, |p| p.ship.is_some().then_some(p.w.pos.z));
            if z.is_some_and(|z| z < 3.2) || c.t > 8.0 {
                keys(w, &[KeyCode::KeyW], false);
                return true;
            }
            false
        }));
        s.push(wait(0.8));
        s.push(Box::new(|w, c| {
            let e = crate_e(c, "crate");
            let inside = crate_of(w, e).is_some_and(|c| c.ship.is_some());
            check(c, held(w) == Some(e) && inside, format!("crate-unload: carried back up the ramp into the cabin (held {}, crate in the cabin {inside})", held(w) == Some(e)));
            // Level look so the crate goes down in front, then let go.
            with_player(w, |p| p.pitch = -0.5);
            true
        }));
        s.push(wait(0.6));
        s.push(Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }));
        s.push(wait(2.0));
        s.push(Box::new(move |w, c| {
            let e = crate_e(c, "crate");
            let cr = crate_of(w, e).unwrap();
            let (locked, pos) = (cr.locked, cr.body.pos);
            let lit = plates(w).0;
            end(w, c, format!("{size} crate back at ({:.2}, {:.2}, {:.2}) ship space, locked {locked}, plates lit {lit}", pos.x, pos.y, pos.z));
            check(c, locked && lit > 0, format!("crate-unload: set down on the plates, the {size} crate locks again ({lit} plates lit)"));
            true
        }));
    }
}
