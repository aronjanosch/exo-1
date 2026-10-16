# EXO-1

Working title. A co-op crew game in space: friends run one walkable ship together, each in a role, through a strange galaxy, and fly and land on procedural planets, in crews of about 2-5 players. The target is version 0.1 (`docs/VISION.md` and `docs/ROADMAP.md` in `exo-1-concept`). Written in Rust with Bevy 0.19 and Avian 0.7 (f64). Built by the initiator and a few friends; code MIT, shipped assets CC0. The repo is public for a protected `main` and not announced yet. Vision, decisions and research live in the private repo `exo-1-concept`.

Status: turning towards version 0.1 of the crew ship. Flight, planets, walking inside a flying ship, crates and LAN co-op come from the spikes and the rounds so far. The crime-empire direction (loop D, round 6 smuggling run) is gone from the code; the state before it is the tag `archive/crime-era`. What stays is reworked for the crew ship: `production_core` becomes the scientist's lab, `jobs_core` and `gameplay_core` the captain's missions. Their data still holds leftovers of that direction (courier and family job templates, givers, unlocks, texts, lab content).

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
- `crates/net_core`: snapshot format, interpolation buffer, clock sync, datagrams, replay matrix, no Bevy types and no sockets. For items (C2): `reliable` (a reliable, ordered message channel over datagrams: sequence numbers, acks piggybacked with a bit mask, resend on timeout and on a hole, duplicates dropped, big messages in frames) and `loose` (poses of moving loose items as datagrams, and their interpolation track); both are tested under `link`'s delay, jitter and loss.
- `crates/gameplay_core`: the kernel every gameplay system builds on (domain events, shared records, conditions, notices, save envelope, progress tracks), no Bevy types. `crates/jobs_core`: jobs, templates, givers, offers, objectives and licences. `crates/production_core`: the lab (mixing, stations, recipe book, ingredients). No Bevy types.
- `crates/display_core`: display panels: widgets (label, button, bar, toggle, slider, dial, trend, list, rows and columns) as plain data, screen tiers (S, M, L) on a plate with a header, a stack layout, the pointer's hover and clicks with action ids, the camera's view ray through the cursor onto a display's quad in the world and from there to panel pixels, and the look as data (`content/ui/displays.json`: colours, fonts, strokes, world redraw rate and texture scale, from the design system "EXO-1 Ship Displays"; dev builds reload it while running), no Bevy types. Fonts in `content/fonts/` (Chakra Petch, Share Tech Mono; SIL OFL).
- `crates/item_core`: items and the path every change takes, no Bevy types: item definitions from `content/items/*.json` (built from components: collected or carried, hands, stack size, own slots, use effect, shape), containers with slot rules (from the dev-area round), the action bar (eight slots, the selected one in the hand, two hands shared with carried cargo), and the request path: a client shows its request at once and sends it to the host, which owns loose items and every inventory and answers with events; a rejected request rolls back. Messages are serde data (`wire`) for the network: C2 (`exo_app/src/items_net.rs`) sends requests and events over `net_core`'s reliable channel, the host (slot 1) owns the items, a late joiner gets the state as events, and loose items send their poses only while they move (an `Event::Rest` says where one came to rest).
- `crates/exo_app`: the game. Bevy glue: terrain LOD and water, heightfield collision ring, ship body, walker on Avian queries, render origin, camera, HUD, UDP transport, scripted scenarios.
- `art/`: Blender Python scripts, the only source of models (`blender -b -P art/props/props.py -- --out content/props` writes the scatter props as `.glb`).
- `content/`: data and the Bevy asset root (`content/props/`: props from `art/`, `content/shaders/`: terrain and water, `content/look/viewpoints.json`: the look harness's viewpoints). `content/planet/<id>.json` is one surface recipe per archetype (unknown fields rejected; `variation` holds the ranges the planet's seed rolls its climate, sea share, moisture and palette in, `content/planet/draft/` proposed archetypes that are not in the planet set): macro fields and shape splines, height bands, the landform budget (stamps placed by the bake), biome rows (nearest row in parameter space, with palettes, scatter multipliers and quotas), scatter (storeys, groups, clusters, masks), terrain material, sky and water, drainage (rain, river and lake thresholds, erosion), site kinds with ground edits and kits; `content/system/system.json` the planets (recipe id, seed, radius, quantum travel radii, distance) and the drive settings. `content/daynight/daynight.json` the day and night (#48, all placeholders): per planet the day length and spin axis (sun height and spawn hour follow from the axis and the star's position in `system.json`), the distances where the light slides to the star's true direction, and light archetypes with sun, night light, ambient, sky and fog by the sun's elevation. `content/cargo/crates.json` is the crate size table (edges double from row to row; hands and holders per size), `content/cargo/budget.json` the object budget per category (cap, persistence cap, timeout, distance). `content/tuning/` holds the general feel: `ground.json` (the ground rules, #92), `walker.json`, `suit.json` (every field required, unknown fields rejected) and `bindings.json` (which key feeds which action, mouse sensitivities), `grab.json` (hands, grab tool, crate friction and sleep). `camera.json` holds the chase camera (offset, pitch) and its effects (field of view and speed dust by speed, look-ahead into turns, touchdown bump). Ship-specific values live in `content/ship/<type>/`: **Kackfass-1** (`kackfass_1`) has `type.json` (ID, name, hull layout) and six `sc_*.json` flight files; see `content/ship/README.md`. All are embedded at build time; dev builds also reload them while running: save a file and the change takes effect within a quarter second (a file that does not parse is reported, the old values stay). `--tuning-dir=<dir>` watches another content-root-shaped folder (`tuning/` and `ship/kackfass_1/`).

## Run

```sh
cargo run -p exo_app                                  # play (window)
cargo dev                                             # same with Bevy dynamic linking, about 1 s rebuilds
scripts/gate                                          # the gate: cargo t at low priority, one at a time on the machine
cargo t                                               # the same without the lock: all checks in parallel with cargo-nextest, every scenario headless (dynamic linking)
cargo ts scenario_swap                                # one app test by name, dynamic like the gate
cargo t -P perf                                       # the timing test perf.rs alone, once per round on a quiet machine
cargo scenario                                        # full scenario without a window, exits non-zero on a failed check (dynamic linking)
cargo check --workspace --all-targets                 # the static build compiles (no linking, no test binaries)
cargo run -p exo_app -- --headless --scenario=full    # full scenario headless, static build
cargo run -p exo_app -- --scenario=full               # same in a window, with screenshots
cargo run -p exo_app -- --hidden --scenario=full      # invisible window, screenshots still work
```

A feature is done when its own tests pass (`cargo test -p <crate>`, and its scenario if it has one: `cargo ts scenario_<name>`); the full gate runs at the end of a big round (`AGENTS.md`, three test sizes). It needs cargo-nextest: `mise install` in the repo root (pinned in `mise.toml`); `.config/nextest.toml` leaves `perf.rs` out of the default run. Other scenarios with dynamic linking: `cargo dev --headless --scenario=<name>`.

Use `cargo dev` and `cargo t` for daily work to share Bevy between executables. Dev and
test builds keep only debug line tables, including our own crates; backtraces retain
source locations, but inspecting local variables needs an explicit debug-info override.
Keep the build cache between runs: cleaning it repeatedly forces dependencies to be
compiled and written again. After large profile or toolchain changes, a one-time
`cargo clean` in the intended `CARGO_TARGET_DIR` removes obsolete variants; do it only
when no build is using that directory. Release builds strip symbols for distribution.

Scenarios (`--scenario=<name>`), reports and screenshots go to `--out=<dir>` (default `target/scenario`). Only the names the dispatch in `crates/exo_app/src/scenario/mod.rs` knows:

| Name | Purpose in one sentence |
| --- | --- |
| `walk` | Walker stands still 5 s, then runs 20 s. |
| `net` | Network bot: take off, cruise, turn, brake, land, repeat (see below). |
| `foreign` | A remote ship flies through the snapshot path; the walker stands and walks beside and in it, also at 1e6 m/s. |
| `foreign_warp` | A remote ship at warp speed next to the walker; the walk must match the run without it (#16). |
| `figure` | Another player's figure in front of the walker and in the cabin; with `--menu` and a window also menu screenshots. |
| `flight` | The flight feel: thrust and rotation ramps, virtual-joystick mouse and pad stick, boost, decoupled glide, camera and touchdown. |
| `first-person-settings` | Mouse look at two sensitivities outside and in the cabin, sit/stand, vehicle mouse and camera isolation. |
| `boost-hud` | The boost capacitor drains, refills after the delay, and the minimal HUD readout stays correct. |
| `help` | F1 shows the walking keys on foot and the ship's switches seated; F1 again hides the panel. |
| `dev-menu` | The F10 dev menu's keys, from a new player's start. |
| `sc-linear` | The SC model's linear law at 400 m: strafe, boost cap, gravity compensation, NAV, speed limiter. |
| `sc-flight-hud` | The flight HUD: lift, strafe, speed tape, limiter mark, level horizon after release. |
| `camera-g` | Boost shake and lag of the chase camera, the forward G field of view, and their rest values after landing. |
| `slope-landing` | A ship set down on a slope stays where it touched down and lies on the slope. |
| `reload` | Edited tuning copies while running (dev builds): a changed value takes effect, a broken file is refused. |
| `warp` | Quantum drive between the two planets: refused starts, unknown planet id, cancel, flight, landing, emergency exit. |
| `planet-flight` | A repeatable flight on a big planet for frame spikes; reports p99 and worst frame and LOD debt. |
| `lod-morph` | A slow descent on a big planet with geomorphing, to check the LOD morph and its frame goal. |
| `swap` | Three planet swaps by warp and an emergency drop; the departed planet leaves nothing behind and the swap frame builds its root chunks. |
| `crate-ride` | A crate on the cabin floor through take-off, flight, warp and landing stays put; a pushed crate keeps the ship velocity. |
| `interact` | One tap (F) picks up a crate and sits at the seat; the HUD prompt names the target. |
| `crate-carry` | Carrying by crate size (walking speed, no lift for the large one, grab tool, drop, throw). |
| `crate-lock` | Crates on the cabin plates lock and stay put in flight; grabbing unlocks, setting down locks again. |
| `crate-budget` | The object budget: spawn past the cap, sleeping crates cost nothing, persistence cap and timeouts hold. |
| `crate-unload` | A crate carried off the plates, down the ramp and out hands over to an Avian body without a jump. |
| `crate-ramp` | A crate on the parked ship's ramp stays a sweep crate and on the ground when the ship takes off. |
| `crate-hull` | A crate thrown at the hull stops outside it, never in the cabin. |
| `crate-wake-stream` | Crates 500 m away stay frozen; when the walker comes they wake as Avian bodies with no drop below ground. |
| `town` | The first city stands on its plot, its buildings stop the walker, the characters stand at their spots. |
| `dev_area` | The dev area: level ground, walker and test console, the cursor on the console, the design system's sample screens. |
| `displays` | The pointer on the test console: hover, press and release, disabled buttons, slider, Escape. |
| `items` | Items on the dev area: collect, hold, drop, throw, a second player's request, inventory drag, a wrench through take-off. |
| `hud` | The crew HUD: compass, vitals, credits fade, prompts, action bar dimming; with a window, pictures of its states. |
| `health` | The walker's health: burn, radiation, on fire and down, on the host at the dev area. |
| `engine_room` | The engine room: corridor, door, air, and the alarm. |
| `reactor` | The reactor on the engine room's screens: temperature band, manual mode, load from the grid. |
| `coolant` | The pump's coolant tank: the drain, the refill by hand and E-COOL. |
| `rods` | Fuel rods by hand: loading, brake pull, and the spicy rod's radiation. |
| `blast` | The meltdown blast reaches a walker two metres off and not one at 25 m. |
| `fire` | Fires in the engine room, sprayed out with the extinguisher. |
| `fire_grid` | The cell model of the engine-room fire, switched on at the faults screen, catches the corridor. |
| `extinguisher` | The extinguisher's supply: wear, empty, refill at the corridor rack. |
| `repair` | Repair at a machine by holding, with the ring off. |
| `savefile` | Save, restart, load: the state that must survive comes back. |
| `licence` | The seat refuses without the flight licence; the exam at the flight school passes and the seat works. |
| `map` | The map's pins and M to open and close it; with a window a screenshot. |
| `planet-look` | The planet look harness: atlases, bake statistics and fixed viewpoint screenshots per planet (`EXO_LOOK=<planet>[:<viewpoint>,...]` for a subset). `--all-planets --radius=30000 --planet-seed=<n>` shows another seed (the seed rolls the recipe's `variation` tweaks), `--planet-recipe=<id>` a draft from `content/planet/draft/`; windowed runs need `--dev-area`. |
| `site-walk` | The walker walks from outside a ruin into it and stands on its flattened pad. |
| `daynight` | One day runs in 12 s on every planet: sun against `daynight_core`, brightness order, night light, lit side agrees with the star. |
| `space` | Fly to space, stop, walk out of the ship and drift, the suit, and back into the cabin. |
| `full` | The whole run: walk, board, take off, fly to space and back, land, walk out and in. |

Other options: `--radius=<m>` (first planet, overrides the file), `--start=<place>` (start beside a place's pad instead of the dev area, e.g. `drip_rock`; a run without a scenario starts on the dev area: a small level test ground with the devices, displays, a warm reactor and the ship models beside it), `--system-seed=<u64>` (debug: fly a generated star system instead of the file; `--system-kind=<harbour|normal|hostile>`, default normal, picks its template), `--record=<file>` (write the run's ship and walker path for the replay matrix), `--no-vsync` (frame-time measurements; with vsync every frame reads the display's period).

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

Items (C2): the host owns every loose item and every inventory; each player's request goes to the host over a reliable channel, shows at once on the acting client and is rolled back if the host refuses. `--sim-delay=<ms>`, `--sim-jitter=<ms>` and `--sim-loss=<percent>` hold back and drop what a process receives, so a LAN or loopback behaves like the internet (set on both ends for each way). Two processes at 150 ms each way and 2 % loss:

```sh
cargo run -p exo_app -- --net-host --sim-delay=150 --sim-jitter=10 --sim-loss=2
cargo run -p exo_app -- --net-connect=127.0.0.1:17441 --slot=2 --sim-delay=150 --sim-jitter=10 --sim-loss=2
```

(The table stands in both worlds, but its items are the host's: they reach the client as events. Walk to the table and press F on an item in both windows, G drops, R throws, Tab opens the inventory.) `cargo ts items_net` plays the same with two app instances in one process on loopback.

## Controls

All keys at a glance, sorted into game and debug: [`KEYS.md`](KEYS.md). In the game F1 shows the keys that apply right now.

Walker: mouse look (click to grab, Escape releases), WASD, Shift run, Space jump, F interact: the one verb, the prompt below the screen centre says what it does (sit at the seat, pick up a crate in reach or pull one with the grab tool, set it down), R throw the held crate (a crate set down fully on the cabin's floor plates mag-locks and the plates light up; red plates: it rests partly off them; grabbing unlocks), Q/E turn it (a small crate takes one hand; a medium one both: walking at 60 %, no sprint, no jump; the large one needs two players; heavy crates lag and slow the view), G cabin gravity on/off (only in a landed ship; in flight it is always on), V debug fly mode.
Ship: the mouse is a virtual joystick: moving it puts the marker off centre and the ship turns at that deflection (dead-zone circle in the middle, full at the outer edge; `ship_mode: "direct"` in `bindings.json` turns by mouse movement instead). W/S forward/back, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost (the capacitor), X firm brake, H gravity compensation, C coupled/decoupled (eases over 4 s; decoupled the ship keeps gliding), F stand up (the same interact key). Set down on ground no steeper than `landing_slope_limit` (`ground.json`), the ship keeps its spot until thrust (any input but down).
Suit (outside a ship in space, no gravity): mouse turns freely, W/S forward/back, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost, X brake to rest.
Quantum drive (seated, above 1.5 times the planet's atmosphere height): J spool up and warp to the selected planet, J again cancels while spooling or calibrating, N selects the target. Every planet has a HUD marker with name and distance; the target's is larger and coloured. Point the nose at the ring in the sky while the gauge fills (within 5 degrees; beyond 8 degrees the jump is lost). During the flight, hold J for 1 s to drop out early (emergency exit).
Gamepad or joystick (the first one connected): left stick moves, right stick turns (ship) or looks (on foot), triggers up/down, bumpers roll, left stick click boost/run, A jump, B brake, X warp, Y sit/stand, D-pad assist/decoupled, Select warp target. The mapping is in `bindings.json` (`"Pad:<button>"`, `"pad": "<axis>"`, raw HOTAS axes as `"Axis<n>"`).
Sound: a thrust hum, wind in the atmosphere, a thud on touchdown and a click on toggles, all synthesized in code (`audio.rs`); headless runs have none.
O: orbit camera (debug). The crew HUD on foot is minimal (Crew HUD v3, sizes in the `hud` section of `content/ui/displays.json`): compass at the top centre, health bottom left (oxygen and burn only while they apply, from the `Vitals` resource), the action bar at the bottom centre, key hints bottom right, a reticle, the prompt 16 px above its target, credits for a moment after a change. F3: debug lines (frame time, chunks, patches, warp, network) under the HUD, and with them the old readout and the planet markers; seated, the readout shows mode, speed, altitude (AGL: height above the ground below 1000 m, ALT: above the planet's sphere higher up) and the boost gauge: in the ship the capacitor charge (Shift drains it, it refills after a short pause; dimmed below the charge a boost needs to start). **Kackfass-1** flies the SC model (`flight_core::sc`, its own values in `content/ship/kackfass_1/sc_*.json`; the HUD mode `SHIP SC` includes the model name; F3 shows its felt G, saturation and turn cap): thrust as a force on the ship's mass, and Star Citizen's flight switches: H gravity compensation, C coupled or decoupled, F8 G-safe, K landing, B master mode SCM or NAV, U comstab, P proximity assist, I wind compensation, Page Up and Page Down the speed limiter (keys TODO(initiator)). Until #206 the axis model flew beside it (F7); it is gone, and so are its keys (F6, F7, L). Set down on ground (`ground.json`, #92) the ground rules keep the ship on its spot; the SC model has no landing gear yet (#162). F9 switches the camera effects (trauma shake under boost and touchdown, spring lag of the chase camera, FOV from forward G; `camera_shake` in the settings scales the shake; values in `content/tuning/camera.json`, all placeholders). K: landing mode (by hand, as in Star Citizen; the HUD shows `LANDING`): near the ground the speed and the descent drop; without it only the descent is held to what the thrust can still stop above the ground.

## Windows build

Linux and Windows are supported; development happens on Linux. Cross build with [llvm-mingw](https://github.com/mstorsjo/llvm-mingw) on `PATH`:

```sh
rustup target add x86_64-pc-windows-gnullvm
cargo build --release -p exo_app --target x86_64-pc-windows-gnullvm
```

Ship `libunwind.dll` from llvm-mingw (`x86_64-w64-mingw32/bin/`) next to the exe. The workflow `.github/workflows/ci.yml` builds it when started by hand (GitHub CI is off otherwise).

## License

MIT, see `LICENSE`. Assets: original or CC0 only; fonts may also be SIL OFL, each with its `OFL.txt` (`content/fonts/`). Vendored agent skills: see `.agents/skills/VENDORED.md`.
