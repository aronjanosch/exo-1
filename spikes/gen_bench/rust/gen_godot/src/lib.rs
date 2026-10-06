//! Variant B: GDExtension class `ExoGen` (see spikes/gen_bench/SPEC.md).
use gen_core::{Gen, M};
use godot::prelude::*;
use std::sync::Arc;
use std::time::Instant;

struct ExoGenExt;

#[gdextension]
unsafe impl ExtensionLibrary for ExoGenExt {}

#[derive(GodotClass)]
#[class(base = RefCounted, init)]
struct ExoGen {
    /// Written only by setup/bake (main thread); build_chunk only reads it.
    gen: Option<Arc<Gen>>,
    base: Base<RefCounted>,
}

#[godot_api]
impl ExoGen {
    #[func]
    fn setup(&mut self, seed: i64) {
        self.gen = Some(Arc::new(Gen::new(seed as i32)));
    }

    #[func]
    fn bake(&mut self) {
        self.bake_threads(1);
    }

    #[func]
    fn bake_threads(&mut self, n: i64) {
        let mut g = Gen::new(0);
        // reuse the noises of the current Gen
        if let Some(old) = self.gen.take() {
            match Arc::try_unwrap(old) {
                Ok(o) => g = o,
                Err(_) => godot_error!("ExoGen: bake while chunks are in flight"),
            }
        } else {
            godot_error!("ExoGen: call setup() first");
            return;
        }
        g.bake(n.max(1) as usize);
        self.gen = Some(Arc::new(g));
    }

    /// &self: shared borrows may overlap, so worker threads can call this in parallel.
    #[func]
    fn build_chunk(&self, face: i64, ix: i64, iy: i64) -> Dictionary<GString, Variant> {
        let t0 = Instant::now();
        let g = self.gen.as_ref().expect("setup() and bake() first");
        let c = g.build_chunk(face as usize, ix as usize, iy as usize);
        let verts: PackedVector3Array = c.verts.iter().map(|v| Vector3::new(v[0], v[1], v[2])).collect();
        let normals: PackedVector3Array = c.normals.iter().map(|v| Vector3::new(v[0], v[1], v[2])).collect();
        let colors: PackedColorArray = c.colors.iter().map(|v| Color::from_rgba(v[0], v[1], v[2], v[3])).collect();
        let uvs: PackedVector2Array = c.uvs.iter().map(|v| Vector2::new(v[0], v[1])).collect();
        let canopy = PackedFloat32Array::from(c.canopy.as_slice());
        let rocks = PackedFloat32Array::from(c.rocks.as_slice());
        let biomes = PackedInt32Array::from(&c.biomes[..]);
        let mut d = Dictionary::new();
        d.set("verts", &verts.to_variant());
        d.set("normals", &normals.to_variant());
        d.set("colors", &colors.to_variant());
        d.set("uvs", &uvs.to_variant());
        d.set("canopy", &canopy.to_variant());
        d.set("rocks", &rocks.to_variant());
        d.set("height_sum", &c.height_sum.to_variant());
        d.set("biomes", &biomes.to_variant());
        d.set("usec", &(t0.elapsed().as_micros() as i64).to_variant());
        debug_assert_eq!(c.verts.len(), M * M);
        d
    }
}
