# Assisted flight feel — spike proposal

Status: behavior accepted by the initiator on 2026-10-04: "Alright lets do it
this way", following the altitude-dependent flight discussion. Implementation
is a local throwaway spike, with provisional values for playtesting. This is
human authorization for the experiment, not AI approval of a PR.

## Accepted altitude refinement and implementation assumptions

The initiator confirmed a 5 km planet radius and accepted slow flight near
terrain, faster travel as clearance increases, release-to-brake and assistance.
The earlier flat 60 m/s forward target is superseded by a smooth clearance curve:
30 m -> 25 m/s, 150 m -> 60 m/s, 600 m -> 150 m/s, 1200 m -> 350 m/s.
These are provisional points within the discussed speed ranges. Reverse,
sideways, vertical and acceleration/braking values remain the proposed starts.

Forward boost blends from no speed increase near 30 m clearance to x2.5 by
150 m, capped at 350 m/s. No boosted vertical/reverse/sideways target. Terrain
preview and descent stopping distance lower requested speed before entering
low flight. They do not guarantee avoidance of obstacles or emergency dives.

Horizon-follow acceleration and atmospheric drag compensation get priority;
the remainder of the vector thrust budget corrects velocity. Braking uses the
40 m/s² budget when the correction opposes current velocity, otherwise 30 m/s²
(60 with Shift). Gravity compensation remains the spike's idealized hover aid.
The previous open controller-budget question is resolved by this conservative
allocation. All of these exact numeric and allocation details are implementation
assumptions for the authorized tuning experiment, not settled game design.

The local branch starts from `spike/combined` to retain the existing planet and
cabin experiments; `main` does not contain this playable baseline. No push or
external PR is part of this local spike task.

## Problem and player experience

Contributor request: perpetual drifting feels strange on the small planets;
research Star Citizen flight and try a small improvement during the initial spike.

Current `spikes/planet/ship.gd` adds acceleration while a movement key is held.
Hover assist defaults off. Air drag falls to zero outside the atmosphere.
Releasing a key therefore does not command a stop. H enables existing gravity
compensation and damping on axes without input, but does not set a target speed
or limit braking acceleration. Horizon follow rotates the ship's frame; it does
not itself curve the velocity vector around the planet.

Proposed experience: hold W to accelerate toward a manageable forward speed;
turn while holding W and the ship progressively redirects its motion; release
movement keys and the ship brakes to a hover, including in space. Momentum is
noticeable but recoverable. H keeps an unassisted comparison available.

## Research and provenance

CIG's [Flight Model and Input Controls](https://robertsspaceindustries.com/en/comm-link/engineering/13951-Flight-Model-And-Input-Controls)
(2014) documents a flight computer with goal velocities, feedback control,
gravity compensation and limited thruster response. It also explains why
insufficient maneuvering thrust causes sliding during turns. This is a historical
primary source for the control principle, not verification of current tuning.

The contributor also supplied `transcripts-summary.tmp.md`, distilled from four
video discussions/walkthroughs. These notes reinforce goal-velocity control and
distinguish assisted auto-braking from configurable **sticky throttle**: W/S
change a retained speed goal, so releasing W can continue commanded cruise even
with assistance active. This differs from uncontrolled inertial drift. The video
URLs/dates and raw transcripts are unavailable in the notes, and ASR-derived
numbers/key names are explicitly unreliable; treat them as supplied research
notes, not independently verified current specifications.

Other useful principles from the summaries are a display of commanded versus
actual motion, a velocity-direction indicator, and protecting maneuvering/braking
authority when increasing forward travel speed. Space/atmosphere acceleration
differences are a possible later experiment. Aerodynamic surfaces, stalls,
thruster damage and travel/combat mode systems exceed this slice.

Correction to the summaries' code interpretation: the drag expression is
evaluated everywhere, but `drag_k * density` is zero where density is zero;
the current ship does not experience this atmospheric drag in vacuum.

The proposal uses that general principle with an original, simplified controller.
No Star Citizen code, assets, ship data, names or texts are imported. Current
Star Citizen acceleration and braking values have not been verified; none of
the values below are claimed to come from that game.

The concept repo's DECISIONS.md already calls for arcade flight, starting values
only and tuning by feel. A full Newtonian simulator remains parked.

## First slice

Behavior is accepted; exact values remain provisional spike tuning:

- Assisted flight defaults on. Existing H toggles assisted/unassisted flight.
- In assisted mode, movement keys request velocity in the ship's local frame.
  Neutral input requests zero velocity, rather than continued acceleration.
- The controller changes velocity with bounded acceleration; it does not snap
  instantly to the target or hard-clamp existing momentum after switching modes.
- Gravity is compensated in assisted mode. Atmospheric drag remains part of
  the existing environment; the controller responds to velocity error.
- Assisted mode preserves horizon follow, boarding and unpiloted hover behavior.
- Unassisted mode preserves existing acceleration, gravity and coasting for
  comparison. Its current boost behavior is outside the proposed tuning below.

| Setting | Proposed starting value | Purpose |
| --- | --- | --- |
| Forward target speed | 25–350 m/s by clearance | Slow near terrain, fast higher up |
| Reverse target speed | 25 m/s | Back away without full forward speed |
| Sideways target speed | 20 m/s | Positioning |
| Vertical target speed | 15 m/s | Takeoff and approach |
| Acceleration limit | 30 m/s² | About 2 s to forward cruise, idealized |
| Braking limit | 40 m/s² | About 1.5 s / 45 m stop from 60 m/s |
| Shift forward target | Up to x2.5, ceiling 350 m/s | Faster travel away from terrain |
| Shift acceleration limit | 60 m/s² | Faster acceleration, with bounded response |

Values are unmeasured tuning assumptions. Stop estimates use constant braking
on a single axis, with no environmental forces or controller settling time.
At 150 m/s and 40 m/s² braking, the ideal stop is 3.75 s / 281.25 m: boost still
requires planning. At 60 m/s and 30 m/s² lateral acceleration, the ideal turn
radius is 120 m; actual turns depend on controller allocation and heading input.

Normalize combined target input so diagonal movement does not grant extra
speed. Define acceleration/braking limits on the whole correction vector,
not independently per axis. Boost only raises the forward target and overall
acceleration budget; sideways, vertical and reverse targets stay as listed.
The accepted refinement above documents how mixed turning/braking corrections
share that budget.

## Alternatives considered

1. Selected: bounded target-velocity assist with the altitude refinement above.
2. Alternative feel: bounded target-velocity assist with sticky forward throttle;
   releasing W retains the requested cruise speed, S lowers it, and an explicit
   stop command requests zero. This requires different controls and acceptance
   checks; it is not included in the proposed release-to-brake behavior.
3. Smaller comparison: enable the existing H assist by default and tune only
   its damping; this can test release-to-stop but retains uncapped acceleration.

Resolved by the initiator: use the recommended assisted release-to-brake behavior
with altitude refinement above. Sticky throttle remains a later candidate.
The supplied summaries are research, not authorization in themselves.
The governing AGENTS.md requires an approved proposal before game code.

## Acceptance for the proposed slice

Use the spike's existing scripted-input path, not direct velocity assignment
to manufacture success. Check motion in atmosphere and outside it:

- Hold W: assisted speed settles near its target without continued acceleration.
- Release W from 60 m/s: speed falls below 0.5 m/s within 2 s and travels less
  than 60 m in a controlled space test.
- Turn while holding W: velocity trends toward the new forward direction with
  bounded acceleration, rather than retaining the old vector indefinitely.
- Release all input near the surface: no accumulating gravitational descent.
- Toggle H at speed: no instantaneous velocity discontinuity; unassisted space
  flight retains momentum with no input.
- Release Shift: return toward normal cruise through braking, not a speed snap.
- Diagonal input respects target normalization and the total acceleration budget.
- Boarding, standing inside the cabin, horizon follow and origin shifts retain
  their existing behavior.

The current interplanetary test writes velocity directly, so it cannot prove
these controller properties. The existing scripted input is the spike test
entry point; a production MCP bridge is still an open project decision.

## Implementation issue draft

One local spike slice: target-velocity assist plus tuning, overlay/control
documentation, and a scripted stop/turn comparison. Acceptance: the checks
above and a human flight-feel playtest. No external issue or PR filed.

No new flight modes, fuel, thruster damage, landing gear, cruise UI or rotation
redesign in this slice. Restrict anticipated edits to spike gameplay files and
their README; no red-class files are needed.

## Vision check

Fit: fits (behavior choice accepted; provisional budget documented above).
Risk class: yellow (gameplay code in the spike; no core configuration changes).

- Vision, Inspiration/Spirit: learn the control principle and implement it
  independently; avoid a one-to-one flight-system copy.
- Frame: small Godot controller change, no new assets or expensive simulation.
  Weak-hardware performance has not been measured.
- Core design: flight feel and tuning remain the initiator's decisions.
- Scope: one ship's assisted movement, consistent with the arcade-flight frame.
- Testability: existing scripted input can exercise the spike; production MCP
  coverage remains required when this becomes a game feature.

This labels the proposal's fit; it does not approve implementation or a PR.
