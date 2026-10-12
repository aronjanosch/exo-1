//! Window only: fires as orange boxes that grow and shrink, the reactor's status light, the aux
//! lights that dim with their supply, the door's colour, the extinguisher gone from its mount
//! while carried, and a white puff where the spray lands. Headless there are no assets and it
//! does nothing. TODO(initiator): the look.
use super::*;
use crate::origin::{RenderOrigin, WorldPos};
use fire_core::FireId;
use reactor_core::AlarmKind;
use std::collections::HashMap;

#[derive(Default)]
pub struct LookState {
    cube: Option<Handle<Mesh>>,
    fire_mat: Option<Handle<StandardMaterial>>,
    fires: HashMap<FireId, Entity>,
    reactor_light: Option<(Entity, Handle<StandardMaterial>)>,
    lights: Vec<Handle<StandardMaterial>>,
    puff: Option<Entity>,
    sprayed: f64,
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    e: Res<Engineering>,
    (area, planet, origin): (Res<DevArea>, Res<PlanetRes>, Res<RenderOrigin>),
    mut boxes: Query<(&DevBox, &MeshMaterial3d<StandardMaterial>, &mut Visibility)>,
    mut placed: Query<(&mut Transform, &mut WorldPos), Without<DevBox>>,
    players: Query<&Player>,
    mut s: Local<LookState>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else { return };
    let Some(frame) = area.frame else { return };
    let cube = s.cube.get_or_insert_with(|| meshes.add(Cuboid::new(1.0, 1.0, 1.0))).clone();
    let place = |p: DVec3| -> (DVec3, Quat) {
        let (w, up) = frame.ground(&planet, p.x, p.z, p.y);
        (w, frame.rotation(up, 0.0).as_quat())
    };
    let glow = |m: &mut Assets<StandardMaterial>, c: LinearRgba| m.add(StandardMaterial { base_color: Color::BLACK, emissive: c, unlit: true, ..default() });

    // Once: the reactor light and the aux lights on posts.
    if s.reactor_light.is_none() {
        let mat = glow(&mut materials, LinearRgba::BLACK);
        let (w, rot) = place(area3(REACTOR_AT, REACTOR_SIZE[1] + 0.3));
        let ent = commands.spawn((Mesh3d(cube.clone()), MeshMaterial3d(mat.clone()), Transform::from_rotation(rot).with_scale(Vec3::splat(0.5)), WorldPos(w))).id();
        s.reactor_light = Some((ent, mat));
        let post = materials.add(StandardMaterial { base_color: Color::srgb(0.25, 0.25, 0.27), perceptual_roughness: 0.9, ..default() });
        for at in LIGHTS {
            let (w, rot) = place(area3(at, 1.1));
            commands.spawn((Mesh3d(cube.clone()), MeshMaterial3d(post.clone()), Transform::from_rotation(rot).with_scale(Vec3::new(0.12, 2.2, 0.12)), WorldPos(w)));
            let mat = glow(&mut materials, LinearRgba::BLACK);
            let (w, rot) = place(area3(at, 2.4));
            commands.spawn((Mesh3d(cube.clone()), MeshMaterial3d(mat.clone()), Transform::from_rotation(rot).with_scale(Vec3::splat(0.4)), WorldPos(w)));
            s.lights.push(mat);
        }
    }
    let r = &e.reactor;
    let light = if !r.condition().functioning() || r.alarm(AlarmKind::TempCritical) || r.alarm(AlarmKind::Shutdown) {
        LinearRgba::new(6.0, 0.2, 0.1, 1.0)
    } else if r.alarm(AlarmKind::TempHigh) || r.alarm(AlarmKind::FuelLow) {
        LinearRgba::new(5.0, 3.5, 0.1, 1.0)
    } else if r.power_on() {
        LinearRgba::new(0.2, 4.0, 0.6, 1.0)
    } else {
        LinearRgba::new(0.15, 0.15, 0.15, 1.0)
    };
    if let Some((_, h)) = &s.reactor_light
        && let Some(mut m) = materials.get_mut(h)
        && m.emissive != light
    {
        m.emissive = light;
    }
    let aux = e.grid.factor(Group::Aux) as f32;
    for h in &s.lights {
        let c = LinearRgba::new(5.0 * aux, 4.2 * aux, 2.5 * aux, 1.0);
        if let Some(mut m) = materials.get_mut(h)
            && m.emissive != c
        {
            m.emissive = c;
        }
    }

    // The door's colour and the extinguisher on its mount.
    for (b, mat, mut vis) in &mut boxes {
        match b.id.as_str() {
            "engineer_door" => {
                let c = if e.door_open() { Color::srgb(0.55, 0.75, 0.5) } else { Color::srgb(0.45, 0.3, 0.2) };
                if let Some(mut m) = materials.get_mut(&mat.0)
                    && m.base_color != c
                {
                    m.base_color = c;
                }
            }
            "engineer_extinguisher" => {
                let v = if e.extinguisher.held { Visibility::Hidden } else { Visibility::Inherited };
                if *vis != v {
                    *vis = v;
                }
            }
            _ => {}
        }
    }

    // Fires.
    let fire_mat = s.fire_mat.get_or_insert_with(|| materials.add(StandardMaterial { base_color: Color::srgba(1.0, 0.45, 0.05, 0.85), emissive: LinearRgba::new(6.0, 1.8, 0.2, 1.0), alpha_mode: AlphaMode::Blend, ..default() })).clone();
    let live: Vec<FireId> = e.deck.fires().iter().map(|f| f.id).collect();
    s.fires.retain(|id, ent| {
        let keep = live.contains(id);
        if !keep {
            commands.entity(*ent).despawn();
        }
        keep
    });
    for f in e.deck.fires() {
        let size = f.size.max(0.05);
        let (w, rot) = place(DVec3::new(f.pos.x, size * 0.3, f.pos.z));
        let tf = Transform { translation: (w - origin.origin).as_vec3(), rotation: rot, scale: Vec3::new(size as f32, (size * 0.6) as f32, size as f32) };
        match s.fires.get(&f.id).copied() {
            Some(ent) => {
                if let Ok((mut t, mut wp)) = placed.get_mut(ent) {
                    *t = tf;
                    wp.0 = w;
                }
            }
            None => {
                let ent = commands.spawn((Mesh3d(cube.clone()), MeshMaterial3d(fire_mat.clone()), tf, WorldPos(w))).id();
                s.fires.insert(f.id, ent);
            }
        }
    }

    // The spray: a puff where it lands while spraying.
    let spraying = e.stats.sprayed_s > s.sprayed;
    s.sprayed = e.stats.sprayed_s;
    let at = players.single().ok().and_then(crate::displays::eye_on_foot).map(|(eye, look)| {
        let feet = area_of(&frame, &planet, eye) - DVec3::Y * EYE_HEIGHT;
        let flat = DVec3::new(look.dot(frame.east), 0.0, look.dot(frame.north)).normalize_or_zero();
        spray_point(&e.deck, feet, flat)
    });
    match (spraying, at, s.puff) {
        (true, Some(p), puff) => {
            let (w, rot) = place(p);
            let tf = Transform { translation: (w - origin.origin).as_vec3(), rotation: rot, scale: Vec3::splat(0.6) };
            match puff {
                Some(ent) => {
                    if let Ok((mut t, mut wp)) = placed.get_mut(ent) {
                        *t = tf;
                        wp.0 = w;
                    }
                }
                None => {
                    let mat = materials.add(StandardMaterial { base_color: Color::srgba(0.95, 0.97, 1.0, 0.6), alpha_mode: AlphaMode::Blend, unlit: true, ..default() });
                    s.puff = Some(commands.spawn((Mesh3d(cube.clone()), MeshMaterial3d(mat), tf, WorldPos(w))).id());
                }
            }
        }
        (false, _, Some(ent)) | (true, None, Some(ent)) => {
            commands.entity(ent).despawn();
            s.puff = None;
        }
        _ => {}
    }
}
