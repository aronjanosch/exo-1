//! daynight_core: time of day on static planets (#48, #104), without Bevy types, in f64. There is
//! one star per system. A planet stays still; its day comes from its own spin: the direction
//! from the planet to the star turns around the planet's axis once per `day_length_s`. Declination
//! and the hour at the spawn point follow from the geometry (axis and star position), they are not
//! configured. Far from every planet the light comes from the star's true direction (one light, all
//! planets lit consistently); near a planet it blends to that planet's rotating one. The light
//! (sun, night light, ambient, sky and fog) comes from keys sorted by the sun's elevation at the
//! viewer, shared by planets through light archetypes ("looks").
//!
//! All values in `content/daynight/daynight.json` are placeholders (TODO(initiator)).
use glam::{DQuat, DVec3};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::f64::consts::TAU;

/// Planet-space direction of the spawn point; its local hour at clock 0 follows from the geometry.
pub const SPAWN_UP: DVec3 = DVec3::Y;

/// The whole file: light archetypes and the sun of every planet (by recipe id).
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct DayNight {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    /// Distances at which the light leaves a planet's rotating day for the star's true direction.
    #[serde(default)]
    pub space_blend: SpaceBlend,
    pub looks: BTreeMap<String, Look>,
    pub planets: BTreeMap<String, PlanetSun>,
}

/// From `near_m` to `far_m` away from the planet's centre the light direction slides (smoothstep)
/// from the planet's rotating sun to the true direction of the star. TODO(initiator): placeholder.
#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(deny_unknown_fields)]
pub struct SpaceBlend {
    pub near_m: f64,
    pub far_m: f64,
}

impl Default for SpaceBlend {
    fn default() -> Self {
        SpaceBlend { near_m: 30_000.0, far_m: 300_000.0 }
    }
}

impl SpaceBlend {
    fn validate(&self) -> Result<(), String> {
        if !(self.near_m.is_finite() && self.far_m.is_finite() && 0.0 <= self.near_m && self.near_m < self.far_m) {
            return Err(format!("space_blend: needs 0 <= near_m < far_m, is {} and {}", self.near_m, self.far_m));
        }
        Ok(())
    }

    /// `local` (the planet's sun) at `dist` <= near_m, `star` (true direction) at >= far_m.
    pub fn mix(&self, local: DVec3, star: DVec3, dist: f64) -> DVec3 {
        let x = ((dist - self.near_m) / (self.far_m - self.near_m)).clamp(0.0, 1.0);
        let f = x * x * (3.0 - 2.0 * x);
        if f <= 0.0 {
            return local;
        }
        if f >= 1.0 {
            return star;
        }
        (DQuat::IDENTITY.slerp(DQuat::from_rotation_arc(local, star), f) * local).normalize()
    }
}

/// Unit direction from a planet's centre to the star.
pub fn to_star(star: DVec3, centre: DVec3) -> DVec3 {
    (star - centre).try_normalize().unwrap_or(DVec3::Z)
}

impl DayNight {
    pub fn from_json(s: &str) -> Result<Self, String> {
        let d: DayNight = serde_json::from_str(s).map_err(|e| format!("daynight.json: {e}"))?;
        d.validate()?;
        Ok(d)
    }

    fn validate(&self) -> Result<(), String> {
        self.space_blend.validate().map_err(|e| format!("daynight.json: {e}"))?;
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

    /// Checks a planet against the star: both finite, the star not at the centre, the axis not
    /// pointing at the star (the sun would never move across the sky).
    pub fn check_geometry(&self, recipe: &str, centre: DVec3, star: DVec3) -> Result<(), String> {
        let (p, _) = self.planet(recipe)?;
        if !(centre.is_finite() && star.is_finite()) {
            return Err(format!("planet '{recipe}': centre and star position must be finite"));
        }
        if star.distance(centre) < 1.0 {
            return Err(format!("planet '{recipe}': the star sits at the planet's centre"));
        }
        let decl = p.declination_deg(to_star(star, centre));
        if decl.abs() > 89.5 {
            return Err(format!("planet '{recipe}': the axis points at the star (declination {decl:.1} deg); tilt it"));
        }
        Ok(())
    }
}

/// The spin of one planet. With the direction to the star it gives the sun's path.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PlanetSun {
    #[serde(rename = "_todo", default)]
    pub todo: Option<String>,
    /// One full turn of the planet against the star (s).
    pub day_length_s: f64,
    /// Spin axis in planet space (normalised on use). The sun turns around it. Its angle to the
    /// star sets the season (declination).
    pub axis: [f64; 3],
    /// Light archetype (a key of `looks`).
    pub look: String,
}

impl PlanetSun {
    fn validate(&self) -> Result<(), String> {
        if !(self.day_length_s.is_finite() && self.day_length_s > 0.0) {
            return Err(format!("day_length_s must be finite and > 0, is {}", self.day_length_s));
        }
        if !self.axis.iter().all(|x| x.is_finite()) || DVec3::from_array(self.axis).length() < 1e-9 {
            return Err(format!("axis must be finite and not zero, is {:?}", self.axis));
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

    /// The sun's angle above the equator (deg), from the geometry: axis against `to_star`.
    pub fn declination_deg(&self, to_star: DVec3) -> f64 {
        self.axis().dot(to_star.normalize()).clamp(-1.0, 1.0).asin().to_degrees()
    }

    /// Unit vector in the equator plane under `up` (its meridian), or None at a pole.
    fn meridian(&self, up: DVec3) -> Option<DVec3> {
        let a = self.axis();
        (up - a * a.dot(up)).try_normalize()
    }

    /// Direction towards the sun (planet space, unit) at clock `t` (s): the direction to the star
    /// turned around the axis. At clock 0 it is the star's own direction.
    pub fn sun_dir(&self, to_star: DVec3, t: f64) -> DVec3 {
        (DQuat::from_axis_angle(self.axis(), self.omega() * t) * to_star.normalize()).normalize()
    }

    /// The direction the world's one light takes for a viewer at `at` (world), for the planet at
    /// `centre` and the star at `star`: this planet's sun near it, the star's true direction far
    /// away (see `SpaceBlend`).
    pub fn light_dir(&self, blend: &SpaceBlend, star: DVec3, centre: DVec3, at: DVec3, t: f64) -> DVec3 {
        let local = self.sun_dir(to_star(star, centre), t);
        let dist = at.distance(centre);
        if dist <= blend.near_m {
            return local;
        }
        blend.mix(local, to_star(star, at), dist)
    }

    /// Hour angle (rad, 0 at noon, growing with time) of the sun seen from `up` at `t`.
    fn hour_angle_at(&self, up: DVec3, to_star: DVec3, t: f64) -> Option<f64> {
        let a = self.axis();
        let m = self.meridian(up)?;
        let s = self.sun_dir(to_star, t);
        let se = (s - a * a.dot(s)).try_normalize()?;
        Some(a.dot(m.cross(se)).atan2(m.dot(se)))
    }

    /// Local hour (0..24, 12 at noon) at the surface point with unit `up`; None at a pole.
    pub fn local_hour(&self, up: DVec3, to_star: DVec3, t: f64) -> Option<f64> {
        self.hour_angle_at(up, to_star, t).map(|h| (12.0 + h / TAU * 24.0).rem_euclid(24.0))
    }

    /// Sun elevation (deg) above the horizon of `up` at `t`.
    pub fn elevation_deg(&self, up: DVec3, to_star: DVec3, t: f64) -> f64 {
        self.sun_dir(to_star, t).dot(up.normalize()).clamp(-1.0, 1.0).asin().to_degrees()
    }

    /// First clock time at or after `from` when `up` has local hour `hour`; None at a pole.
    pub fn time_for_hour(&self, up: DVec3, to_star: DVec3, hour: f64, from: f64) -> Option<f64> {
        let now = self.hour_angle_at(up, to_star, from)?;
        Some(from + (hour_angle(hour) - now).rem_euclid(TAU) / self.omega())
    }

    /// First clock time at or after `from` when the sun stands at `elevation_deg` over `up`,
    /// rising (`evening` false) or setting (`evening` true). None when it never gets there
    /// (polar day or night) or at a pole.
    pub fn time_for_elevation(&self, up: DVec3, to_star: DVec3, elevation_deg: f64, evening: bool, from: f64) -> Option<f64> {
        let up = up.normalize();
        let lat = self.axis().dot(up).clamp(-1.0, 1.0).asin();
        let d = self.declination_deg(to_star).to_radians();
        let c = (elevation_deg.to_radians().sin() - d.sin() * lat.sin()) / (d.cos() * lat.cos());
        if !(-1.0..=1.0).contains(&c) {
            return None;
        }
        let h = c.acos() * if evening { 1.0 } else { -1.0 };
        self.time_for_hour(up, to_star, 12.0 + h / TAU * 24.0, from)
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
            if !k.elevation_deg.is_finite() {
                return Err("elevation_deg must be finite".into());
            }
            let nonneg = [k.sun_lux, k.night_lux, k.ambient, k.fog_density];
            if !nonneg.iter().all(|x| x.is_finite() && *x >= 0.0) {
                return Err(format!("key at {} deg: lux, ambient and fog_density must be finite and >= 0", k.elevation_deg));
            }
            let colors = [k.sun_color, k.night_color, k.ambient_color, k.sky_color, k.fog_tint];
            if !colors.iter().flatten().all(|x| x.is_finite() && *x >= 0.0) {
                return Err(format!("key at {} deg: colours must be finite and >= 0", k.elevation_deg));
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
