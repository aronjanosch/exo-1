//! Planet: bake (coarse global layer, sites, statistics), the one height
//! function, and point queries. Chunk build lives in chunk.rs.
use crate::math::*;
use crate::recipe::*;
use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;
use crate::coarse::{self, Coarse};

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
    scale: BandScale,
}

use crate::landform::{ShapeRt, StampRt};

/// Macro fields at one point.
#[derive(Copy, Clone, Debug, Default)]
pub struct Fields {
    pub elev: f64, // macro elevation field (noise, -1..1), stamps not included
    pub temp: f64,
    pub moist: f64,
    pub land: f64,
    pub weird: f64,
}

impl Fields {
    pub fn get(&self, f: FieldName) -> f64 {
        match f {
            FieldName::Elevation => self.elev,
            FieldName::Temperature => self.temp,
            FieldName::Moisture => self.moist,
            FieldName::Landform => self.land,
            FieldName::Weirdness => self.weird,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Sample {
    pub height: f64,           // metres above the base radius
    pub height_above_sea: f64, // metres
    pub sea: f64,
    pub biome: i32,
    pub slope_deg: f64,
    /// Depth of the water over the ground: sea, lake or river (m, 0 when dry).
    pub water_depth: f64,
    pub temperature: f64,
    pub moisture: f64,
    pub landform: f64,
    pub weirdness: f64,
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
    pub drainage_ms: f64,
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
    /// How far land is from the nearest site (20,000 random land points): the worst and the
    /// median (m). The worst is the largest hole: what a hike to the next site can take (#70).
    pub site_cover_worst_m: f64,
    pub site_cover_median_m: f64,
    /// Placed per site kind, and the landmarks among them (#70).
    pub site_kind_counts: BTreeMap<String, usize>,
    pub landmark_count: usize,
    /// Rows below their `min_share`.
    pub quota_misses: Vec<String>,
    /// Stamps placed per landform kind, and the placement try that succeeded (#69).
    pub landform_counts: BTreeMap<String, usize>,
    pub landform_tries: u32,
    /// Share of 200 random straight walks of 540 m (5 min at 1.8 m/s, on land) that cross at
    /// least two biome rows, and the median number of rows per walk (#68: sizes by walking).
    pub walks_two_biomes_share: f64,
    pub walk_biomes_median: f64,
    /// Drainage (#72): river nodes (macro vertices on a river) and their length, the river ends
    /// at the sea and at a lake, the biggest rain-weighted catchment of any land vertex, lakes
    /// (area share of the whole surface),
    /// the lowering by erosion alone and the deepest cut of all (m).
    pub river_nodes: usize,
    pub river_length_km: f64,
    pub rivers_to_sea: usize,
    pub rivers_to_lake: usize,
    pub largest_catchment_km2: f64,
    /// The longest river from source to mouth, and the longest waterway on through lakes that
    /// spill (km).
    pub longest_river_km: f64,
    pub longest_waterway_km: f64,
    pub lake_count: usize,
    pub lake_area_share: f64,
    pub largest_lake_km2: f64,
    pub deepest_lake_m: f64,
    pub erosion_max_m: f64,
    pub erosion_mean_m: f64,
    pub carve_max_m: f64,
    /// Drainage time per step (ms): the grid, the noise ground, then the steps of `drain`.
    pub drainage_phases_ms: BTreeMap<String, f64>,
    /// The coarse global layer (#177): loaded from the cache or baked, the time it took (ms) and
    /// its size in memory (bytes).
    pub coarse_from_cache: bool,
    pub coarse_ms: f64,
    pub coarse_bytes: usize,
}

/// One macro vertex on a river (#72).
#[derive(Clone, Debug, PartialEq)]
pub struct River {
    pub dir: V3,
    /// Rain-weighted catchment (km²).
    pub catchment_km2: f64,
    /// Bed and water surface, metres above the base radius.
    pub bed_m: f64,
    pub level_m: f64,
    pub next: crate::drainage::Mouth,
    /// Where the water runs to (the next macro vertex, or this one at an end).
    pub to: V3,
    /// Depth of the bed under the ground and half its width (m).
    pub depth_m: f64,
    pub half_width_m: f64,
}

/// A lake (#72): its level, size, deepest point and where it spills.
#[derive(Clone, Debug, PartialEq)]
pub struct Lake {
    pub level_m: f64,
    pub area_m2: f64,
    pub depth_m: f64,
    pub deepest: V3,
    pub outlet: Option<V3>,
}

/// The four grid vertices of the coarse layer around a point and its place between them.
struct MacroCell {
    k: [usize; 4],
    fu: f64,
    fv: f64,
}

impl MacroCell {
    #[inline(always)]
    fn bil(&self, f: impl Fn(usize) -> f64) -> f64 {
        let [k00, k10, k01, k11] = self.k;
        let (a, b, cc, d) = (f(k00), f(k10), f(k01), f(k11));
        let x0 = a + (b - a) * self.fu;
        let x1 = cc + (d - cc) * self.fu;
        x0 + (x1 - x0) * self.fv
    }
}

pub struct Planet {
    pub recipe: Recipe,
    pub radius: f64,
    n_elev: FastNoiseLite,
    n_moist: FastNoiseLite,
    n_temp: FastNoiseLite,
    n_land: FastNoiseLite,
    n_weird: FastNoiseLite,
    bands: Vec<BandRt>,
    pub(crate) scatter: crate::scatter::ScatterRt,
    pub(crate) stamps: Vec<StampRt>,
    landform_tries: u32,
    placement_error: Option<String>,
    /// The coarse global layer (#177): the sea, the drainage's cut and water, rivers and lakes.
    pub coarse: Coarse,
    pub sea: f64,
    pub sites: Vec<crate::site::Site>,
    /// Hand-placed places, set before the bake (`set_places`).
    pub places: Vec<crate::place::Place>,
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
                scale: b.scale,
            })
            .collect();
        let scatter = crate::scatter::ScatterRt::new(&recipe, seed);
        let mut p = Planet {
            n_elev: make_noise(&m.elevation.noise, seed),
            n_moist: make_noise(&m.moisture.noise, seed),
            n_temp: make_noise(&m.temperature.noise, seed),
            n_land: make_noise(&m.landform.noise, seed),
            n_weird: make_noise(&m.weirdness.noise, seed),
            bands,
            scatter,
            stamps: Vec::new(),
            landform_tries: 0,
            placement_error: None,
            coarse: Coarse::default(),
            sea: 0.0,
            sites: Vec::new(),
            places: Vec::new(),
            baked: false,
            height_range: (0.0, 0.0),
            radius,
            recipe,
        };
        let (stamps, tries, ok) = p.place_landforms();
        p.stamps = stamps;
        p.landform_tries = tries;
        p.placement_error = ok.err();
        p
    }

    /// Macro fields straight from the noise (before or without the baked image).
    pub fn fields_at(&self, dir: V3) -> Fields {
        let p = self.p32(dir);
        let tm = &self.recipe.macro_.temperature;
        Fields {
            elev: nz(&self.n_elev, p) as f64,
            temp: tm.base - tm.latitude_gain * dir.y.abs() + tm.noise_gain * nz(&self.n_temp, p) as f64,
            moist: nz(&self.n_moist, p) as f64,
            land: nz(&self.n_land, p) as f64,
            weird: nz(&self.n_weird, p) as f64,
        }
    }

    #[inline(always)]
    pub fn p32(&self, dir: V3) -> [f32; 3] {
        [(dir.x * self.radius) as f32, (dir.y * self.radius) as f32, (dir.z * self.radius) as f32]
    }

    /// Sum of the stamped features (metres) and the landform value they force, if any.
    pub fn stamp_height(&self, dir: V3) -> (f64, Option<f64>) {
        let mut h = 0.0;
        let mut lf = None;
        for s in &self.stamps {
            let (dh, w) = s.height(dir, self.radius);
            h += dh;
            if w > 0.5 && s.landform.is_some() {
                lf = s.landform;
            }
        }
        (h, lf)
    }

    fn bands_height(&self, p: [f32; 3], f: &Fields) -> f64 {
        let sh = &self.recipe.shape;
        let stretch = sh.stretch.eval(f.get(sh.stretch.field));
        let rough = sh.roughness.eval(f.get(sh.roughness.field));
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
            let k = match b.scale {
                BandScale::None => 1.0,
                BandScale::Stretch => stretch,
                BandScale::Roughness => rough,
            };
            h += v as f64 * b.amp * k;
        }
        h
    }

    /// Height offset of the shape at the macro fields (metres, stamps and bands not included).
    pub fn shape_offset(&self, f: &Fields) -> f64 {
        let o = &self.recipe.shape.offset;
        o.eval(f.get(o.field))
    }

    /// The grid vertices around face coordinates (a, b in [-1, 1]). Vertex-centred grid: edge
    /// samples sit exactly on the cube edges, so both faces interpolate the same samples along a
    /// shared edge and there is no seam.
    fn macro_cell(&self, face: usize, a: f64, b: f64) -> MacroCell {
        let n = self.recipe.macro_.resolution;
        let nn = n as f64;
        let u = ((a + 1.0) * 0.5 * nn).clamp(0.0, nn);
        let v = ((b + 1.0) * 0.5 * nn).clamp(0.0, nn);
        let i0 = (u.floor() as usize).min(n - 1);
        let j0 = (v.floor() as usize).min(n - 1);
        let w = n + 1;
        let idx = |i: usize, j: usize| (face * w + j) * w + i;
        MacroCell { k: [idx(i0, j0), idx(i0 + 1, j0), idx(i0, j0 + 1), idx(i0 + 1, j0 + 1)], fu: u - i0 as f64, fv: v - j0 as f64 }
    }

    /// The drainage's change of the ground at face coordinates (m); 0 before the bake.
    fn carve_ab(&self, face: usize, a: f64, b: f64) -> f64 {
        if self.coarse.carve.is_empty() {
            return 0.0;
        }
        let c = &self.coarse.carve;
        self.macro_cell(face, a, b).bil(|k| c[k] as f64 * coarse::UNIT_M as f64)
    }

    /// Water surface at face coordinates (m above the base radius), lakes and rivers only (#72);
    /// None where no surface is defined. Wet where it lies above the ground.
    pub fn water_level_ab(&self, face: usize, a: f64, b: f64) -> Option<f64> {
        if self.coarse.water.is_empty() {
            return None;
        }
        let w = &self.coarse.water;
        let l = self.macro_cell(face, a, b).bil(|k| coarse::dequantize_water(w[k]) as f64);
        (l > crate::drainage::DRY_BELOW).then_some(l)
    }

    /// Whether a lake or river is near: any macro vertex within about `radius_m` (a square on the
    /// face of the direction) with a water surface defined, its shore ring included.
    pub fn water_within(&self, dir: V3, radius_m: f64) -> bool {
        if self.coarse.water.is_empty() {
            return false;
        }
        let d = dir.normalized();
        let face = face_of(d);
        let (a, b) = sphere_to_face_ab(face, d);
        let n = self.recipe.macro_.resolution;
        let e = 1e-4;
        // Metres per unit of a and b here.
        let ma = (cube_to_sphere(face, a + e, b) - cube_to_sphere(face, a - e, b)).length() / (2.0 * e) * self.radius;
        let mb = (cube_to_sphere(face, a, b + e) - cube_to_sphere(face, a, b - e)).length() / (2.0 * e) * self.radius;
        let at = |x: f64| ((x + 1.0) * 0.5 * n as f64).round() as isize;
        let (ri, rj) = ((radius_m / ma * 0.5 * n as f64).ceil() as isize, (radius_m / mb * 0.5 * n as f64).ceil() as isize);
        let (i0, j0) = (at(a), at(b));
        let w = n + 1;
        for j in (j0 - rj).max(0)..=(j0 + rj).min(n as isize) {
            for i in (i0 - ri).max(0)..=(i0 + ri).min(n as isize) {
                if coarse::dequantize_water(self.coarse.water[(face * w + j as usize) * w + i as usize]) as f64 > crate::drainage::DRY_BELOW {
                    return true;
                }
            }
        }
        false
    }

    /// Depth of any water over the ground at face coordinates, given the ground height there.
    pub fn water_depth_ab(&self, face: usize, a: f64, b: f64, h: f64) -> f64 {
        let lake = self.water_level_ab(face, a, b).unwrap_or(f64::MIN);
        (lake.max(self.sea) - h).max(0.0)
    }

    /// THE height function: metres above the base radius, from face coordinates. The noise
    /// ground (shape, stamps, bands), then the ground edits of the sites (#70).
    pub fn height_ab(&self, face: usize, a: f64, b: f64, dir: V3) -> (f64, Fields) {
        let (h, f) = self.base_height_ab(face, a, b, dir);
        (self.apply_edits(dir, h), f)
    }

    /// The height function without the site edits: the noise ground and the drainage's cut.
    pub fn base_height_ab(&self, face: usize, a: f64, b: f64, dir: V3) -> (f64, Fields) {
        let f = self.fields_at(dir);
        let h = self.shape_offset(&f) + self.stamp_height(dir).0 + self.bands_height(self.p32(dir), &f) + self.carve_ab(face, a, b);
        (h, f)
    }

    /// The noise ground alone (shape, stamps, bands): what the drainage starts from.
    fn noise_height_ab(&self, dir: V3) -> (f64, Fields) {
        let f = self.fields_at(dir);
        (self.shape_offset(&f) + self.stamp_height(dir).0 + self.bands_height(self.p32(dir), &f), f)
    }

    pub fn base_height_at(&self, dir: V3) -> f64 {
        let d = dir.normalized();
        let face = face_of(d);
        let (a, b) = sphere_to_face_ab(face, d);
        self.base_height_ab(face, a, b, d).0
    }

    /// THE height function, from a unit direction (inverts the cube mapping first).
    pub fn height_at(&self, dir: V3) -> f64 {
        let d = dir.normalized();
        let face = face_of(d);
        let (a, b) = sphere_to_face_ab(face, d);
        self.height_ab(face, a, b, d).0
    }

    /// Temperature at a point: the field less the lapse with height above the sea.
    pub fn temperature_at(&self, f: &Fields, h_above_sea: f64) -> f64 {
        f.temp - self.recipe.macro_.temperature.lapse_per_m * h_above_sea.max(0.0)
    }

    /// The biome row nearest to the point in parameter space (#68): per row the sum of squared
    /// distances to its points or intervals over the axes it names, plus its offset. Ties go to
    /// the row listed first.
    pub fn biome_for(&self, h_above_sea: f64, f: &Fields, forced_landform: Option<f64>) -> u8 {
        let temp = self.temperature_at(f, h_above_sea);
        let land = forced_landform.unwrap_or(f.land);
        let hs = self.recipe.biome_space.height_scale_m.max(1e-6);
        let mut best = (f64::MAX, self.recipe.biomes[0].id);
        for row in &self.recipe.biomes {
            let c = &row.climate;
            let mut d = row.offset;
            for (r, v) in [(c.elevation, f.elev), (c.temperature, temp), (c.moisture, f.moist), (c.landform, land), (c.weirdness, f.weird), (c.height_above_sea, h_above_sea / hs)] {
                if let Some(r) = r {
                    let x = r.distance(v);
                    d += x * x;
                }
            }
            if d < best.0 {
                best = (d, row.id);
            }
        }
        best.1
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
            water_depth: self.water_depth_ab(face, a, b, h),
            temperature: self.temperature_at(&f, ha),
            moisture: f.moist,
            landform: lf.unwrap_or(f.land),
            weirdness: f.weird,
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

    pub fn palette(&self, id: u8) -> Palette {
        self.recipe.biomes.iter().find(|b| b.id == id).map(|b| b.palette.clone()).unwrap_or(Palette { ground: [1.0, 0.0, 1.0], rock: [1.0, 0.0, 1.0], strata: 0.0, cap: 0.0 })
    }

    pub fn biome_color(&self, id: u8) -> [f32; 3] {
        self.recipe.biomes.iter().find(|b| b.id == id).map(|b| b.color).unwrap_or([1.0, 0.0, 1.0])
    }

    pub fn sites_near(&self, dir: V3, radius_m: f64) -> Vec<&crate::site::Site> {
        let d = dir.normalized();
        self.sites.iter().filter(|s| self.radius * s.dir.dot(d).clamp(-1.0, 1.0).acos() <= radius_m + s.footprint_m).collect()
    }

    /// Coarse layer, sites, statistics. `threads` = 0 means all cores.
    pub fn bake(&mut self, threads: usize) -> BakeStats {
        self.bake_with(threads, None, true)
    }

    /// `bake` with the coarse layer cached in `cache` (loaded when one fits, else baked and
    /// written), and `full_stats` = false skipping the statistics a game never reads (walks, the
    /// site coverage, the stamp features); the sea, the drainage and the sites stay.
    pub fn bake_with(&mut self, threads: usize, cache: Option<&Path>, full_stats: bool) -> BakeStats {
        let t_all = Instant::now();
        let threads = if threads == 0 { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) } else { threads };
        let mut st = BakeStats { threads, ..Default::default() };
        let n = self.recipe.macro_.resolution;
        let w = n + 1;

        // 1-3. the coarse global layer: sea level, rivers and lakes; from the cache when it fits
        let t0 = Instant::now();
        let key = coarse::cache_key(self.recipe.source_hash, self.recipe.seed, self.radius, n);
        let cached = cache.and_then(|d| Coarse::load(d, key)).filter(|c| c.n == n && c.carve.len() == 6 * w * w && c.water.len() == 6 * w * w);
        if let Some(c) = cached {
            self.sea = c.sea;
            self.coarse = c;
            st.coarse_from_cache = true;
            st.sea_level_m = self.sea;
        } else {
            self.bake_coarse(threads, &mut st);
            if let Some(d) = cache
                && let Err(e) = self.coarse.save(d, key)
            {
                eprintln!("planet_core: coarse layer not cached in {}: {e}", d.display());
            }
        }
        st.coarse_ms = t0.elapsed().as_secs_f64() * 1e3;
        st.coarse_bytes = self.coarse.bytes();

        // 4. sites (#70): placed on the noise ground, then their edits join the height function
        let t0 = Instant::now();
        let (sites, misses) = self.place_sites_v2();
        self.sites = sites;
        st.quota_misses.extend(misses);
        if full_stats {
            self.site_stats(&mut st);
        }
        st.sites_ms = t0.elapsed().as_secs_f64() * 1e3;

        let hr;
        // 5. statistics of the full height function on a stride-2 grid
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
                let share = tot.biomes.get(&row.id).copied().unwrap_or(0.0) / tot.w;
                st.biome_area_share.insert(row.id.to_string(), share);
                if let Some(min) = row.min_share
                    && share < min
                {
                    st.quota_misses.push(format!("biome row {}: area share {:.2} % below its quota {:.2} %", row.id, share * 100.0, min * 100.0));
                }
            }
        }
        self.height_range = hr;
        if full_stats {
            self.feature_stats(&mut st);
            self.walk_stats(&mut st, threads);
        }
        st.stats_ms = t0.elapsed().as_secs_f64() * 1e3;

        self.baked = true;
        st.bake_ms = t_all.elapsed().as_secs_f64() * 1e3;
        st
    }

    /// `bake`, failing when a biome row misses its quota (`min_share`).
    pub fn bake_checked(&mut self, threads: usize) -> Result<BakeStats, String> {
        self.bake_checked_with(threads, None, true)
    }

    /// What went wrong placing the landforms, if anything.
    pub fn placement_error(&self) -> Option<&str> {
        self.placement_error.as_deref()
    }

    /// `bake_with`, failing like `bake_checked`.
    pub fn bake_checked_with(&mut self, threads: usize, cache: Option<&Path>, full_stats: bool) -> Result<BakeStats, String> {
        let st = self.bake_with(threads, cache, full_stats);
        if let Some(e) = &self.placement_error {
            return Err(format!("bake: {e}"));
        }
        if st.quota_misses.is_empty() { Ok(st) } else { Err(format!("bake: {}", st.quota_misses.join("; "))) }
    }

    /// The coarse global layer from recipe and seed: the sea level (an area-weighted percentile of
    /// the noise ground at the grid vertices), then the rivers and lakes of the drainage.
    fn bake_coarse(&mut self, threads: usize, st: &mut BakeStats) {
        let n = self.recipe.macro_.resolution;
        let w = n + 1;
        let t0 = Instant::now();
        let parts = {
            let this = &*self;
            par_rows(6 * w, threads, |r0, r1| {
                let mut out = Vec::with_capacity((r1 - r0) * w);
                for row in r0..r1 {
                    let (face, j) = (row / w, row % w);
                    let b = -1.0 + j as f64 * 2.0 / n as f64;
                    for i in 0..w {
                        let a = -1.0 + i as f64 * 2.0 / n as f64;
                        let dir = cube_to_sphere(face, a, b);
                        let f = this.fields_at(dir);
                        out.push(((this.shape_offset(&f) + this.stamp_height(dir).0) as f32, this.area_weight(face, a, b) as f32));
                    }
                }
                out
            })
        };
        let mut pairs: Vec<(f32, f32)> = parts.into_iter().flatten().collect();
        st.macro_ms = t0.elapsed().as_secs_f64() * 1e3;

        let t0 = Instant::now();
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
        st.sea_ms = t0.elapsed().as_secs_f64() * 1e3;

        self.coarse = Coarse { n, sea: self.sea, carve: vec![0; 6 * w * w], water: vec![coarse::NO_WATER_RAW; 6 * w * w], ..Default::default() };
        // rivers and lakes (#72): cut into the noise ground before the sites look for space
        let t0 = Instant::now();
        if let Some(spec) = self.recipe.drainage.clone() {
            self.drain(&spec, threads, st);
        }
        st.drainage_ms = t0.elapsed().as_secs_f64() * 1e3;
    }

    /// Rivers and lakes (#72) on the coarse grid: the cut and the water surface go into the
    /// coarse layer, the rivers and lakes into its lists.
    fn drain(&mut self, spec: &DrainageSpec, threads: usize, st: &mut BakeStats) {
        use crate::drainage::{self, Mesh, Mouth};
        let n = self.recipe.macro_.resolution;
        let w = n + 1;
        let t0 = Instant::now();
        let grid = drainage::MacroGrid::new(n, self.radius, threads);
        st.drainage_phases_ms.insert("0 grid".into(), t0.elapsed().as_secs_f64() * 1e3);
        let t0 = Instant::now();
        let (h0, rain) = {
            let (this, grid) = (&*self, &grid);
            let parts = par_rows(6 * w, threads, |r0, r1| {
                let mut h = Vec::with_capacity((r1 - r0) * w);
                let mut rain = Vec::with_capacity((r1 - r0) * w);
                for row in r0..r1 {
                    let (face, j) = (row / w, row % w);
                    let b = -1.0 + j as f64 * 2.0 / n as f64;
                    for i in 0..w {
                        let k = row * w + i;
                        if !grid.is_node(k) {
                            h.push(0.0);
                            rain.push(0.0);
                            continue;
                        }
                        let a = -1.0 + i as f64 * 2.0 / n as f64;
                        let (hh, f) = this.noise_height_ab(cube_to_sphere(face, a, b));
                        h.push(hh as f32);
                        rain.push((grid.area_m2(k) * (spec.rain_base + spec.rain_moisture_gain * f.moist).max(0.0)) as f32);
                    }
                }
                (h, rain)
            });
            let (mut h, mut rain) = (Vec::with_capacity(6 * w * w), Vec::with_capacity(6 * w * w));
            for (a, b) in parts {
                h.extend(a);
                rain.extend(b);
            }
            (h, rain)
        };
        st.drainage_phases_ms.insert("1 ground".into(), t0.elapsed().as_secs_f64() * 1e3);
        let d = drainage::drain(&grid, &h0, &rain, self.sea as f32, spec);
        for (i, (name, ms)) in d.phases_ms.iter().enumerate() {
            st.drainage_phases_ms.insert(format!("{} {name}", i + 2), *ms);
        }
        for k in 0..6 * w * w {
            let c = grid.canon(k);
            self.coarse.carve[k] = coarse::quantize_carve(d.carve[c]);
            self.coarse.water[k] = coarse::quantize_water(d.water[c]);
        }
        self.coarse.lakes = d
            .lakes
            .iter()
            .map(|l| Lake {
                level_m: l.level_m as f64,
                area_m2: l.area_m2,
                depth_m: l.depth_m as f64,
                deepest: grid.dir(l.deepest as usize),
                outlet: (l.outlet != drainage::NONE).then(|| grid.dir(l.outlet as usize)),
            })
            .collect();
        self.coarse.rivers = d
            .rivers
            .iter()
            .map(|r| {
                let dir = grid.dir(r.node as usize);
                River {
                    dir,
                    catchment_km2: r.catchment_m2 * 1e-6,
                    bed_m: r.bed_m as f64,
                    level_m: r.level_m as f64,
                    next: r.next,
                    to: if r.rcv == drainage::NONE { dir } else { grid.dir(r.rcv as usize) },
                    depth_m: r.depth_m as f64,
                    half_width_m: r.half_width_m as f64,
                }
            })
            .collect();

        let spacing = std::f64::consts::FRAC_PI_2 * self.radius / n as f64;
        st.river_nodes = d.rivers.len();
        st.river_length_km = d
            .rivers
            .iter()
            .map(|r| match r.next {
                Mouth::River(j) => grid.dist_m(r.node as usize, d.rivers[j as usize].node as usize),
                _ => spacing,
            })
            .sum::<f64>()
            * 1e-3
            + 0.0; // an empty float sum is -0.0
        st.rivers_to_sea = d.rivers.iter().filter(|r| r.next == Mouth::Sea).count();
        st.rivers_to_lake = d.rivers.iter().filter(|r| matches!(r.next, Mouth::Lake(_))).count();
        st.largest_catchment_km2 = d.max_catchment_m2 * 1e-6;
        st.longest_river_km = d.longest_river_m * 1e-3;
        st.longest_waterway_km = d.longest_waterway_m * 1e-3;
        st.lake_count = d.lakes.len();
        st.lake_area_share = d.lakes.iter().map(|l| l.area_m2).sum::<f64>() / (4.0 * std::f64::consts::PI * self.radius * self.radius);
        st.largest_lake_km2 = d.lakes.iter().map(|l| l.area_m2 * 1e-6).fold(0.0, f64::max);
        st.deepest_lake_m = d.lakes.iter().map(|l| l.depth_m as f64).fold(0.0, f64::max);
        st.erosion_max_m = d.erosion_max_m;
        st.erosion_mean_m = d.erosion_mean_m;
        st.carve_max_m = -d.carve.iter().copied().fold(0.0f32, f32::min) as f64;
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
        st.landform_tries = self.landform_tries;
        for k in &self.recipe.landforms.kinds {
            st.landform_counts.insert(k.id.clone(), self.stamps.iter().filter(|s| s.kind == k.id).count());
        }
        // The first basin, plateau and escarpment (the spike's three features).
        let first = |name: &str| self.stamps.iter().find(|s| match (&s.shape, name) {
            (ShapeRt::Basin { .. }, "basin") | (ShapeRt::Plateau { .. }, "plateau") | (ShapeRt::Esc { .. }, "esc") => true,
            _ => false,
        });
        if let Some(s) = first("basin")
            && let ShapeRt::Basin { r, .. } = s.shape
        {
            let c = s.c;
            let t1 = c.cross(v3(0.0, 1.0, 0.0)).normalized();
            let t2 = c.cross(t1).normalized();
            st.basin_centre_height_above_sea = self.height_at(c) - self.sea;
            let (r1, s1) = self.run_below_sea(c, t1, r * 1.5);
            let (r2, s2) = self.run_below_sea(c, t2, r * 1.5);
            st.basin_run_through_centre_m = [r1, r2];
            st.basin_span_below_sea_m = [s1, s2];
        }
        if let Some(s) = first("plateau")
            && let ShapeRt::Plateau { r, .. } = s.shape
        {
            let c = s.c;
            st.plateau_stamp_height_m = self.stamp_height(c).0;
            let t1 = c.cross(v3(0.0, 1.0, 0.0)).normalized();
            let t2 = c.cross(t1);
            let (mut sum, mut cnt) = (0.0, 0.0);
            for ia in -5..=5 {
                for ib in -5..=5 {
                    let (x, y) = (ia as f64 * r * 0.1, ib as f64 * r * 0.1);
                    if x.hypot(y) <= r * 0.5 {
                        let d = (c + t1 * (x / self.radius) + t2 * (y / self.radius)).normalized();
                        sum += self.height_at(d) - self.sea;
                        cnt += 1.0;
                    }
                }
            }
            st.plateau_top_mean_above_sea_m = sum / cnt;
        }
        if let Some(s) = first("esc")
            && let ShapeRt::Esc { t, n, len, .. } = s.shape
        {
            let c = s.c;
            let off = 200.0 / self.radius;
            let step_at = |l: f64| {
                let base = c * (l / self.radius).cos() + t * (l / self.radius).sin();
                let hi = (base + n * off).normalized();
                let lo = (base - n * off).normalized();
                (self.stamp_height(hi).0 - self.stamp_height(lo).0, self.height_at(hi) - self.height_at(lo))
            };
            st.escarpment_stamp_step_m = step_at(0.0).0;
            let k = 11;
            st.escarpment_full_step_mean_m = (0..k).map(|i| step_at((i as f64 / (k - 1) as f64 - 0.5) * len * 0.6).1).sum::<f64>() / k as f64;
        }
    }

    fn walk_stats(&self, st: &mut BakeStats, threads: usize) {
        const WALKS: usize = 200;
        // One walk per index, its own random start (on land) and heading: parallel and the same
        // result for any thread count.
        let parts = par_rows(WALKS, threads, |i0, i1| {
            let mut out = Vec::with_capacity(i1 - i0);
            for i in i0..i1 {
                let mut rng = 0x5DEECE66Du64 ^ (self.recipe.seed as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (i as u64 + 1).wrapping_mul(0xBF58476D1CE4E5B9);
                let mut next = || {
                    rng ^= rng << 13;
                    rng ^= rng >> 7;
                    rng ^= rng << 17;
                    (rng >> 11) as f64 / (1u64 << 53) as f64
                };
                let d = loop {
                    let z = next() * 2.0 - 1.0;
                    let phi = next() * std::f64::consts::TAU;
                    let rr = (1.0 - z * z).sqrt();
                    let d = v3(rr * phi.cos(), z, rr * phi.sin());
                    if self.height_at(d) > self.sea {
                        break d;
                    }
                };
                let (e, n) = crate::look::tangent_frame(d);
                let a = next() * std::f64::consts::TAU;
                let t = e * a.cos() + n * a.sin();
                let mut seen: Vec<u8> = Vec::new();
                for k in 0..=54 {
                    let p = crate::look::walk(d, t, k as f64 * 10.0, self.radius);
                    let face = face_of(p);
                    let (a, b) = sphere_to_face_ab(face, p);
                    let (h, f) = self.height_ab(face, a, b, p);
                    let row = self.biome_for(h - self.sea, &f, self.stamp_height(p).1);
                    if !seen.contains(&row) {
                        seen.push(row);
                    }
                }
                out.push(seen.len());
            }
            out
        });
        let mut counts: Vec<usize> = parts.into_iter().flatten().collect();
        counts.sort();
        st.walks_two_biomes_share = counts.iter().filter(|c| **c >= 2).count() as f64 / WALKS as f64;
        st.walk_biomes_median = counts[WALKS / 2] as f64;
    }

    fn site_stats(&self, st: &mut BakeStats) {
        for k in &self.recipe.sites.kinds {
            st.site_kind_counts.insert(k.id.clone(), self.sites.iter().filter(|s| s.id == k.id).count());
        }
        st.landmark_count = self.sites.iter().filter(|s| s.category == SiteCategory::Landmark).count();
        // Gaps between sites proper (not landmarks or finds).
        let sites: Vec<V3> = self.sites.iter().filter(|s| s.category == SiteCategory::Site).map(|s| s.dir).collect();
        let n = sites.len();
        st.site_count = n;
        if n < 2 {
            return;
        }
        let dist = |a: &V3, b: &V3| self.radius * a.dot(*b).clamp(-1.0, 1.0).acos();
        let mut min_pair = f64::MAX;
        let (mut sum, mut max) = (0.0, 0.0f64);
        let mut nns = Vec::with_capacity(n);
        for (i, a) in sites.iter().enumerate() {
            let mut nn = f64::MAX;
            for (j, b) in sites.iter().enumerate() {
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
        let mut rng = 0x243F6A8885A308D3u64 ^ (self.recipe.seed as u64);
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut cover = Vec::with_capacity(20_000);
        while cover.len() < 20_000 {
            let z = next() * 2.0 - 1.0;
            let phi = next() * std::f64::consts::TAU;
            let rr = (1.0 - z * z).sqrt();
            let d = v3(rr * phi.cos(), z, rr * phi.sin());
            // Land by the macro height (cheap; the sea level is the macro percentile).
            let f = self.fields_at(d);
            if self.shape_offset(&f) + self.stamp_height(d).0 <= self.sea {
                continue;
            }
            cover.push(sites.iter().map(|s| dist(s, &d)).fold(f64::MAX, f64::min));
        }
        cover.sort_by(f64::total_cmp);
        st.site_cover_worst_m = cover[cover.len() - 1];
        st.site_cover_median_m = cover[cover.len() / 2];
        st.site_min_pair_m = min_pair;
        st.site_mean_nn_m = sum / n as f64;
        st.site_max_nn_m = max;
    }
}
