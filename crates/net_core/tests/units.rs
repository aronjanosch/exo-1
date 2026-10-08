//! Port of the unit checks of spike 4 (test.gd `_unit_checks`) plus wire, link and clock.
use glam::{DQuat, DVec3};
use net_core::buffer::{Buffer, Mode};
use net_core::clock::ClockSync;
use net_core::link::Link;
use net_core::snapshot::{FrameKind, Snapshot, SIZE};
use net_core::wire::Packet;
use net_core::{to_planet, to_world};

fn snap() -> Snapshot {
    Snapshot::new(1, 1.0, DVec3::new(1.0, 5000.0, 3.0), DVec3::new(10.0, 0.0, 0.0), DQuat::IDENTITY)
}

#[test]
fn fixed_148_byte_roundtrip() {
    let mut a = snap();
    a.lag = 0.4;
    let wire = a.encode();
    assert_eq!(wire.len(), 148);
    let b = Snapshot::decode(&wire).unwrap();
    assert_eq!((b.p, b.v, b.owner, b.t), (a.p, a.v, a.owner, a.t));
    assert!((b.lag - 0.4).abs() < 0.5 / 255.0, "cabin gravity as one byte: {}", b.lag);
}

#[test]
fn position_keeps_f64_precision_far_out() {
    // 200 km from the origin plus a sub-millimetre part: f32 would lose it, f64 keeps it.
    let mut a = snap();
    a.p = DVec3::new(199_999.7500012345, 5000.5, 0.125);
    let b = Snapshot::decode(&a.encode()).unwrap();
    assert_eq!(b.p, a.p);
}

#[test]
fn rejects_invalid() {
    let a = snap();
    assert!(Snapshot::decode(&[1, 2]).is_none(), "truncated");
    let mut wire = a.encode();
    wire[24..32].copy_from_slice(&f64::NAN.to_le_bytes());
    assert!(Snapshot::decode(&wire).is_none(), "non-finite timestamp");
    let mut s = a;
    s.frame = FrameKind::Ship;
    assert!(Snapshot::decode(&s.encode()).is_none(), "ship frame without parent id");
    let mut s = a;
    s.owner = 9;
    assert!(Snapshot::decode(&s.encode()).is_none(), "unknown owner");
    let mut s = a;
    s.p = DVec3::new(2.0e8, 0.0, 0.0);
    assert!(Snapshot::decode(&s.encode()).is_none(), "absurd position");
    let mut s = a;
    s.wp = DVec3::new(2.0e6, 0.0, 0.0);
    assert!(Snapshot::decode(&s.encode()).is_none(), "absurd walker position");
    let mut wire = a.encode();
    wire[68..84].fill(0);
    assert!(Snapshot::decode(&wire).is_none(), "zero quaternion");
    let mut wire = a.encode();
    wire[0] = 9;
    assert!(Snapshot::decode(&wire).is_none(), "wrong version");
    let mut wire = a.encode();
    wire[144..148].copy_from_slice(&256u32.to_le_bytes());
    assert!(Snapshot::decode(&wire).is_none(), "cabin gravity out of range");
    let mut long = a.encode().to_vec();
    long.push(0);
    assert!(Snapshot::decode(&long).is_none(), "wrong size");
    assert_eq!(SIZE, 148);
}

#[test]
fn hermite_reorder_and_duplicate() {
    let a = snap();
    let mut b = a;
    b.seq = 2;
    b.t = 1.1;
    b.p.x += 1.0;
    let mut buf = Buffer::new();
    buf.push(b);
    buf.push(a);
    buf.push(a);
    let s = buf.sample(1.05).unwrap();
    assert!((s.s.p.x - 1.5).abs() < 1e-5 && s.mode == Mode::Interpolate);
    assert_eq!((buf.reordered, buf.duplicates), (1, 1));
    assert_eq!(buf.sample(2.0).unwrap().mode, Mode::Hold, "underrun holds, no extrapolation");
    assert_eq!(buf.sample(0.5).unwrap().mode, Mode::Startup);
}

#[test]
fn frame_transition_never_mixes_frames() {
    let a = snap();
    let mut b = a;
    b.seq = 2;
    b.t = 1.1;
    let mut transition = b;
    transition.seq = 3;
    transition.t = 1.2;
    transition.frame = FrameKind::Ship;
    transition.frame_id = 2;
    transition.wp = DVec3::new(0.0, 0.3, 1.0);
    let mut buf = Buffer::new();
    buf.push(a);
    buf.push(b);
    buf.push(transition);
    assert_eq!(buf.sample(1.15).unwrap().s.frame, FrameKind::Planet);
    let at = buf.sample(1.2).unwrap().s;
    assert_eq!((at.frame, at.frame_id, at.wp), (FrameKind::Ship, 2, transition.wp));
}

#[test]
fn planet_change_holds_old_frame() {
    let a = snap();
    let mut b = a;
    b.seq = 1;
    b.t = 1.1;
    b.planet = 1;
    b.p = DVec3::new(0.0, 5000.0, 0.0);
    let mut buf = Buffer::new();
    buf.push(a);
    buf.push(b);
    let mid = buf.sample(1.05).unwrap().s;
    assert_eq!((mid.planet, mid.p), (0, a.p), "no blend between two planet frames");
    assert_eq!(buf.sample(1.1).unwrap().mode, Mode::Transition);
}

#[test]
fn history_is_bounded() {
    let mut buf = Buffer::new();
    for i in 0..200 {
        let mut s = snap();
        s.seq = i;
        s.t = i as f64;
        buf.push(s);
    }
    assert_eq!(buf.history.len(), 128);
}

#[test]
fn rejoining_owner_resets_history() {
    let mut old = snap();
    old.seq = 100;
    old.t = 10.0;
    let mut fresh = snap();
    fresh.seq = 1;
    fresh.t = 12.0;
    let mut buf = Buffer::new();
    buf.push(old);
    buf.push(fresh);
    buf.push(old);
    assert_eq!(buf.history.len(), 1);
    assert_eq!(buf.sample(12.0).unwrap().s.seq, 1, "queued old life cannot return");
}

#[test]
fn shared_frame_reconstructs_in_any_origin() {
    // Planet 1 is 200 km away: world = centre + relative, exact in f64, and a shift of the render
    // origin (any whole-metre amount) never changes the shared coordinates.
    let rel = DVec3::new(0.25, 5000.5, 0.125);
    let w1 = to_world(1, rel);
    assert_eq!(to_planet(1, w1), rel);
    let origin = DVec3::new(200_000.0, 5_000.0, 0.0);
    let shifted = origin + DVec3::new(10_000.0, 10_000.0, -10_000.0);
    assert_eq!((w1 - shifted) + DVec3::new(10_000.0, 10_000.0, -10_000.0), w1 - origin);
}

#[test]
fn wire_packets_roundtrip_and_reject_garbage() {
    let s = snap();
    for p in [
        Packet::Hello { slot: 3, planet: 1 },
        Packet::Accepted { slot: 3 },
        Packet::Ping { sent: 1.5 },
        Packet::Pong { sent: 1.5, server_time: 99.25 },
        Packet::Snapshot(s),
        Packet::Bye { slot: 2 },
    ] {
        assert_eq!(Packet::decode(&p.encode()), Some(p));
    }
    assert_eq!(Packet::Snapshot(s).encode().len(), 2 + 148);
    assert!(Packet::decode(&[]).is_none());
    assert!(Packet::decode(&[0x00, 5, 1]).is_none(), "wrong magic");
    assert!(Packet::decode(&[0xE1, 99, 1]).is_none(), "unknown type");
    assert!(Packet::decode(&[0xE1, 5, 1, 2, 3]).is_none(), "short snapshot");
    let mut ping = Packet::Ping { sent: 1.0 }.encode();
    ping[2..10].copy_from_slice(&f64::INFINITY.to_le_bytes());
    assert!(Packet::decode(&ping).is_none(), "non-finite ping");
}

#[test]
fn link_is_deterministic_ordered_and_lossy() {
    let run = || {
        let mut l: Link<u32> = Link::new(50.0, 20.0, 10.0, 4401);
        for i in 0..1000 {
            l.enqueue(i as f64 * 0.01, i);
        }
        (l.dropped, l.ready(100.0))
    };
    let (d1, out1) = run();
    let (d2, out2) = run();
    assert_eq!((d1, &out1), (d2, &out2));
    assert!((60..140).contains(&(d1 as i32)), "about 10 % lost, got {d1}");
    let mut l: Link<u32> = Link::new(150.0, 0.0, 0.0, 1);
    l.enqueue(0.0, 1);
    assert!(l.ready(0.1).is_empty());
    assert_eq!(l.ready(0.15), vec![1]);
}

#[test]
fn clock_keeps_lowest_rtt() {
    let mut c = ClockSync::default();
    assert!(!c.ready);
    // True offset 5 s; first pong has an asymmetric 100 ms rtt, second a symmetric 2 ms one.
    c.on_pong(10.0, 15.09, 10.1);
    c.on_pong(11.0, 16.001, 11.002);
    assert!((c.offset - 5.0).abs() < 1e-9 && c.ready);
    c.on_pong(12.0, 17.5, 12.5);
    assert!((c.offset - 5.0).abs() < 1e-9, "a worse rtt does not replace the best");
    assert!((c.server_now(20.0) - 25.0).abs() < 1e-9);
}
