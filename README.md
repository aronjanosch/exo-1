# EXO-1

Working title. A goofy co-op space game in a strange galaxy: walk, fly and land on procedural planets, in small groups of about 2-5 players. Written in Rust with Bevy 0.19 and Avian 0.7 (f64). A private project of the initiator and a few friends.

Status: starting point. The code comes from the spikes (tag `spike/combined-final`, reports in `exo-1-concept`): one planet, walking, a ship with assisted flight, space and back, walking inside a flying ship, LAN co-op with client authority. All speeds, gravity, assists, look and controls are spike test values, **not designed**.

## Start here

1. Clone this repo and `exo-1-concept` side by side.
2. Open this repo in your coding agent and run the skill `exo-onboarding`. It sets up your machine, installs the skills and explains the workflow.
3. Rules for agents: `AGENTS.md`. Vision, decisions and spike reports: `exo-1-concept`.

## Layout

- `crates/planet_core`: planet generator (recipe, bake, height function, chunks), no Bevy types. The bake also routes the rain over the macro grid (#72, `drainage.rs`, mapgen4 style): erosion steps, lakes in deep sinks, river beds cut where enough rain gathers; two macro channels carry the cut and the water surface, so mesh, collision, scatter and the lake and river water agree.
- `crates/flight_core`: assisted-flight ship controller and planet field, no Bevy types.
- `crates/walker_core`: first-person walker with its own move-and-slide over a `World` trait (sweep, depenetrate), no Bevy types.
- `crates/grab_core`: crates and holding them (#80, #81): the crate size table, the hold regulator (force and speed caps by mass, falloff for hands and tool, break timer, throw, shared carry) and the crate body (falls, slides, sleeps; sweeps through a `BoxWorld` trait, lives in a frame like the walker), no Bevy types.
- `crates/warp_core`: planet registry (two planets), quantum drive state machine, speed curve, path and obstruction check, no Bevy types.
- `crates/daynight_core`: time of day (#48, #104): one star per system, the direction to it turns with each static planet's spin; local hour and elevation, light keys by sun elevation, the one light's direction near and far from a planet, no Bevy types.
- `crates/net_core`: snapshot format, interpolation buffer, clock sync, datagrams, replay matrix, no Bevy types and no sockets.
- `crates/exo_app`: the game. Bevy glue: terrain LOD and water, heightfield collision ring, ship body, walker on Avian queries, render origin, camera, HUD, UDP transport, scripted scenarios.
- `art/`: Blender Python scripts, the only source of models (`blender -b -P art/props/props.py -- --out content/props` writes the scatter props as `.glb`).
- `content/`: data and the Bevy asset root (`content/props/`: props from `art/`, `content/shaders/`: terrain and water, `content/look/viewpoints.json`: the look harness's viewpoints). `content/planet/<id>.json` is one surface recipe per planet (unknown fields rejected): macro fields and shape splines, height bands, the landform budget (stamps placed by the bake), biome rows (nearest row in parameter space, with palettes, scatter multipliers and quotas), scatter (storeys, groups, clusters, masks), terrain material, sky and water, drainage (rain, river and lake thresholds, erosion), site kinds with ground edits and kits; `content/system/system.json` the planets (recipe id, seed, radius, quantum travel radii, distance) and the drive settings. `content/daynight/daynight.json` the day and night (#48, all placeholders): per planet the day length and spin axis (sun height and spawn hour follow from the axis and the star's position in `system.json`), the distances where the light slides to the star's true direction, and light archetypes with sun, night light, ambient, sky and fog by the sun's elevation. `content/cargo/crates.json` is the crate size table (edges double from row to row; hands and holders per size), `content/cargo/budget.json` the object budget per category (cap, persistence cap, timeout, distance). `content/tuning/` holds the feel: `ship.json`, `walker.json`, `suit.json` (every field required, unknown fields rejected) and `bindings.json` (which key feeds which action, mouse sensitivities), `grab.json` (hands, grab tool, crate friction and sleep). `camera.json` holds the chase camera (offset, pitch) and its effects (field of view and speed dust by speed, look-ahead into turns, touchdown bump). All are embedded at build time; dev builds also reload them while running: save a file and the change takes effect within a quarter second (a file that does not parse is reported, the old values stay). `--tuning-dir=<dir>` watches another folder.

## Run

```sh
cargo run -p exo_app                                  # play (window)
cargo dev                                             # same with Bevy dynamic linking, about 1 s rebuilds
cargo t                                               # the gate: all checks in parallel with cargo-nextest, includes every scenario headless (dynamic linking, about 2 s rebuilds)
cargo t -P perf                                       # the timing test perf.rs alone, once per round on a quiet machine
cargo scenario                                        # full scenario without a window, exits non-zero on a failed check (dynamic linking)
cargo test --workspace                                # all checks one after the other, static build (about 8 s rebuilds), perf.rs included
cargo run -p exo_app -- --headless --scenario=full    # full scenario headless, static build
cargo run -p exo_app -- --scenario=full               # same in a window, with screenshots
cargo run -p exo_app -- --hidden --scenario=full      # invisible window, screenshots still work
```

A change is done when `cargo t` passes. It needs cargo-nextest: `mise install` in the repo root (pinned in `mise.toml`); `.config/nextest.toml` leaves `perf.rs` out of the default run. Other scenarios with dynamic linking: `cargo dev --headless --scenario=<name>`.

Scenarios (`--scenario=<name>`), reports and screenshots go to `--out=<dir>` (default `target/scenario`):

- `full`: stand, run 20 s, walk up the ramp into the parked ship, take off, fly to space (7000 m), stand and walk in the cabin at 400 m/s rolling, brake, dive back, land, walk out and back in, cabin in atmosphere.
- `walk`: stand still, run 20 s.
- `space`: fly to space, stop, walk out of the ship and drift (stopped ship, one drifting at 3 m/s, a careful step out with taps of W), then the suit: brake, roll, back into the field (righting), fly back into the cabin, drift with a ship coasting at 20 m/s close behind its ramp.
- `foreign`: a remote ship flies through the real snapshot path at 350 m/s; the walker stands and walks in its cabin, then beside it parked. Then #16 without a network: the walker walks beside the landed ship while it carries 1e6 m/s, and stands and walks in its cabin while it ramps up like the quantum drive to 1e6 m/s.
- `t5`: four 300 s walks at 1.8 m/s (basin, escarpment, plateau); slow, for terrain work.
- `warp`: quantum drive between the two planets: refused starts (below 1.5 atmosphere heights, ship on the path), a snapshot with an unknown planet id, calibration lost, cancel, Hearth to Cinder with a walker in the cabin (walking at top speed), landing on Cinder, back seated, an emergency exit at mid-flight and a jump on from the drop point. Checks the end point, the nose at the target's centre and the terrain after the exit. Prints flight times and speeds; with a window it also takes screenshots of the cruise (cabin and outside), the exit and 2 s after it, and checks the new planet's terrain.
- `swap`: three planet swaps by warp (Hearth, Cinder, Hearth, Cinder) and an emergency drop; 30 ticks after each swap it counts what the departed planet left (terrain chunks, entities, meshes, its generator) and the whole world, and how many root chunks the swap frame built (#14, #34). Runs the terrain also headless. Prints the resident memory, not checked.
- `flight`: the flight feel: thrust and rotation ramp to full deflection in the tuned time, the virtual-joystick mouse (full, half, centred, dead zone), the pad's right stick through its dead zone and curve (no device needed), boost raises the speed limit while the capacitor lasts and drops back on release, decoupled (C) blends over 4 s and glides, coupled again damps; camera look-ahead in the turn, wider field of view at speed, a bump on touchdown. With a window it takes screenshots (stick, boost, decoupled, landed, F3). The only scenario on the virtual joystick; the others keep the direct mouse their aiming is written for.
- `boost-hud`: the boost capacitor (#90) and the minimal HUD (#91): seated, climb and cruise, hold boost until the meter is empty (it lasts the drain time, the speed limit drops back although Shift is held), release, the meter refills after the delay in the recharge time, the brake uses no charge; F6 switches to the speed stage (full boost past the drain time, no gauge, `STAGE` in the HUD) and back; at each stage the HUD readout shows the ship's speed and altitude (AGL below `agl_below` of `content/tuning/hud.json`, ALT above, both checked), the gauge, and no debug words.
- `slope-landing`: a ship set down on a slope (#92): finds ground about 5 degrees below `landing_slope_limit` of `ship.json` near the start, lands on it with Ctrl held, rests 10 s and checks that it stays on its touchdown spot (under 1 mm, it used to slide 0.55 m on 33 degrees) and lies on the slope, then thrust up lets go.
- `flight-model` (spike 13): manoeuvres with the axis flight model, each from the same pose 400 m above the ground: W from rest and release, W then X, D, Space, Ctrl, a full right turn at cruise speed, the turn cap switch (F8), the thrust law switch (F7), a brush of the ground at 60 m/s, boost, decoupled glide (C), rolled 90 degrees without input, W in space and decoupled W then X in space (8000 m up), landing from 100 m with Ctrl in landing mode (K). Prints the numbers as a table (also in `<out>/flight-model.txt`); checks the cruise speed, the G-safety turn cap, the thrust law switch (both rules on and back), hold on its side, the landing word on the HUD, touchdown speed, the decoupled glide and a landing without sliding.
- `reload`: edits a copy of the tuning files while running (dev builds): a changed turn rate takes effect, a broken file is refused, the restored file loads again.
- `foreign_warp`: a remote ship at warp speed (1e6 m/s) held next to the walking walker; the walk must be the same as without it (#16).
- `figure`: another player's figure in front of the walker and in the cabin; with `--menu` and a window also screenshots of the menus.
- `planet-look`: the planet look harness (#63): for every planet an atlas (equirectangular height, biome, landform, scatter and water maps) and the bake statistics (land fraction, biome area shares, site gaps, rivers, lakes, drainage time) in `<out>/look/<planet>/`; in a window also a screenshot from each fixed viewpoint of `content/look/viewpoints.json` (orbit, a hill at 200 m, the ground at 1.7 m; at the basin, rim, plateau, a site, a forest edge, the coast, the biggest river, the largest lake), same spots and sun every run. Then the time-of-day shots (`times` in the same file, #48): `noon.png`, `dusk.png` and `night.png` from one viewpoint, with the clock stopped at that time. With `--perf` the frame time per viewpoint. `EXO_LOOK=<planet>[:<viewpoint>,...]` shoots only those (quick iterations).
- `daynight`: on every planet the walker stands at the spawn point while one day runs in 12 s; checks the sun against `daynight_core` every tick, the sun back after a day, the highest and lowest sun, brightness noon > dusk > night > 0 and the night light (#48); then that every planet's lit side agrees with the star direction, near the planet and in space (#104).
- `site-walk`: the walker walks from 40 m outside into a ruin and must stand on its flattened pad (#70).
- `crate-ride`: a test crate on the cabin floor through take-off, flight, a warp to Cinder and the landing (it must stay in the cabin and barely drift); a second crate pushed out over the ramp of the flying ship keeps the ship velocity at the hand-over (#80).
- `interact`: one tap (F) picks up a crate and sits at the seat; the HUD prompt names the target each time (#82).
- `crate-carry`: carries the small crate (sprint allowed) and the medium one (slower, no sprint, no jump) and checks the walking speed, fails to lift the large crate alone, throws the small crate and checks its flight, pulls a medium crate with the grab tool from 8 m (#83).
- `crate-lock`: one crate locked on the cabin's floor plates, one loose off them, one half on them (red); hard acceleration, a strafe and a warp: the locked one does not move, the loose one slides and stays in the cabin; grabbing unlocks, setting down on the plates locks again (#84).
- `crate-budget`: the object budget (#85): 30 crates spawned against a cap of 24 (the longest untouched go), all at rest sleep and cost no steps, a warp to Cinder leaves the persistence cap (4) frozen on Hearth, crates resting untouched on Cinder go after a (shortened) timeout, a crate falling far above everyone goes.
- `net`: the network bot (take off, cruise, turn, brake, land, repeat); see below.

Other options: `--distance=<m>` (distance between the planet centres, default from `content/system/system.json`; frame zones that would reach past half of it shrink to 45 %, a distance too short for the arrival radii is refused), `--origin-shift=<m>` (render-origin threshold, 0 = off, default 1000), `--radius=<m>` (first planet, overrides the file), `--record=<file>` (write the run's ship and walker path for the replay matrix), `--no-vsync` (frame-time measurements; with vsync every frame reads the display's period).

### Performance (`--perf`)

`cargo dev --headless --scenario=full --perf` writes `<out>/perf-full.json`: per scenario phase the simulation step time (p50, p95, max; FixedFirst to FixedLast, physics included), the terrain patch build times, the resident memory (Linux) and, in a window, the frame time (vsync off, screenshot frames left out). Headless runs give step times only; render times need a desktop run with a window.

If a baseline exists, every phase whose step p95 exceeds the baseline's by more than the tolerance fails the scenario (default 50 % plus 0.5 ms, `--perf-tolerance=<share>`). The baseline belongs to one machine: by default it is `target/perf/baseline-<scenario>-<headless|window>.json`, `--perf-baseline=<file>` picks another. To refresh it, run on an idle machine with `--perf-save-baseline` (same command otherwise). `--perf-slow=<ms>` makes every step sleep that long (the test that a slow step fails).

## Co-op (LAN)

Players start in the menu: **Host**, or **Join** with the host's address (type it, `ip:port`) and a slot (2 to 8, one per player), **Settings**, **Quit**. Escape in game opens the pause menu (the world keeps running). Other players show as chunky figures with a name tag ("Pilot <slot>").

Settings (mouse sensitivity, field of view, volume (default 50 %), sound on/off) and rebound keys are saved in `settings/` where the game runs (`settings.json`, `bindings.json`; `--settings-dir=<dir>` picks another). A broken file falls back to the defaults with a message. Scripted and headless runs ignore them.

The flags below skip the menu (scenarios, bots). One player hosts, the others connect. Each simulates its own walker and ship; the host relays snapshots (UDP port 17441).

```sh
cargo run -p exo_app -- --net-host                               # host, slot 1
cargo run -p exo_app -- --net-connect=192.168.1.20:17441 --slot=2 # client, slot 2..8, one slot per player
```

Options: `--port`, `--bind`, `--rate=<Hz>` (default 30), `--buffer=<ms>` (default 150), `--extrapolate=<ms>` (display-only extrapolation during an underrun, default 100, 0 = hold), `--bot` (fly the `net` scenario instead of the keyboard). Ships do not collide with each other; walking on another player's ship works.

## Controls

Walker: mouse look (click to grab, Escape releases), WASD, Shift run, Space jump, F interact: the one verb, the prompt below the screen centre says what it does (sit at the seat, pick up a crate in reach or pull one with the grab tool, set it down), R throw the held crate (a crate set down fully on the cabin's floor plates mag-locks and the plates light up; red plates: it rests partly off them; grabbing unlocks), Q/E turn it (a small crate takes one hand; a medium one both: walking at 60 %, no sprint, no jump; the large one needs two players; heavy crates lag and slow the view), G cabin gravity on/off (only in a landed ship; in flight it is always on), V debug fly mode.
Ship: the mouse is a virtual joystick: moving it puts the marker off centre and the ship turns at that deflection (dead-zone circle in the middle, full at the outer edge; `ship_mode: "direct"` in `bindings.json` turns by mouse movement instead). W/S forward/back, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost (a speed stage, always available), X firm brake, H flight assist, L planet follow, C coupled/decoupled (eases over 4 s; decoupled the ship keeps gliding), F stand up (the same interact key). Set down on ground no steeper than `landing_slope_limit` (`ship.json`), the ship keeps its spot until thrust (any input but down). Input ramps to full deflection over a fraction of a second (`content/tuning/ship.json`).
Suit (outside a ship in space, no gravity): mouse turns freely, W/S forward/back, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost, X brake to rest.
Quantum drive (seated, above 1.5 times the planet's atmosphere height): J spool up and warp to the selected planet, J again cancels while spooling or calibrating, N selects the target. Every planet has a HUD marker with name and distance; the target's is larger and coloured. Point the nose at the ring in the sky while the gauge fills (within 5 degrees; beyond 8 degrees the jump is lost). During the flight, hold J for 1 s to drop out early (emergency exit).
Gamepad or joystick (the first one connected): left stick moves, right stick turns (ship) or looks (on foot), triggers up/down, bumpers roll, left stick click boost/run, A jump, B brake, X warp, Y sit/stand, D-pad assist/follow/decoupled, Select warp target. The mapping is in `bindings.json` (`"Pad:<button>"`, `"pad": "<axis>"`, raw HOTAS axes as `"Axis<n>"`).
Sound: a thrust hum, wind in the atmosphere, a thud on touchdown and a click on toggles, all synthesized in code (`audio.rs`); headless runs have none.
O: orbit camera (debug). F3: debug lines (frame time, chunks, patches, warp, network) under the HUD; the HUD itself shows mode, speed, altitude (AGL: height above the ground below 1000 m, ALT: above the planet's sphere higher up) and the boost gauge: in the ship the capacitor charge (Shift drains it, it refills after a short pause; dimmed below the charge a boost needs to start). F6 (dev switch): boost as the capacitor or the old speed stage; the HUD names the active one next to the bar. The ship flies the axis model (spike 13: limits per axis and direction, decay, precision mode near the ground, G-safety; values in `content/tuning/ship.json`, all placeholders); F3 adds its felt G and limits, F8 switches the G-safety turn cap. F7 switches the two thrust rules together, off by default (#185): the speed cap refuses thrust (steering across the velocity still works) and the brake (X) stops along the velocity, keeping its heading; the HUD shows `thrust old (F7)` or `thrust new (F7)`. K: landing mode (by hand, as in Star Citizen; the HUD shows `LANDING`): near the ground the speed and the descent drop; without it only the descent is held to what the thrust can still stop above the ground.

## Windows build

Linux and Windows are supported; development happens on Linux. Cross build with [llvm-mingw](https://github.com/mstorsjo/llvm-mingw) on `PATH`:

```sh
rustup target add x86_64-pc-windows-gnullvm
cargo build --release -p exo_app --target x86_64-pc-windows-gnullvm
```

Ship `libunwind.dll` from llvm-mingw (`x86_64-w64-mingw32/bin/`) next to the exe. The workflow `.github/workflows/ci.yml` builds it when started by hand (GitHub CI is off otherwise).

## License

MIT, see `LICENSE`. Assets: original or CC0 only. Vendored agent skills: see `.agents/skills/VENDORED.md`.
