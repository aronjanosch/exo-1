//! Named ship types pair a physical/visual hull with their own complete flight profile.
//! JSON loading only; the app supplies file contents, this module performs no file I/O.
use crate::sc::ScTuning;
use serde::Deserialize;

/// A hull layout identifies its visuals, colliders, cabin and seat together.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HullKind {
    #[serde(rename = "kackfass_1")]
    Kackfass1,
}

/// Immutable type identity carried by an instance separately from its mutable flight state.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ShipModel {
    pub id: String,
    pub name: String,
    pub hull: HullKind,
}

impl ShipModel {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let model: Self = content_core::parse_strict("ship type", text)?;
        let mut errors = Vec::new();
        content_core::check_id("ship type", "id", &model.id, &mut errors);
        if model.name.trim().is_empty() {
            errors.push("ship type: name must not be empty".into());
        }
        if errors.is_empty() { Ok(model) } else { Err(errors.join("; ")) }
    }
}

/// A definition loaded from one type folder: `type.json` and the six flight files.
#[derive(Clone, Debug, PartialEq)]
pub struct ShipType {
    pub model: ShipModel,
    pub flight: ScTuning,
}

impl ShipType {
    pub fn from_json(model: &str, flight: [&str; 6]) -> Result<Self, String> {
        Ok(Self { model: ShipModel::from_json(model)?, flight: ScTuning::from_json(flight)? })
    }
}
