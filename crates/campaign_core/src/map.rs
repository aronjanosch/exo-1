use crate::names::Names;
use crate::rng::Rng;
use crate::template::SystemKind;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const KEY_COLUMNS: u64 = 0x4D41_5043_4F4C;
const KEY_EDGES: u64 = 0x4D41_5045_4447;
const KEY_KINDS: u64 = 0x4D41_504B_494E;
const KEY_DANGER: u64 = 0x4D41_5044_414E;
const KEY_SYSTEM: u64 = 0x4D41_5053_5953;
const KIND_ATTEMPTS: u64 = 200;
const NAME_TRIES: u32 = 100;

/// Index of a system in `Map::nodes`. Ordered by column, then row.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[serde(transparent)]
pub struct NodeId(pub u8);

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: NodeId,
    pub column: u8,
    pub kind: SystemKind,
    pub danger: u8,
    /// The system's seed for the star system generator.
    pub seed: u64,
    pub name: String,
    pub forward: Vec<NodeId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Map {
    pub seed: u64,
    pub nodes: Vec<Node>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct MapRules {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    pub columns: Vec<[u32; 2]>,
    pub max_nodes: u32,
    pub forward: [u32; 2],
    pub weights: KindWeights,
    pub danger: KindDanger,
    pub max_harbours: u32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KindWeights {
    pub harbour: f64,
    pub normal: f64,
    pub hostile: f64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KindDanger {
    pub harbour: [u8; 2],
    pub normal: [u8; 2],
    pub hostile: [u8; 2],
}

impl MapRules {
    pub fn from_json(json: &str) -> Result<MapRules, String> {
        let rules: MapRules = serde_json::from_str(json).map_err(|e| e.to_string())?;
        rules.validate()?;
        Ok(rules)
    }

    fn validate(&self) -> Result<(), String> {
        if self.columns.len() < 2 {
            return Err("columns: need at least 2".into());
        }
        if self.columns[0] != [1, 1] {
            return Err("columns[0]: must be [1, 1]".into());
        }
        let [f_lo, f_hi] = self.forward;
        if f_lo < 1 || f_lo > f_hi {
            return Err(format!("forward: {f_lo}..{f_hi} needs 1 or more, ordered"));
        }
        let mut minimum = 0u32;
        for (i, &[lo, hi]) in self.columns.iter().enumerate() {
            if lo > hi || hi > 255 {
                return Err(format!("columns[{i}]: {lo}..{hi}"));
            }
            if i > 0 && lo < f_lo {
                return Err(format!("columns[{i}]: lo {lo} below forward {f_lo}"));
            }
            minimum = minimum.saturating_add(lo);
        }
        if self.max_nodes < minimum || self.max_nodes > 255 {
            return Err(format!(
                "max_nodes {} needs {minimum}..=255",
                self.max_nodes
            ));
        }
        let w = [
            self.weights.harbour,
            self.weights.normal,
            self.weights.hostile,
        ];
        if w.iter().any(|x| !x.is_finite() || *x < 0.0) || w.iter().sum::<f64>() <= 0.0 {
            return Err("weights: finite, zero or more, sum above zero".into());
        }
        for (name, [lo, hi]) in [
            ("harbour", self.danger.harbour),
            ("normal", self.danger.normal),
            ("hostile", self.danger.hostile),
        ] {
            if lo > hi || hi > 5 {
                return Err(format!("danger.{name}: {lo}..{hi}"));
            }
        }
        if self.danger.harbour != [0, 0] {
            return Err("danger.harbour: must be [0, 0]".into());
        }
        if self.max_harbours < 2 {
            return Err("max_harbours: need at least 2".into());
        }
        Ok(())
    }
}

impl Map {
    pub const START: NodeId = NodeId(0);

    pub fn generate(seed: u64, rules: &MapRules, names: &Names) -> Result<Map, String> {
        let sizes = column_sizes(seed, rules)?;
        let starts: Vec<usize> = sizes
            .iter()
            .scan(0, |at, &size| {
                let start = *at;
                *at += size;
                Some(start)
            })
            .collect();
        let fwd = edges(seed, rules, &sizes, &starts)?;

        let mut nodes = Vec::with_capacity(fwd.len());
        for (c, (&start, &size)) in starts.iter().zip(&sizes).enumerate() {
            for i in start..start + size {
                nodes.push(Node {
                    id: NodeId(i as u8),
                    column: c as u8,
                    kind: SystemKind::Normal,
                    danger: 0,
                    seed: 0,
                    name: String::new(),
                    forward: fwd[i].iter().map(|&t| NodeId(t as u8)).collect(),
                });
            }
        }
        let mut map = Map { seed, nodes };
        map.nodes[0].kind = SystemKind::Harbour;

        let attempt = (0..KIND_ATTEMPTS).find(|&a| {
            let mut rng = Rng::derive(seed, KEY_KINDS + a);
            for node in map.nodes.iter_mut().skip(1) {
                node.kind = weighted_kind(&mut rng, &rules.weights);
            }
            map.check_types(rules).is_ok()
        });
        if attempt.is_none() {
            return Err("no map within the rules".into());
        }

        let last = (rules.columns.len() - 1) as f64;
        let mut rng = Rng::derive(seed, KEY_DANGER);
        for node in &mut map.nodes {
            let [lo, hi] = danger_range(rules, node.kind);
            let t = 0.5 * f64::from(node.column) / last + 0.5 * rng.unit();
            let d = f64::from(lo) + (t * f64::from(hi - lo)).round();
            node.danger = d.clamp(f64::from(lo), f64::from(hi)) as u8;
        }

        let mut used_names = HashSet::new();
        let mut used_seeds = HashSet::new();
        for node in &mut map.nodes {
            let mut rng = Rng::derive(seed, KEY_SYSTEM ^ u64::from(node.id.0));
            let found = (0..NAME_TRIES).find_map(|_| {
                let s = rng.next_u64();
                let name = names.system_name(s);
                let free = !used_names.contains(&name) && !used_seeds.contains(&s);
                free.then_some((s, name))
            });
            let (s, name) =
                found.ok_or_else(|| format!("no free name for system {}", node.id.0))?;
            used_names.insert(name.clone());
            used_seeds.insert(s);
            node.seed = s;
            node.name = name;
        }

        map.check(rules)?;
        Ok(map)
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0 as usize).filter(|n| n.id == id)
    }

    pub fn column_count(&self) -> u8 {
        self.nodes.last().map_or(0, |n| n.column.saturating_add(1))
    }

    pub fn is_last(&self, id: NodeId) -> bool {
        match (self.node(id), self.nodes.last()) {
            (Some(n), Some(last)) => n.column == last.column,
            _ => false,
        }
    }

    pub fn forward(&self, id: NodeId) -> &[NodeId] {
        self.node(id).map_or(&[], |n| n.forward.as_slice())
    }

    /// The systems that link forward to `id`, ascending. Nodes are stored in id order, so the
    /// scan already yields them sorted.
    pub fn back(&self, id: NodeId) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter(|n| n.forward.contains(&id))
            .map(|n| n.id)
            .collect()
    }

    pub fn check(&self, rules: &MapRules) -> Result<(), String> {
        let columns = rules.columns.len();
        if self.nodes.is_empty() {
            return Err("no nodes".into());
        }
        if self.nodes.len() > rules.max_nodes as usize {
            return Err(format!(
                "{} nodes above max_nodes {}",
                self.nodes.len(),
                rules.max_nodes
            ));
        }
        let mut sizes = vec![0u32; columns];
        let mut prev = 0u8;
        for (i, n) in self.nodes.iter().enumerate() {
            if n.id.0 as usize != i {
                return Err(format!("node {i} has id {}", n.id.0));
            }
            if n.column < prev || n.column as usize >= columns {
                return Err(format!(
                    "node {i}: column {} out of order or range",
                    n.column
                ));
            }
            prev = n.column;
            sizes[n.column as usize] += 1;
        }
        for (c, (&size, &[lo, hi])) in sizes.iter().zip(&rules.columns).enumerate() {
            if size < lo || size > hi {
                return Err(format!("column {c}: {size} systems, rules say {lo}..{hi}"));
            }
        }

        let last = columns - 1;
        let [f_lo, f_hi] = rules.forward.map(|v| v as usize);
        for n in &self.nodes {
            let c = n.column as usize;
            if c == last {
                if !n.forward.is_empty() {
                    return Err(format!("node {}: last column links forward", n.id.0));
                }
                continue;
            }
            if n.forward.len() < f_lo || n.forward.len() > f_hi {
                return Err(format!(
                    "node {}: {} forward targets",
                    n.id.0,
                    n.forward.len()
                ));
            }
            let mut ids = n.forward.clone();
            ids.sort();
            ids.dedup();
            if ids.len() != n.forward.len() {
                return Err(format!("node {}: repeated forward target", n.id.0));
            }
            for t in &n.forward {
                match self.node(*t) {
                    Some(x) if x.column as usize == c + 1 => {}
                    _ => {
                        return Err(format!(
                            "node {}: target {} not in the next column",
                            n.id.0, t.0
                        ))
                    }
                }
            }
        }

        let mut reached = vec![false; self.nodes.len()];
        let mut todo = vec![Map::START];
        reached[0] = true;
        while let Some(id) = todo.pop() {
            for &next in self.forward(id) {
                if !reached[next.0 as usize] {
                    reached[next.0 as usize] = true;
                    todo.push(next);
                }
            }
        }
        if let Some(i) = reached.iter().position(|r| !r) {
            return Err(format!("node {i} is not reachable from the start"));
        }

        let names: HashSet<&str> = self.nodes.iter().map(|n| n.name.as_str()).collect();
        if names.len() != self.nodes.len() {
            return Err("names repeat".into());
        }
        let seeds: HashSet<u64> = self.nodes.iter().map(|n| n.seed).collect();
        if seeds.len() != self.nodes.len() {
            return Err("system seeds repeat".into());
        }

        self.check_types(rules)?;

        for n in &self.nodes {
            let [lo, hi] = danger_range(rules, n.kind);
            if n.danger < lo || n.danger > hi {
                return Err(format!(
                    "node {}: danger {} outside {lo}..{hi}",
                    n.id.0, n.danger
                ));
            }
        }
        Ok(())
    }

    /// The type rules: they look at kinds, columns and links only, so generation can use them
    /// before names and danger exist.
    // TODO(initiator): the type rules are my assumptions (initiator: "the type mix follows the rules", rules not given).
    fn check_types(&self, rules: &MapRules) -> Result<(), String> {
        let Some(start) = self.nodes.first() else {
            return Err("no nodes".into());
        };
        if start.kind != SystemKind::Harbour {
            return Err("the start is not a harbour".into());
        }
        let last = rules.columns.len() - 1;
        for n in &self.nodes {
            let c = n.column as usize;
            if c == 1 && n.kind == SystemKind::Hostile {
                return Err(format!("node {}: hostile in column 1", n.id.0));
            }
            if c == 1 && n.kind == SystemKind::Harbour {
                return Err(format!("node {}: harbour in column 1", n.id.0));
            }
            let way_out = n
                .forward
                .iter()
                .any(|t| self.node(*t).is_some_and(|x| x.kind != SystemKind::Hostile));
            if c != last && !way_out {
                return Err(format!("node {}: no non-hostile way on", n.id.0));
            }
        }
        let harbours = self
            .nodes
            .iter()
            .filter(|n| n.kind == SystemKind::Harbour)
            .count() as u32;
        if harbours > rules.max_harbours {
            return Err(format!(
                "{harbours} harbours, at most {}",
                rules.max_harbours
            ));
        }
        if !self
            .nodes
            .iter()
            .any(|n| n.column >= 2 && n.kind == SystemKind::Harbour)
        {
            return Err("no harbour from column 2 on".into());
        }
        if !self.nodes.iter().any(|n| n.kind == SystemKind::Hostile) {
            return Err("no hostile system".into());
        }
        Ok(())
    }
}

fn danger_range(rules: &MapRules, kind: SystemKind) -> [u8; 2] {
    match kind {
        SystemKind::Harbour => rules.danger.harbour,
        SystemKind::Normal => rules.danger.normal,
        SystemKind::Hostile => rules.danger.hostile,
    }
}

fn weighted_kind(rng: &mut Rng, w: &KindWeights) -> SystemKind {
    let x = rng.unit() * (w.harbour + w.normal + w.hostile);
    if x < w.harbour {
        SystemKind::Harbour
    } else if x < w.harbour + w.normal {
        SystemKind::Normal
    } else {
        SystemKind::Hostile
    }
}

fn column_sizes(seed: u64, rules: &MapRules) -> Result<Vec<usize>, String> {
    let mut rng = Rng::derive(seed, KEY_COLUMNS);
    let mut sizes: Vec<u32> = rules
        .columns
        .iter()
        .map(|&[lo, hi]| rng.count(lo, hi))
        .collect();
    while sizes.iter().sum::<u32>() > rules.max_nodes {
        let above: Vec<usize> = (0..sizes.len())
            .filter(|&c| sizes[c] > rules.columns[c][0])
            .collect();
        let c = *rng.pick(&above).ok_or("max_nodes below the minimums")?;
        sizes[c] -= 1;
    }
    Ok(sizes.into_iter().map(|s| s as usize).collect())
}

/// Forward targets per node index, sorted. Every node of a column after the first has an
/// incoming link.
fn edges(
    seed: u64,
    rules: &MapRules,
    sizes: &[usize],
    starts: &[usize],
) -> Result<Vec<Vec<usize>>, String> {
    let mut rng = Rng::derive(seed, KEY_EDGES);
    let [f_lo, f_hi] = rules.forward.map(|v| v as usize);
    let mut fwd = vec![Vec::new(); sizes.iter().sum()];
    for c in 0..sizes.len() - 1 {
        let (next_start, next_size) = (starts[c + 1], sizes[c + 1]);
        let cap = f_hi.min(next_size);
        for i in starts[c]..starts[c] + sizes[c] {
            let t = rng.count(f_lo as u32, cap as u32) as usize;
            let mut pool: Vec<usize> = (next_start..next_start + next_size).collect();
            for k in 0..t {
                let j = k + rng.count(0, (pool.len() - k - 1) as u32) as usize;
                pool.swap(k, j);
            }
            pool.truncate(t);
            pool.sort();
            fwd[i] = pool;
        }
        for x in next_start..next_start + next_size {
            if (starts[c]..starts[c] + sizes[c]).any(|i| fwd[i].contains(&x)) {
                continue;
            }
            let candidates: Vec<usize> = (starts[c]..starts[c] + sizes[c])
                .filter(|&i| fwd[i].len() < cap && !fwd[i].contains(&x))
                .collect();
            let pick = *rng
                .pick(&candidates)
                .ok_or("no room for an incoming link")?;
            fwd[pick].push(x);
            fwd[pick].sort();
        }
    }
    Ok(fwd)
}
