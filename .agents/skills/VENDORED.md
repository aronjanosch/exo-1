# Vendored skills

Source: https://github.com/chrisgliddon/bevy-skills, commit b1b4da5744ebbd5c526342b2351967411cd5ca61, MIT (see `LICENSE-bevy-skills`, Copyright (c) 2026 Chris Gliddon).
Copied verbatim from `skills/<name>/` into `.agents/skills/`: `bevy-ecs-queries`, `bevy-testing`.
Copied with a trim: `bevy-diagnostics-profiling`. The trim follows `SPIKE-9B-SKILLS-REVIEW.md` in the concept repo: `RenderDiagnosticsPlugin` is added behind `is_plugin_added` (with `bevy/trace_tracy`, `RenderPlugin` adds it already and a second add panics), a note to add `FrameTimeDiagnosticsPlugin`/`EntityCountDiagnosticsPlugin` via `::default()`, WebGPU/WebGL2/browser and Steam Deck budgets cut, voxel example names renamed to `terrain/...`, dead See-also links to `bevy-voxel-runtime` and `bevy-rendering` removed. `git log -p` on the folder shows the trim against the verbatim copy.
Spike 9b read them completely and checked every example against Bevy 0.19.1 (42 examples, 0 wrong). Other candidates are not vendored; see `SPIKE-9B-REPORT.md` in the concept repo.
Links to sibling skills that are not vendored (See also) are dead.
