//! `planet-flight`: a repeatable flight for frame spikes on a big planet (run it with
//! `--radius=100000 --atmosphere=11670`). The ship is moved on a fixed path (teleport per tick,
//! velocity set so the ring and the camera look ahead as in flight), not by the flight model, so
//! every run streams the same terrain:
//!
//! 1. low NAV: 800 m/s, 300 m above the ground, terrain following, 30 s;
//! 2. approach from orbit: from two atmosphere heights down to 300 m above the ground at 800 m/s
//!    along the ground, 30 s, then 5 s low;
//! 3. low SCM boost: 350 m/s at 40 m above the ground (inside the collision ring), 20 s.
//!
//! Each leg starts after a hold (the warm-up, not measured) that lets its start load, with the
//! ship pinned at the start point but already given the leg's velocity. Per leg it reports frame
//! time p50, p99, max, the frames above the spike threshold, the LOD debt (share of frames whose
//! chunk under the ship is coarser than wanted), a `target:` line against the frame goal
//! (p99 <= 8.3 ms, max <= 25 ms, LOD debt <= 2 %) and the work in the worst frames
//! (`frame_cost`); all frames go to `<out>/frames-planet-flight.csv`. Headless it measures the
//! main thread only (no GPU). Report only: timings are not checked (a shared machine is noisy).
use super::*;
use crate::frame_cost::{summary, FrameRec, FrameSpikes};

/// One leg: name, hold before it (s), duration (s), and the path: time -> (direction from the
/// planet centre, height above the ground or the base radius, speed along the ground).
struct Leg {
    name: &'static str,
    hold: f64,
    secs: f64,
    /// Start direction (planet space, normalised) and heading (tangent).
    start: DVec3,
    heading: DVec3,
    speed: f64,
    /// Height above the ground at time t (s into the leg); `None` = follow nothing, height above
    /// the base radius from `orbit_height`.
    agl: fn(f64, f64) -> f64,
}

/// Pose at `t` s into a leg: position (planet space) and velocity.
fn leg_pose(planet: &PlanetRes, leg: &Leg, atmosphere: f64, t: f64) -> (DVec3, DVec3) {
    let r = planet.radius;
    let at = |t: f64| {
        // Great circle from `start` along `heading`.
        let ang = leg.speed * t / r;
        let dir = leg.start * ang.cos() + leg.heading * ang.sin();
        // Terrain following: the higher ground of here and 1.5 s ahead.
        let ahead = leg.start * (ang + leg.speed * 1.5 / r).cos() + leg.heading * (ang + leg.speed * 1.5 / r).sin();
        let ground = planet.surface(dir).max(planet.surface(ahead));
        dir * (ground + (leg.agl)(t, atmosphere))
    };
    let p = at(t);
    (p, (at(t + 0.05) - p) / 0.05)
}

fn pose_ship(w: &mut World, pos: DVec3, vel: DVec3) {
    let pl = planet(w);
    let up = (pos).normalize();
    let fwd = (vel - up * vel.dot(up)).try_normalize().unwrap_or_else(|| up.any_orthonormal_vector());
    let rot = DQuat::from_mat3(&bevy::math::DMat3::from_cols(fwd.cross(up), up, -fwd));
    teleport_ship(w, pl.centre + pos, rot);
    let e = ship_e(w);
    w.get_mut::<LinearVelocity>(e).unwrap().0 = vel;
}

/// The frame goal: p99 and worst frame (ms), LOD debt (%).
const TARGET_P99: f64 = 8.3;
const TARGET_MAX: f64 = 25.0;
const TARGET_DEBT: f64 = 2.0;

fn spikes(w: &mut World) -> Mut<'_, FrameSpikes> {
    w.resource_mut::<FrameSpikes>()
}

/// The leg's frames from the frame list, summarised into the report.
fn leg_report(c: &mut Ctx, name: &str, frames: &[FrameRec], threshold: f64) {
    let dt: Vec<f64> = frames.iter().map(|f| f.dt_ms).collect();
    let main: Vec<f64> = frames.iter().map(|f| f.main_ms).collect();
    let (p50, p99, max, over) = summary(&dt, threshold);
    let (m50, m99, mmax, _) = summary(&main, threshold);
    let sum = |f: fn(&FrameRec) -> (u32, f64)| frames.iter().map(f).fold((0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1));
    let terrain = sum(|f| (f.cost.terrain_up.n, f.cost.terrain_up.ms));
    let ring = sum(|f| (f.cost.ring_up.n, f.cost.ring_up.ms));
    let scatter = sum(|f| (f.cost.scatter_up.n, f.cost.scatter_up.ms));
    let shift = sum(|f| (f.cost.shift.n.min(1), f.cost.shift.ms));
    let (s_ents, s_inst) = frames.iter().fold((0, 0), |a, f| (a.0 + f.cost.scatter_ents, a.1 + f.cost.scatter_inst));
    let line = format!(
        "{name}: {} frames, frame p50 {p50:.1} p99 {p99:.1} max {max:.1} ms, {over} over {threshold:.0} ms | main p50 {m50:.1} p99 {m99:.1} max {mmax:.1} ms | terrain {} uploads {:.0} ms, ring {} patches {:.0} ms, scatter {} cells ({s_ents} entities, {s_inst} instances) {:.0} ms, origin shifts {} {:.0} ms",
        frames.len(), terrain.0, terrain.1, ring.0, ring.1, scatter.0, scatter.1, shift.0, shift.1
    );
    println!("{line}");
    c.report.push(line);
    let debt = frames.iter().filter(|f| f.cost.lod_debt).count() as f64 / frames.len().max(1) as f64 * 100.0;
    let queue = frames.iter().map(|f| f.cost.terrain_pending).max().unwrap_or(0);
    let ok = p99 <= TARGET_P99 && max <= TARGET_MAX && debt <= TARGET_DEBT;
    let line = format!(
        "target: {name}: {} (p99 {p99:.1}/{TARGET_P99} ms, max {max:.1}/{TARGET_MAX} ms, LOD debt {debt:.1}/{TARGET_DEBT} %, chunk queue max {queue})",
        if ok { "MET" } else { "MISSED" }
    );
    println!("{line}");
    c.report.push(line);
    let mut worst: Vec<&FrameRec> = frames.iter().collect();
    worst.sort_by(|a, b| b.dt_ms.total_cmp(&a.dt_ms));
    for f in worst.iter().take(5) {
        let l = format!("  worst: {}", f.line());
        println!("{l}");
        c.report.push(l);
    }
}

fn write_csv(dir: &std::path::Path, frames: &[FrameRec]) {
    let mut s = String::from("frame,t,phase,dt_ms,main_ms,fixed_n,fixed_ms,update_ms,propagate_ms,terrain_up,terrain_up_ms,terrain_sys_ms,terrain_started,ring_up,ring_up_ms,ring_sys_ms,scatter_cells,scatter_inst,scatter_up_ms,scatter_sys_ms,shift_n,shift_ms,render_n,render_ms,r_assets,r_views,r_queue,r_prepare,r_graph,r_cleanup,lod_debt,terrain_pending,gpu_ms\n");
    for f in frames {
        let c = &f.cost;
        s.push_str(&format!(
            "{},{:.4},{},{:.3},{:.3},{},{:.3},{:.3},{:.3},{},{:.3},{:.3},{},{},{:.3},{:.3},{},{},{:.3},{:.3},{},{:.3},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{:.3}\n",
            f.frame, f.t, f.phase.replace(",", ";"), f.dt_ms, f.main_ms, c.fixed.n, c.fixed.ms, c.update.ms, c.propagate.ms, c.terrain_up.n, c.terrain_up.ms, c.terrain_sys.ms, c.terrain_sys.n,
            c.ring_up.n, c.ring_up.ms, c.ring_sys.ms, c.scatter_up.n, c.scatter_inst, c.scatter_up.ms, c.scatter_sys.ms, c.shift.n, c.shift.ms,
            f.render.frames, f.render.total_ms, f.render.assets_ms, f.render.views_ms, f.render.queue_ms, f.render.prepare_ms, f.render.graph_ms, f.render.cleanup_ms, c.lod_debt as u8, c.terrain_pending, c.gpu_ms
        ));
    }
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join("frames-planet-flight.csv");
    let _ = std::fs::write(&path, s);
    println!("frames: {}", path.display());
}

pub(super) fn planet_flight_steps(s: &mut Vec<Step>, out_dir: &std::path::Path) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        spikes(w).keep_all = true;
        true
    }));
    s.extend(sit());
    // Three legs on different parts of the planet (fresh terrain each), all from the +y side.
    let a = DVec3::new(0.3, 1.0, 0.2).normalize();
    let east = |d: DVec3| DVec3::Y.cross(d).normalize();
    let legs = vec![
        Leg { name: "low NAV 800 m/s, 300 m", hold: 8.0, secs: 30.0, start: a, heading: east(a), speed: 800.0, agl: |_, _| 300.0 },
        Leg {
            name: "approach from orbit at 800 m/s",
            hold: 6.0,
            secs: 35.0,
            start: DVec3::new(-0.6, 0.7, 0.4).normalize(),
            heading: east(DVec3::new(-0.6, 0.7, 0.4).normalize()),
            speed: 800.0,
            // From two atmospheres down to 300 m in 30 s (smooth), then low.
            agl: |t, atm| {
                let k = (t / 30.0).clamp(0.0, 1.0);
                let ease = 1.0 - (1.0 - k) * (1.0 - k);
                2.0 * atm + (300.0 - 2.0 * atm) * ease
            },
        },
        Leg { name: "low SCM 350 m/s, 40 m", hold: 6.0, secs: 20.0, start: DVec3::new(0.5, 0.8, -0.3).normalize(), heading: east(DVec3::new(0.5, 0.8, -0.3).normalize()), speed: 350.0, agl: |_, _| 40.0 },
    ];
    let legs: Vec<std::sync::Arc<Leg>> = legs.into_iter().map(std::sync::Arc::new).collect();
    for leg in legs {
        let l = leg.clone();
        s.push(Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, &format!("hold before {}", l.name));
            }
            let atm = w.resource::<SystemRes>().0.planet(w.resource::<PlanetRes>().id).atmosphere_height;
            let pl = planet(w);
            // Held at the start but already with the leg's velocity: what first shows in flight
            // (engine effects, their pipelines) warms up here, not in the measured frames.
            let (p, v) = leg_pose(&pl, &l, atm, 0.0);
            pose_ship(w, p, v);
            if c.t >= l.hold {
                c.phase.clear();
                spikes(w).reset();
                return true;
            }
            false
        }));
        let l = leg.clone();
        s.push(Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, l.name);
                c.p.insert("from", active_pos(w));
            }
            let atm = w.resource::<SystemRes>().0.planet(w.resource::<PlanetRes>().id).atmosphere_height;
            let pl = planet(w);
            let (p, v) = leg_pose(&pl, &l, atm, c.t);
            pose_ship(w, p, v);
            if c.t < l.secs {
                return false;
            }
            let frames = std::mem::take(&mut spikes(w).all);
            let threshold = spikes(w).threshold_ms;
            let flown = active_pos(w).distance(c.p["from"]);
            leg_report(c, l.name, &frames, threshold);
            check(c, flown > l.speed * l.secs * 0.9, format!("{}: flew {:.1} km", l.name, flown / 1000.0));
            check(c, frames.len() > 10, format!("{}: {} frames measured", l.name, frames.len()));
            let mut all = std::mem::take(&mut c.frames);
            all.extend(frames);
            c.frames = all;
            c.phase.clear();
            true
        }));
    }
    let dir = out_dir.to_path_buf();
    s.push(Box::new(move |_, c| {
        write_csv(&dir, &c.frames);
        true
    }));
}

/// `lod-morph`: a fixed slow descent (700 m down to 80 m above the ground in 21 s, 30 m/s) over a big planet
/// with a screenshot every 0.25 s (windowed), to compare the LOD steps with geomorphing on and
/// off: run it twice, the second time with `EXO_NO_MORPH=1`. Shots land in `<out>/lod-morph/`;
/// `EXO_LOD_DENSE=1` shoots every 1/30 s instead, for a frame-difference check of the pops.
pub(super) fn lod_morph_steps(s: &mut Vec<Step>, out_dir: &std::path::Path, windowed: bool) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    let a = DVec3::new(0.3, 1.0, 0.2).normalize();
    let leg = std::sync::Arc::new(Leg { name: "slow descent 30 m/s, 700 to 80 m", hold: 8.0, secs: 21.0, start: a, heading: DVec3::Y.cross(a).normalize(), speed: 30.0, agl: |t, _| 700.0 - 620.0 * (t / 21.0).clamp(0.0, 1.0) });
    let l = leg.clone();
    s.push(Box::new(move |w, c| {
        let pl = planet(w);
        let (p, _) = leg_pose(&pl, &l, 0.0, 0.0);
        pose_ship(w, p, DVec3::ZERO);
        c.t >= l.hold
    }));
    let dir = out_dir.join("lod-morph");
    s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, leg.name);
            c.v.insert("next", 0.0);
        }
        let pl = planet(w);
        let (p, v) = leg_pose(&pl, &leg, 0.0, c.t);
        pose_ship(w, p, v);
        if c.t >= c.v["next"] {
            shot(w, c, &dir, windowed, &format!("t{:04}", (c.t * 100.0).round() as u32));
            *c.v.get_mut("next").unwrap() += if std::env::var("EXO_LOD_DENSE").is_ok() { 1.0 / 30.0 } else { 0.25 };
        }
        if c.t < leg.secs {
            return false;
        }
        c.phase.clear();
        true
    }));
}
