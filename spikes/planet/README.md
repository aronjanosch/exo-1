# Spike 1: planet (throwaway)

Prototype to test whether a small seamless planet works in Godot 4.7.2. Not final structure. Results go into the spike report. Low-spec hardware is not measured in this spike by the initiator's decision (optimise later); numbers are from the dev machine.

Run: `godot --path .` from the repo root (agents: see `WORKSPACE.md` if present).

## Controls

- Mouse: look. Esc frees the mouse, click captures it again.
- Walker: WASD walk, Shift run, Space jump. V: debug fly mode (no gravity, no collision; Space/Ctrl up/down, Shift x4, mouse wheel speed).
- F at the seat inside the cabin: sit down / stand up. Walk in and out over the ramp at the back.
- Ship: mouse pitch/yaw, W/S forward/reverse, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost. H: flight assist (default on: holds requested velocity, cancels gravity, brakes on release, slows requested descent near terrain). Forward speed grows with terrain clearance; boost raises forward speed away from the ground and acceleration. Without assist the original thrust, x5 boost, gravity and quadratic air drag remain, with coasting in space. L: horizon follow (default on: the ship's frame turns with the local up while moving, so "straight" means along the horizon and pitch relative to the horizon stays constant).
- Release movement keys for gentle assisted slowing; hold X for a firm brake.
  X overrides movement/boost while held, including with H off. Release X to
  restore held input (or manual coasting with H off). The HUD shows brake state.
- F3: toggle debug overlay. F12: screenshot to `user://screenshots/`.
- While seated, a separate flight HUD shows speed, forward speed limit, altitude
  above the planet's reference sphere, terrain clearance (GROUND), vertical
  speed, Flight Assist (H), and Planet Follow (L). It stays
  visible when F3 diagnostics are hidden. Planet Follow follows the planet's
  curved horizon; it is separate from gravity and atmospheric drag.
- NOSE and VIEW show the ship/camera elevation relative to the local horizontal
  plane. Positive points up; zero is level. The chase camera looks 10 degrees
  below the nose, so a level view can mean a climbing ship. The visible distant
  horizon dips below local horizontal with altitude. Planet Follow preserves
  existing pitch; it does not automatically level the nose or lock altitude.
- Terrain debug: 1 LOD colours, 2 skirts on/off, 3 freeze LOD, 4 reset max stats, 5 flat-shading strength (1, 0.6, 0.3, 0; facets also fade to smooth between 80 and 400 m).

Command-line options (after `--`):

- `--radius=<m>`: planet radius (default 5000, the first guide value in DECISIONS.md). LOD depth and collision cell depth follow it (leaf chunks about 31-37 m, collision cells at most about 20 m).
- `--auto-shot`: scripted screenshots (ground, ship, low flight over a cube corner, orbit), then quit.
- `--auto-test`: scripted run (stand still, walk, board, climb to 2000 m, descend, cruise low with a bot altitude hold, land, idle) with frame-time and precision stats per phase; writes `user://spike_results.txt`. Held keys go through `SpikeInput`, so the run does not depend on window focus.
- Depth-buffer test: `godot --path . res://spikes/planet/depth_test.tscn [--rendering-method forward_plus]`.

Spike 5 options (float limit and origin shift, branch `spike/origin-shift`, results in the concept repo's `SPIKE-5-REPORT.md`):

- `--planet-offset=x,y,z`: put the planet centre there instead of the origin.
- `--origin-shift=<m>`: when the active body is farther than `<m>` from the origin, move everything back by its position (whole metres). Runs in `_process`, see `main.gd`.
- `--second-planet=x,y,z,radius`: a second planet relative to the first. Gravity and atmosphere come only from the nearest planet (test assumption).
- `--recenter`: when the nearest planet changes, shift so its centre is the origin.
- `--auto-test --walk-only`: stop after the walk phase. `--fly-out`: climb to 10, 25, 50, 100 km from the origin and measure there. `--fly-to-second`: take off, fly to the second planet with a test autopilot, land, walk.
- Scripted runs (`--auto-test`, `--auto-shot`) never capture the mouse, so they do not pull focus. Results go to `user://spike5_results.txt`, screenshots get an `s5-` prefix.
- Diagnostics: `--safe-margin=<m>` (walker), `--shift-in-physics` (old, broken shift timing), `--trace` (walk and ring trace).
- `jitter_probe.gd` reports, per phase, how far (in pixels at 1080 lines) the GPU's float32 `view * model` puts a point 2 m ahead and the ship's nose, compared with double precision. Calculated on the CPU, not read back.

## State

- Step 1: placeholder UV sphere, radial gravity walker, debug overlay.
- Step 2: cube-sphere terrain (spherified cube), quadtree LOD per face (about 37 m leaf chunks, 32x32 quads), chunks built on `WorkerThreadPool`, skirts, flat shading via derivatives plus smooth normals for colour blending, world triplanar detail.
- Step 3: collision ring. `HeightMapShape3D` patches (32x32, 1 m spacing) per cube-face cell (about 18 m at R = 3 km), each in its own tangent frame with curvature baked into the heights, overlapping neighbours. Ring radius 100 m around the active body (plus 0.5 s look-ahead), only when it is within 100 m of the ground. Built on worker threads, at most 16 new bodies per frame. The CPU height function stays as a safety net and counts real fall-throughs as `rescues`.
- Step 4: ship (`RigidBody3D`, Jolt, engine gravity off). Gravity 9.81 * (R/r)^2, atmosphere density 1 at the surface to 0 at 1200 m, drag scaled by density. Planet-aware sky shader (up and horizon dip from the camera position, blue to black with stars), fog and ambient light follow the density. Boarding without reparenting: the walker is disabled and placed next to the ship on exit; a parked ship is frozen.
- Spike 3 (branch `spike/leave-ship`): walkable greybox cabin, sit/stand with F at the seat, walker in the ship's frame. Results and open points: exo-1-concept `docs/SPIKE-3-REPORT.md`. `--auto-test --board-only` logs a ramp boarding attempt.
- Step 5 (partly): radius option, precision phases in the auto-test, depth-buffer test, Forward+ run.

## Measurements (dev machine: RTX 5070 Ti, 240 Hz vsync)

`--auto-test`, frames right after a test screenshot excluded. Runs at R = 3 km Compatibility, R = 3 km Forward+, R = 1.5 km and R = 5 km Compatibility:

- Frames > 33 ms: 0 in walk, climb, descend, cruise and land in all clean runs; worst single frames 8-29 ms. (Runs with the window on a hidden workspace are throttled to about 8 FPS and are not valid measurements.)
- Rescues (fall-throughs caught by the safety net): 0 in every run.
- Precision, standing still for 5 s: walker 0.0000 mm frame-to-frame movement at 1463 m, 2932 m, 5001 m, 7928 m and 16031 m from the centre; landed ship at most 0.001 mm per frame, at most 0.04 mm drift. No physics jitter up to R = 16 km.
- Radius limit: R = 8 km passes every phase. At R = 16 km everything passes except walking: the walker hits a "wall" after 13 m on ground that does not rise (checked with the CPU height function). Likely cause, not verified: at 16 km from the origin positions are only good to about 1-2 mm, so overlapping collision patches are slightly offset and the capsule catches on an edge. Practical limit without origin shifting: about 8 km (measured). Larger planets need an origin shift (spike 5).
- Chunk build (worker): 2.2-2.7 ms average, 6 ms max. Collision patch build (worker): about 1 ms average, 3.6 ms max.
- Main thread per frame: terrain at most about 5.5 ms (LOD traversal is most of it), collision ring at most about 4.5 ms.
- About 400-550 visible chunks near the ground, 100-200 draw calls; 25-30 chunks from orbit. Radius made no visible difference to frame times.

Depth buffer (near 0.05 m, far 50 km, camera 3000 m from the origin, red/green quad pairs):

| Renderer | Result |
|---|---|
| Forward+ (Vulkan) | No z-fighting at any tested distance (10 m-40 km) and gap (1 cm-1 m): reverse-Z with a float depth buffer works |
| Compatibility (OpenGL) | Z-fighting from about 500 m at 1-10 cm gaps and from 2 km at 1 m gaps. Matches a classic 24-bit depth buffer (calculated resolution 0.3 m at 500 m, 4.8 m at 2 km), so no reverse-Z gain there |

Jolt height maps (verified in the 4.7.2-stable source, `modules/jolt_physics/shapes/jolt_height_map_shape_3d.cpp`): square maps with at least 4 samples per side use Jolt's native height field; only non-square maps fall back to a polygon mesh. The 32x32 patches are on the native path.

## Notes

- Skirts with the flat face normal look like dark lines at chunk borders; giving skirts the smooth normal fixes it.
- Derivative flat normals gave single dark (unlit) pixels next to skirt slivers: on sub-pixel triangles `dFdx`/`dFdy` are zero, `normalize()` returns NaN and NaN survives `mix(..., 0)`. Guarding the cross-product length fixes it. Diagnosed by colouring skirts red and the background white.
- Hairline cracks between chunks of the same LOD exist because every chunk has its own origin (float32 rounding); the skirts fill them as intended.
- Full flat shading reads as pixel noise from a few hundred metres on. Facets now fade to smooth normals with distance.
- Horizon dip on a 3 km planet is large: about 18 degrees at 150 m altitude (calculated).
- The first ring version included terrain amplitude in the distance test and built about 740 patches instead of about 150, with an 84 ms frame; fixed by comparing on the base sphere.
- Flying straight at speed on a 3 km planet leaves the planet within seconds (the ground curves away). Decision (initiator): straight means along the horizon. Implemented as horizon follow, an extra angular rate w = up x v / r; a levelling force was tried first but would fight intended climbs.
- A blind bot cruising at boost speed hit a hill, tumbled and could not land (Ctrl is "down" in ship space). The test bot now holds 80-150 m above ground. A real player can do the same; whether the ship needs a self-righting aid is open.
- Screenshot readback plus PNG save costs 50-140 ms per shot; never measure frames that contain one.
- Godot releases all pressed keys when its window loses focus; tests that press keys through `Input.parse_input_event` then silently stop walking. Fixed with `SpikeInput`.
- Teleporting causes 60-140 ms frames from the burst of new chunks; normal flight does not.

## Open

- Compatibility renderer and z-fighting: fine for terrain alone (no coplanar surfaces), but decals, roads or building bases far away will flicker. Options: Forward+/Mobile (reverse-Z), larger near plane (0.05 to 0.2 gives 4x), or a near plane that grows with altitude. Decision for the initiator.
- Visual jitter at 3-5 km from the origin is not measured, only calculated: float32 steps of 0.24 mm (2-4 km) and 0.5 mm (4-8 km) are below a pixel unless the camera is closer than about 1 m to a surface.
- No LOD fade or geomorphing yet; whether popping is visible needs the initiator's eyes.
- Climb phase showed 3 frames of 33-40 ms in one run (many chunks merging at once); not reproducible so far.
- Normal maps with triplanar (the "hairy ball" question) are not tested; triplanar colour without normal maps needs no tangents.
- The faceted look came from the brief (an assumption) and does not match the look in `DECISIONS.md` (smooth shading). Key 5 down to 0 shows smooth terrain.

## Open tuning values (by feel, later)

Assisted-flight experiment (initiator agreed the behavior on 2026-10-04; exact
numbers below are provisional spike tuning, not final design):

- Forward target: 45 m/s through 30 m terrain clearance, 60 m/s at 150 m,
  150 m/s at 600 m, 350 m/s at 1200 m and above. Smooth blends between them.
- Reverse 25 m/s, strafe 20 m/s, vertical 15 m/s. Combined input is normalized.
- Acceleration budget starts at 30 m/s² and grows with cruise/actual speed
  divided by 3.5 s. Braking starts at 40 m/s² and grows with cruise/actual speed
  divided by 2.25 s. Boost retains at least 60 m/s² acceleration.
  Neutral piloted input uses gentler braking: 14 m/s² minimum, growing with
  cruise/actual speed divided by 6 s. X, active movement correction, and unpiloted
  hover retain the firm authority. Holding X with H off temporarily applies the
  existing velocity/hover aid; releasing restores manual flight.
  Velocity error uses a 0.35 s response and thrust builds with a 0.15 s smoothing
  time constant, easing starts and stops without snapping velocity.
  Horizon curvature and drag compensation reserve some of that budget; gravity
  cancellation retains the existing arcade assumption.
- Shift progressively raises the forward target to x2.5 between 30 and 150 m
  clearance, with a ceiling of 350 m/s. It does not boost reverse/sideways/vertical
  speed. Near-ground flight stays slow even with Shift held.
- Speed requests anticipate descending and sample the upcoming terrain over a
  braking horizon. This is a governor, not a collision-avoidance autopilot;
  sharp approaches and terrain between samples can still be dangerous.
- F3 shows actual speed, commanded speed, and the current forward limit.
- Second feel experiment: `proposals/flight-feel-v2/`. Mouse remains direct
  pitch/yaw in this translation-only comparison; mouse aim and automatic banking
  are subsequent experiments. All values still need a human feel test.
- Controller checks: `GODOT_AGENT_WORKSPACE=7 godot-agent --headless --path .
  --fixed-fps 60 --script res://spikes/planet/flight_test.gd`. Tests use the actual
  ship controller and scripted movement input on a spherical fixture. Terrain,
  boarding and cabin regression: `--auto-test` (headless supported). Fixed-FPS
  accelerated runs verify behavior; their frame-time numbers are not benchmarks.
- Worker shutdown check: `GODOT_AGENT_WORKSPACE=7 godot-agent --headless --path .
  --script res://spikes/planet/shutdown_test.gd`. Frees a scene with queued terrain
  and collision work and checks that the jobs are released before engine shutdown.

- Walk 5 m/s and run 12 m/s are placeholders and fast (real walking is about 1.4 m/s).
- Unassisted ship: thrust 20 m/s^2, boost x5, turn rate cap 2.5 rad/s, quadratic drag k 0.0005 (terminal about 200 m/s, boost about 450 m/s), assisted landing sink factor 0.5.
- Gravity 9.81 at the surface, atmosphere top 1200 m, terrain amplitude 150 m.
- Planet radius: 1.5, 3 and 5 km all run; which one feels right is the initiator's call.
