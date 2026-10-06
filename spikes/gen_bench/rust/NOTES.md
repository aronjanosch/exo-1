# Spike 6, Rust side: notes (measured on the dev machine, 16 logical cores, release build)

## Layout
- `gen_core` (lib, 489 lines): full workload from SPEC.md, f32, `fastnoise-lite` 1.1.1.
- `gen_bench_cli` (bin, 175 lines) = variant C. `bench <out.json>`, `probe <out.csv>`, `compare <a.csv> <b.csv>`.
- `gen_godot` (cdylib, 75 lines) = variant B, class `ExoGen`. Library: `target/release/libgen_godot.so`, loaded via `../exo_gen.gdextension` (debug and release both point at the release .so).
- `../noise_probe.gd` (52 lines): Godot side of the noise parity probe.
- `.gdignore` keeps Godot out of `target/`.

## Licence of the noise crate
`fastnoise-lite` 1.1.1: Cargo.toml says MIT, the packaged crate has no licence file. Upstream repo `Auburn/FastNoiseLite` has a `LICENSE` (read via raw.githubusercontent.com and `gh api .../license`): MIT, "Copyright(c) 2020 Jordan Peck, Copyright(c) 2020 Contributors"; GitHub detects SPDX `MIT`. Result: MIT confirmed. (Not checked: separate licence of the Rust port subdirectory, there is none in the repo root listing; the crate's authors are Jordan Peck and Keavon Chambers.)

## Noise parity (Godot 4.7.2 FastNoiseLite versus fastnoise-lite 1.1.1, f32)
1000 probe points, all 10 configs of SPEC.md: max abs difference 0.000e0 for every noise, so bit-identical at the printed precision (12 decimals). Ridged and OpenSimplex2S included, no adaptation needed. Checksum from Godot calling B (`height_sum` 4169609.07197101, canopy 3021, rocks 11549) equals the C checksum (4169609.072, 3021, 11549, biomes [107142, 89594, 20880, 184]). GDScript variants should therefore match these numbers if they implement the SPEC exactly (small f32/f64 rounding differences possible in A/A2).

## Measured (C, `cargo build --release`, opt-level 3, lto, codegen-units 1)
- Bake 512x512x6: 1 thread 363/387/386 ms; 16 threads 54/54/53 ms (std::thread::scope over row ranges, about 7x).
- One thread, per chunk (2000 samples after 20 warm-up chunks): mean 637 us, P95 792 us, max 930 us.
- Throughput: 1 thread 1646 chunks/s, 4: 6065, 8: 10277, 16: 13827.

Measured (B, Godot headless, release .so):
- One thread from main thread, 200 chunks serial: Godot-side mean 634 us, inside Rust 631 us, so Dictionary and packed-array conversion costs about 3-4 us per chunk (included in the 631: usec is measured around conversion too).
- Bake with `bake_threads(16)`: about 56 ms.
- 200 chunks via `WorkerThreadPool.add_group_task` (16 threads): 16.2 ms total (about 12300 chunks/s), mean per call 1210 us under load. No borrow panic.

## Bindings notes
- `ExoGen.build_chunk` is `&self`; data lives in `Option<Arc<Gen>>`, only `setup`/`bake*` take `&mut self` (main thread, before workers start). Parallel `&self` calls from WorkerThreadPool tasks work only with the godot-rust feature `experimental-threads` (enabled in Cargo.toml). Not tested without it. Safeguards level shown at load: "balanced".
- `bake_threads` takes the Gen out of the Arc (`Arc::try_unwrap`) and errors if chunks are still in flight.
- `Dictionary::set` wants `&Variant` (`d.set("k", &v.to_variant())`), first compile error that cost a few minutes. No other binding problems.
- Dictionary keys are `GString`, values Variant (`Dictionary<GString, Variant>` return type works from GDScript).
- `godot --headless --import` was run once to register the extension; it created untracked `spikes/network/results/visual-{1,2}.png.import` (not mine, not part of this spike, delete or ignore).

## Compile times (release, touch `gen_core/src/lib.rs`, rebuild one crate; deps already built)
- C (`gen_bench_cli`): 3.7 s.
- B (`gen_godot`): 33.8 s (touching only gen_godot: 33.5 s, so the cost is LTO plus codegen-units 1 over the godot crates, not gen_core).
- First full build including godot-rust codegen: not timed separately in this session (several tens of seconds).
