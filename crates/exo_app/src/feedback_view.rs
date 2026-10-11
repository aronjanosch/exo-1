//! Round 6b, the window part of `feedback`: stamps punched onto the screen, camera kicks, dust at
//! touchdown, bursts of little cubes, wallet deltas floating up, and the state you can see in the
//! world: a beacon over the assigned pad, the scan ring around the ship, the patrol craft beside
//! it, the heat vignette. Everything only reads `Gameplay`; nothing here changes the game.
//! Timings and sizes are `TODO(initiator)`.
use std::collections::VecDeque;

use avian3d::prelude::{Position, Rotation};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

use crate::env::PlanetRes;
use crate::feedback::{Anchor, Cue, Tone};
use crate::gameplay::{Gameplay, HOST};
use crate::origin::WorldPose;
use crate::ship::{CameraEffects, Ship};
use crate::terrain::PlanetScene;
use crate::town::Town;
use crate::ui_theme as th;

/// How long a stamp stays (s), how long it takes to land and to fade.
const STAMP_S: f64 = 1.5;
const STAMP_IN_S: f64 = 0.16;
const STAMP_OUT_S: f64 = 0.25;
/// A stamp waiting behind this one shortens it to this (s).
const STAMP_SHORT_S: f64 = 0.8;
/// Stamps waiting at most.
const STAMP_QUEUE: usize = 3;
const MONEY_S: f64 = 1.6;
const DUST_S: f64 = 0.8;
const BITS_S: f64 = 0.9;
const BITS: usize = 12;
/// The pad beacon's height (m) and its pulse far and near (Hz).
const BEACON_H: f64 = 90.0;
const PULSE_FAR_HZ: f64 = 0.6;
const PULSE_NEAR_HZ: f64 = 3.0;

#[derive(Resource, Default)]
struct Seen(usize);

#[derive(Resource, Default)]
struct Stamps {
    /// Text, tone, earliest show time.
    queue: VecDeque<(String, Tone, f64)>,
    current: Option<(Entity, f64)>,
}

/// Moments the HUD answers with a punch: (start time, rise or fall).
#[derive(Resource, Default)]
pub struct Punches {
    pub heat: Option<(f64, bool)>,
    pub goal: Option<f64>,
    pub hops: Vec<(String, f64)>,
}

#[derive(Component)]
struct StampNode;

#[derive(Component)]
struct MoneyNode {
    born: f64,
}

#[derive(Component)]
struct Dust {
    born: f64,
    mat: Handle<StandardMaterial>,
}

#[derive(Component)]
struct Bit {
    born: f64,
    vel: DVec3,
    up: DVec3,
}

#[derive(Component)]
struct Beacon {
    pad: gameplay_core::LocationId,
    mat: Handle<StandardMaterial>,
}

#[derive(Component)]
struct ScanRing;

#[derive(Component)]
struct PatrolCraft {
    lights: [Handle<StandardMaterial>; 2],
}

#[derive(Component)]
struct Vignette;

#[derive(Component)]
struct Flash {
    born: f64,
}

pub fn window_plugin(app: &mut App) {
    app.init_resource::<Seen>().init_resource::<Stamps>().init_resource::<Punches>();
    app.add_systems(Startup, spawn_overlays);
    app.add_systems(Update, (absorb, run_stamps, run_money, run_dust, run_bits, run_flash, beacon, scan_ring, patrol_craft, vignette, hops).chain().in_set(crate::phases::Frame::World));
}

fn spawn_overlays(mut commands: Commands) {
    commands.spawn((
        Vignette,
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), display: Display::None, ..default() },
        BackgroundGradient(vec![Gradient::Radial(RadialGradient::new(
            UiPosition::CENTER,
            RadialGradientShape::FarthestCorner,
            vec![ColorStop::new(Color::NONE, percent(55)), ColorStop::new(th::faded(th::BAD, 0.6), percent(100))],
        ))]),
        GlobalZIndex(-1),
    ));
}

/// Cheap, repeatable scatter for the bursts (no random source needed for looks).
fn scatter(i: usize, salt: u64) -> f64 {
    let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 29;
    (x % 10_000) as f64 / 10_000.0
}

fn tangent_basis(up: DVec3) -> (DVec3, DVec3) {
    let e = up.any_orthonormal_vector();
    (e, up.cross(e))
}

#[allow(clippy::too_many_arguments)]
fn absorb(
    mut commands: Commands,
    time: Res<Time>,
    gp: Res<Gameplay>,
    town: Option<Res<Town>>,
    planet: Res<PlanetRes>,
    mut seen: ResMut<Seen>,
    mut stamps: ResMut<Stamps>,
    mut punches: ResMut<Punches>,
    mut fx: ResMut<CameraEffects>,
    ships: Query<&Position, With<Ship>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let now = time.elapsed_secs_f64();
    let cues = &gp.feedback.cues;
    if seen.0 > cues.len() {
        seen.0 = 0;
    }
    let ship = ships.single().ok().map(|p| p.0);
    for (n, (_, cue)) in cues.iter().enumerate().skip(seen.0) {
        match cue {
            Cue::Stamp { text, tone, hold } => {
                // The same word twice in a row is one stamp; a long queue drops its oldest.
                if stamps.queue.iter().any(|(t, _, _)| t == text) {
                    continue;
                }
                stamps.queue.push_back((text.clone(), *tone, now + hold));
                while stamps.queue.len() > STAMP_QUEUE {
                    stamps.queue.pop_front();
                }
            }
            Cue::Kick(k) => fx.0.trauma = (fx.0.trauma + k).min(1.0),
            Cue::Dust => {
                let Some(p) = ship else { continue };
                let up = planet.up(p);
                let ground = planet.centre + up * planet.surface(up) + up * 0.15;
                let mat = mats.add(StandardMaterial { base_color: Color::srgba(0.8, 0.72, 0.55, 0.7), unlit: true, alpha_mode: AlphaMode::Blend, ..default() });
                commands.spawn((
                    Mesh3d(meshes.add(Torus::new(0.8, 1.6))),
                    MeshMaterial3d(mat.clone()),
                    Transform::default(),
                    WorldPose { pos: ground, rot: DQuat::from_rotation_arc(DVec3::Y, up) },
                    PlanetScene(planet.id),
                    Dust { born: now, mat },
                ));
            }
            Cue::Burst { at, tone } => {
                let pos = match at {
                    Anchor::Ship | Anchor::Walker => ship.map(|p| p + planet.up(p) * 1.5),
                    Anchor::Kettle(id) => town.as_ref().and_then(|t| crate::smuggling::glue::kettles(t, &gp).into_iter().find(|(k, _, _)| k == id)).map(|(_, p, up)| p + up * 1.2),
                    Anchor::Buyer(id) => town.as_ref().and_then(|t| t.all_npcs().find(|n| &n.id == id).map(|n| n.pos + n.up * 1.0)),
                };
                let Some(pos) = pos else { continue };
                let up = planet.up(pos);
                let (e, nn) = tangent_basis(up);
                let c = th::tone(*tone);
                let mat = mats.add(StandardMaterial { base_color: c, emissive: c.to_linear() * 2.0, ..default() });
                let mesh = meshes.add(Cuboid::new(0.14, 0.14, 0.14));
                for i in 0..BITS {
                    let a = scatter(i, n as u64) * std::f64::consts::TAU;
                    let out = 1.5 + 2.0 * scatter(i + 99, n as u64);
                    let vel = (e * a.cos() + nn * a.sin()) * out + up * (3.0 + 2.5 * scatter(i + 7, n as u64));
                    commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), WorldPose { pos, rot: DQuat::IDENTITY }, PlanetScene(planet.id), Bit { born: now, vel, up }));
                }
            }
            Cue::Money(m) => {
                let (text, c) = if *m > 0 { (format!("+{m} cr"), th::GOOD) } else { (format!("{m} cr"), th::BAD) };
                commands.spawn((
                    MoneyNode { born: now },
                    Text::new(text),
                    TextFont { font_size: FontSize::Px(th::LARGE), ..default() },
                    TextColor(c),
                    TextShadow { offset: Vec2::splat(2.0), color: th::SHADOW },
                    Node { position_type: PositionType::Absolute, right: px(th::MARGIN), top: px(34), ..default() },
                    UiTransform::IDENTITY,
                ));
            }
            Cue::Heat { up } => {
                punches.heat = Some((now, *up));
                if *up {
                    commands.spawn((Flash { born: now }, Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() }, BackgroundColor(th::faded(th::BAD, 0.3))));
                }
            }
            Cue::Goal => punches.goal = Some(now),
            Cue::Hop(id) => punches.hops.push((id.clone(), now)),
        }
    }
    seen.0 = cues.len();
}

fn run_stamps(mut commands: Commands, time: Res<Time>, mut stamps: ResMut<Stamps>, mut q: Query<(&mut UiTransform, &mut TextColor, &mut TextShadow), With<StampNode>>) {
    let now = time.elapsed_secs_f64();
    if stamps.current.is_none()
        && let Some(i) = stamps.queue.iter().position(|(_, _, at)| *at <= now)
    {
        let (text, tone, _) = stamps.queue.remove(i).expect("found");
        let e = commands
            .spawn((
                StampNode,
                Text::new(text),
                TextFont { font_size: FontSize::Px(th::STAMP), ..default() },
                TextColor(th::tone(tone)),
                TextShadow { offset: Vec2::splat(3.0), color: th::SHADOW },
                TextLayout::justify(Justify::Center),
                Node { position_type: PositionType::Absolute, top: percent(36), width: percent(100), justify_content: JustifyContent::Center, ..default() },
                UiTransform { scale: Vec2::splat(1.7), ..UiTransform::IDENTITY },
            ))
            .id();
        stamps.current = Some((e, now));
    }
    let Some((e, born)) = stamps.current else { return };
    let life = if stamps.queue.is_empty() { STAMP_S } else { STAMP_SHORT_S };
    let age = now - born;
    if age >= life {
        commands.entity(e).despawn();
        stamps.current = None;
        return;
    }
    if let Ok((mut t, mut c, mut sh)) = q.get_mut(e) {
        let k = (age / STAMP_IN_S) as f32;
        t.scale = Vec2::splat(1.7 - 0.7 * th::ease_out_back(k));
        t.rotation = Rot2::degrees(-4.0 * (1.0 - th::ease_out(k)));
        let a = (((life - age) / STAMP_OUT_S) as f32).min(1.0);
        c.0 = th::faded(c.0.with_alpha(1.0), a);
        sh.color = th::faded(th::SHADOW, a);
    }
}

fn run_money(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &MoneyNode, &mut UiTransform, &mut TextColor)>) {
    let now = time.elapsed_secs_f64();
    // Several deltas at once stack instead of overlapping.
    let mut rank = 0.0;
    for (e, m, mut t, mut c) in &mut q {
        let age = now - m.born;
        if age >= MONEY_S {
            commands.entity(e).despawn();
            continue;
        }
        let k = (age / MONEY_S) as f32;
        t.translation = Val2::px(0.0, 26.0 * rank + 40.0 * th::ease_out(k));
        t.scale = Vec2::splat(if age < 0.12 { 1.4 - 0.4 * (age / 0.12) as f32 } else { 1.0 });
        c.0 = c.0.with_alpha(if k > 0.6 { 1.0 - (k - 0.6) / 0.4 } else { 1.0 });
        rank += 1.0;
    }
}

fn run_dust(mut commands: Commands, time: Res<Time>, mut mats: ResMut<Assets<StandardMaterial>>, mut q: Query<(Entity, &Dust, &mut Transform)>) {
    let now = time.elapsed_secs_f64();
    for (e, d, mut t) in &mut q {
        let k = ((now - d.born) / DUST_S) as f32;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let s = 1.0 + 5.0 * th::ease_out(k);
        t.scale = Vec3::new(s, 0.25, s);
        if let Some(mut m) = mats.get_mut(&d.mat) {
            m.base_color = m.base_color.with_alpha(0.7 * (1.0 - k));
        }
    }
}

fn run_bits(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Bit, &mut WorldPose, &mut Transform)>) {
    let now = time.elapsed_secs_f64();
    let dt = time.delta_secs_f64();
    for (e, mut b, mut wp, mut t) in &mut q {
        let k = (now - b.born) / BITS_S;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let g = b.up * -9.0 * dt;
        b.vel += g;
        wp.pos += b.vel * dt;
        wp.rot = DQuat::from_rotation_y(k * 9.0) * DQuat::from_rotation_x(k * 7.0);
        t.scale = Vec3::splat((1.0 - k as f32).max(0.0));
    }
}

fn run_flash(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &Flash, &mut BackgroundColor)>) {
    let now = time.elapsed_secs_f64();
    for (e, f, mut bg) in &mut q {
        let k = ((now - f.born) / 0.35) as f32;
        if k >= 1.0 {
            commands.entity(e).despawn();
        } else {
            bg.0 = th::faded(th::BAD, 0.3 * (1.0 - k));
        }
    }
}

/// The light column over the pad the tower assigned; it pulses faster as the ship closes in.
#[allow(clippy::too_many_arguments)]
fn beacon(
    mut commands: Commands,
    time: Res<Time>,
    gp: Res<Gameplay>,
    planet: Res<PlanetRes>,
    ships: Query<&Position, With<Ship>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    shown: Query<(Entity, &Beacon)>,
) {
    let clearance = gp.smuggling.traffic.clearance(HOST);
    let want = clearance.map(|c| (c.pad.clone(), c.inspection));
    for (e, b) in &shown {
        if want.as_ref().map(|(p, _)| p) != Some(&b.pad) {
            commands.entity(e).despawn();
        }
    }
    let Some((pad_id, inspection)) = want else { return };
    let Some(pad) = gp.pad_of(&pad_id) else { return };
    let ship = ships.single().ok().map(|p| p.0);
    let near = ship.map_or(1.0, |s| ((s.distance(pad.centre) - 60.0) / 1500.0).clamp(0.0, 1.0));
    let hz = PULSE_NEAR_HZ + (PULSE_FAR_HZ - PULSE_NEAR_HZ) * near;
    let t = time.elapsed_secs_f64();
    let pulse = 0.55 + 0.45 * (t * hz * std::f64::consts::TAU).sin().abs();
    if let Some((_, b)) = shown.iter().find(|(_, b)| b.pad == pad_id) {
        if let Some(mut m) = mats.get_mut(&b.mat) {
            m.base_color = m.base_color.with_alpha((0.18 + 0.32 * pulse) as f32);
        }
        return;
    }
    let c = if inspection { th::BAD } else { th::INFO };
    let mat = mats.add(StandardMaterial { base_color: th::faded(c, 0.4), emissive: c.to_linear() * 1.5, unlit: true, alpha_mode: AlphaMode::Add, ..default() });
    let rot = DQuat::from_rotation_arc(DVec3::Y, pad.up);
    let _ = planet.id;
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(2.5, BEACON_H as f32))),
        MeshMaterial3d(mat.clone()),
        Transform::default(),
        WorldPose { pos: pad.centre + pad.up * (BEACON_H * 0.5), rot },
        PlanetScene(planet.id),
        Beacon { pad: pad_id.clone(), mat: mat.clone() },
    ));
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(pad.radius as f32 - 0.5, pad.radius as f32))),
        MeshMaterial3d(mat.clone()),
        Transform::default(),
        WorldPose { pos: pad.centre + pad.up * 0.2, rot },
        PlanetScene(planet.id),
        Beacon { pad: pad_id, mat },
    ));
}

/// While a scan runs, a ring sweeps up and down the ship.
#[allow(clippy::too_many_arguments)]
fn scan_ring(mut commands: Commands, time: Res<Time>, gp: Res<Gameplay>, planet: Res<PlanetRes>, ships: Query<(&Position, &Rotation), With<Ship>>, mut meshes: ResMut<Assets<Mesh>>, mut mats: ResMut<Assets<StandardMaterial>>, mut rings: Query<(Entity, &mut WorldPose), With<ScanRing>>) {
    let open = gp.smuggling.open_scan.is_some();
    let Ok((p, r)) = ships.single() else { return };
    if !open {
        for (e, _) in &rings {
            commands.entity(e).despawn();
        }
        return;
    }
    let up = r.0 * DVec3::Y;
    let sweep = (time.elapsed_secs_f64() * std::f64::consts::PI / 0.6).sin();
    let pos = p.0 + up * (1.2 + 1.8 * sweep);
    if let Ok((_, mut wp)) = rings.single_mut() {
        wp.pos = pos;
        wp.rot = r.0;
        return;
    }
    let mat = mats.add(StandardMaterial { base_color: th::faded(th::BAD, 0.7), emissive: th::BAD.to_linear() * 3.0, unlit: true, alpha_mode: AlphaMode::Add, ..default() });
    commands.spawn((Mesh3d(meshes.add(Torus::new(5.2, 5.5))), MeshMaterial3d(mat), Transform::default(), WorldPose { pos, rot: r.0 }, PlanetScene(planet.id), ScanRing));
}

/// A patrol that ordered a halt (or scans in the air) flies beside the ship, lights flashing.
#[allow(clippy::too_many_arguments)]
fn patrol_craft(
    mut commands: Commands,
    time: Res<Time>,
    gp: Res<Gameplay>,
    planet: Res<PlanetRes>,
    ships: Query<(&Position, &Rotation), With<Ship>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut craft: Query<(Entity, &PatrolCraft, &mut WorldPose)>,
) {
    let s = &gp.smuggling;
    let active = s.patrol.ordered_left().is_some() || (s.patrol_scan && s.open_scan.is_some());
    let Ok((p, r)) = ships.single() else { return };
    if !active {
        for (e, _, _) in &craft {
            commands.entity(e).despawn();
        }
        return;
    }
    let up = planet.up(p.0);
    let side = r.0 * DVec3::X;
    let ahead = r.0 * DVec3::NEG_Z;
    let target = p.0 + up * 5.0 + side * 12.0 + ahead * 22.0;
    let t = time.elapsed_secs_f64();
    if let Ok((_, c, mut wp)) = craft.single_mut() {
        let k = (time.delta_secs_f64() * 3.0).min(1.0);
        wp.pos = wp.pos.lerp(target, k);
        wp.rot = r.0;
        let on = (t * 4.0) as i64 % 2 == 0;
        for (i, h) in c.lights.iter().enumerate() {
            if let Some(mut m) = mats.get_mut(h) {
                let lit = on == (i == 0);
                let col = if i == 0 { Color::srgb(1.0, 0.15, 0.1) } else { Color::srgb(0.15, 0.3, 1.0) };
                m.emissive = col.to_linear() * if lit { 12.0 } else { 0.2 };
            }
        }
        return;
    }
    let hull = mats.add(StandardMaterial { base_color: Color::srgb(0.12, 0.13, 0.16), metallic: 0.5, perceptual_roughness: 0.5, ..default() });
    let red = mats.add(StandardMaterial { base_color: Color::srgb(1.0, 0.15, 0.1), emissive: LinearRgba::rgb(10.0, 1.0, 0.5), ..default() });
    let blue = mats.add(StandardMaterial { base_color: Color::srgb(0.15, 0.3, 1.0), emissive: LinearRgba::rgb(0.5, 1.0, 10.0), ..default() });
    let light = meshes.add(Sphere::new(0.9));
    commands
        .spawn((Mesh3d(meshes.add(Cuboid::new(4.5, 1.4, 7.0))), MeshMaterial3d(hull), Transform::default(), WorldPose { pos: target + up * 20.0, rot: r.0 }, PlanetScene(planet.id), PatrolCraft { lights: [red.clone(), blue.clone()] }))
        .with_children(|c| {
            c.spawn((Mesh3d(light.clone()), MeshMaterial3d(red), Transform::from_xyz(-1.6, 1.0, 0.0)));
            c.spawn((Mesh3d(light), MeshMaterial3d(blue), Transform::from_xyz(1.6, 1.0, 0.0)));
        });
}

/// Red edges by heat, breathing; a fresh star flashes (`run_flash`).
fn vignette(time: Res<Time>, gp: Res<Gameplay>, mut q: Query<(&mut Node, &mut BackgroundGradient), With<Vignette>>) {
    let Ok((mut n, mut g)) = q.single_mut() else { return };
    let stars = gp.smuggling.heat.stars();
    let d = if stars == 0 { Display::None } else { Display::Flex };
    if n.display != d {
        n.display = d;
    }
    if stars == 0 {
        return;
    }
    let breathe = 0.85 + 0.15 * (time.elapsed_secs_f64() * 1.4).sin() as f32;
    let a = (0.04 + 0.05 * stars as f32) * breathe;
    if let Some(Gradient::Radial(r)) = g.0.first_mut()
        && let Some(stop) = r.stops.last_mut()
    {
        stop.color = th::faded(th::BAD, a);
    }
}

/// A buyer who was paid jumps (squash, hop, land).
fn hops(time: Res<Time>, town: Option<Res<Town>>, origin: Res<crate::origin::RenderOrigin>, mut punches: ResMut<Punches>, mut q: Query<(&crate::town::NpcModel, &mut Transform)>) {
    let now = time.elapsed_secs_f64();
    punches.hops.retain(|(_, born)| now - born < 0.6);
    if town.is_none() {
        return;
    }
    for (npc, mut t) in &mut q {
        let base = (npc.pos - origin.origin).as_vec3();
        let up = npc.up.as_vec3();
        match punches.hops.iter().find(|(id, _)| *id == npc.id) {
            Some((_, born)) => {
                let k = ((now - born) / 0.6) as f32;
                let h = (k * std::f32::consts::PI).sin() * 0.9;
                t.translation = base + up * h;
                let squash = if k < 0.15 { 1.0 - 0.25 * (k / 0.15) } else { 1.0 };
                t.scale = Vec3::new(1.0 / squash.sqrt(), squash, 1.0 / squash.sqrt());
            }
            None => {
                if t.scale != Vec3::ONE {
                    t.scale = Vec3::ONE;
                    t.translation = base;
                }
            }
        }
    }
}
