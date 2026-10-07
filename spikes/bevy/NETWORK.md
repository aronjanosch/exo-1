# Spike 10: network in Bevy (client authority)

Spike 4 (Godot) rebuilt on the spike 9 code. Report: `SPIKE-10-REPORT.md` in the concept repo. Every process simulates its own ship and walker; a host relays snapshots, the others interpolate. No prediction, rollback or anti-cheat. LAN and localhost only (no NAT handling).

All speeds, gravity, flight assists, look, controls and network settings (30 Hz, 150 ms buffer, 144-byte snapshot, slots 1 to 8, 2 s expiry, planets 200 km apart) are the spike 4 test values, **not designed**.

## Layout

- `net_core/`: snapshot (144 bytes), interpolation buffer (Hermite, slerp), fault-injection link, clock sync, wire packets, replay matrix. f64, no Bevy types, no dependency but `glam`.
- `exo_app/src/net_live.rs`: UDP transport (`std::net`), host relay, proxy ships (kinematic Avian bodies with the hull colliders), walker markers, measurements.
- `exo_app/src/net.rs`: snapshot of the local ship and walker. `exo_app/src/record.rs`: `--record` writes a scripted run as a 60 Hz path.
- `exo_app/tests/contacts.rs`: head-on contact in two independent Avian worlds.
- `tools/net_run.py`, `tools/net_suite.sh`: live runs with N headless processes; results in `results/net/`.
- `results/full-path.bin`: the recorded spike 9 `full` scenario (14,946 states, 249 s, 20 phases) for the replay matrix; `results/net-matrix.json`: the 96 cases.

## Checks without sockets

```sh
cargo test -p net_core                               # unit checks and the 96-case replay matrix (a few seconds)
cargo test -p exo_app --test contacts -- --nocapture # head-on contact, lag table
cargo run --release -p exo_app -- --headless --scenario=foreign --origin-shift=10   # walker in a foreign ship at 350 m/s
cargo run --release -p exo_app -- --headless --scenario=full --record=results/full-path.bin   # record the path again
```

## Live runs on one machine

```sh
cargo build --release -p exo_app
python3 tools/net_run.py --players 8 --seconds 40 --tag live-8 --extra="--origin-shift=200"
python3 tools/net_run.py --players 2 --seconds 40 --tag two-planets --planets 0,1 --force-shift 15,10000,10000,-10000
```

Options: `--extrapolate=<ms>` (display only, an underrun carries on with the last velocity for at most this long; default 0 = hold), `--delay` (ms, one way), `--jitter`, `--loss` (percent), `--rate` (Hz), `--buffer` (ms). Faults are injected on the receiving side after the real UDP transport, on snapshots only.

## Two computers (LAN test for the initiator)

Same build on both (or Linux and Windows). UDP port **17441** must be open on the host (`--port` changes it).

Linux: `cargo build --release -p exo_app`, binary `target/release/exo_app`.
Windows (cross build on Linux, llvm-mingw on `PATH`, see spike 7): `cargo build --release -p exo_app --target x86_64-pc-windows-gnullvm`; copy `exo_app.exe` and `libunwind.dll` (from llvm-mingw `x86_64-w64-mingw32/bin`) into one folder.

Host (slot 1):

```sh
exo_app --net-host
```

Client (slot 2..8, one computer each; use the host's LAN address):

```sh
exo_app --net-connect=<host-ip>:17441 --slot=2
```

Both start 20 m apart. The HUD's last line shows slot, remote ships, holds, bandwidth.

Controls are the spike 9 ones (click to grab the mouse, Escape releases): walk to the ship (WASD), up the ramp to the seat, **F** sits and starts the ship, **H** flight assist, **L** planet follow, **X** brake, Shift boost, **F** stands up again. **B** puts the walker into the nearest remote ship's cabin (a test placement, not a boarding mechanic); walking out of the cabin box leaves it.

Acceptance steps (as in spike 4):

1. **Flight.** Both fly. Each sees the other ship about 150 ms behind its true position (the playout buffer), moving smoothly. Own controls respond at once.
2. **Passenger carry.** One flies, the other presses **B**, stands in the cabin while the pilot flies, turns and brakes. The passenger must stay on the deck.
3. **Independent shift.** Fly more than 1 km from the start: the render origin shifts on that computer only (HUD line 3, "origin shifts"). The other computer must show no jump of the ship. `--origin-shift=200` shifts more often.
4. **Reconnect.** Stop a client (Ctrl+C), start it again with the same `--slot` within a few seconds. The old ship disappears after 2 s without snapshots, the new one appears, no ghost remains. A client that loses the host for 3 s ends with a non-zero exit code.
5. **Contact.** Fly the ships into each other and note what each side sees. This is the open design question; the report lists what is measured.

Fault injection on a real LAN run: add `--delay=100 --jitter=20 --loss=5` to a client.

Scripted flight on both sides for a quick check without playing: add `--bot` (and `--headless` if no window is wanted); a run ends by itself with `--seconds=40` and writes `results/net/<tag>-slot<N>.json` (`--tag`, `--net-out`).
