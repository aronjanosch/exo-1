//! `--perf` (#19): per scenario phase the simulation step time (FixedFirst to FixedLast, physics
//! included), the render frame time (windowed only, without vsync, screenshot frames left out),
//! the build time of the ring's terrain patches and the resident memory, written as JSON when the
//! script ends. With a baseline file every phase's step p95 is checked against it: a phase fails
//! when it is slower than the baseline by more than the tolerance.
//!
//! Windowed, also per planet swap the frames around it (#34), and every frame slower than one
//! 60 Hz frame with its phase and warp phase (#15). Reported, not checked.
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

/// Frames kept before a planet swap, and counted after it.
const SWAP_BEFORE: usize = 30;
const SWAP_AFTER: usize = 60;
/// A frame slower than this goes into the spike list (one 60 Hz frame).
const SPIKE_MS: f64 = 1000.0 / 60.0;
/// The spike list stops growing here.
const MAX_SPIKES: usize = 500;

/// The frames around one planet swap. A frame's time is measured at the start of the next one,
/// and the render runs a frame behind, so the swap's cost shows a frame or two after it.
struct SwapWindow {
    label: String,
    /// Frame number of the swap.
    frame: u64,
    /// (frame number, ms), from `SWAP_BEFORE` frames before to `SWAP_AFTER` after.
    frames: Vec<(u64, f64)>,
}

struct Spike {
    frame: u64,
    ms: f64,
    phase: String,
    warp: String,
}

#[derive(Resource)]
pub struct Perf {
    pub opt: PerfOptions,
    windowed: bool,
    phases: Vec<Phase>,
    step_start: Option<Instant>,
    frame_no: u64,
    /// The last `SWAP_BEFORE` frames.
    recent: std::collections::VecDeque<(u64, f64)>,
    swaps_seen: usize,
    swaps: Vec<SwapWindow>,
    spikes: Vec<Spike>,
}

impl Perf {
    pub fn new(opt: PerfOptions, windowed: bool) -> Perf {
        Perf { opt, windowed, phases: Vec::new(), step_start: None, frame_no: 0, recent: Default::default(), swaps_seen: 0, swaps: Vec::new(), spikes: Vec::new() }
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

/// Update, after the ring: frame time (windowed) and the patches built since the last frame;
/// the frames around planet swaps and the slow frames.
#[allow(clippy::too_many_arguments)]
pub fn frame(
    mut perf: ResMut<Perf>,
    time: Res<Time<Real>>,
    script: Option<Res<Script>>,
    mut ring: ResMut<crate::ring::Ring>,
    shots: Query<(), With<bevy::render::view::screenshot::Screenshot>>,
    tel: Option<Res<crate::warp::WarpTelemetry>>,
    wd: Option<Res<crate::warp::WarpDrive>>,
) {
    let name = phase_name(&script);
    let windowed = perf.windowed;
    let built = std::mem::take(&mut ring.built_ms);
    let p = perf.phase(&name);
    p.build_ms.extend(built);
    perf.frame_no += 1;
    let n = perf.frame_no;
    // Planet swaps of this frame's fixed steps (they run before Update).
    if let Some(tel) = &tel {
        while perf.swaps_seen < tel.swaps.len() {
            let (_, from, to, drop) = tel.swaps[perf.swaps_seen];
            let frames = perf.recent.iter().copied().collect();
            perf.swaps.push(SwapWindow { label: format!("{from} -> {to}{}", if drop { " (emergency drop)" } else { "" }), frame: n, frames });
            perf.swaps_seen += 1;
        }
    }
    if !(windowed && shots.is_empty() && time.delta_secs_f64() > 0.0) {
        return;
    }
    let ms = time.delta_secs_f64() * 1000.0;
    perf.phase(&name).frame_ms.push(ms);
    perf.recent.push_back((n, ms));
    if perf.recent.len() > SWAP_BEFORE {
        perf.recent.pop_front();
    }
    for w in perf.swaps.iter_mut().filter(|w| n >= w.frame && n <= w.frame + SWAP_AFTER as u64) {
        w.frames.push((n, ms));
    }
    if ms > SPIKE_MS && perf.spikes.len() < MAX_SPIKES {
        let warp = wd.map(|w| format!("{:?}", w.drive.phase)).unwrap_or_default();
        perf.spikes.push(Spike { frame: n, ms, phase: name, warp });
    }
}

/// The frames around each swap, and the slow frames, for the report (also printed).
fn swap_report(perf: &Perf) -> (Vec<Value>, Vec<Value>) {
    let swaps = perf
        .swaps
        .iter()
        .map(|w| {
            let (before, after): (Vec<(u64, f64)>, Vec<(u64, f64)>) = w.frames.iter().partition(|(f, _)| *f < w.frame);
            let worst = after.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1));
            let base: Vec<f64> = before.iter().map(|x| x.1).collect();
            let line = match worst {
                Some((f, ms)) => format!("perf swap {}: worst frame {ms:.2} ms at swap {:+} frames; the {} frames before: {}", w.label, f as i64 - w.frame as i64, base.len(), stats(&base)),
                None => format!("perf swap {}: no frame measured after it", w.label),
            };
            println!("{line}");
            let offsets: Vec<Value> = w.frames.iter().map(|(f, ms)| json!([*f as i64 - w.frame as i64, ms])).collect();
            json!({ "swap": w.label, "frame": w.frame, "worst_ms": worst.map(|x| x.1), "worst_offset": worst.map(|x| x.0 as i64 - w.frame as i64), "before_ms": stats(&base), "frames": offsets })
        })
        .collect();
    let spikes: Vec<Value> = perf
        .spikes
        .iter()
        .map(|s| {
            let near = perf.swaps.iter().map(|w| s.frame as i64 - w.frame as i64).min_by_key(|d| d.abs());
            json!({ "frame": s.frame, "ms": s.ms, "phase": s.phase, "warp": s.warp, "from_swap": near })
        })
        .collect();
    if perf.windowed {
        println!("perf: {} frames slower than {SPIKE_MS:.1} ms (list in the report)", perf.spikes.len());
    }
    (swaps, spikes)
}

/// Resident memory (Linux `/proc/self/statm`, the process's own entry); elsewhere none.
pub fn rss_mb() -> Option<f64> {
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
    let (swaps, spikes) = swap_report(perf);
    let report = json!({ "scenario": scenario, "windowed": perf.windowed, "slow_ms": perf.opt.slow_ms, "phases": phases, "swaps": swaps, "spikes": spikes });
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
    // Last in the frame: after the HUD it measures.
    app.add_systems(Update, frame.after(crate::view::update_hud).in_set(crate::phases::Frame::Hud));
}
