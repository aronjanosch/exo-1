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

/// Before the start the exit point moves with the ship; the ship lands on the exit point
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

/// Trips of every length of the research note (Star Citizen's short, medium and long buckets
/// scaled to our planets), plus a short hop, end exactly at the exit point.
#[test]
fn trips_of_every_length_arrive() {
    for d in [300_000.0, 2_000_000.0, 12_500_000.0, 62_500_000.0, 187_500_000.0] {
        let mut s = sys();
        s.set_distance(d).unwrap();
        let (mut dr, mut ship) = ready(&s);
        dr.begin(CINDER, &ship, &s, &[]).unwrap();
        let (ev, _) = run(&mut dr, &mut ship, &s, 600.0);
        assert!(ev.iter().any(|(_, e)| *e == Event::Arrived), "no arrival at {d} m");
        assert!(ship.pos.distance(Drive::exit_point(&s, CINDER, DVec3::new(0.0, 7000.0, 0.0))) < 1e-6);
    }
}

// ---- Edge cases and validation (#111, #106 points 1-2, #113 point 3) ----

use crate::load::{Load, Loader, Pending};
use serde_json::{json, Value};

fn with(edit: impl FnOnce(&mut Value)) -> Result<System, String> {
    let mut v: Value = serde_json::from_str(JSON).unwrap();
    edit(&mut v);
    System::from_json(&v.to_string())
}

#[test]
fn shipped_system_is_valid() {
    sys().validate().unwrap();
}

#[test]
fn unknown_fields_are_refused() {
    assert!(with(|v| v["typo"] = json!(1)).is_err());
    assert!(with(|v| v["planets"][0]["radiuss"] = json!(1.0)).is_err());
    assert!(with(|v| v["drive"]["top_sped"] = json!(1.0)).is_err());
    // The comment stays allowed, as a string.
    assert!(with(|v| v["_comment"] = json!("still fine")).is_ok());
}

#[test]
fn planet_count_is_an_error_not_a_panic() {
    assert!(with(|v| v["planets"] = json!([])).is_err());
    let many = with(|v| {
        let p = v["planets"][0].clone();
        v["planets"] = Value::Array(vec![p; 256]);
    });
    assert!(many.is_err());
}

#[test]
fn planet_radii_must_nest() {
    let bad: [(&str, Value); 8] = [
        ("radius", json!(0.0)),
        ("radius", json!(-5.0)),
        // radius < obstruction radius
        ("obstruction_radius", json!(4000.0)),
        ("obstruction_margin", json!(-1.0)),
        // keep-out < arrival radius: the exit point must be outside the keep-out
        ("arrival_radius", json!(5400.0)),
        // arrival < frame radius
        ("frame_radius", json!(12000.0)),
        ("atmosphere_height", json!(-1.0)),
        ("jump_altitude_factor", json!(-1.0)),
    ];
    for (k, x) in bad {
        assert!(with(|v| v["planets"][1][k] = x.clone()).is_err(), "{k} = {x} accepted");
    }
    // A NaN cannot come from JSON, but a centre far out of range can be checked as finite.
    assert!(with(|v| v["planets"][1]["centre"] = json!([1e308, 1e308, 0.0])).is_ok());
}

#[test]
fn drive_values_are_checked() {
    let bad: [(&str, Value); 14] = [
        ("emergency_clear_step", json!(0.0)),
        ("emergency_clear_step", json!(-1000.0)),
        ("exit_speed", json!(0.0)),
        ("top_speed", json!(0.0)),
        ("accel_stage_one", json!(0.0)),
        ("decel_stage_two", json!(-1.0)),
        // exit <= switch <= top
        ("exit_speed", json!(300000.0)),
        ("stage_switch_speed", json!(2000000.0)),
        ("engage_speed", json!(2000000.0)),
        ("spline_tension", json!(0.0)),
        ("calibration_time_min", json!(0.0)),
        // the warning band lies outside the calibration angle
        ("warning_angle", json!(4.0)),
        ("vfx_full_speed", json!(6250.0)),
        ("emergency_drop_time", json!(0.0)),
    ];
    for (k, x) in bad {
        assert!(with(|v| v["drive"][k] = x.clone()).is_err(), "drive.{k} = {x} accepted");
    }
}

/// A drive fed bad values anyway (built by hand, not from a file) must still end: on rails it
/// always reaches the end point.
#[test]
fn zero_exit_speed_still_arrives() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.cfg.exit_speed = 0.0;
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    let (ev, _) = run(&mut d, &mut ship, &s, 200.0);
    assert!(ev.iter().any(|(_, e)| *e == Event::Arrived), "stuck in {:?}", d.phase);
}

#[test]
fn zero_exit_speed_drop_still_ends() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.cfg.exit_speed = 0.0;
    d.cfg.emergency_hold_time = 0.0;
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    while d.phase != Phase::Cruise {
        d.step(DT, &ship, &s, &[]);
        if let Some((p, _)) = d.pose() {
            ship.pos = p;
        }
    }
    d.hold_exit(true, DT, &s, &[]).unwrap();
    let (ev, _) = run(&mut d, &mut ship, &s, 60.0);
    assert!(ev.iter().any(|(_, e)| *e == Event::DroppedOut), "stuck in {:?}", d.phase);
}

#[test]
fn zero_clear_step_does_not_hang_the_drop() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    while d.phase != Phase::Cruise {
        d.step(DT, &ship, &s, &[]);
        if let Some((p, _)) = d.pose() {
            ship.pos = p;
        }
    }
    d.cfg.emergency_hold_time = 0.0;
    let mut probe = d.clone();
    probe.hold_exit(true, DT, &s, &[]).unwrap();
    let other = Obstacle { centre: probe.drop_point().unwrap(), radius: 50.0 };
    d.cfg.emergency_clear_step = 0.0;
    // Returns (it used to loop forever).
    assert!(d.hold_exit(true, DT, &s, &[other]).is_some());
}

/// A ship that got to the exit point first (it was not there at the start) moves the end of the
/// rails back along the path: the hand-over is never inside it.
#[test]
fn arrival_stops_short_of_a_ship_at_the_exit() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    let exit = d.exit().unwrap();
    let other = Obstacle { centre: exit, radius: 20.0 };
    let mut arrived = false;
    let mut t = 0.0;
    while t < 200.0 && !arrived {
        t += DT;
        // The other ship shows up once this one is on its way.
        let obs: &[Obstacle] = if d.phase.on_rails() { std::slice::from_ref(&other) } else { &[] };
        let ev = d.step(DT, &ship, &s, obs);
        if let Some((p, v)) = d.pose() {
            ship.pos = p;
            ship.speed = v.length();
        }
        arrived = ev.contains(&Event::Arrived);
    }
    assert!(arrived);
    let gap = ship.pos.distance(other.centre);
    println!("handed over {gap:.0} m from the ship at the exit");
    assert!(gap >= other.radius, "{gap}");
    assert!(ship.pos.distance(d.exit().unwrap()) < 1e-6, "exit() names the hand-over point");
    assert!((ship.speed - s.drive.exit_speed).abs() < 1e-6);
}

#[test]
fn zero_tension_path_has_a_start_direction() {
    let p = Path::new(DVec3::ZERO, None, DVec3::X * 1000.0, DVec3::X, 0.0);
    assert!((p.start_dir() - DVec3::X).length() < 1e-9, "{}", p.start_dir());
    let q = Path::new(DVec3::ZERO, None, DVec3::ZERO, DVec3::ZERO, 0.25);
    assert!(q.start_dir().is_finite() && (q.start_dir().length() - 1.0).abs() < 1e-9);
}

#[test]
fn zero_tension_calibration_still_needs_the_aim() {
    let s = sys();
    let (mut d, mut ship) = ready(&s);
    d.cfg.spline_tension = 0.0;
    ship.forward = -ship.forward;
    d.begin(CINDER, &ship, &s, &[]).unwrap();
    let (ev, _) = run(&mut d, &mut ship, &s, 30.0);
    assert!(ev.iter().any(|(_, e)| *e == Event::Aborted(Abort::CalibrationLost)), "{ev:?}");
}

// ---- Which planet the simulation holds (Loader) ----

const THIRD: PlanetId = PlanetId(2);

/// Three planets: a third one beside the Hearth-Cinder line, its frame zone across the path but
/// its keep-out far off it.
fn sys3() -> System {
    with(|v| {
        let mut p = v["planets"][1].clone();
        p["name"] = json!("Third");
        p["centre"] = json!([6250000.0, 600000.0, 0.0]);
        v["planets"].as_array_mut().unwrap().push(p);
    })
    .unwrap()
}

/// A background generation that takes `ticks` ticks.
#[derive(Default)]
struct FakePending {
    job: Option<(PlanetId, u32)>,
    ticks: u32,
    /// Every generation started.
    started: Vec<PlanetId>,
}

impl FakePending {
    fn state(&self) -> Pending {
        match self.job {
            None => Pending::None,
            Some((p, 0)) => Pending::Ready(p),
            Some((p, _)) => Pending::Running(p),
        }
    }

    fn tick(&mut self) {
        if let Some((_, n)) = &mut self.job {
            *n = n.saturating_sub(1);
        }
    }
}

/// Flies a whole warp with the loader; returns the swaps (planet, ready when taken) and the
/// generations started. `drop_at`: hold the exit key from this share of the path on.
fn fly_with_loader(s: &System, from: DVec3, to: PlanetId, gen_ticks: u32, drop_at: Option<f64>) -> (Vec<(PlanetId, bool)>, FakePending, PlanetId, DVec3) {
    let mut d = Drive::new(s.drive.clone());
    let mut ship = ShipView { pos: from, forward: DVec3::X, speed: 0.0 };
    let mut probe = d.clone();
    probe.begin(to, &ship, s, &[]).unwrap();
    ship.forward = probe.path().unwrap().start_dir();
    d.begin(to, &ship, s, &[]).unwrap();
    let mut current = s.nearest(from);
    let mut loader = Loader::default();
    let mut pending = FakePending { ticks: gen_ticks, ..Default::default() };
    let mut swaps = Vec::new();
    let mut t = 0.0;
    d.cfg.emergency_hold_time = 0.0;
    while t < 220.0 {
        t += DT;
        let mut ev = Vec::new();
        if let (Some(share), Some(p)) = (drop_at, d.path()) {
            let pressed = d.phase.on_rails() && d.phase != Phase::EmergencyDrop && ship.pos.distance(from) > p.length() * share;
            ev.extend(d.hold_exit(pressed, DT, s, &[]));
        }
        ev.extend(d.step(DT, &ship, s, &[]));
        if let Some((p, v)) = d.pose() {
            ship.pos = p;
            ship.speed = v.length();
        }
        let dropped = ev.contains(&Event::DroppedOut);
        match loader.step(s, current, ship.pos, &d, dropped, pending.state()) {
            Load::Keep => {}
            Load::Start(p) => {
                pending.job = Some((p, pending.ticks));
                pending.started.push(p);
            }
            Load::Discard => pending.job = None,
            Load::Swap(p) => {
                let ready = pending.state() == Pending::Ready(p);
                pending.job = None;
                swaps.push((p, ready));
                current = p;
            }
        }
        pending.tick();
        if d.phase == Phase::Idle && t > 1.0 {
            break;
        }
    }
    (swaps, pending, current, ship.pos)
}

const ORBIT: DVec3 = DVec3::new(0.0, 7000.0, 0.0);

#[test]
fn passing_a_third_planet_keeps_generating_the_target() {
    let s = sys3();
    // The path runs through the third planet's frame zone.
    let mut d = Drive::new(s.drive.clone());
    d.begin(CINDER, &ShipView { pos: ORBIT, forward: DVec3::X, speed: 0.0 }, &s, &[]).unwrap();
    let path = d.path().unwrap();
    assert!((0..=100).any(|i| s.frame_of(path.at(path.length() * i as f64 / 100.0).0) == Some(THIRD)));

    let (swaps, pending, current, _) = fly_with_loader(&s, ORBIT, CINDER, 40, None);
    assert_eq!(swaps, vec![(CINDER, true)], "one swap, to the target, never blocking");
    assert_eq!(pending.started, vec![CINDER]);
    assert_eq!(current, CINDER);
}

#[test]
fn a_slow_generation_waits_on_rails_instead_of_blocking() {
    let s = sys();
    // Takes longer than the flight from the ramp-up to the frame zone (about 1 s at 1e6 m/s).
    let (swaps, ..) = fly_with_loader(&s, ORBIT, CINDER, 60 * 9, None);
    assert_eq!(swaps, vec![(CINDER, true)], "swapped only once the generation was done");
}

#[test]
fn a_generation_not_done_at_the_arrival_is_taken_there() {
    let s = sys();
    // Much longer than the whole flight: the arrival must not wait for it forever.
    let (swaps, ..) = fly_with_loader(&s, ORBIT, CINDER, 60 * 600, None);
    assert_eq!(swaps, vec![(CINDER, false)]);
}

#[test]
fn drop_near_a_third_planet_generates_it_in_the_background() {
    let s = sys3();
    // Drop in the middle of the flight: open space, nearest is the third planet.
    let (swaps, pending, current, pos) = fly_with_loader(&s, ORBIT, CINDER, 30, Some(0.45));
    assert_eq!(s.frame_of(pos), None, "dropped in open space");
    assert_eq!(s.nearest(pos), THIRD);
    assert_eq!(swaps, vec![(THIRD, true)], "no generation in the swap tick");
    assert_eq!(pending.started, vec![CINDER, THIRD]);
    assert_eq!(current, THIRD);
}

#[test]
fn drop_near_the_old_planet_frees_the_target() {
    let s = sys();
    let (swaps, pending, current, pos) = fly_with_loader(&s, ORBIT, CINDER, 30, Some(0.1));
    assert_eq!(s.nearest(pos), HEARTH);
    assert!(swaps.is_empty());
    assert_eq!(current, HEARTH);
    assert!(pending.job.is_none(), "Cinder still held in memory");
}

#[test]
fn a_teleport_into_a_frame_zone_swaps_at_once() {
    let s = sys();
    let mut l = Loader::default();
    let d = Drive::new(s.drive.clone());
    let at_cinder = s.planets[1].centre() + DVec3::Y * 7000.0;
    assert_eq!(l.step(&s, HEARTH, at_cinder, &d, false, Pending::None), Load::Swap(CINDER));
    // Already the simulation's planet: nothing to do; a stale generation goes.
    assert_eq!(l.step(&s, CINDER, at_cinder, &d, false, Pending::None), Load::Keep);
    assert_eq!(l.step(&s, CINDER, at_cinder, &d, false, Pending::Ready(HEARTH)), Load::Discard);
}
