//! Drainage (#72): rivers and lakes on the macro grid, the mapgen4 approach on a sphere. Runs
//! once in the bake, never at runtime. The water is routed downhill over the ground with its
//! sinks filled (priority flood from the sea), the rain is summed along the routes, a few
//! erosion steps lower the ground along them (stream power), the water is routed again, deep
//! and large sinks stay as lakes and river beds are cut where enough rain gathers. The bake keeps
//! two macro channels from it: the change of the ground and the water surface.
use crate::math::*;
use crate::recipe::{DrainageSpec, Erosion, SinkCrossing};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// No neighbour: the sea, the outlet.
pub const NONE: u32 = u32::MAX;
/// Water surface where there is none. A bilinear blend that touches it drops below `DRY_BELOW`,
/// so a cell at the edge of a water area reads dry.
pub const NO_WATER: f32 = -1.0e6;
pub const DRY_BELOW: f64 = -1.0e3;

/// The graph the water runs on.
pub trait Mesh: Sync {
    /// Index range of the nodes.
    fn len(&self) -> usize;
    /// False for an index that is no node (a duplicate entry on a face edge).
    fn is_node(&self, _v: usize) -> bool {
        true
    }
    /// Neighbours of a node, into `out` (cleared first).
    fn neighbours(&self, v: usize, out: &mut Vec<usize>);
    /// Distance between two nodes along the ground (m).
    fn dist_m(&self, a: usize, b: usize) -> f64;
    /// Ground area a node stands for (m²).
    fn area_m2(&self, v: usize) -> f64;
}

pub struct Routing {
    /// Downstream neighbour of each node; NONE for the sea.
    pub rcv: Vec<u32>,
    /// Nodes in the order the flood reached them: a node always after its receiver.
    pub order: Vec<u32>,
    /// Water level with every sink filled to its spill point (the ground where nothing pools).
    pub level: Vec<f32>,
    /// Where the water ends: the sea and the outlets given to the flood.
    pub sink: Vec<bool>,
}

/// Where a river node's water goes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mouth {
    /// Index into the rivers.
    River(u32),
    /// Index into the lakes.
    Lake(u32),
    Sea,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RiverNode {
    pub node: u32,
    /// Rain-weighted catchment (m²).
    pub catchment_m2: f64,
    /// Bed and water surface (m above the base radius).
    pub bed_m: f32,
    pub level_m: f32,
    pub next: Mouth,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LakeOut {
    pub level_m: f32,
    pub area_m2: f64,
    pub depth_m: f32,
    pub deepest: u32,
    /// Where the water leaves: the first node past the lake (NONE without one).
    pub outlet: u32,
    pub nodes: u32,
}

pub struct Drained {
    /// Change of the ground per node (m): erosion and river beds (down), sediment in the sinks a
    /// river fills (up).
    pub carve: Vec<f32>,
    /// Water surface per node (m above the base radius), NO_WATER where none is defined.
    pub water: Vec<f32>,
    pub rivers: Vec<RiverNode>,
    pub lakes: Vec<LakeOut>,
    /// Lake index per node (NONE outside a lake).
    pub lake_of: Vec<u32>,
    /// The largest rain-weighted catchment of any land node (m²).
    pub max_catchment_m2: f64,
    /// The longest river from its source to its mouth (m), and the longest waterway that goes on
    /// through lakes that spill (each crossing counted straight to the outlet).
    pub longest_river_m: f64,
    pub longest_waterway_m: f64,
    /// Lowering by the erosion steps alone over land (m).
    pub erosion_max_m: f64,
    pub erosion_mean_m: f64,
    /// Time per step (ms).
    pub phases_ms: Vec<(&'static str, f64)>,
}

/// Orders f32 like `total_cmp` as an unsigned key.
#[inline(always)]
fn key(x: f32) -> u32 {
    let b = x.to_bits();
    if b & 0x8000_0000 != 0 { !b } else { b | 0x8000_0000 }
}

/// The sea for the water: the connected areas below `sea_level` at least `min_area_m2` large (the
/// largest one if none is). Smaller pockets below the sea level are sinks like any other: they
/// fill to a lake, or a river cuts through them.
pub fn ocean(mesh: &impl Mesh, h: &[f32], sea_level: f32, min_area_m2: f64) -> Vec<bool> {
    let n = mesh.len();
    let below = |v: usize| mesh.is_node(v) && h[v] < sea_level;
    let mut comp = vec![NONE; n];
    let mut areas: Vec<f64> = Vec::new();
    let (mut stack, mut nb) = (Vec::new(), Vec::with_capacity(16));
    for v0 in 0..n {
        if !below(v0) || comp[v0] != NONE {
            continue;
        }
        let id = areas.len() as u32;
        let mut area = 0.0;
        comp[v0] = id;
        stack.push(v0);
        while let Some(v) = stack.pop() {
            area += mesh.area_m2(v);
            mesh.neighbours(v, &mut nb);
            for &u in &nb {
                if below(u) && comp[u] == NONE {
                    comp[u] = id;
                    stack.push(u);
                }
            }
        }
        areas.push(area);
    }
    let largest = areas.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i as u32);
    let keep: Vec<bool> = areas.iter().enumerate().map(|(i, a)| *a >= min_area_m2 || Some(i as u32) == largest).collect();
    comp.iter().map(|&c| c != NONE && keep[c as usize]).collect()
}

/// Priority flood from the sea (the nodes marked in `sea`) and the `outlets` (node, water
/// level; lakes that never spill): each node is reached from its lowest neighbour and sinks fill
/// to their spill point; inside a sink first come first served (a plain queue, Barnes et al.
/// 2014), so the water takes the shortest way to the spill. Where nothing pools the water then
/// takes the steepest way down. Without a sea or outlet the lowest node is the outlet.
pub fn route(mesh: &impl Mesh, h: &[f32], ocean: &[bool], outlets: &[(u32, f32)]) -> Routing {
    let n = mesh.len();
    let mut rcv = vec![NONE; n];
    let mut level = h.to_vec();
    let mut sea = vec![false; n];
    let mut closed = vec![false; n];
    let mut order = Vec::with_capacity(n);
    // Heap keys: the level's ordered bits above a push counter (ties first come first served);
    // the counter indexes the node.
    let mut heap: BinaryHeap<Reverse<u64>> = BinaryHeap::new();
    let mut pushed: Vec<u32> = Vec::with_capacity(n);
    let mut pit = std::collections::VecDeque::new();
    let mut nb = Vec::with_capacity(16);
    let push = |heap: &mut BinaryHeap<Reverse<u64>>, pushed: &mut Vec<u32>, v: usize, l: f32| {
        heap.push(Reverse(((key(l) as u64) << 32) | pushed.len() as u64));
        pushed.push(v as u32);
    };
    for v in 0..n {
        if ocean[v] {
            sea[v] = true;
            closed[v] = true;
        }
    }
    for &(v, l) in outlets {
        sea[v as usize] = true;
        closed[v as usize] = true;
        level[v as usize] = l;
    }
    for v in 0..n {
        if sea[v] {
            mesh.neighbours(v, &mut nb);
            if nb.iter().any(|&u| !sea[u]) {
                push(&mut heap, &mut pushed, v, level[v]);
            }
        }
    }
    if heap.is_empty()
        && let Some(v) = (0..n).filter(|&v| mesh.is_node(v)).min_by(|&a, &b| h[a].total_cmp(&h[b]))
    {
        closed[v] = true;
        push(&mut heap, &mut pushed, v, h[v]);
    }
    let mut popped = vec![false; n];
    loop {
        let v = match pit.pop_front() {
            Some(v) => v,
            None => match heap.pop() {
                Some(Reverse(k)) => pushed[(k & 0xFFFF_FFFF) as usize] as usize,
                None => break,
            },
        };
        order.push(v as u32);
        popped[v] = true;
        mesh.neighbours(v, &mut nb);
        // Where nothing pools, the steepest way down to a node already reached (every lower
        // neighbour is), so `order` stays downstream first.
        let dry = !sea[v] && level[v] <= h[v];
        let mut best = (0.0, NONE);
        for &u in &nb {
            if !closed[u] {
                closed[u] = true;
                rcv[u] = v as u32;
                if h[u] <= level[v] {
                    level[u] = level[v];
                    pit.push_back(u);
                } else {
                    push(&mut heap, &mut pushed, u, h[u]);
                }
            } else if dry && popped[u] && level[u] < level[v] {
                let s = (level[v] - level[u]) as f64 / mesh.dist_m(v, u);
                if s > best.0 {
                    best = (s, u as u32);
                }
            }
        }
        if best.1 != NONE {
            rcv[v] = best.1;
        }
    }
    Routing { rcv, order, level, sink: sea }
}

/// Rain summed downstream.
pub fn accumulate(r: &Routing, rain: &[f32]) -> Vec<f32> {
    let mut flow = rain.to_vec();
    for &v in r.order.iter().rev() {
        let d = r.rcv[v as usize];
        if d != NONE {
            flow[d as usize] += flow[v as usize];
        }
    }
    flow
}

/// Erosion steps with the receivers held: a node sinks towards its receiver (implicit, so it
/// never cuts below it). Sinks and the sea stay; the ground stays above the sea.
pub fn erode(mesh: &impl Mesh, r: &Routing, flow: &[f32], h: &mut [f32], sea_level: f32, e: &Erosion) {
    if e.iterations == 0 {
        return;
    }
    // Per node in downstream order: itself, its receiver, the step's weight, its lowest ground.
    // Towards a sink the base is its water level, not its floor.
    let steps: Vec<(u32, u32, f32, f32)> = r
        .order
        .iter()
        .filter_map(|&v| {
            let (v, d) = (v as usize, r.rcv[v as usize]);
            if d == NONE || r.sink[v] || r.level[v] > h[v] {
                return None;
            }
            let a = flow[v] as f64;
            let f = e.strength * if e.area_exponent == 0.5 { a.sqrt() } else { a.powf(e.area_exponent) } / mesh.dist_m(v, d as usize);
            Some((v as u32, d, f as f32, (h[v] as f64 - e.max_m).max(sea_level as f64).min(h[v] as f64) as f32))
        })
        .collect();
    for _ in 0..e.iterations {
        for &(v, d, f, floor) in &steps {
            let (v, d) = (v as usize, d as usize);
            let base = if r.level[d] > h[d] { r.level[d] } else { h[d] };
            h[v] = ((h[v] + f * base) / (1.0 + f)).max(floor);
        }
    }
}

/// Connected nodes under one filled level: where water pools.
struct Sink {
    level: f32,
    members: Vec<usize>,
    deepest: usize,
    area: f64,
    /// Rain-weighted catchment flowing in (m²): what leaves it towards the spill.
    inflow: f64,
}

/// Every sink of a routing, in node order.
fn sinks(mesh: &impl Mesh, r: &Routing, h: &[f32], flow: &[f32]) -> Vec<Sink> {
    let n = mesh.len();
    let flooded = |v: usize| !r.sink[v] && r.level[v] > h[v];
    let mut comp = vec![NONE; n];
    let mut out = Vec::new();
    let (mut stack, mut nb) = (Vec::new(), Vec::with_capacity(16));
    for v0 in 0..n {
        if !mesh.is_node(v0) || comp[v0] != NONE || !flooded(v0) {
            continue;
        }
        let (id, level) = (out.len() as u32, r.level[v0]);
        let mut members = Vec::new();
        stack.push(v0);
        comp[v0] = id;
        while let Some(v) = stack.pop() {
            members.push(v);
            mesh.neighbours(v, &mut nb);
            for &u in &nb {
                if comp[u] == NONE && flooded(u) && r.level[u] == level {
                    comp[u] = id;
                    stack.push(u);
                }
            }
        }
        let area = members.iter().map(|&v| mesh.area_m2(v)).sum();
        let deepest = *members.iter().min_by(|&&a, &&b| h[a].total_cmp(&h[b]).then(a.cmp(&b))).unwrap();
        let inflow = members.iter().filter(|&&v| r.rcv[v] == NONE || comp[r.rcv[v] as usize] != id).map(|&v| flow[v] as f64).sum();
        out.push(Sink { level, members, deepest, area, inflow });
    }
    out
}

/// Sediment fills a sink at least this far above the sea level (m): the ground between the
/// vertices wanders by about this much, and a flat right at the sea level freckled with sea.
const FILL_ABOVE_SEA_M: f32 = 2.0;

/// Hops a river's cross-section reaches at most.
const SECTION_HOPS: u32 = 4;

/// Everything: erosion, routing, lakes, river beds and the water surface.
pub fn drain(mesh: &impl Mesh, h0: &[f32], rain: &[f32], sea_level: f32, s: &DrainageSpec) -> Drained {
    let n = mesh.len();
    let mut phases_ms = Vec::new();
    let mut t = std::time::Instant::now();
    let mut lap = |name: &'static str| {
        phases_ms.push((name, t.elapsed().as_secs_f64() * 1e3));
        t = std::time::Instant::now();
    };
    let mut h = h0.to_vec();
    let sea = ocean(mesh, h0, sea_level, s.sea_min_area_km2 * 1e6);
    let first = route(mesh, &h, &sea, &[]);
    lap("route");
    let flow = accumulate(&first, rain);
    erode(mesh, &first, &flow, &mut h, sea_level, &s.erosion);
    drop(first);
    lap("erode");
    let (mut erosion_max_m, mut sum, mut land) = (0.0f64, 0.0f64, 0usize);
    for v in (0..n).filter(|&v| mesh.is_node(v) && h0[v] >= sea_level) {
        let l = (h0[v] - h[v]) as f64;
        erosion_max_m = erosion_max_m.max(l);
        sum += l;
        land += 1;
    }
    let r = route(mesh, &h, &sea, &[]);
    let flow = accumulate(&r, rain);
    lap("route again");
    let amin = s.river_min_catchment_km2 * 1e6;

    // Sinks that gather too little rain for their surface (evaporation) hold a smaller lake that
    // never spills, or stay dry; they become outlets of a second flood, so the water around them
    // runs into them instead of towards their spill point.
    let mut lake_of = vec![NONE; n];
    let mut lakes = Vec::new();
    let mut outlets: Vec<(u32, f32)> = Vec::new();
    for c in sinks(mesh, &r, &h, &flow) {
        let budget = if s.lake_evaporation > 0.0 { c.inflow / s.lake_evaporation } else { f64::INFINITY };
        if ((c.level - h[c.deepest]) as f64) < s.lake_min_depth_m || c.area <= budget {
            continue;
        }
        let mut low = c.members.clone();
        low.sort_by(|&a, &b| h[a].total_cmp(&h[b]).then(a.cmp(&b)));
        let (mut k, mut area) = (0, 0.0);
        while k < low.len() && area + mesh.area_m2(low[k]) <= budget {
            area += mesh.area_m2(low[k]);
            k += 1;
        }
        let river_in = c.inflow >= amin;
        if k == 0 && river_in {
            area = mesh.area_m2(low[0]);
            k = 1;
        }
        let level = if k < low.len() { h[low[k]] } else { c.level };
        let depth = level - h[low[0]];
        if depth > 0.0 && (river_in || (area >= s.lake_min_area_m2 && depth as f64 >= s.lake_min_depth_m)) {
            let id = lakes.len() as u32;
            for &v in &low[..k] {
                lake_of[v] = id;
                outlets.push((v as u32, level));
            }
            lakes.push(LakeOut { level_m: level, area_m2: area, depth_m: depth, deepest: low[0] as u32, outlet: NONE, nodes: k as u32 });
        } else {
            outlets.push((low[0] as u32, h[low[0]]));
        }
    }
    let (r, flow) = if outlets.is_empty() {
        (r, flow)
    } else {
        let r = route(mesh, &h, &sea, &outlets);
        let flow = accumulate(&r, rain);
        (r, flow)
    };

    // Sinks that fill to their spill point: a lake when deep enough and fed by a river (a river
    // never cuts through a sink deeper than the lake depth), or large enough and wet enough. A
    // river crosses the others by cutting their sill, or across sediment up to the spill.
    for c in sinks(mesh, &r, &h, &flow) {
        let depth = c.level - h[c.deepest];
        let budget = if s.lake_evaporation > 0.0 { c.inflow / s.lake_evaporation } else { f64::INFINITY };
        if (depth as f64) < s.lake_min_depth_m || (c.inflow < amin && (c.area < s.lake_min_area_m2 || c.area > budget)) {
            if s.river_sinks == SinkCrossing::Fill && c.inflow >= amin {
                let top = c.level.max(sea_level + FILL_ABOVE_SEA_M);
                for &v in &c.members {
                    h[v] = top;
                }
            }
            continue;
        }
        let id = lakes.len() as u32;
        for &v in &c.members {
            lake_of[v] = id;
        }
        let mut o = c.deepest;
        while lake_of[o] == id && r.rcv[o] != NONE {
            o = r.rcv[o] as usize;
        }
        let outlet = if lake_of[o] == id { NONE } else { o as u32 };
        lakes.push(LakeOut { level_m: c.level, area_m2: c.area, depth_m: depth, deepest: c.deepest as u32, outlet, nodes: c.members.len() as u32 });
    }
    let mut nb = Vec::with_capacity(16);

    lap("lakes");
    // Rivers where enough rain gathers (not in a lake). The bed sinks with the catchment, and
    // neither bed nor water rises downstream: a river cuts through a sill instead.
    let mut river_of = vec![NONE; n];
    let mut rivers = Vec::new();
    let (mut depth, mut half) = (Vec::new(), Vec::new());
    for v in 0..n {
        if mesh.is_node(v) && !r.sink[v] && lake_of[v] == NONE && flow[v] as f64 >= amin {
            river_of[v] = rivers.len() as u32;
            let k = flow[v] as f64 / amin;
            let d = (s.river_depth_m[0] * k.powf(0.4)).min(s.river_depth_m[1]) as f32;
            depth.push(d);
            half.push(((s.river_width_m[0] * k.sqrt()).min(s.river_width_m[1]) * 0.5).max(1e-3) as f32);
            rivers.push(RiverNode { node: v as u32, catchment_m2: flow[v] as f64, bed_m: h[v] - d, level_m: 0.0, next: Mouth::Sea });
        }
    }
    let downstream = |v: u32| {
        let d = r.rcv[v as usize];
        if d == NONE { NONE } else { river_of[d as usize] }
    };
    for &v in r.order.iter().rev() {
        let (i, j) = (river_of[v as usize], downstream(v));
        if i != NONE && j != NONE {
            rivers[j as usize].bed_m = rivers[j as usize].bed_m.min(rivers[i as usize].bed_m);
        }
    }
    for (rv, d) in rivers.iter_mut().zip(&depth) {
        rv.level_m = rv.bed_m + d * s.river_fill as f32;
    }
    for &v in r.order.iter().rev() {
        let (i, j) = (river_of[v as usize], downstream(v));
        if i != NONE && j != NONE {
            rivers[j as usize].level_m = rivers[j as usize].level_m.min(rivers[i as usize].level_m);
        }
    }
    for rv in rivers.iter_mut() {
        let d = r.rcv[rv.node as usize];
        rv.next = if d != NONE && lake_of[d as usize] != NONE {
            Mouth::Lake(lake_of[d as usize])
        } else if d == NONE || r.sink[d as usize] {
            Mouth::Sea
        } else {
            debug_assert!(river_of[d as usize] != NONE, "more water downstream is a river too");
            Mouth::River(river_of[d as usize])
        };
    }

    lap("rivers");
    // Cross-sections: a parabola from the bed, cut where it lies below the ground. The water
    // surface reaches the nodes within the half width and the first ring (the nearest river
    // wins), so a bilinear blend finds the bank between them.
    let mut ground = h.clone();
    let mut water = vec![NO_WATER; n];
    let mut near = vec![f32::INFINITY; n];
    // The water a node got lies within a river's half width (else it only marks the bank).
    let mut inside = vec![false; n];
    let mut mark = vec![NONE; n];
    let mut ring: Vec<(usize, u32)> = Vec::new();
    for (i, rv) in rivers.iter().enumerate() {
        let v = rv.node as usize;
        let (bed, d, hw) = (rv.bed_m, depth[i], half[i]);
        ring.clear();
        ring.push((v, 0));
        mark[v] = i as u32;
        let mut k = 0;
        while k < ring.len() {
            let (u, hops) = ring[k];
            k += 1;
            let dist = mesh.dist_m(v, u) as f32;
            let cut = if u == v { bed } else { bed + d * (dist / hw).powi(2) };
            if (u == v || river_of[u] == NONE) && cut < ground[u] {
                ground[u] = cut;
            }
            if (hops <= 1 || dist <= hw) && dist < near[u] {
                near[u] = dist;
                water[u] = rv.level_m;
                inside[u] = dist <= hw;
            }
            if hops == 0 || (cut < h[u] && hops < SECTION_HOPS) {
                mesh.neighbours(u, &mut nb);
                for &x in &nb {
                    if mark[x] != i as u32 {
                        mark[x] = i as u32;
                        ring.push((x, hops + 1));
                    }
                }
            }
        }
    }
    // A diagonal step crosses a cell whose other two corners keep the bank's height, and its
    // middle (their mean) would stand above the water: lower them to just above the water.
    let mut theirs = Vec::with_capacity(16);
    for (i, rv) in rivers.iter().enumerate() {
        let (v, d) = (rv.node as usize, r.rcv[rv.node as usize]);
        if d == NONE || (r.sink[d as usize] && lake_of[d as usize] == NONE) {
            continue;
        }
        let d = d as usize;
        let below = if lake_of[d] != NONE { lakes[lake_of[d] as usize].level_m } else { rivers[river_of[d] as usize].level_m };
        mesh.neighbours(d, &mut theirs);
        mesh.neighbours(v, &mut nb);
        nb.retain(|u| theirs.contains(u));
        // Two shared neighbours: a diagonal (a step along an edge shares four).
        if nb.len() == 2 {
            let top = rv.level_m.min(below) + 0.5 * depth[i] * s.river_fill as f32;
            for &c in &nb {
                if river_of[c] == NONE && lake_of[c] == NONE && top < ground[c] {
                    ground[c] = top;
                }
            }
        }
    }
    // Lakes: flat at their level, one ring beyond for the shore.
    for v in 0..n {
        let id = lake_of[v];
        if id == NONE {
            continue;
        }
        let lv = lakes[id as usize].level_m;
        water[v] = lv;
        mesh.neighbours(v, &mut nb);
        for &u in &nb {
            if lake_of[u] == NONE {
                water[u] = water[u].max(lv);
            }
        }
    }
    // A bank node only marks where the shore is: its water never stands above its own ground, or
    // a sheet would end in the air (a river mouth above the sea, a lower node beside a river).
    for u in 0..n {
        if water[u] > ground[u] && !inside[u] && river_of[u] == NONE && lake_of[u] == NONE {
            water[u] = ground[u];
        }
    }
    let carve = ground.iter().zip(h0).map(|(g, h)| g - h).collect();
    lap("sections");
    let max_catchment_m2 = (0..n).filter(|&v| mesh.is_node(v) && !r.sink[v]).map(|v| flow[v] as f64).fold(0.0, f64::max);
    // Length to the mouth per river node, downstream first (a lake's outlet is reached before
    // the rivers running into the lake).
    let (mut run, mut way) = (vec![0.0f64; rivers.len()], vec![0.0f64; rivers.len()]);
    for &v in &r.order {
        let i = river_of[v as usize];
        if i == NONE {
            continue;
        }
        let (i, v) = (i as usize, v as usize);
        match rivers[i].next {
            Mouth::River(j) => {
                let d = mesh.dist_m(v, rivers[j as usize].node as usize);
                run[i] = d + run[j as usize];
                way[i] = d + way[j as usize];
            }
            Mouth::Lake(id) => {
                let o = lakes[id as usize].outlet;
                if o != NONE && river_of[o as usize] != NONE {
                    way[i] = mesh.dist_m(v, o as usize) + way[river_of[o as usize] as usize];
                }
            }
            Mouth::Sea => {}
        }
    }
    let longest_river_m = run.iter().copied().fold(0.0, f64::max);
    let longest_waterway_m = way.iter().copied().fold(0.0, f64::max);
    Drained { carve, water, rivers, lakes, lake_of, max_catchment_m2, longest_river_m, longest_waterway_m, erosion_max_m, erosion_mean_m: if land > 0 { sum / land as f64 } else { 0.0 }, phases_ms }
}

/// The macro grid as a graph: every vertex of the six face grids (`(face * w + j) * w + i`, as
/// the macro image), the duplicates on shared face edges joined into one node, eight neighbours
/// (six at a cube corner).
pub struct MacroGrid {
    n: usize,
    w: usize,
    radius: f64,
    canon: Vec<u32>,
    /// Edge nodes: their entries on every face.
    aliases: HashMap<u32, Vec<u32>>,
    dirs: Vec<[f32; 3]>,
    area: Vec<f32>,
}

impl MacroGrid {
    pub fn new(n: usize, radius: f64, threads: usize) -> MacroGrid {
        let w = n + 1;
        let total = 6 * w * w;
        let rows = crate::planet::par_rows(6 * w, threads, |r0, r1| {
            let mut out = Vec::with_capacity((r1 - r0) * w);
            for row in r0..r1 {
                let (face, j) = (row / w, row % w);
                let b = -1.0 + j as f64 * 2.0 / n as f64;
                for i in 0..w {
                    let d = cube_to_sphere(face, -1.0 + i as f64 * 2.0 / n as f64, b);
                    out.push([d.x as f32, d.y as f32, d.z as f32]);
                }
            }
            out
        });
        let dirs: Vec<[f32; 3]> = rows.into_iter().flatten().collect();
        // Edge entries meet on the cube at integer points (cube point * n), exact.
        let mut canon: Vec<u32> = (0..total as u32).collect();
        let mut aliases: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut at: HashMap<[i64; 3], u32> = HashMap::new();
        let ints = |x: V3| [x.x.round() as i64, x.y.round() as i64, x.z.round() as i64];
        for face in 0..6 {
            let nrm = FACE_NORMALS[face];
            let u = v3(nrm.y, nrm.z, nrm.x);
            let (ni, ui, vi) = (ints(nrm), ints(u), ints(nrm.cross(u)));
            for j in 0..w {
                for i in 0..w {
                    if i != 0 && i != n && j != 0 && j != n {
                        continue;
                    }
                    let k = ((face * w + j) * w + i) as u32;
                    let (a, b) = (2 * i as i64 - n as i64, 2 * j as i64 - n as i64);
                    let p = [0, 1, 2].map(|c| ni[c] * n as i64 + ui[c] * a + vi[c] * b);
                    let c = *at.entry(p).or_insert(k);
                    canon[k as usize] = c;
                    aliases.entry(c).or_default().push(k);
                }
            }
        }
        // Area of each vertex's cell, from the neighbouring directions; an edge vertex has half
        // a cell on each face, a corner a quarter on three.
        let mut area = vec![0.0f32; total];
        for face in 0..6 {
            for j in 0..w {
                for i in 0..w {
                    let d = |i: usize, j: usize| {
                        let x = dirs[(face * w + j) * w + i];
                        v3(x[0] as f64, x[1] as f64, x[2] as f64)
                    };
                    let (i0, i1, j0, j1) = (i.saturating_sub(1), (i + 1).min(n), j.saturating_sub(1), (j + 1).min(n));
                    let da = (d(i1, j) - d(i0, j)) * (1.0 / (i1 - i0) as f64);
                    let db = (d(i, j1) - d(i, j0)) * (1.0 / (j1 - j0) as f64);
                    let share = if i == 0 || i == n { 0.5 } else { 1.0 } * if j == 0 || j == n { 0.5 } else { 1.0 };
                    let k = (face * w + j) * w + i;
                    area[canon[k] as usize] += (da.cross(db).length() * radius * radius * share) as f32;
                }
            }
        }
        MacroGrid { n, w, radius, canon, aliases, dirs, area }
    }

    #[inline(always)]
    pub fn dir(&self, v: usize) -> V3 {
        let d = self.dirs[v];
        v3(d[0] as f64, d[1] as f64, d[2] as f64)
    }

    /// The node an entry belongs to.
    pub fn canon(&self, k: usize) -> usize {
        self.canon[k] as usize
    }

    fn in_face(&self, k: usize, out: &mut Vec<usize>) {
        let w = self.w;
        let (face, j, i) = (k / (w * w), (k / w) % w, k % w);
        for jj in j.saturating_sub(1)..=(j + 1).min(self.n) {
            for ii in i.saturating_sub(1)..=(i + 1).min(self.n) {
                if (ii, jj) != (i, j) {
                    out.push(self.canon[(face * w + jj) * w + ii] as usize);
                }
            }
        }
    }
}

impl Mesh for MacroGrid {
    fn len(&self) -> usize {
        self.canon.len()
    }
    #[inline(always)]
    fn is_node(&self, v: usize) -> bool {
        self.canon[v] as usize == v
    }
    #[inline(always)]
    fn neighbours(&self, v: usize, out: &mut Vec<usize>) {
        out.clear();
        let w = self.w;
        let (i, j) = (v % w, (v / w) % w);
        // Two away from the face edges: the plain grid (most nodes, the bake's hot path).
        if i >= 2 && j >= 2 && i + 2 <= self.n && j + 2 <= self.n {
            out.extend_from_slice(&[v - w - 1, v - w, v - w + 1, v - 1, v + 1, v + w - 1, v + w, v + w + 1]);
            return;
        }
        if i != 0 && i != self.n && j != 0 && j != self.n {
            self.in_face(v, out);
            return;
        }
        if let Some(al) = self.aliases.get(&(v as u32)) {
            for &a in al {
                self.in_face(a as usize, out);
            }
            let mut kept: Vec<usize> = Vec::with_capacity(out.len());
            for &u in out.iter() {
                if u != v && !kept.contains(&u) {
                    kept.push(u);
                }
            }
            *out = kept;
        }
    }
    #[inline(always)]
    fn dist_m(&self, a: usize, b: usize) -> f64 {
        let (p, q) = (self.dirs[a], self.dirs[b]);
        let (x, y, z) = (p[0] - q[0], p[1] - q[1], p[2] - q[2]);
        ((x * x + y * y + z * z) as f64).sqrt() * self.radius
    }
    fn area_m2(&self, v: usize) -> f64 {
        self.area[v] as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat grid with 8 neighbours, `cell` metres apart.
    struct Grid {
        w: usize,
        h: usize,
        cell: f64,
    }
    impl Mesh for Grid {
        fn len(&self) -> usize {
            self.w * self.h
        }
        fn neighbours(&self, v: usize, out: &mut Vec<usize>) {
            out.clear();
            let (x, y) = ((v % self.w) as isize, (v / self.w) as isize);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (xx, yy) = (x + dx, y + dy);
                    if (dx, dy) != (0, 0) && xx >= 0 && yy >= 0 && xx < self.w as isize && yy < self.h as isize {
                        out.push(yy as usize * self.w + xx as usize);
                    }
                }
            }
        }
        fn dist_m(&self, a: usize, b: usize) -> f64 {
            let (ax, ay) = ((a % self.w) as f64, (a / self.w) as f64);
            let (bx, by) = ((b % self.w) as f64, (b / self.w) as f64);
            (ax - bx).hypot(ay - by) * self.cell
        }
        fn area_m2(&self, _v: usize) -> f64 {
            self.cell * self.cell
        }
    }

    fn spec() -> DrainageSpec {
        DrainageSpec {
            comment: String::new(),
            rain_base: 1.0,
            rain_moisture_gain: 0.0,
            river_min_catchment_km2: 0.05,
            river_depth_m: [1.5, 4.0],
            river_width_m: [10.0, 40.0],
            river_fill: 0.7,
            lake_min_depth_m: 2.0,
            lake_min_area_m2: 2000.0,
            lake_evaporation: 0.0,
            sea_min_area_km2: 0.0,
            river_sinks: SinkCrossing::Cut,
            erosion: Erosion { iterations: 4, strength: 0.01, area_exponent: 0.5, max_m: 10.0 },
        }
    }

    const W: usize = 80;
    const CELL: f64 = 15.0;

    /// A slope down to the sea along x (sea below x = 8), a gentle trough along the middle so the
    /// water gathers, plus a bowl `bowl_m` deep at (50, 40).
    fn terrain(bowl_m: f32) -> Vec<f32> {
        (0..W * W)
            .map(|v| {
                let (x, y) = ((v % W) as f32, (v / W) as f32);
                let slope = (x - 8.0) * 0.6;
                let trough = ((y - 40.0) / 20.0).powi(2) * 10.0;
                let wobble = (x * 0.7).sin() * (y * 0.9).cos() * 0.3;
                let r2 = ((x - 50.0).powi(2) + (y - 40.0).powi(2)) / 36.0;
                slope + trough + wobble - bowl_m * (-r2).exp()
            })
            .collect()
    }

    fn run(h: &[f32]) -> Drained {
        let g = Grid { w: W, h: W, cell: CELL };
        drain(&g, h, &vec![(CELL * CELL) as f32; W * W], 0.0, &spec())
    }

    fn ground(d: &Drained, h: &[f32], v: usize) -> f32 {
        h[v] + d.carve[v]
    }

    #[test]
    fn water_flows_downhill() {
        let h = terrain(0.0);
        let d = run(&h);
        assert!(!d.rivers.is_empty(), "the trough gathers a river");
        for r in &d.rivers {
            if let Mouth::River(n) = r.next {
                let n = &d.rivers[n as usize];
                assert!(n.bed_m <= r.bed_m, "bed rises downstream: {} -> {}", r.bed_m, n.bed_m);
                assert!(n.level_m <= r.level_m, "water rises downstream: {} -> {}", r.level_m, n.level_m);
                assert!(ground(&d, &h, n.node as usize) <= ground(&d, &h, r.node as usize) + 1e-4);
            }
            assert!(r.level_m >= r.bed_m && r.level_m <= h[r.node as usize], "water within the banks");
            assert!(d.water[r.node as usize] > ground(&d, &h, r.node as usize), "the river node is wet");
        }
        assert!(d.carve.iter().all(|c| *c <= 0.0), "cutting through sinks never raises the ground");
        let g = Grid { w: W, h: W, cell: CELL };
        let r = route(&g, &h, &ocean(&g, &h, 0.0, 0.0), &[]);
        for &v in &r.order {
            let rc = r.rcv[v as usize];
            if rc != NONE {
                assert!(r.level[rc as usize] <= r.level[v as usize], "filled level never rises downstream");
            }
        }
    }

    #[test]
    fn lakes_sit_in_sinks() {
        let h = terrain(12.0);
        let d = run(&h);
        assert_eq!(d.lakes.len(), 1, "one lake in the bowl: {:?}", d.lakes);
        let lake = &d.lakes[0];
        let g = Grid { w: W, h: W, cell: CELL };
        let (x, y) = (lake.deepest as usize % W, lake.deepest as usize / W);
        assert!(x.abs_diff(50) <= 3 && y.abs_diff(40) <= 3, "the deepest node is in the bowl: ({x}, {y})");
        // On the slope the bowl spills over its low side: shallower than the bowl itself.
        assert!(lake.depth_m > 3.0 && lake.depth_m <= 12.5, "depth {}", lake.depth_m);
        // Every lake node lies under the level; around it no water stands above the ground, except
        // in the rivers' channels (a river node and its first ring).
        let mut channel: Vec<usize> = Vec::new();
        let mut nb = Vec::new();
        for r in &d.rivers {
            g.neighbours(r.node as usize, &mut nb);
            channel.push(r.node as usize);
            channel.extend(&nb);
        }
        let members: Vec<usize> = (0..W * W).filter(|&v| d.lake_of[v] == 0).collect();
        assert_eq!(members.len(), lake.nodes as usize);
        for &v in &members {
            assert!(ground(&d, &h, v) < lake.level_m && d.water[v] == lake.level_m, "lake node {v} is wet at the level");
            g.neighbours(v, &mut nb);
            for &u in &nb {
                let held = d.lake_of[u] == 0 || channel.contains(&u) || ground(&d, &h, u) >= lake.level_m - 1e-4 || d.water[u] <= ground(&d, &h, u) + 1e-4;
                assert!(held, "water leaks at {u}: ground {} level {} water {} lake node {v}", ground(&d, &h, u), lake.level_m, d.water[u]);
            }
        }
        assert!(lake.outlet != NONE && d.lake_of[lake.outlet as usize] == NONE, "the lake spills somewhere");
    }

    #[test]
    fn a_dry_climate_keeps_a_smaller_lake_that_never_spills() {
        let h = terrain(12.0);
        let g = Grid { w: W, h: W, cell: CELL };
        let wet = run(&h);
        let dry = drain(&g, &h, &vec![(CELL * CELL) as f32; W * W], 0.0, &DrainageSpec { lake_evaporation: 60.0, ..spec() });
        assert_eq!(dry.lakes.len(), 1, "{:?}", dry.lakes);
        let (w, d) = (&wet.lakes[0], &dry.lakes[0]);
        assert!(d.level_m < w.level_m && d.area_m2 < w.area_m2, "smaller and lower: {d:?} vs {w:?}");
        assert_eq!(d.outlet, NONE, "it never spills");
        // The river from upstream ends in it, and below the bowl no water from above arrives.
        assert!(dry.rivers.iter().any(|r| r.next == Mouth::Lake(0)), "a river runs into the dry lake");
        let below = |dd: &Drained| dd.rivers.iter().filter(|r| (r.node as usize % W) < 40).map(|r| r.catchment_m2).fold(0.0, f64::max);
        assert!(below(&dry) < below(&wet) * 0.8, "less water below the bowl: {} vs {}", below(&dry), below(&wet));
        // Around the lake the ground holds the water in, except where a river comes in.
        let rivers: Vec<usize> = dry.rivers.iter().map(|r| r.node as usize).collect();
        let mut nb = Vec::new();
        for v in (0..W * W).filter(|&v| dry.lake_of[v] == 0) {
            g.neighbours(v, &mut nb);
            for &u in &nb {
                let held = dry.lake_of[u] == 0 || ground(&dry, &h, u) >= d.level_m - 1e-4 || rivers.contains(&u);
                assert!(held, "water leaks at {u}: ground {} level {} water {}", ground(&dry, &h, u), d.level_m, dry.water[u]);
            }
        }
    }

    #[test]
    fn rivers_end_in_the_sea_or_a_lake() {
        let h = terrain(12.0);
        let d = run(&h);
        let (mut sea, mut lake) = (0, 0);
        for start in 0..d.rivers.len() {
            let mut at = start;
            let mut steps = 0;
            loop {
                match d.rivers[at].next {
                    Mouth::River(n) => at = n as usize,
                    Mouth::Lake(_) => {
                        lake += 1;
                        break;
                    }
                    Mouth::Sea => {
                        sea += 1;
                        break;
                    }
                }
                steps += 1;
                assert!(steps <= d.rivers.len(), "river {start} runs in a circle");
            }
        }
        assert!(sea > 0 && lake > 0, "rivers reach the sea ({sea}) and the lake ({lake})");
    }

    #[test]
    fn a_shallow_sink_is_cut_through() {
        let h = terrain(1.0);
        let d = run(&h);
        assert!(d.lakes.is_empty(), "a 1 m sink is below the lake depth: {:?}", d.lakes);
        // A river crosses the sink and runs on to the sea, its bed never rising.
        let mut at = d
            .rivers
            .iter()
            .position(|r| ((r.node as usize % W) as isize - 50).abs() <= 2 && ((r.node as usize / W) as isize - 40).abs() <= 4)
            .expect("a river crosses the sink");
        loop {
            match d.rivers[at].next {
                Mouth::River(n) => {
                    assert!(d.rivers[n as usize].bed_m <= d.rivers[at].bed_m);
                    at = n as usize;
                }
                Mouth::Sea => break,
                Mouth::Lake(_) => panic!("no lake here"),
            }
        }
    }

    #[test]
    fn a_sink_on_a_river_fills_with_sediment_instead() {
        let h = terrain(8.0);
        let g = Grid { w: W, h: W, cell: CELL };
        let cut = run(&h);
        let fill = drain(&g, &h, &vec![(CELL * CELL) as f32; W * W], 0.0, &DrainageSpec { lake_min_depth_m: 20.0, river_sinks: SinkCrossing::Fill, ..spec() });
        assert!(cut.lakes.len() == 1 && fill.lakes.is_empty(), "the sink: a lake at 2 m lake depth, filled at 20 m: {:?} {:?}", cut.lakes, fill.lakes);
        // The floor rose; nothing on the way out was cut deeper than a bed.
        let floor = (0..W * W).filter(|&v| ((v % W) as isize - 50).abs() <= 1 && ((v / W) as isize - 40).abs() <= 1).map(|v| fill.carve[v]).fold(f32::MIN, f32::max);
        assert!(floor > 0.5, "the sink's floor rose by {floor} m");
        let deepest_cut = fill.carve.iter().copied().fold(0.0f32, f32::min);
        assert!(-deepest_cut <= spec().river_depth_m[1] as f32 + spec().erosion.max_m as f32 + 1e-3, "cut {deepest_cut} m");
        // One river runs across it to the sea.
        assert!(fill.rivers.iter().all(|r| r.next != Mouth::Lake(0)));
    }

    #[test]
    fn erosion_only_lowers_and_follows_the_water() {
        let h = terrain(0.0);
        let g = Grid { w: W, h: W, cell: CELL };
        let r = route(&g, &h, &ocean(&g, &h, 0.0, 0.0), &[]);
        let flow = accumulate(&r, &vec![(CELL * CELL) as f32; W * W]);
        let mut e = h.clone();
        erode(&g, &r, &flow, &mut e, 0.0, &spec().erosion);
        let low = |v: usize| h[v] - e[v];
        assert!((0..W * W).all(|v| low(v) >= 0.0 && low(v) <= 10.0 + 1e-4));
        // More water, more lowering: the trough beats the flanks at the same distance from the sea.
        assert!(low(40 * W + 60) > low(10 * W + 60), "trough {} flank {}", low(40 * W + 60), low(10 * W + 60));
        assert!((0..W * W).all(|v| h[v] >= 0.0 || e[v] == h[v]), "the sea stays");
    }

    #[test]
    fn macro_grid_joins_face_edges() {
        let n = 16;
        let g = MacroGrid::new(n, 1000.0, 2);
        let w = n + 1;
        let nodes = (0..g.len()).filter(|&v| g.is_node(v)).count();
        assert_eq!(nodes, 6 * n * n + 2, "a cube sphere grid has 6n²+2 vertices");
        let mut nb = Vec::new();
        let mut total_area = 0.0;
        for v in (0..g.len()).filter(|&v| g.is_node(v)) {
            g.neighbours(v, &mut nb);
            assert!(nb.len() == 8 || nb.len() == 6, "node {v} has {} neighbours", nb.len());
            for &u in &nb {
                assert!(g.is_node(u));
                assert!(g.dist_m(v, u) < 2.5 * std::f64::consts::FRAC_PI_2 * 1000.0 / n as f64);
            }
            total_area += g.area_m2(v);
        }
        let sphere = 4.0 * std::f64::consts::PI * 1e6;
        assert!((total_area / sphere - 1.0).abs() < 0.02, "areas sum to the sphere: {}", total_area / sphere);
        // Every entry belongs to a node at the same spot.
        assert!((0..6 * w * w).all(|k| g.dist_m(k, g.canon(k)) < 1e-3));
    }
}
