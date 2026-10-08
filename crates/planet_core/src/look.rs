//! The look harness's planet side (#63): an equirectangular atlas per layer (height, biome,
//! landform, scatter density) and named spots for fixed viewpoints. No window, no GPU.
use crate::math::*;
use crate::landform::ShapeRt;
use crate::planet::*;
use serde::Deserialize;

/// A fixed viewpoint (`content/look/viewpoints.json`). `spot` names a place from
/// `Planet::spot`, or `orbit`: then the camera is `height_m` from the centre along `from`,
/// looking at the centre. On the ground the camera stands `back_m` behind the spot (against its
/// facing), `height_m` above the ground there, turned by `turn_deg` and pitched by `pitch_deg`
/// (in orbit `pitch_deg` turns the view up from the centre towards the horizon).
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
        let known = ["orbit", "basin", "rim", "plateau", "crater", "canyon", "mesa", "spire", "caldera", "signature", "site", "forest_edge", "coast"];
        for p in &v.viewpoints {
            if !known.contains(&p.spot.as_str()) && !p.spot.starts_with("site:") && p.spot != "landmark" {
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

struct Px {
    ha: f64,
    biome: u8,
    land: f64,
    scatter: f32,
}

impl Planet {
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
                    out.push(Px { ha, biome, land: lf.unwrap_or(f.land), scatter: self.scatter_density_at(d, biome, ha, None) });
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
                let lc = lerp3([0.2, 0.35, 0.75], [0.95, 0.55, 0.25], ((p.land + 1.0) * 0.5) as f32);
                land.extend(to_u8([lc[0] * shade, lc[1] * shade, lc[2] * shade]));
                let s = if p.ha <= 0.0 { [0.05, 0.08, 0.2] } else { lerp3([0.12, 0.10, 0.08], [0.3, 1.0, 0.35], p.scatter) };
                scatter.extend(to_u8(s));
            }
        }
        // Sites by kind on the biome layer (landmarks larger), black outline first.
        const KIND: [[u8; 3]; 6] = [[255, 255, 255], [255, 80, 200], [80, 230, 255], [255, 200, 40], [140, 255, 120], [255, 120, 60]];
        for s in &self.sites {
            let big = s.category == crate::recipe::SiteCategory::Landmark;
            mark_n(&mut biome, w, h, s.dir, [0, 0, 0], if big { 5 } else { 3 });
            mark_n(&mut biome, w, h, s.dir, KIND[s.kind % KIND.len()], if big { 4 } else { 2 });
        }
        // Landforms on the height layer: red, the signature yellow.
        for s in &self.stamps {
            mark(&mut height, w, h, s.c, if s.signature { [255, 230, 0] } else { [230, 30, 30] });
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
            "basin" | "rim" | "plateau" | "crater" | "canyon" | "mesa" | "spire" | "caldera" | "signature" => {
                // Stand at `m` metres from the stamp centre, facing it.
                let towards = |c: V3, m: f64| {
                    let (e, _) = tangent_frame(c);
                    let d = walk(c, e, m, r);
                    Spot { dir: d, facing: (c - d * c.dot(d)).normalized() }
                };
                for s in &self.stamps {
                    let c = s.c;
                    match (kind, &s.shape) {
                        ("basin", ShapeRt::Basin { r: br, .. }) => {
                            let (e, _) = tangent_frame(c);
                            // From the centre outwards until the ground is 3 m above the sea.
                            let mut m = 0.0;
                            while m < br * 1.5 {
                                let d = walk(c, e, m, r);
                                if self.height_at(d) - self.sea > 3.0 {
                                    return Some(Spot { dir: d, facing: (c - d * c.dot(d)).normalized() });
                                }
                                m += 10.0;
                            }
                            return Some(towards(c, br * 0.6));
                        }
                        ("rim", ShapeRt::Esc { n, sw, .. }) => {
                            let d = walk(c, *n, sw * 0.5 + 15.0, r);
                            let down = -(*n - d * n.dot(d)).normalized();
                            return Some(Spot { dir: d, facing: down });
                        }
                        ("plateau", ShapeRt::Plateau { r: pr, fall, .. }) => {
                            let (e, _) = tangent_frame(c);
                            let d = walk(c, e, (pr - fall - 20.0).max(0.0), r);
                            return Some(Spot { dir: d, facing: (e - d * e.dot(d)).normalized() });
                        }
                        ("crater", ShapeRt::Crater { r: cr, .. }) => return Some(towards(c, *cr)),
                        // On the floor in the middle, looking along the cut.
                        ("canyon", ShapeRt::Canyon { t, .. }) => return Some(Spot { dir: c, facing: *t }),
                        ("mesa", ShapeRt::Mesa { .. }) => return Some(towards(c, s.reach_m + 150.0)),
                        ("spire", ShapeRt::Spire { r: sr, .. }) if !s.signature => return Some(towards(c, sr * 3.0 + 150.0)),
                        ("caldera", ShapeRt::Caldera { rr, w, .. }) => return Some(towards(c, rr + w * 1.5)),
                        ("signature", _) if s.signature => return Some(towards(c, s.reach_m + 400.0)),
                        _ => {}
                    }
                }
                None
            }
            // `site`: the first site proper; `site:<kind>`: the first of a kind; `landmark`: the
            // first landmark. Standing outside the footprint, facing the centre.
            _ if kind == "site" || kind == "landmark" || kind.starts_with("site:") => {
                use crate::recipe::SiteCategory;
                let s = self.sites.iter().find(|s| match kind {
                    "site" => s.category == SiteCategory::Site,
                    "landmark" => s.category == SiteCategory::Landmark,
                    k => s.id == k["site:".len()..],
                })?;
                let (e, _) = tangent_frame(s.dir);
                let d = walk(s.dir, e, s.footprint_m + if s.category == SiteCategory::Landmark { 120.0 } else { 12.0 }, r);
                Some(Spot { dir: d, facing: (s.dir - d * s.dir.dot(d)).normalized() })
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
                        let here = self.scatter_density_at(d, s.biome as u8, s.height_above_sea, Some("tree"));
                        let sa = self.sample(ahead);
                        let there = self.scatter_density_at(ahead, sa.biome as u8, sa.height_above_sea, Some("tree"));
                        if here < 0.1 && there > 0.5 && sa.height_above_sea > 0.0 {
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
    mark_n(img, w, h, d, c, 2)
}

/// A (2n+1) square dot at a direction.
pub fn mark_n(img: &mut [u8], w: usize, h: usize, d: V3, c: [u8; 3], n: isize) {
    let lon = d.z.atan2(d.x);
    let lat = d.y.clamp(-1.0, 1.0).asin();
    let x = ((lon + std::f64::consts::PI) / std::f64::consts::TAU * w as f64) as isize;
    let y = ((std::f64::consts::FRAC_PI_2 - lat) / std::f64::consts::PI * h as f64) as isize;
    for dy in -n..=n {
        for dx in -n..=n {
            let xx = (x + dx).rem_euclid(w as isize) as usize;
            let yy = (y + dy).clamp(0, h as isize - 1) as usize;
            let k = (yy * w + xx) * 3;
            img[k..k + 3].copy_from_slice(&c);
        }
    }
}
