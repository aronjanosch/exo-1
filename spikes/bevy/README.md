# Spike 9: Bevy validation

Spikes 1, 3, 5 and 8 rebuilt in Rust with Bevy 0.19.1 and Avian 0.7 (f64). Report: `SPIKE-9-REPORT.md` in the concept repo.

All speeds, gravity, flight assists, look and controls are the test values of the Godot spikes, **not designed**.

## Layout

- `flight_core/`: assisted-flight controller and planet field (port of `ship.gd`, `main.gd`), f64, no Bevy types. `cargo test -p flight_core` runs the 51 checks of `flight_test.gd`.
- `walker_core/`: walker with its own move-and-slide over a `World` trait (sweep, depenetrate), no Bevy types.
- `exo_app/`: Bevy glue: terrain LOD and water (view), heightfield collision ring, ship body, walker on Avian queries, render origin, camera, HUD, scripted scenarios.
- `planet_core` is used by path from `../planet_gen/rust/planet_core`, unchanged.

## Run

```sh
cargo run -p exo_app                                  # play (window)
cargo run -p exo_app -- --scenario=full               # scripted run, window, screenshots
cargo run -p exo_app -- --hidden --scenario=full      # same, invisible window (screenshots still work)
cargo run -p exo_app -- --headless --scenario=full    # no window, one physics tick per update
cargo test --workspace                                # all checks, about 11 s after a build
cargo build -p exo_app --features dynamic             # Bevy dynamic linking, about 1 s rebuilds
cargo dev -- --scenario=full                          # alias: run with `dynamic`
cargo run -p exo_app --features dynamic,remote -- --hidden   # BRP on 127.0.0.1:15702 (dev only, red-class, windowed only; MCP server in .mcp.json)
./run-agent.sh ./target/release/exo_app --scenario=full   # window on workspace 7 without focus (frame times)
```

Scenarios: `walk` (stand still, run 20 s), `t5` (spike 8 T5: 1.8 m/s, 300 s, four starts), `full` (walk, ramp into the parked ship, take off, to space at 7000 m, cabin at 400 m/s rolling, back, land, out and back in, cabin in atmosphere). Results and screenshots go to `--out=<dir>` (default `results/`). The run exits non-zero if a check fails.

Options: `--planet-offset=x,y,z` (planet centre in world space), `--origin-shift=<m>` (render-origin threshold, 0 = off, default 1000), `--radius=<m>`.

Windows: `cargo build --release -p exo_app --target x86_64-pc-windows-gnullvm` with llvm-mingw on `PATH` (spike 7), ship `libunwind.dll` next to the exe.

## Controls (Godot spike values)

Walker: mouse look (click to grab, Escape releases), WASD, Shift run, Space jump, V fly mode, F sit at the seat. Ship: mouse pitch/yaw, W/S forward/back, A/D strafe, Space/Ctrl up/down, Q/E roll, Shift boost, X firm brake, H flight assist, L planet follow, F stand up. O orbit camera.
