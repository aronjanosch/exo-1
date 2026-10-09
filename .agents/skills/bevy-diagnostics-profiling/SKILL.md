---
name: bevy-diagnostics-profiling
description: "Use when profiling Bevy 0.19 with `DiagnosticPath`, `DiagnosticsStore`, `info_span!`, or `RenderDiagnosticsPlugin`: custom metrics, CPU/GPU pass timings, queue/task/upload telemetry, stale-result counts, and native frame budgets."
license: MIT
compatibility: opencode,claude-code,cursor
metadata:
  tier: "2"
  area: diagnostics
  bevy_version: "0.19"
---

# Bevy 0.19 — diagnostics and profiling

## When to use this skill

- A frame, render pass, worker pipeline, or upload path needs causal evidence.
- A queue/task system needs stable gauges, counters, latency, and high-water marks.
- Performance claims must pass native acceptance budgets.
- Render GPU diagnostics are absent or disagree with CPU traces.

Diagnostics answer “what is happening over time?” Tracing answers “where did this
execution spend time?” Render diagnostics expose pass-level renderer measurements.
Use all three, but do not treat any one of them as proof of a bottleneck.

In EXO-1, measure first with the game's own perf harness (`--perf`, README section "Performance", `crates/exo_app/src/perf.rs`): step and frame times per scenario phase against a baseline. Reach for this skill when the harness shows a phase is slow and the cause is open.

## Canonical pattern

```rust
use bevy::{
    diagnostic::{
        Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic,
    },
    prelude::*,
};

const PATCH_QUEUE_DEPTH: DiagnosticPath =
    DiagnosticPath::const_new("terrain/patch_queue_depth");

#[derive(Resource, Default)]
struct PatchQueue {
    valid_jobs: usize,
}

struct TerrainDiagnosticsPlugin;

impl Plugin for TerrainDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PatchQueue>()
            .register_diagnostic(
                Diagnostic::new(PATCH_QUEUE_DEPTH).with_suffix(" patches"),
            )
            .add_systems(Update, record_terrain_diagnostics);
    }
}

fn record_terrain_diagnostics(
    queue: Res<PatchQueue>,
    mut diagnostics: Diagnostics,
) {
    diagnostics.add_measurement(&PATCH_QUEUE_DEPTH, || queue.valid_jobs as f64);
}
```

Paths are an API: use namespaced, stable names with explicit units in suffixes or
documentation. Register before recording. `DiagnosticsStore` provides latest and
historical/smoothed values for displays and exporters. Measurements are deferred
telemetry, not a same-system synchronisation mechanism.

## Built-in plugins

- `FrameTimeDiagnosticsPlugin`: FPS, frame time, and frame count.
- `EntityCountDiagnosticsPlugin`: live entity count.
- `SystemInformationDiagnosticsPlugin`: process/system CPU and memory on supported
  native targets. It is unavailable/no-op in dynamic-link builds, which includes
  this project's `cargo t` and `cargo scenario`.
- `LogDiagnosticsPlugin`: development output; filter paths and cadence to avoid noise.
- `RenderDiagnosticsPlugin`: CPU/GPU time per recorded render span and pipeline
  statistics where the backend supports them.

`FrameTimeDiagnosticsPlugin` and `EntityCountDiagnosticsPlugin` are structs with
fields: add them as `FrameTimeDiagnosticsPlugin::default()`.

Add `RenderDiagnosticsPlugin` from `bevy::render::diagnostic`, guarded by
`if !app.is_plugin_added::<RenderDiagnosticsPlugin>()` after `DefaultPlugins`:
with `bevy/trace_tracy`, `RenderPlugin` adds it already (`bevy_render` feature
`tracing-tracy`), and a second `add_plugins` panics with `DuplicatePlugin`. It creates dynamic
paths shaped like `render/<span>/elapsed_cpu` and, when supported,
`render/<span>/elapsed_gpu` plus invocation/primitive statistics.

## Tracing for causality

Enable a backend in an optimised build:

```text
cargo run --release --features bevy/trace_tracy
cargo run --release --features bevy/trace_chrome
```

`trace_tracy_memory` adds allocation tracking with extra overhead. Add coarse spans
around work whose cause matters:

```rust
use bevy::prelude::*;

fn rebuild_visible_sections() {
    let _span = info_span!("rebuild_visible_sections").entered();
    // measured work
}
```

Avoid high-cardinality span names and per-element spans. Record section/entity IDs as
fields only during targeted captures. Keep a low-overhead diagnostic gauge/counter
for continuous observability and switch on detailed traces for investigation.

## Gotchas and platform truth

Bevy 0.19 render timestamp queries and pipeline statistics are supported on Vulkan
and DX12. Metal exposes CPU render timings only through
`RenderDiagnosticsPlugin`. Missing `elapsed_gpu` is an unsupported measurement, not
zero GPU cost. Use vendor/platform GPU profilers where available; RenderDoc is a
graphics debugger, not a performance profiler.

Profile the shipping renderer, resolution, power mode, content, and build profile.

## Choose the relevant deep dive

| Problem | Read |
|---|---|
| Metric types, stable paths, diagnostics store, tracing spans | [Diagnostics and tracing](references/diagnostics-and-tracing.md) |
| Render diagnostics, backend limitations, CPU/GPU tool choice | [Render and platform profiling](references/render-and-platform-profiling.md) |
| Acceptance thresholds, replay design, queue/task/upload telemetry | [Budgets and telemetry](references/budgets-and-telemetry.md) |

## Review checklist

- Every number has a type (gauge/counter/histogram), unit, scope, and sampling rule.
- Queue depth excludes obsolete heap entries; latency timestamps use one clock.
- Spans cover queue wait, worker compute, main-thread apply, and render/upload costs.
- Captures are release-like, repeatable, warm, and made on each claimed target.
- Reports include percentiles and worst sustained intervals, not averages alone.
- Unsupported GPU values remain absent and are never converted to zero.
- Instrumentation overhead is measured and disabled/reduced in normal shipping runs.

## See also

- [`bevy-testing`](../bevy-testing/SKILL.md) — deterministic benchmark replays and visual captures.
- [Bevy diagnostics](https://docs.rs/bevy/0.19.0/bevy/diagnostic/index.html)
- [Bevy render diagnostics](https://docs.rs/bevy/0.19.0/bevy/render/diagnostic/index.html)
- [Bevy 0.19 profiling guide](https://github.com/bevyengine/bevy/blob/v0.19.0/docs/profiling.md)
