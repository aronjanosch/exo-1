//! Hand-placed places (pads now, the city and outposts later): a fixed spot on the planet with the
//! ground edits under it and its pads, one JSON file each in `content/place/` (DECISIONS.md,
//! "Places (hand-placed)"). The bake puts them before the generated sites, which keep away.
//!
//! Place-local frame: metres on the tangent plane, x and y as in `art/city/city_plan.py`, z up.
//! `turn_deg` turns it about up, counter-clockwise seen from above; at 0 its +x points east.
use serde::Deserialize;

use crate::look::tangent_frame;
use crate::math::*;
use crate::recipe::{Edit, SiteCategory};
use crate::site::{Site, edit_rt};

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub id: String,
    /// The planet recipe's name (`hearth`).
    pub planet: String,
    /// Latitude: 90 is the planet's +y pole (where the game starts).
    pub lat_deg: f64,
    pub lon_deg: f64,
    #[serde(default)]
    pub turn_deg: f64,
    /// The ground under it, centred on the place; levels to the ground at its centre.
    #[serde(default)]
    pub edits: Vec<Edit>,
    #[serde(default)]
    pub pads: Vec<PadSpec>,
}

/// A landing pad of a place.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PadSpec {
    pub id: String,
    /// Centre in the place's frame (x, y), metres.
    pub at: [f64; 2],
    pub radius_m: f64,
}

/// A pad on the planet: its centre on the ground (planet space), up and radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadAt {
    pub centre: V3,
    pub up: V3,
    pub radius_m: f64,
}

impl Place {
    /// Unit direction of the place's centre from the planet centre.
    pub fn dir(&self) -> V3 {
        let (lat, lon) = (self.lat_deg.to_radians(), self.lon_deg.to_radians());
        v3(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin()).normalized()
    }

    /// Checks the fields that serde cannot; errors name the field.
    pub fn check(&self) -> Result<(), String> {
        if !(-90.0..=90.0).contains(&self.lat_deg) {
            return Err(format!("{}: lat_deg must be in -90..90", self.id));
        }
        for p in &self.pads {
            if !(p.radius_m > 0.0) {
                return Err(format!("{}: pads.radius_m: '{}' must be positive", self.id, p.id));
            }
        }
        let mut ids: Vec<&str> = self.pads.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        if ids.windows(2).any(|w| w[0] == w[1]) {
            return Err(format!("{}: pads.id: used twice", self.id));
        }
        Ok(())
    }

    /// The place as a site of the bake: fixed direction, its edits, no kit. `ground_m` is the
    /// noise ground at its centre.
    pub(crate) fn site(&self, ground_m: f64, radius: f64) -> Site {
        let d = self.dir();
        let yaw = self.turn_deg.to_radians();
        let (e0, n0) = tangent_frame(d);
        let east = e0 * yaw.cos() + n0 * yaw.sin();
        let north = d.cross(east);
        let mut edits: Vec<&Edit> = self.edits.iter().collect();
        edits.sort_by_key(|e| e.order());
        let (rts, reaches): (Vec<_>, Vec<f64>) = edits.into_iter().map(edit_rt).unzip();
        let pads = self.pads.iter().map(|p| p.at[0].hypot(p.at[1]) + p.radius_m).fold(0.0, f64::max);
        let reach = reaches.iter().copied().fold(pads, f64::max);
        Site {
            kind: None,
            id: self.id.clone(),
            category: SiteCategory::Place,
            dir: d,
            yaw,
            footprint_m: reach,
            ground_m,
            reach_m: reach,
            cos_reach: (reach / radius).cos(),
            east,
            north,
            edits: std::sync::Arc::new(rts),
        }
    }
}

impl crate::planet::Planet {
    /// Sets the hand-placed places of this planet; call before `bake`. Checks each one.
    pub fn set_places(&mut self, places: Vec<Place>) -> Result<(), String> {
        for p in &places {
            p.check()?;
        }
        self.places = places;
        Ok(())
    }

    /// A pad of a placed place, on the edited ground. None if the place or the pad is unknown.
    pub fn pad(&self, place: &str, pad: &str) -> Option<PadAt> {
        let s = self.sites.iter().find(|s| s.category == SiteCategory::Place && s.id == place)?;
        let p = self.places.iter().find(|p| p.id == place)?.pads.iter().find(|p| p.id == pad)?;
        let a = p.at[0].hypot(p.at[1]);
        let dir = if a > 0.0 { crate::look::walk(s.dir, (s.east * p.at[0] + s.north * p.at[1]) * (1.0 / a), a, self.radius) } else { s.dir };
        Some(PadAt { centre: dir * (self.radius + self.height_at(dir)), up: dir, radius_m: p.radius_m })
    }
}
