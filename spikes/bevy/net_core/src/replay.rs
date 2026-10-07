//! Replay matrix of spike 4 without sockets: a recorded 60 Hz path of one player (ship and
//! walker) goes through encode, fault-injected links, decode, buffer and playout, and the shown
//! pose is compared with the sender's path at the same delayed timestamp. Port of test.gd.
use crate::buffer::{between, Buffer, Mode};
use crate::link::Link;
use crate::metrics::{mean, percentile, rms};
use crate::snapshot::{Snapshot, SIZE};
use glam::DVec3;
use std::fmt::Write as _;
use std::time::Instant;

pub const DT: f64 = 1.0 / 60.0;

/// Recorded states at 60 Hz. `flags` of each state is the index of its phase in `phases`.
pub struct Trajectory {
    pub phases: Vec<String>,
    pub states: Vec<Snapshot>,
}

impl Trajectory {
    /// "EXOPATH1\n<count>\n<phase|phase|...>\n" followed by `count` 144-byte snapshots.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = format!("EXOPATH1\n{}\n{}\n", self.states.len(), self.phases.join("|")).into_bytes();
        for s in &self.states {
            b.extend_from_slice(&s.encode());
        }
        b
    }

    pub fn parse(b: &[u8]) -> Result<Trajectory, String> {
        let mut at = 0usize;
        let mut line = || -> Result<&str, String> {
            let end = b[at..].iter().position(|&c| c == b'\n').ok_or("truncated header")?;
            let s = std::str::from_utf8(&b[at..at + end]).map_err(|e| e.to_string())?;
            at += end + 1;
            Ok(s)
        };
        if line()? != "EXOPATH1" {
            return Err("not an EXOPATH1 file".into());
        }
        let n: usize = line()?.parse().map_err(|_| "bad count")?;
        let phases = line()?.split('|').map(str::to_string).collect();
        if b.len() != at + n * SIZE {
            return Err(format!("size {} does not match {} states", b.len(), n));
        }
        let states = (0..n).map(|i| Snapshot::decode(&b[at + i * SIZE..at + (i + 1) * SIZE]).ok_or("bad state")).collect::<Result<_, _>>()?;
        Ok(Trajectory { phases, states })
    }

    /// Truth at an arbitrary time: Hermite between the two 60 Hz states around it.
    pub fn truth(&self, t: f64) -> Snapshot {
        let i = ((t / DT).floor().max(0.0) as usize).min(self.states.len() - 2);
        between(&self.states[i], &self.states[i + 1], t).s
    }

    pub fn phase_at(&self, t: f64) -> usize {
        let i = ((t / DT).floor().max(0.0) as usize).min(self.states.len() - 1);
        self.states[i].flags as usize
    }

    pub fn seconds(&self) -> f64 {
        self.states.len() as f64 * DT
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Case {
    pub players: u32,
    pub rate: u32,
    pub buffer_ms: u32,
    pub delay_ms: u32,
    pub loss_percent: u32,
}

/// The 96 cases of spike 4: players 2/8 x rate 20/30 x buffer 100/150 x delay 0/50/150 x loss 0/1/5/10.
pub fn matrix_cases() -> Vec<Case> {
    let mut v = Vec::new();
    for players in [2, 8] {
        for rate in [20, 30] {
            for buffer_ms in [100, 150] {
                for delay_ms in [0, 50, 150] {
                    for loss_percent in [0, 1, 5, 10] {
                        v.push(Case { players, rate, buffer_ms, delay_ms, loss_percent });
                    }
                }
            }
        }
    }
    v
}

#[derive(Default, Clone, Debug)]
pub struct PhaseMetrics {
    pub error_rms_mm: f64,
    pub error_p95_mm: f64,
    pub error_max_mm: f64,
    pub step_error_p95_mm: f64,
    pub jerk_p95_m_s3: f64,
    pub rotation_p95_deg: f64,
    pub hold_percent: f64,
    pub samples: usize,
}

#[derive(Clone, Debug)]
pub struct CaseResult {
    pub case: Case,
    pub phases: Vec<(String, PhaseMetrics)>,
    /// All phases together.
    pub all: PhaseMetrics,
    pub payload_tx_kb_s: f64,
    pub cpu_mean_ms: f64,
    pub cpu_p95_ms: f64,
    pub dropped: u64,
}

#[derive(Default)]
struct Acc {
    errors: Vec<f64>,
    steps: Vec<f64>,
    jerk: Vec<f64>,
    rotation: Vec<f64>,
    holds: usize,
    samples: usize,
}

impl Acc {
    fn finish(mut self) -> PhaseMetrics {
        PhaseMetrics {
            error_rms_mm: rms(&self.errors),
            error_p95_mm: percentile(&mut self.errors, 0.95),
            error_max_mm: percentile(&mut self.errors, 1.0),
            step_error_p95_mm: percentile(&mut self.steps, 0.95),
            jerk_p95_m_s3: percentile(&mut self.jerk, 0.95),
            rotation_p95_deg: percentile(&mut self.rotation, 0.95),
            hold_percent: self.holds as f64 / self.samples.max(1) as f64 * 100.0,
            samples: self.samples,
        }
    }
}

pub fn run_case(tr: &Trajectory, case: Case) -> CaseResult {
    let remotes = (case.players - 1) as usize;
    let mut links: Vec<Link<[u8; SIZE]>> = (0..remotes).map(|o| Link::new(case.delay_ms as f64, if case.delay_ms > 0 { 20.0 } else { 0.0 }, case.loss_percent as f64, 4400 + o as u64)).collect();
    let mut buffers: Vec<Buffer> = (0..remotes).map(|_| Buffer::new()).collect();
    let mut last_err = vec![DVec3::ZERO; remotes];
    let mut last_pos = vec![DVec3::ZERO; remotes];
    let mut last_vel = vec![DVec3::ZERO; remotes];
    let mut last_acc = vec![DVec3::ZERO; remotes];
    let mut acc: Vec<Acc> = tr.phases.iter().map(|_| Acc::default()).collect();
    let mut all = Acc::default();
    let mut cpu = Vec::with_capacity(tr.states.len());
    let mut bytes = 0usize;
    let step = (60 / case.rate) as usize;
    let delay = case.delay_ms as f64 / 1000.0;
    let buffering = case.buffer_ms as f64 / 1000.0;
    for i in 0..tr.states.len() {
        let now = i as f64 * DT;
        let target = now - delay - buffering;
        let begin = Instant::now();
        if i % step == 0 {
            for (o, link) in links.iter_mut().enumerate() {
                let mut s = tr.states[i];
                s.owner = o as u32 + 2;
                s.seq = i as u32;
                s.p.x += o as f64 * 20.0;
                let wire = s.encode();
                bytes += wire.len();
                link.enqueue(now, wire);
            }
        }
        for o in 0..remotes {
            for item in links[o].ready(now) {
                if let Some(s) = Snapshot::decode(&item) {
                    buffers[o].push(s);
                }
            }
            let Some(shown) = buffers[o].sample(target) else { continue };
            if target < 0.5 {
                continue;
            }
            let mut truth = tr.truth(target);
            truth.p.x += o as f64 * 20.0;
            let error = shown.s.p - truth.p;
            let phase = tr.phase_at(target).min(acc.len() - 1);
            let rotation = shown.s.q.angle_between(truth.q).to_degrees();
            for a in [&mut acc[phase], &mut all] {
                a.errors.push(error.length() * 1000.0);
                a.rotation.push(rotation);
                a.samples += 1;
                if shown.mode == Mode::Hold {
                    a.holds += 1;
                }
            }
            if target > 0.5 + 3.0 * DT {
                let velocity = (shown.s.p - last_pos[o]) / DT;
                let acceleration = (velocity - last_vel[o]) / DT;
                let jerk = (acceleration - last_acc[o]).length() / DT;
                let step_err = (error - last_err[o]).length() * 1000.0;
                for a in [&mut acc[phase], &mut all] {
                    a.steps.push(step_err);
                    a.jerk.push(jerk);
                }
                last_vel[o] = velocity;
                last_acc[o] = acceleration;
            }
            last_err[o] = error;
            last_pos[o] = shown.s.p;
        }
        cpu.push(begin.elapsed().as_secs_f64() * 1000.0);
    }
    let dropped = links.iter().map(|l| l.dropped).sum();
    let phases = tr.phases.iter().cloned().zip(acc.into_iter().map(Acc::finish)).filter(|(_, m)| m.samples > 0).collect();
    CaseResult {
        case,
        phases,
        all: all.finish(),
        payload_tx_kb_s: bytes as f64 / tr.seconds() / 1000.0,
        cpu_mean_ms: mean(&cpu),
        cpu_p95_ms: percentile(&mut cpu, 0.95),
        dropped,
    }
}

pub fn run_matrix(tr: &Trajectory) -> Vec<CaseResult> {
    matrix_cases().into_iter().map(|c| run_case(tr, c)).collect()
}

fn metrics_json(m: &PhaseMetrics) -> String {
    format!(
        "{{\"error_rms_mm\":{:.4},\"error_p95_mm\":{:.4},\"error_max_mm\":{:.4},\"step_error_p95_mm\":{:.4},\"jerk_p95_m_s3\":{:.1},\"rotation_p95_deg\":{:.4},\"hold_percent\":{:.4},\"samples\":{}}}",
        m.error_rms_mm, m.error_p95_mm, m.error_max_mm, m.step_error_p95_mm, m.jerk_p95_m_s3, m.rotation_p95_deg, m.hold_percent, m.samples
    )
}

pub fn to_json(tr: &Trajectory, results: &[CaseResult]) -> String {
    let mut s = String::from("{\n  \"seconds\": ");
    let _ = write!(s, "{:.2},\n  \"cases\": [\n", tr.seconds());
    for (i, r) in results.iter().enumerate() {
        let c = r.case;
        let _ = write!(
            s,
            "    {{\"players\":{},\"rate\":{},\"buffer_ms\":{},\"delay_ms\":{},\"loss_percent\":{},\"payload_tx_kB_s\":{:.3},\"cpu_mean_ms\":{:.4},\"cpu_p95_ms\":{:.4},\"dropped\":{},\"all\":{},\"phases\":{{",
            c.players, c.rate, c.buffer_ms, c.delay_ms, c.loss_percent, r.payload_tx_kb_s, r.cpu_mean_ms, r.cpu_p95_ms, r.dropped, metrics_json(&r.all)
        );
        for (j, (name, m)) in r.phases.iter().enumerate() {
            let _ = write!(s, "{}\"{}\":{}", if j > 0 { "," } else { "" }, name.replace('"', "'"), metrics_json(m));
        }
        let _ = writeln!(s, "}}}}{}", if i + 1 < results.len() { "," } else { "" });
    }
    s.push_str("  ]\n}\n");
    s
}

/// A smooth stand-in path for unit tests: takeoff, cruise, a turn, landing, idle (spike 4 phases,
/// 25 s), analytic positions and velocities.
pub fn synthetic_path() -> Trajectory {
    use crate::snapshot::Snapshot;
    use glam::DQuat;
    let phases = ["takeoff", "cruise", "turn", "landing", "idle"].map(String::from).to_vec();
    let mut states = Vec::new();
    // Heading and speed integrated numerically at 600 Hz, sampled at 60 Hz.
    let (mut p, mut v, mut yaw) = (DVec3::new(0.0, 5000.0, 0.0), DVec3::ZERO, 0.0f64);
    let sub = 10;
    for i in 0..1501 {
        let t = i as f64 * DT;
        let phase = if t < 4.0 { 0 } else if t < 10.0 { 1 } else if t < 16.0 { 2 } else if t < 24.0 { 3 } else { 4 };
        let mut s = Snapshot::new(1, t, p, v, DQuat::from_rotation_y(yaw));
        s.flags = phase;
        states.push(s);
        for _ in 0..sub {
            let h = DT / sub as f64;
            let fwd = DVec3::new(-yaw.sin(), 0.0, -yaw.cos());
            let acc = match phase {
                0 => DVec3::new(0.0, 12.0 * (1.0 - (t / 4.0)).max(0.2), 0.0) * if v.y < 60.0 { 1.0 } else { 0.0 },
                1 | 2 => fwd * (30.0 - v.dot(fwd)).clamp(-20.0, 20.0) * 1.5,
                3 => DVec3::new(0.0, -3.0, 0.0) * if p.y > 5000.1 { 1.0 } else { 0.0 },
                _ => -v * 3.0,
            };
            v += acc * h;
            if phase == 2 {
                yaw += 0.18 * h;
            }
            p += v * h;
            if p.y < 5000.0 {
                p.y = 5000.0;
                v.y = v.y.max(0.0);
            }
        }
    }
    Trajectory { phases, states }
}
