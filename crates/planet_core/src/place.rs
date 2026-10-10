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
    /// Latitude: 90 is the planet's +y pole (where the game starts). Either `lat_deg` and
    /// `lon_deg`, or `near`.
    #[serde(default)]
    pub lat_deg: Option<f64>,
    #[serde(default)]
    pub lon_deg: Option<f64>,
    /// Metres from another place instead of a latitude and longitude, so the distance between
    /// the two stays the same when the planet's radius changes (#170, #177).
    #[serde(default)]
    pub near: Option<Near>,
    #[serde(default)]
    pub turn_deg: f64,
    /// The ground under it, centred on the place; levels to the ground at its centre.
    #[serde(default)]
    pub edits: Vec<Edit>,
    #[serde(default)]
    pub pads: Vec<PadSpec>,
}

/// A position in metres east and north of another, absolute place (east and north as on the
/// ground there; not turned by that place's `turn_deg`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Near {
    pub place: String,
    pub east_m: f64,
    pub north_m: f64,
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
    ///
    /// Panics for a place given by `near` before `Planet::set_places` has resolved it.
    pub fn dir(&self) -> V3 {
        let (lat, lon) = (self.lat_deg.expect("a place is resolved by set_places").to_radians(), self.lon_deg.expect("a place is resolved by set_places").to_radians());
        v3(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin()).normalized()
    }

    /// Checks the fields that serde cannot; errors name the field.
    pub fn check(&self) -> Result<(), String> {
        match (self.lat_deg, self.lon_deg, &self.near) {
            (Some(lat), Some(_), None) if !(-90.0..=90.0).contains(&lat) => return Err(format!("{}: lat_deg must be in -90..90", self.id)),
            (Some(_), Some(_), None) | (None, None, Some(_)) => {}
            (None, None, None) => return Err(format!("{}: needs lat_deg and lon_deg, or near", self.id)),
            _ => return Err(format!("{}: give either lat_deg and lon_deg, or near (not both, not half)", self.id)),
        }
        if let Some(n) = &self.near
            && !(n.east_m.is_finite() && n.north_m.is_finite())
        {
            return Err(format!("{}: near: east_m and north_m must be numbers", self.id));
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
    pub fn set_places(&mut self, mut places: Vec<Place>) -> Result<(), String> {
        for p in &places {
            p.check()?;
        }
        // Places given by `near` get their latitude and longitude from the reference, in metres on
        // this planet's radius. The reference must be an absolute place of this list.
        let absolute: Vec<(String, V3)> = places.iter().filter(|p| p.near.is_none()).map(|p| (p.id.clone(), p.dir())).collect();
        for p in places.iter_mut() {
            let Some(n) = p.near.clone() else { continue };
            let Some((_, d0)) = absolute.iter().find(|(id, _)| *id == n.place) else {
                return Err(format!("{}: near: '{}' is no such place, or is itself given by near: the reference must be an absolute place", p.id, n.place));
            };
            let (east, north) = crate::look::tangent_frame(*d0);
            let m = (n.east_m * n.east_m + n.north_m * n.north_m).sqrt();
            let d = if m > 0.0 { crate::look::walk(*d0, (east * n.east_m + north * n.north_m) * (1.0 / m), m, self.radius) } else { *d0 };
            p.lat_deg = Some(d.y.clamp(-1.0, 1.0).asin().to_degrees());
            p.lon_deg = Some(d.z.atan2(d.x).to_degrees());
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
