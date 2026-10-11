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
    /// Biome palette per vertex (#66): ground rgb + cap share, rock rgb + strata share. Under a
    /// lake or river the alpha is minus the wetness (0..1, full a metre down) instead of the cap
    /// share (#72): the terrain shader then leaves out the sea's tint and shore band.
    pub colors: Vec<[f32; 4]>,
    pub rock: Vec<[f32; 4]>,
    /// The biome border (#235): per vertex the two nearest rows in a fixed order, as their ground
    /// colours (`colors` holds the lower id's, this the higher id's rgb) and the higher id's share
    /// in alpha (0.5 on the border). The shader picks one per pixel, frayed by noise, so a border
    /// is no longer a stair step of the vertex grid. Rock, cap and strata blend by the share here.
    pub ground_b: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    /// Height above the base radius per vertex (M*M, skirt ring included), metres.
    pub heights: Vec<f32>,
    pub biomes: Vec<u8>,
    /// Sea surface over this chunk (#67), when any vertex is below sea level: M*M positions
    /// relative to the centre on the sphere at sea level; the skirt ring sits on its edge
    /// vertex (no area: a dipped skirt showed as a dark line through the transparent water).
    pub water: Option<Vec<[f32; 3]>>,
    /// The sea's triangles (the terrain's layout): every quad but those under a lake or river
    /// surface above the sea (#72; the sea sheet showed through a lake over a pocket below the
    /// sea level).
    pub water_tris: Vec<u32>,
    /// Lakes and rivers over this chunk (#72): M*M positions on their surface relative to the
    /// centre, and the triangles of the quads that hold water (all four corners on a surface, one
    /// of them wet; same winding as the terrain). None when the chunk has none.
    pub inland_water: Option<(Vec<[f32; 3]>, Vec<u32>)>,
    /// Geomorphing (#220): per vertex, what to add to `verts` to get the vertex where the PARENT's
    /// mesh has it, the chunk at half resolution (even grid vertices stay, an odd one goes to the
    /// middle of its two even neighbours along the edge, or along the cell's diagonal). A
    /// chunk's children at full morph look exactly like the parent at none. Skirt vertices follow
    /// the edge vertex they hang from.
    pub morph_delta: Vec<[f32; 3]>,
    /// The normals of that coarse shape (the parent mesh's), absolute.
    pub morph_normals: Vec<[f32; 3]>,
    pub min_h: f32,
    pub max_h: f32,
}

impl ChunkOut {
    /// A hash of the whole chunk (positions, normals, colours, heights, water): equal chunks hash
    /// equal, on any thread and in any run.
    pub fn hash(&self) -> u64 {
        use crate::coarse::{fnv, FNV_START};
        let mut h = FNV_START;
        let mut put = |bytes: &[u8]| h = fnv(bytes, h);
        for c in self.center {
            put(&c.to_le_bytes());
        }
        for v in self.verts.iter().chain(&self.normals) {
            v.iter().for_each(|x| put(&x.to_le_bytes()));
        }
        for v in self.colors.iter().chain(&self.rock).chain(&self.ground_b) {
            v.iter().for_each(|x| put(&x.to_le_bytes()));
        }
        self.uvs.iter().flatten().for_each(|x| put(&x.to_le_bytes()));
        self.heights.iter().for_each(|x| put(&x.to_le_bytes()));
        put(&self.biomes);
        for w in self.water.iter().chain(self.inland_water.as_ref().map(|(p, _)| p).into_iter()) {
            w.iter().flatten().for_each(|x| put(&x.to_le_bytes()));
        }
        self.water_tris.iter().for_each(|x| put(&x.to_le_bytes()));
        h
    }
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
        // exactly representable in f32, so the mesh's f32 position is exact
        let c64 = centre_dir * r;
        let centre = v3(c64.x as f32 as f64, c64.y as f32 as f64, c64.z as f32 as f64);
        let edge_m = self.chunk_edge_m(face, a0, b0, size);
        let skirt_depth = (edge_m / GRID as f64 * 4.0).max(2.0);

        let mut pos = vec![V3::default(); M * M];
        let mut dirs = vec![V3::default(); M * M];
        let mut hs = vec![0.0f64; M * M];
        let mut rows = vec![0u8; M * M];
        let mut pairs = vec![BiomePair::new(0, 0, 0.0, 0.0); M * M];
        let mut levels = vec![f64::NAN; M * M];
        for j in 0..M {
            for i in 0..M {
                let a = a0 + (i as f64 - 1.0) * step;
                let b = b0 + (j as f64 - 1.0) * step;
                let dir = cube_to_sphere(face, a, b);
                let (h, f, lf) = self.height_lf_ab(face, a.clamp(-1.0, 1.0), b.clamp(-1.0, 1.0), dir);
                let k = j * M + i;
                levels[k] = self.water_level_ab(face, a.clamp(-1.0, 1.0), b.clamp(-1.0, 1.0)).unwrap_or(f64::NAN);
                pos[k] = dir * (r + h);
                dirs[k] = dir;
                hs[k] = h;
                pairs[k] = self.biome_pair(h - self.sea, &f, lf);
                rows[k] = pairs[k].nearest;
            }
        }

        let mut out = ChunkOut { center: centre.arr(), min_h: f32::MAX, max_h: f32::MIN, ..Default::default() };
        out.verts.reserve_exact(M * M);
        out.normals.reserve_exact(M * M);
        out.colors.reserve_exact(M * M);
        out.ground_b.reserve_exact(M * M);
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
                let pair = pairs[k];
                let (pa, pb) = (self.palette_ref(pair.lo), self.palette_ref(pair.hi));
                let t = pair.hi_share;
                let mix = |a: f32, b: f32| a + (b - a) * t;
                let (mut ga, mut gb) = (pa.ground, pb.ground);
                let mut rock = [0.0f32; 3];
                for c in 0..3 {
                    rock[c] = mix(pa.rock[c], pb.rock[c]);
                }
                // Under a lake or river the ground darkens with depth, as the terrain shader does
                // under the sea (#72).
                let depth = levels[k] - hs[k];
                let mut alpha = mix(pa.cap, pb.cap);
                if depth > 0.0 {
                    alpha = -(depth.min(1.0) as f32);
                    let wl = &self.recipe.water;
                    let w = (depth / wl.depth_m.max(1e-3) as f64).clamp(0.0, 1.0) as f32;
                    for c in 0..3 {
                        ga[c] += (wl.deep[c] - ga[c]) * w;
                        gb[c] += (wl.deep[c] - gb[c]) * w;
                        rock[c] += (wl.deep[c] - rock[c]) * w;
                    }
                }
                out.colors.push([ga[0], ga[1], ga[2], alpha]);
                out.ground_b.push([gb[0], gb[1], gb[2], t]);
                out.rock.push([rock[0], rock[1], rock[2], mix(pa.strata, pb.strata)]);
                out.heights.push(hs[k] as f32);
                if k == ck {
                    out.min_h = out.min_h.min(hs[k] as f32);
                    out.max_h = out.max_h.max(hs[k] as f32);
                }
            }
        }
        out.biomes = rows;
        morph_targets(&mut out, &pos, &dirs, centre, skirt_depth);
        if (out.min_h as f64) < self.sea {
            let mut w = Vec::with_capacity(M * M);
            for j in 0..M {
                for i in 0..M {
                    let ck = j.clamp(1, M - 2) * M + i.clamp(1, M - 2);
                    let p = dirs[ck] * (r + self.sea) - centre;
                    w.push([p.x as f32, p.y as f32, p.z as f32]);
                }
            }
            out.water = Some(w);
            let above = |k: usize| {
                let ck = (k / M).clamp(1, M - 2) * M + (k % M).clamp(1, M - 2);
                levels[ck].is_finite() && levels[ck] > self.sea + 0.01
            };
            for j in 0..M - 1 {
                for i in 0..M - 1 {
                    let (k00, k10, k01, k11) = (j * M + i, j * M + i + 1, (j + 1) * M + i, (j + 1) * M + i + 1);
                    if ![k00, k10, k01, k11].iter().all(|&k| above(k)) {
                        out.water_tris.extend([k00, k10, k01, k10, k11, k01].map(|k| k as u32));
                    }
                }
            }
            if out.water_tris.is_empty() {
                out.water = None;
            }
        }
        let mut tris = Vec::new();
        for j in 1..=GRID {
            for i in 1..=GRID {
                let (k00, k10, k01, k11) = (j * M + i, j * M + i + 1, (j + 1) * M + i, (j + 1) * M + i + 1);
                let q = [k00, k10, k01, k11];
                if q.iter().all(|&k| levels[k].is_finite()) && q.iter().any(|&k| levels[k] > hs[k]) {
                    tris.extend([k00, k10, k01, k10, k11, k01].map(|k| k as u32));
                }
            }
        }
        if !tris.is_empty() {
            let w = (0..M * M)
                .map(|k| {
                    let level = if levels[k].is_finite() { levels[k] } else { hs[k] };
                    let p = dirs[k] * (r + level) - centre;
                    [p.x as f32, p.y as f32, p.z as f32]
                })
                .collect();
            out.inland_water = Some((w, tris));
        }

        out
    }
}

/// The two real vertices (grid indices 1..=M-2) whose middle is the coarse (parent-level)
/// position of `(i, j)`: the vertex itself on even ones, else its neighbours along the edge, or
/// along the cell diagonal (the shared index buffer runs it from (i+1, j) to (i, j+1)).
fn coarse_pattern(i: usize, j: usize) -> [(usize, usize); 2] {
    match ((i - 1) % 2, (j - 1) % 2) {
        (0, 0) => [(i, j), (i, j)],
        (1, 0) => [(i - 1, j), (i + 1, j)],
        (0, 1) => [(i, j - 1), (i, j + 1)],
        _ => [(i + 1, j - 1), (i - 1, j + 1)],
    }
}

fn morph_targets(out: &mut ChunkOut, pos: &[V3], dirs: &[V3], centre: V3, skirt_depth: f64) {
    let at = |(i, j): (usize, usize)| j * M + i;
    // Normals of the coarse mesh at the even vertices: central differences two cells wide.
    let coarse_n = |i: usize, j: usize| {
        let (i0, i1, j0, j1) = (i.saturating_sub(2).max(1), (i + 2).min(M - 2), j.saturating_sub(2).max(1), (j + 2).min(M - 2));
        let n = (pos[at((i1, j))] - pos[at((i0, j))]).cross(pos[at((i, j1))] - pos[at((i, j0))]).normalized();
        if n.dot(dirs[at((i, j))]) < 0.0 { -n } else { n }
    };
    let mut tp = vec![V3::default(); M * M];
    let mut tn = vec![V3::default(); M * M];
    for j in 1..M - 1 {
        for i in 1..M - 1 {
            let [a, b] = coarse_pattern(i, j);
            tp[at((i, j))] = (pos[at(a)] + pos[at(b)]) * 0.5;
            let (na, nb) = (coarse_n(a.0, a.1), coarse_n(b.0, b.1));
            tn[at((i, j))] = if a == b { na } else { (na + nb).normalized() };
        }
    }
    out.morph_delta.reserve_exact(M * M);
    out.morph_normals.reserve_exact(M * M);
    for j in 0..M {
        for i in 0..M {
            let k = j * M + i;
            let ck = j.clamp(1, M - 2) * M + i.clamp(1, M - 2);
            let target = if k == ck { tp[ck] } else { tp[ck] - dirs[ck] * skirt_depth };
            let own = out.verts[k];
            let d = target - centre - v3(own[0] as f64, own[1] as f64, own[2] as f64);
            out.morph_delta.push([d.x as f32, d.y as f32, d.z as f32]);
            let n = tn[ck];
            out.morph_normals.push([n.x as f32, n.y as f32, n.z as f32]);
        }
    }
}
