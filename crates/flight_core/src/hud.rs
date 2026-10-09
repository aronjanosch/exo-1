//! HUD values that are data (`content/tuning/hud.json`, #91) and the altitude rule. Plain Rust.
use crate::parse_tuning;
use serde::Deserialize;

/// HUD values (`content/tuning/hud.json`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HudTuning {
    /// m: below this height above the ground the altitude element shows AGL, above it ALT.
    pub agl_below: f64,
}

impl HudTuning {
    pub fn from_json(s: &str) -> Result<HudTuning, String> {
        let t: HudTuning = parse_tuning("hud.json", s)?;
        if !(t.agl_below >= 0.0) {
            return Err(format!("hud.json: agl_below {} out of range", t.agl_below));
        }
        Ok(t)
    }
}

/// Which height the altitude element shows (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Height {
    /// Above the terrain under the player, near the ground.
    Agl(f64),
    /// Above the planet's reference sphere.
    Alt(f64),
}

impl Height {
    /// AGL below `agl_below` m above the ground, else ALT.
    pub fn pick(above_ground: f64, above_sphere: f64, agl_below: f64) -> Height {
        if above_ground < agl_below { Height::Agl(above_ground) } else { Height::Alt(above_sphere) }
    }
}

