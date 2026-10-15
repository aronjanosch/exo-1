# Ship types

`kackfass_1/` defines **Kackfass-1**, the former prototype, with unchanged flight values and hull.

- `type.json`: stable type ID, display name, and hull layout key.
- `sc_ship.json`: empty mass (kg), inertia (kg m²), total thruster force per direction (N), torque (N m), and cargo mass anchor.
- `sc_linear.json`, `sc_angular.json`: translation/rotation control and speed profiles.
- `sc_drive.json`: boost, spool and thrust response.
- `sc_air.json`: atmospheric response.
- `sc_modes.json`: mode transition settings.

The app loads a `ShipType` and gives each instance its model identity plus its own mutable flight state. Flight and contact physics use the same mass and inertia, including cargo. G-safe is a flight-computer safety ceiling, not the thrusters' hardware rating.

Hull key `kackfass_1` selects the existing procedural visual, collision, cabin and ramp layout in `crates/exo_app/src/ship.rs`. It does not use the unrelated `starting-ship-study.glb`. Hull geometry is still authored in Rust, not duplicated in JSON.

In development, edits to this type's flight files and display name hot-reload. Changing the ID or hull requires a restart. `--tuning-dir` now takes a content-root-shaped directory, containing `tuning/` and `ship/kackfass_1/`; absent files retain their loaded values.

Kackfass-1 is the only starting ship type for now; there is no ship selection, purchase, upgrade system or network type negotiation in this slice.

## Decoupled flight

Decoupling immediately releases velocity control; the existing `decouple_time` field (4 s) now only controls the gradual return to coupled control. In vacuum, rotating without thrust leaves world velocity unchanged. Gravity compensation remains separately switchable, and explicit spacebrake still brakes.

Kackfass-1 has weak atmospheric drag in both modes (`drag`: x 0.000008, y 0.000002, z 0.000001 per metre). Decoupled lets that drag slow the ship naturally; it does not automatically match speed to the nose or limiter. Aerodynamic lift remains enabled, and the flight computer opposes it as far as thrust and G-safe permit. At high speed, strong lift can exceed the available counter-thrust, especially because strafe taper further weakens the maneuvering thrusters above cruise speed; exact vector preservation is then physically unavailable.
