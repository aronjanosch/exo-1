# EXO-1

Working title. A goofy co-op space game in a strange galaxy: walk, fly and land on procedural planets, in small groups of about 2-5 players. Written in Rust with Bevy 0.19 and Avian 0.7 (f64). A private project of the initiator and a few friends.

Status: starting point. The code comes from the spikes (tag `spike/combined-final`, reports in `exo-1-concept`): one planet, walking, a ship with assisted flight, space and back, walking inside a flying ship, LAN co-op with client authority. All speeds, gravity, assists, look and controls are spike test values, **not designed**.

## Start here

1. Clone this repo and `exo-1-concept` side by side.
2. Open this repo in your coding agent and run the skill `exo-onboarding`. It sets up your machine, installs the skills and explains the workflow.
3. Rules for agents: `AGENTS.md`. Vision, decisions and spike reports: `exo-1-concept`.

## Layout

- `crates/planet_core`: planet generator (recipe, bake, height function, chunks), no Bevy types.
- `crates/flight_core`: assisted-flight ship controller and planet field, no Bevy types.
- `crates/walker_core`: first-person walker with its own move-and-slide over a `World` trait (sweep, depenetrate), no Bevy types.
- `crates/warp_core`: planet registry (two planets), quantum drive state machine, speed curve, path and obstruction check, no Bevy types.
- `crates/net_core`: snapshot format, interpolation buffer, clock sync, datagrams, replay matrix, no Bevy types and no sockets.
- `crates/exo_app`: the game. Bevy glue: terrain LOD and water, heightfield collision ring, ship body, walker on Avian queries, render origin, camera, HUD, UDP transport, scripted scenarios.
- `content/`: data. `content/planet/<id>.json` is one surface recipe per planet (unknown fields rejected), `content/system/system.json` the planets (recipe id, seed, radius, quantum travel radii, distance) and the drive settings. `content/tuning/` holds the feel: `ship.json`, `walker.json`, `suit.json` (every field required, unknown fields rejected) and `bindings.json` (which key feeds which action, mouse sensitivities). `camera.json` holds the chase camera (offset, pitch) and its effects (field of view and speed dust by speed, look-ahead into turns, touchdown bump). All are embedded at build time; dev builds also reload them while running: save a file and the change takes effect within a quarter second (a file that does not parse is reported, the old values stay). `--tuning-dir=<dir>` watches another folder.

## Run

```sh
cargo run -p exo_app                                  # play (window)
cargo dev                                             # same with Bevy dynamic linking, about 1 s rebuilds
cargo t                                               # all checks, includes the full scenario headless (dynamic linking, about 2 s rebuilds)
cargo scenario                                        # full scenario without a window, exits non-zero on a failed check (dynamic linking)
cargo test --workspace                                # all checks, static build as in CI (about 8 s rebuilds)
cargo run -p exo_app -- --headless --scenario=full    # full scenario headless, static build
cargo run -p exo_app -- --scenario=full               # same in a window, with screenshots
cargo run -p exo_app -- --hidden --scenario=full      # invisible window, screenshots still work
```

A change is done when `cargo t` and `cargo scenario` both pass. Other scenarios with dynamic linking: `cargo dev --headless --scenario=<name>`.

Scenarios (`--scenario=<name>`), reports and screenshots go to `--out=<dir>` (default `target/scenario`):

- `full`: stand, run 20 s, walk up the ramp into the parked ship, take off, fly to space (7000 m), stand and walk in the cabin at 400 m/s rolling, brake, dive back, land, walk out and back in, cabin in atmosphere.
- `walk`: stand still, run 20 s.
- `space`: fly to space, stop, walk out of the ship and drift (stopped ship, one drifting at 3 m/s, a careful step out with taps of W), then the suit: brake, roll, back into the field (righting), fly back into the cabin, drift with a ship coasting at 20 m/s close behind its ramp.
- `foreign`: a remote ship flies through the real snapshot path at 350 m/s; the walker stands and walks in its cabin, then beside it parked.
- `t5`: four 300 s walks at 1.8 m/s (basin, escarpment, plateau); slow, for terrain work.
- `warp`: quantum drive between the two planets: refused starts (below 1.5 atmosphere heights, ship on the path), a snapshot with an unknown planet id, calibration lost, cancel, Hearth to Cinder with a walker in the cabin (walking at top speed), landing on Cinder, back seated, an emergency exit at mid-flight and a jump on from the drop point. Checks the end point, the nose at the target's centre and the terrain after the exit. Prints flight times and speeds; with a window it also takes screenshots of the cruise (cabin and outside), the exit and 2 s after it, and checks the new planet's terrain.
- `flight`: the flight feel: thrust and rotation ramp to full deflection in the tuned time, the virtual-joystick mouse (full, half, centred, dead zone), the pad's right stick through its dead zone and curve (no device needed), boost raises the speed limit and drops back, decoupled (C) blends over 4 s and glides, coupled again damps; camera look-ahead in the turn, wider field of view at speed, a bump on touchdown. With a window it takes screenshots (stick, boost, decoupled, landed, F3). The only scenario on the virtual joystick; the others keep the direct mouse their aiming is written for.
- `reload`: edits a copy of the tuning files while running (dev builds): a changed turn rate takes effect, a broken file is refused, the restored file loads again.
- `foreign_warp`: a remote ship at warp speed (1e6 m/s) held next to the walking walker; the walk must be the same as without it (#16).
- `figure`: another player's figure in front of the walker and in the cabin; with `--menu` and a window also screenshots of the menus.
- `planet-look`: the planet look harness (#63): for every planet an atlas (equirectangular height, biome, landform and scatter maps) and the bake statistics (land fraction, biome area shares, site gaps) in `<out>/look/<planet>/`; in a window also a screenshot from each fixed viewpoint of `content/look/viewpoints.json` (orbit, a hill at 200 m, the ground at 1.7 m; at the basin, rim, plateau, a site, a forest edge, the coast), same spots and sun every run. With `--perf` the frame time per viewpoint.
- `net`: the network bot (take off, cruise, turn, brake, land, repeat); see below.

Other options: `--distance=<m>` (distance between the planet centres, default from `content/system/system.json`; frame zones that would reach past half of it shrink to 45 %, a distance too short for the arrival radii is refused), `--origin-shift=<m>` (render-origin threshold, 0 = off, default 1000), `--radius=<m>` (first planet, overrides the file), `--record=<file>` (write the run's ship and walker path for the replay matrix), `--no-vsync` (frame-time measurements; with vsync every frame reads the display's period).

### Performance (`--perf`)

`cargo dev --headless --scenario=full --perf` writes `<out>/perf-full.json`: per scenario phase the simulation step time (p50, p95, max; FixedFirst to FixedLast, physics included), the terrain patch build times, the resident memory (Linux) and, in a window, the frame time (vsync off, screenshot frames left out). Headless runs give step times only; render times need a desktop run with a window.

If a baseline exists, every phase whose step p95 exceeds the baseline's by more than the tolerance fails the scenario (default 50 % plus 0.5 ms, `--perf-tolerance=<share>`). The baseline belongs to one machine: by default it is `target/perf/baseline-<scenario>-<headless|window>.json`, `--perf-baseline=<file>` picks another. To refresh it, run on an idle machine with `--perf-save-baseline` (same command otherwise). `--perf-slow=<ms>` makes every step sleep that long (the test that a slow step fails).

## Co-op (LAN)

Players start in the menu: **Host**, or **Join** with the host's address (type it, `ip:port`) and a slot (2 to 8, one per player), **Settings**, **Quit**. Escape in game opens the pause menu (the world keeps running). Other players show as chunky figures with a name tag ("Pilot <slot>").

Settings (mouse sensitivity, field of view, volume) and rebound keys are saved in `settings/` where the game runs (`settings.json`, `bindings.json`; `--settings-dir=<dir>` picks another). A broken file falls back to the defaults with a message. Scripted and headless runs ignore them.

The flags below skip the menu (scenarios, bots). One player hosts, the others connect. Each simulates its own walker and ship; the host relays snapshots (UDP port 17441).

```sh
cargo run -p exo_app -- --net-host                               # host, slot 1
cargo run -p exo_app -- --net-connect=192.168.1.20:17441 --slot=2 # client, slot 2..8, one slot per player
```

Options: `--port`, `--bind`, `--rate=<Hz>` (default 30), `--buffer=<ms>` (default 150), `--extrapolate=<ms>` (display-only extrapolation during an underrun, default 100, 0 = hold), `--bot` (fly the `net` scenario instead of the keyboard). Ships do not collide with each other; walking on another player's ship works.

## Controls

Walker: mouse look (click to grab, Escape releases), WASD, Shift run, Space jump, F sit at the seat, G cabin gravity on/off (only in a landed ship; in flight it is always on), V debug fly mode.
Ship: the mouse is a virtual joystick: moving it puts the marker off centre and the ship turns at that deflection (dead-zone circle in the middle, full at the outer edge; `ship_mode: "direct"` in `bindings.json` turns by mouse movement instead). W/S forward/back, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost (a speed stage, always available), X firm brake, H flight assist, L planet follow, C coupled/decoupled (eases over 4 s; decoupled the ship keeps gliding), F stand up. Input ramps to full deflection over a fraction of a second (`content/tuning/ship.json`).
Suit (outside a ship in space, no gravity): mouse turns freely, W/S forward/back, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost, X brake to rest.
Quantum drive (seated, above 1.5 times the planet's atmosphere height): J spool up and warp to the selected planet, J again cancels while spooling or calibrating, N selects the target. Every planet has a HUD marker with name and distance; the target's is larger and coloured. Point the nose at the ring in the sky while the gauge fills (within 5 degrees; beyond 8 degrees the jump is lost). During the flight, hold J for 1 s to drop out early (emergency exit).
Gamepad or joystick (the first one connected): left stick moves, right stick turns (ship) or looks (on foot), triggers up/down, bumpers roll, left stick click boost/run, A jump, B brake, X warp, Y sit/stand, D-pad assist/follow/decoupled, Select warp target. The mapping is in `bindings.json` (`"Pad:<button>"`, `"pad": "<axis>"`, raw HOTAS axes as `"Axis<n>"`).
Sound: a thrust hum, wind in the atmosphere, a thud on touchdown and a click on toggles, all synthesized in code (`audio.rs`); headless runs have none.
O: orbit camera (debug). F3: debug lines (frame time, chunks, patches, warp, network) under the HUD; the HUD itself shows mode, speed, altitude and boost.

## Windows build

Linux and Windows are supported; development happens on Linux. Cross build with [llvm-mingw](https://github.com/mstorsjo/llvm-mingw) on `PATH`:

```sh
rustup target add x86_64-pc-windows-gnullvm
cargo build --release -p exo_app --target x86_64-pc-windows-gnullvm
```

Ship `libunwind.dll` from llvm-mingw (`x86_64-w64-mingw32/bin/`) next to the exe. CI builds it on every push (`.github/workflows/ci.yml`).

## License

MIT, see `LICENSE`. Assets: original or CC0 only. Vendored agent skills: see `.agents/skills/VENDORED.md`.
