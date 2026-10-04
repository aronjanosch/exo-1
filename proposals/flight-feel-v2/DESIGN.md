# Flight-feel diagnosis and references

## Contributor decisions and observations

- Existing assisted flight works mechanically. Improve its feel rather than
  building a complex simulator now.
- The confirmed planet radius is 5 km. Faster high-altitude travel is still wanted.
- Ground acceleration/stopping feels sudden; ground cruise feels too slow.
- At speed the ship feels heavy and turns poorly despite a responsive nose.
- Mouse aim should have a ship response rather than direct FPS-like rotation.
- Basic flight HUD is wanted. The initiator delegated the next spike choice with
  "your call"; translation tuning first was selected and documented in SPEC.md.
  Banking policy remains a recommendation for the subsequent steering experiment.

## What the current code actually does

`spikes/planet/ship.gd` requests velocity and immediately spends its available
acceleration budget on the error. With a 25 m/s low target and 40 m/s² braking,
the ideal constant-deceleration stop is 0.625 s. From 350 m/s it is 8.75 s and
about 1531 m. Real response also depends on support demands and settling.
Ship mass stays constant; the speed-dependent feeling comes from the controller.

At 350 m/s and 7000 m distance from the centre, curved flight needs about
17.5 m/s² inward acceleration. Curvature and drag are reserved before velocity
correction, reducing the remaining authority. This explains why simply keeping
the old braking limit while raising travel speed was a poor feel assumption.

Pitch/yaw have the same rate cap; roll does not supply lift or privileged turning
force. Local target velocity is corrected with a shared isotropic vector budget.
Banking therefore has no designed turning advantage. Mouse deltas become angular
rate for one physics step; no persistent desired-direction cursor exists.

Planet/horizon follow transports orientation with the changing local up. With
assistance it also supplies inward acceleration for curved motion. It is neither
gravity nor atmospheric drag. Assisted gravity cancellation is a separate idealized
hover assumption; unassisted flight applies gravity and density-dependent drag.

## Supplied transcript

`transcript.temp.txt` explains attitude/horizon, speed and adjustable speed limit,
altitude, precision approach, and actual travel direction even when the nose points
elsewhere. It demonstrates rolling before changing heading. It does not specify
the mouse control law, numeric turn authority or prove that yaw cannot turn well.
The summaries mention these broader ideas, but the original spike deliberately
excluded a rotation redesign. Their presence in research did not implement them.

## Primary references inspected

- [MouseFlight](https://github.com/brihernandez/MouseFlight): open-source aircraft
  control demo, not a complete military game. Separates desired mouse direction,
  aircraft orientation and smooth camera motion; exposes aim and nose positions
  for the HUD and demonstrates manual pitch/roll overrides. Useful control
  architecture, not flight tuning to copy.
- [Pioneer quickstart](https://github.com/pioneerspacesim/pioneer/blob/master/Quickstart.txt):
  an actual open-source space game. Separates manual thruster control from a
  set-speed flight computer that tries to align velocity with the ship's facing.
  Illustrates why assistance and inertial physics can coexist.
- [Godot aerodynamic control notes](https://github.com/addmix/godot_aerodynamic_physics/blob/main/docs/advanced_concepts/control_theory.md):
  discusses angular-rate feedback, target-direction mouse steering and coordinated
  aircraft control. Useful if atmospheric turn character is later pursued. No
  addon dependency is proposed.

These examples support independent experimentation, not claims about the current
Star Citizen implementation or adoption of another game's exact mechanics.

## Recommendation and unresolved ownership

Selected under the initiator's delegation: first isolate translation response,
then test mouse aim/camera feedback. Prefer
automatic atmospheric banking with manual override for approachable dogfight feel,
subject to the initiator's answer. Do not make a bank merely cosmetic or restrict
space yaw by accident. Minimal HUD should make input, orientation and trajectory
legible before adding combat systems. All new numbers are suggested playtest starts.

Vision fit: original small Godot experiments fit the arcade-first direction in
the concept repo. Gameplay risk class yellow; this is a fit assessment, not
proposal or PR approval. Performance risk is controller/HUD cost per ship, not
fluid simulation; measure frame time in the rendered build if implemented.
