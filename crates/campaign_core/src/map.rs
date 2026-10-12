use crate::names::Names;
use crate::template::SystemKind;
use serde::{Deserialize, Serialize};

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
    pub fn from_json(_json: &str) -> Result<MapRules, String> {
        todo!()
    }
}

impl Map {
    pub const START: NodeId = NodeId(0);

    pub fn generate(_seed: u64, _rules: &MapRules, _names: &Names) -> Result<Map, String> {
        todo!()
    }

    pub fn node(&self, _id: NodeId) -> Option<&Node> {
        todo!()
    }

    pub fn column_count(&self) -> u8 {
        todo!()
    }

    pub fn is_last(&self, _id: NodeId) -> bool {
        todo!()
    }

    pub fn forward(&self, _id: NodeId) -> &[NodeId] {
        todo!()
    }

    pub fn back(&self, _id: NodeId) -> Vec<NodeId> {
        todo!()
    }

    pub fn check(&self, _rules: &MapRules) -> Result<(), String> {
        todo!()
    }
}
