# NIGHT-LOG — milestone C night run, 2026-10-09

Morning report of the unattended run (brief: concept repo `docs/RUN-C-NIGHT-BRIEF.md`). Phase 1 on `feat/milestone-c-grab`, phase 2 (extras) on `night/extras`. Nothing merged, no PR, no issue closed.

## Checklist

Phase 1, `feat/milestone-c-grab`:

- [x] #81 `grab_core`: hold, falloff, break, throw, shared carry (test-first)
- [x] #80 crates as data, living in the ship's frame
- [x] #82 interaction: one verb, one prompt
- [x] #83 grab in the game: hands and the grab tool
- [x] #84 lock grid in the cabin
- [x] #85 object budget
- not in this run: #86 (network)

Phase 1 done 2026-10-09 03:2x (all six issues commented, none closed). Gate on the tip of `feat/milestone-c-grab`: `cargo t` exit 0, `cargo scenario` exit 0.

Phase 2, `night/extras`: see the checklist there (branched from the tip of `feat/milestone-c-grab`).

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

## #83 grab in the game: hands and the grab tool

What: `exo_app/src/grab.rs`, `grab_step` between `walker_step` and `crate_step`. The held crate is pulled each step to a hold point in front of the eye through `grab_core::hold_force` (one holder), as an acceleration on the crate in its own frame (cabin or planet; walker and crate may be in different frames). Hands: hold point `hold_gap` + half the crate's depth ahead, lowered by half its height; force only within 2 m. Tool: starts at the grab distance and reels in at 3 m/s to 3 m; full force to 6 m, none at 10 m. The crate keeps its heading relative to the walker's; Q/E turn it (not while the suit rolls); the turn rate is capped by mass and damped on contact. The hold breaks after 1 s with more than 1.5 m error, or when the walker stands on the crate. R throws (hand velocity plus the impulse along the look). Walker side (`walker_step`): two hands give 60 % speed, no sprint, no jump (`WalkInput::slow`, new); the view turns at 100 / (100 + mass) of its rate; weightless (suit), the hold's reaction and the throw's kick push the walker.

Checks, scenario `crate-carry` (new, in `cargo t`), on open ground behind the ship:
- small crate: held and lifted 0.58 m; with W + Shift 11.07 m/s (run speed 12), still in the hands.
- medium crate: held and lifted 0.24 m; with W + Shift 2.57 m/s (60 % of 5 = 3.0), no jump with Space (feet at most 0.04 m up), still held.
- large crate alone: rose -0.001 m in 2.5 s (1500 N per holder < 2354 N weight).
- throw (25 degrees up): 8.00 m/s (want 8.00), landed 6.96 m away after 1.45 s, resting on the ground.
- grab tool from 8.2 m: prompt "[F] pull the medium crate (grab tool)", after 4 s 3.00 m from the eye and 0.90 m above ground.
- 0 failures.

Observation (not changed): walking speeds measured on this ground are 0.4 to 0.9 m/s below the walker's target in both carry states, so the speed check uses an absolute 1 m/s band. The walker without a crate was not measured on this patch; the cause is not checked.

Open points / `TODO(initiator)`:
- The zero-G reaction is covered by the `grab_core` unit test (`reaction_pushes_holder_in_zero_g`) and wired into the suit; no scenario checks it in the game.
- Hold point height (half the crate's height below the eye line) and the tool's reel speed and hold distance (3 m/s, 3 m) are guesses.
- The view turn share applies to mouse and stick alike, also with the tool.
- Shared carry with two players needs #86 (network).

## #84 lock grid in the cabin

What (`exo_app/src/cargo.rs`): a grid of 0.5 m floor plates in the rear part of the cabin (x -1.5..1.5, z -1.0..3.5: 6 x 9 plates, clear of the seat). A crate in the cabin that comes to rest (sleeps) with nobody pushing it and its whole footprint on the plates snaps to them (heading to a quarter turn, edges onto plate lines) and locks: it is not stepped any more (part of the ship), and its plates are lit green. A crate resting partly on the plates does not lock and its plates show red. Grabbing unlocks (`grab_step`). Loose crates in the cabin now feel the ship's acceleration (from its velocity change per step, the part along the floor, capped at 40 m/s²), so they slide when that beats friction. Plate visuals are thin tiles on the ship (dim, green with glow, red with glow).

Three related rules this needed (all `TODO(initiator)`):
- **Ramp field.** The cabin has no rear wall, so hard forward acceleration threw loose crates out over the ramp. In flight (not landed) a loose crate nobody pushes stops at the ramp edge. A carried, pushed or thrown crate can still leave. Landed, nothing stops them.
- **Warp hold covers PostRampDown.** `Phase::holds_ship()` ends before the drive has braked the ship back down; crates in the cabin are now held through `PostRampDown` too.
- **Cabin safety net.** A loose crate is kept inside the side walls, front wall and ceiling (4 mm gap) and above the floor. Before it existed, after a warp a crate pressed to the front wall went through it: a sweep that starts touching a wall ignores that wall (`ignore_origin_penetration`). With the net, corrections over 2 cm (`CargoStats::wall_catches`) were 0 in `crate-lock`.

Checks, scenario `crate-lock` (new, in `cargo t`):
- set down: the crate fully on the plates locks and snaps to (-0.75, 1.25); the one off the plates and the one half on the right edge do not; 1 plate lit, 1 red.
- hard acceleration (6 s boost forward to 197 m/s), firm brake, 3 s hard strafe, firm brake: locked crate drift 0.0000 m; loose crate slid 5.75 m and stayed in the cabin (ramp field stopped it 441 steps).
- fly to space and warp Hearth -> Cinder: locked crate still locked, drift 0.0000 m; loose crate still in the cabin; net corrections over 2 cm: 0.
- landed on Cinder: grabbing unlocks (held, not locked, 0 plates lit); set down again on the plates it locks again.
- 0 failures. `crate-ride` now also locks its test crate (it snapped 0.255 m onto the plates), so from then on it does not move at all (drift 0.000 m through flight, warp and landing). Its pushed crate now leaves while the ship accelerates (W held until the hand-over: with inertia, the ship's braking after W pushed the crate forward against the shove); hand-over at 25.36 m/s ship speed, jump 0.0 m/s.

Open points:
- Locked crates add no mass to the ship; loose crates do not push it.
- The inertia ignores the ship's turning (no centrifugal push) and the vertical part (cabin gravity holds crates down).
- Plate area and size, the 40 m/s² cap and the ramp field are guesses.

## #85 object budget

What: `content/cargo/budget.json` (one row per category; crates only): cap 24, persistence cap 4, timeout 900 s, distance 3000 m. Pure rules in `grab_core::budget::over_budget` (test-first, 4 tests); `cargo::budget_step` runs after `crate_step` and despawns what they return. Rules: held, locked and cabin crates never go. A loose crate resting on the players' planet goes after the timeout untouched; a loose crate still moving goes beyond the distance from every player and ship. Crates on a planet the players left are frozen (not stepped, `CargoStats::frozen`) and only the persistence cap of them stays, most recently touched first. Over the cap the loose ones go, those left behind first, then the longest untouched. Every crate remembers its planet and when it was last touched (grab).

Design gap, my starting rule (`TODO(initiator)`): **the distance rule only removes crates that are moving** (drifting in space, falling). Crates resting on a planet obey the timeout and, once the players leave, the persistence cap. Otherwise every crate left on a planet would go as soon as the players fly 3 km away, and the persistence cap would never apply. Left-behind crates come back to life when the players return to that planet (they are still entities in world space); nothing is saved to disk (#38).

Found and fixed on the way (`grab_core::CrateBody`): crates resting on sloped terrain never slept. Friction only acted on the horizontal velocity, and each tick the ground turned gravity into a 0.07 m/s creep downhill. Friction now works against the floor normal (static up to friction x normal force, then kinetic). New test: on a 20.6 degree slope (friction angle 26.6) a crate creeps 0.000 m and sleeps; on 34.6 degrees it slides 14.3 m in 3 s.

Checks, scenario `crate-budget` (new, in `cargo t`):
- 30 crates spawned against cap 24: 24 alive, the first spawned (longest untouched) went, the last stays.
- all 24 at rest sleep; 0 crate steps in 0.5 s.
- warp Hearth -> Cinder: 4 crates left on Hearth (persistence cap 4), frozen (2804 skipped steps).
- on Cinder with the timeout shortened to 2 s (test hook): three fresh crates there before, gone after; the 4 on Hearth stay.
- a crate placed 3500 m above the walker (falling) goes.
- 0 failures. Numbers are counts only (no frame times on this machine).

Open points: one category (crates); the budget counts only the local player and the own ship as "players and ships" (#86 adds the others).
