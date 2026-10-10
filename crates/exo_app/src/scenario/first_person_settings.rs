//! First-person mouse units, vertical projection, and isolation from vehicle controls.
use super::*;
use crate::settings::Settings;

fn camera_fov(w: &mut World) -> f64 {
    let mut q = w.query_filtered::<&Projection, With<crate::view::MainCamera>>();
    match q.single(w).unwrap() {
        Projection::Perspective(p) => (p.fov as f64).to_degrees(),
        _ => panic!("expected perspective camera"),
    }
}

fn look(sensitivity: f64, vfov: f64) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "first-person mouse look");
            let mut settings = w.resource_mut::<Settings>();
            settings.mouse_sensitivity = sensitivity;
            settings.fov_deg = vfov;
            c.p.insert("settings_forward", with_player(w, |p| p.w.forward));
            c.v.insert("settings_pitch", with_player(w, |p| p.pitch));
            w.resource_mut::<Controls>().mouse = Vec2::new(100.0, 50.0);
            return false;
        }
        if c.t < 0.1 { return false; }
        let (forward, pitch) = with_player(w, |p| (p.w.forward, p.pitch));
        let yaw = forward.angle_between(c.p["settings_forward"]).to_degrees();
        let pitch = (pitch - c.v["settings_pitch"]).to_degrees();
        check(c, (yaw - 2.2 * sensitivity).abs() < 0.001 && (pitch + 1.1 * sensitivity).abs() < 0.001,
            format!("sensitivity {sensitivity}: 100/50 counts turn yaw {yaw:.4}°, pitch {pitch:.4}°"));
        let fov = camera_fov(w);
        check(c, (fov - vfov).abs() < 0.001, format!("first-person vertical FOV {fov:.2}° (wanted {vfov})"));
        true
    })
}

fn vehicle(sensitivity: f64, vfov: f64) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "vehicle ignores first-person settings");
            let mut settings = w.resource_mut::<Settings>();
            settings.mouse_sensitivity = sensitivity;
            settings.fov_deg = vfov;
            w.resource_mut::<Bindings>().mouse.ship_mode = crate::controls::ShipMouse::Vjoy;
            with_ship(w, |s| s.stick = flight_core::VirtualStick::default());
            w.resource_mut::<Controls>().mouse = Vec2::new(100.0, 0.0);
            return false;
        }
        if c.t < 0.1 { return false; }
        let want = 100.0 * w.resource::<Bindings>().mouse.ship_sensitivity;
        let offset = with_ship(w, |s| s.stick.offset.x);
        check(c, (offset - want).abs() < 1e-8, format!("first-person sensitivity {sensitivity}: ship stick {offset:.4} rad (wanted {want:.4})"));
        let actual = camera_fov(w);
        let want = w.resource::<crate::ship::CameraEffects>().0.fov_deg;
        check(c, (actual - want).abs() < 0.05, format!("first-person FOV {vfov}°: vehicle camera {actual:.2}° (own effects {want:.2}°)"));
        true
    })
}

pub(super) fn steps(s: &mut Vec<Step>) {
    s.push(look(1.0, 40.0));
    s.push(look(3.0, 90.0));
    s.push(Box::new(|w, _| { put_at_seat(w); true }));
    s.push(wait(0.3));
    s.push(look(1.0, 55.0));
    s.push(look(3.0, 85.0));
    s.extend(back_to_seat());
    s.push(Box::new(|w, c| {
        check(c, with_player(w, |p| p.seated), "F seated the walker at the vehicle controls".into());
        true
    }));
    s.push(vehicle(0.1, 40.0));
    s.push(vehicle(10.0, 90.0));
    s.push(Box::new(|w, _| { tap(w, KeyCode::KeyF); true }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        check(c, !with_player(w, |p| p.seated), "F stood the walker up again".into());
        true
    }));
    s.push(look(2.0, 60.0));
}
