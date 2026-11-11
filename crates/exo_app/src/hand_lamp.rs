//! The hand lamp (#329): the item `hand_lamp` (`content/items/hand_lamp.json`) is switched on and off by its use
//! (LMB with it selected, as the O2 mask is put on, `dev_area::gear`), and it is on only while it is the selected bar
//! slot: selecting another slot, dropping or storing it switches it off.
//!
//! Host decides (`LampOn`: who, which slot). The state reaches the others in `PublicStatus::lamp` with the rest
//! of the public status. A beam is a spot light entity (`LampBeam`) that exists while its owner's lamp is on: the own
//! one from the view, a remote figure's from its head along its yaw (the wire has no pitch for walkers, see
//! `net::walker_quat`). Window only: the lit lens on the held item.
use crate::controls::Controls;
use crate::displays::ActiveDisplay;
use crate::items::{Items, Role, LOCAL};
use crate::net_live::RemoteWalker;
use crate::origin::WorldPose;
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use item_core::{PlayerId, Request};
use std::collections::BTreeMap;
use walker_core::Frame;

pub const ITEM: &str = "hand_lamp";
/// The use that switches the lamp (`use` in the item's file).
pub const USE: &str = "lamp";

/// Beam values. TODO(initiator): all of them are starting values, judged by look in play.
const BEAM_COLOUR: Color = Color::srgb(1.0, 0.92, 0.75);
/// Luminous power (lm): about 12 m of useful reach in a dark room.
const BEAM_INTENSITY: f32 = 250_000.0;
const BEAM_RANGE: f32 = 12.0;
/// Cone half angles (rad), as Bevy takes them: outer 40 degrees, inner 25.
const BEAM_OUTER: f32 = 40.0 * std::f32::consts::PI / 180.0;
const BEAM_INNER: f32 = 25.0 * std::f32::consts::PI / 180.0;
/// The own beam starts right of and below the eye (camera space: x right, y up, -z forward), so shadows have some depth.
const OWN_OFFSET: Vec3 = Vec3::new(0.15, -0.12, -0.1);
/// A remote figure's beam starts at its head (figure space).
const FIGURE_OFFSET: Vec3 = Vec3::new(0.15, 1.5, -0.2);
/// Whether the own beam casts shadows (measured in #329: see the issue for the frame times).
const OWN_SHADOWS: bool = true;
const REMOTE_SHADOWS: bool = false;

/// Host: who has the lamp on and in which slot (kept only while that slot holds the lamp and is selected).
#[derive(Resource, Default, Debug)]
pub struct LampOn(pub BTreeMap<PlayerId, usize>);

/// The beam of one player (an entity exists exactly while that player's lamp is on).
#[derive(Component)]
pub struct LampBeam {
    pub owner: PlayerId,
}

/// Whether the own lamp is on: the host's own state, or the status the host sent (a client).
pub fn own_on(items: &Items, lamps: &LampOn) -> bool {
    let me = items.me();
    if items.role == Role::Client {
        items.public.iter().any(|(id, s)| *id == me && s.lamp)
    } else {
        lamps.0.contains_key(&me)
    }
}

/// Whether another player's lamp is on: from the host's own state or from the public status list.
pub fn other_on(items: &Items, lamps: &LampOn, who: PlayerId) -> bool {
    lamps.0.contains_key(&who) || items.public.iter().any(|(id, s)| *id == who && s.lamp)
}

pub fn plugin(app: &mut App) {
    use crate::phases::Fx;
    app.init_resource::<LampOn>();
    app.add_systems(
        FixedUpdate,
        lamp_input
            .after(crate::scenario::run_script)
            .after(crate::items_net::items_net_in)
            .before(crate::health::use_input)
            .before(crate::displays::use_display)
            .in_set(Fx::Input),
    );
    app.add_systems(FixedUpdate, lamp_step.after(crate::items::deliver).in_set(Fx::Walker));
    app.add_systems(Update, beams.in_set(crate::phases::Frame::World));
}

fn holds_lamp(items: &Items, who: PlayerId, slot: usize) -> bool {
    items.host.inventory(who).slot(slot).is_some_and(|s| s.item.kind.as_str() == ITEM)
}

/// The client's click with the lamp selected: the use switches it.
fn lamp_input(time: Res<Time>, mut controls: ResMut<Controls>, mut items: ResMut<Items>, active: Res<ActiveDisplay>, players: Query<&Player>) {
    let Ok(pl) = players.single() else { return };
    let slot = items.client.view.selected;
    let lamp = items.client.view.slot(slot).is_some_and(|s| s.item.kind.as_str() == ITEM);
    let usable = !controls.free_cursor && active.using().is_none() && !pl.seated && !pl.fly;
    if lamp && usable && controls.click {
        controls.click = false;
        let _ = items.request(time.elapsed_secs_f64(), Request::Use { slot, target: LOCAL });
    }
}

/// Host: the uses switch a lamp, and a lamp that left its slot or is no longer the selected slot goes off.
fn lamp_step(mut items: ResMut<Items>, mut lamps: ResMut<LampOn>) {
    if items.role == Role::Client {
        return;
    }
    for (who, slot) in std::mem::take(&mut items.lamp_uses) {
        if !holds_lamp(&items, who, slot) {
            continue;
        }
        if lamps.0.get(&who) == Some(&slot) {
            lamps.0.remove(&who);
        } else {
            lamps.0.insert(who, slot);
        }
    }
    lamps.0.retain(|who, slot| holds_lamp(&items, *who, *slot) && items.host.inventory(*who).selected == *slot);
}

/// A beam for each lamp that is on, posed every frame; the beam of a lamp that went off is gone.
#[allow(clippy::too_many_arguments)]
fn beams(
    mut commands: Commands,
    items: Res<Items>,
    lamps: Res<LampOn>,
    players: Query<&Player>,
    cam: Query<&WorldPose, (With<crate::view::MainCamera>, Without<LampBeam>)>,
    remotes: Query<(&RemoteWalker, &WorldPose), Without<LampBeam>>,
    mut beams: Query<(Entity, &LampBeam, &mut WorldPose)>,
) {
    let me = items.me();
    // (owner, position, rotation) of every lamp that is on.
    let mut want: Vec<(PlayerId, DVec3, DQuat, bool)> = Vec::new();
    if own_on(&items, &lamps) {
        if let Ok(c) = cam.single() {
            want.push((me, c.pos + c.rot * OWN_OFFSET.as_dvec3(), c.rot, OWN_SHADOWS));
        } else if let Ok(pl) = players.single() {
            // Headless: no camera, the walker's own eye and look.
            let up = pl.world_up(Frame::IDENTITY);
            let look = pl.world_look(Frame::IDENTITY);
            let rot = walker_core::look_rot(look, up);
            want.push((me, pl.world_pos(Frame::IDENTITY) + up * EYE_HEIGHT + rot * OWN_OFFSET.as_dvec3(), rot, OWN_SHADOWS));
        }
    }
    for (rw, pose) in &remotes {
        if rw.owner != me && other_on(&items, &lamps, rw.owner) {
            want.push((rw.owner, pose.pos + pose.rot * FIGURE_OFFSET.as_dvec3(), pose.rot, REMOTE_SHADOWS));
        }
    }
    for (e, b, mut pose) in &mut beams {
        match want.iter().find(|w| w.0 == b.owner) {
            Some(w) => {
                pose.pos = w.1;
                pose.rot = w.2;
            }
            None => commands.entity(e).despawn(),
        }
    }
    for (owner, pos, rot, shadows) in want {
        if !beams.iter().any(|(_, b, _)| b.owner == owner) {
            commands.spawn((
                LampBeam { owner },
                SpotLight { color: BEAM_COLOUR, intensity: BEAM_INTENSITY, range: BEAM_RANGE, outer_angle: BEAM_OUTER, inner_angle: BEAM_INNER, shadow_maps_enabled: shadows, ..default() },
                WorldPose { pos, rot },
                Transform::default(),
            ));
        }
    }
}

// ---- Window only: the lit lens -------------------------------------------------------------------------------------

#[derive(Component)]
struct Lens;

#[derive(Resource)]
struct LensAssets(Handle<Mesh>, Handle<StandardMaterial>);

pub fn window_plugin(app: &mut App) {
    app.add_systems(Startup, lens_assets);
    app.add_systems(Update, (own_lens, remote_lens).in_set(crate::phases::Frame::World));
}

fn lens_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    // The front face of the 0.06 x 0.06 x 0.2 m box, a hair proud of it.
    let mesh = meshes.add(Cuboid::new(0.05, 0.05, 0.004));
    let material = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.95, 0.8), emissive: LinearRgba::new(14.0, 11.0, 6.0, 1.0), unlit: true, ..default() });
    commands.insert_resource(LensAssets(mesh, material));
}

fn set_lens(commands: &mut Commands, assets: &LensAssets, held: Entity, kind: Option<&str>, on: bool, children: Option<&Children>, lenses: &mut Query<&mut Visibility, With<Lens>>) {
    let lamp = kind == Some(ITEM);
    let existing = children.and_then(|c| c.iter().find(|ch| lenses.contains(*ch)));
    match existing {
        Some(l) => {
            if let Ok(mut v) = lenses.get_mut(l) {
                let want = if lamp && on { Visibility::Inherited } else { Visibility::Hidden };
                if *v != want {
                    *v = want;
                }
            }
        }
        None if lamp => {
            commands.entity(held).with_children(|c| {
                c.spawn((Lens, bevy::light::NotShadowCaster, Mesh3d(assets.0.clone()), MeshMaterial3d(assets.1.clone()), Transform::from_xyz(0.0, 0.0, -0.102), if on { Visibility::Inherited } else { Visibility::Hidden }));
            });
        }
        None => {}
    }
}

fn own_lens(mut commands: Commands, assets: Option<Res<LensAssets>>, items: Res<Items>, lamps: Res<LampOn>, held: Query<(Entity, &crate::items::HeldModel, Option<&Children>)>, mut lenses: Query<&mut Visibility, With<Lens>>) {
    let Some(assets) = assets else { return };
    let on = own_on(&items, &lamps);
    for (e, m, children) in &held {
        set_lens(&mut commands, &assets, e, m.kind(), on, children, &mut lenses);
    }
}

fn remote_lens(mut commands: Commands, assets: Option<Res<LensAssets>>, items: Res<Items>, lamps: Res<LampOn>, walkers: Query<(&RemoteWalker, Option<&Children>)>, held: Query<(Entity, &crate::items_net::HeldOnRemote, Option<&Children>)>, mut lenses: Query<&mut Visibility, With<Lens>>) {
    let Some(assets) = assets else { return };
    for (rw, kids) in &walkers {
        let Some((e, m, children)) = kids.and_then(|k| k.iter().find_map(|c| held.get(c).ok())) else { continue };
        set_lens(&mut commands, &assets, e, m.kind(), other_on(&items, &lamps, rw.owner), children, &mut lenses);
    }
}
