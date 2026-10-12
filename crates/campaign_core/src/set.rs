use serde::Deserialize;

/// One recipe of the planet set: what it fixes for every planet made from it.
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct RecipeInfo {
    pub id: String,
    pub color: [f32; 3],
    pub terrain_m: f64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct PlanetSet {
    #[serde(rename = "_comment", default)]
    pub comment: Option<String>,
    pub recipes: Vec<RecipeInfo>,
}

impl PlanetSet {
    pub fn from_json(json: &str) -> Result<PlanetSet, String> {
        let set: PlanetSet = serde_json::from_str(json).map_err(|e| e.to_string())?;
        set.validate()?;
        Ok(set)
    }

    fn validate(&self) -> Result<(), String> {
        if self.recipes.is_empty() {
            return Err("recipes: empty".into());
        }
        for (i, r) in self.recipes.iter().enumerate() {
            if r.id.is_empty() {
                return Err(format!("recipes[{i}].id: empty"));
            }
            if self.recipes[..i].iter().any(|o| o.id == r.id) {
                return Err(format!("recipes[{i}].id: duplicate {}", r.id));
            }
            if !(r.terrain_m.is_finite() && r.terrain_m >= 0.0) {
                return Err(format!("recipes[{i}].terrain_m: must be finite and >= 0"));
            }
            if !r.color.iter().all(|c| (0.0..=1.0).contains(c)) {
                return Err(format!("recipes[{i}].color: components must be in [0, 1]"));
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&RecipeInfo> {
        self.recipes.iter().find(|r| r.id == id)
    }
}
