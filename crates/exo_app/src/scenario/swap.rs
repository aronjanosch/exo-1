//! Planet swaps (#14, #34): what a warp to another planet frees and keeps.
use super::*;

/// Ticks between a planet swap and its count: the freed meshes leave `Assets` a frame later,
/// and the ship is still about 1000 km out (only the new planet's roots, no refinement yet).
const SWAP_AUDIT_DELAY: u64 = 30;

/// What one planet swap left behind, counted `SWAP_AUDIT_DELAY` ticks after it.
#[derive(Clone, Debug)]
pub struct SwapRow {
    pub from: PlanetId,
    pub to: PlanetId,
    pub drop: bool,
    /// Terrain chunks, entities (all of its scene) and meshes of the departed planet still there.
    pub old_chunks: usize,
    pub old_entities: usize,
    pub old_meshes: usize,
    /// The departed planet's generator is freed (no `Arc` left anywhere).
    pub old_freed: bool,
    /// Whole world: terrain chunks, entities, meshes.
    pub chunks: usize,
    pub entities: u32,
    pub meshes: usize,
    /// Root chunks built on the main thread on the swap frame (#34).
    pub roots_built_here: usize,
    pub rss_mb: Option<f64>,
}

/// Scenario `swap`: watches `WarpTelemetry::swaps` and counts what each swap left behind.
#[derive(Resource, Default)]
pub struct SwapAudit {
    tick: u64,
    seen: usize,
    /// The current planet's generator, kept weak (taken every tick, so at a swap it is the old one's).
    last: Option<std::sync::Weak<planet_core::Planet>>,
    due: Vec<(u64, PlanetId, PlanetId, bool, Vec<AssetId<Mesh>>, std::sync::Weak<planet_core::Planet>)>,
    pub rows: Vec<SwapRow>,
}

/// Runs after `planet_swap` in the fixed step, before the terrain is rebuilt (Update): the old
/// terrain still knows its meshes.
pub fn swap_audit(w: &mut World) {
    let swaps = w.resource::<WarpTelemetry>().swaps.clone();
    let old_meshes = w.get_resource::<crate::terrain::Terrain>().map(|t| t.mesh_ids()).unwrap_or_default();
    let weak = std::sync::Arc::downgrade(&w.resource::<PlanetRes>().pgen);
    let mut a = w.resource_mut::<SwapAudit>();
    a.tick += 1;
    while a.seen < swaps.len() {
        let (_, from, to, drop) = swaps[a.seen];
        let old = a.last.clone().unwrap_or_default();
        let due = a.tick + SWAP_AUDIT_DELAY;
        a.due.push((due, from, to, drop, old_meshes.clone(), old));
        a.seen += 1;
    }
    a.last = Some(weak);
    let tick = a.tick;
    let ready: Vec<_> = a.due.iter().filter(|d| d.0 <= tick).cloned().collect();
    a.due.retain(|d| d.0 > tick);
    for (_, from, to, drop, meshes, old) in ready {
        let mut q = w.query::<(&crate::terrain::PlanetScene, Has<crate::terrain::TerrainChunk>)>();
        let (mut old_chunks, mut old_entities, mut chunks) = (0, 0, 0);
        for (ps, chunk) in q.iter(w) {
            old_entities += (ps.0 == from) as usize;
            old_chunks += (ps.0 == from && chunk) as usize;
            chunks += chunk as usize;
        }
        let assets = w.resource::<Assets<Mesh>>();
        let row = SwapRow {
            from,
            to,
            drop,
            old_chunks,
            old_entities,
            old_meshes: meshes.iter().filter(|id| assets.contains(**id)).count(),
            old_freed: old.upgrade().is_none(),
            chunks,
            entities: w.entities().count_spawned(),
            meshes: assets.len(),
            roots_built_here: w.get_resource::<crate::terrain::Terrain>().filter(|t| t.for_planet == to).map_or(usize::MAX, |t| t.roots_built_here),
            rss_mb: crate::perf::rss_mb(),
        };
        println!(
            "swap {} {from} -> {to}{}: departed planet left {} terrain chunks, {} entities, {} of its {} meshes, generator freed {}; world {} terrain chunks, {} entities, {} meshes; root chunks built on the swap frame {}; RSS {}",
            w.resource::<SwapAudit>().rows.len() + 1,
            if drop { " (emergency drop)" } else { "" },
            row.old_chunks,
            row.old_entities,
            row.old_meshes,
            meshes.len(),
            row.old_freed,
            row.chunks,
            row.entities,
            row.meshes,
            row.roots_built_here,
            row.rss_mb.map_or("n/a".into(), |m| format!("{m:.0} MB")),
        );
        w.resource_mut::<SwapAudit>().rows.push(row);
    }
}

/// Without a window nobody moves the view: the terrain refines around the own ship.
pub fn headless_view(mut origin: ResMut<RenderOrigin>, ships: Query<&Position, With<Ship>>) {
    if let Ok(p) = ships.single() {
        origin.view = p.0;
    }
}

/// Share the counts after a later swap may differ from the first swap's (#14).
const SWAP_COUNT_TOLERANCE: f64 = 0.05;

pub(super) fn swap_steps(s: &mut Vec<Step>, dir: &std::path::Path, windowed: bool, rounds: usize) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    let name = |n: String| -> &'static str { Box::leak(n.into_boxed_str()) };
    let pname = |p: PlanetId| if p == HEARTH { "Hearth" } else { "Cinder" };
    let mut at = HEARTH;
    for i in 0..rounds {
        let to = if at == HEARTH { CINDER } else { HEARTH };
        s.push(warp_flight(name(format!("swap {}: warp {} -> {}", i + 1, pname(at), pname(to))), name(format!("swap{}", i + 1)), Some(at), to, Flight::Seated, dir.to_path_buf(), windowed));
        s.push(wait_drive_idle());
        at = to;
    }
    let other = if at == HEARTH { CINDER } else { HEARTH };
    // Late drop (past the middle): the nearest planet is the target, it becomes the simulation's.
    s.push(warp_flight(name(format!("swap {}: late emergency exit {} -> {}", rounds + 1, pname(at), pname(other))), "swap-late", Some(at), other, Flight::Emergency, dir.to_path_buf(), windowed));
    s.push(Box::new(move |w, c| {
        let now = w.resource::<PlanetRes>().id;
        check(c, now == other, format!("swap: after the late drop the simulation's planet is {now}, the nearest (the warp's target {other}), no longer {at}"));
        true
    }));
    s.push(wait_drive_idle());
    // Early drop (a fifth into the path): the nearest planet is the one left, it stays; the
    // target is kept generated for a jump on.
    s.push(Box::new(|w, c| {
        c.v.insert("swaps_before_early", tel(w).swaps.len() as f64);
        true
    }));
    s.push(warp_flight(name(format!("early emergency exit {} -> {}", pname(other), pname(at))), "early", Some(other), at, Flight::EarlyEmergency, dir.to_path_buf(), windowed));
    s.push(Box::new(move |w, c| {
        let now = w.resource::<PlanetRes>().id;
        let swaps = tel(w).swaps.len() as f64 - c.v["swaps_before_early"];
        let busy = w.resource::<PendingPlanet>().busy();
        check(c, now == other && swaps == 0.0 && busy, format!("swap: after the early drop the simulation's planet stays {now}, the nearest; {swaps} planet swaps; target kept generated for a jump on {busy}"));
        true
    }));
    s.push(wait_drive_idle());
    // On from the drop point: the kept target, swapped in when the ship enters its zone.
    s.push(warp_flight(name(format!("swap {}: warp on from the drop point to {}", rounds + 2, pname(at))), "swap-on", None, at, Flight::Seated, dir.to_path_buf(), windowed));
    s.push(wait(1.0));
    s.push(Box::new(move |w, c| {
        begin(w, c, "what the planet swaps left behind");
        let rows = w.resource::<SwapAudit>().rows.clone();
        let drops = rows.iter().filter(|r| r.drop).count();
        check(c, rows.len() == rounds + 2 && drops == 1, format!("swap: {} planet swaps, {drops} of them at an emergency drop (expected {} and 1)", rows.len(), rounds + 2));
        let Some(first) = rows.first().cloned() else { return true };
        let near = |a: f64, b: f64| (a - b).abs() <= SWAP_COUNT_TOLERANCE * b.max(1.0);
        for (i, r) in rows.iter().enumerate() {
            check(c, r.old_chunks == 0 && r.old_entities == 0 && r.old_meshes == 0 && r.old_freed,
                format!("swap {}: departed planet {} left {} terrain chunks, {} entities, {} meshes; generator freed {}", i + 1, r.from, r.old_chunks, r.old_entities, r.old_meshes, r.old_freed));
            check(c, near(r.chunks as f64, first.chunks as f64) && near(r.entities as f64, first.entities as f64) && near(r.meshes as f64, first.meshes as f64),
                format!("swap {}: world {} terrain chunks, {} entities, {} meshes; within {:.0} % of the first swap's {}, {}, {}", i + 1, r.chunks, r.entities, r.meshes, SWAP_COUNT_TOLERANCE * 100.0, first.chunks, first.entities, first.meshes));
            // #34: the roots come from the pool, built during the flight.
            check(c, r.roots_built_here == 0, format!("swap {}: {} root chunks built on the swap frame (prebuilt on the pool)", i + 1, r.roots_built_here));
        }
        let rss: Vec<String> = rows.iter().map(|r| r.rss_mb.map_or("n/a".into(), |m| format!("{m:.0}"))).collect();
        end(w, c, format!("RSS after each swap (MB, reported, not checked): {}", rss.join(", ")));
        true
    }));
}
