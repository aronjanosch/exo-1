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

## Follow-up: altitude HUD

Explicit initiator request adds reference altitude alongside terrain clearance.
Risk remains yellow; vision fit remains fits. The value uses planet-relative
position and the current planet radius, so origin shifts do not change it.
Forward+/Vulkan cruise/landing exited zero with zero rescues; inspected screenshot
shows ALTITUDE 103 m and GROUND 111 m, both readable and consistent with the debug
readouts. No controller changes or new automated tests for this display-only edit.

Further playtest: "far better", but release stopping still feels too short at
both speeds. Candidate next experiment: gentler neutral-input slowing, separate
deliberate braking, while preserving turning authority. Not implemented here.
The [CIG landing guide](https://support.robertsspaceindustries.com/hc/en-us/articles/360020925254-How-to-Land-Your-Ship)
describes coupled automatic stopping, decoupled momentum, and X space brake.
Supplied summaries additionally describe retained throttle; no exact current
ship braking values or timings have been verified.

## Follow-up: gentle release plus held brake

Initiator accepted the proposed experiment with "alright". Neutral piloted input
now uses the gentler budget; X overrides translation and retains firm stopping.
Active steering and unpiloted hover retain previous authority. With H off, held
X temporarily applies the existing velocity/hover aid. Bot landing now uses X.
Risk: yellow. Vision: fits. No new dependencies, risky APIs, red paths, content
code or imported implementation. No per-frame allocations added to physics.

Verified: 32 fixture checks passed. Ground neutral release from 45 m/s reached
below 0.5 m/s in 3.57 s. From 350 m/s, neutral release reached that threshold in
7.15 s / 1313 m; X reached it in 2.75 s and settled below threshold at 4 s.
Initial brake correction is bounded; W/Shift override, brake release, H-off
braking/manual resumption and curved stopping passed. Heading correction still
converges within 2.5 s. No instantaneous velocity clamps introduced.

Full headless regression exited zero: zero rescues, successful boarding and
cabin walking, no passenger ejections, six shifts with zero speed discontinuity.
Rendered Vulkan cruise/landing exited zero with zero rescues. Screenshot inspected:
BRAKE [X] ACTIVE is readable alongside ALTITUDE. Bot brake before landing uses
real input; this is still not a collision-avoidance system. Human feel judgment,
production MCP coverage and runtime performance benchmarks remain outstanding.

## Follow-up: planet-follow attitude diagnostic

Initiator accepted the nose-angle/altitude comparison with "okay gut dann weiter".
HUD now shows NOSE and VIEW elevation above local horizontal, updated at the
existing 10 Hz. Controller, camera, gravity and mass are unchanged. Risk: yellow.
Vision: fits. No red paths, dependencies, risky APIs or imported implementation.

All 36 fixture checks passed at 60 physics ticks/s. From 2000 m reference altitude:
level nose with assist/follow lost 12.91 m over 60 s (final radial speed -0.53 m/s);
initially level camera view, nose +10 degrees, gained 1700.33 m in 30 s;
level initial nose with follow off gained 5071.91 m in 30 s. The latter two
demonstrate preserved upward pitch and departure from the sphere respectively.
Small sinking remains measurable; planet follow is not an exact altitude lock.
The camera offset is a supported explanation, not proof of the user's specific
playtest cause. Fixture has a spherical surface, without terrain or atmosphere
at these altitudes; no claim of general terrain-clearance preservation.

Forward+/Vulkan cruise/landing exited zero with zero rescues and two origin shifts
with zero speed discontinuity. Inspected screenshot shows readable NOSE -0.0
degrees / VIEW -10.0 degrees. Scripted input exercises the existing bot interface;
production MCP coverage and human interpretation of the display remain untested.
This review reports evidence and does not approve a PR.
