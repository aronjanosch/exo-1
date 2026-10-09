//! The walker in the cabin at speed and outside in space: stepping out, the suit, drifting behind a
//! moving ship, cabin gravity by hand.
use super::*;

/// Cabin at speed: stand, then walk, while the ship boosts and rolls with the assist off.
pub(super) fn cabin_at_speed(name: &'static str, secs: f64, assist: bool, roll: f64) -> Vec<Step> {
    let mut steps: Vec<Step> = vec![
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF); // stand up
            true
        }),
        wait(1.0),
        Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, name);
                let p = with_player(w, |p| p.w.pos);
                c.p.insert("start", p);
                c.v.insert("drift", 0.0);
                c.v.insert("ymin", f64::MAX);
                c.v.insert("ymax", f64::MIN);
                c.v.insert("left", 0.0);
                c.v.insert("vmax", 0.0);
                with_ship(w, |s| {
                    s.ctl.hover_assist = assist;
                    s.test_input = FlightInput { thrust: DVec3::new(0.0, 0.0, -1.0), boost: true, roll, ..default() };
                });
            }
            let e = ship_e(w);
            let (pos, inside, grounded) = with_player(w, |p| (p.w.pos, p.ship == Some(e), p.w.grounded));
            let half = secs * 0.5;
            // First half stand, then walk forward 1.5 s and back 1 s.
            let walk_t = c.t - half;
            keys(w, &[KeyCode::KeyW], (0.0..1.5).contains(&walk_t));
            keys(w, &[KeyCode::KeyS], (1.5..2.5).contains(&walk_t));
            if c.t < half {
                *c.v.get_mut("drift").unwrap() = c.v["drift"].max(pos.distance(c.p["start"]));
            }
            if inside {
                *c.v.get_mut("ymin").unwrap() = c.v["ymin"].min(pos.y);
                *c.v.get_mut("ymax").unwrap() = c.v["ymax"].max(pos.y);
            } else {
                c.v.insert("left", 1.0);
            }
            let _ = grounded;
            let v = ship_vel(w).length();
            *c.v.get_mut("vmax").unwrap() = c.v["vmax"].max(v);
            if v > 400.0 {
                // Keep rolling, stop boosting at 400 m/s.
                with_ship(w, |s| s.test_input.thrust = DVec3::ZERO);
            }
            if c.t >= secs {
                keys(w, &[KeyCode::KeyW, KeyCode::KeyS], false);
                with_ship(w, |s| {
                    s.test_input = FlightInput::default();
                    s.ctl.hover_assist = true;
                });
                let note = format!(
                    "ship up to {:.0} m/s, roll {roll} (assist {assist}), standing drift {:.4} m, walker height in cabin {:.4}..{:.4} m, left ship {}",
                    c.v["vmax"], c.v["drift"], c.v["ymin"], c.v["ymax"], c.v["left"] > 0.0
                );
                end(w, c, note);
                check(c, c.v["drift"] < 0.001 && c.v["left"] == 0.0 && c.v["ymax"] - c.v["ymin"] < 0.02,
                    format!("{name}: walker stays in the cabin at {:.0} m/s, drift {:.4} m", c.v["vmax"], c.v["drift"]));
                return true;
            }
            false
        }),
    ];
    steps.extend(back_to_seat());
    steps
}

/// Issue #5: stand up in space, walk out of the back and drift. Outside the field the walker
/// keeps the velocity it left the cabin with (ship velocity plus its walking speed) and nothing
/// pulls it. `drift` gives the ship a speed towards the planet first (stopped not exactly).
/// `careful`: W only in 0.1 s taps every 0.5 s, a careful step out: the walker leaves at step-off
/// speed (3 m/s) at most.
pub(super) fn step_out_in_space(name: &'static str, drift: f64, careful: bool) -> Vec<Step> {
    let mut steps: Vec<Step> = vec![
        Box::new(move |w, _| {
            tap(w, KeyCode::KeyF); // stand up
            if drift != 0.0 {
                with_ship(w, |s| s.ctl.hover_assist = false);
                let e = ship_e(w);
                let up = planet(w).up(ship_frame_of(w).origin);
                w.get_mut::<LinearVelocity>(e).unwrap().0 = -up * drift;
            }
            true
        }),
        wait(0.5),
        Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, name);
                c.v.remove("out_t");
                c.v.remove("rel_out");
                let f = ship_frame_of(w);
                face_towards(w, f.to_world(DVec3::new(0.0, 1.0, 12.0)));
                keys(w, &[KeyCode::KeyW], true);
            }
            track_look(w, c);
            let e = ship_e(w);
            let outside = with_player(w, |p| p.ship != Some(e));
            if careful && !outside {
                keys(w, &[KeyCode::KeyW], c.t % 0.5 < 0.1);
            }
            if outside && !c.v.contains_key("out_t") {
                keys(w, &[KeyCode::KeyW], false);
                c.v.insert("out_t", c.t);
            }
            if c.v.get("out_t").is_some_and(|t| c.t - t >= 0.2) && !c.v.contains_key("rel_out") {
                let v = with_player(w, |p| p.w.vel);
                c.p.insert("v_out", v);
                c.v.insert("rel_out", (v - ship_vel(w)).length());
            }
            let Some(&out_t) = c.v.get("out_t") else {
                if c.t >= 30.0 {
                    keys(w, &[KeyCode::KeyW], false);
                    end(w, c, "never left the cabin".into());
                    check(c, false, format!("{name}: walked out of the ship"));
                    return true;
                }
                return false;
            };
            if c.t - out_t >= 5.2 {
                let pl = planet(w);
                let p = player_world(w);
                let v = with_player(w, |p| p.w.vel);
                let dv = (v - c.p["v_out"]).length();
                let (rel, ship_radial) = (c.v["rel_out"], ship_vel(w).dot(pl.up(ship_frame_of(w).origin)));
                end(w, c, format!(
                    "{:.0} m from planet centre, gravity {:.3} m/s², ship radial {ship_radial:+.3} m/s, walker relative to ship after exit {rel:.3} m/s (walk speed 5), velocity change over 5 s drift {dv:.6} m/s",
                    (p - pl.centre).length(), pl.gravity_at(p).length()
                ));
                let ok = if careful { rel <= 3.01 } else { (rel - 5.0).abs() < 0.01 };
                check(c, ok && dv < 1e-6,
                    format!("{name}: walker keeps the ship's velocity plus its own and drifts ({rel:.3} m/s relative, change {dv:.6} m/s)"));
                // The ship's nose is 30 deg up: leaving keeps the cabin's orientation.
                check_steady(c, name);
                return true;
            }
            false
        }),
        Box::new(|w, _| {
            with_ship(w, |s| s.ctl.hover_assist = true);
            true
        }),
    ];
    steps.extend(back_to_seat());
    steps
}

/// Issue #8: the suit in space. Walk out of the stopped ship, brake to rest (X), roll (Q), turn
/// to the ship with the mouse and fly back into the cabin (W), all through `Controls`.
pub(super) fn suit_in_space() -> Vec<Step> {
    let mut steps: Vec<Step> = vec![
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF); // stand up
            true
        }),
        wait(0.5),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: walk out of the stopped ship");
                c.v.remove("out_t");
                let f = ship_frame_of(w);
                face_towards(w, f.to_world(DVec3::new(0.0, 1.0, 12.0)));
                keys(w, &[KeyCode::KeyW], true);
            }
            let e = ship_e(w);
            let (outside, suit) = with_player(w, |p| (p.ship != Some(e), p.body.is_some()));
            if outside && !c.v.contains_key("out_t") {
                c.v.insert("out_t", c.t);
            }
            // The suit takes over on the next step after leaving the cabin.
            if c.v.get("out_t").is_some_and(|t| c.t - t > 0.05) || c.t > 10.0 {
                keys(w, &[KeyCode::KeyW], false);
                end(w, c, format!("outside {outside}, suit mode {suit}"));
                check(c, outside && suit, "suit: outside the ship in space the suit takes over".into());
                return true;
            }
            false
        }),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: brake to rest (X)");
                keys(w, &[KeyCode::KeyX], true);
            }
            let v = with_player(w, |p| p.w.vel).length();
            if v < 0.01 || c.t > 6.0 {
                keys(w, &[KeyCode::KeyX], false);
                end(w, c, format!("{v:.4} m/s after {:.2} s", c.t));
                check(c, v < 0.01, format!("suit: X brakes to rest ({v:.4} m/s in {:.2} s)", c.t));
                return true;
            }
            false
        }),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: roll 1 s (Q)");
                let b = with_player(w, |p| p.body.unwrap_or_default());
                c.p.insert("look0", b * DVec3::NEG_Z);
                c.p.insert("head0", b * DVec3::Y);
                keys(w, &[KeyCode::KeyQ], true);
            }
            if c.t >= 1.0 {
                keys(w, &[KeyCode::KeyQ], false);
                let b = with_player(w, |p| p.body.unwrap_or_default());
                let turned = (b * DVec3::Y).angle_between(c.p["head0"]).to_degrees();
                let look = (b * DVec3::NEG_Z).angle_between(c.p["look0"]).to_degrees();
                end(w, c, format!("head turned {turned:.1} deg, look moved {look:.3} deg"));
                check(c, (turned - 1.5f64.to_degrees()).abs() < 3.0 && look < 0.01, format!("suit: Q rolls about the look axis ({turned:.1} deg in 1 s)"));
                return true;
            }
            false
        }),
        // Back into the field (test setup: put the walker 3000 m above the ground for a moment):
        // the suit hands over to walking, the look direction stays, gravity pulls, and the walker
        // rights itself from the rolled suit orientation without a step.
        Box::new(|w, c| {
            track_look(w, c);
            if c.t == 0.0 {
                begin(w, c, "suit: back in the planetary field (teleport to 3000 m)");
                let pl = planet(w);
                c.p.insert("look_before", world_look(w));
                let back = with_player(w, |p| p.w.pos);
                c.p.insert("back", back);
                let dir = pl.up(back);
                with_player(w, |p| {
                    p.w.pos = pl.centre + dir * (pl.surface(dir) + 3000.0);
                    p.w.halt();
                });
                return false;
            }
            if c.t >= 3.5 {
                let look = world_look(w).angle_between(c.p["look_before"]).to_degrees();
                let (suit, v, up) = with_player(w, |p| (p.body.is_some(), p.w.vel, p.view_up));
                let pl = planet(w);
                let fall = -v.dot(pl.up(player_world(w)));
                let tilt = up.angle_between(pl.up(player_world(w))).to_degrees();
                let step = c.v["up_jump"];
                let back = c.p["back"];
                with_player(w, |p| {
                    p.w.pos = back;
                    p.w.halt();
                });
                end(w, c, format!("suit {suit}, look moved {look:.4} deg, falling {fall:.2} m/s, up {tilt:.3} deg from the planet's, largest righting step {step:.2} deg after 3.5 s"));
                check(c, !suit && look < 0.01 && fall > 0.5, format!("suit: in the field the walker walks again, look kept ({look:.4} deg), falls ({fall:.2} m/s)"));
                check(c, tilt < 0.5 && step < 1.55, format!("suit: rights itself slowly in the field ({tilt:.3} deg left, at most {step:.2} deg per tick)"));
                return true;
            }
            false
        }),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: turn to the ship and fly back in (mouse, W)");
            }
            let e = ship_e(w);
            let f = ship_frame_of(w);
            let (inside, pos, b) = with_player(w, |p| (p.ship == Some(e), p.w.pos, p.body));
            if inside || c.t > 30.0 {
                keys(w, &[KeyCode::KeyW], false);
                end(w, c, format!("back in the cabin {inside} after {:.1} s", c.t));
                check(c, inside, "suit: flew back into the cabin".into());
                return true;
            }
            let Some(b) = b else { return false };
            // Aim at the middle of the cabin like a player with the mouse (rate proportional to
            // the error), thrust once roughly on target.
            let l = b.inverse() * (f.to_world(DVec3::new(0.0, 1.2, 0.0)) - pos).normalize();
            let yaw_err = (-l.x).atan2(-l.z);
            let pitch_err = l.y.atan2((l.x * l.x + l.z * l.z).sqrt());
            let sens = 0.0025;
            let mut m = w.resource_mut::<Controls>();
            m.mouse.x += (-(yaw_err * 0.2) / sens) as f32;
            m.mouse.y += (-(pitch_err * 0.2) / sens) as f32;
            keys(w, &[KeyCode::KeyW], yaw_err.abs() + pitch_err.abs() < 0.1);
            false
        }),
    ];
    steps.extend(back_to_seat());
    steps
}

/// Issue #9: the ship coasts at 20 m/s, the walker drifts with it in the suit 0.4 m behind the
/// end of its ramp. The ship's colliders sit one tick (0.33 m) behind its body; swept as if they
/// stood still they stopped the walker. It must drift on with the ship.
pub(super) fn drift_behind_moving_ship() -> Vec<Step> {
    let mut steps: Vec<Step> = vec![
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF); // stand up
            true
        }),
        wait(0.3),
        Box::new(|w, _| {
            with_ship(w, |s| s.ctl.hover_assist = false);
            let e = ship_e(w);
            let f = ship_frame_of(w);
            w.get_mut::<LinearVelocity>(e).unwrap().0 = f.rot * DVec3::new(0.0, 0.0, -20.0);
            true
        }),
        wait(0.5),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: drift with a ship coasting at 20 m/s, 0.4 m behind its ramp");
                let f = ship_frame_of(w);
                let v = ship_vel(w);
                with_player(w, |p| {
                    p.ship = None;
                    p.w.pos = f.to_world(DVec3::new(0.0, -1.2, 6.6 + 0.35 + 0.4));
                    p.w.vel = v;
                    p.w.move_vel = v;
                    p.body = Some(f.rot);
                    p.w.forward = f.rot * DVec3::NEG_Z;
                    p.pitch = 0.0;
                    p.view_up = f.rot * DVec3::Y;
                });
                c.v.insert("gap0", f.to_local(player_world(w)).z);
            }
            if c.t >= 2.0 {
                let f = ship_frame_of(w);
                let gap = f.to_local(player_world(w)).z;
                let dv = (with_player(w, |p| p.w.vel) - ship_vel(w)).length();
                let moved = gap - c.v["gap0"];
                end(w, c, format!("walker moved {moved:+.4} m against the ship in 2 s, velocity off the ship's by {dv:.4} m/s"));
                check(c, moved.abs() < 0.05 && dv < 0.01, format!("suit: drifts on with a moving ship it touches ({moved:+.4} m, {dv:.4} m/s)"));
                return true;
            }
            false
        }),
        Box::new(|w, _| {
            let e = ship_e(w);
            w.get_mut::<LinearVelocity>(e).unwrap().0 = DVec3::ZERO;
            with_ship(w, |s| s.ctl.hover_assist = true);
            true
        }),
    ];
    steps.extend(back_to_seat());
    steps
}

/// G in the landed ship: cabin gravity comes up (up turns to the floor's up over 1 s, no step),
/// and goes down again (up back to the planet's).
pub(super) fn lag_by_hand() -> Vec<Step> {
    let toggle = |name: &'static str, on: bool| -> Step {
        Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, name);
                tap(w, KeyCode::KeyG);
            }
            track_look(w, c);
            if c.t >= 1.3 {
                let f = ship_frame_of(w);
                let pl = planet(w);
                let up = with_player(w, |p| p.world_up(f));
                let want = if on { f.rot * DVec3::Y } else { pl.up(f.origin) };
                let off = up.angle_between(want).to_degrees();
                let tilt = (f.rot * DVec3::Y).angle_between(pl.up(f.origin)).to_degrees();
                let level = with_ship(w, |s| s.lag.level);
                end(w, c, format!("gravity {:.0} %, up {off:.3} deg from the {} up, ship tilt {tilt:.1} deg", level * 100.0, if on { "floor's" } else { "planet's" }));
                check(c, off < 0.1 && level == if on { 1.0 } else { 0.0 }, format!("{name}: up follows the cabin gravity ({off:.3} deg)"));
                // The field turns the view by the ship's tilt over its ramp time: on a slope steeper
                // than 30 degrees that is more than 0.5 deg a tick, smooth all the same.
                let ramp = with_ship(w, |s| s.lag.ramp_time);
                check_steady_within(c, name, 0.5f64.max(1.25 * tilt * c.dt / ramp));
                return true;
            }
            false
        })
    };
    vec![toggle("G in the landed ship: cabin gravity on", true), toggle("G again: cabin gravity off", false)]
}
