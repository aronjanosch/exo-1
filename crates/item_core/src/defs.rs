//! Item definitions (`content/items/*.json`), one file per item, unknown fields rejected.
//!
//! Like Barotrauma's item components, an item is a mix of a few building blocks: how it is taken
//! (`take`), the hands it needs while held, how many stack in one slot, slots of its own (a box,
//! a bag; 0 for most), what using it does (`use`, an effect id; inert in C1) and its shape as a
//! loose body in the world.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::container::{Item, Stack};

/// How the player takes it with F.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Take {
    /// Into the inventory (the action bar).
    Collect,
    /// Held in the hands, which are busy meanwhile; it never goes into the inventory (cargo).
    Carry,
}

/// The item as a loose body: full edge lengths (m), mass (kg) and its placeholder colour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub extents: [f64; 3],
    pub mass: f64,
    pub colour: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDef {
    /// Lower case letters, digits and `_`; the file is `<id>.json`.
    pub id: String,
    /// Shown in prompts and the inventory.
    pub name: String,
    /// Three capital letters that stand for it in the action bar (`WRN`); empty: the first three
    /// letters of the id (`short_code`).
    #[serde(default)]
    pub code: String,
    pub take: Take,
    /// 1 or 2: the hands it fills while held.
    pub hands: u8,
    /// How many stack in one slot (1: it does not stack).
    pub stack: u32,
    /// Slots of its own (a toolbox, a bag); 0 for most.
    #[serde(default)]
    pub slots: u32,
    /// What using it does, an effect id. Inert until items are used (after C1).
    #[serde(rename = "use", default)]
    pub use_effect: Option<String>,
    /// Seconds the use takes while LMB is held (#295); None: the use is instant.
    #[serde(default)]
    pub use_s: Option<f64>,
    /// Labels for slot rules (`small`, `tool`, `medical`).
    #[serde(default)]
    pub tags: Vec<String>,
    pub shape: Shape,
}

impl ItemDef {
    /// The three letters that stand for the item in the action bar.
    pub fn short_code(&self) -> String {
        if self.code.is_empty() { self.id.chars().take(3).collect::<String>().to_uppercase() } else { self.code.clone() }
    }

    /// The item as the containers see it.
    pub fn item(&self) -> Item {
        let tags: Vec<&str> = self.tags.iter().map(String::as_str).collect();
        Item::new(&self.id, &tags, self.stack)
    }

    /// `n` of it.
    pub fn stack_of(&self, n: u32) -> Stack {
        Stack::new(self.item(), n)
    }
}

/// Every item definition, by id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemDefs(pub BTreeMap<String, ItemDef>);

impl ItemDefs {
    /// Parses one definition per file (`path`, `text`); every error names its file.
    pub fn from_files<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<ItemDefs, String> {
        let mut out = BTreeMap::new();
        for (path, text) in files {
            let d: ItemDef = content_core::parse_strict(path, text)?;
            let at = format!("{path}: item '{}'", d.id);
            let mut errors = Vec::new();
            content_core::check_id(path, "id", &d.id, &mut errors);
            if let Some(e) = errors.pop() {
                return Err(e);
            }
            let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".json");
            if stem != d.id {
                return Err(format!("{at}: the file must be named {}.json", d.id));
            }
            if !d.code.is_empty() && !(d.code.len() == 3 && d.code.chars().all(|c| c.is_ascii_uppercase())) {
                return Err(format!("{at}: code must be three capital letters"));
            }
            if !(1..=2).contains(&d.hands) {
                return Err(format!("{at}: hands must be 1 or 2"));
            }
            if d.stack == 0 {
                return Err(format!("{at}: stack must be at least 1"));
            }
            if d.take == Take::Carry && d.stack != 1 {
                return Err(format!("{at}: a carried item does not stack"));
            }
            if d.shape.extents.iter().any(|e| !(*e > 0.0)) || !(d.shape.mass > 0.0) {
                return Err(format!("{at}: shape extents and mass must be positive"));
            }
            if out.insert(d.id.clone(), d).is_some() {
                return Err(format!("{at}: id used twice"));
            }
        }
        Ok(ItemDefs(out))
    }

    pub fn get(&self, id: &str) -> Option<&ItemDef> {
        self.0.get(id)
    }

    /// The definition of a stack's items.
    pub fn of(&self, s: &Stack) -> Option<&ItemDef> {
        self.get(s.item.kind.as_str())
    }
}
