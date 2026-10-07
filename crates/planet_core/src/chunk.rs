//! Chunk build: crust mesh arrays plus scatter transforms. `&self` only, thread-safe after bake.
use crate::math::*;
use crate::planet::*;

pub const GRID: usize = 32;
pub const M: usize = GRID + 3;

#[derive(Default)]
pub struct ScatterOut {
    pub kind: String,
    /// MultiMesh buffer layout, 16 floats per instance: the 3x4 transform row-major
    /// (relative to the chunk centre), then an RGBA tint.
    pub buffer: Vec<f32>,
}

#[derive(Default)]
pub struct ChunkOut {
    pub center: [f64; 3],
    pub verts: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    /// Height above the base radius per vertex (M*M, skirt ring included), metres.
    pub heights: Vec<f32>,
    pub biomes: Vec<u8>,
    pub min_h: f32,
    pub max_h: f32,
    pub scatter: Vec<ScatterOut>,
}

impl Planet {
    /// Chunk edge length in metres (along an edge of the chunk, base sphere).
    pub fn chunk_edge_m(&self, face: usize, a0: f64, b0: f64, size: f64) -> f64 {
        (cube_to_sphere(face, a0, b0) - cube_to_sphere(face, a0 + size, b0)).length() * self.radius
    }

    pub fn build_chunk(&self, face: usize, a0: f64, b0: f64, size: f64, with_scatter: bool) -> ChunkOut {
        let r = self.radius;
        let step = size / GRID as f64;
        let centre_dir = cube_to_sphere(face, a0 + size * 0.5, b0 + size * 0.5);
        // exactly representable in f32, so the mesh node position in Godot is exact
        let c64 = centre_dir * r;
        let centre = v3(c64.x as f32 as f64, c64.y as f32 as f64, c64.z as f32 as f64);
        let edge_m = self.chunk_edge_m(face, a0, b0, size);
        let skirt_depth = (edge_m / GRID as f64 * 4.0).max(2.0);

        // vertex colour alpha carries the LOD depth (debug colours in the shader)
        let lod_depth = (2.0 / size).log2().round() as f32;
        let mut pos = vec![V3::default(); M * M];
        let mut dirs = vec![V3::default(); M * M];
        let mut hs = vec![0.0f64; M * M];
        let mut rows = vec![0u8; M * M];
        for j in 0..M {
            for i in 0..M {
                let a = a0 + (i as f64 - 1.0) * step;
                let b = b0 + (j as f64 - 1.0) * step;
                let dir = cube_to_sphere(face, a, b);
                let (h, f) = self.height_ab(face, a.clamp(-1.0, 1.0), b.clamp(-1.0, 1.0), dir);
                let lf = self.stamp_height(dir).1;
                let k = j * M + i;
                pos[k] = dir * (r + h);
                dirs[k] = dir;
                hs[k] = h;
                rows[k] = self.biome_for(h - self.sea, &f, lf);
            }
        }

        let mut out = ChunkOut { center: centre.arr(), min_h: f32::MAX, max_h: f32::MIN, ..Default::default() };
        out.verts.reserve_exact(M * M);
        out.normals.reserve_exact(M * M);
        out.colors.reserve_exact(M * M);
        out.uvs.reserve_exact(M * M);
        let mut nrms = vec![V3::default(); M * M];
        for j in 0..M {
            for i in 0..M {
                let k = j * M + i;
                let ck = j.clamp(1, M - 2) * M + i.clamp(1, M - 2);
                let mut nrm = (pos[ck + 1] - pos[ck - 1]).cross(pos[ck + M] - pos[ck - M]).normalized();
                if nrm.dot(dirs[ck]) < 0.0 {
                    nrm = -nrm;
                }
                nrms[k] = nrm;
                out.normals.push([nrm.x as f32, nrm.y as f32, nrm.z as f32]);
                let (vv, uv) = if k == ck {
                    (pos[k] - centre, [0.0, 0.0])
                } else {
                    (pos[ck] - dirs[ck] * skirt_depth - centre, [1.0, 0.0])
                };
                out.verts.push([vv.x as f32, vv.y as f32, vv.z as f32]);
                out.uvs.push(uv);
                let c = self.biome_color(rows[k]);
                out.colors.push([c[0], c[1], c[2], lod_depth / 16.0]);
                out.heights.push(hs[k] as f32);
                if k == ck {
                    out.min_h = out.min_h.min(hs[k] as f32);
                    out.max_h = out.max_h.max(hs[k] as f32);
                }
            }
        }
        out.biomes = rows.clone();

        if with_scatter {
            let sites = self.sites_near(centre_dir, edge_m * 1.5 + self.recipe.sites.clear_radius_m);
            let (ix, iy) = (((a0 + 1.0) / size).round() as u32, ((b0 + 1.0) / size).round() as u32);
            let depth = (2.0 / size).log2().round() as u32;
            let key = hash(self.recipe.seed as u32, face as u32, ix, iy, depth);
            for (ri, rule) in self.recipe.scatter.iter().enumerate() {
                let mut so = ScatterOut { kind: rule.kind.clone(), ..Default::default() };
                let cells = ((edge_m / rule.spacing_m).round() as u32).max(1);
                let mask = &self.masks[ri];
                let cos_slope = rule.slope_max_deg.to_radians().cos();
                for ci in 0..cells {
                    for cj in 0..cells {
                        let s = (ci as f64 + hash01(key, ri as u32, ci, cj, 0) as f64) / cells as f64;
                        let t = (cj as f64 + hash01(key, ri as u32, ci, cj, 1) as f64) / cells as f64;
                        let dir = cube_to_sphere(face, a0 + s * size, b0 + t * size);
                        if nz(mask, self.p32(dir)) <= rule.mask.threshold {
                            continue;
                        }
                        let (gx, gy) = (s * GRID as f64, t * GRID as f64);
                        let gi = (gx.floor() as usize).min(GRID - 1);
                        let gj = (gy.floor() as usize).min(GRID - 1);
                        let (fx, fy) = (gx - gi as f64, gy - gj as f64);
                        let k00 = (gj + 1) * M + gi + 1;
                        let (k10, k01, k11) = (k00 + 1, k00 + M, k00 + M + 1);
                        let lerp2 = |a: f64, b: f64, c: f64, d: f64| {
                            let x0 = a + (b - a) * fx;
                            let x1 = c + (d - c) * fx;
                            x0 + (x1 - x0) * fy
                        };
                        let h = lerp2(hs[k00], hs[k10], hs[k01], hs[k11]);
                        if rule.above_sea && h <= self.sea {
                            continue;
                        }
                        let nv = |f: fn(&V3) -> f64| lerp2(f(&nrms[k00]), f(&nrms[k10]), f(&nrms[k01]), f(&nrms[k11]));
                        let nrm = v3(nv(|v| v.x), nv(|v| v.y), nv(|v| v.z)).normalized();
                        if nrm.dot(dir) < cos_slope {
                            continue;
                        }
                        let nearest = (gj + (fy >= 0.5) as usize + 1) * M + gi + (fx >= 0.5) as usize + 1;
                        let row = rows[nearest];
                        let dens = rule.row_density.get(&row.to_string()).or_else(|| rule.row_density.get("default")).copied().unwrap_or(0.0);
                        if hash01(key, ri as u32, ci, cj, 2) >= dens {
                            continue;
                        }
                        let clear = self.recipe.sites.clear_radius_m;
                        if sites.iter().any(|sd| r * sd.dot(dir).clamp(-1.0, 1.0).acos() < clear) {
                            continue;
                        }
                        let up = dir;
                        let rf = if up.y.abs() < 0.99 { v3(0.0, 1.0, 0.0) } else { v3(1.0, 0.0, 0.0) };
                        let tg = up.cross(rf).normalized();
                        let bt = up.cross(tg);
                        let yaw = hash01(key, ri as u32, ci, cj, 3) as f64 * std::f64::consts::TAU;
                        let x = tg * yaw.cos() + bt * yaw.sin();
                        let z = x.cross(up);
                        let sc = (rule.scale_min + (rule.scale_max - rule.scale_min) * hash01(key, ri as u32, ci, cj, 4)) as f64;
                        let o = dir * (r + h) - centre;
                        let (x, u, z) = (x * sc, up * sc, z * sc);
                        let tint = self
                            .recipe
                            .biomes
                            .iter()
                            .find(|b| b.id == row)
                            .map(|b| b.tints.get(&rule.kind).copied().unwrap_or(b.color))
                            .unwrap_or([1.0; 3]);
                        let v = 0.9 + 0.2 * hash01(key, ri as u32, ci, cj, 5);
                        so.buffer.extend([
                            x.x, u.x, z.x, o.x, x.y, u.y, z.y, o.y, x.z, u.z, z.z, o.z,
                        ].iter().map(|f| *f as f32));
                        so.buffer.extend([tint[0] * v, tint[1] * v, tint[2] * v, 1.0]);
                    }
                }
                out.scatter.push(so);
            }
        }
        out
    }
}
