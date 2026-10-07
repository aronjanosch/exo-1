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
- `crates/net_core`: snapshot format, interpolation buffer, clock sync, datagrams, replay matrix, no Bevy types and no sockets.
- `crates/exo_app`: the game. Bevy glue: terrain LOD and water, heightfield collision ring, ship body, walker on Avian queries, render origin, camera, HUD, UDP transport, scripted scenarios.
- `content/`: data. `content/planet/recipe.json` is the planet recipe.

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
- `space`: fly to space, stop, walk out of the ship and drift (stopped ship and one drifting at 3 m/s).
- `foreign`: a remote ship flies through the real snapshot path at 350 m/s; the walker stands and walks in its cabin, then beside it parked.
- `t5`: four 300 s walks at 1.8 m/s (basin, escarpment, plateau); slow, for terrain work.
- `net`: the network bot (take off, cruise, turn, brake, land, repeat); see below.

Other options: `--origin-shift=<m>` (render-origin threshold, 0 = off, default 1000), `--radius=<m>`, `--record=<file>` (write the run's ship and walker path for the replay matrix).

## Co-op (LAN)

One player hosts, the others connect. Each simulates its own walker and ship; the host relays snapshots (UDP port 17441).

```sh
cargo run -p exo_app -- --net-host                               # host, slot 1
cargo run -p exo_app -- --net-connect=192.168.1.20:17441 --slot=2 # client, slot 2..8, one slot per player
```

Options: `--port`, `--bind`, `--rate=<Hz>` (default 30), `--buffer=<ms>` (default 150), `--extrapolate=<ms>` (display-only extrapolation during an underrun, default 100, 0 = hold), `--bot` (fly the `net` scenario instead of the keyboard). Ships do not collide with each other; walking on another player's ship works.

## Controls

Walker: mouse look (click to grab, Escape releases), WASD, Shift run, Space jump, F sit at the seat, V debug fly mode.
Ship: mouse pitch/yaw, W/S forward/back, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost, X firm brake, H flight assist, L planet follow, F stand up.
O: orbit camera (debug).

## Windows build

Linux and Windows are supported; development happens on Linux. Cross build with [llvm-mingw](https://github.com/mstorsjo/llvm-mingw) on `PATH`:

```sh
rustup target add x86_64-pc-windows-gnullvm
cargo build --release -p exo_app --target x86_64-pc-windows-gnullvm
```

Ship `libunwind.dll` from llvm-mingw (`x86_64-w64-mingw32/bin/`) next to the exe. CI builds it on every push (`.github/workflows/ci.yml`).

## License

MIT, see `LICENSE`. Assets: original or CC0 only. Vendored agent skills: see `.agents/skills/VENDORED.md`.
