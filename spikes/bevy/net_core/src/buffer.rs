//! Timestamp-ordered, bounded history in shared coordinates. No extrapolation. Hermite for
//! position (uses velocity), slerp for rotation. Port of buffer.gd.
use crate::snapshot::Snapshot;
use glam::DVec3;
use std::collections::VecDeque;

pub const MAX_HISTORY: usize = 128;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Asked for a time before the first entry: show the first one.
    Startup,
    Interpolate,
    /// Asked for a time after the last entry (an underrun): hold the last one.
    Hold,
    /// Between two planets or frames: the old frame is held until the new timestamp.
    Transition,
}

#[derive(Copy, Clone, Debug)]
pub struct Sample {
    pub s: Snapshot,
    pub mode: Mode,
}

#[derive(Default)]
pub struct Buffer {
    pub history: VecDeque<Snapshot>,
    pub duplicates: u32,
    pub reordered: u32,
    restart_floor: Option<f64>,
}

impl Buffer {
    pub fn new() -> Buffer {
        Buffer::default()
    }

    pub fn push(&mut self, s: Snapshot) {
        if self.restart_floor.is_some_and(|f| s.t < f) {
            return;
        }
        // A rejoining owner restarts its sequence but has a newer shared timestamp. Reordered
        // older packets do not trigger this reset or bridge two lives.
        if let Some(last) = self.history.back() {
            if s.t > last.t && s.seq < last.seq {
                self.history.clear();
                self.restart_floor = Some(s.t);
            }
        }
        for i in 0..self.history.len() {
            if self.history[i].seq == s.seq {
                self.duplicates += 1;
                return;
            }
            if self.history[i].t > s.t {
                self.history.insert(i, s);
                self.reordered += 1;
                self.trim();
                return;
            }
        }
        self.history.push_back(s);
        self.trim();
    }

    fn trim(&mut self) {
        while self.history.len() > MAX_HISTORY {
            self.history.pop_front();
        }
    }

    pub fn sample(&self, t: f64) -> Option<Sample> {
        let first = self.history.front()?;
        if t < first.t {
            return Some(Sample { s: *first, mode: Mode::Startup });
        }
        for i in 1..self.history.len() {
            if self.history[i].t >= t {
                return Some(between(&self.history[i - 1], &self.history[i], t));
            }
        }
        Some(Sample { s: *self.history.back().unwrap(), mode: Mode::Hold })
    }
}

pub fn between(a: &Snapshot, b: &Snapshot, t: f64) -> Sample {
    let span = b.t - a.t;
    let u = ((t - a.t) / span.max(1e-6)).clamp(0.0, 1.0);
    let mut s = *a;
    let mut mode = Mode::Interpolate;
    s.t = t;
    if a.planet == b.planet {
        s.p = hermite(a.p, a.v, b.p, b.v, span, u);
        s.v = a.v.lerp(b.v, u);
        s.q = a.q.slerp(b.q, u);
    } else if u >= 1.0 {
        s = *b;
        mode = Mode::Transition;
    }
    if a.frame == b.frame && a.frame_id == b.frame_id && a.planet == b.planet {
        s.wp = hermite(a.wp, a.wv, b.wp, b.wv, span, u);
        s.wv = a.wv.lerp(b.wv, u);
        s.wq = a.wq.slerp(b.wq, u);
    } else if u >= 1.0 {
        s.frame = b.frame;
        s.frame_id = b.frame_id;
        s.wp = b.wp;
        s.wv = b.wv;
        s.wq = b.wq;
        s.flags = b.flags;
    }
    Sample { s, mode }
}

pub fn hermite(p0: DVec3, v0: DVec3, p1: DVec3, v1: DVec3, dt: f64, u: f64) -> DVec3 {
    let u2 = u * u;
    let u3 = u2 * u;
    p0 * (2.0 * u3 - 3.0 * u2 + 1.0) + v0 * dt * (u3 - 2.0 * u2 + u) + p1 * (-2.0 * u3 + 3.0 * u2) + v1 * dt * (u3 - u2)
}
