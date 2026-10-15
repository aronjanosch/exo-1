use glam::{DQuat, DVec2, DVec3};
use walker_core::*;

/// Solid below each plane (n · x < d); the walkable ground is the upper envelope.
struct Planes {
    planes: Vec<(DVec3, f64)>,
    frame: Frame,
}

impl Planes {
    fn capsule_dist(&self, n: DVec3, d: f64, feet: DVec3, up: DVec3) -> f64 {
        let c = WalkerConfig::default();
        let a = feet + up * c.radius;
        let b = feet + up * (c.height - c.radius);
        n.dot(a).min(n.dot(b)) - d - c.radius
    }
    fn world_planes(&self) -> Vec<(DVec3, f64)> {
        self.planes
            .iter()
            .map(|&(n, d)| {
                let wn = self.frame.rot * n;
                (wn, wn.dot(self.frame.to_world(n * d)))
            })
            .collect()
    }
}

impl World for Planes {
    fn sweep(&self, feet: DVec3, up: DVec3, motion: DVec3) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        for (n, d) in self.world_planes() {
            let dist = self.capsule_dist(n, d, feet, up);
            let into = -n.dot(motion);
            if into <= 0.0 {
                continue;
            }
            let t = (dist.max(0.0) / into).min(f64::MAX);
            if t <= 1.0 {
                let h = Hit { distance: t * motion.length(), normal: n, velocity: DVec3::ZERO };
                if best.is_none_or(|b| h.distance < b.distance) {
                    best = Some(h);
                }
            }
        }
        best
    }
    fn depenetrate(&self, feet: DVec3, up: DVec3) -> DVec3 {
        let mut push = DVec3::ZERO;
        for (n, d) in self.world_planes() {
            let dist = self.capsule_dist(n, d, feet + push, up);
            if dist < 0.0 {
                push += n * (-dist + 0.01);
            }
        }
        push
    }
}

fn flat() -> Planes {
    Planes { planes: vec![(DVec3::Y, 0.0)], frame: Frame::IDENTITY }
}

/// Flat ground y = 0 for x < 0, then a slope of `deg` rising towards +x.
fn slope(deg: f64) -> Planes {
    let a = deg.to_radians();
    let n = DVec3::new(-a.sin(), a.cos(), 0.0);
    Planes { planes: vec![(DVec3::Y, 0.0), (n, 0.0)], frame: Frame::IDENTITY }
}

const DT: f64 = 1.0 / 60.0;

#[test]
fn walking_in_a_fast_rolling_frame_has_no_drift() {
    let mut world = Planes { planes: vec![(DVec3::Y, 0.0)], frame: Frame::IDENTITY };
    let v = DVec3::new(0.0, 30.0, -400.0);
    let w_ang = DVec3::new(0.0, 0.0, 1.2); // roll, rad/s
    let mut w = Walker::new(DVec3::new(0.0, 0.3, 0.0), DVec3::NEG_Z);
    let mut max_dev: f64 = 0.0;
    let mut grounded = 0;
    for i in 0..1200 {
        world.frame.origin += v * DT;
        world.frame.rot = (DQuat::from_scaled_axis(world.frame.rot * w_ang * DT) * world.frame.rot).normalize();
        let dir = if i < 600 { DVec2::ZERO } else { DVec2::new(0.0, if i % 240 < 120 { 1.0 } else { -1.0 }) };
        let frame = world.frame;
        w.step(&frame, DVec3::Y, 9.81, &WalkInput { dir, ..Default::default() }, &world, DT);
        if i >= 30 {
            max_dev = max_dev.max(w.pos.y.abs());
            grounded += w.grounded as usize;
        }
        if i == 599 {
            println!("standing drift {:.6} m", (w.pos - DVec3::new(0.0, w.pos.y, 0.0)).length());
            assert!((w.pos - DVec3::new(0.0, w.pos.y, 0.0)).length() < 1e-6);
        }
    }
    println!("cabin height deviation max {max_dev:.6} m, grounded {grounded}/1170");
    assert!(max_dev < 0.02 && grounded == 1170);
}

#[test]
fn change_frame_keeps_world_state() {
    let old = Frame::IDENTITY;
    let new = Frame { origin: DVec3::new(100.0, 5.0, 0.0), rot: DQuat::from_rotation_z(0.3) };
    let mut w = Walker::new(DVec3::new(101.0, 6.0, 1.0), DVec3::X);
    w.vel = DVec3::new(300.0, 0.0, 0.0);
    w.change_frame(&old, &new, -DVec3::new(299.0, 0.0, 0.0));
    assert!((new.to_world(w.pos) - DVec3::new(101.0, 6.0, 1.0)).length() < 1e-9);
    assert!(((new.rot * w.vel) - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-9);
}

/// In the air against a steep slope, sliding along it lifted the walker.
#[test]
fn airborne_walker_does_not_climb_steep_slope() {
    let world = slope(57.0);
    let mut w = Walker::new(DVec3::new(-0.5, 0.05, 0.0), DVec3::X);
    let mut top: f64 = 0.0;
    for _ in 0..600 {
        w.step(&Frame::IDENTITY, DVec3::Y, 9.81, &WalkInput { dir: DVec2::new(0.0, 1.0), ..Default::default() }, &world, DT);
        top = top.max(w.pos.y);
    }
    println!("airborne against 57 deg: highest {top:.3} m");
    assert!(top < 0.3);
}

/// Weightless the walker keeps the velocity it brought (e.g. from the ship), input does
/// not move it, and a floor it drifts along does not stop it.
#[test]
fn weightless_walker_keeps_its_velocity() {
    let world = Planes { planes: vec![], frame: Frame::IDENTITY };
    let mut w = Walker::new(DVec3::ZERO, DVec3::NEG_Z);
    w.grounded = true;
    w.vel = DVec3::new(0.0, -3.0, 2.0);
    let input = WalkInput { dir: DVec2::new(1.0, 1.0), run: true, jump: true, ..Default::default() };
    for _ in 0..120 {
        w.step(&Frame::IDENTITY, DVec3::Y, 0.0, &input, &world, DT);
    }
    assert!((w.vel - DVec3::new(0.0, -3.0, 2.0)).length() < 1e-12, "velocity {:?}", w.vel);
    assert!((w.pos - DVec3::new(0.0, -6.0, 4.0)).length() < 1e-9, "position {:?}", w.pos);
    assert!(!w.grounded);

    let world = flat();
    let mut w = Walker::new(DVec3::new(0.0, 0.5, 0.0), DVec3::NEG_Z);
    w.vel = DVec3::new(2.0, -1.0, 0.0);
    for _ in 0..120 {
        w.step(&Frame::IDENTITY, DVec3::Y, 0.0, &WalkInput::default(), &world, DT);
    }
    assert!((w.vel - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-9, "velocity {:?}", w.vel);
    assert!(w.pos.y >= 0.0 && w.pos.x > 3.9, "position {:?}", w.pos);
}

#[test]
fn split_look_keeps_the_world_direction() {
    let up = DVec3::Y;
    let forward = DVec3::NEG_Z;
    let look = look_dir(forward, up, 0.4);
    let tilted = DQuat::from_rotation_z(0.3) * DQuat::from_rotation_x(-0.2) * up;
    let (f, pitch) = split_look(look, tilted, forward);
    assert!(f.dot(tilted).abs() < 1e-12, "heading not perpendicular to up");
    assert!((look_dir(f, tilted, pitch) - look).length() < 1e-12);
    // Straight up: no heading, the fallback is used.
    let (f, pitch) = split_look(DVec3::Y, DVec3::Y, DVec3::NEG_Z);
    assert!((f - DVec3::NEG_Z).length() < 1e-12 && (pitch - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
}

#[test]
fn view_helpers() {
    let look = DVec3::new(1.0, 0.3, -0.2).normalize();
    let q = look_rot(look, DVec3::Y);
    assert!((q * DVec3::NEG_Z - look).length() < 1e-12);
    assert!((q * DVec3::Y).dot(look).abs() < 1e-12 && (q * DVec3::Y).dot(DVec3::Y) > 0.9);

    let dt = 1.0 / 60.0;
    let mut up = DVec3::X;
    let mut largest: f64 = 0.0;
    for _ in 0..240 {
        let next = turn_towards(up, DVec3::Y, dt, 0.5, std::f64::consts::FRAC_PI_2);
        largest = largest.max(next.angle_between(up));
        up = next;
    }
    assert!(up.angle_between(DVec3::Y) < 1e-3, "arrives: {up:?}");
    assert!(largest <= std::f64::consts::FRAC_PI_2 * dt + 1e-9, "rate cap: {largest}");

    let b = turn_body(DQuat::IDENTITY, 0.0, 0.0, 0.5);
    assert!((b * DVec3::NEG_Z - DVec3::NEG_Z).length() < 1e-12, "roll keeps the look");
}

#[test]
fn moving_surface_stops_only_the_relative_velocity() {
    struct Wall(DVec3);
    impl World for Wall {
        fn sweep(&self, _feet: DVec3, _up: DVec3, _motion: DVec3) -> Option<Hit> {
            Some(Hit { distance: 0.0, normal: DVec3::Z, velocity: self.0 })
        }
        fn depenetrate(&self, _feet: DVec3, _up: DVec3) -> DVec3 {
            DVec3::ZERO
        }
    }
    let drift = DVec3::new(0.0, 0.0, -3.0);
    let mut w = Walker::new(DVec3::ZERO, DVec3::NEG_Z);
    w.vel = drift;
    w.step(&Frame::IDENTITY, DVec3::Y, 0.0, &WalkInput::default(), &Wall(drift), DT);
    assert!((w.vel - drift).length() < 1e-12, "moves with the wall: {:?}", w.vel);
    w.step(&Frame::IDENTITY, DVec3::Y, 0.0, &WalkInput::default(), &Wall(DVec3::ZERO), DT);
    assert!(w.vel.length() < 1e-12, "stopped by a standing wall: {:?}", w.vel);
}

fn walk(world: &Planes, w: &mut Walker, steps: usize, dir: DVec2) -> usize {
    let mut grounded = 0;
    for _ in 0..steps {
        w.step(&Frame::IDENTITY, DVec3::Y, 9.81, &WalkInput { dir, ..Default::default() }, world, DT);
        grounded += w.grounded as usize;
    }
    grounded
}

#[test]
fn on_flat_ground_the_walker_lands_and_stays_on_the_floor() {
    let world = flat();
    let mut w = Walker::new(DVec3::new(0.0, 2.0, 0.0), DVec3::X);
    walk(&world, &mut w, 120, DVec2::ZERO);
    assert!(w.grounded && w.pos.y.abs() < 0.02, "landed at {}", w.pos.y);
    assert_eq!(walk(&world, &mut w, 600, DVec2::new(0.0, 1.0)), 600);
}
