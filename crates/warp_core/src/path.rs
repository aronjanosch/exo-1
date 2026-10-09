//! The warp path: a cubic Hermite spline from the ship to the exit point. It leaves along the
//! planet's tangent when the straight line would run into the planet, and comes in radially at
//! the end (the ship arrives facing the target's centre). The obstruction check walks the same
//! curve, so a path can go around a body the line would hit.
use crate::system::{Obstacle, PlanetDef, PlanetId};
use glam::DVec3;

const TABLE: usize = 512;

#[derive(Clone, Debug)]
pub struct Path {
    p0: DVec3,
    p1: DVec3,
    m0: DVec3,
    m1: DVec3,
    /// Unit direction at the start (`m0` can be zero: no tension, no chord).
    start: DVec3,
    /// Arc length at u = i / TABLE.
    arc: Vec<f64>,
}

/// Direction of travel at an end of the path: the straight direction if it points away from
/// the planet's ground, else its projection on the tangent plane (the radial part removed).
/// Close to radial the projection is tiny and its side random, so it blends into a stable tangent
/// (same side as the projection) instead of flipping from tick to tick.
fn tangent_dir(dir: DVec3, up: DVec3) -> DVec3 {
    let radial = dir.dot(up);
    if radial >= 0.0 {
        return dir;
    }
    let t = dir - up * radial;
    let a = if up.y.abs() < 0.9 { DVec3::Y } else { DVec3::X };
    let mut f = up.cross(a).normalize();
    if t.dot(f) < 0.0 {
        f = -f;
    }
    let w = 1.0 - (t.length() / 0.2).min(1.0);
    (t + f * w).normalize()
}

impl Path {
    /// `from_up` is the radial direction at the start when the ship is at a planet (in its frame
    /// zone), None in open space. `end_dir` is the direction of travel at the exit point.
    pub fn new(p0: DVec3, from_up: Option<DVec3>, p1: DVec3, end_dir: DVec3, tension: f64) -> Path {
        let chord = p1 - p0;
        let len = chord.length();
        let dir = if len > 0.0 { chord / len } else { DVec3::NEG_Z };
        let start = from_up.map_or(dir, |up| tangent_dir(dir, up));
        let m0 = start * len * tension;
        let m1 = end_dir.normalize_or(dir) * len * tension;
        let mut p = Path { p0, p1, m0, m1, start, arc: vec![0.0; TABLE + 1] };
        let mut last = p0;
        for i in 1..=TABLE {
            let q = p.at_u(i as f64 / TABLE as f64);
            p.arc[i] = p.arc[i - 1] + q.distance(last);
            last = q;
        }
        p
    }

    pub fn at_u(&self, u: f64) -> DVec3 {
        let (u2, u3) = (u * u, u * u * u);
        self.p0 * (2.0 * u3 - 3.0 * u2 + 1.0) + self.m0 * (u3 - 2.0 * u2 + u) + self.p1 * (-2.0 * u3 + 3.0 * u2) + self.m1 * (u3 - u2)
    }

    pub fn length(&self) -> f64 {
        self.arc[TABLE]
    }

    fn u_at(&self, s: f64) -> f64 {
        let s = s.clamp(0.0, self.length());
        let i = self.arc.partition_point(|&a| a <= s).clamp(1, TABLE);
        let (a, b) = (self.arc[i - 1], self.arc[i]);
        let f = if b > a { (s - a) / (b - a) } else { 0.0 };
        (i - 1) as f64 / TABLE as f64 + f / TABLE as f64
    }

    /// Position at arc length `s` and the unit direction of travel there.
    pub fn at(&self, s: f64) -> (DVec3, DVec3) {
        let u = self.u_at(s);
        let pos = self.at_u(u);
        let h = 1e-4;
        let d = self.at_u((u + h).min(1.0)) - self.at_u((u - h).max(0.0));
        (pos, d.normalize_or_zero())
    }

    pub fn start_dir(&self) -> DVec3 {
        self.start
    }

    pub fn end(&self) -> DVec3 {
        self.p1
    }

    /// First thing the curve runs into: a planet (its obstruction radius plus margin) or an
    /// obstacle, by index. Sweeps segments between samples, never single points (at 1000 km/s a
    /// tick is 17 km).
    pub fn blocked(&self, planets: &[PlanetDef], obstacles: &[Obstacle]) -> Option<Blocker> {
        let n = TABLE * 2;
        let mut prev = self.at_u(0.0);
        for i in 1..=n {
            let q = self.at_u(i as f64 / n as f64);
            for (k, pl) in planets.iter().enumerate() {
                if segment_hits_sphere(prev, q, pl.centre(), pl.keep_out()) {
                    return Some(Blocker::Planet(PlanetId(k as u8)));
                }
            }
            for (k, o) in obstacles.iter().enumerate() {
                if segment_hits_sphere(prev, q, o.centre, o.radius) {
                    return Some(Blocker::Obstacle(k));
                }
            }
            prev = q;
        }
        None
    }
}

/// What is at `p`: a planet whose keep-out sphere holds it, or an obstacle closer than its radius.
pub fn blocked_at(p: DVec3, planets: &[PlanetDef], obstacles: &[Obstacle]) -> Option<Blocker> {
    if let Some(k) = planets.iter().position(|pl| pl.centre().distance(p) < pl.keep_out()) {
        return Some(Blocker::Planet(PlanetId(k as u8)));
    }
    obstacles.iter().position(|o| o.centre.distance(p) < o.radius).map(Blocker::Obstacle)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocker {
    Planet(PlanetId),
    Obstacle(usize),
}

/// Does the segment a..b come closer than `r` to `c`?
pub fn segment_hits_sphere(a: DVec3, b: DVec3, c: DVec3, r: f64) -> bool {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 > 0.0 { ((c - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t).distance_squared(c) < r * r
}
