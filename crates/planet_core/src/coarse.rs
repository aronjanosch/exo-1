//! The coarse global layer (#177, spike 14): what only a whole-planet pass can give. The sea level,
//! the drainage's change of the ground and the water surface per vertex of the cube-sphere grid
//! (heights as i16 / u16 in tenths of a metre), and the rivers and lakes. It is baked once per
//! client from recipe, seed and radius, and cached on disk under a hash of those and the code
//! version. The fine detail (noise, stamps, bands) never lives here: it is a function of the
//! position, evaluated per chunk.
use crate::drainage::Mouth;
use crate::math::*;
use crate::planet::{Lake, River};
use std::path::{Path, PathBuf};

/// Bump when the bake's output changes for the same recipe, seed and radius: an old cache is then
/// ignored and baked again.
pub const CODE_VERSION: u32 = 1;

/// Unit of the stored heights (m).
pub const UNIT_M: f32 = 0.1;
/// Stored water level 0 = none; otherwise (level + WATER_OFFSET_M) / UNIT_M.
const WATER_OFFSET_M: f32 = 1000.0;
pub const NO_WATER_RAW: u16 = 0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Coarse {
    /// Cells per face edge; the grid has (n + 1)² vertices per face.
    pub n: usize,
    pub sea: f64,
    /// Change of the ground per vertex (`(face * w + j) * w + i`), tenths of a metre.
    pub carve: Vec<i16>,
    /// Water surface per vertex, `NO_WATER_RAW` where none is defined.
    pub water: Vec<u16>,
    pub rivers: Vec<River>,
    pub lakes: Vec<Lake>,
}

pub fn quantize_carve(m: f32) -> i16 {
    (m / UNIT_M).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

pub fn quantize_water(level_m: f32) -> u16 {
    if level_m <= crate::drainage::DRY_BELOW as f32 {
        return NO_WATER_RAW;
    }
    ((level_m + WATER_OFFSET_M) / UNIT_M).round().clamp(1.0, u16::MAX as f32) as u16
}

/// The stored water level as the f32 the drainage used (`NO_WATER` where none).
#[inline(always)]
pub fn dequantize_water(raw: u16) -> f32 {
    if raw == NO_WATER_RAW { crate::drainage::NO_WATER } else { raw as f32 * UNIT_M - WATER_OFFSET_M }
}

/// FNV-1a, 64 bit.
pub fn fnv(bytes: &[u8], mut h: u64) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
pub const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

/// Cache key of a planet's coarse layer: recipe text, seed, radius, grid size and code version.
pub fn cache_key(recipe_hash: u64, seed: i32, radius: f64, n: usize) -> u64 {
    let mut h = recipe_hash;
    for part in [&seed.to_le_bytes()[..], &radius.to_bits().to_le_bytes(), &(n as u64).to_le_bytes(), &CODE_VERSION.to_le_bytes()] {
        h = fnv(part, h);
    }
    h
}

pub fn cache_path(dir: &Path, key: u64) -> PathBuf {
    dir.join(format!("coarse-{key:016x}.bin"))
}

const MAGIC: &[u8; 4] = b"EXOC";

struct W(Vec<u8>);
impl W {
    fn u32(&mut self, v: u32) {
        self.0.extend(v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend(v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.0.extend(v.to_le_bytes());
    }
    fn v3(&mut self, v: V3) {
        self.f64(v.x);
        self.f64(v.y);
        self.f64(v.z);
    }
}

struct R<'a>(&'a [u8], usize);
impl R<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.0.get(self.1..self.1 + n).ok_or("cache: truncated")?;
        self.1 += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn v3(&mut self) -> Result<V3, String> {
        Ok(v3(self.f64()?, self.f64()?, self.f64()?))
    }
}

fn put_mouth(w: &mut W, m: Mouth) {
    match m {
        Mouth::River(i) => {
            w.u32(0);
            w.u32(i);
        }
        Mouth::Lake(i) => {
            w.u32(1);
            w.u32(i);
        }
        Mouth::Sea => {
            w.u32(2);
            w.u32(0);
        }
    }
}

fn get_mouth(r: &mut R) -> Result<Mouth, String> {
    let (k, i) = (r.u32()?, r.u32()?);
    match k {
        0 => Ok(Mouth::River(i)),
        1 => Ok(Mouth::Lake(i)),
        2 => Ok(Mouth::Sea),
        _ => Err("cache: bad river mouth".into()),
    }
}

impl Coarse {
    /// Bytes of the layer in memory (the arrays and the lists).
    pub fn bytes(&self) -> usize {
        self.carve.len() * 2 + self.water.len() * 2 + self.rivers.len() * std::mem::size_of::<River>() + self.lakes.len() * std::mem::size_of::<Lake>()
    }

    /// The layer as bytes: header, payload, and a checksum of both.
    pub fn encode(&self, key: u64) -> Vec<u8> {
        let mut w = W(Vec::with_capacity(self.bytes() + 64));
        w.0.extend(MAGIC);
        w.u32(CODE_VERSION);
        w.u64(key);
        w.u64(self.n as u64);
        w.f64(self.sea);
        w.u64(self.carve.len() as u64);
        for &c in &self.carve {
            w.0.extend(c.to_le_bytes());
        }
        w.u64(self.water.len() as u64);
        for &c in &self.water {
            w.0.extend(c.to_le_bytes());
        }
        w.u64(self.rivers.len() as u64);
        for r in &self.rivers {
            w.v3(r.dir);
            w.v3(r.to);
            w.f64(r.catchment_km2);
            w.f64(r.bed_m);
            w.f64(r.level_m);
            w.f64(r.depth_m);
            w.f64(r.half_width_m);
            put_mouth(&mut w, r.next);
        }
        w.u64(self.lakes.len() as u64);
        for l in &self.lakes {
            w.f64(l.level_m);
            w.f64(l.area_m2);
            w.f64(l.depth_m);
            w.v3(l.deepest);
            match l.outlet {
                Some(o) => {
                    w.u32(1);
                    w.v3(o);
                }
                None => {
                    w.u32(0);
                    w.v3(V3::default());
                }
            }
        }
        let sum = fnv(&w.0, FNV_START);
        w.u64(sum);
        w.0
    }

    /// A hash of the whole layer: equal layers hash equal, whatever made them.
    pub fn hash(&self) -> u64 {
        fnv(&self.encode(0), FNV_START)
    }

    pub fn decode(bytes: &[u8], key: u64) -> Result<Coarse, String> {
        if bytes.len() < 8 {
            return Err("cache: too short".into());
        }
        let (body, sum) = bytes.split_at(bytes.len() - 8);
        if fnv(body, FNV_START) != u64::from_le_bytes(sum.try_into().unwrap()) {
            return Err("cache: checksum".into());
        }
        let mut r = R(body, 0);
        if r.take(4)? != MAGIC {
            return Err("cache: not a coarse layer".into());
        }
        if r.u32()? != CODE_VERSION {
            return Err("cache: other code version".into());
        }
        if r.u64()? != key {
            return Err("cache: other recipe, seed or radius".into());
        }
        let n = r.u64()? as usize;
        let sea = r.f64()?;
        let len = r.u64()? as usize;
        let carve = r.take(len * 2)?.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        let len = r.u64()? as usize;
        let water = r.take(len * 2)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let len = r.u64()? as usize;
        let mut rivers = Vec::with_capacity(len);
        for _ in 0..len {
            rivers.push(River {
                dir: r.v3()?,
                to: r.v3()?,
                catchment_km2: r.f64()?,
                bed_m: r.f64()?,
                level_m: r.f64()?,
                depth_m: r.f64()?,
                half_width_m: r.f64()?,
                next: get_mouth(&mut r)?,
            });
        }
        let len = r.u64()? as usize;
        let mut lakes = Vec::with_capacity(len);
        for _ in 0..len {
            let (level_m, area_m2, depth_m, deepest) = (r.f64()?, r.f64()?, r.f64()?, r.v3()?);
            let (has, o) = (r.u32()?, r.v3()?);
            lakes.push(Lake { level_m, area_m2, depth_m, deepest, outlet: (has == 1).then_some(o) });
        }
        Ok(Coarse { n, sea, carve, water, rivers, lakes })
    }

    /// Loads the layer cached under `key`, or None when there is none or it does not fit.
    pub fn load(dir: &Path, key: u64) -> Option<Coarse> {
        let bytes = std::fs::read(cache_path(dir, key)).ok()?;
        Coarse::decode(&bytes, key).ok()
    }

    /// Writes the layer to the cache (a temporary file first, so a half-written one never loads).
    pub fn save(&self, dir: &Path, key: u64) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = cache_path(dir, key);
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, self.encode(key))?;
        std::fs::rename(tmp, path)
    }
}
