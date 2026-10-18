//! Flight feel: the virtual joystick, boost, decoupled, camera effects (the SC model).
use super::*;
use crate::ship::basis_for_up;

/// Move the virtual stick to an angle (radians, mouse axes) by mouse pixels through `Controls`.
fn stick_to(w: &mut World, target: DVec2) {
    let off = with_ship(w, |s| s.stick.offset);
    let sens = w.resource::<Bindings>().mouse.ship_sensitivity;
    let d = (target - off) / sens;
    w.resource_mut::<Controls>().mouse += Vec2::new(d.x as f32, d.y as f32);
}

/// Yaw rate of the ship about its own up (rad/s, left positive).
fn yaw_rate(w: &mut World) -> f64 {
    let e = ship_e(w);
    let (r, av) = (w.get::<Rotation>(e).unwrap().0, w.get::<AngularVelocity>(e).unwrap().0);
    av.dot(r * DVec3::Y)
}

/// Full-stick yaw rate of a ship hovering at rest (rad/s): the SC model's rate at its share for
/// speed 0 (the spring settles to it within the check's 1.5 s).
fn rest_yaw_rate(w: &World) -> f64 {
    let t = &w.resource::<crate::tuning::Tuning>().ship_type.flight.angular;
    t.rate.yaw * t.rate_over_speed.eval(0.0)
}

/// Stick right by a share of its travel past the dead zone, hold, check the yaw rate is that
/// share of the turn rate; 0 centres the stick.
fn stick_yaw(name: &'static str, share: f64) -> Step {
    Box::new(move |w, c| {
        let (dz, max) = {
            let m = &w.resource::<Bindings>().mouse;
            (m.vjoy_deadzone, m.vjoy_max_angle)
        };
        if c.t == 0.0 {
            begin(w, c, name);
            let angle = if share == 0.0 { 0.0 } else { dz + share * (max - dz) };
            stick_to(w, DVec2::new(angle, 0.0));
        }
        if c.t >= 1.5 {
            let rate = yaw_rate(w);
            let want = -share * rest_yaw_rate(w);
            end(w, c, format!("yaw rate {rate:+.3} rad/s, wanted {want:+.3}"));
            return true;
        }
        false
    })
}

pub(super) fn flight_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step, out_dir: &std::path::Path, windowed: bool) {
    let dir = out_dir.to_path_buf();
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(hold_until("climb to 300 m above ground", &[KeyCode::Space, KeyCode::ShiftLeft], 60.0, |w| above_ground(w) > 300.0));
    s.push(hold_until("hover", &[], 10.0, |w| ship_vel(w).length() < 0.5));
    s.push(stick_yaw("stick: full right", 1.0));
    s.push(shot_step("stick-full-right"));
    s.push(stick_yaw("stick: half right", 0.5));
    s.push(stick_yaw("stick: centred", 0.0));
    s.push(Box::new(|w, c| {
        let dz = w.resource::<Bindings>().mouse.vjoy_deadzone;
        if c.t == 0.0 {
            begin(w, c, "stick: inside the dead zone");
            stick_to(w, DVec2::new(dz * 0.8, 0.0));
        }
        if c.t >= 1.0 {
            let rate = yaw_rate(w);
            end(w, c, format!("yaw rate {rate:+.4} rad/s"));
            check(c, rate.abs() < 0.01, format!("stick: inside the dead zone the ship does not turn ({rate:+.4} rad/s)"));
            stick_to(w, DVec2::ZERO);
            return true;
        }
        false
    }));
    s.push(Box::new(|w, c| {
        let stick = 0.8f32;
        if c.t == 0.0 {
            begin(w, c, "pad: right stick at 0.8");
            w.resource_mut::<Controls>().pad_axes.insert(GamepadAxis::RightStickX, stick);
        }
        if c.t >= 1.5 {
            w.resource_mut::<Controls>().pad_axes.clear();
            let shaped = w.resource::<Bindings>().axis(crate::controls::Axis::TurnYaw).shape(stick as f64);
            let want = -shaped * rest_yaw_rate(w);
            let rate = yaw_rate(w);
            end(w, c, format!("yaw rate {rate:+.3} rad/s, wanted {want:+.3}"));
            return true;
        }
        false
    }));
    s.push(stick_yaw("stick: centred", 0.0));
    // Boost raises the limit and drops back on release; with the capacitor while the
    // charge lasts, so Shift is held for half the drain time.
    s.push(hold_until("cruise", &[KeyCode::KeyW], 6.0, |_| false));
    {
        let dir = dir.clone();
        s.push(Box::new(move |w, c| {
        let (limit, v) = (with_ship(w, |s| s.sc.status.cap), ship_vel(w).length());
        if c.t == 0.0 {
            begin(w, c, "boost");
            c.v.insert("limit0", limit);
            c.v.insert("v0", v);
            keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], true);
        }
        if c.t >= 0.5 * w.resource::<crate::tuning::Tuning>().ship_type.flight.drive.boost_capacitor.drain_time {
            shot(w, c, &dir, windowed, "boost");
            keys(w, &[KeyCode::ShiftLeft], false);
            let (l0, v0) = (c.v["limit0"], c.v["v0"]);
            c.v.insert("limit1", limit);
            c.v.insert("v1", v);
            end(w, c, format!("limit {l0:.0} -> {limit:.0} m/s, speed {v0:.0} -> {v:.0} m/s"));
            return true;
        }
        false
    }));
    }
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "boost released");
        }
        if c.t >= 8.0 {
            keys(w, &[KeyCode::KeyW], false);
            let (limit, v) = (with_ship(w, |s| s.sc.status.cap), ship_vel(w).length());
            let (l1, v1) = (c.v["limit1"], c.v["v1"]);
            end(w, c, format!("limit {l1:.0} -> {limit:.0} m/s, speed {v1:.0} -> {v:.0} m/s"));
            return true;
        }
        false
    }));
    s.push(hold_until("firm brake", &[KeyCode::KeyX], 15.0, |w| ship_vel(w).length() < 0.5));
    // Decoupling immediately releases velocity control, including when W is released together.
    s.push(hold_until("cruise", &[KeyCode::KeyW], 4.0, |_| false));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "decouple (C) while cruising");
            keys(w, &[KeyCode::KeyW], false);
            tap(w, KeyCode::KeyC);
            return false;
        }
        let level = with_ship(w, |s| s.sc.modes.coupling);
        if c.t >= 0.2 {
            keys(w, &[KeyCode::KeyW], false);
            c.v.insert("v_release", ship_vel(w).length());
            end(w, c, format!("coupling {level:.3} after {:.1} s", c.t));
            check(c, level == 0.0, format!("decouple immediately releases velocity control: coupling {level:.3}"));
            return true;
        }
        false
    }));
    {
        let dir = dir.clone();
        s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "decoupled glide, no input");
        }
        if (c.t - 2.0).abs() < c.dt * 0.5 {
            shot(w, c, &dir, windowed, "decoupled");
        }
        if c.t >= 3.0 {
            let (v0, v) = (c.v["v_release"], ship_vel(w).length());
            end(w, c, format!("speed {v0:.1} -> {v:.1} m/s"));
            tap(w, KeyCode::KeyC);
            return true;
        }
        false
    }));
    }
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "coupled again, no input");
            c.v.insert("v_couple", ship_vel(w).length());
        }
        if c.t >= 8.0 {
            let (v0, v) = (c.v["v_couple"], ship_vel(w).length());
            let level = with_ship(w, |s| s.sc.modes.coupling);
            end(w, c, format!("speed {v0:.1} -> {v:.1} m/s, coupling {level:.2}"));
            check(c, level == 1.0, format!("coupled again: the assist is on ({level:.2}), {v0:.1} -> {v:.1} m/s in 8 s"));
            return true;
        }
        false
    }));
    s.push(hold_until("firm brake", &[KeyCode::KeyX], 15.0, |w| ship_vel(w).length() < 0.5));
// A test hook levels the ship first: a nose-up hull after the cruise flies off.
    s.push(Box::new(|w, _| {
        let e = ship_e(w);
        let (p, up) = (w.get::<Position>(e).unwrap().0, planet(w).up(ship_frame_of(w).origin));
        teleport_ship(w, p, basis_for_up(up));
        true
    }));
    s.push(hold_until("descend to 120 m above ground", &[KeyCode::ControlLeft, KeyCode::ShiftLeft], 240.0, |w| above_ground(w) < 120.0));
    s.push(hold_until("firm brake", &[KeyCode::KeyX], 15.0, |w| ship_vel(w).length() < 0.5));
    s.push(land("land"));
    s.push(shot_step("landed"));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::F3);
        true
    }));
    s.push(wait(0.3));
    s.push(shot_step("debug-hud-f3"));
    s.push(wait(1.0));
}

/// Exercise the real Controls -> flight controller -> Avian path, not just the core integrator.
pub(super) fn coupled_loop_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    // The lagged Comstab-off trajectory needs more room for a whole loop. Keep
    // its low segment too, but do not confuse a later terrain impact with a controller lock.
    for (height, share, comstab, duration, release_at) in [
        (400.0, 1.0, true, 15.0, None), (400.0, 1.0, false, 6.0, None),
        (800.0, 1.0, false, 15.0, None), (800.0, 0.6, true, 15.0, None), (800.0, 0.6, false, 15.0, None),
        // Leave thrust headroom here to check deceleration as well as steering.
        (2000.0, 0.2, true, 8.0, Some(5.0)),
    ] {
        s.push(Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, "coupled loop with held throttle");
                keys(w, &[KeyCode::KeyW], false);
                let pl = planet(w);
                let up = DVec3::Y;
                teleport_ship(w, pl.centre + up * (pl.surface(up) + height), basis_for_up(up));
                with_ship(w, |s| {
                    s.sc.reset_state();
                    s.sc.modes.comstab = comstab;
                    s.parked = false;
                    s.ground.release();
                    s.stick = Default::default();
                });
                for key in ["loop_throttle", "loop_pause", "loop_longest", "loop_angle", "loop_samples", "loop_peak_g", "loop_release_speed", "loop_coast_arc"] {
                    c.v.insert(key, 0.0);
                }
                c.p.insert("loop_last_velocity", DVec3::ZERO);
                let mb = &w.resource::<Bindings>().mouse;
                let angle = mb.vjoy_deadzone + share * (mb.vjoy_max_angle - mb.vjoy_deadzone);
                stick_to(w, DVec2::new(0.0, angle));
                return false;
            }
            if c.t >= 1.0 && c.v["loop_throttle"] == 0.0 {
                keys(w, &[KeyCode::KeyW], true);
                c.v.insert("loop_throttle", 1.0);
            }
            let e = ship_e(w);
            let rot = w.get::<Rotation>(e).unwrap().0;
            let av = w.get::<AngularVelocity>(e).unwrap().0;
            let velocity = w.get::<LinearVelocity>(e).unwrap().0;
            let pitch = -(rot.inverse() * av).x;
            let was_released = c.v["loop_throttle"] == 2.0;
            if release_at.is_some_and(|at| c.t >= at) && c.v["loop_throttle"] == 1.0 {
                keys(w, &[KeyCode::KeyW], false);
                c.v.insert("loop_throttle", 2.0);
                c.v.insert("loop_release_speed", velocity.length());
            }
            let last_velocity = c.p["loop_last_velocity"];
            if was_released && velocity.length() > 1.0 && last_velocity.length() > 1.0 {
                let arc = last_velocity.normalize().cross(velocity.normalize()).dot(rot * DVec3::NEG_X);
                c.v.insert("loop_coast_arc", c.v["loop_coast_arc"] + arc);
            }
            c.p.insert("loop_last_velocity", velocity);
            let (g, safeties) = with_ship(w, |s| (s.sc.status.felt_g,
                s.sc.modes.coupled && s.sc.modes.grav_comp && s.sc.modes.g_safe && s.sc.modes.proximity && s.sc.tuning.linear.anti_drift));
            c.v.insert("loop_peak_g", c.v["loop_peak_g"].max(g));
            if c.t > 2.0 {
                let pause = if pitch < 0.1 * share { c.v["loop_pause"] + c.dt } else { 0.0 };
                c.v.insert("loop_pause", pause);
                c.v.insert("loop_longest", c.v["loop_longest"].max(pause));
                c.v.insert("loop_angle", c.v["loop_angle"] + pitch * c.dt);
                c.v.insert("loop_samples", c.v["loop_samples"] + 1.0);
            }
            let contact = c.t > 1.0 && w.resource::<ContactGraph>().iter_active_touching().any(|p| p.body1 == Some(e) || p.body2 == Some(e));
            let clearance = above_ground(w);
            if c.t >= duration || contact || clearance < 2.0 {
                keys(w, &[KeyCode::KeyW], false);
                stick_to(w, DVec2::ZERO);
                let pause = c.v["loop_longest"];
                let angle = c.v["loop_angle"];
                let peak_g = c.v["loop_peak_g"];
                let samples = c.v["loop_samples"];
                end(w, c, format!("start {height:.0} m, stick {share}, comstab {comstab}: longest pause {pause:.3} s, rotated {:.1} deg, clearance {clearance:.1} m, peak {peak_g:.3} g", angle.to_degrees()));
                check(c, pause < 0.5 && samples > 0.0, format!("held throttle does not lock pitch: longest pause {pause:.3} s"));
                check(c, !contact && clearance >= 2.0, "the control check remained collision-free".into());
                let min_angle = if duration >= 15.0 { std::f64::consts::PI } else { 0.0 };
                check(c, angle > min_angle, format!("pitch input keeps turning: {:.1} deg", angle.to_degrees()));
                check(c, safeties && peak_g <= flight_core::limits::G_SAFE_MAX_G + 1e-6, format!("assists remain enabled and G-safe holds: {peak_g:.3} g"));
                if release_at.is_some() {
                    let speed_before = c.v["loop_release_speed"];
                    let arc = c.v["loop_coast_arc"];
                    check(c, speed_before > 5.0 && velocity.length() < speed_before,
                        format!("gas release brakes: {speed_before:.2} -> {:.2} m/s", velocity.length()));
                    check(c, arc > 0.0, format!("velocity direction keeps turning with the nose while braking: {:.1} deg", arc.to_degrees()));
                }
                return true;
            }
            false
        }));
    }
}
