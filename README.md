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
- `content/`: data and the Bevy asset root (`content/props/`: props from `art/`, `content/shaders/`: terrain and water, `content/look/viewpoints.json`: the look harness's viewpoints). `content/planet/<id>.json` is one surface recipe per planet (unknown fields rejected): macro fields and shape splines, height bands, the landform budget (stamps placed by the bake), biome rows (nearest row in parameter space, with palettes, scatter multipliers and quotas), scatter (storeys, groups, clusters, masks), terrain material, sky and water, drainage (rain, river and lake thresholds, erosion), site kinds with ground edits and kits; `content/system/system.json` the planets (recipe id, seed, radius, quantum travel radii, distance) and the drive settings. `content/daynight/daynight.json` the day and night (#48, all placeholders): per planet the day length and spin axis (sun height and spawn hour follow from the axis and the star's position in `system.json`), the distances where the light slides to the star's true direction, and light archetypes with sun, night light, ambient, sky and fog by the sun's elevation. `content/cargo/crates.json` is the crate size table (edges double from row to row; hands and holders per size), `content/cargo/budget.json` the object budget per category (cap, persistence cap, timeout, distance). `content/tuning/` holds the feel: `ground.json` (the ground rules, #92), `sc_*.json` (the SC flight model), `walker.json`, `suit.json` (every field required, unknown fields rejected) and `bindings.json` (which key feeds which action, mouse sensitivities), `grab.json` (hands, grab tool, crate friction and sleep). `camera.json` holds the chase camera (offset, pitch) and its effects (field of view and speed dust by speed, look-ahead into turns, touchdown bump). All are embedded at build time; dev builds also reload them while running: save a file and the change takes effect within a quarter second (a file that does not parse is reported, the old values stay). `--tuning-dir=<dir>` watches another folder.

## Run

```sh
cargo run -p exo_app                                  # play (window)
cargo dev                                             # same with Bevy dynamic linking, about 1 s rebuilds
scripts/gate                                          # the gate: cargo t at low priority, one at a time on the machine
cargo t                                               # the same without the lock: all checks in parallel with cargo-nextest, every scenario headless (dynamic linking)
cargo ts scenario_warp                                # one app test by name, dynamic like the gate
cargo t -P perf                                       # the timing test perf.rs alone, once per round on a quiet machine
cargo scenario                                        # full scenario without a window, exits non-zero on a failed check (dynamic linking)
cargo check --workspace --all-targets                 # the static build compiles (no linking, no test binaries)
cargo run -p exo_app -- --headless --scenario=full    # full scenario headless, static build
cargo run -p exo_app -- --scenario=full               # same in a window, with screenshots
cargo run -p exo_app -- --hidden --scenario=full      # invisible window, screenshots still work
```

A feature is done when its own tests pass (`cargo test -p <crate>`, `cargo ts scenario_<name>`); the full gate runs at the end of a big round (`AGENTS.md`, three test sizes). It needs cargo-nextest: `mise install` in the repo root (pinned in `mise.toml`); `.config/nextest.toml` leaves `perf.rs` out of the default run. Other scenarios with dynamic linking: `cargo dev --headless --scenario=<name>`.

Use `cargo dev` and `cargo t` for daily work to share Bevy between executables. Dev and
test builds keep only debug line tables, including our own crates; backtraces retain
source locations, but inspecting local variables needs an explicit debug-info override.
Keep the build cache between runs: cleaning it repeatedly forces dependencies to be
compiled and written again. After large profile or toolchain changes, a one-time
`cargo clean` in the intended `CARGO_TARGET_DIR` removes obsolete variants; do it only
when no build is using that directory. Release builds strip symbols for distribution.

Scenarios (`--scenario=<name>`), reports and screenshots go to `--out=<dir>` (default `target/scenario`):

- `full`: stand, run 20 s, walk up the ramp into the parked ship, take off, fly to space (7000 m), stand and walk in the cabin at 400 m/s rolling, brake, dive back, land, walk out and back in, cabin in atmosphere.
- `walk`: stand still, run 20 s.
- `space`: fly to space, stop, walk out of the ship and drift (stopped ship, one drifting at 3 m/s, a careful step out with taps of W), then the suit: brake, roll, back into the field (righting), fly back into the cabin, drift with a ship coasting at 20 m/s close behind its ramp.
- `foreign`: a remote ship flies through the real snapshot path at 350 m/s; the walker stands and walks in its cabin, then beside it parked. Then #16 without a network: the walker walks beside the landed ship while it carries 1e6 m/s, and stands and walks in its cabin while it ramps up like the quantum drive to 1e6 m/s.
- `t5`: four 300 s walks at 1.8 m/s (basin, escarpment, plateau); slow, for terrain work.
- `warp`: quantum drive between the two planets: refused starts (below 1.5 atmosphere heights, ship on the path), a snapshot with an unknown planet id, calibration lost, cancel, Hearth to Cinder with a walker in the cabin (walking at top speed), landing on Cinder, back seated, an emergency exit at mid-flight and a jump on from the drop point. Checks the end point, the nose at the target's centre and the terrain after the exit. Prints flight times and speeds; with a window it also takes screenshots of the cruise (cabin and outside), the exit and 2 s after it, and checks the new planet's terrain.
- `swap`: three planet swaps by warp (Hearth, Cinder, Hearth, Cinder) and an emergency drop; 30 ticks after each swap it counts what the departed planet left (terrain chunks, entities, meshes, its generator) and the whole world, and how many root chunks the swap frame built (#14, #34). Runs the terrain also headless. Prints the resident memory, not checked.
- `flight`: the flight feel: thrust and rotation ramp to full deflection in the tuned time, the virtual-joystick mouse (full, half, centred, dead zone), the pad's right stick through its dead zone and curve (no device needed), boost raises the speed limit while the capacitor lasts and drops back on release, decoupled (C) blends over 4 s and glides, coupled again damps; camera look-ahead in the turn, wider field of view at speed, a bump on touchdown. With a window it takes screenshots (stick, boost, decoupled, landed, F3). The only scenario on the virtual joystick; the others keep the direct mouse their aiming is written for.
- `boost-hud`: the boost capacitor (#90) and the minimal HUD (#91): seated, climb and cruise, hold boost until the meter is empty (it lasts the drain time, the speed limit drops back although Shift is held), release, the meter refills after the delay in the recharge time, the brake uses no charge; at each step the HUD readout shows the ship's speed and altitude (AGL below `agl_below` of `content/tuning/hud.json`, ALT above, both checked), the gauge, and no debug words.
- `thruster-audio` (#150): the thruster sound layers (`flight_core::audio`) from the controls: a strafe right for 1 s makes the +x hiss the largest; a boost for 2 s raises the roar above 0.5 and counts one boost start; after release and 3 s of rest the hiss and roar are below 0.05 and the rumble is the idle level.
- `slope-landing`: a ship set down on a slope (#92): finds ground about 5 degrees below `landing_slope_limit` of `ground.json` near the start, lands on it with Ctrl held, rests 10 s and checks that it stays on its touchdown spot (under 1 mm, it used to slide 0.55 m on 33 degrees) and lies on the slope, then thrust up lets go.
- `sc-lift` (round 5, renamed in #206 from `sc-switch`): the ship lifts off more than 5 m in 2 s of Space, drifts less than 1 m in a 3 s hover, the HUD mode starts with `SHIP SC`, 3 s of W give more than 20 m/s forward.
- `sc-turn` (#196): the SC model's rotation: a full pitch overshoots its target rate and settles, a reversal decelerates at a constant rate, a roll release stops without overshoot, F8 (G-safe) caps a turn at speed.
- `sc-hud` (#197): the flight panel: C toasts `DECOUPLED` and shows the 4 s blend, H `GRAV COMP OFF`, B `NAV`, Page Down `LIMIT 90 %`, X the `BRAKE` badge (no toast).
- `sc-flight-hud` (#200): the flight HUD: sit, lift; D strafes right (velocity +x, right thrust bar longest), W fills the speed tape, Page Down x5 puts the limiter mark at 50 %, the horizon is level after release.
- `sc-linear` (#195): the SC model's linear law at 400 m: a strafe, the boost cap refusing thrust, W+D then X (the heading holds), gravity compensation off (falls) and on (holds), NAV above the SCM cap, the speed limiter.
- `sc-air` (#199): the SC model's air: a hover drifts with the wind with wind compensation (I) off and holds with it on, turbulence low and fast, none high up.
- `sc-body` (#198): the SC model's thrusters: W from a hover reaches half thrust after the spool and jerk (about 0.55 s), a boost reaches full strength after its pre-delay and ramp.
- `help` (round 5): F1 shows the walking keys on foot, the ship's switches seated, F1 again hides the panel.
- `camera-g` (#148, #149): boost for 2 s from 300 m: trauma above 0.3, the chase camera trails behind the ship within `lag_max`, forward G widens the view; braking to a stop and F9 (through the bindings) off: no shake, lag or G field of view, and a boost stays shake-free; F9 on, landing: trauma and lag back to rest values (below 0.05 and 1 cm).
- `reload`: edits a copy of the tuning files while running (dev builds): a changed turn rate takes effect, a broken file is refused, the restored file loads again.
- `foreign_warp`: a remote ship at warp speed (1e6 m/s) held next to the walking walker; the walk must be the same as without it (#16).
- `figure`: another player's figure in front of the walker and in the cabin; with `--menu` and a window also screenshots of the menus.
- `planet-look`: the planet look harness (#63): for every planet an atlas (equirectangular height, biome, landform, scatter and water maps) and the bake statistics (land fraction, biome area shares, site gaps, rivers, lakes, drainage time) in `<out>/look/<planet>/`; in a window also a screenshot from each fixed viewpoint of `content/look/viewpoints.json` (orbit, a hill at 200 m, the ground at 1.7 m; at the basin, rim, plateau, a site, a forest edge, the coast, the biggest river, the largest lake), same spots and sun every run. Then the time-of-day shots (`times` in the same file, #48): `noon.png`, `dusk.png` and `night.png` from one viewpoint, with the clock stopped at that time. With `--perf` the frame time per viewpoint. `EXO_LOOK=<planet>[:<viewpoint>,...]` shoots only those (quick iterations).
- `daynight`: on every planet the walker stands at the spawn point while one day runs in 12 s; checks the sun against `daynight_core` every tick, the sun back after a day, the highest and lowest sun, brightness noon > dusk > night > 0 and the night light (#48); then that every planet's lit side agrees with the star direction, near the planet and in space (#104).
- `site-walk`: the walker walks from 40 m outside into a ruin and must stand on its flattened pad (#70).
- `crate-ride`: a test crate on the cabin floor through take-off, flight, a warp to Cinder and the landing (it must stay in the cabin and barely drift); a second crate pushed out over the ramp of the flying ship keeps the ship velocity at the hand-over (#80).
- `interact`: one tap (F) picks up a crate and sits at the seat; the HUD prompt names the target each time (#82).
- `crate-carry`: carries the small crate (sprint allowed) and the medium one (slower, no sprint, no jump) and checks the walking speed, fails to lift the large crate alone, throws the small crate and checks its flight, pulls a medium crate with the grab tool from 8 m (#83); outside the crates are Avian bodies (#210), so it also checks the hold error and the condition lost in a 3 m drop.
- `crate-lock`: one crate locked on the cabin's floor plates, one loose off them, one half on them (red); hard acceleration, a strafe and a warp: the locked one does not move, the loose one slides and stays in the cabin; grabbing unlocks, setting down on the plates locks again (#84).
- `crate-unload`: carries a crate off the plates, down the ramp and out, sets it down, and carries it back up to lock again; checks the handover to an Avian body at the ramp's end does not jump (#210).
- `crate-ramp`: a crate on the parked ship's ramp stays a sweep crate (the ramp counts as ship) and stays on the ground when the ship takes off (#210).
- `crate-hull`: a crate thrown at the parked ship's side wall stops outside the hull, never in the cabin (#210).
- `crate-wake-stream`: crates 500 m away are frozen; the walker comes, the patch is built and they wake as Avian bodies with no drop below the ground (#210).
- `crate-budget`: the object budget (#85): 30 crates spawned against a cap of 24 (the longest untouched go), all at rest sleep and cost no steps, a warp to Cinder leaves the persistence cap (4) frozen on Hearth, crates resting untouched on Cinder go after a (shortened) timeout, a crate falling far above everyone goes.
- `deliver` and `courier`: the first rounds of the loop, headless, with the counter, briefings, standing and the notices of the ritual; `courier` walks parcels by hand from Drip Rock to the small drops 150 to 400 m away.
- `customers`: a customer's order becomes an offer at the wholesaler's counter, the delivery pleases or lets the customer down, relationship and next order follow (#168).
- `licence`: the seat refuses without the flight licence, the exam at the flight school's counter (fee, ship lent, the exam's events set by test hooks), pass with honours, the seat works (#169).
- `map`: the pins (places, the tracked job's next stop, the player), M opens and closes the map; with a window a screenshot of it (#166).
- `first-person-settings`: mouse look at two sensitivities outside and in the cabin, sit/stand through F, and vehicle mouse/camera isolation. Headless includes the real camera projection system.
- `net`: the network bot (take off, cruise, turn, brake, land, repeat); see below.

Other options: `--distance=<m>` (distance between the planet centres, default from `content/system/system.json`; frame zones that would reach past half of it shrink to 45 %, a distance too short for the arrival radii is refused), `--origin-shift=<m>` (render-origin threshold, 0 = off, default 1000), `--radius=<m>` (first planet, overrides the file), `--record=<file>` (write the run's ship and walker path for the replay matrix), `--no-vsync` (frame-time measurements; with vsync every frame reads the display's period).

### Performance (`--perf`)

`cargo dev --headless --scenario=full --perf` writes `<out>/perf-full.json`: per scenario phase the simulation step time (p50, p95, max; FixedFirst to FixedLast, physics included), the terrain patch build times, the resident memory (Linux) and, in a window, the frame time (vsync off, screenshot frames left out). Headless runs give step times only; render times need a desktop run with a window.

If a baseline exists, every phase whose step p95 exceeds the baseline's by more than the tolerance fails the scenario (default 50 % plus 0.5 ms, `--perf-tolerance=<share>`). The baseline belongs to one machine: by default it is `target/perf/baseline-<scenario>-<headless|window>.json`, `--perf-baseline=<file>` picks another. To refresh it, run on an idle machine with `--perf-save-baseline` (same command otherwise). `--perf-slow=<ms>` makes every step sleep that long (the test that a slow step fails).

## Co-op (LAN)

Players start in the menu: **Host**, or **Join** with the host's address (type it, `ip:port`) and a slot (2 to 8, one per player), **Settings**, **Quit**. Escape in game opens the pause menu (the world keeps running). Other players show as chunky figures with a name tag ("Pilot <slot>").

Settings (mouse sensitivity, field of view, volume (default 50 %), sound on/off) and rebound keys are saved in `settings/` where the game runs (`settings.json`, `bindings.json`; `--settings-dir=<dir>` picks another). A broken file falls back to the defaults with a message. Scripted and headless runs ignore them.

The settings menu opens on **Sound** (volume, sound on/off). Tabs at the top group the other settings: **Display** (vertical FOV, horizontal equivalents, camera shake), **Controls** (mouse sensitivity), and **Keybinds** (all key assignments). Changing tabs cancels an unconfirmed numeric edit or pending key rebind.

First-person settings apply on foot, including inside a ship, and never while steering a vehicle. **Vertical FOV** is 40–90° (default 75°), with horizontal equivalents for 4:3, 16:9 and 21:9 shown live. **Mouse sensitivity** is 0.10–10.00 (default 1.00); yaw and pitch both use 0.022° per mouse count times sensitivity, independent of FOV and frame time. Drag either slider or click its number to type (dot or comma decimal separator). Enter confirms, Escape cancels; arrow keys, Home/End, Backspace/Delete and Ctrl+A edit the number. Slider changes apply immediately and save on release; typed values apply and save on confirmation. Ship steering and its camera keep their own tuning in `content/tuning/`.

Mouse look uses relative raw-device motion (Windows `WM_INPUT`), applied once per rendered frame before physics. The first-person camera uses the current look rotation without mouse smoothing or fixed-tick rotation interpolation, including frames with no physics tick. Position and the carrying ship still interpolate; horizon transitions, camera effects and held-crate turn resistance retain their existing behaviour. Scripted mouse input stays on the deterministic fixed-tick path.

The flags below skip the menu (scenarios, bots). One player hosts, the others connect. Each simulates its own walker and ship; the host relays snapshots (UDP port 17441).

```sh
cargo run -p exo_app -- --net-host                               # host, slot 1
cargo run -p exo_app -- --net-connect=192.168.1.20:17441 --slot=2 # client, slot 2..8, one slot per player
```

Options: `--port`, `--bind`, `--rate=<Hz>` (default 30), `--buffer=<ms>` (default 150), `--extrapolate=<ms>` (display-only extrapolation during an underrun, default 100, 0 = hold), `--bot` (fly the `net` scenario instead of the keyboard). Ships do not collide with each other; walking on another player's ship works.

## Controls

All keys at a glance, sorted into game and debug: [`KEYS.md`](KEYS.md). In the game F1 shows the keys that apply right now.

Walker: mouse look (click to grab, Escape releases), WASD, Shift run, Space jump, F interact: the one verb, the prompt below the screen centre says what it does (sit at the seat, pick up a crate in reach or pull one with the grab tool, set it down), R throw the held crate (a crate set down fully on the cabin's floor plates mag-locks and the plates light up; red plates: it rests partly off them; grabbing unlocks), Q/E turn it (a small crate takes one hand; a medium one both: walking at 60 %, no sprint, no jump; the large one needs two players; heavy crates lag and slow the view), G cabin gravity on/off (only in a landed ship; in flight it is always on), V debug fly mode.
Ship: the mouse is a virtual joystick: moving it puts the marker off centre and the ship turns at that deflection (dead-zone circle in the middle, full at the outer edge; `ship_mode: "direct"` in `bindings.json` turns by mouse movement instead). W/S forward/back, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost (the capacitor), X firm brake, H gravity compensation, C coupled/decoupled (eases over 4 s; decoupled the ship keeps gliding), F stand up (the same interact key). Set down on ground no steeper than `landing_slope_limit` (`ground.json`), the ship keeps its spot until thrust (any input but down).
Suit (outside a ship in space, no gravity): mouse turns freely, W/S forward/back, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost, X brake to rest.
Quantum drive (seated, above 1.5 times the planet's atmosphere height): J spool up and warp to the selected planet, J again cancels while spooling or calibrating, N selects the target. Every planet has a HUD marker with name and distance; the target's is larger and coloured. Point the nose at the ring in the sky while the gauge fills (within 5 degrees; beyond 8 degrees the jump is lost). During the flight, hold J for 1 s to drop out early (emergency exit).
Gamepad or joystick (the first one connected): left stick moves, right stick turns (ship) or looks (on foot), triggers up/down, bumpers roll, left stick click boost/run, A jump, B brake, X warp, Y sit/stand, D-pad assist/decoupled, Select warp target. The mapping is in `bindings.json` (`"Pad:<button>"`, `"pad": "<axis>"`, raw HOTAS axes as `"Axis<n>"`).
Sound: a thrust hum, wind in the atmosphere, a thud on touchdown and a click on toggles, all synthesized in code (`audio.rs`); headless runs have none.
O: orbit camera (debug). F3: debug lines (frame time, chunks, patches, warp, network) under the HUD; the HUD itself shows mode, speed, altitude (AGL: height above the ground below 1000 m, ALT: above the planet's sphere higher up) and the boost gauge: in the ship the capacitor charge (Shift drains it, it refills after a short pause; dimmed below the charge a boost needs to start). The ship flies the SC model (`flight_core::sc`, values in `content/tuning/sc_*.json`, all placeholders; the HUD mode `SHIP SC`; F3 shows its felt G, saturation and turn cap): thrust as a force on the ship's mass, and Star Citizen's flight switches: H gravity compensation, C coupled or decoupled, F8 G-safe, K landing, B master mode SCM or NAV, U comstab, P proximity assist, I wind compensation, Page Up and Page Down the speed limiter (keys TODO(initiator)). Until #206 the axis model flew beside it (F7); it is gone, and so are its keys (F6, F7, L). Set down on ground (`ground.json`, #92) the ground rules keep the ship on its spot; the SC model has no landing gear yet (#162). F9 switches the camera effects (trauma shake under boost and touchdown, spring lag of the chase camera, FOV from forward G; `camera_shake` in the settings scales the shake; values in `content/tuning/camera.json`, all placeholders). K: landing mode (by hand, as in Star Citizen; the HUD shows `LANDING`): near the ground the speed and the descent drop; without it only the descent is held to what the thrust can still stop above the ground.

## Windows build

Linux and Windows are supported; development happens on Linux. Cross build with [llvm-mingw](https://github.com/mstorsjo/llvm-mingw) on `PATH`:

```sh
rustup target add x86_64-pc-windows-gnullvm
cargo build --release -p exo_app --target x86_64-pc-windows-gnullvm
```

Ship `libunwind.dll` from llvm-mingw (`x86_64-w64-mingw32/bin/`) next to the exe. The workflow `.github/workflows/ci.yml` builds it when started by hand (GitHub CI is off otherwise).

## License

MIT, see `LICENSE`. Assets: original or CC0 only. Vendored agent skills: see `.agents/skills/VENDORED.md`.
