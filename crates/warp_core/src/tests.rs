use super::*;
use crate::path::Blocker;
use glam::DVec3;

const JSON: &str = include_str!("../../../content/system/system.json");

fn sys() -> System {
    System::from_json(JSON).expect("system.json")
}

/// Ship in orbit above Hearth on the side facing Cinder, nose along the course.
fn ready(sys: &System) -> (Drive, ShipView) {
    let pos = DVec3::new(0.0, 7000.0, 0.0);
    let d = Drive::new(sys.drive.clone());
    let mut ship = ShipView { pos, forward: DVec3::X, speed: 0.0 };
    let path_dir = {
        let mut t = d.clone();
        t.begin(1, &ship, sys, &[]).unwrap();
        t.path().unwrap().start_dir()
    };
    ship.forward = path_dir;
    (d, ship)
}

const DT: f64 = 1.0 / 60.0;

/// Runs the drive until it is idle or `max` seconds pass; returns every event with its time.
fn run(d: &mut Drive, ship: &mut ShipView, s: &System, max: f64) -> (Vec<(f64, Event)>, f64) {
    let mut t = 0.0;
    let mut out = Vec::new();
    let mut max_speed: f64 = 0.0;
    while t < max {
        t += DT;
        let ev = d.step(DT, ship, s, &[]);
        if let Some((p, v)) = d.pose() {
            ship.pos = p;
            ship.speed = v.length();
            max_speed = max_speed.max(ship.speed);
        }
        for e in ev {
            out.push((t, e));
        }
        if d.phase == Phase::Idle && t > 1.0 {
            break;
        }
    }
    (out, max_speed)
}

#[test]
fn warp_hearth_to_cinder_and_back() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.begin(1, &ship, &s, &[]).unwrap();
    let (ev, vmax) = run(&mut d, &mut ship, &s, 200.0);
    let arrived = ev.iter().find(|(_, e)| *e == Event::Arrived).expect("arrived").0;
    let start = ev.iter().find(|(_, e)| *e == Event::Phase(Phase::RampUp)).unwrap().0;
    println!("warp A->B: start {start:.1} s, flight {:.1} s, top speed {:.0} m/s", arrived - start, vmax);
    let exit = Drive::exit_point(&s, 1, DVec3::new(0.0, 7000.0, 0.0));
    assert!(ship.pos.distance(exit) < 1e-6, "exit error {}", ship.pos.distance(exit));
    assert_eq!(d.phase, Phase::Idle);
    assert!(vmax <= s.drive.top_speed + 1.0);
    // Order of the phases.
    let phases: Vec<Phase> = ev.iter().filter_map(|(_, e)| if let Event::Phase(p) = e { Some(*p) } else { None }).collect();
    assert_eq!(
        phases,
        [Phase::Calibrating, Phase::PreRamp, Phase::RampUp, Phase::Cruise, Phase::RampDown, Phase::PostRampDown, Phase::Cooldown, Phase::Idle]
    );
    // Back: from the exit point to Hearth.
    ship.forward = DVec3::X; // reset below
    let mut back = d.clone();
    ship.speed = s.drive.exit_speed;
    back.begin(0, &ship, &s, &[]).unwrap();
    ship.forward = back.path().unwrap().start_dir();
    let (ev, _) = run(&mut back, &mut ship, &s, 200.0);
    assert!(ev.iter().any(|(_, e)| *e == Event::Arrived));
    let home = Drive::exit_point(&s, 0, exit);
    assert!(ship.pos.distance(home) < 1e-6);
}

#[test]
fn short_trip_never_reaches_top_speed() {
    let mut s = sys();
    s.set_distance(300_000.0);
    let (mut d, mut ship) = ready(&s);
    d.begin(1, &ship, &s, &[]).unwrap();
    let (ev, vmax) = run(&mut d, &mut ship, &s, 200.0);
    assert!(ev.iter().any(|(_, e)| *e == Event::Arrived));
    assert!(vmax < s.drive.top_speed * 0.5, "{vmax}");
    assert!(ship.pos.distance(Drive::exit_point(&s, 1, DVec3::new(0.0, 7000.0, 0.0))) < 1e-6);
}

#[test]
fn lost_alignment_aborts_calibration() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.begin(1, &ship, &s, &[]).unwrap();
    let mut evs = Vec::new();
    for i in 0..(60 * 20) {
        if i == 60 * 8 {
            // Turn 20 degrees away in the middle of the calibration.
            ship.forward = (ship.forward + DVec3::Z * 0.36).normalize();
        }
        evs.extend(d.step(DT, &ship, &s, &[]));
    }
    assert!(evs.contains(&Event::Aborted(Abort::CalibrationLost)), "{evs:?}");
    assert_eq!(d.phase, Phase::Idle);
    assert!(d.pose().is_none());
}

#[test]
fn warning_band_pauses_the_gauge() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.begin(1, &ship, &s, &[]).unwrap();
    let aim = ship.forward;
    // Through spool and the delay.
    for _ in 0..(60 * 6) {
        d.step(DT, &ship, &s, &[]);
    }
    assert_eq!(d.phase, Phase::Calibrating);
    let g = d.gauge;
    // 6.5 degrees off: inside the warning band.
    let axis = aim.cross(DVec3::Z).normalize();
    ship.forward = glam::DQuat::from_axis_angle(axis, 6.5f64.to_radians()) * aim;
    for _ in 0..60 {
        d.step(DT, &ship, &s, &[]);
    }
    assert!(d.warning);
    assert_eq!(d.gauge, g);
    assert_eq!(d.phase, Phase::Calibrating);
}

#[test]
fn cancel_while_spooling() {
    let s = sys();
    let (mut d, ship) = ready(&s);
    d.begin(1, &ship, &s, &[]).unwrap();
    for _ in 0..60 {
        d.step(DT, &ship, &s, &[]);
    }
    assert_eq!(d.cancel(), Some(Event::Aborted(Abort::Cancelled)));
    assert_eq!(d.phase, Phase::Idle);
}

#[test]
fn start_refused_when_low_or_blocked() {
    let s = sys();
    let mut d = Drive::new(s.drive.clone());
    // Too low: 500 m above the ground.
    let low = ShipView { pos: DVec3::new(0.0, 5500.0, 0.0), forward: DVec3::X, speed: 0.0 };
    assert_eq!(d.begin(1, &low, &s, &[]), Err(Abort::TooLow));
    // Another ship on the line.
    let ship = ShipView { pos: DVec3::new(0.0, 7000.0, 0.0), forward: DVec3::X, speed: 0.0 };
    let mut t = d.clone();
    t.begin(1, &ship, &s, &[]).unwrap();
    let (mid, _) = t.path().unwrap().at(t.path().unwrap().length() * 0.5);
    let blocker = Obstacle { centre: mid, radius: 50.0 };
    assert!(matches!(d.begin(1, &ship, &s, &[blocker]), Err(Abort::Obstructed(Blocker::Obstacle(0)))));
    // Own planet as target.
    assert_eq!(d.begin(0, &ship, &s, &[]), Err(Abort::NoTarget));
    // Busy / cooling down.
    d.begin(1, &ship, &s, &[]).unwrap();
    assert_eq!(d.begin(1, &ship, &s, &[]), Err(Abort::NotReady));
}

#[test]
fn planet_on_the_line_blocks_unless_the_spline_goes_around() {
    let s = sys();
    let mut d = Drive::new(s.drive.clone());
    // Behind Hearth, seen from Cinder: the straight line would cross Hearth, the tangent
    // start lets the spline leave sideways. The obstruction check walks the curve, not the line.
    let ship = ShipView { pos: DVec3::new(-7000.0, 0.0, 0.0), forward: DVec3::X, speed: 0.0 };
    let r = d.begin(1, &ship, &s, &[]);
    println!("behind the planet: {r:?}");
    assert!(r.is_ok(), "{r:?}");
    // A third planet in the way of the curve blocks.
    let top = ShipView { pos: DVec3::new(0.0, 7000.0, 0.0), forward: DVec3::X, speed: 0.0 };
    let mut probe = Drive::new(s.drive.clone());
    probe.begin(1, &top, &s, &[]).unwrap();
    let (mid, _) = probe.path().unwrap().at(probe.path().unwrap().length() * 0.5);
    let mut s3 = s.clone();
    let mut p = s3.planets[1].clone();
    p.centre = (mid + DVec3::Y * 3000.0).to_array();
    s3.planets.push(p);
    let mut t = Drive::new(s3.drive.clone());
    assert_eq!(t.begin(1, &top, &s3, &[]), Err(Abort::Obstructed(Blocker::Planet(2))));
}

#[test]
fn tunnel_follows_speed() {
    let s = sys();
    assert_eq!(s.drive.tunnel_level(0.0), 0.0);
    assert_eq!(s.drive.tunnel_level(s.drive.vfx_start_speed), 0.0);
    assert_eq!(s.drive.tunnel_level(s.drive.vfx_full_speed), 1.0);
    assert!(s.drive.tunnel_level(1.5e4) > 0.0 && s.drive.tunnel_level(1.5e4) < 1.0);
}

#[test]
fn registry_frame_zones() {
    let s = sys();
    assert_eq!(s.frame_of(DVec3::new(0.0, 7000.0, 0.0)), Some(0));
    assert_eq!(s.frame_of(DVec3::new(12_500_000.0, 7000.0, 0.0)), Some(1));
    assert_eq!(s.frame_of(DVec3::new(6_000_000.0, 0.0, 0.0)), None);
    // Frame zones must stay below half the distance to the next planet.
    assert!(s.planets[0].frame_radius < 12_500_000.0 / 2.0);
}

#[test]
fn path_leaves_and_arrives_along_the_tangent() {
    let s = sys();
    let (d, ship) = ready(&s);
    let mut d = d;
    d.begin(1, &ship, &s, &[]).unwrap();
    let path = d.path().unwrap();
    let len = path.length();
    let chord = d.exit().distance(ship.pos);
    println!("path {:.0} km for a chord of {:.0} km ({:+.2} %)", len / 1000.0, chord / 1000.0, 100.0 * (len / chord - 1.0));
    // End direction is tangent to the arrival sphere at the exit point.
    let (p, dir) = path.at(len);
    let up = (p - s.planets[1].centre()).normalize();
    assert!(dir.dot(up).abs() < 0.05, "arrival not tangent: {}", dir.dot(up));
    // Start direction is tangent to the departure planet (the line runs level with the ground).
    let up0 = (ship.pos - s.planets[0].centre()).normalize();
    assert!(path.start_dir().dot(up0) > -0.05);
    // The detour stays small.
    assert!(len / chord < 1.05, "{}", len / chord);
}
