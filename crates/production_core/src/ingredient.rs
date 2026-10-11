//! The lab's records (round 6, #228): `ingredient/`, `product_base/` and `lab/tuning.json`, loaded
//! by `content_core` and checked against the kernel's commodities and effects.
use std::collections::BTreeMap;

use content_core::{File, Loaded, Record, check_id, load_records, parse_strict};
use gameplay_core::{CommodityId, Content, EffectId};
use serde::{Deserialize, Serialize};

pub const INGREDIENT_FOLDER: &str = "ingredient";
pub const BASE_FOLDER: &str = "product_base";
pub const TUNING_PATH: &str = "lab/tuning.json";
/// The tag of the goods sold as product bases (illegal).
pub const CONTRABAND_TAG: &str = "contraband";

/// Turns one effect into another in the mix, when `from` is in it and `to` is not.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub from: EffectId,
    pub to: EffectId,
}

/// A legal good that goes into the kettle: it adds one effect and may transform the ones in the mix.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ingredient {
    pub id: CommodityId,
    /// The effect it adds (none: it only transforms).
    #[serde(default)]
    pub adds: Option<EffectId>,
    /// Applied in this order, before the add.
    #[serde(default)]
    pub transforms: Vec<Transform>,
}

impl Record for Ingredient {
    type Id = CommodityId;
    fn id(&self) -> &CommodityId {
        &self.id
    }
}

/// A contraband good that the kettle starts from.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductBase {
    pub id: CommodityId,
    /// Effects the base starts with (may be empty).
    #[serde(default)]
    pub effects: Vec<EffectId>,
    /// At most this many effects in a mix. TODO(initiator): 6 is a starting value.
    pub max_effects: u8,
    /// Product crates per brew. TODO(initiator): 2 is a starting value.
    pub crates_out: u32,
}

impl Record for ProductBase {
    type Id = CommodityId;
    fn id(&self) -> &CommodityId {
        &self.id
    }
}

/// The brewing numbers. Every value is a starting value, TODO(initiator).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    /// Seconds a brew takes after `start`.
    pub brew_s: f64,
    /// Seconds for one full needle sweep 0 to 1 to 0.
    pub needle_period_s: f64,
    /// Needle position (0..1) of the best brew.
    pub sweet_spot: f64,
    /// Distance from the sweet spot at which the quality is 0.
    pub sweet_width: f64,
    /// Ingredients per kettle, after the base.
    pub max_ingredients: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LabContent {
    pub ingredients: BTreeMap<CommodityId, Loaded<Ingredient>>,
    pub bases: BTreeMap<CommodityId, Loaded<ProductBase>>,
    pub tuning: Tuning,
}

impl LabContent {
    /// Loads `ingredient/`, `product_base/` and `lab/tuning.json` from `files` and checks them
    /// against the kernel. Returns every error, each naming the file and the field.
    pub fn load(files: &[File], kernel: &Content) -> Result<LabContent, Vec<String>> {
        let mut e = Vec::new();
        let ingredients = load_records::<Ingredient>(files, INGREDIENT_FOLDER, &mut e);
        let bases = load_records::<ProductBase>(files, BASE_FOLDER, &mut e);
        let tuning = match files.iter().find(|f| f.path == TUNING_PATH) {
            None => {
                e.push(format!("{TUNING_PATH}: missing"));
                None
            }
            Some(f) => match parse_strict::<Tuning>(&f.path, &f.text) {
                Ok(t) => Some(t),
                Err(m) => {
                    e.push(m);
                    None
                }
            },
        };
        for Loaded { path, record: r } in ingredients.values() {
            check_ingredient(path, r, kernel, &mut e);
        }
        for Loaded { path, record: r } in bases.values() {
            check_base(path, r, kernel, &mut e);
        }
        for id in bases.keys().filter(|id| ingredients.contains_key(*id)) {
            e.push(format!("{BASE_FOLDER}/{id}.json: id: '{id}' is both a base and an ingredient"));
        }
        if let Some(t) = &tuning {
            check_tuning(t, &mut e);
        }
        match tuning {
            Some(tuning) if e.is_empty() => Ok(LabContent { ingredients, bases, tuning }),
            _ => Err(e),
        }
    }
}

fn has_contraband_tag(k: &Content, id: &CommodityId) -> bool {
    k.commodities.get(id).is_some_and(|c| c.record.tags.iter().any(|t| t.as_str() == CONTRABAND_TAG))
}

fn check_effect(path: &str, field: &str, id: &EffectId, k: &Content, e: &mut Vec<String>) {
    if !k.effects.contains_key(id) {
        e.push(format!("{path}: {field}: unknown effect '{id}'"));
    }
}

fn check_ingredient(path: &str, r: &Ingredient, k: &Content, e: &mut Vec<String>) {
    check_id(path, "id", r.id.as_str(), e);
    k.check_commodity(path, "id", &r.id, e);
    if has_contraband_tag(k, &r.id) {
        e.push(format!("{path}: id: '{}' is tagged '{CONTRABAND_TAG}', an ingredient is a legal good", r.id));
    }
    if r.adds.is_none() && r.transforms.is_empty() {
        e.push(format!("{path}: adds: needs an effect or a transform"));
    }
    if let Some(a) = &r.adds {
        check_effect(path, "adds", a, k, e);
    }
    for (i, t) in r.transforms.iter().enumerate() {
        check_effect(path, &format!("transforms[{i}].from"), &t.from, k, e);
        check_effect(path, &format!("transforms[{i}].to"), &t.to, k, e);
        if t.from == t.to {
            e.push(format!("{path}: transforms[{i}]: from and to are the same '{}'", t.from));
        }
    }
}

fn check_base(path: &str, r: &ProductBase, k: &Content, e: &mut Vec<String>) {
    check_id(path, "id", r.id.as_str(), e);
    k.check_commodity(path, "id", &r.id, e);
    if k.commodities.contains_key(&r.id) && !has_contraband_tag(k, &r.id) {
        e.push(format!("{path}: id: '{}' must be tagged '{CONTRABAND_TAG}'", r.id));
    }
    for (i, f) in r.effects.iter().enumerate() {
        check_effect(path, &format!("effects[{i}]"), f, k, e);
    }
    let mut seen = std::collections::HashSet::new();
    if r.effects.iter().any(|f| !seen.insert(f)) {
        e.push(format!("{path}: effects: duplicate effect"));
    }
    if r.max_effects == 0 {
        e.push(format!("{path}: max_effects: must be at least 1"));
    }
    if r.effects.len() > r.max_effects as usize {
        e.push(format!("{path}: effects: {} start effects, but max_effects is {}", r.effects.len(), r.max_effects));
    }
    if r.crates_out == 0 {
        e.push(format!("{path}: crates_out: must be at least 1"));
    }
}

fn check_tuning(t: &Tuning, e: &mut Vec<String>) {
    let p = TUNING_PATH;
    if !(t.brew_s > 0.0) {
        e.push(format!("{p}: brew_s: must be > 0"));
    }
    if !(t.needle_period_s > 0.0) {
        e.push(format!("{p}: needle_period_s: must be > 0"));
    }
    if !(0.0..=1.0).contains(&t.sweet_spot) {
        e.push(format!("{p}: sweet_spot: must be in 0..1"));
    }
    if !(t.sweet_width > 0.0) {
        e.push(format!("{p}: sweet_width: must be > 0"));
    }
    if t.max_ingredients == 0 {
        e.push(format!("{p}: max_ingredients: must be at least 1"));
    }
}
