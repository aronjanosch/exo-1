# Spike 8 report: the procedural planet

Branch `spike/planet-gen`. Written by the cloud session; the look and feel is for the initiator to judge locally (see "Look at first"). Tags: **measured** (this session, this machine: 8 cores, no GPU), **calculated**, **assumed**.

## What was built

- `rust/planet_core` (plain Rust): recipe parsing (`recipe.json`, serde), macro bake, three stamped features, sea level, the one height function, chunk build, scatter, sites, statistics. `cargo test --profile fast` runs T1 (core), T2, T3 and a core-side T5 estimate.
- `rust/planet_godot`: GDExtension class `PlanetGen` (thin conversion layer). API as in the brief plus `patch_heights`, `sea_level`, `height_range`, `biome_colors`, `height_at_xyz` (double precision, for tests) and a `radius_override`/`seed_override` on `load_recipe`.
- `recipe.json`: seed, radius, macro noises, noise bands (region, face, foot, warped with warp), stamps, sea-level rule, biome table (4 rows, ids 0-3, no names; rows are an ordered list of conditions, first match wins, a fifth row is a data change only), scatter rules (per-row density), site rule.
- Scene: `terrain.gd` builds chunks through `PlanetGen.build_chunk` (quadtree, job model, skirts, uploads kept; bounds use the real relief); `collision_ring.gd` gets patch heights from `PlanetGen.patch_heights`; the player's safety net and the overlay use `PlanetGen.height_at`/`sample`. Water sphere at sea level (no collision, double sided). MultiMesh canopy and rocks on the finest chunks (`dressing.gd`), 24 pillar site markers, biome-tinted shader. Each planet body has its own `PlanetGen` (start planet = recipe seed, other planets their own seed).
- Debug: F3 overlay ground line (biome, height above sea, slope, temperature, moisture, landform, macro elevation, stamp height, distance to nearest site); key **O** orbit camera (15 km, mouse rotates, wheel zooms); `--planet-walk`, `--planet-shots`. Keys and options are in `spikes/planet/README.md`.
- Tests and tools: `rust/planet_core/tests/core.rs`, `scene_test.gd` (T1 scene), `bench.gd` (T4, T6), `planet_walk.gd` (T5), `planet_shots.gd`, `rust_builds/check_planet.gd` and `build_export.py` (now takes `PROJECT=planet_gen`). Raw outputs are in `results/`.

## Installed in the container

Rust 1.99.0 stable (mise, rustup) plus the target `x86_64-pc-windows-gnullvm`; Godot 4.7.2-stable (mise `godot`); llvm-mingw 20260922 (mise `github:mstorsjo/llvm-mingw`); Godot 4.7.2 export templates in `~/.local/share/godot/export_templates/4.7.2.stable`; for screenshots only: `libxcursor1` and `libxinerama1` unpacked from Ubuntu `.deb` files into `~/lib-extra` (used through `LD_LIBRARY_PATH`, nothing system-wide). Crates: only `godot` 0.5.5, `fastnoise-lite` 1.1.1, `serde`, `serde_json`. Xvfb was already there.

## Tests

### T1 one height function (measured)
- Core, 1000 random directions: face-coordinate path versus direction path, max difference 7.2e-13 m.
- Core, every interior vertex of 50 random chunks (depths 2, 4, 6, 8; 54450 vertices): max |mesh height - height_at| = 0.55 mm.
- Scene (`scene_test.gd`, the meshes the terrain really built, 1050 chunk meshes at depths 0-8, 1,143,450 vertices): max 0.585 mm. The limit is the 32-bit vertex storage of coarse chunks (vertices are up to 8 km from their chunk centre at depth 0).
- Collision: core patches (1 m spacing, 24576 samples over escarpment, plateau edge, basin rim, flat ground, origin rounded to f32): 0.88 mm. Scene (183 patches, 187,392 samples, frame centre exact): 0.30 mm. **With the 32-bit body origin the physics server sees, the worst error is 2.3 mm** (position resolution of float32 at R = 5 km times terrain slope); that is an engine limit, not a generator error.
- Note: a `Vector3` (32-bit) direction is 0.3 mm of lateral position at 5 km, which alone makes a 1 mm test on steep ground unreliable. `height_at_xyz` takes doubles for that reason.

### T2 no seams (measured)
All border vertices of every chunk at depth 1, 3, 5 (3072, 49152, 786432 vertices) have a partner in a neighbouring chunk, worst distance 0.086 mm, 0.043 mm, 0.013 mm, unmatched 0. This covers all twelve cube edges. What made it work: the macro images are vertex-centred (513 x 513 samples, edge samples exactly on the cube edge), so both faces interpolate the same samples along a shared edge; heights are computed in f64 and chunk centres are rounded to f32 so the node position is exact.

### T3 macro statistics, `bake()` (measured; recipe as committed)
| quantity | value |
|---|---|
| sea level | -7.49 m (30th area-weighted percentile of the macro elevation) |
| land fraction, macro elevation | 70.00 % (by construction) |
| land fraction, full height function | **79.7 %** (the bands have a positive mean: ridged noise) |
| area share of rows 0 / 1 / 2 / 3 (full height, tuned recipe) | 41.4 % / 45.1 % / 11.0 % / 2.5 % (baseline recipe: 46.4 / 37.3 / 13.8 / 2.5) |
| height above sea, min / max / mean | -90.2 m / +166.7 m / +20.0 m |
| basin below sea, contiguous run through the centre, two great circles | 1680 m and 1690 m (target > 1 km: met); total below-sea span 1870 m and 1690 m; centre is 80.5 m below sea |
| plateau as built | stamp +110.0 m; mean full height above sea within 350 m of the centre +133.5 m |
| escarpment as built | stamp step +60.0 m; mean full-height step between the two sides (200 m each side of the line) +73.7 m |
| bake time | 486-547 ms on 8 threads (macro 247-288, sea level 101-106, statistics 132-157, sites 0.5); 2.2 s on 1 thread |

### T4 sites (measured)
24 placed (budget 24, 600 m minimum separation, land only, slope under 20 degrees). Smallest pairwise distance 726.9 m (>= 600 m), mean nearest-neighbour distance 2041 m, largest nearest-neighbour distance **3414 m** (the longest hike to the next site is 3.4 km, about 32 minutes at 1.8 m/s). With 24 sites on about 250 km² of land the sites are far apart; see open questions.

### T5 walk (measured, `--planet-walk --fixed-fps 60`; 1.8 m/s, 300 s simulated, 4 starts, biome and height every metre)
Baseline recipe (region wavelengths as in the brief), `results/t5_walk_baseline_recipe.txt`:

| start | path | biome changes | shortest complete stretch | height above sea | water | rescues |
|---|---|---|---|---|---|---|
| spawn, east | 547 m | 0 (row 0 all the way) | none | -16.1..+11.8 | yes | 0 |
| basin shore, towards centre | 558 m | 0 (row 1) | none | -60.0..+13.0 | yes | 0 |
| escarpment foot, up the step | 313 m | 1 | 294 m | -8.4..+17.1 | yes | 0 |
| plateau approach | 354 m | 0 (row 0) | none | +17.6..+63.0 | no | 0 |

Finding: five minutes of walking is only 540 m, and with the brief's wavelengths one biome change comes about every 926 m (core estimate over 150 random great circles, median complete stretch 380 m), so a walk often stays in one row. I changed two test values: moisture noise frequency 0.0003 to 0.0008 and landform noise frequency 0.0004 to 0.001 (core estimate: one change per 489 m, median stretch 271 m; 50 of 613 stretches shorter than 20 m, caused by the height thresholds along coasts). Tuned recipe (committed), `results/t5_walk_tuned_recipe.txt`:

| start | path | biome changes | shortest complete stretch | height above sea | water | rescues |
|---|---|---|---|---|---|---|
| spawn, east | 547 m | 1 | 250 m | -16.1..+11.8 | yes | 0 |
| basin shore | 558 m | 0 | none | -60.0..+13.0 | yes | 0 |
| escarpment foot | 310 m | 2 | 48 m | -8.4..+14.8 | yes | 0 |
| plateau approach | 354 m | 2 | 21 m | +17.6..+62.9 | no | 0 |

No fall-through: 0 rescues in all 8 runs (72,000 frames), 0 frames without a collision patch. Two walks stopped short (313 and 354 m instead of 540 m): the walker is stopped by the steep side of the escarpment and the plateau slope (floor angle limit 50 degrees); that is the measured result, not a bug. "Frames not on floor" is 6-37 % in these runs (e.g. 5362 of 18000 at the spawn): the capsule is not flagged as on the floor for part of the walk although it follows the ground (no rescues); cause not investigated (assumption: floor snapping on the 1 m height-field steps at 1.8 m/s).

### T6 cost (measured, CPU only, `fast` profile, from GDScript including the conversion into packed arrays, main thread, 300 chunks per row)
| depth | without scatter mean / P95 / max (us) | with scatter |
|---|---|---|
| 0 | 2344 / 2437 / 2713 | |
| 2 | 2260 / 2358 / 2543 | |
| 4 | 2013 / 2058 / 2172 | |
| 6 | 1811 / 1865 / 2034 | |
| 7 | 1747 / 1856 / 2052 | 1801 / 1915 / 1980 |
| 8 (finest) | 1664 / 1737 / 1899 | 1687 / 1760 / 1930 |

The brief's measured Rust chunk of spike 6 was 0.65 ms; this one is 2.5-3x slower (f64 math, stamps, biome table, conversion). The LOD asks for at most about 240 chunks/s, so the worker load is about half a core. Scatter at the finest depth (1500 random chunks of about 31 m): mean 9.0 instances per chunk, max 36, 87 % of chunks have some; canopy 3008 and rocks 10539 in total (canopy 2.0 per chunk, rocks 7.0). Bake 486 ms (8 threads), sites 0.5 ms. In a 130 ms-per-frame software-rendered shot the scene had 974 chunk nodes and 916 draw calls near the ground (not a frame-rate measurement).

### T7 regressions (measured)
- `--auto-test` (walk, board, climb, cruise, land, cabin phases): exit 0, rescues 0 in every phase (twice, before and after the last shader change). `shutdown_test.gd`: PASS.
- Spike-7 build script: `build_export.py` (now with `PROJECT=` selecting the workspace; default unchanged) builds Linux and Windows release libraries and exports both for spike 7's `gen_bench` (270 s / 263 s cargo, exit 0) and for spike 8 (`PROJECT=planet_gen`, 102 s / 101 s cargo, exit 0, Linux and Windows exports 9-10 s each). Spike 7's own `check.gd` was not re-run on the export.
- The exported Linux check build of spike 8 (`check_planet.gd`) runs headless and returns ok (sea level, 24 sites, 1000-direction height checksum 11950.598, 200 chunks). The exported Windows build was produced (exe, `planet_godot.dll`, `libunwind.dll`) but **not run**: no Windows machine or Wine here.

### Screenshots (9 files in `shots/`)
No Vulkan device exists here (no loader, no `/dev/dri`). The shots were taken with the Compatibility renderer (OpenGL on Mesa llvmpipe) under Xvfb, so the look can differ slightly from Forward+ (the overlay text still says forward_plus because it prints the project setting). Four orbit views (`orbit-basin`, `orbit-rim`, `orbit-plateau`, `orbit-far`), `basin-shore`, `escarpment`, `plateau-edge`, `forest-edge`, `site-marker`. Light is moved behind the camera for these shots only. 960 x 540.

## Assumptions (all marked in code or recipe)
- Stamps are evaluated analytically in the height function (resolution independent); the baked macro images hold the noise-only elevation, temperature, moisture, landform and one reserved unused channel (wind and rain shadow). Sea level is computed over macro noise plus stamps.
- Macro images are vertex-centred 513 x 513 per face (the brief said 512²); face lookup by dominant axis plus a Gauss-Newton inverse of the cube mapping.
- Basin: bowl `depth * (1 - t²)²`. Escarpment: runs east-west (tangent = centre x world up), the raised side is a shelf 1000 m deep with 300 m end and back tapers, forces landform id 3. Plateau: smoothstep falloff over the outer 120 m.
- Temperature at a vertex = macro temperature (1.0 - 0.5 |y| + 0.3 noise) minus 0.004 per metre above sea. Biome rules: row 3 (cold high) above +60 m and temperature <= 0.5; row 2 (rim) landform id 3; row 0 (wet) moisture >= 0; row 1 otherwise. First match wins.
- Water: double-sided transparent sphere, sea level -7.49 m; the walker can walk under it (no swimming, no collision, as decided).
- Scatter: cell grid sized per chunk (about 3 x 3 canopy cells, 5 x 5 rock cells in a 31 m chunk), instance on the mesh triangle height, tint from the biome row.
- Sites: plain dart throwing on the sphere, deterministic from the seed; marker 24 m tall, 0.9 m base, orange; site must be 2 m above sea and under 20 degrees slope.
- Orbit key O, walker paused while orbiting; the screenshot script moves the sun.
- The LOD debug colour is carried in the vertex alpha (depth / 16) because per-instance shader uniforms ran out of slots on the GL driver ("Too many instances using shader instance variables").
- T5 start points (basin 900 m out, escarpment 300 m below the step, plateau 1000 m out) are my choice.

## Problems and how they were solved
- Seams between cube faces with an image bilinear lookup that clamps inside the face: solved with vertex-centred macro images.
- 1 mm tests on steep ground failed because of 32-bit directions and body origins in the engine API: added `height_at_xyz` and report both numbers.
- `--fixed-fps` makes frames faster than the worker threads, so settle loops must count wall time or run without it (`scene_test.gd` runs without).
- Scripted-run parse errors (untyped `main`) cost a 15 minute walk run that did nothing; fixed with explicit types.
- Land fraction: the sea level aims at 70 % on the macro elevation but the full height has 79.7 % land. Not corrected (brief says macro); one number to change in the recipe or the rule if 70 % full land is wanted.

## Look at first (initiator, locally)
1. Orbit (O): the basin is a lake and the plateau a white cap; the rim is only 2 km long and hard to see from 15 km. Biome rows read as large flat-coloured blotches (rows are decided by a single noise each).
2. On foot: the escarpment and the plateau slope stop the walker (steep). Decide if that is wanted.
3. Forest edges and canopy density (`forest-edge.png`), site pillars (24 m is visible from far on flat ground).
4. Frame rate with the MultiMesh canopy (not measurable here).

## Open questions
- Sites: 24 sites give a 3.4 km worst hike. Raise the budget or use a more even sampling (best-candidate) if sites should be about 1 km apart? A design decision.
- Which row should be the rim of the broken ground: today it is a landform id from noise plus the escarpment; no design input exists.
- Slope limit of the walker (50 degrees) versus the escarpment step (up to about 48 degrees plus noise).
- `fastnoise-lite` is f32 only; seam tolerance is met with f64 geometry. No additional dependencies were needed.

## Learnings (for LEARNINGS.md in the concept repo)
- **Vertex-centred cube-face images make seams impossible by construction.** Why: both faces interpolate the same samples on a shared edge. How to apply: any baked per-face field on a cube sphere (heights, biomes, moisture) should include the edge samples (N+1 per side), not texel centres.
- **A one-millimetre agreement test at 5 km is limited by 32-bit numbers, not by the generator.** Why: a float32 position at 5000 m has 0.5 mm resolution and a 32-bit direction 0.3 mm of lateral position; times slope that is 1-2 mm. How to apply: do geometry in f64 (Rust), make chunk centres f32-exact, and give tests a double-precision query.
- **Biome wavelengths must be sized by walking distance, not by planet size.** Why: five minutes at 1.8 m/s is 540 m; the research wavelength (2.5 km) gave one change per 926 m. How to apply: choose region frequencies so the median stretch is about half the intended walk (here 0.0008 to 0.001 gave 271 m).
- **Test settle loops in headless runs must not depend on frame counts under `--fixed-fps`.** Why: frames run faster than worker threads. How to apply: wait for pending jobs to be empty, with wall-clock limits.
