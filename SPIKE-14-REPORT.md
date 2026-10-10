# Spike 14 report: seed-based planet terrain at 30 / 100 / 300 km

Branch `spike/planet-seed-lod` (code repo), issue #177, brief `docs/SPIKE-14-BRIEF.md` (concept repo). Run on the NAS, headless, no GPU. Nothing here is merged; the initiator flies it on the desktop and decides the numbers.

**Reading the timings:** the NAS shares its CPU (load average 5-12 on 8 cores) and is not for performance tests (`WORKSPACE.md`). Bake and chunk times are release builds, one run each, and indicative only. Frame time, pop-in and screenshots cannot be measured here (open list below).

## The seven steps

| # | Step | Status |
|---|------|--------|
| 1 | Radius and atmosphere as parameters end to end | done |
| 2 | Coarse global layer, disk cache, determinism | done |
| 3 | Rivers as polylines with width, carved per chunk without seams | done |
| 4 | Fine detail per quadtree chunk on threads, evicted when far; landforms as functions | done in `planet_core`; the thread pool and eviction are the existing ones in `exo_app/src/terrain.rs` (not changed, not run here: no window) |
| 5 | Sites and landforms by density rules in metres | done (placement slow at 300 km, see below) |
| 6 | Skirts on the chunk edges | already there; now checked by a test |
| 7 | Headless measurements at 30, 100, 300 km, 1000 km dry run | done (table below) |

Tests at the pushed tip: `cargo test -p planet_core` exit 0, 60 tests passed (13 unit, 47 integration; the measurement harness is `#[ignore]`). Scenarios green: `cargo ts scenario_radius scenario_warp scenario_look scenario_site --run-ignored all` (warp at 30, 100 and 300 km, warp at 5 km, look, site).

## What was built, step by step

**1. Parameters.** `planet_core::Ceilings` (`ceilings.rs`): one rule from the radius to atmosphere height, tallest terrain, obstruction radius and arrival radius. Start values, `TODO(initiator)`: atmosphere = 1200 m at 5 km, then 0.1167 x radius (3.5 km at 30 km, 11.7 km at 100 km, 35 km at 300 km); tallest terrain = 1/10 of the atmosphere; obstruction = radius + atmosphere/3; arrival = radius + max(7 km, 2 atmospheres). At 5 km it gives Hearth's 5400 m and 12000 m exactly. `--radius=` and the new `--atmosphere=` apply it to the first planet (and the planet's terrain ceiling). Fixed 5 km assumptions fixed: camera far plane from the largest radius (2.5 radii, at least 120 km), debug orbit camera at 3 radii, the warp scenario's orbit start and its "start refused / allowed" altitudes relative to the planet's jump altitude and atmosphere. One more thing broke and was fixed: gravity ended at a fixed 6000 m, inside the atmosphere of a big planet, and a walker in the cabin does not walk outside the field (reproduced at 5 km with a high start): the field's `gravity_end_height` is now at least two atmospheres.

**2. Coarse layer** (`coarse.rs`). The five macro noise fields are no longer baked into a global image: they are functions of the position, evaluated per chunk and per query. What stays global: the sea level (percentile on the grid), the drainage's erosion cut (i16, 0.1 m) and the lakes' water level (u16, 0.1 m) per grid vertex, the river and lake lists, and the generated sites. Cache file `coarse-<hash>.bin` under `target/terrain-cache` (or `EXO_CACHE_DIR`), key = hash of recipe text, seed, radius, grid size, code version (`coarse::CODE_VERSION`) and the hand-placed places; checksum on the file, a damaged or foreign file is ignored and rebaked. Tests (`tests/coarse.rs`): layer hash and chunk hash equal with 1, 3 and all threads and across runs (chunk also built on 4 threads at once); unchanged recipe loads, changed recipe / seed / radius rebakes, each under its own key; the loaded planet gives the same sites, heights and chunks, and a cache hit places no sites; flipped byte and truncated file are ignored and repaired. Heights quantised to 0.1 m: three drainage test tolerances widened to match (`QUANT` in `tests/drainage.rs`).

**3. Rivers** (`rivers.rs`). Per drainage vertex: bed, water level, depth, half width and the vertex the water runs to. A river is the segment between two vertices; its cross-section (parabola, reach 6 half widths) is evaluated per point through a 3D hash grid, so the same function gives the same channel in every chunk. The relief finer than the coarse grid (bands with a base frequency above a quarter cell; none at 5 km) is flattened in the valley and comes back between 1 and 6 half widths; the drainage runs on the coarse ground without those bands. At a confluence the lowest cross-section wins. Tests (`tests/rivers.rs`): on a 30 km planet with a 366 m grid, 80 % of sampled river segments lie below their banks half-way between two vertices and hold water; the shared edge of two chunks differs by under 1 mm; no step above 1.5 m in 0.25 m across the channel. Lakes stay on the coarse grid (bilinear): blocky at large radius, see open list.

**4. Chunks.** Chunk build costs the same at every radius (table: about 2-3 ms per 32 x 32 chunk, 1.6-3.2 ms from root to finest level at every radius); coarse memory does not follow the planet (6.6 to 16.6 MB from 5 to 300 km; 106 MB at 1000 km, almost all of it river vertices: their count grows with the area). `exo_app/src/terrain.rs` already builds chunks on the async compute pool and discards them when far; nothing there changed.

**5. Density rules.** Landform and site kinds take `per_100_km2: [min, max]` instead of `count`; the count is the surface times the frequency (at least one when the frequency is above 0), picked from the range as before. The numbers in `hearth.json` and `cinder.json` reproduce today's counts at 5 km exactly (checked by script). Placement runs on a 3D hash grid (`grid.rs`): separation, budgets and the nearest-site score of the farthest-point heuristic (exact up to three cells, beyond that "far"); `stamp_height` and `apply_edits` read only the cells of the point, with results identical to the scan over all stamps and sites (test `the_grids_give_the_linear_scan_s_heights`). Candidate budgets grow with the wanted count. Tests (`tests/density.rs`): counts at 5, 10, 20, 30 km within 25 % of the area's share; no two sites or landforms of a kind closer than their separation at 5, 10, 20 km; bake under two minutes at 30 km (1.3 s measured). The first cut of the recipe fields came from a Haiku lane; its spatial index was not wired in and wrong at face edges, so it was replaced by `grid.rs`, and its weakened landform test was restored (only its fixture text follows the new field name).

**6. Skirts.** `build_chunk` already hangs a skirt (4 cells deep, at least 2 m) on every chunk edge. `tests/skirts.rs` compares a parent's edge with its first child's at 5, 30 and 100 km (2376 edge samples each): the largest height gap between levels is 3.0, 3.2 and 7.3 m, and the skirt of the higher chunk covers it in every sample (0 open). Geomorphing not needed by this test; pop-in is the desktop's judgement.

**Ceilings.** `Planet::ceiling_m` bends the height above 70 % of the ceiling towards it (tanh), so the tallest terrain stays below its share of the atmosphere by construction; `tests/ceiling.rs` checks 5, 30, 100, 300 km (119.8 of 120 m at 5 km, where Hearth's free relief is 348 m; 198, 294 and 167 m at the others, below 350, 1167 and 3501 m). Today's planets have no ceiling (`system.json` unchanged).

## Measurements

Release build, NAS, indicative. "Today" = `main`, 5 km: macro image of 7 f32 channels, 6 x 513² x 28 B = 44 MB (plus 2-3x temporaries during the drainage), bake with full statistics best 1.9 s, median 2.4 s (`tests/drainage.rs bake_time`, measured). Seed 1337, `hearth.json`, macro resolution 512 (grid cell = 1.57 x radius / 512).

| radius | atmosphere | coarse layer in memory | first bake (coarse + sites) | cached load | chunk build, root to finest | `height_at` | rivers | sites | landforms |
|---|---|---|---|---|---|---|---|---|---|
| 5 km (new path) | 1.2 km | 6.6 MB | 2.2 s (0.05 s sites) | 0.18 s | 3.0 to 2.0 ms | 2.0 us | 2610 | 31 | 12 |
| 30 km | 3.5 km | 7.7 MB | 4.3 s (0.9 s sites) | 0.24 s | 3.2 to 2.2 ms | 2.2 us | 14733 | 1090 | 368 |
| 100 km | 11.7 km | 7.1 MB | 13.1 s (9.1 s sites) | 0.28 s | 2.9 to 1.9 ms | 2.0 us | 8039 | 12105 | 4382 |
| 300 km | 35 km | 16.6 MB | 101 s (96.6 s sites) | 0.58 s | 2.6 to 1.6 ms | 1.7 us | 106918 | 108937 | 40752 |

- First bake without the sites: 2.1, 3.4, 4.0, 4.5 s at 5, 30, 100, 300 km. The sites are the cost: placing one takes about 0.9 ms (several height samples with gradients per candidate), and there are 109,000 at 300 km. Cached, the sites cost 0 to 48 ms. Placing them lazily per region (a jittered grid per kind, Minecraft style) would remove this; not done.
- Cached load includes the height-range pass and the sites' ground edits; it does not include loading the six root chunks.
- Today's path at 30 km (appendix C of `scale-and-early-game.md`, derived, not measured): 700 MB and 10+ s. The new path: 7.7 MB and 4.3 s cold, 0.24 s cached.
- **1000 km dry run** (coarse layer alone, no landforms or sites, cell 3068 m): 106 MB in memory and on disk, first bake 5.7 s (drainage 4.5 s), cached load 0.86 s. 1.04 million river vertices; the rivers' list (about 100 B each) is the size, the grids are 6.3 MB.
- Gravity: the walker-in-the-cabin warp scenario passes at 30, 100, 300 km after the field fix.
- **Rebuild of `planet_core`:** clean build 11.6 s (dev profile, which is optimised here), tests from clean 23 s more, after touching `planet.rs` 8 s for all test binaries. `exo_app` and its 900 MB static link are not part of it; the first `cargo ts` in a fresh target dir needs the whole Bevy build (about 2 h on this NAS under load, one rustc crash on memory with 6 jobs, fine with 4).
- Flight minutes (angle Drip Rock to Bent Spoon 0.401 rad, places are by latitude/longitude so the trip scales with the radius; speeds from `ship.json` `cruise_speed` 150 m/s, `boost_speed_forward` 350 m/s, and 785 m/s for NAV = half a circle in 20 s at 5 km, a guess from the issue's "40 s round"; the flight values come in #143):

| radius | pad to pad | at 150 m/s | at 350 m/s | at 785 m/s | median nearest site | worst hole to the next site |
|---|---|---|---|---|---|---|
| 5 km | 2.0 km | 0.2 min | 0.1 min | 0.0 min | 2.5 km | 3.3 km |
| 30 km | 12.0 km | 1.3 min | 0.6 min | 0.3 min | 2.6 km | 4.4 km |
| 100 km | 40.1 km | 4.5 min | 1.9 min | 0.9 min | 2.6 km | 3.8 km |

Density keeps the spacing between sites at 2.5 km at every radius (that is the rule's job), so "flight minutes between places of interest" come from the hand-placed places and from how far apart set pieces are, not from the site density. Whole-planet crossing (half circumference) at 350 m/s: 0.7 min at 5 km, 4.5 min at 30 km, 15 min at 100 km, 45 min at 300 km.

## Findings and assumptions (`TODO(initiator)`)

- Densities are 5 km counts divided by the area, for every kind. For the big set pieces (needle, basin, escarpment, plateau, caldera, canyon) that is wrong at 300 km: about 3,600 needles. "Few large set pieces over many small ones" means a much lower frequency for those kinds; the initiator sets the numbers (a recipe change, no code).
- Relief is absolute metres in the recipe: the tallest ground is 340-460 m above the radius at every radius. The target "highest mountains 3-5 km at 300 km" needs recipe amplitudes (or a per-planet relief scale); the ceiling only limits.
- The atmosphere/ceiling rule above is a placeholder (start values, KSP ratio as reference).
- Landform and site quotas (`min_share` of biome rows) are tuned to 5 km: at 30 km row 3 has 0.26 % against 0.5 %. In the app a miss is now a warning on stderr (not a panic) at any radius; `cargo test -p planet_core` still checks them at 5 km.
- Lakes: bilinear on the coarse grid, so at 920 m cells (300 km) they are blocky; a lake polygon or per-chunk shore would be the next step. Rivers do not run into the sea: the polyline ends at the coast vertex.
- Rivers per area: the largest catchment stays 10-30 km² at every radius (terrain wavelength is absolute), so a bigger planet has more rivers, not longer ones.
- `sites_near` (scatter) is still a scan over all sites; at 109,000 sites that is 0.1 ms per scatter cell. A grid lookup is a few lines.
- Hand-placed places keep their latitude and longitude, so Drip Rock to Bent Spoon is 40 km at 100 km radius; the `_comment` distances in `content/place/*.json` were not changed.
- Cache files live in `target/terrain-cache` (the game's own directory; the ask-first list wants a word on file access outside it, none is used). `crates/exo_app/target/` is now ignored: the first commit of this branch tracked three cache files by mistake and a follow-up commit removed them.

## Open list for the desktop (not measurable here)

1. Frame time at 5, 30, 100, 300 km, windowed, no vsync, against today.
2. Pop-in in a NAV approach from orbit down to the ground (acceptance criterion: little), at 30, 100, 300 km; if it shows, geomorphing as the fallback.
3. Screenshots from the ground, low flight and orbit at each radius (`--radius=100000 --atmosphere=11670`).
4. Flying Drip Rock to Bent Spoon and between sites at SCM and NAV; the real flight values come in #143.
5. The look of the carved rivers at 30 km and above (a valley flattened to the coarse ground, relief returning at 6 half widths), lake edges, and whether the sea coast and shore are fine at 920 m cells.
6. The numbers: radius, atmosphere, ceilings, densities of the set pieces.

## Recommended radius and atmosphere

Fly **100 km first** (atmosphere 11.7 km, terrain ceiling 1.17 km, obstruction 103.9 km, arrival 123.4 km): cold bake 13 s, cached 0.3 s, 12,000 sites that keep today's 2.5 km spacing, Drip Rock to Bent Spoon 4.5 min at cruise and 1.9 min at boost, a crossing of 15 min at 350 m/s, gravity field to 23 km. 30 km is the safe fallback (12 km first job, 4.3 s bake). 300 km only after the site placement is made lazy (101 s cold now) and the set-piece densities and the relief are decided: 300 km works in every other respect (coarse layer 16.6 MB, cached load 0.6 s, same chunk cost, the 300 km warp scenario passes).
