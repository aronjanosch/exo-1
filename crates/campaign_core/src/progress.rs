use crate::map::{Map, NodeId};
use serde::{Deserialize, Serialize};

/// What the save keeps of the campaign map: the map is regenerated from `seed`, so only where the crew is and where it has been.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Progress {
    pub seed: u64,
    pub current: NodeId,
    /// Ascending, no duplicates, includes `current`.
    pub visited: Vec<NodeId>,
}

impl Progress {
    /// The crew starts at `Map::START`, which is visited.
    pub fn start(map: &Map) -> Progress {
        Progress {
            seed: map.seed,
            current: Map::START,
            visited: vec![Map::START],
        }
    }

    pub fn has_visited(&self, id: NodeId) -> bool {
        self.visited.binary_search(&id).is_ok()
    }

    /// The systems the captain may pick: the forward targets of `current`; with `allow_back` also the systems that link forward to it. Ascending, no duplicates.
    // TODO(initiator): jumping back is open (Barotrauma: possible, but brings little); the game passes allow_back = false until decided.
    pub fn targets(&self, map: &Map, allow_back: bool) -> Vec<NodeId> {
        let mut ts = map.forward(self.current).to_vec();
        if allow_back {
            ts.extend(map.back(self.current));
        }
        ts.sort();
        ts.dedup();
        ts
    }

    /// The jump: `to` must be one of `targets`; sets `current`, adds `to` to `visited` (sorted, once). Errors leave the state as it was.
    pub fn jump(&mut self, map: &Map, to: NodeId, allow_back: bool) -> Result<(), String> {
        if map.seed != self.seed {
            return Err(format!("progress is for map {}, not {}", self.seed, map.seed));
        }
        if map.node(to).is_none() {
            return Err(format!("no system {}", to.0));
        }
        if !self.targets(map, allow_back).contains(&to) {
            return Err(format!("system {} is not a target from {}", to.0, self.current.0));
        }
        self.current = to;
        if let Err(i) = self.visited.binary_search(&to) {
            self.visited.insert(i, to);
        }
        Ok(())
    }

    /// Checks a loaded state against its map: seed matches, ids exist, `current` is visited, `visited` sorted and unique, `START` visited.
    pub fn check(&self, map: &Map) -> Result<(), String> {
        if map.seed != self.seed {
            return Err(format!("progress is for map {}, not {}", self.seed, map.seed));
        }
        if let Some(id) = self.visited.iter().chain([&self.current]).find(|id| map.node(**id).is_none()) {
            return Err(format!("no system {}", id.0));
        }
        if !self.visited.windows(2).all(|w| w[0] < w[1]) {
            return Err("visited is not ascending and unique".into());
        }
        if !self.has_visited(self.current) {
            return Err("current is not visited".into());
        }
        if !self.has_visited(Map::START) {
            return Err("the start is not visited".into());
        }
        Ok(())
    }
}
