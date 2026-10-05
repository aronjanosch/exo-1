# Procedural generation for a 6 km planet

Research note for the terrain spike. This is not a proposal, not an approved design, and not a decision about fiction, balance, or biome names. Those stay with people.

`docs/VISION.md` is not in this repo (the README points at `exo-1-concept`, which was not available here). Where this note assumes something the vision might contradict, it is marked as an assumption.

Nothing below is a mechanic, asset, or name to copy. Other games are evidence about scale, pipeline order, and which knobs matter. The shell pipeline at the end is an original assembly of public techniques: noise on a sphere, a finite graph, chunked meshes, and scatter rules stored as data.

## 1. What 6 km actually is

Radius `R = 6000 m`.

| Quantity | Value | What it means on foot |
| --- | --- | --- |
| Circumference | 37.7 km | One loop of the planet |
| Surface area | 452 km² | About one and a half Valheim discs, wrapped |
| Equator to a pole | 9.4 km | The entire climate gradient, if latitude is the climate |
| Farthest two points (along the surface) | 18.8 km | Half a loop |

Travel at ordinary speeds, no vehicle boost assumed:

| Pace | Speed | Full loop | Equator to pole |
| --- | --- | --- | --- |
| Stroll | 1.4 m/s | ~7.5 h | ~1.9 h |
| Walk | 1.8 m/s | ~5.8 h | ~1.5 h |
| Jog | 4 m/s | ~2.6 h | ~39 min |
| Rover-like | 12 m/s | ~52 min | ~13 min |

The planet is one afternoon on foot and one short trip in a vehicle. It is a complete place with a memory, in the same family as a large Terraria world or a Valheim seed, and far smaller than a continent.

The horizon is the number that should drive every wavelength. Distance along the ground to the horizon for an eye at height `h` is `R * acos(R / (R + h))`.

| Eye height | Horizon |
| --- | --- |
| 1.7 m (standing) | 143 m |
| 10 m (roof, small rise) | 346 m |
| 30 m | 599 m |
| 50 m | 772 m |
| 100 m | 1.1 km |
| 200 m | 1.5 km |
| 400 m | 2.1 km |

Standing on the surface, the visible world is a disc about 300 m across. A hill you can walk onto is what reveals the next place. From orbit the whole marble is one picture, so the same seed has to read at both distances.

Relief relative to the radius is the other constraint. A 400 m peak is 6.7% of `R`. Everest is about 0.14% of Earth's radius. Game relief that feels right underfoot can make the planet look like a potato from orbit. The spike should try a modest envelope first (see assumptions) and judge both cameras.

**Assumption:** the spike is a world you walk on and also look at from above. If the planet is only a skybox for a ship, this note is the wrong shape.

## 2. Why a normal generator will not survive the shrink

These are the failure modes that show up when a generator tuned for flat, huge, or hand-built worlds is dropped onto this sphere.

1. **Long landforms never become a place.** A mountain belt tens of kilometres long (the thing terrain mods chase) is a third of this planet. You never travel "along the range." You live on it, or you loop it. Macro shape should be a handful of basins, rims, and plateaus, each about 1.5–4 km across.

2. **Gentle hills disappear.** A 2 km wave that is only 10 m tall is a slope under your boots. You cannot see it as a hill, because the hilltop is over the horizon. Landforms have to be **faces**: scarps, rims, mesas, crater bowls, spires, canyon walls. A 40 m cliff is a wall. A 15 m dome is a feeling in the legs.

3. **Latitude stripes become a beach ball.** Equator to pole is 9.4 km. A climate painted only from latitude is a stack of rings. From orbit that reads as a painted ball. On the ground you walk out of a ring without ever seeing it as a region. Latitude can be a bias (high ground and poles a little colder). It should not be the biome.

4. **2D maps pinch at the poles.** A heightmap in longitude/latitude crowds vertices at the poles and seams the date line. Sample a 3D point on the sphere (or a cube face projected onto the sphere). The same rule applies to moisture, temperature noise, and forest masks.

5. **Infinite-chunk logic cannot spend a budget.** A generator that only looks at the local column can place "a village every N chunks" and still produce 70 of them, or one. This planet fits in memory. It can promise "about one big basin," "this many unique sites," "forests in wet clumps." That is the main advantage of the size, and the reason to run a global pass before any chunk exists.

6. **Whole-planet mesh resolution will not fit.** At a 2 m walk grid the sphere is on the order of 3×10⁸ quads. An icosphere has to be split by distance:
   - subdivision 6 ≈ 41k vertices, ~105 m spacing (good for rivers and regions)
   - subdivision 8 ≈ 655k vertices, ~26 m (still coarse for feet)
   - subdivision 9 ≈ 2.6M vertices, ~13 m (heavy, and still not a footpath)
   Near the player the mesh wants ~2 m. Far away, and on the back of the planet as seen from orbit, tens to hundreds of metres is enough. Level of detail is part of the spike, not a later optimization.

7. **Earth erosion at Earth wavelengths makes mush.** Hydraulic erosion is a good way to turn noise into terrain, and it is affordable once, on a coarse shell, because the world is finite. Running a continental erosion model, or stacking octaves until the ground looks like satellite photos, spends the silhouette on detail nobody can see from 143 m away.

8. **Trees that ignore the radial "up" fall over.** On this sphere, up is the direction away from the centre. A foliage pass that uses world-Y plants a forest that leans.

9. **View-distance foliage from flat games is wasted here.** Instancing still matters. Impostor forests out to 2 km matter much less, because the ground itself has already sunk behind the curve. Spend foliage cost inside a few hundred metres.

## 3. How big this is next to other games

Figures are published numbers where a wiki or a developer gives them, and labeled estimates where a player measured.

| World | Scale | What their generator is doing | What transfers |
| --- | --- | --- | --- |
| Outer Wilds, Timber Hearth | ~500 m diameter (community measurements; developer diary describes hand-built modular rock, not a noise planet) | Authored paths, landmarks, hard silhouettes | Landmarks and readable floors/walls. The surface is ~0.8 km², about 1/570 of ours, so the method does not scale up as "model it all by hand" |
| Subnautica crater | Wiki pages disagree between ~2 km and ~3 km across; a few km² of play | Fixed biome volumes, hand placed, with scatter inside them | A biome is a volume plus a prop table. Our surface is on the order of 100× that play area, so the volumes can be authored only at macro scale |
| Starbound planets | Width and height are data (`planetSizes`), small examples in the modding docs are a couple of thousand tiles; the surface wraps | One planet type picks a size, a terrain style, a biome list, detached enclaves, and a dungeon count | The data shape: size, terrain recipe, enclave count, site budget. All of that is JSON, which matches "content is data" |
| Terraria, large | 8400×2400 tiles, documented as 16800×4800 feet (~5.1×1.5 km) | Ordered passes: terrain, caves, biome paints, then quotas (hives, cabins, forges) | Finite passes and quotas. The tile simulation itself is the wrong runtime |
| Don't Starve | Finite island per seed | Biome areas, set pieces with rarity, paths that connect them | A budget of special sites, plus connectivity, on top of a cheap ground |
| Valheim | Disc of radius 10 km, area ~314 km² (ours is 452 km²) | Height from layered noise; biome from ordered tests on distance-from-center, altitude, and noise. World split into 64 m zones. One location per zone. Vegetation and clutter test biome, altitude, slope, and a forest fractal (0 at a forest's centre, 1 at its edge) | Zone size, forest mask, placement filters, "one special thing per cell." The biome *rings* depend on a flat world you can see across. Here they would be invisible at eye level and striped from orbit |
| Astroneer planets | Five planets share one size; moons are half the diameter (official wiki). A fan celestial-body lists Sylva at radius 8363 m; that figure is unofficial. A 2019 pole-to-pole-and-back walk of Sylva is timestamped around 48 minutes | Voxel shell, regions (biomes) rolled per save, interior layers by depth, resources and flora scattered per region, terrain you can dig | Region = palette + props + resources. Digging is a different runtime (see the fork in section 7). Walk time is the same order of magnitude as a loop of our planet |
| Space Engineers Moon | 19 km diameter, 9.5 km radius. Earth-like planet is 120 km diameter | Cube-face height map and biome map, plus procedural ore. Voxels, destructible. A 2048² map on a 120 km planet is about 20 m per texel (modding tutorial) | Painted or baked cube faces are a proven macro store. On our area, a 1024² image per cube face is on the order of 10 m per texel, 2048² closer to 4 m. Enough for a baked macro height and biome, not for the walk mesh |
| No Man's Sky | Player estimates vary: a 2016 half-planet walk was turned into ~38 km diameter; later coordinate math has been read as ~64 km radius, and cartography tools list circumferences of a few hundred game units by planet class. Treat all of these as estimates | GDC 2017 (Innes McKendrick): voxel density in cube space, evaluated as a sphere; a height field plus 3D noise; density faded with height so the field stays a landscape instead of floating blobs; then polygonize, shade, populate. Flora: the biome constrains rules; trunks share a few rigs; branches and leaves vary; each specimen has slope and altitude preferences; three storeys (ground cover, shrubs, trees) | Pipeline order, and the three storeys. Their planets are large enough that players describe them as empty. We can afford a global landmark budget they generate locally |
| Vintage Story | Configurable, default climate bands repeat over ~100k blocks; not a sphere | No fixed biome ids as the source of truth. Noise maps for climate, forest, bushes, flowers, beach, geologic province, landform. Landforms are data (octave thresholds and height keys). Rock is layered strata. Soil, snow, and plants come after the shape. Trees depend on climate and a forest density. "Realistic" latitude climate or "patchy" climate | This is the best reference for the *fields*: shape, then rock, then surface, then plants. Patchy climate fits our equator-to-pole distance better than a realistic latitude ramp. Strata make cliffs readable, which we need |
| Dwarf Fortress | Rectangular world, advanced gen exposes the knobs | Seed elevation, rainfall, temperature, drainage, volcanism, wildness. Reject and retry if biome quotas fail. Smooth, erode, run rivers, then rain shadows, then vegetation. Biome falls out of elevation + drainage + rainfall, with temperature picking variants | Quotas ("this world must contain these region types"), drainage as its own field, rain after the mountains exist. The history simulation is far outside a terrain spike |
| Minecraft 1.18+ | Effectively infinite columns, 16×16 chunks | See section 4 | The parameter vector, the spline from parameters to shape, and the idea that decoration is a later pass with filters |
| Minecraft terrain mods | Same infinite world, landforms at "thousands of blocks" | See section 4 | The landform vocabulary (mesa, caldera, canyon, plateau, sink). The wavelengths do not transfer |
| Spore planets | Small enough to walk around quickly; authored by painting | Simple height and climate, procedural plants on top | A planet can be a palette plus a few paints. Useful as a warning that low-frequency shape plus props already reads as a planet, and as a reminder that we are not painting by hand for 452 km² |

Kerbal-scale planets and the very large planetary games (hundreds to thousands of kilometres) solve streaming and precision. They do not solve "what is a place when the horizon is 143 m."

## 4. Minecraft, because the biome question always starts there

Source for the vanilla pipeline: the Minecraft Wiki page *World generation* (Java Edition, the 1.18 multi-noise model).

### 4.1 The vanilla pipeline

Generation of a chunk is a list of stages. The ones that matter here:

1. **Structure starts and references**, before terrain exists.
2. **Biomes**, from climate parameters, still before the surface exists.
3. **Noise**, the solid/air shape, including 3D density so overhangs are possible.
4. **Surface**, replace the top with materials chosen by biome and local conditions.
5. **Carvers**, tunnels and ravines cut through the solid.
6. **Features**, in a fixed decoration order. Vegetation is late (`vegetal_decoration`): trees, cacti, kelp, ground cover. Ore and small "local modifications" happen earlier. A final step can freeze the top in cold places.
7. **Spawn.**

Two facts are easy to miss:

- **Shape and biome share noises, and splines sit between them.** Continentalness, erosion, and weirdness drive both "which biome" and "how tall and how flat." Splines map those noises to a height offset, a vertical stretch, and a jaggedness. High continentalness lifts the average height. High erosion flattens. Weirdness is folded into a peaks-and-valleys signal (`1 − |(3|weirdness|) − 2|` on the wiki) so ridges and rivers prefer valleys. A mountain biome and a mountain shape co-occur because they read the same fields, not because someone painted a biome mask and then hoped the mesh matched.
- **A biome is a row in a parameter table.** Overworld biomes are intervals in temperature, humidity, continentalness, erosion, weirdness, and depth. Temperature and humidity pick the life. Continentalness picks ocean, coast, or inland. Erosion picks flat versus mountainous. Weirdness picks variants and shattered ground. Depth picks surface versus cave biomes. If a point hits no interval, the nearest row wins. The Nether version is even cleaner: each biome is a point in a small space, and nearness decides.

Decoration rules are per feature: how many tries, where in the chunk, which biome, which block is under it. Structures use spacing, a minimum separation, and a frequency, then give up if the biome is wrong. Strongholds are the exception that uses a global pattern (rings), which is the finite-world instinct showing through.

Feature order is not perfectly stable: a feature may spill into a chunk that already finished, so two walks of the same seed can differ slightly (the wiki notes this). On a finite planet we can do better by placing unique sites in the global pass, before chunks, so load order cannot move them.

### 4.2 What vanilla is tuned for

A chunk is 16 blocks. Players see on the order of a few hundred metres across flat ground. Hill spacing, tree spacing, and "a structure every several hundred blocks" all assume that. Our eye-level view is ~143 m, and our loop is 37.7 km. Using those frequencies unchanged produces either one endless slope or a carpet of noise. The **shape of the system** transfers. The **numbers** have to be retuned on the sphere and then looked at.

### 4.3 Mods worth stealing ideas from

These mods keep Minecraft's infinite columns. Their scale is the part to leave behind.

| Mod / pack | What it changes | Idea that survives at 6 km |
| --- | --- | --- |
| TerraForged, and the later ReTerraForged / UltraTerraForged line | Replaces the shape: continents, rivers that follow the land, coasts, configurable landforms (buttes, canyons, fjords, karst, deltas, volcanic coasts). Biomes come from whoever else is installed | Landform first, biome second. Rivers that know the height field. A menu of landform types with on/off switches, stored as data |
| Tectonic (and Terratonic, the Terralith pairing) | Continent-scale masses, ranges that run for thousands to tens of thousands of blocks, plateaus, valleys, canyons, underground rivers where the range is too tall for a surface river | The landform list (range, plateau, valley, canyon, caldera on islands). One Tectonic "range" can be longer than our circumference, so a range here is a hike of a few peaks, not a belt |
| Terralith | A large biome palette, structures, a few spectacle landforms (volcanoes, arches, floating islands), built mostly from the base block set | A biome can be a new silhouette and a new surface using a small material kit. Dozens of biomes are a content project. The spike wants a few strong ones |
| Stardust Labs Continents (often paired with Terralith) | Separates landmasses with large oceans | An ocean you cannot see across is a real place. One basin can do that job. A world of ocean does not, because the horizon is already empty at sea level |
| Dynamic Trees | Trees are a species: growth rules, leaf type, soil, energy. Placement uses a Poisson-disc / stippling pass. Disc radius comes from a density noise, and the biome scales that density. Forests clump; plains stay open | Species as data. Density as a field, not a constant "trees per chunk." Poisson spacing so canopies do not stack into a solid. Growth simulation can wait; the placement model is the part to take |
| Biomes O' Plenty, Oh The Biomes You'll Go, Regions Unexplored, William Wythers' Overhauled Overworld | More biomes, more plants, retuned spacing and structure mix | A biome is a content palette (ground, plants, props, a site type). Variety past a handful of palettes is authoring, not a better noise |
| Geophilic and similar "environmental" packs | Litter, puddles, fallen pieces, small ground props | A second scatter pass, sparse, sells a ground that the height field alone will not |
| Cave-biome and structure packs (Alex's Caves, YUNG's set, and the vanilla lush/dripstone/deep-dark rows) | Destinations underground, tied to the same climate parameters via the depth axis | Interiors are a later pass with their own budget. Vanilla already shows the pattern: cave biomes are rows in the same table, selected when depth is high |

Vanilla cave noises (cheese pockets, spaghetti tunnels, noodle tunnels) are a fine way to fill a thick crust. They do not guarantee you can walk through them. A room-and-corridor graph does. Deep Rock Galactic is the reference for that second approach: generate a graph of rooms that fit together, then carve. That is a cave spike, not this one.

## 5. Foliage, as its own system

Across Minecraft, Valheim, Vintage Story, No Man's Sky, and the environmental mods, foliage is never the height field. It is a pass that runs after the surface exists and can say no.

The rules that show up over and over:

1. **A density field**, multiplied by the biome. Dynamic Trees samples a noise for how tight the Poisson discs are, then scales by the biome's tree amount. Valheim uses a forest fractal with an inside and an edge. Vintage Story has an explicit forest map and a global forestation offset. No Man's Sky lets the biome set density and also how plants sit on the edge of ground patches.

2. **Filters, all of them cheap.** Biome, altitude band, slope limit, "not underwater," "not inside a site's clear radius," sometimes "this soil only." Minecraft's feature placement is this list. Valheim's vegetation entries are this list. Starbound puts the plant choices on the biome object.

3. **Clumps with edges.** A forest that is a smooth global density looks like stubble. A mask that is solid in the middle and empty outside gives you a wood you can enter. Valheim's forest threshold is the clearest description: 0 at the centre, 1 at the edge, above 1 means outside.

4. **Three storeys, different radii.** No Man's Sky's own description splits flora into ground cover, shrubs, and trees, with ground cover often a different, simpler generator. On our horizon:
   - Ground cover only needs to exist inside ~40 m. Cards or a shader pattern on the terrain. Past that it is sub-pixel.
   - Shrubs and rocks matter out to the standing horizon and a bit beyond (~150–250 m).
   - Trees matter further when you are elevated, because a crown pokes over the curve. They are the skyline.

5. **A few meshes, many instances.** No Man's Sky shares trunk rigs and varies branches, leaves, and color. For a simple look and a spike, three to six authored meshes, recolored and rescaled per biome, will read faster than a grammar that builds a new plant per specimen. Species-as-data can still store scale, tint, lean, and which mesh, so a grammar can be added later without changing the scatter.

6. **Rare beats common.** Dynamic Trees' extra gen features (alt leaves, fallen leaves, fruit, vines) are a second roll on an already placed tree. The environmental mods do the same on the ground. One rare mesh in a thousand spots is a landmark. A new mesh on every tile is noise.

7. **Clear the ground under a site.** Valheim locations and Minecraft structures both suppress the ordinary scatter inside a radius. If the global pass places sites first, chunks can honor that radius and load order stops eating them.

Godot already has the runtime this wants:

- `FastNoiseLite` samples in 3D, has fractal types (including ridged) and domain warp. It runs on the CPU, which is what we want for a single function that both the mesh and the scatter can call.
- `MultiMeshInstance3D` is the scatter. Each instance's up axis is the radial direction (`position.normalized()`), not world Y. Per-instance custom data can carry tint, scale jitter, and a wind phase.
- Because the standing horizon is ~143 m, a spike can instance real meshes inside the loaded chunks and skip impostors until a profile says otherwise.

Starting numbers to **test**, not to keep (assumption):

| Storey | Spacing inside a clump | Where |
| --- | --- | --- |
| Canopy | 8–12 m | Forest mask above a threshold, slope under ~25°, not in a site radius |
| Shrubs / medium rocks | 4–8 m | Wider mask than the canopy, including mask edges |
| Ground cards | 1–2 m | Only within ~40 m of the camera |
| Solitary large rock or spire | Poisson, 80–200 m minimum | Slope and biome filters, these are the navigation silhouettes |

Outside the forest mask, canopy density should fall to nearly nothing. Plains that are "a tree every 30 m forever" read as a failed forest.

## 6. Sphere-native map techniques

Two public writeups are the right ancestors for the macro pass. Both are finite-world thinking, which matches us better than chunk streaming.

**Mapgen4** (Red Blob Games, 2018/2023) builds a mesh of regions, assigns elevation (noise, plus optional painted constraints and distance fields), blows a prevailing wind across the regions, drops moisture with a rain shadow, and accumulates flow downhill into rivers. The river code is on the graph, so it does not care that the dual mesh started as a square. Biomes fall out of elevation and moisture. The author notes that about 25k cells is the pretty operating point, and that the code can go past a million.

**The 2018 sphere experiment** (same author, ProcJam, `1843-planet-generation`) ports that idea:

- Evenly distributed points on a sphere looked boring; jitter helped.
- 3D simplex elevation was "reasonable" and not yet interesting.
- A quick tectonic-plate model (about 10–50 plates, random region growth, mountains where plates push together) can place ranges, and the author was still unhappy with it. A 2022 note says the plate code was buggy and the rest of the parameters had been tuned to the bug.
- Rivers on the sphere needed no change to the graph algorithm. Rendering them did.
- Temperature-by-latitude and trees were left undone.

The lesson for us: a graph on the sphere is enough to run rainfall and rivers, plate tectonics is optional and easy to get wrong, and pure noise is the thing he moved on from because it was not interesting. At our size, 10–50 plates is also the wrong resolution of *authorship*. A plate here would be a huge fraction of the planet. Forcing a small number of macro features (one basin, one rim, one high plateau) and letting noise fill inside them gives a planet you can learn. Plates can be a later experiment if the forced-feature version looks too designed.

Practical macro resolution: an icosphere at subdivision 6 is about 41k vertices and 105 m apart on this planet. That is in the same range as mapgen4's pretty mesh, and it is a reasonable river graph. Subdivision 5 (about 10k vertices, 210 m) is enough for a first biome map.

Wind on a sphere does not have a global "left." A zonal flow (around the axis, flipping or fading with latitude) is the simple version. The spike can skip wind and use a moisture noise. The graph should still store a downslope and a flow slot so rivers do not require a new data model later.

## 7. Systems that could run the spike

| System | Walk + orbit | Global budgets (one basin, N sites) | Foliage | Digging, caves | Fits GDScript + FastNoiseLite |
| --- | --- | --- | --- | --- | --- |
| Shader noise on a cube-sphere, no stored height | Orbit is easy; gameplay has to reimplement the shader | No | Hard, unless the CPU repeats the shader | No | The mesh half is proven in GDScript (see below). Two copies of the noise will drift |
| CPU chunked cube-sphere, 3D noise only | Yes | Weak. You can bias the noise, you cannot promise a quota | Yes | No | Yes |
| **Macro graph + chunked cube-sphere + scatter** | Yes | Yes | Yes | Surface only | Yes |
| Thin voxel shell (transvoxel / marching cubes) | Yes | Yes, if a graph still runs first | Yes | Yes | A maintained voxel library is a GDExtension. The hard rules say to ask before taking that dependency |
| Hand-placed tiles wrapped on a sphere | Highest landmark control | Yes | Authored | Authored | Possible, and too much content for 452 km² before anyone has walked a test |
| Landing-tile planets (a patch appears when you set down) | Breaks the loop | Local only | Yes | Usually no | Solves a different game |

There is already public GDScript for the mesh half: cube projected to a sphere, quadtree chunks, split near the camera, horizon culling, shader displacement (the Cuberact "Planet Chunked LOD" learning project, Godot 4.6). An older Godot addon (Hoimar Planet-Generator) has the same shape and left vegetation as future work. Those are existence proofs that the renderer baseline can draw a sphere with level of detail. They are not code to import. Their shader-owns-the-height choice is the row we should not ship, because scatter, collision, and sites all need to ask "what is the ground here?" and get the same answer as the mesh.

### The fork

**Assumption for everything that follows:** the planet is a solid shell you walk on. You do not dig it into a new shape, and caves are not in the spike.

If digging or building with terrain is core, the spike should be a voxel shell and this note's mesh stage is the wrong one. That choice also pulls in the GDExtension question, which needs an explicit yes. Astroneer and Space Engineers are the references for that fork. Say so before any code.

### Recommendation

Use three stages, in this order, one seed, one function for the ground:

1. **Macro shell.** Once per seed. A jittered icosphere (start around subdivision 5 or 6). Fields: elevation, a sea radius, temperature (weak latitude bias + noise + colder with height), moisture (noise in the spike; wind and rain shadow as a reserved slot), a landform id, a biome row, downslope, and flow (unused until rivers). After the noises, stamp a few forced features so the planet cannot come out as undifferentiated noise: one broad basin, one rim or escarpment, one high plateau. Quotas can reject and retry a seed that failed to produce coast, or a minimum area per biome row. This is the Dwarf Fortress / Terraria instinct, on a sphere, and it is cheap at 10k–40k vertices.

2. **Crust mesh.** Cube-sphere quadtree. Finest chunks around 2 m and about 64 m across (32×32 quads). Load a radius that covers the horizon from a small hill, on the order of 600–800 m. Coarser chunks outward, including the back side when the camera is in orbit. Each vertex samples the macro fields (barycentric on the icosphere, or a baked cube-face image) and adds a short-wavelength noise for walkable roughness. Collision comes from the same mesh, only on the fine chunks near the body. A chunk is deterministic from seed + chunk id, so it can be dropped and rebuilt.

3. **Dressing.** After the fine chunk's surface exists: honor site clear-radii, then canopy, then shrubs and rocks, as MultiMeshes. Unique sites themselves are placed in stage 1 (Poisson on the sphere, minimum separation, biome filter, a budget) and only *instanced* when their chunk loads. The spike can instance a single distinct marker mesh so spacing can be judged before anyone designs a site.

Shared climate parameters are why stage 1 and stage 2 must use the same fields. That is the Minecraft spline lesson, reduced to a table small enough to read. A biome row is something like: temperature band, moisture band, elevation band, landform, surface tint, scatter entries, which site types are allowed. Four rows are enough for the spike. Adding a fifth should be data.

Wavelengths to start from (assumption, the spike exists to falsify these):

| Band | Wavelength | Height | Role |
| --- | --- | --- | --- |
| Basin / landmass | 4–10 km | decides water | One major basin plus lesser lows. Target about 70% land so a walk keeps finding ground, with the basin wide enough (> ~1 km) that the far shore is over the horizon |
| Region | 1.5–4 km | 40–150 m | Plateaus and broad highs. This is the thing you point at from orbit |
| Face | 0.4–1.5 km | 30–120 m local | Rims, scarps, bowls. This is the thing you walk toward |
| Foot | 80–250 m | 4–20 m | The ground you feel |
| Grain | 2–20 m | shading and tiny normal noise | Not a mesh feature |

Keep the orbital silhouette inside roughly ±150 m of the mean radius on the first try, and put the drama into faces. A 400 m spike will tell us immediately if the potato problem is real; it can be a single stamped feature rather than the global amplitude.

Surface shading stays in the simple-look budget: triplanar (cliffs and a sphere both need it), a blend by slope and height (flat tint, steep rock, a high-altitude cap), biome tint as a vertex color or a uniform. Colored bands on cliffs, Vintage Story's strata idea with invented materials, can wait one step past the spike. They are the cheapest way to make a face readable.

### What this leaves out on purpose

- **Rivers.** Stage 1 already has downslope and flow, copied from the mapgen4 graph approach, which the sphere experiment showed does not care about the square. Carving them into the crust is the first follow-up if the shell is fun to walk. Doing it inside the spike muddies the scale test.
- **Erosion.** A few iterations on the macro graph, before chunks, is the follow-up after rivers. Not a runtime effect.
- **Caves and digging.** A height shell cannot grow a cave without a second representation. If interiors become real, prefer a budgeted room graph in a crust a few dozen metres thick over unbounded 3D noise, and reopen the voxel question then.
- **Grass cards, wind, atmosphere scattering, multiple planets, interiors of sites.** Each is a later slice. The marker meshes are there so site spacing is visible without building the sites.

### Data, so it can live in a schema

A planet recipe is data: seed, radius, noise stack (type, frequency, amplitude, domain warp), the forced-feature list, sea level, the biome table, scatter rules, site budget and minimum separation. A scatter rule is data: mesh id, storey, density, slope max, altitude min/max, forest-mask range, clear-radius honor. No code in the content. No downloads. The generator is engine code; the recipe is content.

Chunk id and site id should be stable functions of the seed, so a future agent interface can ask "biome at this point," "height at this point," and "sites within this radius" without walking the mesh. That is the hook the playable-by-CI rule will need. The spike does not have to expose a full network protocol. It should not paint itself into a corner where those queries require screenshots.

## 8. The spike itself

One scene, one seed, two cameras (feet and orbit). Godot 4.7, Forward+, Vulkan, GDScript, `FastNoiseLite`, `MultiMeshInstance3D`. No addon, no GDExtension, no change to project settings unless a later task asks for that explicitly.

**In**

- Cube-sphere chunks with level of detail, radial up, no polar pinch, no seam at a cube edge.
- Macro graph with the fields in section 7, four biome rows, three stamped macro features.
- Material blend by slope and height, tinted by biome row.
- One foliage storey (canopy) plus a medium rock scatter, both filtered, forest mask on, radial up.
- A handful of site markers from a global Poisson budget.
- The same height function for the mesh and for a debug query (print biome, height, and slope under the player).

**Out**

Rivers, erosion, caves, voxels, grass cards, atmospheres, real site scenes, more than four biome rows, any fiction names.

**How to judge it**

Walk for a few minutes and also look from orbit, same seed.

- From orbit you can point at the basin, the rim, and the plateau as different regions.
- On foot the ground curves, and a small rise hides what is beyond it.
- Five minutes of walking changes the surface character, or clearly enters one region from another.
- Forests have an edge. Trees stand out along the radius.
- Marker sites are far enough apart that each one is an arrival, and close enough that a short hike can find the next.
- The fine mesh holds a steady frame rate with the canopy instanced. Measure it. Do not invent a budget in advance.

If the orbit view is a potato, lower the region amplitude and keep the faces. If the ground feels like one noise, the stamped features are too weak or too wide. If biomes flip every few steps, the region wavelength is too short. Those three outcomes are the spike doing its job.

## 9. Assumptions gathered

1. People walk this planet and also see it from orbit.
2. Digging and caves are outside the spike. A yes on digging replaces the crust mesh with a voxel shell and needs an explicit GDExtension decision.
3. First relief target is about ±150 m for the broad shape, with local faces of 30–120 m. To be thrown out if the walk feels flat or the orbit looks broken.
4. First land fraction is about 70%, with one basin wider than a kilometre.
5. Four structural biome rows, fiction names chosen later. Suggested roles only: wet low ground, dry flats, broken rim, cold high ground.
6. Moisture is a noise in the spike. Wind and rain shadow are a reserved field.
7. Canopy spacing 8–12 m inside the mask is a test value.
8. Site markers exist to test spacing. What a site *is* is a design question, not a generator question.

## 10. One decision that changes the spike

Is this planet a shell you walk on, or a material you dig? The rest of the pipeline can wait on that answer. Shell is the recommendation until someone says digging is core.

## 11. Sources

- Minecraft Wiki, *World generation* (Java Edition multi-noise biomes, splines, decoration steps, carvers): https://minecraft.wiki/w/World_generation
- Henrik Kniberg, terrain-generation overview and the cave-noise note, linked from that wiki page
- Dynamic Trees wiki, world generation (Poisson placement, density from noise, biome scaling): https://github.com/DynamicTreesTeam/DynamicTrees/wiki/World-generation
- TerraForged: https://www.curseforge.com/minecraft/mc-mods/terraforged
- UltraTerraForged landform list: https://www.curseforge.com/minecraft/mc-mods/ultraterraforged
- Tectonic: https://www.curseforge.com/minecraft/mc-mods/tectonic
- Terralith FAQ: https://blog.curseforge.com/terralith-mod-faqs/
- Vintage Story wiki, *Terrain Generation*, *World generation*, *World Configuration*, *Temperature*: https://wiki.vintagestory.at/Terrain_Generation/en
- Dwarf Fortress Wiki, *World generation* and *Advanced world generation*: https://www.dwarffortresswiki.org/index.php/World_generation
- Red Blob Games, mapgen4: https://www.redblobgames.com/maps/mapgen4/ and the sphere experiment: https://www.redblobgames.com/x/1843-planet-generation/
- Valheim zone, location, and vegetation fields (Jotunn modding notes): https://valheim-modding.github.io/Jotunn/tutorials/zones.html
- Valheim biome order and the 10 km disc (community reverse-engineering writeup): https://metabot.gg/en/valheim/map
- Terraria Wiki, *World size* and *World generation*: https://terraria.wiki.gg/wiki/World_generation
- Starbounder, `Planetgen.config`, planet size and surface biome objects: https://starbounder.org/Modding:Planetgen.config
- Astroneer Wiki, *Planets* and *Region*: https://astroneer.wiki.gg/wiki/Planets
- Space Engineers Wiki, *Planets* (diameters) and the creating-a-planet modding notes (texel scale): https://spaceengineers.wiki.gg/wiki/Planets
- Innes McKendrick, GDC 2017, *Continuous World Generation in No Man's Sky* (voxel density, height fade, populate): https://www.gdcvault.com/play/1024265/Continuous_World_Generation_in__No_Man_s_Sky_
- No Man's Sky flora writeup (three storeys, slope and altitude, shared rigs): https://nomanssky-archive.fandom.com/wiki/Flora and the spawning overview at https://stepmodifications.org/wiki/NoMansSky:Reference_Guides/Spawning_Overview
- Outer Wilds planet diameters are community measurements (scout-on-a-pole threads). Developer note on Timber Hearth's modular rocks: https://www.mobiusdigitalgames.com/news/taking-on-timber-hearth
- Subnautica Wiki, *Getting Started* and *Crater Edge* (the two pages do not agree on a single width)
- Godot `FastNoiseLite`: https://docs.godotengine.org/en/stable/classes/class_fastnoiselite.html
- Cuberact, Planet Chunked LOD (GDScript cube-sphere quadtree, learning project, not a dependency): https://www.cuberact.org/projects/planet-chunked-lod/
- Hoimar, Planet-Generator addon (same family of technique, vegetation left open): https://github.com/Hoimar/Planet-Generator
