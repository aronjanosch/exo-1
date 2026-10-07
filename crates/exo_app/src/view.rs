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
use bevy::math::{DQuat, DVec3};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use flight_core::{PlanetEnv, CHASE_CAMERA_OFFSET, CHASE_CAMERA_PITCH_DEG};

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
pub struct Hud;

/// Walker feet, the camera's up and the look direction in world space before and after the last
/// fixed step.
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
    /// Cabin the walker was in last frame and whether it was weightless, to notice a frame change.
    cabin: (Option<Entity>, bool),
    /// Camera pose shown last frame.
    last_rot: DQuat,
    last_pos: DVec3,
    /// Blend after a frame change: rotation and eye offset from the new view to the old one, and
    /// its age (s). The eye sits on "up", so a turned up moves it too.
    horizon: Option<(DQuat, DVec3, f64)>,
}

/// Time the horizon takes to turn into the new frame (issue #7). Start value, tune by feel.
const HORIZON_BLEND_SECS: f64 = 0.4;

/// Share of the old view still shown: 1 at the change, eases to 0.
fn horizon_weight(age: f64) -> f64 {
    let x = (age / HORIZON_BLEND_SECS).clamp(0.0, 1.0);
    1.0 - x * x * (3.0 - 2.0 * x)
}

/// Rotation applied on top of the new view: starts at `offset` (the old view), eases to none.
fn horizon_offset(offset: DQuat, age: f64) -> DQuat {
    DQuat::IDENTITY.slerp(offset, horizon_weight(age))
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

/// Capsule for the walker of another player.
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
    mut commands: Commands,
) {
    let Ok((pl, interp, e)) = players.single_mut() else { return };
    let Some((own, p, r)) = ships.iter().next() else { return };
    // The cabin the walker is in can be a remote ship's proxy.
    let (p, r) = match pl.ship {
        Some(c) if c != own => remotes.get(c).unwrap_or((p, r)),
        _ => (p, r),
    };
    let frame = crate::walker::ship_frame(p, r);
    let now = (pl.world_pos(frame), pl.view_up, pl.world_look(frame));
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

#[allow(clippy::too_many_arguments)]
pub fn update_camera(
    time: Res<Time>,
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
        pose.rot = walker_core::look_rot(-d, up);
    } else if pl.seated {
        let (sp, sr) = si.at(f);
        pose.pos = sp + sr * CHASE_CAMERA_OFFSET;
        pose.rot = sr * DQuat::from_rotation_x(CHASE_CAMERA_PITCH_DEG.to_radians());
    } else {
        let feet = pi.prev.0.lerp(pi.curr.0, f);
        let up = pi.prev.1.lerp(pi.curr.1, f).normalize();
        let look = pi.prev.2.lerp(pi.curr.2, f).normalize();
        let pos = feet + up * EYE_HEIGHT;
        let rot = walker_core::look_rot(look, up);
        // Entering or leaving a cabin or the weightless body frame turns "up": the walker keeps
        // its look direction, the horizon turns over HORIZON_BLEND_SECS from where it was.
        if (pl.ship, pl.body.is_some()) != view.cabin {
            view.horizon = Some((view.last_rot * rot.inverse(), view.last_pos - pos, 0.0));
        }
        (pose.pos, pose.rot) = match view.horizon {
            Some((offset, eye, age)) if age < HORIZON_BLEND_SECS => {
                view.horizon = Some((offset, eye, age + time.delta_secs_f64()));
                (pos + eye * horizon_weight(age), horizon_offset(offset, age) * rot)
            }
            _ => {
                view.horizon = None;
                (pos, rot)
            }
        };
    }
    view.cabin = (pl.ship, pl.body.is_some());
    view.last_rot = pose.rot;
    view.last_pos = pose.pos;
    origin.view = pose.pos;
    let density = planet.density_at(pose.pos) as f32;
    let sky = Color::srgb(0.02, 0.02, 0.05).mix(&Color::srgb(0.45, 0.62, 0.85), density);
    clear.0 = sky;
    fog.color = sky;
    fog.falloff = FogFalloff::Exponential { density: 0.00025 * density };
    ambient.brightness = 80.0 + 320.0 * density;
}

/// Cabin gravity state; G works only while landed.
fn lag_text(ship: &Ship) -> String {
    let state = if ship.lag.is_on() { "on" } else { "off" };
    let key = if ship.lag.landed { " (G)" } else { "" };
    format!("gravity {state} {:.0} %{key}", ship.lag.level * 100.0)
}

/// Two decimals below 1 m/s, so a ship at rest can be told from a slow drift (issue #6).
fn speed_text(v: f64) -> String {
    if v < 1.0 { format!("{v:.2}") } else { format!("{v:.1}") }
}

#[allow(clippy::too_many_arguments)]
pub fn update_hud(
    time: Res<Time<Real>>,
    planet: Res<PlanetRes>,
    ring: Res<Ring>,
    terrain: Res<Terrain>,
    stats: Res<WalkStats>,
    players: Query<&Player>,
    ships: Query<(&Ship, &avian3d::prelude::Position, &avian3d::prelude::LinearVelocity, &avian3d::prelude::Rotation)>,
    mut hud: Query<&mut Text, With<Hud>>,
    net: Option<Res<crate::net_live::Net>>,
) {
    let (Ok(pl), Ok((ship, sp, sv, sr)), Ok(mut text)) = (players.single(), ships.single(), hud.single_mut()) else { return };
    let mode = if pl.seated {
        format!(
            "SHIP  assist {} (H)  follow {} (L)  {}{}  {} m/s  limit {:.0}  ground {:.0} m  alt {:.0} m",
            if ship.ctl.hover_assist { "on" } else { "off" },
            if ship.ctl.horizon_follow { "on" } else { "off" },
            lag_text(ship),
            if ship.ctl.brake_active { "  BRAKE (X)" } else { "" },
            speed_text(sv.0.length()),
            ship.ctl.forward_speed_limit,
            planet.above_ground(sp.0),
            (sp.0 - planet.centre).length() - planet.radius,
        )
    } else {
        // Velocity relative to the planet centre, and its part along "up" (negative = towards the
        // planet): the cabin carries the ship's velocity (own ship), outside it is the walker's own.
        let (v, pos) = if pl.ship.is_some() { (sv.0 + sr.0 * pl.w.vel, sp.0) } else { (pl.w.vel, pl.w.pos) };
        let radial = v.dot(planet.up(pos));
        let speed = format!("speed {} m/s, vertical {:+.2} m/s, altitude {:.0} m", speed_text(v.length()), radial, (pos - planet.centre).length() - planet.radius);
        if pl.ship.is_some() {
            format!("in cabin  [F] sit at the seat  {}  {speed}", lag_text(ship))
        } else if pl.fly {
            format!("FLY (V)  {speed}")
        } else if pl.body.is_some() {
            format!("SUIT  WASD Space/Ctrl thrust  Shift boost  Q/E roll  X brake  {speed}")
        } else {
            format!("walk  grounded {}  {speed}", pl.w.grounded)
        }
    };
    **text = format!(
        "{mode}\n{:.1} ms | chunks {} pending {} | patches {} pending {} | rescues {}",
        time.delta_secs_f64() * 1000.0,
        terrain.visible,
        terrain.pending,
        ring.patches.len(),
        ring.pending(),
        stats.rescues,
    );
    if let Some(n) = net {
        text.0.push('\n');
        text.0.push_str(&n.hud_line());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #7: the horizon starts where it was and ends in the new frame, without a step.
    #[test]
    fn horizon_blend_starts_at_the_old_view_and_ends_in_the_new() {
        let offset = DQuat::from_rotation_z(0.5);
        assert!(horizon_offset(offset, 0.0).angle_between(offset) < 1e-6);
        assert!(horizon_offset(offset, HORIZON_BLEND_SECS).angle_between(DQuat::IDENTITY) < 1e-6);
        let dt = 1.0 / 144.0;
        let mut age = 0.0;
        let mut prev = offset;
        while age < HORIZON_BLEND_SECS {
            age += dt;
            let q = horizon_offset(offset, age);
            assert!(q.angle_between(prev) < 0.5 * 1.5 * dt / HORIZON_BLEND_SECS + 1e-6, "step too large at {age}");
            prev = q;
        }
    }
}
