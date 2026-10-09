//! Flight feel: input ramps, the virtual joystick, boost, decoupled, camera effects.
use super::*;

/// Hold keys from rest and measure how long the ramped input takes to reach full deflection
/// (`ShipController::ramp.out[axis]`), against the tuning value.
fn ramp_check(name: &'static str, ks: &'static [KeyCode], axis: usize, want: fn(&flight_core::ShipTuning) -> f64) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            keys(w, ks, true);
            return false;
        }
        let out = with_ship(w, |s| s.ctl.ramp.out[axis]);
        if out.abs() >= 1.0 - 1e-6 || c.t > 3.0 {
            keys(w, ks, false);
            let want = want(&w.resource::<crate::tuning::Tuning>().ship);
            end(w, c, format!("full after {:.3} s (tuning {want:.3} s)", c.t));
            check(c, (c.t - want).abs() <= c.dt * 1.01, format!("{name}: full deflection after {:.3} s, tuning {want:.3} s", c.t));
            return true;
        }
        false
    })
}

/// Move the virtual stick to an angle (radians, mouse axes) by mouse pixels through `Controls`.
fn stick_to(w: &mut World, target: DVec2) {
    let off = with_ship(w, |s| s.stick.offset);
    let sens = w.resource::<Bindings>().mouse.ship_sensitivity;
    let d = (target - off) / sens;
    w.resource_mut::<Controls>().mouse += Vec2::new(d.x as f32, d.y as f32);
}

/// Yaw rate of the ship about its own up (rad/s, left positive), without the horizon follow.
fn yaw_rate(w: &mut World) -> f64 {
    let e = ship_e(w);
    let (r, av) = (w.get::<Rotation>(e).unwrap().0, w.get::<AngularVelocity>(e).unwrap().0);
    av.dot(r * DVec3::Y)
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
            let want = -share * w.resource::<crate::tuning::Tuning>().ship.turn_rate;
            end(w, c, format!("yaw rate {rate:+.3} rad/s, wanted {want:+.3}"));
            check(c, (rate - want).abs() <= 0.05 * want.abs().max(1.0), format!("{name}: yaw {rate:+.3} rad/s for {share} of the stick (wanted {want:+.3})"));
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
    // #25: thrust and rotation ramp to full deflection.
    s.push(ramp_check("ramp: W to full thrust", &[KeyCode::KeyW], 2, |t| t.linear_ramp_time));
    s.push(hold_until("hover", &[], 10.0, |w| ship_vel(w).length() < 0.5));
    s.push(Box::new(|w, c| {
        // Stick to full right: the turn ramps like any rotation.
        if c.t == 0.0 {
            begin(w, c, "ramp: stick to full yaw");
            let max = w.resource::<Bindings>().mouse.vjoy_max_angle;
            stick_to(w, DVec2::new(max * 2.0, 0.0));
            return false;
        }
        let out = with_ship(w, |s| s.ctl.ramp.out[5]);
        if out.abs() >= 1.0 - 1e-6 || c.t > 3.0 {
            let want = w.resource::<crate::tuning::Tuning>().ship.angular_ramp_time;
            end(w, c, format!("full after {:.3} s (tuning {want:.3} s)", c.t));
            check(c, (c.t - want).abs() <= c.dt * 1.01, format!("ramp: stick to full yaw after {:.3} s, tuning {want:.3} s", c.t));
            return true;
        }
        false
    }));
    // Virtual joystick: yaw rate is deflection times turn rate; inside the dead zone nothing.
    s.push(stick_yaw("stick: full right", 1.0));
    s.push(shot_step("stick-full-right"));
    s.push(Box::new(|w, c| {
        // Still at full right: the view leads the turn to the right, capped (#27).
        let look = w.resource::<crate::ship::CameraEffects>().0.look;
        let max = w.resource::<crate::tuning::Tuning>().camera.look_ahead_max_yaw_deg.to_radians();
        check(c, (look.y + max).abs() < 0.01 * max && look.x.abs() < 0.01, format!("camera: look-ahead {:+.2} deg yaw in a full right turn (cap {:.0})", look.y.to_degrees(), max.to_degrees()));
        true
    }));
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
    // #29: the pad's right stick through the same axes, without a device.
    s.push(Box::new(|w, c| {
        let stick = 0.8f32;
        if c.t == 0.0 {
            begin(w, c, "pad: right stick at 0.8");
            w.resource_mut::<Controls>().pad_axes.insert(GamepadAxis::RightStickX, stick);
        }
        if c.t >= 1.5 {
            w.resource_mut::<Controls>().pad_axes.clear();
            let shaped = w.resource::<Bindings>().axis(crate::controls::Axis::TurnYaw).shape(stick as f64);
            let want = -shaped * w.resource::<crate::tuning::Tuning>().ship.turn_rate;
            let rate = yaw_rate(w);
            end(w, c, format!("yaw rate {rate:+.3} rad/s, wanted {want:+.3}"));
            check(c, (rate - want).abs() <= 0.05 * want.abs(), format!("pad: stick 0.8 right yaws {rate:+.3} rad/s (dead zone and curve: {want:+.3})"));
            return true;
        }
        false
    }));
    s.push(stick_yaw("stick: centred", 0.0));
    // Boost raises the limit and drops back on release (#24); with the capacitor (#90) while the
    // charge lasts, so Shift is held for half the drain time.
    s.push(hold_until("cruise", &[KeyCode::KeyW], 6.0, |_| false));
    {
        let dir = dir.clone();
        s.push(Box::new(move |w, c| {
        let (limit, v) = (with_ship(w, |s| s.ctl.forward_speed_limit), ship_vel(w).length());
        if c.t == 0.0 {
            begin(w, c, "boost");
            c.v.insert("limit0", limit);
            c.v.insert("v0", v);
            keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], true);
        }
        if c.t >= 0.5 * with_ship(w, |s| s.ctl.tuning.boost_capacitor.drain_time) {
            // Screenshot while boost is still held, so the HUD shows it.
            shot(w, c, &dir, windowed, "boost");
            keys(w, &[KeyCode::ShiftLeft], false);
            let (l0, v0) = (c.v["limit0"], c.v["v0"]);
            c.v.insert("limit1", limit);
            c.v.insert("v1", v);
            end(w, c, format!("limit {l0:.0} -> {limit:.0} m/s, speed {v0:.0} -> {v:.0} m/s"));
            check(c, limit > 1.5 * l0 && v > v0 + 20.0, format!("boost: limit {l0:.0} -> {limit:.0} m/s, speed {v0:.0} -> {v:.0} m/s"));
            let (fov, base) = (w.resource::<crate::ship::CameraEffects>().0.fov_deg, w.resource::<crate::tuning::Tuning>().camera.fov_curve.eval(0.0));
            check(c, fov > base + 1.5, format!("camera: field of view {fov:.1} deg at {v:.0} m/s (at rest {base:.0})"));
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
            let (limit, v) = (with_ship(w, |s| s.ctl.forward_speed_limit), ship_vel(w).length());
            let (l0, l1, v1) = (c.v["limit0"], c.v["limit1"], c.v["v1"]);
            end(w, c, format!("limit {l1:.0} -> {limit:.0} m/s, speed {v1:.0} -> {v:.0} m/s"));
            // At least 70 % of the raise is gone and the limit is back near the cruise limit (the
            // ground below moves that a little).
            check(c, l1 - limit > 0.7 * (l1 - l0) && (limit - l0).abs() < 0.3 * l0 && v < v1 - 20.0, format!("boost released: limit {l1:.0} -> {limit:.0} m/s (before {l0:.0}), speed {v1:.0} -> {v:.0} m/s"));
            return true;
        }
        false
    }));
    s.push(hold_until("firm brake", &[KeyCode::KeyX], 15.0, |w| ship_vel(w).length() < 0.5));
    // #26: decoupled blends the damping out over decouple_time; the ship keeps gliding.
    s.push(hold_until("cruise", &[KeyCode::KeyW], 4.0, |_| false));
    s.push(Box::new(|w, c| {
        let time = w.resource::<crate::tuning::Tuning>().ship.decouple_time;
        if c.t == 0.0 {
            begin(w, c, "decouple (C) while cruising");
            keys(w, &[KeyCode::KeyW], true);
            tap(w, KeyCode::KeyC);
            return false;
        }
        let level = with_ship(w, |s| s.ctl.coupling);
        if (c.t - time * 0.5).abs() < c.dt * 0.5 {
            check(c, (level - 0.5).abs() < 0.02, format!("decouple: coupling {level:.3} halfway through the blend"));
        }
        if c.t >= time + 0.2 {
            keys(w, &[KeyCode::KeyW], false);
            c.v.insert("v_release", ship_vel(w).length());
            end(w, c, format!("coupling {level:.3} after {:.1} s", c.t));
            check(c, level == 0.0, format!("decouple: coupling {level:.3} after {:.1} s (blend {time} s)", c.t));
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
            check(c, v > 0.97 * v0, format!("decoupled: keeps gliding without input, {v0:.1} -> {v:.1} m/s in 3 s"));
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
            let level = with_ship(w, |s| s.ctl.coupling);
            end(w, c, format!("speed {v0:.1} -> {v:.1} m/s, coupling {level:.2}"));
            check(c, level == 1.0 && v < 0.7 * v0, format!("coupled again: the assist damps, {v0:.1} -> {v:.1} m/s in 8 s"));
            return true;
        }
        false
    }));
    s.push(hold_until("firm brake", &[KeyCode::KeyX], 15.0, |w| ship_vel(w).length() < 0.5));
    // Touchdown gives a camera bump (#27); on a slope the second side may give another.
    s.push(Box::new(|w, c| {
        c.v.insert("bumps0", w.resource::<crate::ship::CameraEffects>().0.bumps as f64);
        true
    }));
    s.push(land("land"));
    s.push(Box::new(|w, c| {
        let bumps = w.resource::<crate::ship::CameraEffects>().0.bumps as f64 - c.v["bumps0"];
        check(c, (1.0..=2.0).contains(&bumps), format!("camera: {bumps} touchdown bump(s) on landing"));
        true
    }));
    s.push(shot_step("landed"));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::F3);
        true
    }));
    s.push(wait(0.3));
    s.push(shot_step("debug-hud-f3"));
    // Screenshots are written a few frames later.
    s.push(wait(1.0));
}
