//! - The host works it out from its own state (`HealthState`, the repair sessions, the sprays) and
//!   sends the whole list to every client when it changes (`NetOut::Status`); a client that joins
//!   gets the list as it is. The host's own treatment hold is read here, a client's comes over the
//!   wire (`ToHost::Treat`, sent when the hold starts, moves to another player or stops).
//! - Not public: the vitals (only the owner gets them) and the reactor's and fires' own values.
use crate::dev_area::fire::Sprays;
use crate::health::{HealthState, UseHold};
use health_core::Life;
use crate::items::{Items, NetOut, Role};
use crate::rescue::Rescuers;
use crate::net_live::{Net, RemoteWalker};
use crate::repair::RepairSessions;
use bevy::prelude::*;
use item_core::wire::{Doing, Injuries, PublicStatus};
use item_core::PlayerId;
use std::collections::BTreeMap;

pub fn plugin(app: &mut App) {
    use crate::phases::Fx;
    app.init_resource::<TreatState>();
    app.add_systems(FixedUpdate, treat_out.run_if(resource_exists::<Net>).after(crate::health::use_input).in_set(Fx::Input));
    app.add_systems(FixedLast, public_out.run_if(resource_exists::<Net>).before(crate::items_net::items_net_out));
    app.add_systems(FixedUpdate, remote_status_in.run_if(resource_exists::<Net>).after(crate::items_net::items_net_in).in_set(Fx::Input));
}

#[derive(Resource, Default)]
struct TreatState {
    by_player: BTreeMap<PlayerId, Option<PlayerId>>,
    sent: Option<PlayerId>,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct RemoteStatus(pub PublicStatus);

fn treat_out(mut items: ResMut<Items>, hold: Res<UseHold>, mut st: ResMut<TreatState>) {
    if items.role != Role::Client {
        return;
    }
    let now = hold.0.map(|h| h.target);
    if now != st.sent {
        st.sent = now;
        items.ask_treat(now);
    }
}

/// exist with the dev area only.
#[allow(clippy::too_many_arguments)]
fn public_out(mut items: ResMut<Items>, st: Res<HealthState>, hold: Res<UseHold>, sprays: Option<Res<Sprays>>, repairs: Res<RepairSessions>, rescue: Res<Rescuers>, mut treat: ResMut<TreatState>) {
    if items.role != Role::Host {
        return;
    }
    for (who, target) in std::mem::take(&mut items.treat_asks) {
        treat.by_player.insert(who, target);
    }
    let me = items.me();
    treat.by_player.insert(me, hold.0.map(|h| h.target));
    let now: Vec<(PlayerId, PublicStatus)> = st
        .bodies
        .iter()
        .map(|(id, b)| {
            let down = b.respawn_in.is_some();
            let unconscious = b.health.state() == Life::Unconscious;
            let doing = if down || unconscious {
                Doing::Nothing
            } else if let Some(s) = repairs.sessions.iter().find(|s| s.player == *id) {
                Doing::Repairing(s.device)
            } else if sprays.as_ref().is_some_and(|s| s.0.contains_key(id)) {
                Doing::Spraying
            } else if let Some(patient) = rescue.by_rescuer.get(id) {
                Doing::Reviving(*patient)
            } else if let Some(Some(target)) = treat.by_player.get(id) {
                Doing::Treating(*target)
            } else {
                Doing::Nothing
            };
            // The injuries the others see (the medic view): the same numbers as the owner's vitals.
            let v = crate::health::vitals_of(&st.tuning, b);
            let hurt = Injuries::from_shares(v.health, v.burn_level, v.radiation, v.oxygen.map_or(0.0, |air| 1.0 - air));
            (*id, PublicStatus { burning: b.health.burning(), down, unconscious, doing, hurt })
        })
        .collect();
    if now != items.public {
        items.public = now.clone();
        items.net_out.push(NetOut::Status { to: None, s: now });
    }
}

fn remote_status_in(mut commands: Commands, items: Res<Items>, mut q: Query<(Entity, &RemoteWalker, Option<&mut RemoteStatus>)>) {
    for (e, rw, status) in &mut q {
        let want = items.public.iter().find(|(id, _)| *id == rw.owner).map(|(_, s)| *s).unwrap_or_default();
        match status {
            Some(mut s) => {
                if s.0 != want {
                    s.0 = want;
                }
            }
            None => {
                commands.entity(e).insert(RemoteStatus(want));
            }
        }
    }
}

pub fn window_plugin(app: &mut App) {
    use crate::phases::Frame;
    app.add_systems(Update, (add_status_visuals, tag_parts, pose_parts, flames, status_tags).chain().in_set(Frame::World));
}

/// Where the three flames sit on the figure (figure-local, metres).
const FLAME_AT: [Vec3; 3] = [Vec3::new(-0.28, 1.25, 0.05), Vec3::new(0.26, 1.45, -0.05), Vec3::new(0.0, 1.9, 0.0)];

#[derive(Component)]
struct Flame {
    walker: Entity,
    index: f32,
}

#[derive(Component)]
struct FlameLight {
    walker: Entity,
}

/// A figure part (a body part or the held item) and where it stands upright; the down pose lays
/// it over from the feet.
#[derive(Component)]
struct Upright(Transform);

#[derive(Component)]
struct StatusTag(Entity);

const LYING: f32 = -std::f32::consts::FRAC_PI_2;

fn add_status_visuals(mut commands: Commands, q: Query<Entity, Added<RemoteWalker>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    if q.is_empty() {
        return;
    }
    let flame_mesh = meshes.add(Cuboid::from_size(Vec3::new(0.16, 0.3, 0.16)));
    let flame_mat = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.45, 0.1), emissive: LinearRgba::new(4.0, 1.2, 0.2, 1.0), unlit: true, ..default() });
    for e in &q {
        commands.entity(e).with_children(|c| {
            for (i, at) in FLAME_AT.into_iter().enumerate() {
                c.spawn((Flame { walker: e, index: i as f32 }, Mesh3d(flame_mesh.clone()), MeshMaterial3d(flame_mat.clone()), Transform::from_translation(at), Visibility::Hidden));
            }
            c.spawn((FlameLight { walker: e }, PointLight { color: Color::srgb(1.0, 0.5, 0.2), intensity: 60_000.0, range: 6.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(0.0, 1.4, 0.0), Visibility::Hidden));
        });
        commands.spawn((StatusTag(e), Text::new(""), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(Color::WHITE), Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden));
    }
}

fn tag_parts(mut commands: Commands, figures: Query<(), With<RemoteWalker>>, parts: Query<(Entity, &ChildOf, &Transform), (With<Mesh3d>, Without<Flame>, Without<Upright>)>) {
    for (e, parent, t) in &parts {
        if figures.contains(parent.parent()) {
            commands.entity(e).insert(Upright(*t));
        }
    }
}

fn pose_parts(statuses: Query<&RemoteStatus>, mut parts: Query<(&ChildOf, &Upright, &mut Transform), Without<Flame>>) {
    let lie = Quat::from_rotation_z(LYING);
    for (parent, up, mut t) in &mut parts {
        let down = statuses.get(parent.parent()).is_ok_and(|s| s.0.down || s.0.unconscious);
        let want = if down { Transform { translation: lie * up.0.translation, rotation: lie * up.0.rotation, scale: up.0.scale } } else { up.0 };
        if *t != want {
            *t = want;
        }
    }
}

fn flames(time: Res<Time>, statuses: Query<&RemoteStatus>, mut flames: Query<(&Flame, &mut Transform, &mut Visibility), Without<FlameLight>>, mut lights: Query<(&FlameLight, &mut Visibility), Without<Flame>>) {
    let t = time.elapsed_secs();
    let burning = |w: Entity| statuses.get(w).is_ok_and(|s| s.0.burning && !s.0.down && !s.0.unconscious);
    for (f, mut tr, mut vis) in &mut flames {
        let on = burning(f.walker);
        let want = if on { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if on {
            let k = 0.85 + 0.2 * (t * 13.0 + f.index * 2.1).sin();
            tr.scale = Vec3::new(k, 0.8 + 0.5 * (t * 17.0 + f.index).sin().abs(), k);
        }
    }
    for (l, mut vis) in &mut lights {
        let want = if burning(l.walker) { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

/// The label: DOWN, UNCONSCIOUS, else what the player does, else BURNING; over the name tag.
fn label_of(s: &PublicStatus) -> Option<(&'static str, Color)> {
    if s.down {
        return Some(("DOWN", Color::srgb(0.6, 0.6, 0.6)));
    }
    if s.unconscious {
        return Some(("UNCONSCIOUS", Color::srgb(0.6, 0.6, 0.6)));
    }
    match s.doing {
        Doing::Treating(_) => Some(("TREATING", Color::srgb(0.5, 0.9, 1.0))),
        Doing::Repairing(_) => Some(("REPAIRING", Color::srgb(1.0, 0.85, 0.4))),
        Doing::Spraying => Some(("SPRAYING", Color::srgb(0.6, 0.85, 1.0))),
        Doing::Reviving(_) => Some(("REVIVING", Color::srgb(0.95, 0.5, 0.9))),
        Doing::Nothing if s.burning => Some(("BURNING", Color::srgb(1.0, 0.5, 0.2))),
        Doing::Nothing => None,
    }
}

/// The labels follow their figures on screen, as the name tags do; hidden behind the camera, beyond
/// 150 m, and without a label.
fn status_tags(
    mut commands: Commands,
    cam: Query<(&Camera, &GlobalTransform), With<crate::view::MainCamera>>,
    walkers: Query<(&GlobalTransform, &RemoteStatus)>,
    mut tags: Query<(Entity, &StatusTag, &mut Text, &mut TextColor, &mut Node, &mut Visibility, &ComputedNode)>,
) {
    let Ok((camera, cam_gt)) = cam.single() else { return };
    for (tag, StatusTag(of), mut text, mut colour, mut node, mut vis, computed) in &mut tags {
        let Ok((gt, status)) = walkers.get(*of) else {
            commands.entity(tag).despawn();
            continue;
        };
        let Some((words, c)) = label_of(&status.0) else {
            *vis = Visibility::Hidden;
            continue;
        };
        if text.0 != words {
            text.0 = words.to_string();
        }
        colour.0 = c;
        let head = gt.translation() + gt.up() * 2.5;
        let far = head.distance(cam_gt.translation()) > 150.0;
        match camera.world_to_viewport(cam_gt, head) {
            Ok(p) if !far => {
                let size = computed.size() * computed.inverse_scale_factor();
                // One line above the name tag, which sits on the head.
                node.left = px(p.x - size.x * 0.5);
                node.top = px(p.y - 2.0 * size.y - 4.0);
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
}
