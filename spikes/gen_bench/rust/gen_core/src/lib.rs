//! Spike 6 workload (see spikes/gen_bench/SPEC.md). f32, FastNoiseLite crate.
use fastnoise_lite::{FastNoiseLite, FractalType, NoiseType};
use std::ops::{Add, Mul, Neg, Sub};

pub const R: f32 = 6000.0;
pub const SEED: i32 = 1337;
pub const GRID: usize = 32;
pub const M: usize = GRID + 3;
pub const MACRO_N: usize = 512;
pub const DEPTH_CHUNKS: usize = 128;
pub const CHUNK_COUNT: usize = 200;

#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
#[inline(always)]
pub fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}
impl Add for V3 {
    type Output = V3;
    #[inline(always)]
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = V3;
    #[inline(always)]
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for V3 {
    type Output = V3;
    #[inline(always)]
    fn mul(self, s: f32) -> V3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Neg for V3 {
    type Output = V3;
    #[inline(always)]
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}
impl V3 {
    #[inline(always)]
    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    #[inline(always)]
    pub fn cross(self, o: V3) -> V3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    #[inline(always)]
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
    #[inline(always)]
    pub fn normalized(self) -> V3 {
        let l = self.length();
        if l == 0.0 {
            self
        } else {
            self * (1.0 / l)
        }
    }
}

const FACE_NORMALS: [V3; 6] = [
    V3 { x: 1.0, y: 0.0, z: 0.0 },
    V3 { x: -1.0, y: 0.0, z: 0.0 },
    V3 { x: 0.0, y: 1.0, z: 0.0 },
    V3 { x: 0.0, y: -1.0, z: 0.0 },
    V3 { x: 0.0, y: 0.0, z: 1.0 },
    V3 { x: 0.0, y: 0.0, z: -1.0 },
];

#[inline]
pub fn cube_to_sphere(face: usize, a: f32, b: f32) -> V3 {
    let nrm = FACE_NORMALS[face];
    let u = v3(nrm.y, nrm.z, nrm.x);
    let v = nrm.cross(u);
    let p = nrm + u * a + v * b;
    let (x2, y2, z2) = (p.x * p.x, p.y * p.y, p.z * p.z);
    v3(
        p.x * (1.0 - y2 * 0.5 - z2 * 0.5 + y2 * z2 / 3.0).sqrt(),
        p.y * (1.0 - z2 * 0.5 - x2 * 0.5 + z2 * x2 / 3.0).sqrt(),
        p.z * (1.0 - x2 * 0.5 - y2 * 0.5 + x2 * y2 / 3.0).sqrt(),
    )
}

// ---- noise table -------------------------------------------------------

#[derive(Copy, Clone)]
pub enum Fr {
    None,
    Fbm,
    Ridged,
}
pub struct NoiseCfg {
    pub name: &'static str,
    pub seed_off: i32,
    pub freq: f32,
    pub fractal: Fr,
    pub octaves: i32,
}
const fn nc(name: &'static str, seed_off: i32, freq: f32, fractal: Fr, octaves: i32) -> NoiseCfg {
    NoiseCfg { name, seed_off, freq, fractal, octaves }
}
pub const NOISE_CFGS: [NoiseCfg; 10] = [
    nc("region", 1, 0.00033, Fr::Fbm, 3),
    nc("face", 2, 0.0011, Fr::Ridged, 4),
    nc("foot", 3, 0.0066, Fr::Fbm, 4),
    nc("warp", 4, 0.0025, Fr::None, 1),
    nc("warped", 5, 0.002, Fr::Fbm, 2),
    nc("m_elev", 10, 0.0002, Fr::Fbm, 3),
    nc("m_moist", 11, 0.0003, Fr::Fbm, 2),
    nc("m_temp", 12, 0.0003, Fr::Fbm, 2),
    nc("m_land", 13, 0.0004, Fr::None, 1),
    nc("forest", 20, 0.005, Fr::Fbm, 2),
];

pub fn make_noise(c: &NoiseCfg, seed: i32) -> FastNoiseLite {
    let mut n = FastNoiseLite::with_seed(seed + c.seed_off);
    n.set_noise_type(Some(NoiseType::OpenSimplex2S));
    n.set_frequency(Some(c.freq));
    n.set_fractal_lacunarity(Some(2.0));
    n.set_fractal_gain(Some(0.5));
    n.set_fractal_octaves(Some(c.octaves));
    n.set_fractal_type(Some(match c.fractal {
        Fr::None => FractalType::None,
        Fr::Fbm => FractalType::FBm,
        Fr::Ridged => FractalType::Ridged,
    }));
    n
}

pub struct Noises {
    pub region: FastNoiseLite,
    pub face: FastNoiseLite,
    pub foot: FastNoiseLite,
    pub warp: FastNoiseLite,
    pub warped: FastNoiseLite,
    pub m_elev: FastNoiseLite,
    pub m_moist: FastNoiseLite,
    pub m_temp: FastNoiseLite,
    pub m_land: FastNoiseLite,
    pub forest: FastNoiseLite,
}
impl Noises {
    pub fn new(seed: i32) -> Self {
        let m = |i: usize| make_noise(&NOISE_CFGS[i], seed);
        Noises {
            region: m(0),
            face: m(1),
            foot: m(2),
            warp: m(3),
            warped: m(4),
            m_elev: m(5),
            m_moist: m(6),
            m_temp: m(7),
            m_land: m(8),
            forest: m(9),
        }
    }
    /// All ten noises in table order at one point (for the parity probe).
    pub fn all_at(&self, p: V3) -> [f32; 10] {
        let f = |n: &FastNoiseLite| n.get_noise_3d(p.x, p.y, p.z);
        [
            f(&self.region),
            f(&self.face),
            f(&self.foot),
            f(&self.warp),
            f(&self.warped),
            f(&self.m_elev),
            f(&self.m_moist),
            f(&self.m_temp),
            f(&self.m_land),
            f(&self.forest),
        ]
    }
}
#[inline(always)]
fn nz(n: &FastNoiseLite, p: V3) -> f32 {
    n.get_noise_3d(p.x, p.y, p.z)
}

// ---- macro bake --------------------------------------------------------

pub type MacroData = Vec<f32>; // ((f*512+j)*512+i)*4+c

fn bake_row(nz_: &Noises, face: usize, j: usize, out: &mut [f32]) {
    let n = MACRO_N as f32;
    let b = -1.0 + (j as f32 + 0.5) * 2.0 / n;
    for i in 0..MACRO_N {
        let a = -1.0 + (i as f32 + 0.5) * 2.0 / n;
        let dir = cube_to_sphere(face, a, b);
        let p = dir * R;
        let elev = nz(&nz_.m_elev, p);
        let moist = nz(&nz_.m_moist, p);
        let temp = 1.0 - dir.y.abs() * 1.2 + 0.3 * nz(&nz_.m_temp, p) - 0.2 * elev.max(0.0);
        let land = ((nz(&nz_.m_land, p) + 1.0) * 2.0).floor().clamp(0.0, 3.0);
        let o = &mut out[i * 4..i * 4 + 4];
        o[0] = elev;
        o[1] = temp;
        o[2] = moist;
        o[3] = land;
    }
}

pub fn bake_single(noises: &Noises) -> MacroData {
    bake_threads(noises, 1)
}

/// Parallel over rows (contiguous row ranges per thread). threads <= 1 runs inline.
pub fn bake_threads(noises: &Noises, threads: usize) -> MacroData {
    let row_len = MACRO_N * 4;
    let rows = 6 * MACRO_N;
    let mut data = vec![0.0f32; rows * row_len];
    let threads = threads.max(1);
    if threads == 1 {
        for (r, out) in data.chunks_mut(row_len).enumerate() {
            bake_row(noises, r / MACRO_N, r % MACRO_N, out);
        }
        return data;
    }
    let per = rows.div_ceil(threads);
    std::thread::scope(|s| {
        for (t, slab) in data.chunks_mut(per * row_len).enumerate() {
            s.spawn(move || {
                for (k, out) in slab.chunks_mut(row_len).enumerate() {
                    let r = t * per + k;
                    bake_row(noises, r / MACRO_N, r % MACRO_N, out);
                }
            });
        }
    });
    data
}

#[inline]
fn macro_lookup(m: &[f32], face: usize, a: f32, b: f32) -> (f32, f32, f32, i32) {
    let top = (MACRO_N - 1) as f32;
    let nn = MACRO_N as f32;
    let u = (((a + 1.0) * 0.5 * nn) - 0.5).clamp(0.0, top);
    let v = (((b + 1.0) * 0.5 * nn) - 0.5).clamp(0.0, top);
    let i0 = (u.floor() as usize).min(MACRO_N - 1);
    let j0 = (v.floor() as usize).min(MACRO_N - 1);
    let i1 = (i0 + 1).min(MACRO_N - 1);
    let j1 = (j0 + 1).min(MACRO_N - 1);
    let fu = u - i0 as f32;
    let fv = v - j0 as f32;
    let idx = |i: usize, j: usize| ((face * MACRO_N + j) * MACRO_N + i) * 4;
    let (k00, k10, k01, k11) = (idx(i0, j0), idx(i1, j0), idx(i0, j1), idx(i1, j1));
    let bil = |c: usize| {
        let x0 = m[k00 + c] + (m[k10 + c] - m[k00 + c]) * fu;
        let x1 = m[k01 + c] + (m[k11 + c] - m[k01 + c]) * fu;
        x0 + (x1 - x0) * fv
    };
    let ni = ((u + 0.5) as usize).min(MACRO_N - 1);
    let nj = ((v + 0.5) as usize).min(MACRO_N - 1);
    let land = m[idx(ni, nj) + 3] as i32;
    (bil(0), bil(1), bil(2), land)
}

// ---- hash --------------------------------------------------------------

#[inline(always)]
pub fn lowbias32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    x
}
#[inline(always)]
pub fn hash(key: u32, kind: u32, ci: u32, cj: u32, salt: u32) -> u32 {
    let inner = kind
        .wrapping_mul(0x85EBCA6B)
        .wrapping_add(ci.wrapping_mul(0xC2B2AE35))
        .wrapping_add(cj.wrapping_mul(0x27D4EB2F))
        .wrapping_add(salt.wrapping_mul(0x165667B1));
    lowbias32(key.wrapping_mul(0x9E3779B1) ^ lowbias32(inner))
}
#[inline(always)]
pub fn hash01(key: u32, kind: u32, ci: u32, cj: u32, salt: u32) -> f32 {
    (hash(key, kind, ci, cj, salt) >> 8) as f32 / 16777216.0
}

// ---- chunks ------------------------------------------------------------

pub fn chunk_list() -> Vec<(usize, usize, usize)> {
    (0..CHUNK_COUNT)
        .map(|k| (k % 6, (k * 37 + 11) % 128, (k * 53 + 29) % 128))
        .collect()
}

#[derive(Default)]
pub struct ChunkOut {
    pub verts: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub canopy: Vec<f32>,
    pub rocks: Vec<f32>,
    pub height_sum: f64,
    pub biomes: [i32; 4],
}

const BIOME_COLORS: [[f32; 4]; 4] = [
    [0.20, 0.55, 0.20, 1.0],
    [0.75, 0.65, 0.40, 1.0],
    [0.45, 0.40, 0.38, 1.0],
    [0.90, 0.92, 0.95, 1.0],
];

pub struct Gen {
    pub noises: Noises,
    pub macro_: MacroData,
}

impl Gen {
    pub fn new(seed: i32) -> Self {
        Gen { noises: Noises::new(seed), macro_: Vec::new() }
    }
    pub fn bake(&mut self, threads: usize) {
        self.macro_ = bake_threads(&self.noises, threads);
    }

    /// Thread-safe (&self) after bake().
    pub fn build_chunk(&self, face: usize, ix: usize, iy: usize) -> ChunkOut {
        let n = &self.noises;
        let size = 2.0f32 / DEPTH_CHUNKS as f32;
        let a0 = -1.0 + ix as f32 * size;
        let b0 = -1.0 + iy as f32 * size;
        let step = size / GRID as f32;
        let centre_dir = cube_to_sphere(face, a0 + size * 0.5, b0 + size * 0.5);
        let centre = centre_dir * R;
        let edge_m = (cube_to_sphere(face, a0, b0) - cube_to_sphere(face, a0 + size, b0)).length() * R;
        let skirt_depth = (edge_m / GRID as f32 * 4.0).max(2.0);

        let mut pos = [V3::default(); M * M];
        let mut dirs = [V3::default(); M * M];
        let mut hs = [0.0f32; M * M];
        let mut rows = [0u8; M * M];
        let mut out = ChunkOut::default();

        for j in 0..M {
            for i in 0..M {
                let a = a0 + (i as f32 - 1.0) * step;
                let b = b0 + (j as f32 - 1.0) * step;
                let dir = cube_to_sphere(face, a, b);
                let p = dir * R;
                let (elev, temp, moist, land) =
                    macro_lookup(&self.macro_, face, a.clamp(-1.0, 1.0), b.clamp(-1.0, 1.0));
                let w = 150.0;
                let pw = v3(
                    p.x + w * nz(&n.warp, p + v3(1013.0, 0.0, 0.0)),
                    p.y + w * nz(&n.warp, p + v3(0.0, 2027.0, 0.0)),
                    p.z + w * nz(&n.warp, p + v3(0.0, 0.0, 3041.0)),
                );
                let h = elev * 80.0
                    + nz(&n.region, p) * 100.0
                    + nz(&n.face, p) * 60.0
                    + nz(&n.foot, p) * 8.0
                    + nz(&n.warped, pw) * 25.0;
                let row = if h > 120.0 && temp < 0.45 {
                    3
                } else if land == 3 {
                    2
                } else if moist > 0.0 {
                    0
                } else {
                    1
                };
                let k = j * M + i;
                pos[k] = dir * (R + h);
                dirs[k] = dir;
                hs[k] = h;
                rows[k] = row;
            }
        }

        out.verts.reserve_exact(M * M);
        out.normals.reserve_exact(M * M);
        out.colors.reserve_exact(M * M);
        out.uvs.reserve_exact(M * M);
        let mut nrms = [V3::default(); M * M];
        for j in 0..M {
            for i in 0..M {
                let k = j * M + i;
                let ck = j.clamp(1, M - 2) * M + i.clamp(1, M - 2);
                let mut nrm = (pos[ck + 1] - pos[ck - 1]).cross(pos[ck + M] - pos[ck - M]).normalized();
                if nrm.dot(dirs[ck]) < 0.0 {
                    nrm = -nrm;
                }
                nrms[k] = nrm;
                out.normals.push([nrm.x, nrm.y, nrm.z]);
                let (vv, uv) = if k == ck {
                    (pos[k] - centre, [0.0, 0.0])
                } else {
                    (pos[ck] - dirs[ck] * skirt_depth - centre, [1.0, 0.0])
                };
                out.verts.push([vv.x, vv.y, vv.z]);
                out.uvs.push(uv);
                out.colors.push(BIOME_COLORS[rows[k] as usize]);
            }
        }
        for j in 1..=GRID + 1 {
            for i in 1..=GRID + 1 {
                let k = j * M + i;
                out.height_sum += hs[k] as f64;
                out.biomes[rows[k] as usize] += 1;
            }
        }

        // scatter
        let key = (face * 16384 + ix * 128 + iy) as u32;
        let site = hash(key, 2, 0, 0, 0) & 3 == 0;
        let site_clear = |dir: V3| !(site && (dir - centre_dir).length() * R < 12.0);
        for kind in 0..2u32 {
            let cells: u32 = if kind == 0 { 7 } else { 11 };
            for ci in 0..cells {
                for cj in 0..cells {
                    let s = (ci as f32 + hash01(key, kind, ci, cj, 0)) / cells as f32;
                    let t = (cj as f32 + hash01(key, kind, ci, cj, 1)) / cells as f32;
                    let gx = s * GRID as f32;
                    let gy = t * GRID as f32;
                    let gi = (gx.floor() as usize).min(GRID - 1);
                    let gj = (gy.floor() as usize).min(GRID - 1);
                    let fx = gx - gi as f32;
                    let fy = gy - gj as f32;
                    let k00 = (gj + 1) * M + gi + 1;
                    let (k10, k01, k11) = (k00 + 1, k00 + M, k00 + M + 1);
                    let lerp2 = |a: f32, b: f32, c: f32, d: f32| {
                        let x0 = a + (b - a) * fx;
                        let x1 = c + (d - c) * fx;
                        x0 + (x1 - x0) * fy
                    };
                    let h = lerp2(hs[k00], hs[k10], hs[k01], hs[k11]);
                    let nv = |f: fn(&V3) -> f32| lerp2(f(&nrms[k00]), f(&nrms[k10]), f(&nrms[k01]), f(&nrms[k11]));
                    let nrm = v3(nv(|v| v.x), nv(|v| v.y), nv(|v| v.z)).normalized();
                    let dir = cube_to_sphere(face, a0 + s * size, b0 + t * size);
                    let nd = nrm.dot(dir);
                    let (ok, scale_base, scale_mul) = if kind == 0 {
                        let ok = nd >= 0.8192
                            && (-20.0..=140.0).contains(&h)
                            && nz(&n.forest, dir * R) > 0.1
                            && site_clear(dir);
                        (ok, 0.8, 0.6)
                    } else {
                        let ok = nd >= 0.7071
                            && (-50.0..=300.0).contains(&h)
                            && hash01(key, 1, ci, cj, 2) < 0.5
                            && site_clear(dir);
                        (ok, 0.5, 1.0)
                    };
                    if !ok {
                        continue;
                    }
                    let up = dir;
                    let rf = if up.y.abs() < 0.99 { v3(0.0, 1.0, 0.0) } else { v3(1.0, 0.0, 0.0) };
                    let tg = up.cross(rf).normalized();
                    let bt = up.cross(tg);
                    let yaw = hash01(key, kind, ci, cj, 3) * std::f32::consts::TAU;
                    let x = tg * yaw.cos() + bt * yaw.sin();
                    let z = x.cross(up);
                    let sc = scale_base + scale_mul * hash01(key, kind, ci, cj, 4);
                    let o = dir * (R + h) - centre;
                    let (x, u, z) = (x * sc, up * sc, z * sc);
                    let dst = if kind == 0 { &mut out.canopy } else { &mut out.rocks };
                    dst.extend_from_slice(&[x.x, x.y, x.z, u.x, u.y, u.z, z.x, z.y, z.z, o.x, o.y, o.z]);
                }
            }
        }
        out
    }
}
