//! The planet as the simulation sees it: generator, centre (world, f64) and the field model.
use bevy::math::DVec3;
use bevy::prelude::*;
use flight_core::{Field, PlanetEnv};
use planet_core::{Planet, Recipe, V3};
use std::sync::Arc;

pub const RECIPE: &str = include_str!("../../../content/planet/recipe.json");

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
    /// Index in the system registry (`warp_core::System::planets`).
    pub id: usize,
}

impl PlanetRes {
    pub fn load(radius: f64, centre: DVec3) -> PlanetRes {
        Self::build(radius, centre, None)
    }

    /// A planet of the registry: its seed, radius and centre.
    pub fn load_def(id: usize, def: &warp_core::PlanetDef) -> PlanetRes {
        let mut p = Self::build(def.radius, def.centre(), Some(def.seed));
        p.id = id;
        p
    }

    fn build(radius: f64, centre: DVec3, seed: Option<i32>) -> PlanetRes {
        let mut recipe = Recipe::from_json(RECIPE).expect("recipe");
        if radius > 0.0 {
            recipe.radius = radius;
        }
        if let Some(seed) = seed {
            recipe.seed = seed;
        }
        let mut p = Planet::new(recipe);
        let st = p.bake(0);
        let (lo, hi) = p.height_range;
        PlanetRes {
            radius: p.radius,
            sea: p.sea,
            relief: lo.abs().max(hi.abs()),
            pgen: Arc::new(p),
            centre,
            field: Field::default(),
            bake_ms: st.bake_ms,
            id: 0,
        }
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
