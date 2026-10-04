# Translation feel slice — local review

Summary: eased velocity correction and thrust buildup, raised near-ground cruise
to 45 m/s, and scaled correction authority for fast flight. Added a small piloting
HUD independent of F3. Mouse aim/banking are not implemented in this slice.

Risk class: yellow, spike gameplay and documentation. No red-class paths touched.
Vision fit: fits the original, arcade-first, small Godot experiment. No imported
assets/code, new dependencies, runtime code loading or new risky APIs. The HUD
updates text at 10 Hz while seated; controller changes use bounded vector math.

## Verified

- Real rigid-body spherical fixture: 22 checks, zero failures. Gentle thrust
  buildup: 0.152 m/s after two ticks. Near-ground speed 45 m/s; still moving
  30.8 m/s half a second after release and settled below 0.5 m/s within 2 s.
- At 60 m/s: release stop 54.66 m over 2 s, residual 0.311 m/s.
- At 350 m/s: release stop 2.75 s / 474.5 m, residual 0.348 m/s. A small
  heading change converged within 2.5 s, with less than 0.02 m/s lateral motion.
- High-speed curved flight remained within 3 m of initial 2000 m clearance in
  the fixture. Diagonal input, boost release, assist switching, descent, mountain
  clearance and translated planet frame checks passed.
- Rendered Forward+/Vulkan low cruise/landing run exited zero: minimum clearance
  84 m during bot cruise, landed 0.03 m above terrain, zero rescues. HUD screenshot
  inspected: speed/limit/clearance/vertical speed/states are readable at bottom left.
- Worker shutdown fixture exited zero, releasing 24 terrain and 60 collision jobs
  when the scene was removed. Uses weak references to detect retained job objects.
- Full headless terrain/boarding/cabin/origin regression after the cleanup fix
  exited zero. Reached 2000 m, cruised with minimum 61 m ground clearance,
  landed at 0.28 m reference clearance, walked out/back in, and kept the passenger
  inside during boost/roll and cabin walking. Zero rescues; seven origin shifts
  with zero measured speed discontinuity. No script errors/warnings in that run.

## Shutdown failure found and repaired

Two full headless runs finished every gameplay phase, then exited 134. Symbolized
core: Main::cleanup -> unregister_core_types -> WorkerThreadPool::finish -> bound
callable destruction -> GDScriptInstance destructor -> invalid mutex. No evidence
of OOM. Both terrain and collision ring lacked exit cleanup for pending task IDs;
their bound RefCounted script Jobs remained in the worker pool. Godot documents
that every task must be waited for so its resources can be cleaned up:
[WorkerThreadPool](https://docs.godotengine.org/en/stable/classes/class_workerthreadpool.html#class-workerthreadpool-method-add-task).
Added `_exit_tree` waits in both nodes; no rendering/LOD policy changed.

## Limits and missing

Human feel judgment remains outstanding. Terrain sampling cannot guarantee
collision avoidance; the bot uses its own altitude hold. No aerodynamic surfaces,
banked turning advantage, reticle steering or combat simulation was tested.
Fixed-FPS runs are behavioral checks, not frame-time benchmarks. No low-end or
multiplayer benchmark, production MCP bridge, external issue or PR. No configured
standalone GDScript linter was found; Godot loads/compiles scripts in these runs.

Labels suggested: spike, flight-feel, yellow-gameplay. This review reports evidence;
it does not approve a proposal or PR.
