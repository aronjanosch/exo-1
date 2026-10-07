//! The recipe is data (spikes/planet_gen/recipe.json): no code, no expressions.
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Fractal {
    None,
    Fbm,
    Ridged,
}

#[derive(Deserialize, Clone, Debug)]
pub struct NoiseSpec {
    pub seed_offset: i32,
    pub frequency: f32,
    pub fractal: Fractal,
    pub octaves: i32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Elevation {
    pub noise: NoiseSpec,
    pub amplitude: f64,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Moisture {
    pub noise: NoiseSpec,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Temperature {
    pub noise: NoiseSpec,
    pub base: f64,
    pub latitude_gain: f64,
    pub noise_gain: f64,
    /// Temperature drop per metre above sea level (applied per vertex).
    pub lapse_per_m: f64,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Landform {
    pub noise: NoiseSpec,
    pub classes: i32,
}
#[derive(Deserialize, Clone, Debug)]
pub struct MacroSpec {
    pub resolution: usize,
    pub elevation: Elevation,
    pub moisture: Moisture,
    pub temperature: Temperature,
    pub landform: Landform,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Warp {
    pub seed_offset: i32,
    pub frequency: f32,
    pub amplitude: f64,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Band {
    pub name: String,
    pub amplitude: f64,
    pub noise: NoiseSpec,
    #[serde(default)]
    pub warp: Option<Warp>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Stamp {
    Basin {
        center: [f64; 3],
        radius_m: f64,
        depth_m: f64,
    },
    Escarpment {
        center: [f64; 3],
        length_m: f64,
        height_m: f64,
        slope_width_m: f64,
        shelf_depth_m: f64,
        end_taper_m: f64,
        #[serde(default)]
        landform: Option<i32>,
    },
    Plateau {
        center: [f64; 3],
        radius_m: f64,
        height_m: f64,
        falloff_m: f64,
    },
}

#[derive(Deserialize, Clone, Debug)]
pub struct SeaLevel {
    pub land_fraction: f64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    HeightAboveSea,
    Temperature,
    Moisture,
    Landform,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Condition {
    pub field: Field,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default, rename = "in")]
    pub one_of: Option<Vec<i32>>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct BiomeRow {
    pub id: u8,
    pub color: [f32; 3],
    #[serde(default)]
    pub tints: HashMap<String, [f32; 3]>,
    /// All conditions must hold; the first matching row wins. No `when` = catch-all.
    #[serde(default)]
    pub when: Vec<Condition>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ScatterMask {
    pub noise: NoiseSpec,
    pub threshold: f32,
}
#[derive(Deserialize, Clone, Debug)]
pub struct ScatterRule {
    pub kind: String,
    pub spacing_m: f64,
    pub slope_max_deg: f64,
    pub above_sea: bool,
    pub mask: ScatterMask,
    pub row_density: HashMap<String, f32>,
    pub scale_min: f32,
    pub scale_max: f32,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SiteRule {
    pub count: usize,
    pub min_separation_m: f64,
    pub clear_radius_m: f64,
    pub max_slope_deg: f64,
    pub min_height_above_sea_m: f64,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Recipe {
    pub seed: i32,
    pub radius: f64,
    #[serde(rename = "macro")]
    pub macro_: MacroSpec,
    pub bands: Vec<Band>,
    pub stamps: Vec<Stamp>,
    pub sea_level: SeaLevel,
    pub biomes: Vec<BiomeRow>,
    pub scatter: Vec<ScatterRule>,
    pub sites: SiteRule,
}

impl Recipe {
    pub fn from_json(s: &str) -> Result<Recipe, String> {
        let r: Recipe = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if r.biomes.is_empty() || !r.biomes.last().unwrap().when.is_empty() {
            return Err("last biome row must be a catch-all (no `when`)".into());
        }
        let mut ids: Vec<u8> = r.biomes.iter().map(|b| b.id).collect();
        ids.sort();
        ids.dedup();
        if ids.len() != r.biomes.len() {
            return Err("duplicate biome ids".into());
        }
        if r.macro_.resolution < 8 {
            return Err("macro resolution too small".into());
        }
        Ok(r)
    }
}
