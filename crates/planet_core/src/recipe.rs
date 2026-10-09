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
}
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Weirdness {
    pub noise: NoiseSpec,
}
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct MacroSpec {
    pub resolution: usize,
    pub elevation: Elevation,
    pub moisture: Moisture,
    pub temperature: Temperature,
    pub landform: Landform,
    /// Variants (#68): odd places where this field is high.
    pub weirdness: Weirdness,
}

/// One of the shared macro fields (#68). All are about -1..1 except temperature (its own scale).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FieldName {
    Elevation,
    Temperature,
    Moisture,
    Landform,
    Weirdness,
}

/// Piecewise linear map from a field to a value; clamped at both ends.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Spline {
    pub field: FieldName,
    pub points: Vec<[f64; 2]>,
}

impl Spline {
    pub fn eval(&self, x: f64) -> f64 {
        let p = &self.points;
        if x <= p[0][0] {
            return p[0][1];
        }
        for w in p.windows(2) {
            if x <= w[1][0] {
                let t = (x - w[0][0]) / (w[1][0] - w[0][0]).max(1e-12);
                return w[0][1] + (w[1][1] - w[0][1]) * t;
            }
        }
        p[p.len() - 1][1]
    }
}

/// How the shared fields shape the ground (#68): a height offset, a vertical stretch of the
/// bands marked `stretch`, a roughness of the bands marked `roughness`. Biomes read the same
/// fields, so a mountain biome and a mountain shape come together.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub offset: Spline,
    pub stretch: Spline,
    pub roughness: Spline,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BandScale {
    #[default]
    None,
    Stretch,
    Roughness,
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
    /// Scaled by the shape's stretch or roughness (#68).
    #[serde(default)]
    pub scale: BandScale,
}

/// A value drawn per placement from [min, max] (or a fixed number).
#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(untagged)]
pub enum Span {
    Fixed(f64),
    Between([f64; 2]),
}

impl Span {
    pub fn pick(&self, u: f64) -> f64 {
        match *self {
            Span::Fixed(v) => v,
            Span::Between([a, b]) => a + (b - a) * u,
        }
    }
    pub fn max(&self) -> f64 {
        match *self {
            Span::Fixed(v) => v,
            Span::Between([a, b]) => a.max(b),
        }
    }
}

/// The shape of a landform stamp (#69); every size is drawn per placement from its span.
#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StampShape {
    /// A smooth bowl.
    Basin { radius_m: Span, depth_m: Span },
    /// A straight step: the upper shelf on one side, a slope, the lower ground on the other.
    Escarpment { length_m: Span, height_m: Span, slope_width_m: Span, shelf_depth_m: Span, end_taper_m: Span },
    /// A flat-topped rise.
    Plateau { radius_m: Span, height_m: Span, falloff_m: Span },
    /// A bowl with a raised rim.
    Crater { radius_m: Span, depth_m: Span, rim_height_m: Span, rim_width_m: Span },
    /// A cut with walls and a floor, along a meandering line.
    Canyon { length_m: Span, floor_width_m: Span, wall_width_m: Span, depth_m: Span, meander_m: Span, end_taper_m: Span },
    /// Several flat-topped buttes inside a radius.
    MesaField { radius_m: Span, buttes: [u32; 2], butte_radius_m: Span, height_m: Span, falloff_m: Span },
    /// A solitary needle, a silhouette to walk towards.
    Spire { height_m: Span, base_radius_m: Span },
    /// A ring mountain with a sunken centre.
    Caldera { ring_radius_m: Span, ring_height_m: Span, ring_width_m: Span, centre_depth_m: Span },
}

/// Macro fields a landform may be placed in (temperature without the lapse).
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Where {
    #[serde(default)]
    pub elevation: Option<[f64; 2]>,
    #[serde(default)]
    pub temperature: Option<[f64; 2]>,
    #[serde(default)]
    pub moisture: Option<[f64; 2]>,
    #[serde(default)]
    pub landform: Option<[f64; 2]>,
    #[serde(default)]
    pub weirdness: Option<[f64; 2]>,
}

/// One kind of landform in the planet's budget (#69): how many (a range), how far from every
/// other stamp, where.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct LandformKind {
    pub id: String,
    pub shape: StampShape,
    pub count: [u32; 2],
    /// No other stamp centre closer than this (the larger of the two kinds' values counts).
    pub min_separation_m: f64,
    #[serde(default, rename = "where")]
    pub where_: Where,
    /// Forces the landform field to this value where the stamp is high (rim biomes).
    #[serde(default)]
    pub landform: Option<f64>,
    /// The planet's signature landform: placed first, bigger than the global relief.
    #[serde(default)]
    pub signature: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Landforms {
    /// Placement tries (each with its own seed) before the bake gives up.
    pub retry_limit: u32,
    /// Random candidate spots per try.
    pub candidates: u32,
    pub kinds: Vec<LandformKind>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SeaLevel {
    pub land_fraction: f64,
}

/// A point or an interval on one axis of the biome parameter space.
#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(untagged)]
pub enum Range {
    Point(f64),
    Interval([f64; 2]),
}

impl Range {
    /// Distance from a value to the point or interval (0 inside).
    pub fn distance(&self, v: f64) -> f64 {
        match *self {
            Range::Point(p) => (v - p).abs(),
            Range::Interval([lo, hi]) => (lo - v).max(v - hi).max(0.0),
        }
    }
}

/// Where a biome row sits in parameter space (#68). Missing axes do not count.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Climate {
    #[serde(default)]
    pub elevation: Option<Range>,
    #[serde(default)]
    pub temperature: Option<Range>,
    #[serde(default)]
    pub moisture: Option<Range>,
    #[serde(default)]
    pub landform: Option<Range>,
    #[serde(default)]
    pub weirdness: Option<Range>,
    /// Metres; divided by `biome_space.height_scale_m` before it counts.
    #[serde(default)]
    pub height_above_sea: Option<Range>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BiomeSpace {
    /// Metres of height above sea that weigh like one unit of a field.
    pub height_scale_m: f64,
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

/// Erosion steps on the macro shell (#72), stream power: per step a node sinks towards its
/// downstream neighbour by `strength * catchment^area_exponent / distance` (implicit, catchment
/// in m²), never more than `max_m` in all and never below the sea.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Erosion {
    pub iterations: u32,
    pub strength: f64,
    pub area_exponent: f64,
    pub max_m: f64,
}

/// How a river crosses a sink that holds no lake (#72).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SinkCrossing {
    /// Its bed cuts the way out down to the sink's floor (a gorge through the sill).
    Cut,
    /// Sediment fills the sink to its spill point and the river runs across the flat.
    Fill,
}

/// Rivers and lakes from drainage on the macro grid (#72), cut in the bake.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DrainageSpec {
    #[serde(rename = "_comment", default)]
    pub comment: String,
    /// Rain per square metre, relative: `rain_base + rain_moisture_gain * moisture`, at least 0.
    pub rain_base: f64,
    pub rain_moisture_gain: f64,
    /// Rain-weighted catchment (km²) where a river starts.
    pub river_min_catchment_km2: f64,
    /// Bed depth at the river's start and at most (m); grows with the catchment^0.4.
    pub river_depth_m: [f64; 2],
    /// Bed width at the start and at most (m); grows with the square root of the catchment.
    pub river_width_m: [f64; 2],
    /// Share of the bed depth that holds water.
    pub river_fill: f64,
    /// A sink holds a lake when it is at least this deep, and this large or fed by a river; a
    /// river cuts through shallower ones.
    pub lake_min_depth_m: f64,
    pub lake_min_area_m2: f64,
    /// Rain-weighted catchment (m²) a lake needs per m² of its surface: a sink that gathers less
    /// holds a smaller lake that never spills, or stays dry. 0 = every sink fills to its spill.
    pub lake_evaporation: f64,
    /// Areas below the sea level count as the sea for the water from this size on (km²); smaller
    /// pockets fill to a lake or a river cuts through them. 0 = every one is sea.
    pub sea_min_area_km2: f64,
    pub river_sinks: SinkCrossing,
    pub erosion: Erosion,
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
    /// Its point or intervals in parameter space; the nearest row wins (#68).
    pub climate: Climate,
    /// Added to the row's distance (a row that should win less often gets a positive offset).
    #[serde(default)]
    pub offset: f64,
    /// Smallest area share the bake accepts for this row (quota), 0..1.
    #[serde(default)]
    pub min_share: Option<f64>,
    /// Site kinds allowed in this row (#70); missing = all.
    #[serde(default)]
    pub sites: Option<Vec<String>>,
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

/// A ground edit under a site (#70), applied after the noise in `order`. The height function
/// includes it, so mesh, collision and scatter agree.
#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    /// Level to the ground height at the site centre: a disc (`radius_m`) or a rectangle
    /// (`half_extent_m`, along the site's yaw), blending out over `rolloff_m`; `dish_m` sinks the
    /// middle a little.
    Flatten {
        order: i32,
        #[serde(default)]
        radius_m: Option<f64>,
        #[serde(default)]
        half_extent_m: Option<[f64; 2]>,
        rolloff_m: f64,
        #[serde(default)]
        dish_m: f64,
    },
    /// Pull the height towards the mean of a ring around it (the noise below, not earlier edits).
    Smooth { order: i32, radius_m: f64, strength: f64 },
    /// Raise (positive) or lower (negative) a disc, with an optional rim at its edge.
    Raise {
        order: i32,
        radius_m: f64,
        rolloff_m: f64,
        amount_m: f64,
        #[serde(default)]
        rim_m: f64,
        #[serde(default = "two")]
        rim_width_m: f64,
    },
}
fn two() -> f64 {
    2.0
}

impl Edit {
    pub fn order(&self) -> i32 {
        match self {
            Edit::Flatten { order, .. } | Edit::Smooth { order, .. } | Edit::Raise { order, .. } => *order,
        }
    }
}

/// Pieces scattered around a kit piece's spot.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PieceScatter {
    pub count: [u32; 2],
    pub radius_m: [f64; 2],
}

/// One piece of a site's kit: a prop at an offset (metres east/north in the site's turned
/// frame), or several scattered around it.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct KitPiece {
    pub prop: String,
    #[serde(default)]
    pub at: [f64; 2],
    #[serde(default)]
    pub yaw_deg: Option<f64>,
    #[serde(default = "span_one")]
    pub scale: Span,
    #[serde(default)]
    pub sink_m: f64,
    /// 0 = upright along the radius, 1 = along the ground's normal.
    #[serde(default)]
    pub align: f64,
    pub tint: [f32; 3],
    #[serde(default)]
    pub scatter: Option<PieceScatter>,
}
fn span_one() -> Span {
    Span::Fixed(1.0)
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SiteCategory {
    Site,
    Landmark,
    Find,
}

/// A kind of site (#70). Placeholder kinds until the initiator picks (ids, no fiction names).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SiteKind {
    pub id: String,
    pub category: SiteCategory,
    pub count: [u32; 2],
    /// Ordinary scatter stays out of this radius (m).
    pub footprint_m: f64,
    /// To sites of the same kind, and to every site (the larger value of the two kinds counts).
    pub min_separation_m: f64,
    pub min_separation_all_m: f64,
    /// Allowed biome rows (and the row's own `sites` list must allow the kind); missing = all.
    #[serde(default)]
    pub biomes: Option<Vec<u8>>,
    #[serde(default = "slope_flat")]
    pub slope_deg: [f64; 2],
    #[serde(default = "height_land")]
    pub height_above_sea_m: [f64; 2],
    /// Pick the highest of the candidates (landmarks: seen above the horizon) instead of the
    /// one farthest from all sites so far.
    #[serde(default)]
    pub prefer_high: bool,
    #[serde(default)]
    pub edits: Vec<Edit>,
    pub kit: String,
}
fn slope_flat() -> [f64; 2] {
    [0.0, 20.0]
}
fn height_land() -> [f64; 2] {
    [2.0, 1.0e9]
}

/// At most `max` sites of `category` within `radius_m` of each other (#70).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub category: SiteCategory,
    pub radius_m: f64,
    pub max: u32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SiteRule {
    /// Candidates compared per placed site (best-candidate sampling).
    pub best_of: u32,
    /// Random spots tried per site kind before it gives up.
    pub candidates: u32,
    pub kinds: Vec<SiteKind>,
    pub budgets: Vec<Budget>,
    pub kits: BTreeMap<String, Vec<KitPiece>>,
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
    pub landforms: Landforms,
    pub shape: Shape,
    pub biome_space: BiomeSpace,
    pub sea_level: SeaLevel,
    pub material: TerrainLook,
    pub sky: Sky,
    pub water: Water,
    /// Rivers and lakes (#72); missing = none.
    #[serde(default)]
    pub drainage: Option<DrainageSpec>,
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
        if r.biomes.is_empty() {
            return Err("at least one biome row".into());
        }
        for (name, sp) in [("offset", &r.shape.offset), ("stretch", &r.shape.stretch), ("roughness", &r.shape.roughness)] {
            if sp.points.len() < 2 || sp.points.windows(2).any(|w| w[1][0] <= w[0][0]) {
                return Err(format!("shape.{name}: at least two points with rising x"));
            }
        }
        let mut ids: Vec<u8> = r.biomes.iter().map(|b| b.id).collect();
        ids.sort();
        ids.dedup();
        if ids.len() != r.biomes.len() {
            return Err("duplicate biome ids".into());
        }
        r.check_scatter()?;
        for k in &r.sites.kinds {
            if !r.sites.kits.contains_key(&k.kit) {
                return Err(format!("site kind {}: kit '{}' is not defined", k.id, k.kit));
            }
            if k.count[0] > k.count[1] {
                return Err(format!("site kind {}: count min above max", k.id));
            }
            for e in &k.edits {
                if let Edit::Flatten { radius_m: None, half_extent_m: None, .. } = e {
                    return Err(format!("site kind {}: flatten needs radius_m or half_extent_m", k.id));
                }
            }
        }
        if r.landforms.kinds.iter().filter(|k| k.signature).count() > 1 {
            return Err("landforms: at most one signature landform".into());
        }
        for k in &r.landforms.kinds {
            if k.count[0] > k.count[1] {
                return Err(format!("landform {}: count min above max", k.id));
            }
        }
        if r.material.strata_colors.is_empty() || r.material.strata_colors.len() > 4 {
            return Err("material.strata_colors: 1 to 4 colours".into());
        }
        if r.macro_.resolution < 8 {
            return Err("macro resolution too small".into());
        }
        if let Some(d) = &r.drainage {
            if d.river_min_catchment_km2 <= 0.0 {
                return Err("drainage.river_min_catchment_km2: above 0".into());
            }
            if d.river_depth_m[0] > d.river_depth_m[1] || d.river_width_m[0] > d.river_width_m[1] || d.river_width_m[0] <= 0.0 {
                return Err("drainage: river depth and width are [start, max] with start <= max, width above 0".into());
            }
            if !(0.0..=1.0).contains(&d.river_fill) {
                return Err("drainage.river_fill: 0..1".into());
            }
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
