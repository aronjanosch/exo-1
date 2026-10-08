use super::*;
use crate::path::Blocker;
use crate::system::PlanetId;

const HEARTH: PlanetId = PlanetId(0);
const CINDER: PlanetId = PlanetId(1);
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
        t.begin(CINDER, &ship, sys, &[]).unwrap();
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
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    let (ev, vmax) = run(&mut d, &mut ship, &s, 200.0);
    let arrived = ev.iter().find(|(_, e)| *e == Event::Arrived).expect("arrived").0;
    let start = ev.iter().find(|(_, e)| *e == Event::Phase(Phase::RampUp)).unwrap().0;
    println!("warp A->B: start {start:.1} s, flight {:.1} s, top speed {:.0} m/s", arrived - start, vmax);
    let exit = Drive::exit_point(&s, CINDER, DVec3::new(0.0, 7000.0, 0.0));
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
    back.begin(HEARTH, &ship, &s, &[]).unwrap();
    ship.forward = back.path().unwrap().start_dir();
    let (ev, _) = run(&mut back, &mut ship, &s, 200.0);
    assert!(ev.iter().any(|(_, e)| *e == Event::Arrived));
    let home = Drive::exit_point(&s, HEARTH, exit);
    assert!(ship.pos.distance(home) < 1e-6);
}

#[test]
fn short_trip_never_reaches_top_speed() {
    let mut s = sys();
    s.set_distance(300_000.0).unwrap();
    let (mut d, mut ship) = ready(&s);
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    let (ev, vmax) = run(&mut d, &mut ship, &s, 200.0);
    assert!(ev.iter().any(|(_, e)| *e == Event::Arrived));
    assert!(vmax < s.drive.top_speed * 0.5, "{vmax}");
    assert!(ship.pos.distance(Drive::exit_point(&s, CINDER, DVec3::new(0.0, 7000.0, 0.0))) < 1e-6);
}

#[test]
fn lost_alignment_aborts_calibration() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.begin(CINDER, &ship, &s, &[]).unwrap();
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
    d.begin(CINDER, &ship, &s, &[]).unwrap();
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
    d.begin(CINDER, &ship, &s, &[]).unwrap();
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
    // Too low: below 1.5 atmosphere heights (1,800 m), also just above the atmosphere top.
    let limit = s.planets[0].min_jump_altitude();
    assert_eq!(limit, 1800.0);
    for alt in [500.0, 1300.0, limit - 1.0] {
        let low = ShipView { pos: DVec3::new(0.0, 5000.0 + alt, 0.0), forward: DVec3::X, speed: 0.0 };
        assert_eq!(d.begin(CINDER, &low, &s, &[]), Err(Abort::TooLow), "{alt} m");
    }
    let ok = ShipView { pos: DVec3::new(0.0, 5000.0 + limit + 1.0, 0.0), forward: DVec3::X, speed: 0.0 };
    assert!(d.clone().begin(CINDER, &ok, &s, &[]).is_ok());
    // Another ship on the line.
    let ship = ShipView { pos: DVec3::new(0.0, 7000.0, 0.0), forward: DVec3::X, speed: 0.0 };
    let mut t = d.clone();
    t.begin(CINDER, &ship, &s, &[]).unwrap();
    let (mid, _) = t.path().unwrap().at(t.path().unwrap().length() * 0.5);
    let blocker = Obstacle { centre: mid, radius: 50.0 };
    assert!(matches!(d.begin(CINDER, &ship, &s, &[blocker]), Err(Abort::Obstructed(Blocker::Obstacle(0)))));
    // Own planet as target.
    assert_eq!(d.begin(HEARTH, &ship, &s, &[]), Err(Abort::NoTarget));
    // Busy / cooling down.
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    assert_eq!(d.begin(CINDER, &ship, &s, &[]), Err(Abort::NotReady));
}

#[test]
fn planet_on_the_line_blocks_unless_the_spline_goes_around() {
    let s = sys();
    let mut d = Drive::new(s.drive.clone());
    // Behind Hearth, seen from Cinder: the straight line would cross Hearth, the tangent
    // start lets the spline leave sideways. The obstruction check walks the curve, not the line.
    let ship = ShipView { pos: DVec3::new(-7000.0, 0.0, 0.0), forward: DVec3::X, speed: 0.0 };
    let r = d.begin(CINDER, &ship, &s, &[]);
    println!("behind the planet: {r:?}");
    assert!(r.is_ok(), "{r:?}");
    // A third planet in the way of the curve blocks.
    let top = ShipView { pos: DVec3::new(0.0, 7000.0, 0.0), forward: DVec3::X, speed: 0.0 };
    let mut probe = Drive::new(s.drive.clone());
    probe.begin(CINDER, &top, &s, &[]).unwrap();
    let (mid, _) = probe.path().unwrap().at(probe.path().unwrap().length() * 0.5);
    let mut s3 = s.clone();
    let mut p = s3.planets[1].clone();
    p.centre = (mid + DVec3::Y * 3000.0).to_array();
    s3.planets.push(p);
    let mut t = Drive::new(s3.drive.clone());
    assert_eq!(t.begin(CINDER, &top, &s3, &[]), Err(Abort::Obstructed(Blocker::Planet(PlanetId(2)))));
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
    assert_eq!(s.frame_of(DVec3::new(0.0, 7000.0, 0.0)), Some(HEARTH));
    assert_eq!(s.frame_of(DVec3::new(12_500_000.0, 7000.0, 0.0)), Some(CINDER));
    assert_eq!(s.frame_of(DVec3::new(6_000_000.0, 0.0, 0.0)), None);
    // Frame zones must stay below half the distance to the next planet.
    assert!(s.planets[0].frame_radius < 12_500_000.0 / 2.0);
    assert_eq!(s.id(1), Some(CINDER));
    assert_eq!(s.id(2), None);
}

#[test]
fn path_leaves_along_the_tangent_and_arrives_facing_the_centre() {
    let s = sys();
    let (d, ship) = ready(&s);
    let mut d = d;
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    let path = d.path().unwrap();
    let len = path.length();
    let exit = d.exit().unwrap();
    let chord = exit.distance(ship.pos);
    println!("path {:.0} km for a chord of {:.0} km ({:+.2} %)", len / 1000.0, chord / 1000.0, 100.0 * (len / chord - 1.0));
    // The exit point is on the arrival sphere, and the path comes in radially: the direction of
    // travel at the end points at the centre.
    let c = s.planets[1].centre();
    assert!((exit.distance(c) - s.planets[1].arrival_radius).abs() < 1e-6);
    let (p, dir) = path.at(len);
    let off = dir.angle_between(c - p).to_degrees();
    println!("arrival: {off:.4} deg off the centre");
    assert!(off < 0.1, "{off}");
    // The last 50 km are radial too (within 1 degree), not only the last point.
    let (q, dq) = path.at(len - 50_000.0);
    assert!(dq.angle_between(c - q).to_degrees() < 1.0);
    // Start direction is tangent to the departure planet (the line runs level with the ground).
    let up0 = (ship.pos - s.planets[0].centre()).normalize();
    assert!(path.start_dir().dot(up0) > -0.05);
    // The detour stays small.
    assert!(len / chord < 1.05, "{}", len / chord);
}

/// F4: before the start the exit point moves with the ship; the ship lands on the exit point
/// of the path it flies, within 1e-6 m.
#[test]
fn exit_point_follows_the_path_until_the_start() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    let first = d.exit().unwrap();
    // The ship drifts 300 m sideways while spooling.
    for _ in 0..60 {
        ship.pos += DVec3::Z * 5.0;
        d.step(DT, &ship, &s, &[]);
    }
    let moved = d.exit().unwrap();
    assert!(moved.distance(first) > 1e-4, "exit point did not follow");
    run(&mut d, &mut ship, &s, 200.0);
    assert!(ship.pos.distance(moved) < 1e-6, "{}", ship.pos.distance(moved));
}

#[test]
fn emergency_exit_drops_into_open_space() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    let mut t = 0.0;
    let (mut t_hold, mut t_drop, mut t_out) = (None, None, None);
    let mut v_at_drop = 0.0;
    let mut events = Vec::new();
    while t < 200.0 && d.phase != Phase::Idle || t < 1.0 {
        t += DT;
        // Hold the key from the middle of the cruise on.
        let half = d.path().map(|p| p.length() * 0.5).unwrap_or(f64::MAX);
        let pressed = d.phase.on_rails() && ship.pos.distance(DVec3::new(0.0, 7000.0, 0.0)) > half && t_drop.is_none();
        if pressed && t_hold.is_none() {
            t_hold = Some(t);
        }
        if let Some(e) = d.hold_exit(pressed, DT, &s, &[]) {
            t_drop = Some(t);
            v_at_drop = d.speed();
            events.push(e);
        }
        let ev = d.step(DT, &ship, &s, &[]);
        if ev.contains(&Event::DroppedOut) {
            t_out = Some(t);
        }
        events.extend(ev);
        if let Some((p, v)) = d.pose() {
            ship.pos = p;
            ship.speed = v.length();
        }
    }
    let (t_hold, t_drop, t_out) = (t_hold.unwrap(), t_drop.expect("drop"), t_out.expect("dropped out"));
    println!(
        "emergency exit: held {:.2} s, dropped from {:.0} km/s to {:.0} m/s in {:.2} s, at {:.0} km from Hearth, {:.0} km from Cinder",
        t_drop - t_hold,
        v_at_drop / 1000.0,
        ship.speed,
        t_out - t_drop,
        ship.pos.distance(s.planets[0].centre()) / 1000.0,
        ship.pos.distance(s.planets[1].centre()) / 1000.0
    );
    assert!(!events.contains(&Event::Arrived));
    assert!((t_drop - t_hold - s.drive.emergency_hold_time).abs() < 2.0 * DT);
    assert!((t_out - t_drop - s.drive.emergency_drop_time).abs() < 0.1, "{}", t_out - t_drop);
    assert!((ship.speed - s.drive.exit_speed).abs() < 1e-6, "{}", ship.speed);
    assert_eq!(d.phase, Phase::Idle);
    // Open space: outside every keep-out radius and every frame zone.
    for p in &s.planets {
        assert!(ship.pos.distance(p.centre()) > p.keep_out());
    }
    assert_eq!(s.frame_of(ship.pos), None);
    // From there a new jump to Cinder starts straight (no planet to leave from) and arrives.
    ship.forward = DVec3::X;
    let target = s.effective_target(CINDER, ship.pos);
    assert_eq!(target, CINDER);
    d.begin(target, &ship, &s, &[]).unwrap();
    ship.forward = d.path().unwrap().start_dir();
    let (ev, _) = run(&mut d, &mut ship, &s, 200.0);
    assert!(ev.iter().any(|(_, e)| *e == Event::Arrived));
}

#[test]
fn emergency_drop_point_moves_past_a_ship() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    // Fly to the cruise.
    while d.phase != Phase::Cruise {
        d.step(DT, &ship, &s, &[]);
        if let Some((p, v)) = d.pose() {
            ship.pos = p;
            ship.speed = v.length();
        }
    }
    // Where a drop would end now, with a ship parked there.
    let mut probe = d.clone();
    probe.cfg.emergency_hold_time = 0.0;
    probe.hold_exit(true, DT, &s, &[]).unwrap();
    let free = probe.drop_point().unwrap();
    let other = Obstacle { centre: free, radius: 50.0 };
    d.cfg.emergency_hold_time = 0.0;
    d.hold_exit(true, DT, &s, &[other]).unwrap();
    let moved = d.drop_point().unwrap();
    println!("drop point moved {:.0} m past the ship", moved.distance(free));
    assert!(moved.distance(free) >= 50.0);
}

#[test]
fn effective_target_skips_the_planet_you_are_at() {
    let s = sys();
    let at_hearth = DVec3::new(0.0, 7000.0, 0.0);
    assert_eq!(s.effective_target(HEARTH, at_hearth), CINDER);
    assert_eq!(s.effective_target(CINDER, at_hearth), CINDER);
    let between = DVec3::new(6_000_000.0, 0.0, 0.0);
    assert_eq!(s.effective_target(HEARTH, between), HEARTH);
}

#[test]
fn distance_setting_keeps_frame_zones_apart() {
    let mut s = sys();
    let notes = s.set_distance(1_500_000.0).unwrap();
    println!("{notes:?}");
    assert_eq!(notes.len(), 2);
    for p in &s.planets {
        assert!(p.frame_radius < 750_000.0);
    }
    assert!(sys().set_distance(20_000.0).is_err());
    assert!(sys().set_distance(12_500_000.0).unwrap().is_empty());
}

/// The start values at the three trip lengths of the research note (Star Citizen's short,
/// medium and long buckets scaled to our planets), plus a short hop: spool and calibration,
/// flight, top speed reached, sky size of the target. Printed for the spike report.
#[test]
fn trip_times_by_distance() {
    println!("distance km | sky arcmin | start->engage s | flight s | top km/s | cruise s");
    for d in [300_000.0, 2_000_000.0, 12_500_000.0, 62_500_000.0, 187_500_000.0] {
        let mut s = sys();
        s.set_distance(d).unwrap();
        let (mut dr, mut ship) = ready(&s);
        dr.begin(CINDER, &ship, &s, &[]).unwrap();
        let (ev, vmax) = run(&mut dr, &mut ship, &s, 600.0);
        let t = |p: Phase| ev.iter().find(|(_, e)| *e == Event::Phase(p)).map(|(t, _)| *t);
        let arrived = ev.iter().find(|(_, e)| *e == Event::Arrived).expect("arrived").0;
        let ramp = t(Phase::RampUp).unwrap();
        let cruise = match (t(Phase::Cruise), t(Phase::RampDown)) {
            (Some(a), Some(b)) => b - a,
            _ => 0.0,
        };
        let arcmin = 2.0 * (s.planets[1].radius / d).atan().to_degrees() * 60.0;
        println!("{:11.0} | {arcmin:10.2} | {ramp:15.1} | {:8.1} | {:8.0} | {cruise:8.1}", d / 1000.0, arrived - ramp, vmax / 1000.0);
        assert!(ship.pos.distance(Drive::exit_point(&s, CINDER, DVec3::new(0.0, 7000.0, 0.0))) < 1e-6);
    }
}
