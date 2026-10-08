# NIGHT-LOG — milestone C night run, 2026-10-09

Morning report of the unattended run (brief: concept repo `docs/RUN-C-NIGHT-BRIEF.md`). Phase 1 on `feat/milestone-c-grab`, phase 2 (extras) on `night/extras`. Nothing merged, no PR, no issue closed.

## Checklist

Phase 1, `feat/milestone-c-grab`:

- [x] #81 `grab_core`: hold, falloff, break, throw, shared carry (test-first)
- [ ] #80 crates as data, living in the ship's frame
- [ ] #82 interaction: one verb, one prompt
- [ ] #83 grab in the game: hands and the grab tool
- [ ] #84 lock grid in the cabin
- [ ] #85 object budget
- not in this run: #86 (network)

Phase 2, `night/extras`: not started.

## Architecture choice (read this first)

A crate is **not** an Avian rigid body. It is a `grab_core::CrateBody`: a box that stays upright about its frame's up, falls, slides with friction, sleeps at rest and moves by sweeps through a `BoxWorld` trait (the walker's pattern: `walker_core::World`). It lives in a frame like the walker: the planet, or a ship cabin (ship-local). Reasons: decision 1 (crate lives in the ship's frame, held during warp), the research risks (Avian moves child colliders one step late; the warp sets the ship pose at up to 1e6 m/s), and decision 6 (locked crates are part of the ship). Cost: no tumbling, no crate pushing the ship. `TODO(initiator)`: fine for the playtest, or do crates need to tumble?

## #81 grab_core

What: new crate `crates/grab_core` (no Bevy types, f64). Crate table (`content/cargo/crates.json`), grab tuning (`content/tuning/grab.json`), hold regulator (velocity servo to a hold point; force cap per holder, speed cap falling as 1/mass above `ref_mass`), falloff (hands: full to 2 m, nothing beyond; tool: full to 6 m, zero at 10 m), cone check, turn cap with contact damping, break timer, throw velocity and kick, carry state, view turn share, crate body.

Checks (`cargo test -p grab_core`, 21 tests):
- lag grows with mass: error 0.4 s after grabbing from 1.4 m away: small 0.024 m, medium 0.280 m, large (two holders) 1.138 m; all below 0.05 m after 4 s.
- falloff curve: hands 1 at 2.0 m, 0 at 2.01 m; tool 1 to 6 m, 0.5 at 8 m, 0 at 10 m.
- break timer: breaks after 1.000 s of error above 1.5 m; a 0.5 s snag does not; standing on the crate breaks at once.
- throw speed per size: small 8.0 m/s (capped), medium 2.0 m/s, large 0.5 m/s; the thrower gets the opposite impulse.
- one holder cannot lift the large crate (falls 33 m in 3 s with no floor), two hold it (error 0.000 m).
- table rejects a missing field, unknown fields, bad mass, extents, hands, holders, duplicate names, edges that do not double.

`TODO(initiator)` values (all in `content/tuning/grab.json` and `content/cargo/crates.json`):
- sizes: small 0.5 m 15 kg one hand; medium 1.0 m 60 kg two hands; large 2.0 m 240 kg two hands, two holders.
- hold force 1500 N per holder (one holder lifts up to about 150 kg, two up to about 300 kg).
- speed cap 8 m/s up to 30 kg, then 1/mass, at least 0.5 m/s; gain 8 /s; response 0.06 s.
- break: 1.5 m for 1.0 s. Throw: 120 N s impulse, at most 8 m/s. Holder mass 90 kg.
- turn: 3 rad/s up to 30 kg, 20 % while touching. View turn share 100 / (100 + mass).
- two hands: 60 % walking speed, no sprint, no jump. Friction 0.5. Sleep below 0.05 m/s for 0.5 s.
