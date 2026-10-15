//! that give a full one for an empty one (Barotrauma's extinguisher bracket). The values are in
use serde::Deserialize;

use crate::authority::RackId;

pub const FILE: &str = "fire/extinguisher.json";

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SupplyConfig {
    #[serde(default, rename = "_comment")]
    pub comment: Option<String>,
    /// Condition (percent) lost per second while the item is used (sprayed).
    pub drain_per_s: f64,
    pub racks: Vec<RackSpec>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RackSpec {
    pub id: RackId,
    #[serde(rename = "box")]
    pub box_id: String,
    pub slots: usize,
    pub stock: usize,
    /// The item the rack holds (r40, the suit rack's O2 mask); the extinguisher when left out.
    #[serde(default)]
    pub item: Option<String>,
    /// The tag a slot takes (r40, "mask" for the suit rack); the extinguisher's tag when left out.
    #[serde(default)]
    pub tag: Option<String>,
    /// The name the rack's prompt shows (r41, "Suit rack"); "Extinguisher rack" when left out.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RackFile {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    racks: Vec<RackSpec>,
}

/// The racks of a file that holds only racks (no drain), as another owner's gear keeps them.
pub fn parse_racks(file: &str, text: &str) -> Result<Vec<RackSpec>, String> {
    let f: RackFile = crate::defs::parse_content(file, text)?;
    for r in &f.racks {
        if r.slots == 0 || r.stock > r.slots {
            return Err(format!("{file}: rack {} needs slots for its stock", r.id));
        }
    }
    Ok(f.racks)
}

impl SupplyConfig {
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

    pub fn rack_in(&self, box_id: &str) -> Option<RackId> {
        self.racks.iter().find(|r| r.box_id == box_id).map(|r| r.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = r#"{ "drain_per_s": 2.0, "racks": [ { "id": 1, "box": "a", "slots": 3, "stock": 2 } ] }"#;

    #[test]
    fn a_rack_names_itself_for_its_prompt_when_it_says_so() {
        let c = SupplyConfig::parse(&TEXT.replace("\"stock\": 2", "\"stock\": 2, \"name\": \"Suit rack\"")).expect("parses");
        assert_eq!(c.racks[0].name.as_deref(), Some("Suit rack"));
        assert_eq!(SupplyConfig::parse(TEXT).expect("parses").racks[0].name, None);
    }

    #[test]
    fn a_racks_only_file_parses_and_checks_its_stock() {
        let ok = r#"{ "_comment": "x", "racks": [ { "id": 3, "box": "b", "slots": 2, "stock": 2, "item": "suit" } ] }"#;
        assert_eq!(parse_racks("g.json", ok).expect("parses")[0].item.as_deref(), Some("suit"));
        assert!(parse_racks("g.json", &ok.replace("\"stock\": 2", "\"stock\": 3")).is_err());
    }

    #[test]
    fn a_stock_bigger_than_the_rack_or_no_drain_is_refused() {
        assert!(SupplyConfig::parse(&TEXT.replace("\"stock\": 2", "\"stock\": 9")).is_err());
        assert!(SupplyConfig::parse(&TEXT.replace("2.0", "0.0")).is_err());
        assert!(SupplyConfig::parse(&TEXT.replace("\"slots\": 3", "\"slots\": 3, \"x\": 1")).is_err(), "unknown fields are refused");
    }
}
