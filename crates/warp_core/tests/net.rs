//! Spike 11, step 6: a ship warps, another client sees it through the real snapshot path
//! (wire format, interpolation buffer, 150 ms playout). Measures the position error against the
//! truth at the shown time, by phase, and what the swap of the sender's planet does.
use glam::{DQuat, DVec3};
use net_core::buffer::{Buffer, Mode};
use net_core::snapshot::Snapshot;
use warp_core::{Drive, Event, Phase, ShipView, System};

const DT: f64 = 1.0 / 60.0;
const PLAYOUT: f64 = 0.15;
const JSON: &str = include_str!("../../../content/system/system.json");

/// Truth per tick: time, position, velocity, drive phase.
fn truth(sys: &System) -> Vec<(f64, DVec3, DVec3, Phase)> {
    let start = DVec3::new(0.0, 7000.0, 0.0);
    let mut d = Drive::new(sys.drive.clone());
    let mut ship = ShipView { pos: start, forward: DVec3::X, speed: 0.0 };
    d.begin(1, &ship, sys, &[]).unwrap();
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
        let ev = d.step(DT, &ship, sys, &[]);
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
        let _ = ev.iter().any(|e| *e == Event::Arrived);
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
    let mut active = 0usize;
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
            let mut s = Snapshot::new(2, t, p - centres[active], v, DQuat::IDENTITY);
            s.planet = active as u32;
            // The walker stands in the cabin.
            s.frame = net_core::snapshot::FrameKind::Ship;
            s.frame_id = 2;
            s.wp = DVec3::new(0.0, 0.3, -2.0);
            s.wv = DVec3::ZERO;
            s.seq = seq;
            seq += 1;
            if loss_every == 0 || (seq as usize) % loss_every != 0 {
                let mut r = Snapshot::decode(&s.encode()).expect("decodes");
                if normalise {
                    r.to_frame_of(&centres, 0);
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
