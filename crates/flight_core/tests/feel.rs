//! The virtual stick (the mouse as a joystick, #25 feel).
use flight_core::*;
use glam::DVec2;

#[test]
fn stick_deflection() {
    let mut s = VirtualStick::default();
    let (dz, max) = (0.02, 0.2);
    s.push(DVec2::new(0.01, 0.0), max);
    assert_eq!(s.deflection(dz, max, None), DVec2::ZERO, "inside the dead zone");
    s.push(DVec2::new(0.1, 0.0), max);
    let d = s.deflection(dz, max, None);
    assert!((d.y + (0.11 - dz) / (max - dz)).abs() < 1e-12, "mouse right yaws right: {d}");
    s.push(DVec2::new(5.0, 0.0), max);
    assert!((s.offset.length() - max).abs() < 1e-12, "offset capped at the max angle");
    assert!((s.deflection(dz, max, None).y + 1.0).abs() < 1e-12);
    let mut up = VirtualStick::default();
    up.push(DVec2::new(0.0, -0.2), max);
    assert!(up.deflection(dz, max, None).x > 0.99, "mouse up lifts the nose");
}
