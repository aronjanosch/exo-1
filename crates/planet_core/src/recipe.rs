//! The recipe is data (`content/planet/<id>.json`, one per planet): no code, no expressions.
//! Unknown fields are rejected.
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Fractal {
    None,
    Fbm,
    Ridged,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct NoiseSpec {
    pub seed_offset: i32,
    pub frequency: f32,
    pub fractal: Fractal,
    pub octaves: i32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Elevation {
    pub noise: NoiseSpec,
    pub amplitude: f64,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Moisture {
    pub noise: NoiseSpec,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Temperature {
    pub noise: NoiseSpec,
    pub base: f64,
    pub latitude_gain: f64,
    pub noise_gain: f64,
    /// Temperature drop per metre above sea level (applied per vertex).
    pub lapse_per_m: f64,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Landform {
    pub noise: NoiseSpec,
    pub classes: i32,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct MacroSpec {
    pub resolution: usize,
    pub elevation: Elevation,
    pub moisture: Moisture,
    pub temperature: Temperature,
    pub landform: Landform,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Warp {
    pub seed_offset: i32,
    pub frequency: f32,
    pub amplitude: f64,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Band {
    pub name: String,
    pub amplitude: f64,
    pub noise: NoiseSpec,
    #[serde(default)]
    pub warp: Option<Warp>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub field: Field,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default, rename = "in")]
    pub one_of: Option<Vec<i32>>,
}
/// Terrain colours of a biome row (#66): flat ground, steep rock, and how much of the planet's
/// strata and cap shows (0..1). Blended at borders by the mesh.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    pub ground: [f32; 3],
    pub rock: [f32; 3],
    pub strata: f32,
    pub cap: f32,
}

/// The terrain material's planet-wide values (#66): where rock takes over, the strata bands on
/// steep faces, the cap high up, the detail pattern near the camera.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct TerrainLook {
    /// Slope (degrees) where rock starts and where it is full.
    pub rock_slope_deg: [f32; 2],
    /// Band colours, bottom to top, repeating (1 to 4).
    pub strata_colors: Vec<[f32; 3]>,
    pub strata_band_m: f32,
    /// Bands wobble up and down by this much (m).
    pub strata_jitter_m: f32,
    pub cap_color: [f32; 3],
    /// Cap starts this high above the sea (m) and is full `cap_fade_m` higher.
    pub cap_height_m: f32,
    pub cap_fade_m: f32,
    /// Brightness variation of the grain near the camera (0..1), and where it has faded (m).
    pub detail_strength: f32,
    pub detail_far_m: f32,
}

/// Sky and haze of a planet (#67), for Bevy's `Atmosphere`: scattering per kilometre (our
/// atmosphere is 1.2 km, so about 80 times Earth's per-metre values for a similar look), the
/// share of the atmosphere height each term falls off over, and a distance haze.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Sky {
    /// Rayleigh scattering per km (r, g, b): the colour of the sky.
    pub rayleigh_per_km: [f32; 3],
    pub rayleigh_scale: f32,
    pub mie_per_km: f32,
    pub mie_absorption_per_km: f32,
    pub mie_asymmetry: f32,
    pub mie_scale: f32,
    /// Absorption per km (r, g, b) of a layer in the middle of the atmosphere (Earth: ozone).
    pub absorption_per_km: [f32; 3],
    pub ground_albedo: f32,
    pub haze_color: [f32; 3],
    /// Exponential distance fog density per metre at the ground (fades out with height).
    pub haze_density: f32,
}

/// Water of a planet (#67): surface colour and opacity, the ground tint below it by depth
/// (shallow lighter), a wet band along the shore, the ripple of the surface.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Water {
    pub surface: [f32; 3],
    pub alpha: f32,
    pub deep: [f32; 3],
    /// Depth (m) at which the ground below has turned into `deep`.
    pub depth_m: f32,
    pub shore: [f32; 3],
    pub shore_width_m: f32,
    /// Ripple pattern size (m) and speed (m/s).
    pub ripple_m: f32,
    pub ripple_speed: f32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BiomeRow {
    pub id: u8,
    /// Map colour (atlas, orbit impostor).
    pub color: [f32; 3],
    pub palette: Palette,
    #[serde(default)]
    pub tints: HashMap<String, [f32; 3]>,
    /// Scatter multipliers by entry or group id (0 = off, missing = 1), #65.
    #[serde(default)]
    pub scatter: BTreeMap<String, f32>,
    /// All conditions must hold; the first matching row wins. No `when` = catch-all.
    #[serde(default)]
    pub when: Vec<Condition>,
}

/// A named noise mask for scatter (a forest, a rock field): weight 0 below `threshold`, rising to
/// 1 over `edge` above it, so a wood has an inside and an edge.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ScatterMask {
    pub noise: NoiseSpec,
    pub threshold: f32,
    #[serde(default = "default_edge")]
    pub edge: f32,
}
fn default_edge() -> f32 {
    0.05
}

/// A storey of the scatter (#65): its own grid spacing and view range. Ground cover, shrubs and
/// rocks, trees.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Storey {
    /// Grid spacing of the candidate spots (m).
    pub spacing_m: f64,
    /// Drawn up to this distance from the camera on the ground (m)...
    pub range_m: f64,
    /// ...and up to this one from 200 m above the ground up.
    pub elevated_range_m: f64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ClusterShape {
    pub weight: f32,
    /// Extra instances around the first one, min and max.
    pub count: [u32; 2],
    /// Distance between neighbours, min and max (m).
    pub spacing_m: [f64; 2],
}

/// How an entry clusters: with `chance` a placed instance gets company, shaped by a weighted
/// pick of `shapes`. A lone rock and a boulder field are the same mesh with different presets.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ClusterPreset {
    pub chance: f32,
    #[serde(default)]
    pub shapes: Vec<ClusterShape>,
}

/// A second roll on a placed instance: a rare look (another mesh, a scale, a tint).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Rare {
    pub chance: f32,
    pub mesh: String,
    #[serde(default = "one")]
    pub scale: f32,
    #[serde(default)]
    pub tint: Option<[f32; 3]>,
}
fn one() -> f32 {
    1.0
}

/// One thing that can stand on the ground. Its weight is a chance per candidate spot of its
/// storey (times the group weight, the biome multiplier and the mask).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ScatterEntry {
    pub id: String,
    pub weight: f32,
    pub storey: String,
    /// Mesh set; the planet's `meshes` map picks the prop (the variant).
    pub mesh: String,
    /// Key into the biome row's `tints` (falls back to the row colour).
    pub tint: String,
    #[serde(default)]
    pub cluster: Option<String>,
    #[serde(default)]
    pub mask: Option<String>,
    #[serde(default = "slope_any")]
    pub slope_deg: [f64; 2],
    #[serde(default = "height_any")]
    pub height_above_sea_m: [f64; 2],
    /// 0 = upright along the radius, 1 = along the ground's normal.
    #[serde(default)]
    pub align: f64,
    pub scale: [f32; 2],
    /// Sunk into the ground by this much (m, at scale 1).
    #[serde(default)]
    pub sink_m: f64,
    /// Kept out of a site's clear radius.
    #[serde(default = "yes")]
    pub clear_sites: bool,
    #[serde(default)]
    pub rare: Option<Rare>,
}
fn slope_any() -> [f64; 2] {
    [0.0, 90.0]
}
fn height_any() -> [f64; 2] {
    [-1.0e9, 1.0e9]
}
fn yes() -> bool {
    true
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ScatterGroup {
    pub id: String,
    pub weight: f32,
    pub entries: Vec<ScatterEntry>,
}

/// The planet's scatter (#65): planet-wide groups of entries; biome rows only multiply them.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ScatterSpec {
    pub storeys: BTreeMap<String, Storey>,
    pub masks: BTreeMap<String, ScatterMask>,
    pub clusters: BTreeMap<String, ClusterPreset>,
    /// Mesh set -> prop id (`content/props/<id>.glb`): the planet's variant of each set.
    pub meshes: BTreeMap<String, String>,
    pub groups: Vec<ScatterGroup>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SiteRule {
    pub count: usize,
    pub min_separation_m: f64,
    pub clear_radius_m: f64,
    pub max_slope_deg: f64,
    pub min_height_above_sea_m: f64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    #[serde(rename = "_comment", default)]
    pub comment: String,
    /// From the planet system (`content/system/system.json`), not from the recipe file.
    #[serde(skip)]
    pub seed: i32,
    #[serde(skip)]
    pub radius: f64,
    #[serde(rename = "macro")]
    pub macro_: MacroSpec,
    pub bands: Vec<Band>,
    pub stamps: Vec<Stamp>,
    pub sea_level: SeaLevel,
    pub material: TerrainLook,
    pub sky: Sky,
    pub water: Water,
    pub biomes: Vec<BiomeRow>,
    pub scatter: ScatterSpec,
    pub sites: SiteRule,
}

impl Recipe {
    /// A recipe file for a planet of the system: its seed and radius.
    pub fn for_planet(s: &str, seed: i32, radius: f64) -> Result<Recipe, String> {
        let mut r = Recipe::from_json(s)?;
        r.seed = seed;
        r.radius = radius;
        Ok(r)
    }

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
        r.check_scatter()?;
        if r.material.strata_colors.is_empty() || r.material.strata_colors.len() > 4 {
            return Err("material.strata_colors: 1 to 4 colours".into());
        }
        if r.macro_.resolution < 8 {
            return Err("macro resolution too small".into());
        }
        Ok(r)
    }

    fn check_scatter(&self) -> Result<(), String> {
        let sc = &self.scatter;
        let mut ids: Vec<&str> = Vec::new();
        for g in &sc.groups {
            ids.push(&g.id);
            for e in &g.entries {
                ids.push(&e.id);
                let at = |what: &str, name: &str| format!("scatter entry {}: {what} '{name}' is not defined", e.id);
                if !sc.storeys.contains_key(&e.storey) {
                    return Err(at("storey", &e.storey));
                }
                if !sc.meshes.contains_key(&e.mesh) {
                    return Err(at("mesh", &e.mesh));
                }
                if let Some(c) = &e.cluster
                    && !sc.clusters.contains_key(c)
                {
                    return Err(at("cluster preset", c));
                }
                if let Some(m) = &e.mask
                    && !sc.masks.contains_key(m)
                {
                    return Err(at("mask", m));
                }
                if let Some(r) = &e.rare
                    && !sc.meshes.contains_key(&r.mesh)
                {
                    return Err(at("rare mesh", &r.mesh));
                }
            }
        }
        let n = ids.len();
        ids.sort();
        ids.dedup();
        if ids.len() != n {
            return Err("scatter group and entry ids must be unique".into());
        }
        for b in &self.biomes {
            for k in b.scatter.keys() {
                if !ids.contains(&k.as_str()) {
                    return Err(format!("biome row {}: scatter multiplier for unknown entry or group '{k}'", b.id));
                }
            }
        }
        Ok(())
    }
}
