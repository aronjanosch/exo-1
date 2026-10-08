//! Chunk build: crust mesh arrays (scatter: scatter.rs). `&self` only, thread-safe after bake.
use crate::math::*;
use crate::planet::*;

pub const GRID: usize = 32;
pub const M: usize = GRID + 3;

#[derive(Default)]
pub struct ChunkOut {
    pub center: [f64; 3],
    pub verts: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Biome palette per vertex (#66): ground rgb + cap share, rock rgb + strata share.
    pub colors: Vec<[f32; 4]>,
    pub rock: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    /// Height above the base radius per vertex (M*M, skirt ring included), metres.
    pub heights: Vec<f32>,
    pub biomes: Vec<u8>,
    pub min_h: f32,
    pub max_h: f32,
}

impl Planet {
    /// Chunk edge length in metres (along an edge of the chunk, base sphere).
    pub fn chunk_edge_m(&self, face: usize, a0: f64, b0: f64, size: f64) -> f64 {
        (cube_to_sphere(face, a0, b0) - cube_to_sphere(face, a0 + size, b0)).length() * self.radius
    }

    pub fn build_chunk(&self, face: usize, a0: f64, b0: f64, size: f64) -> ChunkOut {
        let r = self.radius;
        let step = size / GRID as f64;
        let centre_dir = cube_to_sphere(face, a0 + size * 0.5, b0 + size * 0.5);
        // exactly representable in f32, so the mesh node position in Godot is exact
        let c64 = centre_dir * r;
        let centre = v3(c64.x as f32 as f64, c64.y as f32 as f64, c64.z as f32 as f64);
        let edge_m = self.chunk_edge_m(face, a0, b0, size);
        let skirt_depth = (edge_m / GRID as f64 * 4.0).max(2.0);

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
                let p = self.palette(rows[k]);
                out.colors.push([p.ground[0], p.ground[1], p.ground[2], p.cap]);
                out.rock.push([p.rock[0], p.rock[1], p.rock[2], p.strata]);
                out.heights.push(hs[k] as f32);
                if k == ck {
                    out.min_h = out.min_h.min(hs[k] as f32);
                    out.max_h = out.max_h.max(hs[k] as f32);
                }
            }
        }
        out.biomes = rows.clone();

        out
    }
}
