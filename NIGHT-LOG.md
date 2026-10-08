# NIGHT-LOG — milestone C night run, 2026-10-09

Morning report of the unattended run (brief: concept repo `docs/RUN-C-NIGHT-BRIEF.md`). Phase 1 on `feat/milestone-c-grab`, phase 2 (extras) on `night/extras`. Nothing merged, no PR, no issue closed.

## Checklist

Phase 1, `feat/milestone-c-grab`:

- [x] #81 `grab_core`: hold, falloff, break, throw, shared carry (test-first)
- [x] #80 crates as data, living in the ship's frame
- [x] #82 interaction: one verb, one prompt
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

## #80 crates as data, living in the ship's frame

What: `exo_app/src/cargo.rs`. A `Crate` component wraps a `grab_core::CrateBody` and a frame (`ship: Option<Entity>`). Crates in a cabin step ship-local in the frame of the cabin colliders (`walker::cabin_frame`, the same one-tick-behind rule as the walker) with the cabin gravity (LAG); on the planet they step in world space with the planet's gravity and a CPU height-function safety net where no collision patch exists. Box sweeps go through Avian shape casts against the world, hulls and ramps. Leaving the cabin (bottom centre out of the cabin box by 0.3 m) or entering it (0.2 m inside) keeps the world pose and hands over the ship velocity (`CrateBody::change_frame`, same rule as the walker). While the warp drive holds the ship (`Phase::holds_ship`), crates in its cabin are not stepped at all: they are held to the ship. A crate at rest sleeps (not stepped until pushed). Rendering: a coloured box with two dark bands per size, riding the ship's interpolated pose. Every normal game and the `full` scenario start with one small test crate on the cabin floor behind the seat.

Checks:
- Scenario `crate-ride` (new, also in `cargo t` as `tests/scenario_crate_ride.rs`): the crate rests and sleeps (5.5 mm settle from its 6 mm spawn gap); take-off to 300 m, a crate pushed out over the ramp while the ship flies at 13.65 m/s: world velocity before and after the hand-over identical (jump 0.0 m/s), 2.48 m/s relative to the ship (the push); fly to space, warp Hearth -> Cinder (1000 km/s top speed, the crate held for 3082 crate steps), landing on Cinder: largest drift of the test crate 0.005 m over 7078 ticks, never outside the cabin. 0 failures.
- `full`: new check, the test crate is still in the cabin after both flights.
- Table test rejects a missing field: `grab_core` `table_rejects_missing_field` (#81 commit).

Open points:
- The walker walks through crates and crates do not touch each other (no collider of their own). Stacking is on the extras list.
- Crates do not tumble and do not push the ship (see "Architecture choice").
- Crates only know the own ship's cabin, not another player's (#86).

## #82 interaction: one verb, one prompt

What: `exo_app/src/interact.rs`. `Tap::Seat` is now `Tap::Interact` (binding `interact`, F and pad North); a new `Tap::Throw` (R, pad right stick click) for #83. Each fixed step the `interaction` system picks the target: stand up when seated, set down when holding, otherwise the crate nearest the centre of the view cone (25 degrees, up to the tool's 10 m), or the seat when the feet are within 1.8 m of it (the old rule). A crate in reach of the hands wins over the seat; a crate only in the tool's reach does not (so F at the seat still sits even with a crate 4 m behind). The prompt ("[F] sit", "[F] pick up the small crate", "[F] pull the medium crate (grab tool)", "[F] set the small crate down  [R] throw", "[F] stand up") is a resource the HUD shows on its own line below the screen centre; the key label comes from the bindings. The sit and stand code moved out of `walker_step` unchanged.

Old player files (`settings/bindings.json` with `seat` and no `throw`) still load: `seat` is read as `interact`, a missing `throw` gets R.

Checks:
- Scenario `interact` (new, in `cargo t`): prompt "[F] pick up the small crate" with the crate targeted by the hands; F picks it up (prompt "[F] set the small crate down  [R] throw"); F sets it down; walking to the seat the prompt is "[F] sit"; the same F sits (prompt "[F] stand up"); targets used: crate, drop, seat; F stands up. 7 checks, 0 failures.
- Unit test `old_seat_binding_still_loads`.
- Existing scenarios: see the gate below.

`TODO(initiator)`: cone half angle 25 degrees (`grab.json`); the seat by proximity instead of by the cone (keeps every scripted F at the seat working; with the cone the walker has to look at the seat). The HUD check reads the prompt resource, which the HUD line shows as is (headless runs have no HUD to read).
