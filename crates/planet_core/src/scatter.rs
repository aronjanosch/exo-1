//! Scatter (#65): instances of props on the ground, per cell of the cube-sphere quadtree and
//! storey. A cell's instances are a pure function of the seed, the cell and the recipe, so the
//! same seed gives the same instances whatever order cells load in.
//!
//! Per candidate spot of a storey's jittered grid: every entry of that storey gets a weight
//! (group weight x entry weight x the biome row's multipliers x its mask), filters say no
//! (slope, height above sea, a site's clear radius), then one roll picks at most one entry
//! (weights are chances; their sum above 1 means always one). A picked entry may cluster
//! (its preset), and each placed instance gets a second roll for its rare look.
use crate::math::*;
use crate::planet::*;
use crate::recipe::*;
use fastnoise_lite::FastNoiseLite;

/// Placement rules of one entry, resolved to indices.
pub(crate) struct EntryRt {
    pub storey: usize,
    pub mask: Option<usize>,
    pub cluster: Option<usize>,
    /// Group weight x entry weight.
    pub weight: f32,
    /// Prop ids (the planet's variant of the mesh set), plain and rare.
    pub prop: String,
    pub rare_prop: Option<String>,
    pub cos_slope: [f64; 2],
}

pub(crate) struct ScatterRt {
    pub storeys: Vec<(String, Storey)>,
    pub masks: Vec<(FastNoiseLite, f32, f32)>,
    pub clusters: Vec<ClusterPreset>,
    pub entries: Vec<EntryRt>,
    /// Per biome row id: multiplier per entry (group x entry multipliers).
    pub mult: Vec<(u8, Vec<f32>)>,
}

impl ScatterRt {
    pub fn new(recipe: &Recipe, seed: i32) -> ScatterRt {
        let sc = &recipe.scatter;
        let storeys: Vec<(String, Storey)> = sc.storeys.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mask_names: Vec<&String> = sc.masks.keys().collect();
        let cluster_names: Vec<&String> = sc.clusters.keys().collect();
        let mut entries = Vec::new();
        let mut keys: Vec<(String, String)> = Vec::new(); // (group id, entry id)
        for g in &sc.groups {
            for e in &g.entries {
                let c = |d: f64| d.to_radians().cos();
                entries.push(EntryRt {
                    storey: storeys.iter().position(|(k, _)| *k == e.storey).unwrap(),
                    mask: e.mask.as_ref().map(|m| mask_names.iter().position(|k| *k == m).unwrap()),
                    cluster: e.cluster.as_ref().map(|m| cluster_names.iter().position(|k| *k == m).unwrap()),
                    weight: g.weight * e.weight,
                    prop: sc.meshes[&e.mesh].clone(),
                    rare_prop: e.rare.as_ref().map(|r| sc.meshes[&r.mesh].clone()),
                    // Slope range as cosines of the normal against the radius (cos falls with slope).
                    cos_slope: [c(e.slope_deg[1]), c(e.slope_deg[0])],
                });
                keys.push((g.id.clone(), e.id.clone()));
            }
        }
        let mult = recipe
            .biomes
            .iter()
            .map(|b| {
                let m = keys.iter().map(|(g, e)| b.scatter.get(g).copied().unwrap_or(1.0) * b.scatter.get(e).copied().unwrap_or(1.0)).collect();
                (b.id, m)
            })
            .collect();
        ScatterRt {
            storeys,
            masks: sc.masks.values().map(|m| (make_noise(&m.noise, seed), m.threshold, m.edge)).collect(),
            clusters: sc.clusters.values().cloned().collect(),
            entries,
            mult,
        }
    }
}

/// One placed prop. Basis columns are unit vectors (x, up, z); position relative to the cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Instance {
    /// Index into `Planet::scatter_entries()` (recipe order, groups flattened).
    pub entry: u16,
    pub rare: bool,
    pub pos: [f32; 3],
    pub basis: [[f32; 3]; 3],
    pub scale: f32,
    pub tint: [f32; 3],
}

#[derive(Default, Clone, Debug)]
pub struct ScatterCell {
    /// Cell centre on the base sphere, world planet space (f32-exact, like a chunk centre).
    pub center: [f64; 3],
    pub instances: Vec<Instance>,
}

/// What the app needs to know about one entry.
#[derive(Clone, Debug)]
pub struct EntryInfo {
    pub id: String,
    pub storey: String,
    pub prop: String,
    pub rare_prop: Option<String>,
}

struct Ground {
    dir: V3,
    h: f64,
    /// The water surface the entry's height band counts from: the sea, or a lake or river
    /// above it (#72).
    water: f64,
    nrm: V3,
    biome: u8,
}

impl Planet {
    pub fn scatter_entries(&self) -> Vec<EntryInfo> {
        let sc = &self.recipe.scatter;
        let ids = sc.groups.iter().flat_map(|g| g.entries.iter().map(|e| e.id.clone()));
        ids.zip(&self.scatter.entries)
            .map(|(id, e)| EntryInfo { id, storey: self.scatter.storeys[e.storey].0.clone(), prop: e.prop.clone(), rare_prop: e.rare_prop.clone() })
            .collect()
    }

    pub fn storeys(&self) -> &[(String, Storey)] {
        &self.scatter.storeys
    }

    fn mask_weight(&self, mask: Option<usize>, p: [f32; 3]) -> f32 {
        let Some(m) = mask else { return 1.0 };
        let (n, thr, edge) = &self.scatter.masks[m];
        let v = nz(n, p);
        ((v - thr) / edge.max(1e-6)).clamp(0.0, 1.0)
    }

    fn mults(&self, biome: u8) -> &[f32] {
        self.scatter.mult.iter().find(|(id, _)| *id == biome).map(|(_, m)| m.as_slice()).unwrap_or(&[])
    }

    /// Ground at face coordinates: direction, height, normal (from the height function), biome.
    fn ground(&self, face: usize, a: f64, b: f64, step: f64) -> Ground {
        let a = a.clamp(-1.0, 1.0);
        let b = b.clamp(-1.0, 1.0);
        let dir = cube_to_sphere(face, a, b);
        let (h, f) = self.height_ab(face, a, b, dir);
        let lf = self.stamp_height(dir).1;
        let at = |a: f64, b: f64| {
            let (a, b) = (a.clamp(-1.0, 1.0), b.clamp(-1.0, 1.0));
            let d = cube_to_sphere(face, a, b);
            d * (self.radius + self.height_ab(face, a, b, d).0)
        };
        let ta = at(a + step, b) - at(a - step, b);
        let tb = at(a, b + step) - at(a, b - step);
        let mut nrm = ta.cross(tb).normalized();
        if nrm.dot(dir) < 0.0 {
            nrm = -nrm;
        }
        let water = self.water_level_ab(face, a, b).map_or(self.sea, |l| l.max(self.sea));
        Ground { dir, h, water, nrm, biome: self.biome_for(h - self.sea, &f, lf) }
    }

    fn passes(&self, e: &EntryRt, spec: &ScatterEntry, g: &Ground, sites: &[&crate::site::Site]) -> bool {
        let ha = g.h - g.water;
        let c = g.nrm.dot(g.dir);
        if ha < spec.height_above_sea_m[0] || ha > spec.height_above_sea_m[1] || c < e.cos_slope[0] - 1e-9 || c > e.cos_slope[1] + 1e-9 {
            return false;
        }
        !(spec.clear_sites && sites.iter().any(|s| self.radius * s.dir.dot(g.dir).clamp(-1.0, 1.0).acos() < s.footprint_m))
    }

    /// Chance per spot of the storeys `which` (all but ground cover when None) at a point,
    /// mask and biome row included, filters left out: the atlas's scatter layer. The height
    /// counts from the water surface (the sea, or a lake or river above it).
    pub fn scatter_density_at(&self, dir: V3, biome: u8, h_above_water: f64, which: Option<&str>) -> f32 {
        let p = self.p32(dir);
        let m = self.mults(biome);
        let specs = self.recipe.scatter.groups.iter().flat_map(|g| g.entries.iter());
        let mut sum = 0.0;
        for (i, (e, spec)) in self.scatter.entries.iter().zip(specs).enumerate() {
            let name = self.scatter.storeys[e.storey].0.as_str();
            if which.map_or(name == "ground", |w| w != name) || h_above_water < spec.height_above_sea_m[0] || h_above_water > spec.height_above_sea_m[1] {
                continue;
            }
            sum += e.weight * m.get(i).copied().unwrap_or(1.0) * self.mask_weight(e.mask, p);
        }
        sum.min(1.0)
    }

    /// Instances of storey `storey` for the quadtree cell (face, a0, b0, size).
    pub fn build_scatter(&self, face: usize, a0: f64, b0: f64, size: f64, storey: usize) -> ScatterCell {
        let r = self.radius;
        let centre_dir = cube_to_sphere(face, a0 + size * 0.5, b0 + size * 0.5);
        let c64 = centre_dir * r;
        let centre = v3(c64.x as f32 as f64, c64.y as f32 as f64, c64.z as f32 as f64);
        let mut out = ScatterCell { center: centre.arr(), instances: Vec::new() };
        let Some((_, st)) = self.scatter.storeys.get(storey) else { return out };
        let edge_m = self.chunk_edge_m(face, a0, b0, size);
        let n = ((edge_m / st.spacing_m).round() as u32).max(1);
        // One metre in face coordinates, for the normals.
        let step = size / edge_m;
        let sites = self.sites_near(centre_dir, edge_m * 1.5);
        let (ix, iy) = (((a0 + 1.0) / size).round() as u32, ((b0 + 1.0) / size).round() as u32);
        let depth = (2.0 / size).log2().round() as u32;
        let key = hash(self.recipe.seed as u32 ^ (storey as u32).wrapping_mul(0x632BE5AB), face as u32, ix, iy, depth);
        let specs: Vec<&ScatterEntry> = self.recipe.scatter.groups.iter().flat_map(|g| g.entries.iter()).collect();
        let mine: Vec<usize> = (0..self.scatter.entries.len()).filter(|&i| self.scatter.entries[i].storey == storey).collect();
        if mine.is_empty() {
            return out;
        }
        let mut w = vec![0.0f32; mine.len()];
        for ci in 0..n {
            for cj in 0..n {
                let h = |salt: u32| hash01(key, 0, ci, cj, salt);
                let s = (ci as f64 + h(0) as f64) / n as f64;
                let t = (cj as f64 + h(1) as f64) / n as f64;
                let (a, b) = (a0 + s * size, b0 + t * size);
                let dir = cube_to_sphere(face, a, b);
                let p = self.p32(dir);
                // Cheap part first: weights from the biome at the point and the masks.
                let fl = self.macro_lookup(face, a.clamp(-1.0, 1.0), b.clamp(-1.0, 1.0));
                let (hh, lf) = (self.height_ab(face, a.clamp(-1.0, 1.0), b.clamp(-1.0, 1.0), dir).0, self.stamp_height(dir).1);
                let biome = self.biome_for(hh - self.sea, &fl, lf);
                let m = self.mults(biome);
                let mut total = 0.0;
                for (k, &i) in mine.iter().enumerate() {
                    let e = &self.scatter.entries[i];
                    w[k] = e.weight * m.get(i).copied().unwrap_or(1.0) * self.mask_weight(e.mask, p);
                    total += w[k];
                }
                if total <= 0.0 {
                    continue;
                }
                let u = h(2) * total.max(1.0);
                if u >= total {
                    continue;
                }
                let (mut acc, mut k) = (0.0, 0);
                while k + 1 < mine.len() && acc + w[k] <= u {
                    acc += w[k];
                    k += 1;
                }
                let i = mine[k];
                let (e, spec) = (&self.scatter.entries[i], specs[i]);
                let g = self.ground(face, a, b, step);
                if !self.passes(e, spec, &g, &sites) {
                    continue;
                }
                let mut spots = vec![(g, 0u32)];
                if let Some(cl) = e.cluster.map(|c| &self.scatter.clusters[c])
                    && !cl.shapes.is_empty()
                    && h(3) < cl.chance
                {
                    let tw: f32 = cl.shapes.iter().map(|s| s.weight).sum();
                    let mut u = h(4) * tw;
                    let shape = cl.shapes.iter().find(|s| {
                        u -= s.weight;
                        u < 0.0
                    }).unwrap_or(&cl.shapes[0]);
                    let count = shape.count[0] + ((shape.count[1] - shape.count[0] + 1) as f32 * h(5)) as u32;
                    let count = count.min(shape.count[1]);
                    let (mut ca, mut cb) = (a, b);
                    for m in 0..count {
                        // A random walk from the first instance: clumps, not rings.
                        let hm = |salt: u32| hash01(key, 1 + m, ci, cj, salt);
                        let ang = hm(0) as f64 * std::f64::consts::TAU;
                        let d = shape.spacing_m[0] + (shape.spacing_m[1] - shape.spacing_m[0]) * hm(1) as f64;
                        ca += ang.cos() * d * step;
                        cb += ang.sin() * d * step;
                        let gm = self.ground(face, ca, cb, step);
                        if self.passes(e, spec, &gm, &sites) {
                            spots.push((gm, 1 + m));
                        }
                    }
                }
                let tints = self.recipe.biomes.iter().find(|b| b.id == spots[0].0.biome);
                for (g, m) in spots {
                    let hm = |salt: u32| hash01(key, 100 + m, ci, cj, salt);
                    let up = (g.dir * (1.0 - spec.align) + g.nrm * spec.align).normalized();
                    let (tg, bt) = crate::look::tangent_frame(up);
                    let yaw = hm(0) as f64 * std::f64::consts::TAU;
                    let x = tg * yaw.cos() + bt * yaw.sin();
                    let z = x.cross(up);
                    let rare = spec.rare.as_ref().filter(|r| hm(1) < r.chance);
                    let mut sc = spec.scale[0] + (spec.scale[1] - spec.scale[0]) * hm(2);
                    if let Some(rr) = rare {
                        sc *= rr.scale;
                    }
                    let base = tints.map(|b| b.tints.get(&spec.tint).copied().unwrap_or(b.color)).unwrap_or([1.0; 3]);
                    let tint = rare.and_then(|rr| rr.tint).unwrap_or(base);
                    let v = 0.88 + 0.24 * hm(3);
                    let o = g.dir * (r + g.h - spec.sink_m * sc as f64) - centre;
                    let f = |v: V3| [v.x as f32, v.y as f32, v.z as f32];
                    out.instances.push(Instance {
                        entry: i as u16,
                        rare: rare.is_some(),
                        pos: f(o),
                        basis: [f(x), f(up), f(z)],
                        scale: sc,
                        tint: [tint[0] * v, tint[1] * v, tint[2] * v],
                    });
                }
            }
        }
        out
    }
}
