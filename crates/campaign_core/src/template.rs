use crate::set::PlanetSet;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SystemKind {
    Harbour,
    Normal,
    Hostile,
}

impl SystemKind {
    pub const ALL: [SystemKind; 3] = [SystemKind::Harbour, SystemKind::Normal, SystemKind::Hostile];
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum EnemyKind {
    Ship,
    Station,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Template {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    pub kind: SystemKind,
    pub star: StarRule,
    pub planets: PlanetRule,
    pub stations: StationRule,
    pub sites: SiteRule,
    #[serde(default)]
    pub enemy: Option<EnemyRule>,
    pub events: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct StarRule {
    pub distance_m: [f64; 2],
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PlanetRule {
    pub count: [u32; 2],
    pub recipes: Vec<String>,
    pub radius_m: [f64; 2],
    pub atmosphere_height_m: [f64; 2],
    pub gap_m: [f64; 2],
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct StationRule {
    pub count: [u32; 2],
    pub altitude_m: [f64; 2],
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct SiteRule {
    pub count: [u32; 2],
    pub kinds: Vec<String>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct EnemyRule {
    pub kinds: Vec<EnemyKind>,
    pub altitude_m: [f64; 2],
}

fn ordered(name: &str, r: [f64; 2]) -> Result<(), String> {
    if !(r[0].is_finite() && r[1].is_finite()) {
        return Err(format!("{name}: not finite"));
    }
    if r[0] > r[1] {
        return Err(format!("{name}: {} > {}", r[0], r[1]));
    }
    Ok(())
}

fn ordered_count(name: &str, r: [u32; 2]) -> Result<(), String> {
    if r[0] > r[1] {
        return Err(format!("{name}: {} > {}", r[0], r[1]));
    }
    Ok(())
}

fn unique(name: &str, items: &[impl PartialEq + std::fmt::Debug]) -> Result<(), String> {
    for (i, item) in items.iter().enumerate() {
        if items[..i].contains(item) {
            return Err(format!("{name}: duplicate {item:?}"));
        }
    }
    Ok(())
}

impl Template {
    pub fn from_json(json: &str) -> Result<Template, String> {
        let t: Template = serde_json::from_str(json).map_err(|e| e.to_string())?;
        t.validate()?;
        Ok(t)
    }

    fn validate(&self) -> Result<(), String> {
        ordered("star.distance_m", self.star.distance_m)?;
        if self.star.distance_m[0] <= 0.0 {
            return Err("star.distance_m: must be > 0".into());
        }

        ordered_count("planets.count", self.planets.count)?;
        if self.planets.count[0] < 1 {
            return Err("planets.count: lo must be >= 1".into());
        }
        if self.planets.count[1] > 8 {
            return Err(format!("planets.count: {} > 8", self.planets.count[1]));
        }
        if self.planets.recipes.is_empty() {
            return Err("planets.recipes: empty".into());
        }
        unique("planets.recipes", &self.planets.recipes)?;
        ordered("planets.radius_m", self.planets.radius_m)?;
        if self.planets.radius_m[0] <= 0.0 {
            return Err("planets.radius_m: lo must be > 0".into());
        }
        ordered("planets.atmosphere_height_m", self.planets.atmosphere_height_m)?;
        if self.planets.atmosphere_height_m[0] < 0.0 {
            return Err("planets.atmosphere_height_m: lo must be >= 0".into());
        }
        ordered("planets.gap_m", self.planets.gap_m)?;
        if self.planets.gap_m[0] <= 0.0 {
            return Err("planets.gap_m: lo must be > 0".into());
        }

        ordered_count("stations.count", self.stations.count)?;
        if self.stations.count[1] > 1 {
            return Err(format!("stations.count: {} > 1", self.stations.count[1]));
        }
        ordered("stations.altitude_m", self.stations.altitude_m)?;
        if self.stations.altitude_m[0] <= 0.0 {
            return Err("stations.altitude_m: lo must be > 0".into());
        }

        ordered_count("sites.count", self.sites.count)?;
        if self.sites.count[1] > 0 && self.sites.kinds.is_empty() {
            return Err("sites.kinds: empty, but sites.count allows sites".into());
        }

        if let Some(enemy) = &self.enemy {
            if enemy.kinds.is_empty() {
                return Err("enemy.kinds: empty".into());
            }
            unique("enemy.kinds", &enemy.kinds)?;
            ordered("enemy.altitude_m", enemy.altitude_m)?;
            if enemy.altitude_m[0] <= 0.0 {
                return Err("enemy.altitude_m: lo must be > 0".into());
            }
        }

        match self.kind {
            SystemKind::Harbour => {
                if self.stations.count[0] != 1 {
                    return Err("stations.count: a harbour needs lo == 1".into());
                }
                if self.enemy.is_some() {
                    return Err("enemy: a harbour has none".into());
                }
            }
            SystemKind::Hostile => {
                if self.enemy.is_none() {
                    return Err("enemy: a hostile system needs one".into());
                }
            }
            SystemKind::Normal => {}
        }
        Ok(())
    }

    /// Every recipe id of the template is in the set.
    pub fn check_set(&self, set: &PlanetSet) -> Result<(), String> {
        for id in &self.planets.recipes {
            if set.get(id).is_none() {
                return Err(format!("planets.recipes: unknown recipe {id}"));
            }
        }
        Ok(())
    }
}
