//! Supply: how fast a wearing item runs down while it is used, and the racks on the wall
//! that give a full one for an empty one (Barotrauma's extinguisher bracket). The values are in
//! `content/gameplay/fire/extinguisher.json`; the racks' places are the level's (`exo_app`).
use serde::Deserialize;

use crate::authority::RackId;

/// The file, relative to the gameplay content.
pub const FILE: &str = "fire/extinguisher.json";

/// The drain and the racks, from `FILE`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SupplyConfig {
    /// The file's note (`_comment`), read by people only.
    #[serde(default, rename = "_comment")]
    pub comment: Option<String>,
    /// Condition (percent) lost per second while the item is used (sprayed).
    pub drain_per_s: f64,
    pub racks: Vec<RackSpec>,
}

/// One rack: its id in the world, the level's box it stands in, its slots and its starting stock.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RackSpec {
    pub id: RackId,
    #[serde(rename = "box")]
    pub box_id: String,
    pub slots: usize,
    pub stock: usize,
}

impl SupplyConfig {
    /// Parses the file's text and checks it.
    pub fn parse(text: &str) -> Result<SupplyConfig, String> {
        let c: SupplyConfig = crate::defs::parse_content(FILE, text)?;
        if !(c.drain_per_s > 0.0) {
            return Err(format!("{FILE}: drain_per_s must be positive"));
        }
        for r in &c.racks {
            if r.slots == 0 || r.stock > r.slots {
                return Err(format!("{FILE}: rack {} needs slots for its stock", r.id));
            }
        }
        Ok(c)
    }

    /// The rack standing in a box, if any.
    pub fn rack_in(&self, box_id: &str) -> Option<RackId> {
        self.racks.iter().find(|r| r.box_id == box_id).map(|r| r.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = r#"{ "drain_per_s": 2.0, "racks": [ { "id": 1, "box": "a", "slots": 3, "stock": 2 } ] }"#;

    #[test]
    fn a_valid_file_parses_and_finds_its_rack() {
        let c = SupplyConfig::parse(TEXT).unwrap();
        assert_eq!(c.rack_in("a"), Some(1));
        assert_eq!(c.rack_in("b"), None);
    }

    #[test]
    fn a_stock_bigger_than_the_rack_or_no_drain_is_refused() {
        assert!(SupplyConfig::parse(&TEXT.replace("\"stock\": 2", "\"stock\": 9")).is_err());
        assert!(SupplyConfig::parse(&TEXT.replace("2.0", "0.0")).is_err());
        assert!(SupplyConfig::parse(&TEXT.replace("\"slots\": 3", "\"slots\": 3, \"x\": 1")).is_err(), "unknown fields are refused");
    }
}
