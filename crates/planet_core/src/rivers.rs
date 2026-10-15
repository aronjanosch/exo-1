use crate::drainage::Mouth;
use crate::math::*;
use crate::planet::River;
use std::collections::HashMap;

/// Reach of a river's valley, in half widths: the cross-section's parabola is cut that far, and the
/// fine relief of the ground comes back between one and this many half widths.
pub const REACH_HALF_WIDTHS: f64 = 6.0;
/// The water is defined this far (m) beyond the half width, so a chunk's quads see the bank.
pub const SHORE_M: f64 = 3.0;
pub const CURVE_STEPS: usize = 4;
/// How far a river vertex moves across the flow, at most, in grid spacings (the river stays in
/// the valley the drainage cut along the grid). TODO(initiator): value.
pub const MEANDER: f64 = 0.3;
/// Wavelength of the meander's noise, in grid spacings.
const MEANDER_WAVE: f64 = 4.0;

#[derive(Default)]
pub struct RiverIndex {
    cell: f64,
    fill: f64,
    /// Pieces near each cell: river index * CURVE_STEPS + piece.
    map: HashMap<u64, Vec<u32>>,
    /// Each river vertex's point on the curve (metres from the centre), smoothed and meandered.
    pts: Vec<V3>,
    /// The main river flowing into each vertex (the largest catchment), if any.
    up: Vec<Option<u32>>,
}

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

/// A straight piece of a river's curve: its ends (metres from the centre) and the values at both.
struct Seg {
    a: V3,
    b: V3,
    /// bed, level, depth, half width at each end
    va: [f64; 4],
    vb: [f64; 4],
}

fn values(r: &River) -> [f64; 4] {
    [r.bed_m, r.level_m, r.depth_m, r.half_width_m]
}

/// Smooth value noise in 3D, -1..1 (deterministic, no seed: the river layout is the seed's).
fn vnoise(p: V3) -> f64 {
    let h = |x: i64, y: i64, z: i64| {
        let mut n = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
        n ^= n >> 29;
        n = n.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        n ^= n >> 32;
        (n >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    };
    let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
    let (ix, iy, iz) = (fx as i64, fy as i64, fz as i64);
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let (u, v, w) = (s(p.x - fx), s(p.y - fy), s(p.z - fz));
    let l = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = l(h(ix, iy, iz), h(ix + 1, iy, iz), u);
    let x10 = l(h(ix, iy + 1, iz), h(ix + 1, iy + 1, iz), u);
    let x01 = l(h(ix, iy, iz + 1), h(ix + 1, iy, iz + 1), u);
    let x11 = l(h(ix, iy + 1, iz + 1), h(ix + 1, iy + 1, iz + 1), u);
    l(l(x00, x10, v), l(x01, x11, v), w)
}

fn catmull(p0: V3, p1: V3, p2: V3, p3: V3, t: f64) -> V3 {
    let (t2, t3) = (t * t, t * t * t);
    (p1 * 2.0 + (p2 - p0) * t + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * t2 + (p1 * 3.0 - p0 - p2 * 3.0 + p3) * t3) * 0.5
}

impl RiverIndex {
    pub fn build(rivers: &[River], radius: f64, spacing_m: f64, fill: f64) -> RiverIndex {
        let n = rivers.len();
        let mut up: Vec<Option<u32>> = vec![None; n];
        for (j, r) in rivers.iter().enumerate() {
            if let Mouth::River(i) = r.next {
                let i = i as usize;
                if up[i].is_none_or(|u| rivers[u as usize].catchment_km2 < r.catchment_km2) {
                    up[i] = Some(j as u32);
                }
            }
        }
        let next = |i: usize| match rivers[i].next {
            Mouth::River(j) => Some(j as usize),
            _ => None,
        };
        // Sources and mouths stay where the drainage put them; every other vertex moves towards
        // its neighbours along the river, twice, then across the flow by the meander noise.
        let mut pts: Vec<V3> = rivers.iter().map(|r| r.dir * radius).collect();
        for _ in 0..2 {
            let prev = pts.clone();
            for i in 0..n {
                if let (Some(u), Some(j)) = (up[i], next(i)) {
                    pts[i] = (prev[u as usize] * 0.25 + prev[i] * 0.5 + prev[j] * 0.25).normalized() * radius;
                }
            }
        }
        let smooth = pts.clone();
        for i in 0..n {
            if let (Some(u), Some(j)) = (up[i], next(i)) {
                let along = smooth[j] - smooth[u as usize];
                let across = smooth[i].cross(along).normalized();
                let shift = MEANDER * spacing_m * vnoise(smooth[i] * (1.0 / (MEANDER_WAVE * spacing_m)));
                pts[i] = (smooth[i] + across * shift).normalized() * radius;
            }
        }
        let mut idx = RiverIndex { cell: 0.0, fill, map: HashMap::new(), pts, up };
        let hw_max = rivers.iter().map(|r| r.half_width_m).fold(0.0, f64::max);
        let cell = (2.0 * spacing_m).max(REACH_HALF_WIDTHS * hw_max).max(1.0);
        idx.cell = cell;
        let mut map: HashMap<u64, Vec<u32>> = HashMap::new();
        for i in 0..n {
            for k in 0..CURVE_STEPS {
                let s = idx.seg(rivers, radius, i, k);
                let reach = REACH_HALF_WIDTHS * s.va[3].max(s.vb[3]) + SHORE_M;
                let lo = |f: fn(V3) -> f64| (f(s.a).min(f(s.b)) - reach) / cell;
                let hi = |f: fn(V3) -> f64| (f(s.a).max(f(s.b)) + reach) / cell;
                let (x0, y0, z0) = (lo(|v| v.x).floor() as i64, lo(|v| v.y).floor() as i64, lo(|v| v.z).floor() as i64);
                let (x1, y1, z1) = (hi(|v| v.x).floor() as i64, hi(|v| v.y).floor() as i64, hi(|v| v.z).floor() as i64);
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        for z in z0..=z1 {
                            let e = map.entry(key([x, y, z])).or_default();
                            let code = (i * CURVE_STEPS + k) as u32;
                            if e.last() != Some(&code) {
                                e.push(code);
                            }
                        }
                    }
                }
            }
        }
        idx.map = map;
        idx
    }

    /// The point of river `i`'s curve at `t` (0 at its vertex, 1 where it runs to), metres from
    /// the centre, on the sphere.
    pub fn curve_point(&self, rivers: &[River], radius: f64, i: usize, t: f64) -> V3 {
        let r = &rivers[i];
        let p1 = self.pts[i];
        let (p2, p3) = match r.next {
            Mouth::River(j) => {
                let j = j as usize;
                let p2 = self.pts[j];
                let p3 = match rivers[j].next {
                    Mouth::River(k) => self.pts[k as usize],
                    _ => rivers[j].to * radius,
                };
                (p2, p3)
            }
            _ => {
                let p2 = r.to * radius;
                (p2, p2 * 2.0 - p1)
            }
        };
        let p0 = self.up[i].map_or(p1 * 2.0 - p2, |u| self.pts[u as usize]);
        catmull(p0, p1, p2, p3, t).normalized() * radius
    }

    fn seg(&self, rivers: &[River], radius: f64, i: usize, k: usize) -> Seg {
        let r = &rivers[i];
        let va = values(r);
        let vb = match r.next {
            Mouth::River(j) => values(&rivers[j as usize]),
            _ => va,
        };
        let (t0, t1) = (k as f64 / CURVE_STEPS as f64, (k + 1) as f64 / CURVE_STEPS as f64);
        let lerp = |t: f64| std::array::from_fn(|c| va[c] + (vb[c] - va[c]) * t);
        Seg { a: self.curve_point(rivers, radius, i, t0), b: self.curve_point(rivers, radius, i, t1), va: lerp(t0), vb: lerp(t1) }
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
        for &code in list {
            let s = self.seg(rivers, radius, code as usize / CURVE_STEPS, code as usize % CURVE_STEPS);
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
                    for &code in list {
                        let s = self.seg(rivers, radius, code as usize / CURVE_STEPS, code as usize % CURVE_STEPS);
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
