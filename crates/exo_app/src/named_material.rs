//! Named materials of a glTF scene (window only). A scene root asks for materials by the names
//! they carry in the file (`WantMaterials`). When the scene is in, every mesh with one of those
//! names gets its own copy of the material, so two kettles or crates can take different colours,
//! and the copies are kept on the root as `NamedMaterials`.
use bevy::gltf::GltfMaterialName;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use std::collections::HashMap;

/// The material names a scene root wants per-instance copies of.
#[derive(Component)]
pub struct WantMaterials(pub &'static [&'static str]);

/// The per-instance copies, by the name they have in the file. Present once the scene is in.
#[derive(Component, Default)]
pub struct NamedMaterials(pub HashMap<&'static str, Handle<StandardMaterial>>);

pub fn window_plugin(app: &mut App) {
    app.add_observer(on_ready);
}

fn on_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    wants: Query<&WantMaterials>,
    children: Query<&Children>,
    meshes: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let root = ready.entity;
    let Ok(want) = wants.get(root) else { return };
    let mut named: HashMap<&'static str, Handle<StandardMaterial>> = HashMap::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if let Ok(kids) = children.get(e) {
            stack.extend(kids.iter());
        }
        let Ok((name, m)) = meshes.get(e) else { continue };
        let Some(key) = want.0.iter().copied().find(|w| *w == name.0) else { continue };
        let handle = match named.get(key) {
            Some(h) => h.clone(),
            None => {
                let Some(base) = mats.get(&m.0).cloned() else { continue };
                let h = mats.add(base);
                named.insert(key, h.clone());
                h
            }
        };
        commands.entity(e).insert(MeshMaterial3d(handle));
    }
    commands.entity(root).insert(NamedMaterials(named));
}
