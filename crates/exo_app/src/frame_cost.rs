//! Frame spikes (#213): what each frame cost on the main thread and which streaming work happened
//! in it. The suspects (terrain uploads, collision ring patches, scatter cells, origin shifts,
//! transform propagation, fixed steps) add their count and time to `FrameCost`; `frame_end`
//! closes the frame into a `FrameRec`. Always on (a few `Instant` reads per frame).
//!
//! - F3 shows worst and p99 of the last `WINDOW_S` seconds and the last spike.
//! - Every frame slower than `FrameSpikes::threshold_ms` (25 ms, `--spike-ms=`) prints a
//!   `spike:` line naming what ran in it.
//! - Scenarios set `keep_all` and read every frame for their report (`planet-flight`).
//!
//! Frame time is wall time from one frame's start (`First`) to the next one's, measured with
//! `Instant` (headless runs advance `Time<Real>` by a fixed tick). In a window the render thread
//! runs a frame behind: a frame whose main-thread cost is small but whose time is long waited for
//! the render (or the GPU, or the present).
use bevy::prelude::*;
use std::collections::VecDeque;
use std::time::Instant;

/// Seconds of frames behind the F3 worst and p99.
pub const WINDOW_S: f64 = 5.0;
/// Default spike threshold (ms).
pub const DEFAULT_THRESHOLD_MS: f64 = 25.0;
/// Worst frames kept for the run.
const WORST_KEPT: usize = 16;

/// Work of one suspect in one frame: items done and main-thread milliseconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct Work {
    pub n: u32,
    pub ms: f64,
}

impl Work {
    pub fn add(&mut self, n: u32, since: Instant) {
        self.n += n;
        self.ms += since.elapsed().as_secs_f64() * 1000.0;
    }
}

/// The current frame's costs, written by the systems that do the work.
#[derive(Resource, Clone, Debug, Default)]
pub struct FrameCost {
    /// Terrain chunks uploaded (mesh + entities) and the upload time.
    pub terrain_up: Work,
    /// The whole terrain system (quadtree walk, task starts, uploads); n = tasks started.
    pub terrain_sys: Work,
    /// Collision patches turned into heightfield colliders.
    pub ring_up: Work,
    /// The whole ring system; n = patch jobs started.
    pub ring_sys: Work,
    /// Scatter cells spawned; n = cells, `scatter_inst` = instances spawned.
    pub scatter_up: Work,
    pub scatter_inst: u32,
    /// The whole scatter system (cell walk, task starts, uploads).
    pub scatter_sys: Work,
    /// Render-origin shift: n = transforms moved.
    pub shift: Work,
    /// Transform propagation (PostUpdate).
    pub propagate: Work,
    /// Fixed steps (physics and simulation) run in this frame.
    pub fixed: Work,
    /// Avian step stages in this frame (ms): start..broad phase, narrow phase, solver, sleeping and
    /// finalize.
    pub phys: [f64; 4],
    /// Avian's prepare (colliders, mass properties, transforms) and writeback sets, outside the step.
    pub prep_ms: f64,
    pub wb_ms: f64,
    /// Of the solver stage: swept CCD (Avian's own timer).
    pub ccd_ms: f64,
    /// Of the fixed step: the `FixedUpdate` schedule (game systems).
    pub fixed_update_ms: f64,
    /// The Update schedule.
    pub update: Work,
    /// The chunk under the camera is coarser than wanted (its children are still building).
    pub lod_debt: bool,
    /// Chunk jobs in flight or waiting for upload.
    pub terrain_pending: u32,
}

/// The render thread's last finished frame (windowed): ms per stage of the `Render` schedule.
/// The render runs one frame behind the main thread, so a frame's record carries the render that
/// finished during it. `views` holds the swapchain acquire (it waits when the GPU is behind).
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderCost {
    /// Render frames finished since the last main frame (0: none, the main thread did not wait).
    pub frames: u32,
    pub total_ms: f64,
    /// Extract commands, asset and mesh preparation (GPU uploads of new meshes).
    pub assets_ms: f64,
    /// Views (swapchain acquire) and specialisation.
    pub views_ms: f64,
    pub queue_ms: f64,
    /// Sort, prepare resources and bind groups.
    pub prepare_ms: f64,
    /// The render graph (encoding, submit, present).
    pub graph_ms: f64,
    pub cleanup_ms: f64,
}

/// Shared between the main world and the render world.
#[derive(Resource, Clone, Default)]
pub struct RenderShared(pub std::sync::Arc<std::sync::Mutex<(RenderCost, [Option<Instant>; 7])>>);

/// One closed frame.
#[derive(Clone, Debug, Default)]
pub struct FrameRec {
    pub frame: u64,
    /// Seconds since the first frame.
    pub t: f64,
    /// Wall time to the next frame's start.
    pub dt_ms: f64,
    /// Main thread from `First` to the end of `Last`.
    pub main_ms: f64,
    pub cost: FrameCost,
    pub render: RenderCost,
    /// Scenario phase, if a script runs.
    pub phase: String,
}

impl FrameRec {
    /// The `spike:` line: frame time and what ran in the frame.
    pub fn line(&self) -> String {
        let c = &self.cost;
        let mut s = format!(
            "frame {} t {:.2} s: {:.1} ms (main {:.1}, fixed {}x {:.1}, update {:.1}, propagate {:.1})",
            self.frame, self.t, self.dt_ms, self.main_ms, c.fixed.n, c.fixed.ms, c.update.ms, c.propagate.ms
        );
        if c.fixed.ms > 8.0 {
            s += &format!(" [game {:.1}, prep {:.1}, writeback {:.1}, avian broad {:.1} narrow {:.1} solver {:.1} (ccd {:.1}) rest {:.1}]", c.fixed_update_ms, c.prep_ms, c.wb_ms, c.phys[0], c.phys[1], c.phys[2], c.ccd_ms, c.phys[3]);
        }
        let mut parts = Vec::new();
        if c.terrain_up.n > 0 || c.terrain_sys.ms > 1.0 {
            parts.push(format!("terrain {} up {:.1} ms (sys {:.1}, {} started)", c.terrain_up.n, c.terrain_up.ms, c.terrain_sys.ms, c.terrain_sys.n));
        }
        if c.ring_up.n > 0 || c.ring_sys.ms > 1.0 {
            parts.push(format!("ring {} patches {:.1} ms (sys {:.1})", c.ring_up.n, c.ring_up.ms, c.ring_sys.ms));
        }
        if c.scatter_up.n > 0 || c.scatter_sys.ms > 1.0 {
            parts.push(format!("scatter {} cells {} inst {:.1} ms (sys {:.1})", c.scatter_up.n, c.scatter_inst, c.scatter_up.ms, c.scatter_sys.ms));
        }
        if c.shift.n > 0 {
            parts.push(format!("origin shift {} transforms {:.1} ms", c.shift.n, c.shift.ms));
        }
        let other = self.dt_ms - self.main_ms;
        if other > 5.0 {
            parts.push(format!("off main {other:.1} ms"));
        }
        let r = &self.render;
        if r.frames > 0 {
            parts.push(format!(
                "render {:.1} ms (assets {:.1}, views {:.1}, queue {:.1}, prepare {:.1}, graph {:.1}, cleanup {:.1})",
                r.total_ms, r.assets_ms, r.views_ms, r.queue_ms, r.prepare_ms, r.graph_ms, r.cleanup_ms
            ));
        }
        if !parts.is_empty() {
            s.push_str(": ");
            s.push_str(&parts.join(", "));
        }
        if !self.phase.is_empty() {
            s.push_str(&format!(" [{}]", self.phase));
        }
        s
    }
}

#[derive(Resource)]
pub struct FrameSpikes {
    pub threshold_ms: f64,
    /// Print a `spike:` line for each frame above the threshold.
    pub log: bool,
    /// Keep every frame in `all` (scenarios).
    pub keep_all: bool,
    pub all: Vec<FrameRec>,
    /// Frames of the last `WINDOW_S` seconds: (t, dt_ms).
    recent: VecDeque<(f64, f64)>,
    /// The worst frames of the run, slowest first.
    pub worst: Vec<FrameRec>,
    pub over: u32,
    pub frames: u64,
    pub last_spike: Option<FrameRec>,
    start: Option<Instant>,
    frame_start: Option<Instant>,
    open: Option<FrameRec>,
}

impl Default for FrameSpikes {
    fn default() -> Self {
        FrameSpikes {
            threshold_ms: DEFAULT_THRESHOLD_MS,
            log: true,
            keep_all: false,
            all: Vec::new(),
            recent: VecDeque::new(),
            worst: Vec::new(),
            over: 0,
            frames: 0,
            last_spike: None,
            start: None,
            frame_start: None,
            open: None,
        }
    }
}

impl FrameSpikes {
    pub fn with_threshold(threshold_ms: f64) -> FrameSpikes {
        FrameSpikes { threshold_ms, ..default() }
    }

    /// Worst and p99 (ms) of the last `WINDOW_S` seconds.
    pub fn window(&self) -> (f64, f64) {
        let mut v: Vec<f64> = self.recent.iter().map(|x| x.1).collect();
        if v.is_empty() {
            return (0.0, 0.0);
        }
        v.sort_by(f64::total_cmp);
        (v[v.len() - 1], percentile(&v, 0.99))
    }

    /// The F3 line.
    pub fn hud_line(&self) -> String {
        let (worst, p99) = self.window();
        let last = self.last_spike.as_ref().map(|r| format!("  last spike {:.0} ms {:.0} s ago", r.dt_ms, self.recent.back().map_or(0.0, |x| x.0) - r.t)).unwrap_or_default();
        format!("frame worst {worst:.1} ms  p99 {p99:.1} ms ({WINDOW_S:.0} s)  over {:.0} ms: {}{last}", self.threshold_ms, self.over)
    }

    fn close(&mut self, rec: FrameRec) {
        self.frames += 1;
        self.recent.push_back((rec.t, rec.dt_ms));
        while self.recent.front().is_some_and(|x| rec.t - x.0 > WINDOW_S) {
            self.recent.pop_front();
        }
        if rec.dt_ms > self.threshold_ms {
            self.over += 1;
            if self.log {
                println!("spike: {}", rec.line());
            }
            self.last_spike = Some(rec.clone());
        }
        if self.worst.len() < WORST_KEPT || rec.dt_ms > self.worst[self.worst.len() - 1].dt_ms {
            let at = self.worst.partition_point(|w| w.dt_ms >= rec.dt_ms);
            self.worst.insert(at, rec.clone());
            self.worst.truncate(WORST_KEPT);
        }
        if self.keep_all {
            self.all.push(rec);
        }
    }

    /// Forget the frames so far (a scenario's measurement starts).
    pub fn reset(&mut self) {
        let keep = (self.threshold_ms, self.log, self.keep_all);
        *self = FrameSpikes::default();
        (self.threshold_ms, self.log, self.keep_all) = keep;
    }
}

/// Nearest-rank percentile of a sorted list.
pub fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

/// p50, p99, max and the count above `over` of a list of frame times.
pub fn summary(ms: &[f64], over: f64) -> (f64, f64, f64, usize) {
    let mut v = ms.to_vec();
    v.sort_by(f64::total_cmp);
    (percentile(&v, 0.5), percentile(&v, 0.99), v.last().copied().unwrap_or(0.0), v.iter().filter(|&&x| x > over).count())
}

fn frame_begin(mut spikes: ResMut<FrameSpikes>, mut cost: ResMut<FrameCost>, script: Option<Res<crate::scenario::Script>>, shared: Res<RenderShared>) {
    let now = Instant::now();
    let start = *spikes.start.get_or_insert(now);
    let render = std::mem::take(&mut shared.0.lock().unwrap().0);
    if let (Some(mut rec), Some(t0)) = (spikes.open.take(), spikes.frame_start) {
        rec.dt_ms = (now - t0).as_secs_f64() * 1000.0;
        rec.render = render;
        spikes.close(rec);
    }
    spikes.frame_start = Some(now);
    let frame = spikes.frames;
    *cost = FrameCost::default();
    spikes.open = Some(FrameRec { frame, t: (now - start).as_secs_f64(), phase: script.map(|s| s.ctx.phase.clone()).unwrap_or_default(), ..default() });
}

fn frame_end(mut spikes: ResMut<FrameSpikes>, cost: Res<FrameCost>) {
    let Some(t0) = spikes.frame_start else { return };
    let main_ms = t0.elapsed().as_secs_f64() * 1000.0;
    if let Some(rec) = spikes.open.as_mut() {
        rec.main_ms = main_ms;
        rec.cost = cost.clone();
    }
}

#[derive(Resource, Default)]
struct Marks {
    fixed: Option<Instant>,
    update: Option<Instant>,
    propagate: Option<Instant>,
    phys: Option<Instant>,
    fixed_update: Option<Instant>,
    prep: Option<Instant>,
}

/// Closes one Avian stage: the time since the previous mark goes to `phys[N]`.
fn phys_mark<const N: usize>(mut m: ResMut<Marks>, mut cost: ResMut<FrameCost>) {
    let now = Instant::now();
    if let Some(t) = m.phys.replace(now) {
        cost.phys[N] += now.duration_since(t).as_secs_f64() * 1000.0;
    }
}
fn phys_ccd(d: Res<avian3d::dynamics::solver::SolverDiagnostics>, mut cost: ResMut<FrameCost>) {
    cost.ccd_ms += d.swept_ccd.as_secs_f64() * 1000.0;
}
fn prep_begin(mut m: ResMut<Marks>) {
    m.prep = Some(Instant::now());
}
fn prep_end(mut m: ResMut<Marks>, mut cost: ResMut<FrameCost>) {
    if let Some(t) = m.prep.take() {
        cost.prep_ms += t.elapsed().as_secs_f64() * 1000.0;
    }
}
fn wb_end(mut m: ResMut<Marks>, mut cost: ResMut<FrameCost>) {
    if let Some(t) = m.prep.take() {
        cost.wb_ms += t.elapsed().as_secs_f64() * 1000.0;
    }
}
fn fu_begin(mut m: ResMut<Marks>) {
    m.fixed_update = Some(Instant::now());
}
fn fu_end(mut m: ResMut<Marks>, mut cost: ResMut<FrameCost>) {
    if let Some(t) = m.fixed_update.take() {
        cost.fixed_update_ms += t.elapsed().as_secs_f64() * 1000.0;
    }
}
fn phys_start(mut m: ResMut<Marks>) {
    m.phys = Some(Instant::now());
}

fn fixed_begin(mut m: ResMut<Marks>) {
    m.fixed = Some(Instant::now());
}
fn fixed_end(mut m: ResMut<Marks>, mut cost: ResMut<FrameCost>) {
    if let Some(t) = m.fixed.take() {
        cost.fixed.add(1, t);
    }
}
fn update_begin(mut m: ResMut<Marks>) {
    m.update = Some(Instant::now());
}
fn update_end(mut m: ResMut<Marks>, mut cost: ResMut<FrameCost>) {
    if let Some(t) = m.update.take() {
        cost.update.add(0, t);
    }
}
fn propagate_begin(mut m: ResMut<Marks>) {
    m.propagate = Some(Instant::now());
}
fn propagate_end(mut m: ResMut<Marks>, mut cost: ResMut<FrameCost>) {
    if let Some(t) = m.propagate.take() {
        cost.propagate.add(0, t);
    }
}

/// Marks between the render stages: mark `i` closes stage `i - 1`.
fn render_mark<const I: usize>(shared: Res<RenderShared>) {
    let now = Instant::now();
    let mut g = shared.0.lock().unwrap();
    let (cost, marks) = &mut *g;
    marks[I] = Some(now);
    if I == 0 {
        return;
    }
    let Some(prev) = marks[I - 1] else { return };
    let ms = (now - prev).as_secs_f64() * 1000.0;
    match I {
        1 => cost.assets_ms += ms,
        2 => cost.views_ms += ms,
        3 => cost.queue_ms += ms,
        4 => cost.prepare_ms += ms,
        5 => cost.graph_ms += ms,
        _ => {
            cost.cleanup_ms += ms;
            if let Some(m0) = marks[0] {
                cost.total_ms += (now - m0).as_secs_f64() * 1000.0;
                cost.frames += 1;
            }
        }
    }
}

fn render_plugin(app: &mut App, shared: RenderShared) {
    use bevy::render::{Render, RenderApp, RenderSystems as R};
    let Some(render) = app.get_sub_app_mut(RenderApp) else { return };
    render.insert_resource(shared);
    render.add_systems(
        Render,
        (
            render_mark::<0>.before(R::ExtractCommands),
            render_mark::<1>.after(R::PrepareMeshes).after(R::PrepareAssets).before(R::CreateViews),
            render_mark::<2>.after(R::Specialize).before(R::PrepareViews).before(R::Queue),
            render_mark::<3>.after(R::Queue).before(R::PhaseSort),
            render_mark::<4>.after(R::Prepare).before(R::Render),
            render_mark::<5>.after(R::Render).before(R::Cleanup),
            render_mark::<6>.after(R::Cleanup).in_set(R::PostCleanup),
        ),
    );
}

pub fn plugin(app: &mut App) {
    let shared = RenderShared::default();
    app.insert_resource(shared.clone());
    render_plugin(app, shared);
    app.init_resource::<FrameCost>().init_resource::<Marks>();
    if !app.world().contains_resource::<FrameSpikes>() {
        app.init_resource::<FrameSpikes>();
    }
    app.add_systems(First, frame_begin);
    app.add_systems(Last, frame_end);
    app.add_systems(FixedFirst, fixed_begin);
    app.add_systems(FixedLast, fixed_end);
    app.add_systems(FixedUpdate, (fu_begin.before(crate::phases::Fx::Input), fu_end.after(crate::phases::Fx::Effects)));
    {
        use avian3d::prelude::{PhysicsSchedule, PhysicsStepSystems as S, PhysicsSystems as P};
        app.add_systems(
            FixedPostUpdate,
            (
                prep_begin.before(P::Prepare),
                prep_end.after(P::Prepare).before(P::StepSimulation),
                prep_begin.after(P::StepSimulation).before(P::Writeback),
                wb_end.after(P::Writeback),
            ),
        );
        app.add_systems(
            PhysicsSchedule,
            (
                phys_start.in_set(S::First),
                phys_mark::<0>.after(S::BroadPhase).before(S::NarrowPhase),
                phys_mark::<1>.after(S::NarrowPhase).before(S::Solver),
                phys_mark::<2>.after(S::Solver).before(S::Sleeping),
                (phys_mark::<3>, phys_ccd).chain().in_set(S::Last),
            ),
        );
    }
    app.add_systems(Update, (update_begin.before(crate::phases::Frame::Input), update_end.after(crate::phases::Frame::Hud)));
    app.add_systems(
        PostUpdate,
        (propagate_begin.before(TransformSystems::Propagate).after(crate::origin::sync_world_pos).after(crate::origin::sync_bodies).after(crate::origin::sync_world_pose), propagate_end.after(TransformSystems::Propagate)),
    );
}
