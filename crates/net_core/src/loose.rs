//! Loose bodies (items lying in the world) on the unreliable side: the host sends the poses of the
use crate::buffer::{hermite, Mode};
use glam::{DQuat, DVec3};
use std::collections::VecDeque;

/// The frame a body is posed in; the ids are the game's (planet id, ship owner's slot).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BodyFrame {
    Planet(u32),
    Ship(u32),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct BodyState {
    pub id: u64,
    pub frame: BodyFrame,
    pub pos: DVec3,
    pub vel: DVec3,
    pub rot: DQuat,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BodyDatagram {
    pub t: f64,
    pub seq: u32,
    pub bodies: Vec<BodyState>,
}

/// Bodies in one datagram (65 bytes each; the rest goes into the next datagram).
pub const MAX_BODIES: usize = 16;
const ENTRY: usize = 8 + 1 + 4 + 24 + 12 + 16;
const HEADER: usize = 8 + 4 + 1;
/// A body is within this of the origin of its frame (m); anything beyond is absurd.
const MAX_RANGE: f64 = 1.0e9;
const MAX_SPEED: f64 = 1.0e4;

impl BodyDatagram {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(HEADER + self.bodies.len() * ENTRY);
        b.extend_from_slice(&self.t.to_le_bytes());
        b.extend_from_slice(&self.seq.to_le_bytes());
        b.push(self.bodies.len().min(MAX_BODIES) as u8);
        for s in self.bodies.iter().take(MAX_BODIES) {
            b.extend_from_slice(&s.id.to_le_bytes());
            let (kind, id) = match s.frame {
                BodyFrame::Planet(i) => (0u8, i),
                BodyFrame::Ship(i) => (1u8, i),
            };
            b.push(kind);
            b.extend_from_slice(&id.to_le_bytes());
            for c in [s.pos.x, s.pos.y, s.pos.z] {
                b.extend_from_slice(&c.to_le_bytes());
            }
            for c in [s.vel.x, s.vel.y, s.vel.z] {
                b.extend_from_slice(&(c as f32).to_le_bytes());
            }
            for c in [s.rot.x, s.rot.y, s.rot.z, s.rot.w] {
                b.extend_from_slice(&(c as f32).to_le_bytes());
            }
        }
        b
    }

    /// None for anything invalid: wrong size, unknown frame kind, non-finite values, a position
    /// or speed that cannot be, a rotation that is not one.
    pub fn decode(b: &[u8]) -> Option<BodyDatagram> {
        if b.len() < HEADER {
            return None;
        }
        let t = f64::from_le_bytes(b[0..8].try_into().ok()?);
        let seq = u32::from_le_bytes(b[8..12].try_into().ok()?);
        let n = b[12] as usize;
        if !t.is_finite() || t < 0.0 || n > MAX_BODIES || b.len() != HEADER + n * ENTRY {
            return None;
        }
        let f64_at = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        let f32_at = |o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as f64;
        let mut bodies = Vec::with_capacity(n);
        for i in 0..n {
            let o = HEADER + i * ENTRY;
            let id = u64::from_le_bytes(b[o..o + 8].try_into().ok()?);
            let fid = u32::from_le_bytes(b[o + 9..o + 13].try_into().ok()?);
            let frame = match b[o + 8] {
                0 => BodyFrame::Planet(fid),
                1 => BodyFrame::Ship(fid),
                _ => return None,
            };
            let pos = DVec3::new(f64_at(o + 13), f64_at(o + 21), f64_at(o + 29));
            let vel = DVec3::new(f32_at(o + 37), f32_at(o + 41), f32_at(o + 45));
            let rot = DQuat::from_xyzw(f32_at(o + 49), f32_at(o + 53), f32_at(o + 57), f32_at(o + 61));
            if !pos.is_finite() || pos.length() > MAX_RANGE || !vel.is_finite() || vel.length() > MAX_SPEED || !rot.is_finite() || !(0.5..=1.5).contains(&rot.length_squared()) {
                return None;
            }
            bodies.push(BodyState { id, frame, pos, vel, rot: rot.normalize() });
        }
        Some(BodyDatagram { t, seq, bodies })
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Pose {
    pub t: f64,
    pub frame: BodyFrame,
    pub pos: DVec3,
    pub vel: DVec3,
    pub rot: DQuat,
}

impl Pose {
    pub fn of(t: f64, s: &BodyState) -> Pose {
        Pose { t, frame: s.frame, pos: s.pos, vel: s.vel, rot: s.rot }
    }
}

pub const MAX_HISTORY: usize = 64;

/// The history of one body: poses ordered by time, interpolated at a playout time. No
/// extrapolation beyond `sample_extrapolated`'s cap. Between two poses in different frames the
/// older one is held until the newer's time.
#[derive(Default, Debug, Clone)]
pub struct Track {
    pub history: VecDeque<Pose>,
}

impl Track {
    pub fn new() -> Track {
        Track::default()
    }

    /// Inserts by time; a pose at a time already there (a doubled datagram, or the resting pose
    /// that repeats the last streamed one) is dropped.
    pub fn push(&mut self, p: Pose) {
        let mut i = self.history.len();
        while i > 0 && self.history[i - 1].t > p.t {
            i -= 1;
        }
        if i > 0 && (self.history[i - 1].t - p.t).abs() < 1e-9 {
            return;
        }
        self.history.insert(i, p);
        while self.history.len() > MAX_HISTORY {
            self.history.pop_front();
        }
    }

    pub fn last(&self) -> Option<&Pose> {
        self.history.back()
    }

    /// The pose at `t` and how it was found (`Startup`: before the first, `Hold`: after the last).
    pub fn sample(&self, t: f64) -> Option<(Pose, Mode)> {
        let first = self.history.front()?;
        if t < first.t {
            return Some((*first, Mode::Startup));
        }
        for i in 1..self.history.len() {
            if self.history[i].t >= t {
                return Some(between(&self.history[i - 1], &self.history[i], t));
            }
        }
        Some((*self.history.back().unwrap(), Mode::Hold))
    }

    /// As `sample`; after the last pose the body goes on with its velocity for at most `max`
    /// seconds (display only).
    pub fn sample_extrapolated(&self, t: f64, max: f64) -> Option<(Pose, Mode)> {
        let (mut p, mode) = self.sample(t)?;
        if mode == Mode::Hold && max > 0.0 {
            let dt = (t - p.t).clamp(0.0, max);
            p.pos += p.vel * dt;
        }
        Some((p, mode))
    }
}

fn between(a: &Pose, b: &Pose, t: f64) -> (Pose, Mode) {
    let span = b.t - a.t;
    let u = ((t - a.t) / span.max(1e-6)).clamp(0.0, 1.0);
    if a.frame != b.frame {
        return if u >= 1.0 { (*b, Mode::Transition) } else { (Pose { t, ..*a }, Mode::Transition) };
    }
    let p = Pose { t, frame: a.frame, pos: hermite(a.pos, a.vel, b.pos, b.vel, span, u), vel: a.vel.lerp(b.vel, u), rot: a.rot.slerp(b.rot, u) };
    (p, Mode::Interpolate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::Rng;

    fn state(id: u64, pos: DVec3, vel: DVec3) -> BodyState {
        BodyState { id, frame: BodyFrame::Ship(1), pos, vel, rot: DQuat::from_rotation_y(0.4) }
    }

    #[test]
    fn datagram_round_trips_within_the_quantisation() {
        let d = BodyDatagram {
            t: 12.5,
            seq: 9,
            bodies: vec![
                state(3, DVec3::new(1.0, 0.31, -2.5), DVec3::new(0.1, -3.0, 0.0)),
                BodyState { id: u64::MAX - 1, frame: BodyFrame::Planet(1), pos: DVec3::new(12_500_000.123456789, 20_001.5, -7.0), vel: DVec3::ZERO, rot: DQuat::IDENTITY },
            ],
        };
        let bytes = d.encode();
        assert_eq!(bytes.len(), HEADER + 2 * ENTRY);
        let back = BodyDatagram::decode(&bytes).expect("decodes");
        assert_eq!((back.t, back.seq, back.bodies.len()), (12.5, 9, 2));
        assert_eq!(back.bodies[1].pos, d.bodies[1].pos, "positions stay f64");
        assert_eq!(back.bodies[1].frame, BodyFrame::Planet(1));
        assert!((back.bodies[0].vel - d.bodies[0].vel).length() < 1e-6 && back.bodies[0].rot.angle_between(d.bodies[0].rot) < 1e-6);
    }

    #[test]
    fn decode_rejects_what_cannot_be() {
        let ok = BodyDatagram { t: 1.0, seq: 1, bodies: vec![state(1, DVec3::ZERO, DVec3::ZERO)] };
        let good = ok.encode();
        assert!(BodyDatagram::decode(&good).is_some());
        assert!(BodyDatagram::decode(&good[..good.len() - 1]).is_none(), "short");
        let mut b = good.clone();
        b[HEADER + 8] = 7;
        assert!(BodyDatagram::decode(&b).is_none(), "frame kind");
        let mut b = good.clone();
        b[HEADER + 13..HEADER + 21].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(BodyDatagram::decode(&b).is_none(), "nan position");
        let mut b = good.clone();
        b[HEADER + 13..HEADER + 21].copy_from_slice(&1.0e12f64.to_le_bytes());
        assert!(BodyDatagram::decode(&b).is_none(), "absurd position");
        let mut b = good.clone();
        b[HEADER + 37..HEADER + 41].copy_from_slice(&1.0e9f32.to_le_bytes());
        assert!(BodyDatagram::decode(&b).is_none(), "absurd speed");
        let mut b = good.clone();
        b[HEADER + 49..HEADER + 53].copy_from_slice(&0.0f32.to_le_bytes());
        b[HEADER + 53..HEADER + 65].fill(0);
        assert!(BodyDatagram::decode(&b).is_none(), "not a rotation");
        let mut b = good;
        b[12] = 200;
        assert!(BodyDatagram::decode(&b).is_none(), "count");
        let mut rng = Rng::new(3);
        for _ in 0..2000 {
            let n = (rng.next_u64() % 150) as usize;
            let junk: Vec<u8> = (0..n).map(|_| rng.next_u64() as u8).collect();
            let _ = BodyDatagram::decode(&junk);
        }
    }

    fn pose(t: f64, x: f64, vx: f64) -> Pose {
        Pose { t, frame: BodyFrame::Planet(0), pos: DVec3::new(x, 0.0, 0.0), vel: DVec3::new(vx, 0.0, 0.0), rot: DQuat::IDENTITY }
    }

    #[test]
    fn track_interpolates_holds_and_extrapolates_for_a_while() {
        let mut tr = Track::new();
        assert!(tr.sample(1.0).is_none());
        for i in 0..=10 {
            let t = i as f64 * 0.1;
            tr.push(pose(t, 5.0 * t, 5.0));
        }
        let (p, m) = tr.sample(0.45).unwrap();
        assert_eq!(m, Mode::Interpolate);
        assert!((p.pos.x - 2.25).abs() < 1e-9, "{}", p.pos.x);
        assert_eq!(tr.sample(-1.0).unwrap().1, Mode::Startup);
        let (h, m) = tr.sample(2.0).unwrap();
        assert_eq!((m, h.pos.x), (Mode::Hold, 5.0));
        let (e, _) = tr.sample_extrapolated(2.0, 0.1).unwrap();
        assert!((e.pos.x - 5.5).abs() < 1e-9, "capped at 100 ms: {}", e.pos.x);
        assert_eq!(tr.sample_extrapolated(2.0, 0.0).unwrap().0.pos.x, 5.0);
    }

    #[test]
    fn track_orders_late_poses_and_drops_doubles() {
        let mut tr = Track::new();
        for t in [0.0, 0.2, 0.1, 0.1, 0.3, 0.2] {
            tr.push(pose(t, t, 1.0));
        }
        let ts: Vec<f64> = tr.history.iter().map(|p| p.t).collect();
        assert_eq!(ts, vec![0.0, 0.1, 0.2, 0.3]);
        for i in 0..200 {
            tr.push(pose(i as f64, 0.0, 0.0));
        }
        assert!(tr.history.len() <= MAX_HISTORY);
    }

    #[test]
    fn track_holds_the_old_frame_until_the_new_one_starts() {
        let mut tr = Track::new();
        tr.push(pose(0.0, 1.0, 0.0));
        tr.push(Pose { frame: BodyFrame::Ship(1), ..pose(1.0, 9.0, 0.0) });
        let (p, m) = tr.sample(0.5).unwrap();
        assert_eq!((m, p.frame, p.pos.x), (Mode::Transition, BodyFrame::Planet(0), 1.0));
        let (p, _) = tr.sample(1.0).unwrap();
        assert_eq!((p.frame, p.pos.x), (BodyFrame::Ship(1), 9.0));
    }

}
