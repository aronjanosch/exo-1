//! Debug test to see what's happening with density placement
use planet_core::*;

const HEARTH: &str = include_str!("../../../content/planet/hearth.json");

fn recipe(text: &str, seed: i32, radius: f64) -> Recipe {
    Recipe::for_planet(&text.replace("\"resolution\": 512", "\"resolution\": 128"), seed, radius).unwrap()
}

#[test]
fn debug_recipe_and_placement() {
    let r = recipe(HEARTH, 1337, 5000.0);
    println!("\nRecipe loaded, radius=5000");
    println!("Landforms:");
    for k in &r.landforms.kinds {
        println!("  {}: count={:?}, per_100_km2={:?}", k.id, k.count, k.per_100_km2);
    }
    println!("Sites:");
    for k in &r.sites.kinds {
        println!("  {}: count={:?}, per_100_km2={:?}", k.id, k.count, k.per_100_km2);
    }

    let mut p = Planet::new(r);
    println!("\nPlanet created");
    println!("Stamps: {}", p.stamps().len());
    println!("Sites: {}", p.sites.len());
    println!("Placement error: {:?}", p.placement_error());

    let _ = p.bake_with(0, None, false);
    println!("After bake:");
    println!("Stamps: {}", p.stamps().len());
    println!("Sites: {}", p.sites.len());
}
