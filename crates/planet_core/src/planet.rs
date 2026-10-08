//! Planet: bake (macro shell, sea level, sites, statistics), the one height
//! function, and point queries. Chunk build lives in chunk.rs.
use crate::math::*;
use crate::recipe::*;
use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use serde::Serialize;
use std::collections::BTreeMap;
use std::time::Instant;

pub const CHANNELS: usize = 5; // elevation (noise only), temperature, moisture, landform, reserved (wind/rain shadow, unused)

pub fn make_noise(spec: &NoiseSpec, seed: i32) -> FastNoiseLite {
    let mut n = FastNoiseLite::with_seed(seed + spec.seed_offset);
    n.set_noise_type(Some(NoiseType::OpenSimplex2S));
    n.set_frequency(Some(spec.frequency));
    n.set_fractal_lacunarity(Some(2.0));
    n.set_fractal_gain(Some(0.5));
    n.set_fractal_octaves(Some(spec.octaves));
    n.set_fractal_type(Some(match spec.fractal {
        Fractal::None => FractalType::None,
        Fractal::Fbm => FractalType::FBm,
        Fractal::Ridged => FractalType::Ridged,
    }));
    n
}

#[inline(always)]
pub fn nz(n: &FastNoiseLite, p: [f32; 3]) -> f32 {
    n.get_noise_3d(p[0], p[1], p[2])
}

struct BandRt {
    noise: FastNoiseLite,
    amp: f64,
    warp: Option<(FastNoiseLite, f64)>,
}

pub(crate) enum StampRt {
    Basin { c: V3, r: f64, d: f64, cos_reach: f64 },
    Esc { c: V3, t: V3, n: V3, spec: (f64, f64, f64, f64, f64), landform: Option<i32>, cos_reach: f64 },
    Plateau { c: V3, r: f64, h: f64, fall: f64, cos_reach: f64 },
}

/// Macro fields at one point.
#[derive(Copy, Clone, Debug, Default)]
pub struct Fields {
    pub elev: f64, // macro noise elevation in metres, stamps not included
    pub temp: f64,
    pub moist: f64,
    pub land: i32,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Sample {
    pub height: f64,           // metres above the base radius
    pub height_above_sea: f64, // metres
    pub sea: f64,
    pub biome: i32,
    pub slope_deg: f64,
    pub temperature: f64,
    pub moisture: f64,
    pub landform: i32,
    pub macro_elevation: f64,
    pub stamp_height: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BakeStats {
    pub bake_ms: f64,
    pub macro_ms: f64,
    pub sea_ms: f64,
    pub stats_ms: f64,
    pub sites_ms: f64,
    pub threads: usize,
    pub sea_level_m: f64,
    pub land_fraction_macro: f64,
    pub land_fraction_full: f64,
    pub biome_area_share: BTreeMap<String, f64>,
    pub min_height_above_sea: f64,
    pub max_height_above_sea: f64,
    pub mean_height_above_sea: f64,
    pub basin_run_through_centre_m: [f64; 2],
    pub basin_span_below_sea_m: [f64; 2],
    pub basin_centre_height_above_sea: f64,
    pub plateau_stamp_height_m: f64,
    pub plateau_top_mean_above_sea_m: f64,
    pub escarpment_stamp_step_m: f64,
    pub escarpment_full_step_mean_m: f64,
    pub site_count: usize,
    pub site_min_pair_m: f64,
    pub site_mean_nn_m: f64,
    pub site_max_nn_m: f64,
    pub site_median_nn_m: f64,
}

pub struct Planet {
    pub recipe: Recipe,
    pub radius: f64,
    n_elev: FastNoiseLite,
    n_moist: FastNoiseLite,
    n_temp: FastNoiseLite,
    n_land: FastNoiseLite,
    bands: Vec<BandRt>,
    pub(crate) masks: Vec<FastNoiseLite>,
    pub(crate) stamps: Vec<StampRt>,
    pub macro_img: Vec<f32>,
    pub sea: f64,
    pub sites: Vec<V3>,
    pub baked: bool,
    /// (min, max) crust height above the base radius found by the bake statistics.
    pub height_range: (f64, f64),
}

pub(crate) fn par_rows<T: Send, F: Fn(usize, usize) -> T + Sync>(rows: usize, threads: usize, f: F) -> Vec<T> {
    let threads = threads.max(1);
    let per = rows.div_ceil(threads);
    std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads)
            .filter(|t| t * per < rows)
            .map(|t| {
                let f = &f;
                s.spawn(move || f(t * per, ((t + 1) * per).min(rows)))
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

impl Planet {
    pub fn new(recipe: Recipe) -> Planet {
        let seed = recipe.seed;
        let radius = recipe.radius;
        let m = &recipe.macro_;
        let bands = recipe
            .bands
            .iter()
            .map(|b| BandRt {
                noise: make_noise(&b.noise, seed),
                amp: b.amplitude,
                warp: b.warp.as_ref().map(|w| {
                    let spec = NoiseSpec { seed_offset: w.seed_offset, frequency: w.frequency, fractal: Fractal::None, octaves: 1 };
                    (make_noise(&spec, seed), w.amplitude)
                }),
            })
            .collect();
        let masks = recipe.scatter.iter().map(|s| make_noise(&s.mask.noise, seed)).collect();
        let stamps = recipe
            .stamps
            .iter()
            .map(|s| match s {
                Stamp::Basin { center, radius_m, depth_m } => StampRt::Basin {
                    c: V3::from_arr(*center).normalized(),
                    r: *radius_m,
                    d: *depth_m,
                    cos_reach: (radius_m / radius).cos(),
                },
                Stamp::Escarpment { center, length_m, height_m, slope_width_m, shelf_depth_m, end_taper_m, landform } => {
                    let c = V3::from_arr(*center).normalized();
                    // assumption: the segment runs east-west (tangent = c x world up)
                    let t = c.cross(v3(0.0, 1.0, 0.0)).normalized();
                    let n = c.cross(t).normalized();
                    let reach = ((length_m * 0.5).hypot(*shelf_depth_m) + slope_width_m) / radius;
                    StampRt::Esc {
                        c,
                        t,
                        n,
                        spec: (*length_m, *height_m, *slope_width_m, *shelf_depth_m, *end_taper_m),
                        landform: *landform,
                        cos_reach: reach.min(3.0).cos(),
                    }
                }
                Stamp::Plateau { center, radius_m, height_m, falloff_m } => StampRt::Plateau {
                    c: V3::from_arr(*center).normalized(),
                    r: *radius_m,
                    h: *height_m,
                    fall: *falloff_m,
                    cos_reach: (radius_m / radius).cos(),
                },
            })
            .collect();
        Planet {
            n_elev: make_noise(&m.elevation.noise, seed),
            n_moist: make_noise(&m.moisture.noise, seed),
            n_temp: make_noise(&m.temperature.noise, seed),
            n_land: make_noise(&m.landform.noise, seed),
            bands,
            masks,
            stamps,
            macro_img: Vec::new(),
            sea: 0.0,
            sites: Vec::new(),
            baked: false,
            height_range: (0.0, 0.0),
            radius,
            recipe,
        }
    }

    #[inline(always)]
    pub fn p32(&self, dir: V3) -> [f32; 3] {
        [(dir.x * self.radius) as f32, (dir.y * self.radius) as f32, (dir.z * self.radius) as f32]
    }

    /// Sum of the stamped features (metres) and the landform id they force, if any.
    pub fn stamp_height(&self, dir: V3) -> (f64, Option<i32>) {
        let mut h = 0.0;
        let mut lf = None;
        for s in &self.stamps {
            match s {
                StampRt::Basin { c, r, d, cos_reach } => {
                    let dt = dir.dot(*c);
                    if dt > *cos_reach {
                        let t = self.radius * dt.clamp(-1.0, 1.0).acos() / r;
                        let w = 1.0 - t * t;
                        h -= d * w * w; // smooth bowl, zero slope at the rim
                    }
                }
                StampRt::Esc { c, t, n, spec, landform, cos_reach } => {
                    let dc = dir.dot(*c);
                    if dc > *cos_reach {
                        let (len, height, sw, shelf, taper) = *spec;
                        let x = self.radius * dir.dot(*n).clamp(-1.0, 1.0).asin();
                        let l = self.radius * dir.dot(*t).atan2(dc);
                        let w = smoothstep(-sw * 0.5, sw * 0.5, x)
                            * (1.0 - smoothstep(shelf - taper, shelf, x))
                            * (1.0 - smoothstep(len * 0.5 - taper, len * 0.5, l.abs()));
                        h += height * w;
                        if w > 0.5 && landform.is_some() {
                            lf = *landform;
                        }
                    }
                }
                StampRt::Plateau { c, r, h: ph, fall, cos_reach } => {
                    let dt = dir.dot(*c);
                    if dt > *cos_reach {
                        let s = self.radius * dt.clamp(-1.0, 1.0).acos();
                        h += ph * (1.0 - smoothstep(r - fall, *r, s));
                    }
                }
            }
        }
        (h, lf)
    }

    fn bands_height(&self, p: [f32; 3]) -> f64 {
        let mut h = 0.0;
        for b in &self.bands {
            let v = if let Some((w, wa)) = &b.warp {
                let wa = *wa as f32;
                let q = [
                    p[0] + wa * nz(w, [p[0] + 1013.0, p[1], p[2]]),
                    p[1] + wa * nz(w, [p[0], p[1] + 2027.0, p[2]]),
                    p[2] + wa * nz(w, [p[0], p[1], p[2] + 3041.0]),
                ];
                nz(&b.noise, q)
            } else {
                nz(&b.noise, p)
            };
            h += v as f64 * b.amp;
        }
        h
    }

    /// Macro fields at face coordinates (a, b in [-1, 1]). Vertex-centred grid: edge
    /// samples sit exactly on the cube edges, so both faces interpolate the same
    /// samples along a shared edge and there is no seam.
    pub fn macro_lookup(&self, face: usize, a: f64, b: f64) -> Fields {
        let n = self.recipe.macro_.resolution;
        let nn = n as f64;
        let u = ((a + 1.0) * 0.5 * nn).clamp(0.0, nn);
        let v = ((b + 1.0) * 0.5 * nn).clamp(0.0, nn);
        let i0 = (u.floor() as usize).min(n - 1);
        let j0 = (v.floor() as usize).min(n - 1);
        let (fu, fv) = (u - i0 as f64, v - j0 as f64);
        let w = n + 1;
        let idx = |i: usize, j: usize| ((face * w + j) * w + i) * CHANNELS;
        let (k00, k10, k01, k11) = (idx(i0, j0), idx(i0 + 1, j0), idx(i0, j0 + 1), idx(i0 + 1, j0 + 1));
        let m = &self.macro_img;
        let bil = |c: usize| {
            let (a, b, cc, d) = (m[k00 + c] as f64, m[k10 + c] as f64, m[k01 + c] as f64, m[k11 + c] as f64);
            let x0 = a + (b - a) * fu;
            let x1 = cc + (d - cc) * fu;
            x0 + (x1 - x0) * fv
        };
        let k = if fu >= 0.5 { if fv >= 0.5 { k11 } else { k10 } } else if fv >= 0.5 { k01 } else { k00 };
        Fields { elev: bil(0), temp: bil(1), moist: bil(2), land: m[k + 3] as i32 }
    }

    /// THE height function: metres above the base radius, from face coordinates.
    pub fn height_ab(&self, face: usize, a: f64, b: f64, dir: V3) -> (f64, Fields) {
        let f = self.macro_lookup(face, a, b);
        let h = f.elev + self.stamp_height(dir).0 + self.bands_height(self.p32(dir));
        (h, f)
    }

    /// THE height function, from a unit direction (inverts the cube mapping first).
    pub fn height_at(&self, dir: V3) -> f64 {
        let d = dir.normalized();
        let face = face_of(d);
        let (a, b) = sphere_to_face_ab(face, d);
        self.height_ab(face, a, b, d).0
    }

    pub fn biome_for(&self, h_above_sea: f64, f: &Fields, forced_landform: Option<i32>) -> u8 {
        let t = &self.recipe.macro_.temperature;
        let temp = f.temp - t.lapse_per_m * h_above_sea.max(0.0);
        let land = forced_landform.unwrap_or(f.land);
        for row in &self.recipe.biomes {
            let ok = row.when.iter().all(|c| {
                let v = match c.field {
                    Field::HeightAboveSea => h_above_sea,
                    Field::Temperature => temp,
                    Field::Moisture => f.moist,
                    Field::Landform => land as f64,
                };
                c.min.map_or(true, |m| v >= m)
                    && c.max.map_or(true, |m| v <= m)
                    && c.one_of.as_ref().map_or(true, |l| l.contains(&(v as i32)))
            });
            if ok {
                return row.id;
            }
        }
        self.recipe.biomes.last().unwrap().id
    }

    /// Everything the bot interface wants to know about one spot.
    pub fn sample(&self, dir: V3) -> Sample {
        let d = dir.normalized();
        let face = face_of(d);
        let (a, b) = sphere_to_face_ab(face, d);
        let (h, f) = self.height_ab(face, a, b, d);
        let (sh, lf) = self.stamp_height(d);
        let ha = h - self.sea;
        let up = if d.y.abs() < 0.99 { v3(0.0, 1.0, 0.0) } else { v3(1.0, 0.0, 0.0) };
        let east = d.cross(up).normalized();
        let north = east.cross(d);
        let e = 2.0 / self.radius;
        let hh = |v: V3| self.height_at(v);
        let gx = (hh(d + east * e) - hh(d - east * e)) / 4.0;
        let gy = (hh(d + north * e) - hh(d - north * e)) / 4.0;
        Sample {
            height: h,
            height_above_sea: ha,
            sea: self.sea,
            biome: self.biome_for(ha, &f, lf) as i32,
            slope_deg: gx.hypot(gy).atan().to_degrees(),
            temperature: f.temp - self.recipe.macro_.temperature.lapse_per_m * ha.max(0.0),
            moisture: f.moist,
            landform: lf.unwrap_or(f.land),
            macro_elevation: f.elev,
            stamp_height: sh,
        }
    }

    /// Collision patch heights in a tangent frame: for each grid point (x, z) the height y
    /// where the vertical line through it meets the terrain,
    /// |C + xT + zB + y*up| = R + h(dir). Frame centre C = up * R. Row-major, z outer.
    pub fn patch_heights(&self, up: V3, t: V3, b: V3, n: usize) -> Vec<f32> {
        let r = self.radius;
        let c = up * r;
        let half = (n as f64 - 1.0) * 0.5;
        let mut out = Vec::with_capacity(n * n);
        for j in 0..n {
            let z = j as f64 - half;
            for i in 0..n {
                let x = i as f64 - half;
                let flat = c + t * x + b * z;
                let mut y = 0.0;
                for _ in 0..3 {
                    let dir = (flat + up * y).normalized();
                    let surface = r + self.height_at(dir);
                    y = (surface * surface - x * x - z * z).sqrt() - r;
                }
                out.push(y as f32);
            }
        }
        out
    }

    pub fn biome_color(&self, id: u8) -> [f32; 3] {
        self.recipe.biomes.iter().find(|b| b.id == id).map(|b| b.color).unwrap_or([1.0, 0.0, 1.0])
    }

    pub fn sites_near(&self, dir: V3, radius_m: f64) -> Vec<V3> {
        let d = dir.normalized();
        self.sites.iter().copied().filter(|s| self.radius * s.dot(d).clamp(-1.0, 1.0).acos() <= radius_m).collect()
    }

    /// Macro shell, sea level, sites, statistics. `threads` = 0 means all cores.
    pub fn bake(&mut self, threads: usize) -> BakeStats {
        let t_all = Instant::now();
        let threads = if threads == 0 { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) } else { threads };
        let mut st = BakeStats { threads, ..Default::default() };
        let n = self.recipe.macro_.resolution;
        let w = n + 1;
        let rows = 6 * w;
        let r = self.radius;

        // 1. macro images + (elevation incl. stamps, area weight) for the percentile
        let t0 = Instant::now();
        let mut img = vec![0.0f32; rows * w * CHANNELS];
        let mut ew = vec![0.0f32; rows * w * 2];
        {
            let per = rows.div_ceil(threads);
            let this = &*self;
            std::thread::scope(|s| {
                for (t, (slab, ewslab)) in img.chunks_mut(per * w * CHANNELS).zip(ew.chunks_mut(per * w * 2)).enumerate() {
                    s.spawn(move || {
                        for (k, (out, eo)) in slab.chunks_mut(w * CHANNELS).zip(ewslab.chunks_mut(w * 2)).enumerate() {
                            let row = t * per + k;
                            let (face, j) = (row / w, row % w);
                            let b = -1.0 + j as f64 * 2.0 / n as f64;
                            for i in 0..w {
                                let a = -1.0 + i as f64 * 2.0 / n as f64;
                                let dir = cube_to_sphere(face, a, b);
                                let p = this.p32(dir);
                                let m = &this.recipe.macro_;
                                let elev = nz(&this.n_elev, p) as f64 * m.elevation.amplitude;
                                let tm = &m.temperature;
                                let temp = tm.base - tm.latitude_gain * dir.y.abs() + tm.noise_gain * nz(&this.n_temp, p) as f64;
                                let cl = m.landform.classes as f32;
                                let land = ((nz(&this.n_land, p) + 1.0) * 0.5 * cl).floor().clamp(0.0, cl - 1.0);
                                let o = &mut out[i * CHANNELS..(i + 1) * CHANNELS];
                                o[0] = elev as f32;
                                o[1] = temp as f32;
                                o[2] = nz(&this.n_moist, p);
                                o[3] = land;
                                o[4] = 0.0;
                                eo[i * 2] = (elev + this.stamp_height(dir).0) as f32;
                                eo[i * 2 + 1] = this.area_weight(face, a, b) as f32;
                            }
                        }
                    });
                }
            });
        }
        self.macro_img = img;
        st.macro_ms = t0.elapsed().as_secs_f64() * 1e3;

        // 2. sea level: area-weighted percentile of the macro elevation
        let t0 = Instant::now();
        let mut pairs: Vec<(f32, f32)> = ew.chunks(2).map(|c| (c[0], c[1])).collect();
        pairs.sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let total: f64 = pairs.iter().map(|p| p.1 as f64).sum();
        let target = total * (1.0 - self.recipe.sea_level.land_fraction);
        let mut acc = 0.0;
        self.sea = pairs.last().unwrap().0 as f64;
        for p in &pairs {
            acc += p.1 as f64;
            if acc >= target {
                self.sea = p.0 as f64;
                break;
            }
        }
        let above: f64 = pairs.iter().filter(|p| p.0 as f64 > self.sea).map(|p| p.1 as f64).sum();
        st.sea_level_m = self.sea;
        st.land_fraction_macro = above / total;
        drop(pairs);
        drop(ew);
        st.sea_ms = t0.elapsed().as_secs_f64() * 1e3;

        let hr;
        // 3. statistics of the full height function on a stride-2 grid
        let t0 = Instant::now();
        {
            let this = &*self;
            let stride = 2usize;
            #[derive(Default)]
            struct Acc {
                w: f64,
                land: f64,
                hsum: f64,
                min: f64,
                max: f64,
                biomes: BTreeMap<u8, f64>,
            }
            let parts = par_rows(6 * (n / stride + 1), threads, |r0, r1| {
                let mut acc = Acc { min: f64::MAX, max: f64::MIN, ..Default::default() };
                let g = n / stride + 1;
                for row in r0..r1 {
                    let (face, jj) = (row / g, row % g);
                    let j = jj * stride;
                    let b = -1.0 + j as f64 * 2.0 / n as f64;
                    for ii in 0..g {
                        let a = -1.0 + (ii * stride) as f64 * 2.0 / n as f64;
                        let dir = cube_to_sphere(face, a, b);
                        let (h, f) = this.height_ab(face, a, b, dir);
                        let ha = h - this.sea;
                        let wgt = this.area_weight(face, a, b);
                        let lf = this.stamp_height(dir).1;
                        acc.w += wgt;
                        acc.hsum += wgt * ha;
                        if ha > 0.0 {
                            acc.land += wgt;
                        }
                        acc.min = acc.min.min(ha);
                        acc.max = acc.max.max(ha);
                        *acc.biomes.entry(this.biome_for(ha, &f, lf)).or_insert(0.0) += wgt;
                    }
                }
                acc
            });
            let mut tot = Acc { min: f64::MAX, max: f64::MIN, ..Default::default() };
            for p in parts {
                tot.w += p.w;
                tot.land += p.land;
                tot.hsum += p.hsum;
                tot.min = tot.min.min(p.min);
                tot.max = tot.max.max(p.max);
                for (k, v) in p.biomes {
                    *tot.biomes.entry(k).or_insert(0.0) += v;
                }
            }
            st.land_fraction_full = tot.land / tot.w;
            st.mean_height_above_sea = tot.hsum / tot.w;
            st.min_height_above_sea = tot.min;
            st.max_height_above_sea = tot.max;
            hr = (tot.min + self.sea, tot.max + self.sea);
            for row in &self.recipe.biomes {
                st.biome_area_share.insert(row.id.to_string(), tot.biomes.get(&row.id).copied().unwrap_or(0.0) / tot.w);
            }
        }
        self.height_range = hr;
        self.feature_stats(&mut st);
        st.stats_ms = t0.elapsed().as_secs_f64() * 1e3;

        // 4. sites
        let t0 = Instant::now();
        self.sites = self.place_sites();
        self.baked = true;
        self.site_stats(&mut st);
        st.sites_ms = t0.elapsed().as_secs_f64() * 1e3;
        st.bake_ms = t_all.elapsed().as_secs_f64() * 1e3;
        let _ = r;
        st
    }

    /// Area element of the cube-sphere mapping at (a, b), relative units.
    fn area_weight(&self, face: usize, a: f64, b: f64) -> f64 {
        let e = 1e-4;
        let da = cube_to_sphere(face, a + e, b) - cube_to_sphere(face, a - e, b);
        let db = cube_to_sphere(face, a, b + e) - cube_to_sphere(face, a, b - e);
        da.cross(db).length() / (4.0 * e * e)
    }

    /// Contiguous run (m) around the start point along a great circle where the
    /// height is below sea level, and the total span first-to-last below-sea sample.
    fn run_below_sea(&self, c: V3, t: V3, half_range_m: f64) -> (f64, f64) {
        let step = 5.0;
        let n = (half_range_m / step) as i32;
        let below = |k: i32| {
            let ang = k as f64 * step / self.radius;
            let d = c * ang.cos() + t * ang.sin();
            self.height_at(d) < self.sea
        };
        let mut run = 0.0;
        if below(0) {
            let (mut lo, mut hi) = (0, 0);
            while lo > -n && below(lo - 1) {
                lo -= 1;
            }
            while hi < n && below(hi + 1) {
                hi += 1;
            }
            run = (hi - lo + 1) as f64 * step;
        }
        let (mut first, mut last) = (i32::MAX, i32::MIN);
        for k in -n..=n {
            if below(k) {
                first = first.min(k);
                last = last.max(k);
            }
        }
        let span = if first <= last { (last - first + 1) as f64 * step } else { 0.0 };
        (run, span)
    }

    fn feature_stats(&self, st: &mut BakeStats) {
        for s in &self.stamps {
            match s {
                StampRt::Basin { c, r, .. } => {
                    let t1 = c.cross(v3(0.0, 1.0, 0.0)).normalized();
                    let t2 = c.cross(t1).normalized();
                    st.basin_centre_height_above_sea = self.height_at(*c) - self.sea;
                    let (r1, s1) = self.run_below_sea(*c, t1, r * 1.5);
                    let (r2, s2) = self.run_below_sea(*c, t2, r * 1.5);
                    st.basin_run_through_centre_m = [r1, r2];
                    st.basin_span_below_sea_m = [s1, s2];
                }
                StampRt::Plateau { c, r, .. } => {
                    st.plateau_stamp_height_m = self.stamp_height(*c).0;
                    // mean full height above sea over a disc of half the radius
                    let t1 = c.cross(v3(0.0, 1.0, 0.0)).normalized();
                    let t2 = c.cross(t1);
                    let (mut sum, mut cnt) = (0.0, 0.0);
                    for ia in -5..=5 {
                        for ib in -5..=5 {
                            let (x, y) = (ia as f64 * r * 0.1, ib as f64 * r * 0.1);
                            if x.hypot(y) <= r * 0.5 {
                                let d = (*c + t1 * (x / self.radius) + t2 * (y / self.radius)).normalized();
                                sum += self.height_at(d) - self.sea;
                                cnt += 1.0;
                            }
                        }
                    }
                    st.plateau_top_mean_above_sea_m = sum / cnt;
                }
                StampRt::Esc { c, t, n, spec, .. } => {
                    let off = 200.0 / self.radius;
                    let step_at = |l: f64| {
                        let base = *c * (l / self.radius).cos() + *t * (l / self.radius).sin();
                        let hi = (base + *n * off).normalized();
                        let lo = (base - *n * off).normalized();
                        (self.stamp_height(hi).0 - self.stamp_height(lo).0, self.height_at(hi) - self.height_at(lo))
                    };
                    st.escarpment_stamp_step_m = step_at(0.0).0;
                    let len = spec.0;
                    let k = 11;
                    let mean: f64 = (0..k).map(|i| step_at((i as f64 / (k - 1) as f64 - 0.5) * len * 0.6).1).sum::<f64>() / k as f64;
                    st.escarpment_full_step_mean_m = mean;
                }
            }
        }
    }

    fn place_sites(&self) -> Vec<V3> {
        let rule = &self.recipe.sites;
        let mut state: u64 = 0x9E3779B97F4A7C15 ^ (self.recipe.seed as u64).wrapping_mul(0xBF58476D1CE4E5B9);
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut sites: Vec<V3> = Vec::new();
        let min_dot = (rule.min_separation_m / self.radius).cos();
        for _ in 0..400_000 {
            if sites.len() >= rule.count {
                break;
            }
            let z = next() * 2.0 - 1.0;
            let phi = next() * std::f64::consts::TAU;
            let rr = (1.0 - z * z).sqrt();
            let d = v3(rr * phi.cos(), z, rr * phi.sin());
            if sites.iter().any(|s| s.dot(d) > min_dot) {
                continue;
            }
            let s = self.sample(d);
            if s.height_above_sea < rule.min_height_above_sea_m || s.slope_deg > rule.max_slope_deg {
                continue;
            }
            sites.push(d);
        }
        sites
    }

    fn site_stats(&self, st: &mut BakeStats) {
        let n = self.sites.len();
        st.site_count = n;
        if n < 2 {
            return;
        }
        let dist = |a: &V3, b: &V3| self.radius * a.dot(*b).clamp(-1.0, 1.0).acos();
        let mut min_pair = f64::MAX;
        let (mut sum, mut max) = (0.0, 0.0f64);
        let mut nns = Vec::with_capacity(n);
        for (i, a) in self.sites.iter().enumerate() {
            let mut nn = f64::MAX;
            for (j, b) in self.sites.iter().enumerate() {
                if i != j {
                    let d = dist(a, b);
                    nn = nn.min(d);
                    min_pair = min_pair.min(d);
                }
            }
            sum += nn;
            max = max.max(nn);
            nns.push(nn);
        }
        nns.sort_by(f64::total_cmp);
        st.site_median_nn_m = nns[n / 2];
        st.site_min_pair_m = min_pair;
        st.site_mean_nn_m = sum / n as f64;
        st.site_max_nn_m = max;
    }
}
