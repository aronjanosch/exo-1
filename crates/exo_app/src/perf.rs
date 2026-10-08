//! `--perf` (#19): per scenario phase the simulation step time (FixedFirst to FixedLast, physics
//! included), the render frame time (windowed only, without vsync, screenshot frames left out),
//! the build time of the ring's terrain patches and the resident memory, written as JSON when the
//! script ends. With a baseline file every phase's step p95 is checked against it: a phase fails
//! when it is slower than the baseline by more than the tolerance.
use crate::scenario::Script;
use bevy::prelude::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct PerfOptions {
    /// Baseline to compare against; missing file: no comparison (the run says so).
    pub baseline: PathBuf,
    /// Write this run as the new baseline instead of comparing.
    pub save_baseline: bool,
    /// Allowed slowdown of a phase's step p95: relative share and absolute milliseconds on top.
    pub tolerance: f64,
    pub slack_ms: f64,
    /// Test hook: every fixed step sleeps this long (a deliberately slowed step).
    pub slow_ms: f64,
}

impl Default for PerfOptions {
    fn default() -> Self {
        PerfOptions { baseline: PathBuf::new(), save_baseline: false, tolerance: 0.5, slack_ms: 0.5, slow_ms: 0.0 }
    }
}

#[derive(Default)]
struct Phase {
    name: String,
    step_ms: Vec<f64>,
    frame_ms: Vec<f64>,
    build_ms: Vec<f64>,
    rss_mb: Option<f64>,
}

#[derive(Resource)]
pub struct Perf {
    pub opt: PerfOptions,
    windowed: bool,
    phases: Vec<Phase>,
    step_start: Option<Instant>,
}

impl Perf {
    pub fn new(opt: PerfOptions, windowed: bool) -> Perf {
        Perf { opt, windowed, phases: Vec::new(), step_start: None }
    }

    fn phase(&mut self, name: &str) -> &mut Phase {
        let name = if name.is_empty() { "other" } else { name };
        if self.phases.last().is_none_or(|p| p.name != name) {
            // A phase name seen before (a repeated step) continues its own entry.
            if let Some(i) = self.phases.iter().position(|p| p.name == name) {
                let p = self.phases.remove(i);
                self.phases.push(p);
            } else {
                self.phases.push(Phase { name: name.to_string(), ..default() });
            }
        }
        self.phases.last_mut().unwrap()
    }
}

fn phase_name(script: &Option<Res<Script>>) -> String {
    script.as_ref().map(|s| s.ctx.phase.clone()).unwrap_or_default()
}

pub fn step_begin(mut perf: ResMut<Perf>) {
    perf.step_start = Some(Instant::now());
    if perf.opt.slow_ms > 0.0 {
        std::thread::sleep(std::time::Duration::from_secs_f64(perf.opt.slow_ms / 1000.0));
    }
}

pub fn step_end(mut perf: ResMut<Perf>, script: Option<Res<Script>>) {
    let Some(t0) = perf.step_start.take() else { return };
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let name = phase_name(&script);
    let p = perf.phase(&name);
    p.step_ms.push(ms);
    p.rss_mb = rss_mb().or(p.rss_mb);
}

/// Update, after the ring: frame time (windowed) and the patches built since the last frame.
pub fn frame(mut perf: ResMut<Perf>, time: Res<Time<Real>>, script: Option<Res<Script>>, mut ring: ResMut<crate::ring::Ring>, shots: Query<(), With<bevy::render::view::screenshot::Screenshot>>) {
    let name = phase_name(&script);
    let windowed = perf.windowed;
    let built = std::mem::take(&mut ring.built_ms);
    let p = perf.phase(&name);
    p.build_ms.extend(built);
    if windowed && shots.is_empty() && time.delta_secs_f64() > 0.0 {
        p.frame_ms.push(time.delta_secs_f64() * 1000.0);
    }
}

/// Resident memory (Linux `/proc/self/statm`, the process's own entry); elsewhere none.
fn rss_mb() -> Option<f64> {
    let s = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: f64 = s.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096.0 / 1_048_576.0)
}

fn stats(v: &[f64]) -> Value {
    if v.is_empty() {
        return Value::Null;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let at = |q: f64| s[((s.len() - 1) as f64 * q).round() as usize];
    json!({ "n": s.len(), "p50": at(0.5), "p95": at(0.95), "max": s[s.len() - 1] })
}

/// When the script ends: write `<out>/perf-<scenario>.json`, then save it as the baseline or check
/// it against the baseline. Returns (passed, line) per checked phase.
pub fn finish(perf: &Perf, scenario: &str, out_dir: &std::path::Path) -> Vec<(bool, String)> {
    let phases: Vec<Value> = perf
        .phases
        .iter()
        .map(|p| json!({ "name": p.name, "step_ms": stats(&p.step_ms), "frame_ms": stats(&p.frame_ms), "chunk_build_ms": stats(&p.build_ms), "rss_mb": p.rss_mb }))
        .collect();
    let report = json!({ "scenario": scenario, "windowed": perf.windowed, "slow_ms": perf.opt.slow_ms, "phases": phases });
    let _ = std::fs::create_dir_all(out_dir);
    let path = out_dir.join(format!("perf-{scenario}.json"));
    let text = serde_json::to_string_pretty(&report).unwrap();
    let _ = std::fs::write(&path, &text);
    println!("perf: {}", path.display());
    let base = &perf.opt.baseline;
    if perf.opt.save_baseline {
        if let Some(d) = base.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        std::fs::write(base, &text).expect("write perf baseline");
        println!("perf: baseline saved to {}", base.display());
        return Vec::new();
    }
    let Some(old) = std::fs::read_to_string(base).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()) else {
        println!("perf: no baseline at {} (save one with --perf-save-baseline)", base.display());
        return Vec::new();
    };
    let p95 = |v: &Value| v["step_ms"]["p95"].as_f64();
    let mut out = Vec::new();
    for p in &phases {
        let Some(was) = old["phases"].as_array().and_then(|a| a.iter().find(|o| o["name"] == p["name"])).and_then(p95) else { continue };
        let Some(now) = p95(p) else { continue };
        let limit = was * (1.0 + perf.opt.tolerance) + perf.opt.slack_ms;
        out.push((now <= limit, format!("perf '{}': step p95 {now:.3} ms, baseline {was:.3} ms, limit {limit:.3} ms", p["name"].as_str().unwrap_or("?"))));
    }
    out
}

pub fn plugin(app: &mut App) {
    app.add_systems(FixedFirst, step_begin);
    app.add_systems(FixedLast, step_end);
    app.add_systems(Update, frame.after(crate::ring::update_ring));
}
