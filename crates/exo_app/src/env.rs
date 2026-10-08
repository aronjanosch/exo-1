//! The planet as the simulation sees it: generator, centre (world, f64) and the field model.
use bevy::math::DVec3;
use bevy::prelude::*;
use flight_core::{Field, PlanetEnv};
use planet_core::{BakeStats, Planet, Recipe, V3};
use std::sync::Arc;
use warp_core::{PlanetDef, PlanetId};

/// Every planet recipe, embedded at build time: `content/planet/<id>.json` (#64).
pub const RECIPES: &[(&str, &str)] = &[
    ("hearth", include_str!("../../../content/planet/hearth.json")),
    ("cinder", include_str!("../../../content/planet/cinder.json")),
];

/// The recipe a planet of the system names, with the planet's seed and radius.
pub fn recipe_for(def: &PlanetDef) -> Result<Recipe, String> {
    let text = RECIPES.iter().find(|(id, _)| *id == def.recipe).map(|(_, t)| *t).ok_or_else(|| format!("planet {}: no recipe '{}' in content/planet", def.name, def.recipe))?;
    Recipe::for_planet(text, def.seed, def.radius).map_err(|e| format!("content/planet/{}.json: {e}", def.recipe))
}

pub fn to_v3(d: DVec3) -> V3 {
    planet_core::v3(d.x, d.y, d.z)
}
pub fn from_v3(v: V3) -> DVec3 {
    DVec3::new(v.x, v.y, v.z)
}

#[derive(Resource, Clone)]
pub struct PlanetRes {
    pub pgen: Arc<Planet>,
    /// World position of the centre (f64, never shifted: physics runs in f64 world space).
    pub centre: DVec3,
    pub radius: f64,
    pub sea: f64,
    /// Largest |height| over the planet, for chunk bounds.
    pub relief: f64,
    pub field: Field,
    pub bake_ms: f64,
    /// The planet in the system registry.
    pub id: PlanetId,
}

impl PlanetRes {
    /// A planet of the registry: the recipe with its seed and radius, at its centre, with its
    /// atmosphere height.
    pub fn load(id: PlanetId, def: &PlanetDef) -> PlanetRes {
        Self::load_with_stats(id, def).0
    }
    /// `load`, with the bake's statistics.
    pub fn load_with_stats(id: PlanetId, def: &PlanetDef) -> (PlanetRes, BakeStats) {
        let recipe = recipe_for(def).unwrap_or_else(|e| panic!("{e}"));
        let mut p = Planet::new(recipe);
        let st = p.bake_checked(0).unwrap_or_else(|e| panic!("content/planet/{}.json: {e}", def.recipe));
        let (lo, hi) = p.height_range;
        let res = PlanetRes {
            radius: p.radius,
            sea: p.sea,
            relief: lo.abs().max(hi.abs()),
            pgen: Arc::new(p),
            centre: def.centre(),
            field: Field { atmosphere_height: def.atmosphere_height, ..Field::default() },
            bake_ms: st.bake_ms,
            id,
        };
        (res, st)
    }
    /// Distance from the centre to the ground along a direction.
    pub fn surface(&self, dir: DVec3) -> f64 {
        self.radius + self.pgen.height_at(to_v3(dir.normalize()))
    }
    pub fn up(&self, world: DVec3) -> DVec3 {
        (world - self.centre).normalize()
    }
    /// Height of a world point above the ground below it.
    pub fn above_ground(&self, world: DVec3) -> f64 {
        let p = world - self.centre;
        p.length() - self.surface(p)
    }
}

impl PlanetEnv for PlanetRes {
    fn to_planet(&self, world: DVec3) -> DVec3 {
        world - self.centre
    }
    fn radius(&self) -> f64 {
        self.radius
    }
    fn height_at(&self, dir: DVec3) -> f64 {
        self.pgen.height_at(to_v3(dir.normalize()))
    }
    fn field(&self) -> &Field {
        &self.field
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_planet_of_the_system_names_a_recipe_that_loads() {
        let sys = warp_core::System::from_json(crate::warp::SYSTEM).unwrap();
        for def in &sys.planets {
            let r = super::recipe_for(def).unwrap();
            assert_eq!((r.seed, r.radius), (def.seed, def.radius), "{}", def.name);
        }
        let mut missing = sys.planets[0].clone();
        missing.recipe = "nowhere".into();
        assert!(super::recipe_for(&missing).unwrap_err().contains("no recipe 'nowhere'"));
    }
}
