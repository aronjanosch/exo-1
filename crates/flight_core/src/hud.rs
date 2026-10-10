//! HUD values that are data (`content/tuning/hud.json`, #91) and the altitude rule. Plain Rust.
use serde::Deserialize;

/// HUD values (`content/tuning/hud.json`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HudTuning {
    /// m: below this height above the ground the altitude element shows AGL, above it ALT.
    pub agl_below: f64,
    /// s: a flight panel toast stays this long after the last switch change (#197).
    pub toast_time: f64,
    /// g: the G bar's full scale (#200).
    pub g_full: f64,
    /// m/s: below this speed the flight path marker is hidden (#200).
    pub velocity_min: f64,
}

impl HudTuning {
    pub fn from_json(s: &str) -> Result<HudTuning, String> {
        let t: HudTuning = content_core::parse_strict("hud.json", s)?;
        if !(t.agl_below >= 0.0) {
            return Err(format!("hud.json: agl_below {} out of range", t.agl_below));
        }
        if !(t.toast_time > 0.0 && t.toast_time.is_finite()) {
            return Err(format!("hud.json: toast_time {} out of range", t.toast_time));
        }
        if !(t.g_full > 0.0 && t.g_full.is_finite()) {
            return Err(format!("hud.json: g_full {} out of range", t.g_full));
        }
        if !(t.velocity_min >= 0.0 && t.velocity_min.is_finite()) {
            return Err(format!("hud.json: velocity_min {} out of range", t.velocity_min));
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

