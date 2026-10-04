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
