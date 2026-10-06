# Spike 8 brief — the procedural planet (long-running task for a cloud session)

Status: ready to start. Written 2026-10-07 by the initiator's planning session. Self-contained: everything needed from the concept repo is quoted here. Work on branch `spike/planet-gen` only.

Every number below carries its origin: **decided** (the initiator chose it), **test value** (a starting point the spike exists to check, change it if a test shows it is wrong and say so), **measured** (from earlier spikes), **calculated**.

## Goal

Build the first real planet generator and put it into the existing planet scene, so the initiator can walk it and look at it from orbit. The spike answers: does a planet built from a macro shell, a crust mesh and a scatter pass read as a place, on foot and from orbit? The cloud session builds and measures; the initiator judges the look and feel locally afterwards (see "Judged by the initiator").

## Decided inputs (do not change)

- Godot 4.7.2 stays the engine. The terrain generator is written in Rust as a GDExtension (godot-rust). Decided 2026-10-06.
- Planet radius **5000 m** (decided 2026-10-07).
- Four biome rows **without names**, ids 0-3 and test values only (decided 2026-10-07). Suggested roles from the research: 0 wet low ground, 1 dry flats, 2 broken rim, 3 cold high ground. No fiction names anywhere: not in code, data, comments or the report.
- Three stamped macro features (one broad basin, one rim or escarpment, one high plateau), a sea level aiming at about 70 % land and a broad relief of about ±150 m are taken over from the research as **test values** (decided 2026-10-07). Water is a plain sphere at sea level: no waves, no swimming, no collision.
- Platforms: Linux and Windows, no web export, no macOS (decided).
- The planet is a shell you walk on. No digging, no caves (research assumption, kept).

## Scale at 5 km (calculated)

Circumference 31.4 km, surface 314 km², equator to pole 7.9 km. Horizon along the ground for an eye at height h, `R * acos(R / (R + h))`: 1.7 m → 130 m, 10 m → 316 m, 50 m → 705 m, 100 m → 990 m. Standing, the visible world is a disc about 260 m across; a small rise reveals the next place.

## Read first (in this repo)

1. `AGENTS.md` (risk classes, risky APIs, spikes, hard rules).
2. `spikes/planet/README.md`, then `spikes/planet/terrain.gd` (cube-sphere quadtree LOD, chunk jobs on the `WorkerThreadPool`, skirts, `cube_to_sphere`), `collision_ring.gd` (Jolt height-field patches near bodies, computes its own heights today), `main.gd` (`_add_planet`, `height_at`, planet radius 5000), `auto_test.gd` (headless regression run, `--auto-test`).
3. `spikes/gen_bench/SPEC.md` and `spikes/gen_bench/rust/` (spike 6: the Rust generator `gen_core`, the GDExtension `gen_godot` with class `ExoGen`, noise bit-identical to Godot's `FastNoiseLite`, thread-safe `&self` chunk builds with feature `experimental-threads`).
4. `spikes/rust_builds/build_export.py` and `.github/workflows/spike7-rust-builds.yml` (spike 7: Linux and Windows builds, llvm-mingw cross build).

Measured facts from earlier spikes you can rely on: the spike-6 Rust chunk (35x35 grid, 7 noise calls per vertex, scatter) takes about 0.65 ms on one thread and 12000+ chunks/s on 16 threads; the LOD asks for at most about 240 chunks/s; the 4-uploads-per-frame limit in `terrain.gd` was the first bottleneck at high speed. Rust rebuilds take about 0.5 s with the Cargo profile `fast`, 35 s with full `release` (LTO).

## Setup in the cloud

- Godot 4.7.2 stable on `PATH` as `godot` (if missing: official Linux binary from the `godotengine/godot` release `4.7.2-stable`). Use 4.7.x so `project.godot` is not migrated.
- Rust stable with cargo (rustup). No other system packages unless a step needs them; list what you installed in the report.
- `godot --headless --path . --import`. The first import of a fresh checkout can abort on exit (SIGABRT) although it worked; check `.godot/extension_list.cfg`.
- The GPU is probably a software device (lavapipe) or none. Frame rates from the cloud are worthless; measure CPU-side times only.
- Opening the editor rewrites `project.godot` (red class). Restore it with `git checkout -- project.godot` unless a task needs the change.

## Architecture (agreed shape, keep it)

- New Cargo workspace `spikes/planet_gen/rust/` with two crates, started as a copy of spike 6's code:
  - `planet_core` (lib, plain Rust, no Godot types): recipe, macro bake, height function, chunk build, scatter, sites, statistics. Unit tests with `cargo test`.
  - `planet_godot` (cdylib): class `PlanetGen` (RefCounted), thin conversion layer only. `.gdextension` file next to it, Linux and Windows entries like spike 7.
- **One height function.** Mesh vertices, collision patches, the player's safety net, scatter and the debug query all get the height from `planet_core`. Nothing in GDScript computes terrain height any more.
- **The recipe is data**, `spikes/planet_gen/recipe.json`, parsed in Rust with `serde_json`: seed, radius, noise stack (type, frequency, amplitude, octaves, warp), stamped features, sea-level rule, biome table, scatter rules, site budget and spacing. No code, no expressions in the data. A fifth biome row must be a data change only.
- `PlanetGen` API (GDScript-facing; extend if needed, keep it small):
  - `load_recipe(json: String) -> bool`, `bake() -> Dictionary` (returns statistics, see test T3)
  - `height_at(dir: Vector3) -> float`, `heights(dirs: PackedVector3Array) -> PackedFloat32Array`
  - `sample(dir: Vector3) -> Dictionary` with height, sea level, biome id, slope in degrees, macro fields
  - `build_chunk(face: int, a0: float, b0: float, size: float, with_scatter: bool) -> Dictionary` (vertex, normal, colour, uv arrays as in spike 1, plus scatter transforms per kind when asked)
  - `sites() -> PackedVector3Array` (unit directions), `sites_near(dir: Vector3, radius_m: float) -> PackedVector3Array`
- These queries are the hook for the later bot/agent interface: biome, height and sites must be answerable without a mesh or a screenshot.

## Generator (test values unless marked)

1. **Macro shell**, once per seed: baked cube-face images as in spike 6 (512² per face; the research allows an icosphere instead, the cube image is the spike's choice). Fields: elevation, temperature (weak latitude bias + noise, colder with height), moisture (noise; wind and rain shadow are a reserved, unused field), landform id, biome id. Then stamp the three features:
   - basin: centre direction `normalize(1, 0.2, 0.3)`, surface radius 1500 m (so wider than 1 km and the far shore is over the horizon), depth -90 m, smooth bowl
   - escarpment: a 2 km long step along a great-circle segment centred at `normalize(-0.4, 0.3, 1)`, height +60 m, the slope 80 m wide
   - plateau: centre `normalize(-1, -0.2, -0.4)`, radius 700 m, +110 m with a flat top, edge falloff 120 m
   - sea level: the 30th area-weighted percentile of the macro elevation, so land is about 70 % by construction (report the measured value)
2. **Crust**, per chunk vertex: macro elevation plus three bands with research wavelengths, kept inside about ±150 m broad relief plus the stamps: region (λ about 2.5 km, frequency 0.0004, 60 m, FBM 3), face (λ about 800 m, 0.00125, 40 m, ridged 4), foot (λ about 150 m, 0.0067, 8 m, FBM 4), plus spike 6's domain-warped band. Biome row per vertex from the fields, written as vertex colour.
3. **Dressing**, finest LOD chunks only, as `MultiMeshInstance3D` per chunk and kind, instance up axis = radial direction:
   - canopy: jittered grid, spacing about 10 m, forest-mask noise with a threshold so forests have edges (dense inside, nothing outside), slope under 25°, above sea level, outside site clear radii, biome rows 0 and 1 only
   - rocks: spacing about 6 m, wider mask including forest edges, slope under 45°, all rows, denser in row 2
   - placeholder meshes built in code (simple cone plus cylinder tree, low-poly rock), original, tinted per biome, scale jitter from the hash
4. **Sites**: global Poisson on the sphere, minimum separation 600 m, budget 24, land only, one distinct marker mesh each (a tall simple pillar), clear radius 15 m. What a site is stays a design question; the marker only shows spacing.
5. **Shading**: keep the simple look. Blend by slope and height (flat tint, steep rock colour, a high-altitude cap), tinted by the biome vertex colour. Change `terrain.gdshader` minimally.
6. **Water**: one sphere at the sea radius, flat colour, optionally slightly transparent, no collision.

## Integration into the planet scene

- `terrain.gd` builds chunks through `PlanetGen.build_chunk` instead of its own noise; keep the quadtree, the job model, skirts and uploads. Chunk bounds must use the real relief (stamps included), not the old `height_amplitude`.
- `collision_ring.gd` gets its heights from `PlanetGen.heights`.
- `main.gd`: each planet body gets its own `PlanetGen` with its own seed; the planet the player starts on uses `recipe.json`.
- Debug: an overlay line (F3 overlay) with biome id, height above sea, slope and macro fields under the player, from `sample()`. One key for an orbit camera (about 15 km out, looking at the planet, mouse to rotate). Pick free keys (taken: 1-5, A, D, E, F, F3, F12, H, L, Q, S, V, W, X, Space, Shift, Ctrl, Esc) and add them to `spikes/planet/README.md`.
- Command-line options for scripted runs, like the existing `--auto-test`: `--planet-walk` (test T5) and `--planet-shots` (screenshots). Scripted runs never capture the mouse.

## Milestones (in order; each ends on its completion criterion, commit after each)

1. **Core in Rust.** `planet_core` with recipe, bake, stamps, sea level, height function, chunk build, scatter, sites, statistics. Done when `cargo test` passes T1 (core part), T2 and T3, and the statistics print.
2. **In the scene.** `PlanetGen` extension, terrain and collision ring switched over, water sphere. Done when T1 (scene part) passes and the existing `--auto-test` run exits 0 with zero rescues.
3. **Dressing and sites.** MultiMesh scatter, site markers, shading. Done when T4 and T6 report numbers.
4. **Walk and look.** Debug overlay, orbit camera, `--planet-walk`, `--planet-shots`. Done when T5 reports and the screenshots exist (or the report says why they could not be made).
5. **Report.** `spikes/planet_gen/REPORT.md` filled in (see Deliverables). Done when every test has a number or a stated reason.

## Tests (headless, all numbers into the report)

- **T1 one height function**: for 1000 random directions and for every vertex of 50 random chunks at several depths, `|mesh height - height_at| < 1 mm`; collision patch heights against `height_at` likewise.
- **T2 no seams**: neighbouring chunks at the same depth, including across all twelve cube edges, share border vertex positions within 1 mm (skirts excluded).
- **T3 macro statistics** from `bake()`: land fraction, area share per biome row, min/max height above sea, the basin's below-sea extent along two great circles through its centre (target > 1 km), plateau and escarpment heights as built, bake time.
- **T4 sites**: count placed, smallest pairwise distance (must be ≥ 600 m), mean and largest nearest-neighbour distance (the largest says how long the longest hike to the next site is).
- **T5 walk**: a scripted walker on the ground at 1.8 m/s for 5 minutes of simulated time along a fixed great circle (fixed fps, `--fixed-fps 60`), plus three more starts. Log biome id and height every metre. Report: number of biome changes, shortest stretch inside one biome (if biomes flip every few steps the region wavelength is too short), height range, whether it reached water. Walking must not fall through the ground (count frames where only the safety net held the body).
- **T6 cost**: bake time, chunk build time per depth (mean, P95, max; with and without scatter), scatter instances per fine chunk (mean, max), sites; CPU time only.
- **T7 regressions**: `--auto-test` (walk, board, fly, cabin) exits 0 with zero rescues; the spike-7 build script still builds Linux and Windows.
- **Screenshots** if a Vulkan device exists (lavapipe is fine, slow is fine): orbit view from four sides, the basin shore, the escarpment, the plateau edge, one forest edge, one site marker. Save as PNG under `spikes/planet_gen/shots/` (small, at most 12 files).

## Judged by the initiator (locally, after the cloud run; do not claim these)

From the research note: from orbit you can point at the basin, the rim and the plateau as different regions; on foot the ground curves and a small rise hides what is beyond; five minutes of walking changes the surface character or enters a new region; forests have an edge and trees stand out along the radius; sites are far enough apart that each is an arrival and close enough that a short hike finds the next; the fine mesh holds a steady frame rate with the canopy instanced (measured locally). If the orbit view is a potato, lower the region amplitude; if the ground feels like one noise, the stamps are too weak or too wide; if biomes flip every few steps, the region wavelength is too short.

## Out of scope

Rivers, erosion, caves, voxels, grass cards, atmosphere scattering, real site scenes, more than four biome rows, fiction names, LOD redesign, networking changes, other planets' content beyond giving each its own `PlanetGen`.

## Rules for this session

- Branch `spike/planet-gen` only (it starts from `spike/combined`, the state of all spikes so far; the initiator fast-forwards `spike/combined` after review). Commit small and often, one commit per working step and one for each notable bug state, message says what it shows. Push the branch; never push to `main`, never create tags (the initiator tags the frozen state).
- Run `git branch --show-current` right before every commit.
- Dependencies: `godot` (godot-rust 0.5.5), `fastnoise-lite` 1.1.1, `serde`, `serde_json` are approved. Ask (stop and write the question into the report) before adding anything else.
- When a design gap appears that this brief does not cover: take the research value or the simplest choice, mark it as **assumption** in code and report, and continue. Never invent names, lore or mechanics.
- Keep `project.godot` unchanged unless a step requires it; if it changes, say why in the report.
- No personal data, no real names.

## Deliverables

- Code on `spike/planet-gen`, building with `cargo build --profile fast` and the spike-7 script.
- `spikes/planet_gen/REPORT.md`: what was built, every test with its numbers (tagged measured, calculated or assumed), the list of assumptions made, problems and how they were solved, what the initiator should look at first, open questions. Add a short "learnings" section (what, why, how to apply) for the concept repo's `LEARNINGS.md`; the initiator's session copies it over.
- Screenshots as above, if possible.
