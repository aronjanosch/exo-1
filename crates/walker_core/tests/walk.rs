//! Walker checks against an analytic world (half-spaces), no engine.
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
                // point on the plane: n * d in local space
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
                let h = Hit { distance: t * motion.length(), normal: n };
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

fn walk(world: &Planes, w: &mut Walker, steps: usize, dir: DVec2) -> usize {
    let mut grounded = 0;
    for _ in 0..steps {
        w.step(&Frame::IDENTITY, DVec3::Y, 9.81, &WalkInput { dir, ..Default::default() }, world, DT);
        grounded += w.grounded as usize;
    }
    grounded
}

#[test]
fn lands_and_walks_flat_ground_always_on_floor() {
    let world = flat();
    let mut w = Walker::new(DVec3::new(0.0, 2.0, 0.0), DVec3::X);
    walk(&world, &mut w, 120, DVec2::ZERO);
    assert!(w.grounded && w.pos.y.abs() < 0.02, "landed at {}", w.pos.y);
    let g = walk(&world, &mut w, 600, DVec2::new(0.0, 1.0));
    println!("flat: grounded {g}/600, x {:.3}, y {:.4}", w.pos.x, w.pos.y);
    assert_eq!(g, 600);
    assert!((w.pos.x - 50.0).abs() < 0.1);
}

#[test]
fn slope_below_limit_is_climbed_steep_slope_stops() {
    for (deg, climbs) in [(30.0, true), (45.0, true), (55.0, false), (70.0, false)] {
        let world = slope(deg);
        let mut w = Walker::new(DVec3::new(-5.0, 0.0, 0.0), DVec3::X);
        let g = walk(&world, &mut w, 600, DVec2::new(0.0, 1.0));
        println!("slope {deg}: x {:.2} y {:.2} grounded {g}/600", w.pos.x, w.pos.y);
        if climbs {
            assert!(w.pos.y > 5.0, "should climb {deg}");
        } else {
            assert!(w.pos.x < 0.1 && w.pos.y < 0.3, "should stop at {deg}: {:?}", w.pos);
        }
    }
}

#[test]
fn jump_leaves_and_returns_to_ground() {
    let world = flat();
    let mut w = Walker::new(DVec3::ZERO, DVec3::X);
    walk(&world, &mut w, 10, DVec2::ZERO);
    w.step(&Frame::IDENTITY, DVec3::Y, 9.81, &WalkInput { jump: true, ..Default::default() }, &world, DT);
    let mut peak: f64 = 0.0;
    for _ in 0..120 {
        w.step(&Frame::IDENTITY, DVec3::Y, 9.81, &WalkInput::default(), &world, DT);
        peak = peak.max(w.pos.y);
    }
    println!("jump peak {peak:.3}");
    assert!((peak - 25.0 / (2.0 * 9.81)).abs() < 0.1 && w.grounded);
}

/// Spike 3: stand and walk in a cabin floor of a frame flying at 400 m/s while rolling.
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

/// Spike 9 bug: in the air against a steep slope, sliding along it lifted the walker.
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

/// Issue #5: weightless the walker keeps the velocity it brought (e.g. from the ship), input does
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

    // Drifting into a floor: the part into it stops, the part along it stays.
    let world = flat();
    let mut w = Walker::new(DVec3::new(0.0, 0.5, 0.0), DVec3::NEG_Z);
    w.vel = DVec3::new(2.0, -1.0, 0.0);
    for _ in 0..120 {
        w.step(&Frame::IDENTITY, DVec3::Y, 0.0, &WalkInput::default(), &world, DT);
    }
    assert!((w.vel - DVec3::new(2.0, 0.0, 0.0)).length() < 1e-9, "velocity {:?}", w.vel);
    assert!(w.pos.y >= 0.0 && w.pos.x > 3.9, "position {:?}", w.pos);
}

/// Issue #7: re-splitting the look direction about a tilted up keeps it in the world.
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

/// Issue #8: suit thrusters push along the body axes, the brake comes to rest without overshoot within 4 s.
#[test]
fn suit_thrusts_along_the_body_and_brakes_to_rest() {
    let cfg = SuitConfig::default();
    let rot = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2); // body forward (-z) is world -x
    let a = suit_accel(&cfg, rot, DVec3::ZERO, &SuitInput { thrust: DVec3::new(0.0, 0.0, -1.0), ..Default::default() });
    assert!((a - DVec3::new(-cfg.accel, 0.0, 0.0)).length() < 1e-12, "{a:?}");

    let world = Planes { planes: vec![], frame: Frame::IDENTITY };
    let mut w = Walker::new(DVec3::ZERO, DVec3::NEG_Z);
    w.vel = DVec3::new(3.0, -1.0, 0.5);
    let brake = SuitInput { brake: true, ..Default::default() };
    let mut ticks = 0;
    while w.vel.length() > 1e-3 && ticks < 600 {
        let accel = suit_accel(&cfg, rot, w.vel, &brake);
        w.step(&Frame::IDENTITY, DVec3::Y, 0.0, &WalkInput { accel, ..Default::default() }, &world, DT);
        assert!(w.vel.dot(DVec3::new(3.0, -1.0, 0.5)) >= 0.0, "brake overshoots");
        ticks += 1;
    }
    assert!(ticks < 240, "brake took {ticks} ticks");
}
