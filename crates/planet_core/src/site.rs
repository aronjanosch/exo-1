//! Sites (#70): kinds placed by the bake's global pass (best-candidate sampling, separation,
//! filters, budgets per category), the ground edits under them (part of the height function)
//! and the pieces of their kits. A site is a pure function of the seed and the recipe.
use crate::look::{tangent_frame, walk};
use crate::math::*;
use crate::planet::*;
use crate::recipe::*;

pub(crate) enum EditRt {
    FlattenDisc { r: f64, roll: f64, dish: f64 },
    FlattenRect { hx: f64, hy: f64, roll: f64, dish: f64 },
    Smooth { r: f64, strength: f64 },
    Raise { r: f64, roll: f64, amount: f64, rim: f64, rim_w: f64 },
}

/// A placed site.
#[derive(Clone)]
pub struct Site {
    /// Index into `recipe.sites.kinds`.
    pub kind: usize,
    pub id: String,
    pub category: SiteCategory,
    pub dir: V3,
    /// Turn of the site's frame (rad, from local east).
    pub yaw: f64,
    pub footprint_m: f64,
    /// Height of the noise ground at the centre (m above the base radius): what flatten levels to.
    pub ground_m: f64,
    /// Reach of the edits (m) and its cosine on the unit sphere.
    pub reach_m: f64,
    pub(crate) cos_reach: f64,
    pub(crate) east: V3,
    pub(crate) north: V3,
    pub(crate) edits: std::sync::Arc<Vec<EditRt>>,
}

/// A piece of a site's kit, ready to draw: planet-space position and unit basis (x, up, z).
#[derive(Clone, Debug)]
pub struct Piece {
    pub prop: String,
    pub pos: V3,
    pub basis: [V3; 3],
    pub scale: f64,
    pub tint: [f32; 3],
    /// Ground height under the piece and how far it is sunk (tests: no floating, no burying).
    pub ground_m: f64,
    pub sink_m: f64,
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn edit_rt(e: &Edit) -> (EditRt, f64) {
    match *e {
        Edit::Flatten { radius_m: Some(r), rolloff_m, dish_m, .. } => (EditRt::FlattenDisc { r, roll: rolloff_m, dish: dish_m }, r + rolloff_m),
        Edit::Flatten { half_extent_m: Some([hx, hy]), rolloff_m, dish_m, .. } => (EditRt::FlattenRect { hx, hy, roll: rolloff_m, dish: dish_m }, hx.hypot(hy) + rolloff_m),
        Edit::Flatten { .. } => (EditRt::FlattenDisc { r: 0.0, roll: 0.0, dish: 0.0 }, 0.0),
        Edit::Smooth { radius_m, strength, .. } => (EditRt::Smooth { r: radius_m, strength }, radius_m * 1.5),
        Edit::Raise { radius_m, rolloff_m, amount_m, rim_m, rim_width_m, .. } => {
            (EditRt::Raise { r: radius_m, roll: rolloff_m, amount: amount_m, rim: rim_m, rim_w: rim_width_m }, radius_m + rolloff_m + rim_width_m * 3.0)
        }
    }
}

impl Planet {
    /// The height function's last step: the ground edits of every site in reach, in order.
    /// `base` is the noise height at `dir` (shape, stamps, bands).
    pub(crate) fn apply_edits(&self, dir: V3, base: f64) -> f64 {
        let mut h = base;
        for s in &self.sites {
            let dc = dir.dot(s.dir);
            if dc <= s.cos_reach {
                continue;
            }
            let x = self.radius * dir.dot(s.east).clamp(-1.0, 1.0).asin();
            let y = self.radius * dir.dot(s.north).clamp(-1.0, 1.0).asin();
            let d = x.hypot(y);
            for e in s.edits.iter() {
                match *e {
                    EditRt::FlattenDisc { r, roll, dish } => {
                        let w = 1.0 - smoothstep(r, r + roll.max(1e-6), d);
                        let t = (d / r.max(1e-6)).min(1.0);
                        h += (s.ground_m - dish * (1.0 - t * t) - h) * w;
                    }
                    EditRt::FlattenRect { hx, hy, roll, dish } => {
                        let out = (x.abs() - hx).max(0.0).hypot((y.abs() - hy).max(0.0));
                        let w = 1.0 - smoothstep(0.0, roll.max(1e-6), out);
                        let t = (x.abs() / hx.max(1e-6)).max(y.abs() / hy.max(1e-6)).min(1.0);
                        h += (s.ground_m - dish * (1.0 - t * t) - h) * w;
                    }
                    EditRt::Smooth { r, strength } => {
                        let w = 1.0 - smoothstep(r * 0.5, r * 1.5, d);
                        if w > 0.0 {
                            let mut sum = 0.0;
                            for k in 0..6 {
                                let a = k as f64 / 6.0 * std::f64::consts::TAU;
                                sum += self.base_height_at(walk(dir, s.east * a.cos() + s.north * a.sin(), r * 0.5, self.radius));
                            }
                            h += (sum / 6.0 - h) * w * strength.clamp(0.0, 1.0);
                        }
                    }
                    EditRt::Raise { r, roll, amount, rim, rim_w } => {
                        let w = 1.0 - smoothstep(r, r + roll.max(1e-6), d);
                        h += amount * w + rim * (-((d - r) / rim_w.max(1e-6)).powi(2)).exp();
                    }
                }
            }
        }
        h
    }

    fn kind_allowed_in(&self, k: &SiteKind, biome: u8) -> bool {
        k.biomes.as_ref().is_none_or(|b| b.contains(&biome))
            && self.recipe.biomes.iter().find(|r| r.id == biome).is_none_or(|r| r.sites.as_ref().is_none_or(|l| l.contains(&k.id)))
    }

    /// The global pass (#70): landmarks first (highest candidate), then the site kinds in turns,
    /// each new site the candidate farthest from all sites so far. Returns the sites and the kinds
    /// that missed their minimum count.
    pub(crate) fn place_sites_v2(&self) -> (Vec<Site>, Vec<String>) {
        let rule = &self.recipe.sites;
        let mut rng = Rng::new(0x9E3779B97F4A7C15 ^ (self.recipe.seed as u64).wrapping_mul(0xBF58476D1CE4E5B9));
        let kinds = &rule.kinds;
        let want: Vec<u32> = kinds.iter().map(|k| (k.count[0] + ((k.count[1] - k.count[0] + 1) as f64 * rng.next()) as u32).min(k.count[1])).collect();
        let mut placed: Vec<Site> = Vec::new();
        let mut tries = vec![0u32; kinds.len()];
        let mut got = vec![0u32; kinds.len()];
        let dist = |a: V3, b: V3| self.radius * a.dot(b).clamp(-1.0, 1.0).acos();
        // Landmarks first, then the others in round-robin turns.
        let mut order: Vec<usize> = (0..kinds.len()).filter(|&i| kinds[i].prefer_high).collect();
        order.extend((0..kinds.len()).filter(|&i| !kinds[i].prefer_high));
        loop {
            let mut any = false;
            for &ki in &order {
                let k = &kinds[ki];
                if got[ki] >= want[ki] || tries[ki] >= rule.candidates {
                    continue;
                }
                any = true;
                let reach_k = k.edits.iter().map(|e| edit_rt(e).1).fold(k.footprint_m, f64::max);
                let mut best: Option<(f64, V3, f64)> = None;
                let mut found = 0;
                while found < rule.best_of.max(1) && tries[ki] < rule.candidates {
                    tries[ki] += 1;
                    let z = rng.next() * 2.0 - 1.0;
                    let phi = rng.next() * std::f64::consts::TAU;
                    let rr = (1.0 - z * z).sqrt();
                    let d = v3(rr * phi.cos(), z, rr * phi.sin());
                    let ok_sep = placed.iter().all(|s| {
                        let other = &kinds[s.kind];
                        let sep = if s.kind == ki { k.min_separation_m.max(k.min_separation_all_m) } else { k.min_separation_all_m.max(other.min_separation_all_m) };
                        dist(s.dir, d) >= sep
                    });
                    if !ok_sep {
                        continue;
                    }
                    // Never under a stamp.
                    if self.stamps.iter().any(|s| dist(s.c, d) < s.reach_m + k.footprint_m) {
                        continue;
                    }
                    // Budgets of the kind's category.
                    if rule.budgets.iter().any(|b| b.category == k.category && placed.iter().filter(|s| s.category == b.category && dist(s.dir, d) < b.radius_m).count() as u32 >= b.max) {
                        continue;
                    }
                    let smp = self.sample(d);
                    let ha = smp.height_above_sea;
                    if ha < k.height_above_sea_m[0] || ha > k.height_above_sea_m[1] || smp.slope_deg < k.slope_deg[0] || smp.slope_deg > k.slope_deg[1] || smp.water_depth > 0.0 {
                        continue;
                    }
                    if !self.kind_allowed_in(k, smp.biome as u8) {
                        continue;
                    }
                    // No lake or river within the edits' reach, so they do not dam one (#72).
                    if self.water_within(d, reach_k) {
                        continue;
                    }
                    found += 1;
                    let score = if k.prefer_high { ha } else { placed.iter().map(|s| dist(s.dir, d)).fold(f64::MAX, f64::min) };
                    if best.is_none_or(|b| score > b.0) {
                        best = Some((score, d, smp.height));
                    }
                }
                if let Some((_, d, ground)) = best {
                    let yaw = rng.next() * std::f64::consts::TAU;
                    let (e0, n0) = tangent_frame(d);
                    let east = e0 * yaw.cos() + n0 * yaw.sin();
                    let north = d.cross(east);
                    let mut edits: Vec<&Edit> = k.edits.iter().collect();
                    edits.sort_by_key(|e| e.order());
                    let (rts, reaches): (Vec<EditRt>, Vec<f64>) = edits.into_iter().map(edit_rt).unzip();
                    let reach = reaches.iter().copied().fold(0.0, f64::max);
                    placed.push(Site {
                        kind: ki,
                        id: k.id.clone(),
                        category: k.category,
                        dir: d,
                        yaw,
                        footprint_m: k.footprint_m,
                        ground_m: ground,
                        reach_m: reach,
                        cos_reach: (reach / self.radius).cos(),
                        east,
                        north,
                        edits: std::sync::Arc::new(rts),
                    });
                    got[ki] += 1;
                }
            }
            if !any {
                break;
            }
        }
        let misses = kinds
            .iter()
            .enumerate()
            .filter(|(i, k)| got[*i] < k.count[0])
            .map(|(i, k)| format!("site kind {}: placed {} of at least {} ({} candidates)", k.id, got[i], k.count[0], tries[i]))
            .collect();
        (placed, misses)
    }

    /// The pieces of site `i`'s kit, standing on the edited ground.
    pub fn site_pieces(&self, i: usize) -> Vec<Piece> {
        let Some(s) = self.sites.get(i) else { return Vec::new() };
        let kind = &self.recipe.sites.kinds[s.kind];
        let Some(kit) = self.recipe.sites.kits.get(&kind.kit) else { return Vec::new() };
        let mut rng = Rng::new(0xD1B54A32D192ED03 ^ (self.recipe.seed as u64).wrapping_mul(0x94D049BB133111EB) ^ (i as u64 + 1).wrapping_mul(0x2545F4914F6CDD1D));
        let mut out = Vec::new();
        for p in kit {
            let spots: Vec<[f64; 2]> = match &p.scatter {
                None => vec![p.at],
                Some(sc) => {
                    let n = (sc.count[0] + ((sc.count[1] - sc.count[0] + 1) as f64 * rng.next()) as u32).min(sc.count[1]);
                    (0..n)
                        .map(|_| {
                            let a = rng.next() * std::f64::consts::TAU;
                            let r = sc.radius_m[0] + (sc.radius_m[1] - sc.radius_m[0]) * rng.next().sqrt();
                            [p.at[0] + r * a.cos(), p.at[1] + r * a.sin()]
                        })
                        .collect()
                }
            };
            for at in spots {
                let off = s.east * at[0] + s.north * at[1];
                let m = at[0].hypot(at[1]);
                let dir = if m > 1e-9 { walk(s.dir, off * (1.0 / m), m, self.radius) } else { s.dir };
                let ground = self.height_at(dir);
                // Normal from the edited height function.
                let (e, n) = tangent_frame(dir);
                let hh = |v: V3| v.normalized() * (self.radius + self.height_at(v.normalized()));
                let eps = 0.5 / self.radius;
                let nrm = (hh(dir + e * eps) - hh(dir - e * eps)).cross(hh(dir + n * eps) - hh(dir - n * eps)).normalized();
                let nrm = if nrm.dot(dir) < 0.0 { -nrm } else { nrm };
                let up = (dir * (1.0 - p.align) + nrm * p.align).normalized();
                let yaw = s.yaw + p.yaw_deg.map(|d| d.to_radians()).unwrap_or_else(|| rng.next() * std::f64::consts::TAU);
                let (te, tn) = tangent_frame(up);
                let x = te * yaw.cos() + tn * yaw.sin();
                let z = x.cross(up);
                let scale = p.scale.pick(rng.next());
                out.push(Piece {
                    prop: p.prop.clone(),
                    pos: dir * (self.radius + ground - p.sink_m * scale),
                    basis: [x, up, z],
                    scale,
                    tint: p.tint,
                    ground_m: ground,
                    sink_m: p.sink_m * scale,
                });
            }
        }
        out
    }
}
