//! The look harness's planet side (#63): an equirectangular atlas per layer (height, biome,
//! landform, scatter density) and named spots for fixed viewpoints. No window, no GPU.
use crate::math::*;
use crate::planet::*;
use serde::Deserialize;

/// A fixed viewpoint (`content/look/viewpoints.json`). `spot` names a place from
/// `Planet::spot`, or `orbit`: then the camera is `height_m` from the centre along `from`,
/// looking at the centre. On the ground the camera stands `back_m` behind the spot (against its
/// facing), `height_m` above the ground there, turned by `turn_deg` and pitched by `pitch_deg`.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Viewpoint {
    pub id: String,
    pub spot: String,
    pub height_m: f64,
    #[serde(default)]
    pub back_m: f64,
    #[serde(default)]
    pub pitch_deg: f64,
    #[serde(default)]
    pub turn_deg: f64,
    #[serde(default)]
    pub from: Option<[f64; 3]>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Viewpoints {
    #[serde(rename = "_comment", default)]
    pub comment: String,
    /// Sun for every ground viewpoint, in the spot's frame: degrees above the horizon and
    /// degrees clockwise from the facing. Orbit viewpoints get the sun behind the camera's left.
    pub sun_elevation_deg: f64,
    pub sun_azimuth_deg: f64,
    pub atlas_width: usize,
    pub viewpoints: Vec<Viewpoint>,
}

impl Viewpoints {
    pub fn from_json(s: &str) -> Result<Viewpoints, String> {
        let v: Viewpoints = serde_json::from_str(s).map_err(|e| e.to_string())?;
        let known = ["orbit", "basin", "rim", "plateau", "site", "forest_edge", "coast"];
        for p in &v.viewpoints {
            if !known.contains(&p.spot.as_str()) {
                return Err(format!("viewpoint {}: unknown spot {}", p.id, p.spot));
            }
            if p.spot == "orbit" && p.from.is_none() {
                return Err(format!("viewpoint {}: orbit needs `from`", p.id));
            }
        }
        Ok(v)
    }
}

/// One atlas layer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AtlasLayer {
    Height,
    Biome,
    Landform,
    Scatter,
}

impl AtlasLayer {
    pub const ALL: [AtlasLayer; 4] = [AtlasLayer::Height, AtlasLayer::Biome, AtlasLayer::Landform, AtlasLayer::Scatter];
    pub fn name(self) -> &'static str {
        match self {
            AtlasLayer::Height => "height",
            AtlasLayer::Biome => "biome",
            AtlasLayer::Landform => "landform",
            AtlasLayer::Scatter => "scatter",
        }
    }
}

/// Equirectangular maps of one planet, RGB8 rows from the north pole down, longitude -180..180.
pub struct Atlas {
    pub width: usize,
    pub height: usize,
    pub layers: Vec<(AtlasLayer, Vec<u8>)>,
}

/// A place a viewpoint stands at: a unit direction from the planet centre and the direction
/// (unit tangent) the camera faces there.
#[derive(Copy, Clone, Debug)]
pub struct Spot {
    pub dir: V3,
    pub facing: V3,
}

/// Direction of an equirectangular pixel centre.
pub fn pixel_dir(x: usize, y: usize, w: usize, h: usize) -> V3 {
    let lon = (x as f64 + 0.5) / w as f64 * std::f64::consts::TAU - std::f64::consts::PI;
    let lat = std::f64::consts::FRAC_PI_2 - (y as f64 + 0.5) / h as f64 * std::f64::consts::PI;
    v3(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
}

/// Local east and north at a direction (north towards +y; near the poles east follows +x).
pub fn tangent_frame(d: V3) -> (V3, V3) {
    let up = if d.y.abs() < 0.99 { v3(0.0, 1.0, 0.0) } else { v3(1.0, 0.0, 0.0) };
    let east = up.cross(d).normalized();
    (east, d.cross(east))
}

/// Move `metres` along the surface from `d` in tangent direction `t`.
pub fn walk(d: V3, t: V3, metres: f64, radius: f64) -> V3 {
    let a = metres / radius;
    (d * a.cos() + t * a.sin()).normalized()
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn to_u8(c: [f32; 3]) -> [u8; 3] {
    [(c[0].clamp(0.0, 1.0) * 255.0) as u8, (c[1].clamp(0.0, 1.0) * 255.0) as u8, (c[2].clamp(0.0, 1.0) * 255.0) as u8]
}

/// Fixed palette for landform classes and other ids (not recipe colours: a debug map).
fn id_color(i: i32) -> [f32; 3] {
    const P: [[f32; 3]; 8] = [
        [0.90, 0.60, 0.20], [0.30, 0.70, 0.90], [0.60, 0.85, 0.35], [0.85, 0.35, 0.55],
        [0.95, 0.90, 0.40], [0.50, 0.45, 0.90], [0.40, 0.90, 0.75], [0.75, 0.75, 0.75],
    ];
    P[i.rem_euclid(8) as usize]
}

struct Px {
    ha: f64,
    biome: u8,
    land: i32,
    scatter: f32,
}

impl Planet {
    /// Density (0..1) of the densest scatter rule at a point, as the chunk build would roll it
    /// (mask and biome row; slope and site filters left out).
    pub fn scatter_density_at(&self, dir: V3, biome: u8, h_above_sea: f64) -> f32 {
        let p = self.p32(dir);
        let mut best: f32 = 0.0;
        for (ri, rule) in self.recipe.scatter.iter().enumerate() {
            if rule.above_sea && h_above_sea <= 0.0 {
                continue;
            }
            if nz(&self.masks[ri], p) <= rule.mask.threshold {
                continue;
            }
            let dens = rule.row_density.get(&biome.to_string()).or_else(|| rule.row_density.get("default")).copied().unwrap_or(0.0);
            best = best.max(dens);
        }
        best
    }

    /// The atlas at `width` x `width / 2` pixels. Sites are white dots on the biome layer.
    pub fn atlas(&self, width: usize, threads: usize) -> Atlas {
        let (w, h) = (width, width / 2);
        let threads = if threads == 0 { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) } else { threads };
        let rows = par_rows(h, threads, |y0, y1| {
            let mut out = Vec::with_capacity((y1 - y0) * w);
            for y in y0..y1 {
                for x in 0..w {
                    let d = pixel_dir(x, y, w, h);
                    let face = face_of(d);
                    let (a, b) = sphere_to_face_ab(face, d);
                    let (hh, f) = self.height_ab(face, a, b, d);
                    let lf = self.stamp_height(d).1;
                    let ha = hh - self.sea;
                    let biome = self.biome_for(ha, &f, lf);
                    out.push(Px { ha, biome, land: lf.unwrap_or(f.land), scatter: self.scatter_density_at(d, biome, ha) });
                }
            }
            out
        });
        let px: Vec<Px> = rows.into_iter().flatten().collect();
        let (lo, hi) = (self.height_range.0 - self.sea, self.height_range.1 - self.sea);
        let mut height = Vec::with_capacity(w * h * 3);
        let mut biome = Vec::with_capacity(w * h * 3);
        let mut land = Vec::with_capacity(w * h * 3);
        let mut scatter = Vec::with_capacity(w * h * 3);
        // Metres per pixel along a parallel / meridian, for the hillshade.
        let my = std::f64::consts::PI * self.radius / h as f64;
        for y in 0..h {
            let lat = std::f64::consts::FRAC_PI_2 - (y as f64 + 0.5) / h as f64 * std::f64::consts::PI;
            let mx = (std::f64::consts::TAU * self.radius * lat.cos() / w as f64).max(1.0);
            for x in 0..w {
                let p = &px[y * w + x];
                let at = |dx: isize, dy: isize| {
                    let xx = (x as isize + dx).rem_euclid(w as isize) as usize;
                    let yy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                    px[yy * w + xx].ha
                };
                let gx = (at(1, 0) - at(-1, 0)) / (2.0 * mx);
                let gy = (at(0, -1) - at(0, 1)) / (2.0 * my);
                // Light from the north-west, 45 degrees up.
                let n = v3(-gx, -gy, 1.0).normalized();
                let shade = (0.55 + 0.45 * n.dot(v3(-0.5, 0.5, 0.707).normalized())).clamp(0.25, 1.0) as f32;
                let c = if p.ha <= 0.0 {
                    let t = (p.ha / lo.min(-1.0)) as f32;
                    lerp3([0.35, 0.55, 0.80], [0.05, 0.12, 0.35], t)
                } else {
                    let t = (p.ha / hi.max(1.0)) as f32;
                    if t < 0.5 { lerp3([0.25, 0.55, 0.25], [0.65, 0.55, 0.35], t * 2.0) } else { lerp3([0.65, 0.55, 0.35], [0.97, 0.97, 0.97], t * 2.0 - 1.0) }
                };
                height.extend(to_u8([c[0] * shade, c[1] * shade, c[2] * shade]));
                let bc = self.biome_color(p.biome);
                let bs = if p.ha <= 0.0 { 0.55 } else { shade };
                biome.extend(to_u8([bc[0] * bs, bc[1] * bs, bc[2] * bs]));
                let lc = id_color(p.land);
                land.extend(to_u8([lc[0] * shade, lc[1] * shade, lc[2] * shade]));
                let s = if p.ha <= 0.0 { [0.05, 0.08, 0.2] } else { lerp3([0.12, 0.10, 0.08], [0.3, 1.0, 0.35], p.scatter) };
                scatter.extend(to_u8(s));
            }
        }
        for s in &self.sites {
            mark(&mut biome, w, h, *s, [255, 255, 255]);
        }
        Atlas {
            width: w,
            height: h,
            layers: vec![(AtlasLayer::Height, height), (AtlasLayer::Biome, biome), (AtlasLayer::Landform, land), (AtlasLayer::Scatter, scatter)],
        }
    }

    /// A named spot for the look harness: `basin` (its shore, facing the centre), `rim` (on
    /// top of the escarpment, facing down it), `plateau` (near the top's edge, facing out),
    /// `site` (30 m from the first site, facing it), `forest_edge` (outside a forest, facing
    /// in), `coast` (just above the sea, facing it). None when the planet has no such place.
    pub fn spot(&self, kind: &str) -> Option<Spot> {
        let r = self.radius;
        match kind {
            "basin" | "rim" | "plateau" => {
                for s in &self.stamps {
                    match (kind, s) {
                        ("basin", StampRt::Basin { c, r: br, .. }) => {
                            let (e, _) = tangent_frame(*c);
                            // From the centre outwards until the ground is 3 m above the sea.
                            let mut m = 0.0;
                            while m < br * 1.5 {
                                let d = walk(*c, e, m, r);
                                if self.height_at(d) - self.sea > 3.0 {
                                    let to_c = (*c - d * c.dot(d)).normalized();
                                    return Some(Spot { dir: d, facing: to_c });
                                }
                                m += 10.0;
                            }
                            return Some(Spot { dir: walk(*c, e, br * 0.6, r), facing: -e });
                        }
                        ("rim", StampRt::Esc { c, n, spec, .. }) => {
                            let d = walk(*c, *n, spec.2 * 0.5 + 15.0, r);
                            let down = -(*n - d * n.dot(d)).normalized();
                            return Some(Spot { dir: d, facing: down });
                        }
                        ("plateau", StampRt::Plateau { c, r: pr, fall, .. }) => {
                            let (e, _) = tangent_frame(*c);
                            let d = walk(*c, e, (pr - fall - 20.0).max(0.0), r);
                            let out = (e - d * e.dot(d)).normalized();
                            return Some(Spot { dir: d, facing: out });
                        }
                        _ => {}
                    }
                }
                None
            }
            "site" => {
                let s = *self.sites.first()?;
                let (e, _) = tangent_frame(s);
                let d = walk(s, e, 30.0, r);
                Some(Spot { dir: d, facing: (s - d * s.dot(d)).normalized() })
            }
            "forest_edge" | "coast" => {
                let mut rng = 0x2545F4914F6CDD1Du64 ^ (self.recipe.seed as u64);
                let mut next = || {
                    rng ^= rng << 13;
                    rng ^= rng >> 7;
                    rng ^= rng << 17;
                    (rng >> 11) as f64 / (1u64 << 53) as f64
                };
                for _ in 0..20_000 {
                    let z = next() * 2.0 - 1.0;
                    let phi = next() * std::f64::consts::TAU;
                    let rr = (1.0 - z * z).sqrt();
                    let d = v3(rr * phi.cos(), z, rr * phi.sin());
                    let (e, _) = tangent_frame(d);
                    let s = self.sample(d);
                    if s.height_above_sea < 2.0 || s.slope_deg > 15.0 {
                        continue;
                    }
                    let ahead = walk(d, e, 40.0, r);
                    if kind == "coast" {
                        if s.height_above_sea < 6.0 && self.height_at(ahead) - self.sea < -1.0 {
                            return Some(Spot { dir: d, facing: e });
                        }
                    } else {
                        let here = self.scatter_density_at(d, s.biome as u8, s.height_above_sea);
                        let sa = self.sample(ahead);
                        let there = self.scatter_density_at(ahead, sa.biome as u8, sa.height_above_sea);
                        if here == 0.0 && there > 0.5 && sa.height_above_sea > 0.0 {
                            return Some(Spot { dir: d, facing: e });
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }
}

/// A 5 x 5 pixel dot at a direction.
pub fn mark(img: &mut [u8], w: usize, h: usize, d: V3, c: [u8; 3]) {
    let lon = d.z.atan2(d.x);
    let lat = d.y.clamp(-1.0, 1.0).asin();
    let x = ((lon + std::f64::consts::PI) / std::f64::consts::TAU * w as f64) as isize;
    let y = ((std::f64::consts::FRAC_PI_2 - lat) / std::f64::consts::PI * h as f64) as isize;
    for dy in -2..=2 {
        for dx in -2..=2 {
            let xx = (x + dx).rem_euclid(w as isize) as usize;
            let yy = (y + dy).clamp(0, h as isize - 1) as usize;
            let k = (yy * w + xx) * 3;
            img[k..k + 3].copy_from_slice(&c);
        }
    }
}
