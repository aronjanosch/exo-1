//! Camera, light, sky, HUD and ship visuals. Nothing here feeds back into the simulation.
use crate::env::PlanetRes;
use crate::origin::{BodyInterp, RenderOrigin, WorldPose};
use crate::ring::Ring;
use crate::ship::{Ship, ShipPart};
use crate::terrain::Terrain;
use crate::walker::{Player, WalkStats, EYE_HEIGHT};
use crate::controls::Controls;
use bevy::camera::PerspectiveProjection;
use bevy::light::GlobalAmbientLight;
use bevy::math::{DMat3, DQuat, DVec3};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use flight_core::{PlanetEnv, CHASE_CAMERA_OFFSET, CHASE_CAMERA_PITCH_DEG};

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
pub struct Hud;

/// Walker feet, up and heading in world space before and after the last fixed step.
#[derive(Component, Default)]
pub struct PlayerInterp {
    pub prev: (DVec3, DVec3, DVec3),
    pub curr: (DVec3, DVec3, DVec3),
}

#[derive(Resource, Default)]
pub struct ViewState {
    pub orbit: bool,
    pub orbit_yaw: f64,
    pub orbit_pitch: f64,
    /// Frame-time samples (ms) of the current scenario phase.
    pub frame_ms: Vec<f64>,
    /// Frames to leave out of frame-time stats (screenshot readback).
    pub skip_frames: u32,
}

pub fn setup_view(mut commands: Commands) {
    commands.spawn((
        MainCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 75f32.to_radians(), near: 0.05, far: 50_000.0, ..default() }),
        Transform::default(),
        WorldPose::default(),
        DistanceFog { color: Color::srgb(0.72, 0.82, 0.95), falloff: FogFalloff::Exponential { density: 0.00025 }, ..default() },
    ));
    commands.spawn((
        DirectionalLight { illuminance: 9000.0, shadow_maps_enabled: false, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, 30f32.to_radians(), -50f32.to_radians(), 0.0)),
    ));
    commands.insert_resource(GlobalAmbientLight { color: Color::srgb(0.55, 0.65, 0.8), brightness: 400.0, ..default() });
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        Node { position_type: PositionType::Absolute, top: px(8), left: px(8), ..default() },
    ));
}

/// Meshes for ship parts (the simulation spawns only colliders and markers).
pub fn add_ship_visuals(
    mut commands: Commands,
    q: Query<(Entity, &ShipPart), Added<ShipPart>>,
    ships: Query<Entity, Added<Ship>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (e, part) in &q {
        commands.entity(e).insert((Mesh3d(meshes.add(Cuboid::from_size(part.size))), MeshMaterial3d(materials.add(part.color))));
    }
    for e in &ships {
        commands.entity(e).insert(BodyInterp::default());
    }
}

/// Capsule for the walker of another player (spike 10).
pub fn add_remote_walker_visuals(
    mut commands: Commands,
    q: Query<Entity, Added<crate::net_live::RemoteWalker>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for e in &q {
        commands.entity(e).with_children(|c| {
            c.spawn((Mesh3d(meshes.add(Capsule3d::new(0.35, 1.1))), MeshMaterial3d(materials.add(Color::srgb(0.25, 0.95, 0.45))), Transform::from_xyz(0.0, 0.9, 0.0)));
        });
    }
}

pub fn record_player_view(
    mut players: Query<(&Player, Option<&mut PlayerInterp>, Entity)>,
    ships: Query<(Entity, &avian3d::prelude::Position, &avian3d::prelude::Rotation), With<Ship>>,
    remotes: Query<(&avian3d::prelude::Position, &avian3d::prelude::Rotation), With<crate::ship::RemoteShip>>,
    planet: Res<PlanetRes>,
    mut commands: Commands,
) {
    let Ok((pl, interp, e)) = players.single_mut() else { return };
    let Some((own, p, r)) = ships.iter().next() else { return };
    // The cabin the walker is in can be a remote ship's proxy (spike 10).
    let (p, r) = match pl.ship {
        Some(c) if c != own => remotes.get(c).unwrap_or((p, r)),
        _ => (p, r),
    };
    let frame = crate::walker::ship_frame(p, r);
    let fwd = if pl.ship.is_some() { frame.rot * pl.w.forward } else { pl.w.forward };
    let now = (pl.world_pos(frame), pl.world_up(frame, &planet), fwd);
    match interp {
        Some(mut i) => {
            i.prev = i.curr;
            i.curr = now;
        }
        None => {
            commands.entity(e).insert(PlayerInterp { prev: now, curr: now });
        }
    }
}

fn look_basis(fwd: DVec3, up: DVec3) -> DQuat {
    let f = (fwd - up * fwd.dot(up)).normalize();
    DQuat::from_mat3(&DMat3::from_cols(f.cross(up), up, -f))
}

#[allow(clippy::too_many_arguments)]
pub fn update_camera(
    fixed: Res<Time<Fixed>>,
    planet: Res<PlanetRes>,
    mut origin: ResMut<RenderOrigin>,
    mut view: ResMut<ViewState>,
    mut controls: ResMut<Controls>,
    players: Query<(&Player, &PlayerInterp)>,
    ships: Query<&BodyInterp, With<Ship>>,
    mut cam: Query<(&mut WorldPose, &mut DistanceFog), With<MainCamera>>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let f = fixed.overstep_fraction_f64();
    let Ok((pl, pi)) = players.single() else { return };
    let Ok(si) = ships.single() else { return };
    let Ok((mut pose, mut fog)) = cam.single_mut() else { return };
    if controls.take_tap(KeyCode::KeyO) {
        view.orbit = !view.orbit;
    }
    if view.orbit {
        let d = DVec3::new(view.orbit_pitch.cos() * view.orbit_yaw.sin(), view.orbit_pitch.sin(), view.orbit_pitch.cos() * view.orbit_yaw.cos());
        pose.pos = planet.centre + d * 15_000.0;
        let up = if d.y.abs() < 0.99 { DVec3::Y } else { DVec3::X };
        pose.rot = look_basis(-d, (up - d * up.dot(d)).normalize());
    } else if pl.seated {
        let (sp, sr) = si.at(f);
        pose.pos = sp + sr * CHASE_CAMERA_OFFSET;
        pose.rot = sr * DQuat::from_rotation_x(CHASE_CAMERA_PITCH_DEG.to_radians());
    } else {
        let feet = pi.prev.0.lerp(pi.curr.0, f);
        let up = pi.prev.1.lerp(pi.curr.1, f).normalize();
        let fwd = pi.prev.2.lerp(pi.curr.2, f).normalize();
        pose.pos = feet + up * EYE_HEIGHT;
        pose.rot = look_basis(fwd, up) * DQuat::from_rotation_x(pl.pitch);
    }
    origin.view = pose.pos;
    let density = planet.density_at(pose.pos) as f32;
    let sky = Color::srgb(0.02, 0.02, 0.05).mix(&Color::srgb(0.45, 0.62, 0.85), density);
    clear.0 = sky;
    fog.color = sky;
    fog.falloff = FogFalloff::Exponential { density: 0.00025 * density };
    ambient.brightness = 80.0 + 320.0 * density;
}

#[allow(clippy::too_many_arguments)]
pub fn update_hud(
    time: Res<Time<Real>>,
    planet: Res<PlanetRes>,
    origin: Res<RenderOrigin>,
    ring: Res<Ring>,
    terrain: Res<Terrain>,
    stats: Res<WalkStats>,
    mut view: ResMut<ViewState>,
    players: Query<&Player>,
    ships: Query<(&Ship, &avian3d::prelude::Position, &avian3d::prelude::LinearVelocity, &avian3d::prelude::Rotation)>,
    mut hud: Query<&mut Text, With<Hud>>,
    mut phys: ResMut<crate::PhysicsTiming>,
    net: Option<Res<crate::net_live::Net>>,
) {
    let phys_ms = std::mem::take(&mut phys.frame_ms);
    let ms = time.delta_secs_f64() * 1000.0;
    if view.skip_frames > 0 {
        view.skip_frames -= 1;
    } else {
        view.frame_ms.push(ms);
        if ms > 30.0 {
            eprintln!("LONG FRAME {ms:.1} ms: chunks visible {} pending {}, patches {} pending {}, ring system {:.2} ms, terrain max {:.2} ms, physics {phys_ms:.2} ms", terrain.visible, terrain.pending, ring.patches.len(), ring.pending(), ring.last_frame_ms, terrain.frame_ms_max);
        }
    }
    let (Ok(pl), Ok((ship, sp, sv, sr)), Ok(mut text)) = (players.single(), ships.single(), hud.single_mut()) else { return };
    let mode = if pl.seated {
        format!(
            "SHIP  assist {} (H)  follow {} (L)  {:.1} m/s  limit {:.0}  ground {:.0} m  alt {:.0} m",
            if ship.ctl.hover_assist { "on" } else { "off" },
            if ship.ctl.horizon_follow { "on" } else { "off" },
            sv.0.length(),
            ship.ctl.forward_speed_limit,
            planet.above_ground(sp.0),
            (sp.0 - planet.centre).length() - planet.radius,
        )
    } else {
        // Velocity relative to the planet centre, and its part along "up" (negative = towards the
        // planet): the cabin carries the ship's velocity (own ship), outside it is the walker's own.
        let (v, pos) = if pl.ship.is_some() { (sv.0 + sr.0 * pl.w.vel, sp.0) } else { (pl.w.vel, pl.w.pos) };
        let radial = v.dot(planet.up(pos));
        let speed = format!("speed {:.1} m/s, vertical {:+.1} m/s, altitude {:.0} m", v.length(), radial, (pos - planet.centre).length() - planet.radius);
        if pl.ship.is_some() {
            format!("in cabin  [F] sit at the seat  {speed}")
        } else if pl.fly {
            format!("FLY (V)  {speed}")
        } else {
            format!("walk  grounded {}  {speed}", pl.w.grounded)
        }
    };
    **text = format!(
        "{:.2} ms  {}\nchunks {} pending {} build max {:.2} ms | patches {} pending {} | rescues {} net-only {}\norigin shifts {} (max {:.3} ms), view {:.1} km from planet centre",
        ms, mode, terrain.visible, terrain.pending, terrain.build_ms_max, ring.patches.len(), ring.pending(), stats.rescues, stats.net_only,
        origin.shifts, origin.shift_ms_max, (origin.view - planet.centre).length() / 1000.0,
    );
    if let Some(n) = net {
        text.0.push('\n');
        text.0.push_str(&n.hud_line());
    }
}
