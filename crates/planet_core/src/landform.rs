//! Landform library (#69): stamp shapes and their placement by budget. The global pass places
//! every stamp from the seed before anything else (it needs only the macro noise): the
//! signature first, then each kind in recipe order, a random candidate at a time, kept when its
//! fields fit and it is far enough from every stamp so far. A try that misses a kind's minimum
//! count starts over with the next sub-seed, up to `retry_limit`; then the bake fails.
use crate::math::*;
use crate::planet::*;
use crate::recipe::*;

/// A placed stamp, sizes drawn.
pub(crate) enum ShapeRt {
    Basin { r: f64, d: f64 },
    /// t along the step, n towards the upper shelf.
    Esc { t: V3, n: V3, len: f64, height: f64, sw: f64, shelf: f64, taper: f64 },
    Plateau { r: f64, h: f64, fall: f64 },
    Crater { r: f64, d: f64, rim: f64, rim_w: f64 },
    Canyon { t: V3, n: V3, len: f64, floor: f64, wall: f64, depth: f64, meander: f64, taper: f64 },
    Mesa { buttes: Vec<(V3, f64, f64, f64)> },
    Spire { h: f64, r: f64 },
    Caldera { rr: f64, hr: f64, w: f64, dc: f64 },
}

pub(crate) struct StampRt {
    pub kind: String,
    pub signature: bool,
    pub c: V3,
    /// Great-circle reach of the shape from its centre (m), and its cosine on the unit sphere.
    pub reach_m: f64,
    pub cos_reach: f64,
    pub landform: Option<f64>,
    pub shape: ShapeRt,
}

/// What a placed stamp is, for the atlas, the look harness and tests.
#[derive(Clone, Debug)]
pub struct PlacedStamp {
    pub kind: String,
    pub shape: &'static str,
    pub signature: bool,
    pub centre: V3,
    pub reach_m: f64,
    /// Largest height the shape adds or takes away (m).
    pub relief_m: f64,
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn dir(&mut self) -> V3 {
        let z = self.next() * 2.0 - 1.0;
        let phi = self.next() * std::f64::consts::TAU;
        let rr = (1.0 - z * z).sqrt();
        v3(rr * phi.cos(), z, rr * phi.sin())
    }
    /// A random unit tangent at `c`.
    fn tangent(&mut self, c: V3) -> V3 {
        let (e, n) = crate::look::tangent_frame(c);
        let a = self.next() * std::f64::consts::TAU;
        e * a.cos() + n * a.sin()
    }
}

fn inside(r: Option<[f64; 2]>, v: f64) -> bool {
    r.is_none_or(|[lo, hi]| v >= lo && v <= hi)
}

impl StampRt {
    fn shape_name(&self) -> &'static str {
        match self.shape {
            ShapeRt::Basin { .. } => "basin",
            ShapeRt::Esc { .. } => "escarpment",
            ShapeRt::Plateau { .. } => "plateau",
            ShapeRt::Crater { .. } => "crater",
            ShapeRt::Canyon { .. } => "canyon",
            ShapeRt::Mesa { .. } => "mesa_field",
            ShapeRt::Spire { .. } => "spire",
            ShapeRt::Caldera { .. } => "caldera",
        }
    }

    fn relief(&self) -> f64 {
        match &self.shape {
            ShapeRt::Basin { d, .. } => *d,
            ShapeRt::Esc { height, .. } => *height,
            ShapeRt::Plateau { h, .. } => *h,
            ShapeRt::Crater { d, rim, .. } => d.max(*rim),
            ShapeRt::Canyon { depth, .. } => *depth,
            ShapeRt::Mesa { buttes } => buttes.iter().map(|b| b.2).fold(0.0, f64::max),
            ShapeRt::Spire { h, .. } => *h,
            ShapeRt::Caldera { hr, dc, .. } => hr.max(*dc),
        }
    }

    /// Height added at `dir` (m) and the weight (0..1) where the stamp is "on" (landform forcing).
    pub fn height(&self, dir: V3, radius: f64) -> (f64, f64) {
        let dc = dir.dot(self.c);
        if dc <= self.cos_reach {
            return (0.0, 0.0);
        }
        let s = radius * dc.clamp(-1.0, 1.0).acos();
        match &self.shape {
            ShapeRt::Basin { r, d } => {
                let t = s / r;
                let w = (1.0 - t * t).max(0.0);
                (-d * w * w, w)
            }
            ShapeRt::Esc { t, n, len, height, sw, shelf, taper } => {
                let x = radius * dir.dot(*n).clamp(-1.0, 1.0).asin();
                let l = radius * dir.dot(*t).atan2(dc);
                let w = smoothstep(-sw * 0.5, sw * 0.5, x) * (1.0 - smoothstep(shelf - taper, *shelf, x)) * (1.0 - smoothstep(len * 0.5 - taper, len * 0.5, l.abs()));
                (height * w, w)
            }
            ShapeRt::Plateau { r, h, fall } => {
                let w = 1.0 - smoothstep(r - fall, *r, s);
                (h * w, w)
            }
            ShapeRt::Crater { r, d, rim, rim_w } => {
                let t = s / r;
                if t < 1.0 {
                    (-d * (1.0 - t * t) + rim * t.powi(6), t.powi(6))
                } else {
                    let g = (-((s - r) / rim_w).powi(2)).exp();
                    (rim * g, g)
                }
            }
            ShapeRt::Canyon { t, n, len, floor, wall, depth, meander, taper } => {
                let l = radius * dir.dot(*t).atan2(dc);
                let wave = meander * (l / len * std::f64::consts::TAU * 1.5).sin();
                let x = radius * dir.dot(*n).clamp(-1.0, 1.0).asin() - wave;
                let w = (1.0 - smoothstep(floor * 0.5, floor * 0.5 + wall, x.abs())) * (1.0 - smoothstep(len * 0.5 - taper, len * 0.5, l.abs()));
                (-depth * w, w)
            }
            ShapeRt::Mesa { buttes } => {
                let mut best = (0.0f64, 0.0f64);
                for (bc, br, bh, bf) in buttes {
                    let sb = radius * dir.dot(*bc).clamp(-1.0, 1.0).acos();
                    let w = 1.0 - smoothstep(br - bf, *br, sb);
                    if bh * w > best.0 {
                        best = (bh * w, w);
                    }
                }
                best
            }
            ShapeRt::Spire { h, r } => {
                let t = (s / r).min(1.0);
                let w = (1.0 - t).powf(2.2);
                (h * w, w)
            }
            ShapeRt::Caldera { rr, hr, w, dc: depth } => {
                let g = (-((s - rr) / w).powi(2)).exp();
                let inner = 1.0 - smoothstep(rr * 0.55, rr * 0.9, s);
                (hr * g - depth * inner, g)
            }
        }
    }
}

/// Draws a kind's sizes at centre `c` and builds the stamp.
fn build(k: &LandformKind, c: V3, rng: &mut Rng, radius: f64) -> StampRt {
    let mut p = |s: &Span| s.pick(rng.next());
    let (shape, reach) = match &k.shape {
        StampShape::Basin { radius_m, depth_m } => {
            let r = p(radius_m);
            (ShapeRt::Basin { r, d: p(depth_m) }, r)
        }
        StampShape::Escarpment { length_m, height_m, slope_width_m, shelf_depth_m, end_taper_m } => {
            let (len, height, sw, shelf, taper) = (p(length_m), p(height_m), p(slope_width_m), p(shelf_depth_m), p(end_taper_m));
            let t = rng.tangent(c);
            let n = c.cross(t).normalized();
            (ShapeRt::Esc { t, n, len, height, sw, shelf, taper }, (len * 0.5).hypot(shelf) + sw)
        }
        StampShape::Plateau { radius_m, height_m, falloff_m } => {
            let r = p(radius_m);
            (ShapeRt::Plateau { r, h: p(height_m), fall: p(falloff_m) }, r)
        }
        StampShape::Crater { radius_m, depth_m, rim_height_m, rim_width_m } => {
            let (r, d, rim, rim_w) = (p(radius_m), p(depth_m), p(rim_height_m), p(rim_width_m));
            (ShapeRt::Crater { r, d, rim, rim_w }, r + rim_w * 3.0)
        }
        StampShape::Canyon { length_m, floor_width_m, wall_width_m, depth_m, meander_m, end_taper_m } => {
            let (len, floor, wall, depth, meander, taper) = (p(length_m), p(floor_width_m), p(wall_width_m), p(depth_m), p(meander_m), p(end_taper_m));
            let t = rng.tangent(c);
            let n = c.cross(t).normalized();
            (ShapeRt::Canyon { t, n, len, floor, wall, depth, meander, taper }, len * 0.5 + floor + wall + meander)
        }
        StampShape::MesaField { radius_m, buttes, butte_radius_m, height_m, falloff_m } => {
            let r = p(radius_m);
            let n = buttes[0] + ((buttes[1] - buttes[0] + 1) as f64 * rng.next()) as u32;
            let mut list = Vec::new();
            let mut reach: f64 = 0.0;
            for _ in 0..n.min(buttes[1]) {
                let t = rng.tangent(c);
                let off = r * rng.next().sqrt();
                let bc = crate::look::walk(c, t, off, radius);
                let br = butte_radius_m.pick(rng.next());
                list.push((bc, br, height_m.pick(rng.next()), falloff_m.pick(rng.next())));
                reach = reach.max(off + br);
            }
            (ShapeRt::Mesa { buttes: list }, reach)
        }
        StampShape::Spire { height_m, base_radius_m } => {
            let r = p(base_radius_m);
            (ShapeRt::Spire { h: p(height_m), r }, r)
        }
        StampShape::Caldera { ring_radius_m, ring_height_m, ring_width_m, centre_depth_m } => {
            let (rr, hr, w, dc) = (p(ring_radius_m), p(ring_height_m), p(ring_width_m), p(centre_depth_m));
            (ShapeRt::Caldera { rr, hr, w, dc }, rr + w * 3.0)
        }
    };
    StampRt { kind: k.id.clone(), signature: k.signature, c, reach_m: reach, cos_reach: (reach / radius).min(3.0).cos(), landform: k.landform, shape }
}

impl Planet {
    /// The global pass: every stamp of the budget, or the reason it failed after all tries.
    pub(crate) fn place_landforms(&self) -> (Vec<StampRt>, u32, Result<(), String>) {
        let lf = &self.recipe.landforms;
        let mut order: Vec<&LandformKind> = lf.kinds.iter().filter(|k| k.signature).collect();
        order.extend(lf.kinds.iter().filter(|k| !k.signature));
        let mut last_miss = String::new();
        for attempt in 0..lf.retry_limit.max(1) {
            let mut rng = Rng(0xA0761D6478BD642F ^ (self.recipe.seed as u64).wrapping_mul(0xE7037ED1A0B428DB) ^ (attempt as u64 + 1).wrapping_mul(0x8EBC6AF09C88C6E3));
            let mut placed: Vec<(StampRt, f64)> = Vec::new();
            let mut miss = None;
            for k in &order {
                // How many of this kind this try wants, then candidates until they are placed.
                let want = k.count[0] + ((k.count[1] - k.count[0] + 1) as f64 * rng.next()) as u32;
                let want = want.min(k.count[1]);
                let mut got = 0;
                for _ in 0..lf.candidates {
                    if got >= want {
                        break;
                    }
                    let c = rng.dir();
                    let f = self.fields_at(c);
                    let w = &k.where_;
                    if !(inside(w.elevation, f.elev) && inside(w.temperature, f.temp) && inside(w.moisture, f.moist) && inside(w.landform, f.land) && inside(w.weirdness, f.weird)) {
                        continue;
                    }
                    if placed.iter().any(|(s, sep)| self.radius * s.c.dot(c).clamp(-1.0, 1.0).acos() < sep.max(k.min_separation_m)) {
                        continue;
                    }
                    let st = build(k, c, &mut rng, self.radius);
                    placed.push((st, k.min_separation_m));
                    got += 1;
                }
                if got < k.count[0] {
                    miss = Some(format!("landform {}: placed {got} of at least {}", k.id, k.count[0]));
                    break;
                }
            }
            match miss {
                None => return (placed.into_iter().map(|p| p.0).collect(), attempt + 1, Ok(())),
                Some(m) => last_miss = m,
            }
        }
        (Vec::new(), lf.retry_limit, Err(format!("{last_miss} after {} tries (landforms.retry_limit)", lf.retry_limit)))
    }

    /// The placed stamps.
    pub fn stamps(&self) -> Vec<PlacedStamp> {
        self.stamps
            .iter()
            .map(|s| PlacedStamp { kind: s.kind.clone(), shape: s.shape_name(), signature: s.signature, centre: s.c, reach_m: s.reach_m, relief_m: s.relief() })
            .collect()
    }
}
