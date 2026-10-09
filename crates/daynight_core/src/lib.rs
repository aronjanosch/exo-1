//! daynight_core: time of day on static planets (#48), without Bevy types, in f64. The planet
//! stays still and the sun turns around the planet's axis once per `day_length_s`. The light
//! (sun, night light, ambient, sky and fog) comes from keys sorted by the sun's elevation at the
//! viewer, shared by planets through light archetypes ("looks").
//!
//! All values in `content/daynight/daynight.json` are placeholders (TODO(initiator)).
use glam::{DQuat, DVec3};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::f64::consts::TAU;

/// Planet-space direction of the spawn point; `start_hour` is its local hour at clock 0.
pub const SPAWN_UP: DVec3 = DVec3::Y;

/// The whole file: light archetypes and the sun of every planet (by recipe id).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DayNight {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    pub looks: BTreeMap<String, Look>,
    pub planets: BTreeMap<String, PlanetSun>,
}

impl DayNight {
    pub fn from_json(s: &str) -> Result<Self, String> {
        let d: DayNight = serde_json::from_str(s).map_err(|e| format!("daynight.json: {e}"))?;
        d.validate()?;
        Ok(d)
    }

    fn validate(&self) -> Result<(), String> {
        for (id, look) in &self.looks {
            look.validate().map_err(|e| format!("daynight.json: look '{id}': {e}"))?;
        }
        for (id, p) in &self.planets {
            p.validate().map_err(|e| format!("daynight.json: planet '{id}': {e}"))?;
            if !self.looks.contains_key(&p.look) {
                return Err(format!("daynight.json: planet '{id}': unknown look '{}'", p.look));
            }
        }
        Ok(())
    }

    /// The sun and look of a planet, or an error naming the missing recipe id.
    pub fn planet(&self, recipe: &str) -> Result<(&PlanetSun, &Look), String> {
        let p = self.planets.get(recipe).ok_or_else(|| format!("daynight.json: no planet '{recipe}'"))?;
        Ok((p, &self.looks[&p.look]))
    }
}

/// The sun's path around one planet.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PlanetSun {
    #[serde(rename = "_todo", default)]
    pub todo: Option<String>,
    /// One full turn of the sun (s).
    pub day_length_s: f64,
    /// Spin axis in planet space (normalised on use). The sun turns around it.
    pub axis: [f64; 3],
    /// The sun's angle above the equator (deg): season and tilt in one number.
    pub declination_deg: f64,
    /// Local hour at the spawn point (`SPAWN_UP`) at clock 0; 12 is noon.
    pub start_hour: f64,
    /// Light archetype (a key of `looks`).
    pub look: String,
}

impl PlanetSun {
    fn validate(&self) -> Result<(), String> {
        if !(self.day_length_s > 0.0) {
            return Err(format!("day_length_s must be > 0, is {}", self.day_length_s));
        }
        if DVec3::from_array(self.axis).length() < 1e-9 {
            return Err("axis must not be zero".into());
        }
        if !(self.declination_deg.abs() < 90.0) {
            return Err(format!("declination_deg must be within -90..90, is {}", self.declination_deg));
        }
        if !(0.0..24.0).contains(&self.start_hour) {
            return Err(format!("start_hour must be within 0..24, is {}", self.start_hour));
        }
        Ok(())
    }

    pub fn axis(&self) -> DVec3 {
        DVec3::from_array(self.axis).normalize()
    }

    /// Angular speed of the sun (rad/s).
    pub fn omega(&self) -> f64 {
        TAU / self.day_length_s
    }

    /// Unit vector in the equator plane under `up` (its meridian), or None at a pole.
    fn meridian(&self, up: DVec3) -> Option<DVec3> {
        let a = self.axis();
        (up - a * a.dot(up)).try_normalize()
    }

    /// Meridian used for the spawn point; at a pole any equator direction will do.
    fn spawn_meridian(&self) -> DVec3 {
        let a = self.axis();
        self.meridian(SPAWN_UP).unwrap_or_else(|| a.any_orthonormal_vector())
    }

    /// Direction towards the sun (planet space, unit) at clock `t` (s).
    pub fn sun_dir(&self, t: f64) -> DVec3 {
        let a = self.axis();
        let d = self.declination_deg.to_radians();
        let h0 = hour_angle(self.start_hour);
        let s0 = DQuat::from_axis_angle(a, h0) * self.spawn_meridian() * d.cos() + a * d.sin();
        (DQuat::from_axis_angle(a, self.omega() * t) * s0).normalize()
    }

    /// Hour angle (rad, 0 at noon, growing with time) of the sun seen from `up` at `t`.
    fn hour_angle_at(&self, up: DVec3, t: f64) -> Option<f64> {
        let a = self.axis();
        let m = self.meridian(up)?;
        let s = self.sun_dir(t);
        let se = (s - a * a.dot(s)).try_normalize()?;
        Some(a.dot(m.cross(se)).atan2(m.dot(se)))
    }

    /// Local hour (0..24, 12 at noon) at the surface point with unit `up`; None at a pole.
    pub fn local_hour(&self, up: DVec3, t: f64) -> Option<f64> {
        self.hour_angle_at(up, t).map(|h| (12.0 + h / TAU * 24.0).rem_euclid(24.0))
    }

    /// Sun elevation (deg) above the horizon of `up` at `t`.
    pub fn elevation_deg(&self, up: DVec3, t: f64) -> f64 {
        self.sun_dir(t).dot(up.normalize()).clamp(-1.0, 1.0).asin().to_degrees()
    }

    /// First clock time at or after `from` when `up` has local hour `hour`; None at a pole.
    pub fn time_for_hour(&self, up: DVec3, hour: f64, from: f64) -> Option<f64> {
        let now = self.hour_angle_at(up, from)?;
        Some(from + (hour_angle(hour) - now).rem_euclid(TAU) / self.omega())
    }

    /// First clock time at or after `from` when the sun stands at `elevation_deg` over `up`,
    /// rising (`evening` false) or setting (`evening` true). None when it never gets there
    /// (polar day or night) or at a pole.
    pub fn time_for_elevation(&self, up: DVec3, elevation_deg: f64, evening: bool, from: f64) -> Option<f64> {
        let up = up.normalize();
        let lat = self.axis().dot(up).clamp(-1.0, 1.0).asin();
        let d = self.declination_deg.to_radians();
        let c = (elevation_deg.to_radians().sin() - d.sin() * lat.sin()) / (d.cos() * lat.cos());
        if !(-1.0..=1.0).contains(&c) {
            return None;
        }
        let h = c.acos() * if evening { 1.0 } else { -1.0 };
        self.time_for_hour(up, 12.0 + h / TAU * 24.0, from)
    }
}

/// Hour angle (rad) of a local hour: 0 at noon.
fn hour_angle(hour: f64) -> f64 {
    (hour - 12.0) / 24.0 * TAU
}

/// A light archetype: keys sorted by sun elevation, interpolated linearly, clamped at the ends.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Look {
    #[serde(rename = "_todo", default)]
    pub todo: Option<String>,
    pub keys: Vec<LightKey>,
}

impl Look {
    fn validate(&self) -> Result<(), String> {
        if self.keys.is_empty() {
            return Err("needs at least one key".into());
        }
        for w in self.keys.windows(2) {
            if !(w[0].elevation_deg < w[1].elevation_deg) {
                return Err(format!("keys must be sorted by elevation_deg ({} before {})", w[0].elevation_deg, w[1].elevation_deg));
            }
        }
        for k in &self.keys {
            if k.sun_lux < 0.0 || k.night_lux < 0.0 || k.ambient < 0.0 || k.fog_density < 0.0 {
                return Err(format!("key at {} deg: lux, ambient and fog_density must be >= 0", k.elevation_deg));
            }
        }
        Ok(())
    }

    /// The light at sun elevation `elevation_deg` (the result's `elevation_deg` is the input).
    pub fn sample(&self, elevation_deg: f64) -> LightKey {
        let k = &self.keys;
        let i = k.partition_point(|x| x.elevation_deg <= elevation_deg);
        let mut out = if i == 0 {
            k[0].clone()
        } else if i == k.len() {
            k[i - 1].clone()
        } else {
            let (a, b) = (&k[i - 1], &k[i]);
            a.lerp(b, (elevation_deg - a.elevation_deg) / (b.elevation_deg - a.elevation_deg))
        };
        out.elevation_deg = elevation_deg;
        out
    }
}

/// The light at one sun elevation.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LightKey {
    pub elevation_deg: f64,
    /// Sun illuminance (lux) of the directional light; 0 below the horizon.
    pub sun_lux: f64,
    pub sun_color: [f64; 3],
    /// Night light: a weak directional light from the antisolar direction.
    pub night_lux: f64,
    pub night_color: [f64; 3],
    /// Ambient brightness (scaled by air density in the app) and colour.
    pub ambient: f64,
    pub ambient_color: [f64; 3],
    /// Colour behind the atmosphere (night-sky glow; space at day).
    pub sky_color: [f64; 3],
    /// Multiply the recipe's `haze_color` and `haze_density`.
    pub fog_tint: [f64; 3],
    pub fog_density: f64,
}

fn lerp3(a: [f64; 3], b: [f64; 3], f: f64) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f)
}

impl LightKey {
    fn lerp(&self, b: &LightKey, f: f64) -> LightKey {
        let l = |x: f64, y: f64| x + (y - x) * f;
        LightKey {
            elevation_deg: l(self.elevation_deg, b.elevation_deg),
            sun_lux: l(self.sun_lux, b.sun_lux),
            sun_color: lerp3(self.sun_color, b.sun_color, f),
            night_lux: l(self.night_lux, b.night_lux),
            night_color: lerp3(self.night_color, b.night_color, f),
            ambient: l(self.ambient, b.ambient),
            ambient_color: lerp3(self.ambient_color, b.ambient_color, f),
            sky_color: lerp3(self.sky_color, b.sky_color, f),
            fog_tint: lerp3(self.fog_tint, b.fog_tint, f),
            fog_density: l(self.fog_density, b.fog_density),
        }
    }

    /// Light falling on flat ground (lux-like, for checks): sun and night light by the sine of
    /// their elevation, plus ambient.
    pub fn ground_brightness(&self) -> f64 {
        let s = self.elevation_deg.to_radians().sin();
        self.sun_lux * s.max(0.0) + self.night_lux * (-s).max(0.0) + self.ambient
    }
}

#[cfg(test)]
mod tests;
