//! Rivers as polylines with width (#177, spike 14). The drainage runs on the coarse grid and
//! leaves, per river vertex, a bed, a water level, a depth and a width. A river is the segment
//! from each vertex to the one the water runs to; its cross-section is evaluated per point of a
//! chunk from these numbers, so a river a few metres wide is carved on a grid a kilometre wide,
//! and the channel is the same function of the position in every chunk (no seams).
use crate::drainage::Mouth;
use crate::math::*;
use crate::planet::River;
use std::collections::HashMap;

/// Reach of a river's valley, in half widths: the cross-section's parabola is cut that far, and the
/// fine relief of the ground comes back between one and this many half widths.
pub const REACH_HALF_WIDTHS: f64 = 6.0;
/// The water is defined this far (m) beyond the half width, so a chunk's quads see the bank.
pub const SHORE_M: f64 = 3.0;

#[derive(Default)]
pub struct RiverIndex {
    cell: f64,
    fill: f64,
    map: HashMap<u64, Vec<u32>>,
}

/// What the rivers did to a point of the ground.
#[derive(Copy, Clone, Debug)]
pub struct Carved {
    /// The ground after the rivers' cross-sections (m above the base radius).
    pub ground: f64,
    /// How much of the fine relief stays here: 0 in the channel, 1 beyond the valley.
    pub relief: f64,
    /// Water surface where the point lies in a channel or on its shore.
    pub level: Option<f64>,
}

fn key(c: [i64; 3]) -> u64 {
    let k = |v: i64| ((v + (1 << 20)) & 0x1F_FFFF) as u64;
    k(c[0]) | k(c[1]) << 21 | k(c[2]) << 42
}

/// The segment of river `i`: its ends (points on the sphere) and the values at both.
struct Seg {
    a: V3,
    b: V3,
    /// bed, level, depth, half width at each end
    va: [f64; 4],
    vb: [f64; 4],
}

fn seg(rivers: &[River], i: usize, radius: f64) -> Seg {
    let r = &rivers[i];
    let va = [r.bed_m, r.level_m, r.depth_m, r.half_width_m];
    let vb = match r.next {
        Mouth::River(j) => {
            let n = &rivers[j as usize];
            [n.bed_m, n.level_m, n.depth_m, n.half_width_m]
        }
        _ => va,
    };
    Seg { a: r.dir * radius, b: r.to * radius, va, vb }
}

impl RiverIndex {
    /// The index of a planet's rivers. `spacing_m` is the coarse grid's cell size.
    pub fn build(rivers: &[River], radius: f64, spacing_m: f64, fill: f64) -> RiverIndex {
        let hw_max = rivers.iter().map(|r| r.half_width_m).fold(0.0, f64::max);
        let cell = (2.0 * spacing_m).max(REACH_HALF_WIDTHS * hw_max).max(1.0);
        let mut map: HashMap<u64, Vec<u32>> = HashMap::new();
        for i in 0..rivers.len() {
            let s = seg(rivers, i, radius);
            let reach = REACH_HALF_WIDTHS * s.va[3].max(s.vb[3]) + SHORE_M;
            let lo = |f: fn(V3) -> f64| (f(s.a).min(f(s.b)) - reach) / cell;
            let hi = |f: fn(V3) -> f64| (f(s.a).max(f(s.b)) + reach) / cell;
            let (x0, y0, z0) = (lo(|v| v.x).floor() as i64, lo(|v| v.y).floor() as i64, lo(|v| v.z).floor() as i64);
            let (x1, y1, z1) = (hi(|v| v.x).floor() as i64, hi(|v| v.y).floor() as i64, hi(|v| v.z).floor() as i64);
            for x in x0..=x1 {
                for y in y0..=y1 {
                    for z in z0..=z1 {
                        map.entry(key([x, y, z])).or_default().push(i as u32);
                    }
                }
            }
        }
        RiverIndex { cell, fill, map }
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    fn bucket(&self, p: V3) -> Option<&Vec<u32>> {
        self.map.get(&key([(p.x / self.cell).floor() as i64, (p.y / self.cell).floor() as i64, (p.z / self.cell).floor() as i64]))
    }

    /// The rivers' cross-sections applied to the ground `g` at the point `p` (on the sphere of the
    /// planet, metres from the centre).
    pub fn carve(&self, rivers: &[River], radius: f64, p: V3, g: f64) -> Carved {
        let mut out = Carved { ground: g, relief: 1.0, level: None };
        let Some(list) = self.bucket(p) else { return out };
        let mut best = f64::MAX;
        let mut hit = false;
        for &i in list {
            let s = seg(rivers, i as usize, radius);
            let ab = s.b - s.a;
            let len2 = ab.dot(ab);
            let t = if len2 > 0.0 { ((p - s.a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let d = (p - (s.a + ab * t)).length();
            let lerp = |k: usize| s.va[k] + (s.vb[k] - s.va[k]) * t;
            let (bed, depth, hw) = (lerp(0), lerp(2), lerp(3).max(1e-3));
            if d > REACH_HALF_WIDTHS * hw {
                continue;
            }
            let f = bed + depth * (d / hw).powi(2);
            // Each river's cross-section from the ground without rivers; the lowest wins, so a
            // river's channel is never filled up to another's bank at a confluence.
            let gi = if g >= f { f } else { f + (g - f) * smoothstep(hw, 2.0 * hw, d) };
            out.ground = if hit { out.ground.min(gi) } else { gi };
            hit = true;
            out.relief = out.relief.min(smoothstep(hw, REACH_HALF_WIDTHS * hw, d));
            if d <= hw + SHORE_M && d < best {
                best = d;
                out.level = Some(bed + depth * self.fill);
            }
        }
        out
    }

    /// Whether a river lies within `radius_m` of `p` (its half width counted in).
    pub fn within(&self, rivers: &[River], radius: f64, p: V3, radius_m: f64) -> bool {
        let c = self.cell;
        let (x0, x1) = (((p.x - radius_m) / c).floor() as i64, ((p.x + radius_m) / c).floor() as i64);
        let (y0, y1) = (((p.y - radius_m) / c).floor() as i64, ((p.y + radius_m) / c).floor() as i64);
        let (z0, z1) = (((p.z - radius_m) / c).floor() as i64, ((p.z + radius_m) / c).floor() as i64);
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let Some(list) = self.map.get(&key([x, y, z])) else { continue };
                    for &i in list {
                        let s = seg(rivers, i as usize, radius);
                        let ab = s.b - s.a;
                        let len2 = ab.dot(ab);
                        let t = if len2 > 0.0 { ((p - s.a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
                        let d = (p - (s.a + ab * t)).length();
                        if d <= radius_m + s.va[3].max(s.vb[3]) + SHORE_M {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}
