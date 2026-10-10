//! Recipe records: what transforms commodities at a station kind.
use std::collections::BTreeMap;
use std::fmt;

use content_core::{File, Loaded, Record, check_id, load_records};
use gameplay_core::{CommodityId, Content};
use serde::{Deserialize, Serialize};

pub const FOLDER: &str = "recipe";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RecipeId(pub String);

impl RecipeId {
    pub fn new(s: impl Into<String>) -> RecipeId {
        RecipeId(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RecipeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StationKind(pub String);

impl StationKind {
    pub fn new(s: impl Into<String>) -> StationKind {
        StationKind(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An input or output of a recipe: commodity and amount.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Commodity {
    pub id: CommodityId,
    pub amount: u32,
}

/// A recipe transforms inputs into outputs over a duration at a station kind.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub id: RecipeId,
    pub station: StationKind,
    pub inputs: Vec<Commodity>,
    pub outputs: Vec<Commodity>,
    /// Seconds to complete the recipe. TODO(initiator): starting values.
    pub time_s: f64,
}

impl Record for Recipe {
    type Id = RecipeId;
    fn id(&self) -> &RecipeId {
        &self.id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecipeContent {
    pub recipes: BTreeMap<RecipeId, Loaded<Recipe>>,
}

impl RecipeContent {
    /// Loads `recipe/` from `files` and checks fields and references.
    /// Returns every error, each naming the file and the field.
    pub fn load(files: &[File], kernel: &Content) -> Result<RecipeContent, Vec<String>> {
        let mut e = Vec::new();
        let recipes = load_records::<Recipe>(files, FOLDER, &mut e);
        let c = RecipeContent { recipes };
        for Loaded { path, record: r } in c.recipes.values() {
            c.check_recipe(path, r, kernel, &mut e);
        }
        if e.is_empty() { Ok(c) } else { Err(e) }
    }

    fn check_recipe(&self, path: &str, r: &Recipe, k: &Content, e: &mut Vec<String>) {
        check_id(path, "id", r.id.as_str(), e);
        if r.inputs.is_empty() {
            e.push(format!("{path}: inputs: empty"));
        }
        for (i, Commodity { id, amount }) in r.inputs.iter().enumerate() {
            k.check_commodity(path, &format!("inputs[{i}].id"), id, e);
            if *amount == 0 {
                e.push(format!("{path}: inputs[{i}].amount: must be > 0"));
            }
        }
        // Check for duplicate input ids
        let mut seen = std::collections::HashSet::new();
        for Commodity { id, .. } in &r.inputs {
            if !seen.insert(id) {
                e.push(format!("{path}: inputs: duplicate id '{id}'"));
            }
        }
        if r.outputs.is_empty() {
            e.push(format!("{path}: outputs: empty"));
        }
        for (i, Commodity { id, amount }) in r.outputs.iter().enumerate() {
            k.check_commodity(path, &format!("outputs[{i}].id"), id, e);
            if *amount == 0 {
                e.push(format!("{path}: outputs[{i}].amount: must be > 0"));
            }
        }
        // Check for duplicate output ids
        let mut seen = std::collections::HashSet::new();
        for Commodity { id, .. } in &r.outputs {
            if !seen.insert(id) {
                e.push(format!("{path}: outputs: duplicate id '{id}'"));
            }
        }
        if !(r.time_s > 0.0) {
            e.push(format!("{path}: time_s: must be > 0"));
        }
    }
}
