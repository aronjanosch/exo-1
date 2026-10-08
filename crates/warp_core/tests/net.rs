//! A ship warps, another client sees it through the real snapshot path
//! (wire format, interpolation buffer, 150 ms playout). Measures the position error against the
//! truth at the shown time, by phase, and what the swap of the sender's planet does.
use glam::{DQuat, DVec3};
use net_core::buffer::{Buffer, Mode};
use net_core::snapshot::{Limits, Snapshot};
use warp_core::{Drive, Phase, PlanetId, ShipView, System};

const DT: f64 = 1.0 / 60.0;
const PLAYOUT: f64 = 0.15;
const JSON: &str = include_str!("../../../content/system/system.json");

/// Truth per tick: time, position, velocity, drive phase.
fn truth(sys: &System) -> Vec<(f64, DVec3, DVec3, Phase)> {
    let start = DVec3::new(0.0, 7000.0, 0.0);
    let mut d = Drive::new(sys.drive.clone());
    let mut ship = ShipView { pos: start, forward: DVec3::X, speed: 0.0 };
    d.begin(PlanetId(1), &ship, sys, &[]).unwrap();
    ship.forward = d.path().unwrap().start_dir();
    let mut out = Vec::new();
    let mut t = 0.0;
    // One second at rest, then the whole drive, then 5 s at exit speed along the exit heading.
    for _ in 0..60 {
        out.push((t, start, DVec3::ZERO, Phase::Idle));
        t += DT;
    }
    let (mut pos, mut vel) = (start, DVec3::ZERO);
    let mut after = 0;
    loop {
        d.step(DT, &ship, sys, &[]);
        match d.pose() {
            Some((p, v)) => {
                pos = p;
                vel = v;
                ship.pos = p;
                ship.speed = v.length();
            }
            None if d.phase == Phase::PostRampDown || d.phase == Phase::Cooldown => {
                pos += vel * DT;
                ship.pos = pos;
                after += 1;
            }
            None => {}
        }
        // The first tick after arrival carries the exit pose; from then on the ship coasts.
        out.push((t, pos, if d.phase.on_rails() || after > 0 { vel } else { DVec3::ZERO }, d.phase));
        t += DT;
        if after > 300 {
            break;
        }
    }
    out
}

struct Stat {
    max: f64,
    sum: f64,
    n: u32,
}

fn run(sys: &System, loss_every: usize, normalise: bool, label: &str) -> Vec<(Phase, f64, f64, u32)> {
    let tr = truth(sys);
    let centres: Vec<DVec3> = sys.planets.iter().map(|p| p.centre()).collect();
    // The sender's planet: the one whose frame zone it entered last (as in the game).
    let mut active = PlanetId(0);
    let mut buf = Buffer::new();
    let mut stats: Vec<(Phase, Stat)> = Vec::new();
    let mut holds = 0;
    let mut seq = 0u32;
    for (i, &(t, p, v, _)) in tr.iter().enumerate() {
        if i % 2 == 0 {
            if let Some(f) = sys.frame_of(p) {
                active = f;
            }
            // Planet-relative on the wire, like the game's sender.
            let mut s = Snapshot::new(2, t, p - centres[active.index()], v, DQuat::IDENTITY);
            s.planet = active.wire();
            // The walker stands in the cabin.
            s.frame = net_core::snapshot::FrameKind::Ship;
            s.frame_id = 2;
            s.wp = DVec3::new(0.0, 0.3, -2.0);
            s.wv = DVec3::ZERO;
            s.seq = seq;
            seq += 1;
            if loss_every == 0 || !(seq as usize).is_multiple_of(loss_every) {
                let mut r = Snapshot::decode(&s.encode()).expect("decodes");
                if normalise {
                    assert!(r.to_frame_of(&centres, 0));
                }
                buf.push(r);
            }
        }
        let target = t - PLAYOUT;
        if target < 0.2 {
            continue;
        }
        let Some(sample) = (if loss_every > 0 { buf.sample_extrapolated(target, 0.1) } else { buf.sample(target) }) else { continue };
        let j = i - (PLAYOUT / DT).round() as usize;
        let (_, tp, _, phase) = tr[j];
        let err = (centres[sample.s.planet as usize] + sample.s.p).distance(tp);
        if sample.mode == Mode::Hold {
            holds += 1;
        }
        match stats.iter_mut().find(|(ph, _)| *ph == phase) {
            Some((_, s)) => {
                s.max = s.max.max(err);
                s.sum += err;
                s.n += 1;
            }
            None => stats.push((phase, Stat { max: err, sum: err, n: 1 })),
        }
    }
    println!("{label}: error of the shown position against the truth at the shown time (playout {} ms), holds {holds}", (PLAYOUT * 1000.0) as u32);
    let mut rows = Vec::new();
    for (ph, s) in &stats {
        println!("  {ph:?}: max {:.3} m, mean {:.3} m over {} ticks", s.max, s.sum / s.n as f64, s.n);
        rows.push((*ph, s.max, s.sum / s.n as f64, s.n));
    }
    rows
}

#[test]
fn remote_ship_warping_is_shown_within_metres() {
    let sys = System::from_json(JSON).unwrap();
    let tr = truth(&sys);
    let top = tr.iter().map(|t| t.2.length()).fold(0.0, f64::max);
    println!("truth: {:.1} s, top speed {:.0} m/s, {:.0} km per snapshot interval at top speed", tr.len() as f64 * DT, top, top * 2.0 * DT / 1000.0);
    let before = run(&sys, 0, false, "ideal link, 30 Hz, each snapshot in its sender's planet frame");
    let worst_before = before.iter().map(|r| r.1).fold(0.0, f64::max);
    let ideal = run(&sys, 0, true, "ideal link, 30 Hz, normalised to one frame on receipt");
    for (ph, max, ..) in &ideal {
        assert!(*max < 25.0, "{ph:?}: {max} m");
    }
    println!("worst error without normalising: {worst_before:.0} m (the swap of the sender's planet)");
    // One packet in ten lost, display-only extrapolation of 100 ms.
    run(&sys, 10, true, "10 % loss, extrapolation 100 ms");
    // What the 150 ms playout means for where the ship is drawn at top speed.
    println!("display lag at top speed: {:.0} km behind the true position", top * PLAYOUT / 1000.0);
}

/// The receiver's limits as the game derives them from its system.
fn limits(sys: &System) -> Limits {
    Limits { planets: sys.planets.len() as u32, ship_position: sys.max_ship_offset(), ship_speed: sys.max_ship_speed() }
}

/// At the longest trip of the research table (187,500 km) every snapshot of the flight is
/// admitted by limits derived from the loaded system.
#[test]
fn longest_trip_snapshots_are_admitted() {
    let mut sys = System::from_json(JSON).unwrap();
    sys.set_distance(187_500_000.0).unwrap();
    let lim = limits(&sys);
    let tr = truth(&sys);
    let centres: Vec<DVec3> = sys.planets.iter().map(|p| p.centre()).collect();
    let mut active = PlanetId(0);
    let (mut far, mut fast, mut n) = (0.0f64, 0.0f64, 0);
    for &(t, p, v, _) in &tr {
        if let Some(f) = sys.frame_of(p) {
            active = f;
        }
        let mut s = Snapshot::new(2, t, p - centres[active.index()], v, DQuat::IDENTITY);
        s.planet = active.wire();
        s.frame = net_core::snapshot::FrameKind::Ship;
        s.frame_id = 2;
        s.wp = DVec3::new(0.0, 0.3, -2.0);
        s.wv = DVec3::ZERO;
        let r = Snapshot::decode(&s.encode()).expect("decodes");
        assert!(lim.admits(&r), "t {t:.2}: {:.0} m out at {:.0} m/s, limits {lim:?}", r.p.length(), r.v.length());
        far = far.max(r.p.length());
        fast = fast.max(r.v.length());
        n += 1;
    }
    println!("187,500 km: {n} snapshots admitted, farthest {:.0} km of {:.0} km allowed, fastest {:.0} of {:.0} km/s", far / 1000.0, lim.ship_position / 1000.0, fast / 1000.0, lim.ship_speed / 1000.0);
    assert!(far > 1.0e8);
}
