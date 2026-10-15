//! Seed-rolled variation of one archetype recipe (#281): the recipe's `variation` block gives
//! ranges, the planet's seed rolls one value per range, `Recipe::for_planet` applies them.
//! Same recipe + same seed = same `Tweaks`; the terrain cache key includes them.
use serde::Deserialize;

use crate::coarse::{FNV_START, fnv};

/// Ranges the seed rolls inside, as data (`variation` in `content/planet/<id>.json`).
/// All four are [min, max]; a range with min = max is a fixed value.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VariationSpec {
    /// The one planet seed that is authored by hand (the places in `content/place/` without a
    /// `seed` belong to it): it keeps the recipe exactly as written, with no tweaks.
    #[serde(default)]
    pub authored_seed: Option<i32>,
    /// Added to `macro.temperature.base` (the climate scale of the recipe: a hotter or colder planet).
    pub climate_base: [f64; 2],
    /// Share of the surface under water, 0..1: replaces `1 - sea_level.land_fraction`.
    pub sea_share: [f64; 2],
    /// Added to the moisture field (wetter or drier everywhere; the field is about -1..1).
    pub moisture_bias: [f64; 2],
    /// Hue rotation (degrees) of the biome colours, tints and strata: the same planet in another colour.
    pub palette_shift_deg: [f64; 2],
}

/// The rolled values of one planet instance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tweaks {
    pub climate_base: f64,
    pub sea_share: f64,
    pub moisture_bias: f64,
    pub palette_shift_deg: f64,
}

impl Tweaks {
    /// No change to the recipe (`sea_share` is the recipe's own).
    pub fn identity(sea_share: f64) -> Tweaks {
        Tweaks { climate_base: 0.0, sea_share, moisture_bias: 0.0, palette_shift_deg: 0.0 }
    }

    /// One value per range, from the seed alone (splitmix64, one step per value).
    pub fn roll(spec: &VariationSpec, seed: i32) -> Tweaks {
        let mut state = (seed as u32 as u64) ^ 0x5851_F42D_4C95_7F2D;
        let mut pick = |r: [f64; 2]| {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            r[0] + (r[1] - r[0]) * ((z >> 11) as f64 / (1u64 << 53) as f64)
        };
        Tweaks { climate_base: pick(spec.climate_base), sea_share: pick(spec.sea_share), moisture_bias: pick(spec.moisture_bias), palette_shift_deg: pick(spec.palette_shift_deg) }
    }

    /// Every value inside its range of `spec`.
    pub fn within(&self, spec: &VariationSpec) -> bool {
        let inside = |v: f64, r: [f64; 2]| v >= r[0].min(r[1]) && v <= r[0].max(r[1]);
        inside(self.climate_base, spec.climate_base) && inside(self.sea_share, spec.sea_share) && inside(self.moisture_bias, spec.moisture_bias) && inside(self.palette_shift_deg, spec.palette_shift_deg)
    }

    /// Part of the terrain cache key.
    pub fn hash(&self) -> u64 {
        let mut h = FNV_START;
        for v in [self.climate_base, self.sea_share, self.moisture_bias, self.palette_shift_deg] {
            h = fnv(&v.to_bits().to_le_bytes(), h);
        }
        h
    }
}

impl VariationSpec {
    pub fn validate(&self) -> Result<(), String> {
        for (name, r) in [("climate_base", self.climate_base), ("sea_share", self.sea_share), ("moisture_bias", self.moisture_bias), ("palette_shift_deg", self.palette_shift_deg)] {
            if !(r[0].is_finite() && r[1].is_finite() && r[0] <= r[1]) {
                return Err(format!("variation.{name}: [min, max] with min <= max"));
            }
        }
        if self.sea_share[0] < 0.0 || self.sea_share[1] > 1.0 {
            return Err("variation.sea_share: within 0..1".into());
        }
        Ok(())
    }
}

/// Rotate the hue of a linear colour about the grey axis (luminance kept, as the usual matrix).
pub fn shift_hue(c: [f32; 3], deg: f64) -> [f32; 3] {
    if deg == 0.0 {
        return c;
    }
    let (s, k) = deg.to_radians().sin_cos();
    let (s, k) = (s as f32, k as f32);
    let m = [
        [0.213 + k * 0.787 - s * 0.213, 0.715 - k * 0.715 - s * 0.715, 0.072 - k * 0.072 + s * 0.928],
        [0.213 - k * 0.213 + s * 0.143, 0.715 + k * 0.285 + s * 0.140, 0.072 - k * 0.072 - s * 0.283],
        [0.213 - k * 0.213 - s * 0.787, 0.715 - k * 0.715 + s * 0.715, 0.072 + k * 0.928 + s * 0.072],
    ];
    let mut o = [0.0; 3];
    for (i, row) in m.iter().enumerate() {
        o[i] = (row[0] * c[0] + row[1] * c[1] + row[2] * c[2]).clamp(0.0, 1.0);
    }
    o
}
