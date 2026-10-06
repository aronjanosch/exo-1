# Spike 6 workload spec (identical in every variant)

Benchmark values only. Nothing here is a design decision (biome rows, sizes, densities, thresholds are test values, origin: assumption). Source: `SPIKE-6-BRIEF.md` plus research note section 7/8 in the concept repo.

## Constants

- `R = 6000.0` m, `SEED = 1337`, `GRID = 32`, `M = GRID + 3 = 35` (grid with a one-vertex ring for normals/skirts, same layout as `terrain.gd::_build_job` from spike 1).
- Chunks: depth 7 (`2 / 128` cube units per side, about 74 m). Chunk id = `(face, ix, iy)`, `ix, iy` in 0..127. Origin of the chunk in cube coordinates: `a0 = -1 + ix * size`, `b0 = -1 + iy * size`, `size = 2 / 128`. Chunk centre = `cube_to_sphere(face, a0 + size/2, b0 + size/2) * R`.
- Chunk list (200): for `k` in 0..199: `face = k % 6`, `ix = (k * 37 + 11) % 128`, `iy = (k * 53 + 29) % 128` (integer maths).
- `cube_to_sphere` and `FACE_NORMALS`: exactly as in `spikes/planet/terrain.gd`.
- Indices are a constant array for the grid; built once, not part of timing, not compared.

## Noise (all `TYPE_SIMPLEX_SMOOTH` = OpenSimplex2S, lacunarity 2.0, gain 0.5, input = `dir * R` in metres unless stated)

| name | seed | frequency | fractal | octaves |
| --- | --- | --- | --- | --- |
| region | SEED+1 | 0.00033 | FBM | 3 |
| face | SEED+2 | 0.0011 | RIDGED | 4 |
| foot | SEED+3 | 0.0066 | FBM | 4 |
| warp | SEED+4 | 0.0025 | NONE | 1 |
| warped | SEED+5 | 0.002 | FBM | 2 |
| m_elev | SEED+10 | 0.0002 | FBM | 3 |
| m_moist | SEED+11 | 0.0003 | FBM | 2 |
| m_temp | SEED+12 | 0.0003 | FBM | 2 |
| m_land | SEED+13 | 0.0004 | NONE | 1 |
| forest | SEED+20 | 0.005 | FBM | 2 |

## Macro bake (once per seed, timed separately)

512 x 512 texels per cube face, 4 floats per texel `(elev, temp, moist, landform)`. Texel `(i, j)` of face `f`: `a = -1 + (i + 0.5) * 2 / 512`, `b = -1 + (j + 0.5) * 2 / 512`, `p = cube_to_sphere(f, a, b) * R`.

- `elev = m_elev(p)`
- `moist = m_moist(p)`
- `temp = 1.0 - abs(dir.y) * 1.2 + 0.3 * m_temp(p) - 0.2 * max(elev, 0.0)`
- `landform = float(clamp(floor((m_land(p) + 1.0) * 2.0), 0, 3))`

Storage: flat float array, index `((f * 512 + j) * 512 + i) * 4 + c`.

Lookup `(face, a, b)`: `u = (a + 1) * 0.5 * 512 - 0.5`, `v` likewise, clamp both to `[0, 511]`. Bilinear for elev, temp, moist (clamped at face edges, no cross-face filtering). Landform: nearest texel `(int(u + 0.5), int(v + 0.5))`, clamped.

## Per vertex (all `M x M` grid vertices, `i, j` in 0..M-1)

1. `a = a0 + (i - 1) * step`, `b = b0 + (j - 1) * step`, `step = size / GRID`. `dir = cube_to_sphere(face, a, b)`, `p = dir * R`.
2. Macro lookup at `(face, a, b)` (clamp `a`, `b` into `[-1, 1]` before the lookup).
3. `h = elev * 80 + region(p) * 100 + face(p) * 60 + foot(p) * 8 + warped(pw) * 25`, with `pw = p + 150 * (warp(p + (1013, 0, 0)), warp(p + (0, 2027, 0)), warp(p + (0, 0, 3041)))`. That is 7 noise calls per vertex.
4. Biome row (first match wins): `h > 120 and temp < 0.45` -> 3; `landform == 3` -> 2; `moist > 0` -> 0; else 1. Colours: row 0 `(0.20, 0.55, 0.20)`, 1 `(0.75, 0.65, 0.40)`, 2 `(0.45, 0.40, 0.38)`, 3 `(0.90, 0.92, 0.95)`, alpha 1. Count rows over the inner 33 x 33 vertices only (`i, j` in 1..33).
5. `pos = dir * (R + h)`. Normal and skirt exactly as in `terrain.gd::_build_job`: `nrm = normalize(cross(pos[ck+1] - pos[ck-1], pos[ck+M] - pos[ck-M]))`, flipped to `dot(nrm, dirs[ck]) >= 0`, with `ck` the clamped index; `verts[k] = pos[k] - centre` for inner vertices, `pos[ck] - dirs[ck] * skirt_depth - centre` for ring vertices (uv `(1, 0)`, else `(0, 0)`), `skirt_depth = max(2.0, edge_m / GRID * 4.0)`, `edge_m = |cube_to_sphere(face, a0, b0) - cube_to_sphere(face, a0 + size, b0)| * R`.

## Scatter (after the surface exists)

Uses the inner 33 x 33 heights, normals and `dirs`. Hash (all `uint32`, wrapping):

```
lowbias32(x): x ^= x >> 16; x *= 0x7feb352d; x ^= x >> 15; x *= 0x846ca68b; x ^= x >> 16
key  = face * 16384 + ix * 128 + iy
hash(kind, ci, cj, salt) = lowbias32((key * 0x9E3779B1) ^ lowbias32(kind * 0x85EBCA6B + ci * 0xC2B2AE35 + cj * 0x27D4EB2F + salt * 0x165667B1))
hash01(...) = (hash(...) >> 8) / 16777216.0
```

Two kinds: canopy (`kind = 0`, 7 x 7 cells) and rock (`kind = 1`, 11 x 11 cells). For each cell `(ci, cj)`: `s = (ci + hash01(kind, ci, cj, 0)) / cells`, `t = (cj + hash01(kind, ci, cj, 1)) / cells`. Grid position `gx = s * GRID`, `gy = t * GRID` (inner grid index = floor + 1 in the `M` grid). Bilinear height `h` and bilinear (then normalised) normal `n` from the four surrounding inner vertices. `dir = cube_to_sphere(face, a0 + s * size, b0 + t * size)`.

Site: the chunk has a site iff `hash(2, 0, 0, 0) & 3 == 0`; its position is the chunk centre direction `dir_c`. Reject a candidate when `|dir - dir_c| * R < 12.0`.

Filters (all must pass):

- canopy: `dot(n, dir) >= 0.8192`, `-20 <= h <= 140`, `forest(dir * R) > 0.1`, site clear.
- rock: `dot(n, dir) >= 0.7071`, `-50 <= h <= 300`, `hash01(1, ci, cj, 2) < 0.5`, site clear.

Accepted candidate writes 12 floats (basis column x, column y, column z, origin): `up = dir`; `ref = (0,1,0)` if `abs(up.y) < 0.99` else `(1,0,0)`; `t = normalize(cross(up, ref))`, `bt = cross(up, t)`; `yaw = hash01(kind, ci, cj, 3) * TAU`; `x = t * cos(yaw) + bt * sin(yaw)`; `z = cross(x, up)`; `scale = 0.8 + 0.6 * hash01(kind, ci, cj, 4)` (canopy) or `0.5 + 1.0 * hash01(kind, ci, cj, 4)` (rock); output `x*scale, up*scale, z*scale, dir * (R + h) - centre`.

## API of every variant (GDScript class, or the GDExtension class `ExoGen`)

```
setup(seed: int)                         # build noise objects
bake() -> void                           # macro bake (timed by the harness)
build_chunk(face: int, ix: int, iy: int) -> Dictionary   # thread-safe after bake
  verts: PackedVector3Array (M*M), normals: PackedVector3Array, colors: PackedColorArray,
  uvs: PackedVector2Array, canopy: PackedFloat32Array (12 per), rocks: PackedFloat32Array (12 per),
  height_sum: float (inner 33x33 heights), biomes: PackedInt32Array(4), usec: int (time inside the generator)
```

Checksum per run (sum over the 200 chunks): `height_sum` (float), canopy count, rock count, biome counts. Variants agree when noise agrees; otherwise report the difference.
