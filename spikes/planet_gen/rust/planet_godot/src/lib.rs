//! GDExtension class `PlanetGen`: thin conversion layer over planet_core.
use godot::prelude::*;
use planet_core::{Planet, Recipe, V3};
use std::sync::Arc;
use std::time::Instant;

struct PlanetGenExt;

#[gdextension]
unsafe impl ExtensionLibrary for PlanetGenExt {}

fn to_v3(v: Vector3) -> V3 {
    planet_core::v3(v.x as f64, v.y as f64, v.z as f64)
}
fn from_v3(v: V3) -> Vector3 {
    Vector3::new(v.x as f32, v.y as f32, v.z as f32)
}

fn json_to_variant(v: &serde_json::Value) -> Variant {
    match v {
        serde_json::Value::Null => Variant::nil(),
        serde_json::Value::Bool(b) => b.to_variant(),
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0).to_variant(),
        serde_json::Value::String(s) => GString::from(s.as_str()).to_variant(),
        serde_json::Value::Array(a) => {
            let mut arr = VarArray::new();
            for x in a {
                arr.push(&json_to_variant(x));
            }
            arr.to_variant()
        }
        serde_json::Value::Object(o) => {
            let mut d = Dictionary::<GString, Variant>::new();
            for (k, x) in o {
                d.set(k.as_str(), &json_to_variant(x));
            }
            d.to_variant()
        }
    }
}

#[derive(GodotClass)]
#[class(base = RefCounted, init)]
struct PlanetGen {
    /// Written only by load_recipe/bake (main thread); everything else only reads it.
    planet: Option<Arc<Planet>>,
    base: Base<RefCounted>,
}

#[godot_api]
impl PlanetGen {
    /// `seed_override` < 0 keeps the recipe's seed, `radius_override` <= 0 its radius.
    #[func]
    fn load_recipe(&mut self, json: GString, seed_override: i64, radius_override: f64) -> bool {
        match Recipe::from_json(&json.to_string()) {
            Ok(mut r) => {
                if seed_override >= 0 {
                    r.seed = seed_override as i32;
                }
                if radius_override > 0.0 {
                    r.radius = radius_override;
                }
                self.planet = Some(Arc::new(Planet::new(r)));
                true
            }
            Err(e) => {
                godot_error!("PlanetGen.load_recipe: {}", e);
                false
            }
        }
    }

    /// Bakes the macro shell, sea level, sites. Returns the statistics (T3). threads 0 = all cores.
    #[func]
    fn bake(&mut self, threads: i64) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::<GString, Variant>::new();
        let Some(arc) = self.planet.as_mut() else {
            godot_error!("PlanetGen.bake: load_recipe first");
            return out;
        };
        let Some(p) = Arc::get_mut(arc) else {
            godot_error!("PlanetGen.bake: chunks still in flight");
            return out;
        };
        let st = p.bake(threads.max(0) as usize);
        if let Ok(serde_json::Value::Object(o)) = serde_json::to_value(&st) {
            for (k, v) in o {
                out.set(k.as_str(), &json_to_variant(&v));
            }
        }
        out
    }

    fn p(&self) -> &Planet {
        let p = self.planet.as_ref().expect("load_recipe() and bake() first");
        assert!(p.baked, "bake() first");
        p
    }

    #[func]
    fn radius(&self) -> f64 {
        self.planet.as_ref().map(|p| p.radius).unwrap_or(0.0)
    }

    /// Sea level in metres above the base radius.
    #[func]
    fn sea_level(&self) -> f64 {
        self.p().sea
    }

    /// Lowest and highest crust height above the base radius over the whole planet (sampled at bake).
    #[func]
    fn height_range(&self) -> Vector2 {
        let p = self.p();
        let (lo, hi) = p.height_range;
        Vector2::new(lo as f32, hi as f32)
    }

    /// Metres above the base radius.
    #[func]
    fn height_at(&self, dir: Vector3) -> f64 {
        self.p().height_at(to_v3(dir))
    }

    /// Same as height_at, but the direction arrives as three doubles (a Vector3 is 32 bit, which
    /// is 0.3 mm of lateral position at R = 5 km). For tests.
    #[func]
    fn height_at_xyz(&self, x: f64, y: f64, z: f64) -> f64 {
        self.p().height_at(planet_core::v3(x, y, z))
    }

    #[func]
    fn heights(&self, dirs: PackedVector3Array) -> PackedFloat32Array {
        let p = self.p();
        dirs.as_slice().iter().map(|d| p.height_at(to_v3(*d)) as f32).collect()
    }

    /// Heights for a collision patch (tangent frame, see planet_core::Planet::patch_heights).
    #[func]
    fn patch_heights(&self, up: Vector3, t: Vector3, b: Vector3, n: i64) -> PackedFloat32Array {
        PackedFloat32Array::from(self.p().patch_heights(to_v3(up), to_v3(t), to_v3(b), n as usize).as_slice())
    }

    #[func]
    fn sample(&self, dir: Vector3) -> Dictionary<GString, Variant> {
        let s = self.p().sample(to_v3(dir));
        let mut d = Dictionary::<GString, Variant>::new();
        if let Ok(serde_json::Value::Object(o)) = serde_json::to_value(&s) {
            for (k, v) in o {
                d.set(k.as_str(), &json_to_variant(&v));
            }
        }
        d
    }

    #[func]
    fn biome_colors(&self) -> Dictionary<i64, Color> {
        let p = self.p();
        let mut d = Dictionary::<i64, Color>::new();
        for b in &p.recipe.biomes {
            d.set(b.id as i64, Color::from_rgb(b.color[0], b.color[1], b.color[2]));
        }
        d
    }

    #[func]
    fn sites(&self) -> PackedVector3Array {
        self.p().sites.iter().map(|s| from_v3(*s)).collect()
    }

    #[func]
    fn sites_near(&self, dir: Vector3, radius_m: f64) -> PackedVector3Array {
        self.p().sites_near(to_v3(dir), radius_m).iter().map(|s| from_v3(*s)).collect()
    }

    /// &self: shared borrows may overlap, so worker threads can call this in parallel.
    /// Arrays as in spike 1 (vertex, normal, colour, uv; indices are built by the caller once),
    /// plus `center`, `heights`, `biomes`, `min_h`, `max_h`, `usec`, and `scatter`:
    /// kind -> { buffer: PackedFloat32Array (MultiMesh layout, 16 floats per instance), count }.
    #[func]
    fn build_chunk(&self, face: i64, a0: f64, b0: f64, size: f64, with_scatter: bool) -> Dictionary<GString, Variant> {
        let t0 = Instant::now();
        let c = self.p().build_chunk(face as usize, a0, b0, size, with_scatter);
        let verts: PackedVector3Array = c.verts.iter().map(|v| Vector3::new(v[0], v[1], v[2])).collect();
        let normals: PackedVector3Array = c.normals.iter().map(|v| Vector3::new(v[0], v[1], v[2])).collect();
        let colors: PackedColorArray = c.colors.iter().map(|v| Color::from_rgba(v[0], v[1], v[2], v[3])).collect();
        let uvs: PackedVector2Array = c.uvs.iter().map(|v| Vector2::new(v[0], v[1])).collect();
        let mut d = Dictionary::<GString, Variant>::new();
        d.set("verts", &verts.to_variant());
        d.set("normals", &normals.to_variant());
        d.set("colors", &colors.to_variant());
        d.set("uvs", &uvs.to_variant());
        d.set("heights", &PackedFloat32Array::from(c.heights.as_slice()).to_variant());
        d.set("biomes", &PackedByteArray::from(c.biomes.as_slice()).to_variant());
        d.set("center", &Vector3::new(c.center[0] as f32, c.center[1] as f32, c.center[2] as f32).to_variant());
        d.set("min_h", &c.min_h.to_variant());
        d.set("max_h", &c.max_h.to_variant());
        let mut sc = Dictionary::<GString, Variant>::new();
        for s in &c.scatter {
            let mut e = Dictionary::<GString, Variant>::new();
            e.set("count", &((s.buffer.len() / 16) as i64).to_variant());
            e.set("buffer", &PackedFloat32Array::from(s.buffer.as_slice()).to_variant());
            sc.set(s.kind.as_str(), &e.to_variant());
        }
        d.set("scatter", &sc.to_variant());
        d.set("usec", &(t0.elapsed().as_micros() as i64).to_variant());
        d
    }
}
