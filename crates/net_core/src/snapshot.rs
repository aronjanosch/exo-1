//! Fixed data-only wire format, 148 bytes, little endian. Planet-relative f64 positions, f32
//! velocities and quaternions (as in spike 4). Never anything but plain numbers.
use crate::MAX_OWNER;
use glam::{DQuat, DVec3};

pub const SIZE: usize = 148;
/// 2: cabin gravity (`lag`) added.
pub const VERSION: u32 = 2;
const MAX_PLANET: u32 = 7;
/// Plausibility limits (anything beyond is rejected as absurd). A ship in a quantum drive flight
/// is millions of metres from its planet's centre and moves at up to 1e6 m/s (spike 11); a walker
/// is always within the planet's own range.
pub const MAX_SHIP_POSITION: f64 = 1.0e8;
pub const MAX_SHIP_SPEED: f64 = 2.0e6;
const MAX_WALKER_RANGE: f64 = 1.0e6;

/// Which frame the walker pose is in.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Planet = 0,
    /// Cabin of the ship owned by `frame_id`.
    Ship = 1,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub owner: u32,
    pub planet: u32,
    pub frame: FrameKind,
    /// Owner of the ship the walker is in (frame == Ship), else 0.
    pub frame_id: u32,
    pub seq: u32,
    /// Shared (server) time in seconds.
    pub t: f64,
    /// Ship, planet-relative.
    pub p: DVec3,
    pub v: DVec3,
    pub q: DQuat,
    /// Walker: planet-relative or ship-local depending on `frame`.
    pub wp: DVec3,
    pub wv: DVec3,
    pub wq: DQuat,
    pub flags: u32,
    pub input_tick: u32,
    /// Cabin gravity (LAG) level of the ship, 0..1; sent as one byte.
    pub lag: f64,
}

impl Snapshot {
    pub fn new(owner: u32, t: f64, p: DVec3, v: DVec3, q: DQuat) -> Snapshot {
        Snapshot {
            owner,
            planet: 0,
            frame: FrameKind::Planet,
            frame_id: 0,
            seq: 0,
            t,
            p,
            v,
            q,
            wp: p + DVec3::new(6.0, 0.0, 0.0),
            wv: v,
            wq: q,
            flags: 0,
            input_tick: 0,
            lag: 1.0,
        }
    }

    /// Re-expresses the positions (ship, and walker while in the planet frame) relative to planet
    /// `to` instead of `self.planet`. With one common frame the buffer interpolates straight
    /// through a change of the sender's planet (it holds the older sample across one otherwise).
    pub fn to_frame_of(&mut self, centres: &[DVec3], to: usize) {
        let shift = centres[self.planet as usize] - centres[to];
        self.p += shift;
        if self.frame == FrameKind::Planet {
            self.wp += shift;
        }
        self.planet = to as u32;
    }

    pub fn encode(&self) -> [u8; SIZE] {
        let mut b = [0u8; SIZE];
        b[0..4].copy_from_slice(&VERSION.to_le_bytes());
        b[4..8].copy_from_slice(&self.owner.to_le_bytes());
        b[8..12].copy_from_slice(&self.planet.to_le_bytes());
        b[12..16].copy_from_slice(&(self.frame as u32).to_le_bytes());
        b[16..20].copy_from_slice(&self.frame_id.to_le_bytes());
        b[20..24].copy_from_slice(&self.seq.to_le_bytes());
        b[24..32].copy_from_slice(&self.t.to_le_bytes());
        put_d(&mut b, 32, self.p);
        put_f(&mut b, 56, self.v);
        put_q(&mut b, 68, self.q);
        put_d(&mut b, 84, self.wp);
        put_f(&mut b, 108, self.wv);
        put_q(&mut b, 120, self.wq);
        b[136..140].copy_from_slice(&self.flags.to_le_bytes());
        b[140..144].copy_from_slice(&self.input_tick.to_le_bytes());
        b[144..148].copy_from_slice(&((self.lag.clamp(0.0, 1.0) * 255.0).round() as u32).to_le_bytes());
        b
    }

    /// None for anything invalid: wrong size or version, unknown owner/planet/frame, parent id
    /// missing, non-finite or absurd values.
    pub fn decode(b: &[u8]) -> Option<Snapshot> {
        if b.len() != SIZE || u32_at(b, 0) != VERSION {
            return None;
        }
        let owner = u32_at(b, 4);
        let planet = u32_at(b, 8);
        let frame_raw = u32_at(b, 12);
        let frame_id = u32_at(b, 16);
        if !(1..=MAX_OWNER).contains(&owner) || planet > MAX_PLANET || frame_raw > 1 || frame_id > MAX_OWNER {
            return None;
        }
        let frame = if frame_raw == 1 { FrameKind::Ship } else { FrameKind::Planet };
        if frame == FrameKind::Ship && frame_id == 0 {
            return None;
        }
        let t = f64_at(b, 24);
        if !t.is_finite() || t < 0.0 {
            return None;
        }
        let (p, v, wp, wv) = (get_d(b, 32), get_f(b, 56), get_d(b, 84), get_f(b, 108));
        for (x, max) in [(p, MAX_SHIP_POSITION), (v, MAX_SHIP_SPEED), (wp, MAX_WALKER_RANGE), (wv, MAX_WALKER_RANGE)] {
            if !x.is_finite() || x.length() > max {
                return None;
            }
        }
        let (q, wq) = (get_q(b, 68)?, get_q(b, 120)?);
        let lag = u32_at(b, 144);
        if lag > 255 {
            return None;
        }
        Some(Snapshot { owner, planet, frame, frame_id, seq: u32_at(b, 20), t, p, v, q, wp, wv, wq, flags: u32_at(b, 136), input_tick: u32_at(b, 140), lag: lag as f64 / 255.0 })
    }
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn f64_at(b: &[u8], o: usize) -> f64 {
    f64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn put_d(b: &mut [u8], o: usize, v: DVec3) {
    for (i, c) in [v.x, v.y, v.z].into_iter().enumerate() {
        b[o + i * 8..o + i * 8 + 8].copy_from_slice(&c.to_le_bytes());
    }
}
fn put_f(b: &mut [u8], o: usize, v: DVec3) {
    for (i, c) in [v.x, v.y, v.z].into_iter().enumerate() {
        b[o + i * 4..o + i * 4 + 4].copy_from_slice(&(c as f32).to_le_bytes());
    }
}
fn put_q(b: &mut [u8], o: usize, q: DQuat) {
    for (i, c) in [q.x, q.y, q.z, q.w].into_iter().enumerate() {
        b[o + i * 4..o + i * 4 + 4].copy_from_slice(&(c as f32).to_le_bytes());
    }
}
fn get_d(b: &[u8], o: usize) -> DVec3 {
    DVec3::new(f64_at(b, o), f64_at(b, o + 8), f64_at(b, o + 16))
}
fn get_f(b: &[u8], o: usize) -> DVec3 {
    DVec3::new(f32_at(b, o) as f64, f32_at(b, o + 4) as f64, f32_at(b, o + 8) as f64)
}
fn get_q(b: &[u8], o: usize) -> Option<DQuat> {
    let q = DQuat::from_xyzw(f32_at(b, o) as f64, f32_at(b, o + 4) as f64, f32_at(b, o + 8) as f64, f32_at(b, o + 12) as f64);
    if !q.is_finite() || !(0.5..=1.5).contains(&q.length_squared()) {
        return None;
    }
    Some(q.normalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ship_in_a_quantum_flight_is_not_absurd() {
        // `new` copies the ship's values into the walker's; a ship in flight has a walker in its cabin.
        let at = |p: DVec3, v: DVec3| {
            let mut s = Snapshot::new(1, 1.0, p, v, DQuat::IDENTITY);
            s.frame = FrameKind::Ship;
            s.frame_id = 1;
            s.wp = DVec3::new(0.0, 0.3, -2.0);
            s.wv = DVec3::ZERO;
            s
        };
        let s = at(DVec3::new(6.0e6, 7000.0, 0.0), DVec3::new(1.0e6, 0.0, 0.0));
        assert!(Snapshot::decode(&s.encode()).is_some());
        let far = at(DVec3::new(2.0e8, 0.0, 0.0), DVec3::ZERO);
        assert!(Snapshot::decode(&far.encode()).is_none());
        let fast = at(DVec3::ZERO, DVec3::new(5.0e6, 0.0, 0.0));
        assert!(Snapshot::decode(&fast.encode()).is_none());
        // The walker keeps the tight limit.
        let mut w = Snapshot::new(1, 1.0, DVec3::ZERO, DVec3::ZERO, DQuat::IDENTITY);
        w.wp = DVec3::new(5.0e6, 0.0, 0.0);
        assert!(Snapshot::decode(&w.encode()).is_none());
    }

    #[test]
    fn frame_change_keeps_the_world_position() {
        let centres = [DVec3::ZERO, DVec3::new(12_500_000.0, 0.0, 0.0)];
        let mut s = Snapshot::new(1, 1.0, DVec3::new(10.0, 20.0, 30.0), DVec3::ZERO, DQuat::IDENTITY);
        s.planet = 1;
        s.wp = DVec3::new(1.0, 2.0, 3.0);
        let world = (centres[1] + s.p, centres[1] + s.wp);
        s.to_frame_of(&centres, 0);
        assert_eq!(s.planet, 0);
        assert_eq!((s.p, s.wp), world);
        // A walker in a cabin is in ship coordinates: unchanged.
        let mut c = Snapshot::new(1, 1.0, DVec3::ZERO, DVec3::ZERO, DQuat::IDENTITY);
        c.planet = 1;
        c.frame = FrameKind::Ship;
        c.frame_id = 1;
        c.wp = DVec3::new(0.0, 0.3, -2.0);
        c.to_frame_of(&centres, 0);
        assert_eq!(c.wp, DVec3::new(0.0, 0.3, -2.0));
    }
}
