# Flight feel, second experiment

Status: first translation slice authorized by the initiator's "your call" on
2026-10-04, in response to the proposed experiment and recommended first slice.
The initiator delegated the provisional choice/tuning for this local spike.
Mouse steering and atmospheric banking remain subsequent experiments. This
records human authorization, not AI approval of a PR.

## Selected first slice

Translation tuning and minimal speed/clearance/state HUD, on the existing
`spike/assisted-flight` branch the initiator is playtesting. Keeping that baseline
avoids losing the assisted controller by branching from main. Ground cruise is
45 m/s through 30 m clearance; all higher speed points remain unchanged.

Velocity-error response is 0.35 s; acceleration smoothing is exponential with
a 0.15 s time constant (about 0.35 s to reach 90% of a steady request).
Acceleration budget is at least 30 m/s², rising to reference speed / 3.5 s;
Shift preserves a minimum 60 m/s². Braking budget is at least 40 m/s², rising
to max(actual speed, active forward limit) / 2.25 s. Curvature/drag support
still takes priority within the shared budget. These are arcade starts for
testing, not another game's values or guaranteed response times.

Terrain preview retains the conservative base braking estimate and includes
three smoothing time constants in the response allowance. Test ground thrust
buildup, a softer ground stop, 60 m/s settling within 2 s / 60 m, a 350 m/s
stop within 3.5 s / 650 m, and convergence after a small high-speed heading
change within 2.5 s. Retain ascent/descent, terrain, boarding and origin tests.
Human feedback remains needed to decide whether the result feels right.

Follow-up playtest: the initiator reports "far better", but stopping after
release is still too short at both low and high speed. They explicitly requested
altitude in the HUD. This follow-up adds reference altitude (distance from centre
minus planet radius), retaining separate terrain clearance. Acceptance: both
readouts are visible/readable while seated. Braking behavior is unchanged while
the release-versus-explicit-brake design is discussed.

## Accepted follow-up: gentle release and deliberate brake

The initiator answered "alright" to the recommendation of gentler automatic
slowing plus a deliberate strong brake, preserving steering authority. On the
same local spike branch, neutral translation input while piloted uses a provisional
14 m/s² base budget, rising to max(actual speed, forward limit) / 6 s. Active
movement/turn correction and unpiloted hover retain their previous authority.

Hold X to request zero velocity with the previous firm braking budget. X overrides
translation/boost input but permits orientation changes; releasing restores held
movement input. With H off, X temporarily applies the existing velocity/hover aid
while held; release restores manual thrust, gravity and coasting. HUD shows brake
state. No sticky throttle or new travel mode.

Acceptance: neutral stops take longer than 2 s near ground and 4 s at 350 m/s,
still settle within 5/9 s in the fixture, and maintain curved flight. X stops
faster, overrides W/Shift, works with H off, and release restores manual coasting
or held assisted input without velocity snaps. Preserve heading correction and
origin/terrain/boarding/cabin checks. Bot uses X before starting its landing.
This supersedes the first slice's release-stop timing; X retains the firm profile.

## Accepted diagnostic: apparent climbing with planet follow

The initiator answered "okay gut dann weiter" after the proposed nose-angle and
height-gain check. Add NOSE and VIEW elevation angles to the piloting HUD, relative
to the local horizontal plane perpendicular to gravity. Positive means pointing
upward. This is not the visible distant horizon, which dips below that plane.

Measure the existing controller with real scripted W input: level nose with
assist/follow on for 60 s; camera-level initial aim (nose +10 degrees) for 30 s;
and level initial nose with assist on/follow off for 30 s. Log reference-altitude
change, vertical velocity and nose/view angles. Level follow should remain within
20 m of initial altitude; camera-level should climb while retaining its pitch;
follow-off should leave the spherical surface on a straight trajectory. Repeat
the existing flight checks and visually verify the HUD. No gravity, mass, camera,
steering or thrust changes in this diagnostic slice.

## Accepted follow-up: finite planetary influence

The initiator wants full gravity inside atmosphere, then a gentle fade to no
perceptible gravity at 6 km reference altitude, allowing 7 km if recommended.
Selected recommendation: 6 km with the existing 1200 m atmosphere unchanged.
Use 9.81 m/s² throughout atmosphere, then a smoothstep fade to exactly zero
at 6000 m. This replaces the previous inverse-square gravity in this spike.
Keep both heights configurable; measure from the reference sphere, not terrain.

Planet Follow uses the same influence factor for orientation transport, curved
acceleration and terrain preview. At zero influence, enabled L does not bend
the path or orientation. H still provides assisted movement and release braking;
turning H off permits inertial coasting. Show effective follow percentage in HUD.
No speed, steering, camera, atmosphere or cabin-gravity retuning. The previously
measured small level-flight drift remains a separate correction, not an altitude
lock introduced by this change.

Acceptance: full strength through 1200 m, half at 3600 m, zero at/above 6000 m;
continuous smooth boundaries and origin-shift invariance. Retain low/full-follow
flight and braking checks. With H/L off, a stationary ship beyond 6 km does not
fall. With H on and L enabled beyond 6 km, forward flight stays straight, while
manual steering still works. Repeat rendered terrain/landing regression.

Regression repair: the full test exposed a shutdown abort from unreaped terrain
and collision WorkerThreadPool tasks. Scene exit now waits for those tasks while
the scripts are alive. This is a separate small correctness fix/commit, not a new
flight system or a red-class change.

## Problem and desired experience

The initiator reports that low flight starts and stops too suddenly, feels too
slow, and behaves like mouse look. High flight feels heavy and retains too much
sideways momentum during turns. Roll currently contributes no turning advantage.
The next experiment should already feel like a spaceship without a full flight
simulation. Planet radius remains 5 km; altitude-dependent travel remains wanted.

## Proposed slices, each separately reviewable

1. **Translation feel:** smooth acceleration changes, soften low-speed stopping,
   and increase high-speed braking/turn correction authority. Retain the existing
   steering initially so the difference can be attributed to translation tuning.
   Add actual speed, requested speed limit and terrain clearance readouts for
   the playtest. Acceptance: compare acceleration, stopping and heading-change
   response at low/mid/high clearance through scripted input and human play.
2. **Mouse steering:** persistent desired direction, bounded ship response and
   smooth camera following. Display desired direction and actual nose direction
   separately, plus a velocity-direction marker. Banking policy is unresolved.
   Acceptance: a mouse target persists after mouse movement stops; nose converges
   without snapping; camera and markers remain coherent through roll, vertical
   flight and origin shifts. Existing manual roll remains available.
3. **Atmospheric turn character:** only if wanted after the first comparisons,
   distinguish pitch-led banked turns from yaw-led turns, blending with atmosphere
   density. Acceptance: bank-and-pull changes the actual trajectory as designed,
   rather than only rotating the model. Free space steering remains available.

These are local issue drafts, not external issues or an approved implementation
plan. The initiator can choose a different first slice.

## Candidate translation values — recommendations, not decisions

- Ramp into acceleration over roughly 0.3–0.5 s rather than applying full
  correction in the first tick. Smooth stopping onset and the final settling.
- Explore a 1–1.5 s stop from low cruise and roughly 3 s from 350 m/s. Those
  times imply different correction limits; they are not verified values from
  another game. No instantaneous velocity clamps.
- Try about 45 m/s at the lowest clearance band, keeping the existing higher
  bands for the initial comparison. Retain descent assistance and terrain preview;
  faster ground flight must be checked against stopping distance and hills.
- Preserve enough correction authority after curvature/drag support to steer at
  travel speed. Do not hide high-speed loss of control behind a faster nose.
- Measure response along the trajectory, not just the ship's orientation.

The precise response curve, tolerances and budgets must be written into the
chosen slice before implementation. Do not promise a fixed stop time in every
terrain approach: support forces, mixed inputs and safety constraints matter.

## Minimal HUD, proposed

Actual speed in m/s, active requested forward limit, height above terrain,
vertical speed, readable Flight Assist / Planet Follow states. Steering slice
adds desired-direction cursor, nose marker and velocity marker. Hide the velocity
marker near zero speed; show an edge cue for an off-screen direction. Keep the
debug overlay separate from the normal piloting display. No combat HUD yet.

## Open question

Atmospheric mouse turns: automatic banking with manual roll override; manual
bank-and-pull; or equally effective yaw/pitch throughout? A bank animation alone
does not solve the current trajectory problem. No choice inferred from research.

## Scope and validation limits

No aerodynamic surfaces, stall model, damage, weapons, new travel modes or engine
replacement. Original implementation, no imported game code/assets. Anticipated
edits stay in spike gameplay files; no red-class paths needed. Use SpikeInput for
automated comparisons and retain boarding, cabin and origin-shift regressions.
The production MCP interface remains a prerequisite for a production feature.
Human playtesting decides feel; bot measurements alone cannot approve it.
