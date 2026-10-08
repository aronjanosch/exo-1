//! Camera, light, sky, HUD and ship visuals. Nothing here feeds back into the simulation.
use crate::env::PlanetRes;
use crate::origin::{BodyInterp, RenderOrigin, WorldPose};
use crate::ring::Ring;
use crate::ship::{Ship, ShipPart};
use crate::terrain::Terrain;
use crate::walker::{Player, WalkStats, EYE_HEIGHT};
use crate::controls::{Actions, Tap};
use bevy::camera::PerspectiveProjection;
use bevy::light::GlobalAmbientLight;
use bevy::math::{DQuat, DVec3};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use flight_core::{PlanetEnv, CHASE_CAMERA_OFFSET, CHASE_CAMERA_PITCH_DEG};
use warp_core::PlanetId;

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
pub struct Hud;

/// Sphere standing in for a planet that is too far for its terrain.
#[derive(Component)]
pub struct Impostor(pub PlanetId);

/// HUD marker of a planet: a box at its place on screen with name and distance (Star Citizen
/// marks every destination this way). The jump target's box is larger and coloured.
#[derive(Component)]
pub struct NavMarker(pub PlanetId);

#[derive(Component)]
pub struct NavLabel;

/// Marker in the sky where the drive's course runs while spooling and calibrating: point the
/// nose at it.
#[derive(Component)]
pub struct AimMarker;

/// Star streak of the tunnel look: its place on a tube around the course (angle, radius) and
/// phase along it. Placed in the world around the camera, along the direction of travel, so it
/// shows through the cabin window whichever way the walker looks.
#[derive(Component)]
pub struct Streak {
    angle: f32,
    radius: f32,
    phase: f32,
}

/// Distance from a planet's centre beyond which its impostor replaces the terrain (m). The
/// terrain is culled by the camera's far plane at about this distance.
pub const IMPOSTOR_FROM: f64 = 100_000.0;
/// Planet markers and the ground and altitude readout show only when the camera is farther
/// than this from a planet's centre / closer than this (the terrain is drawn closer).
pub const NEAR_PLANET: f64 = IMPOSTOR_FROM;
const STREAKS: usize = 160;

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
        Projection::Perspective(PerspectiveProjection { fov: 75f32.to_radians(), near: 0.05, far: 120_000.0, ..default() }),
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

/// One lit sphere per planet and the streaks of the tunnel look.
pub fn setup_warp_view(
    mut commands: Commands,
    sys: Res<crate::warp::SystemRes>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let sphere = meshes.add(Sphere::new(1.0).mesh().uv(48, 24));
    for (i, p) in sys.0.ids().zip(&sys.0.planets) {
        let c = p.color;
        commands.spawn((
            NavMarker(i),
            Node { position_type: PositionType::Absolute, ..default() },
            Visibility::Hidden,
            children![
                (Node { width: px(14), height: px(14), border: UiRect::all(px(2)), ..default() }, BorderColor::all(Color::WHITE)),
                (NavLabel, Text::new(p.name.clone()), TextLayout::no_wrap(), TextFont { font_size: FontSize::Px(13.0), ..default() }, Node { margin: UiRect::left(px(6)), ..default() }),
            ],
        ));
        commands.spawn((
            Impostor(i),
            Mesh3d(sphere.clone()),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(c[0], c[1], c[2]), perceptual_roughness: 0.9, ..default() })),
            Transform::default(),
            WorldPose::default(),
            Visibility::Hidden,
        ));
    }
    commands.spawn((
        AimMarker,
        Mesh3d(meshes.add(Torus::new(0.8, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::new(0.4, 3.0, 4.0, 1.0), unlit: true, ..default() })),
        Transform::default(),
        WorldPose::default(),
        Visibility::Hidden,
    ));
    let streak = meshes.add(Cuboid::new(0.04, 0.04, 1.0));
    let mat = materials.add(StandardMaterial { base_color: Color::WHITE, emissive: LinearRgba::new(6.0, 8.0, 12.0, 1.0), unlit: true, ..default() });
    for i in 0..STREAKS {
        // A cheap hash spreads them over the tube.
        let h = |k: f32| ((i as f32 * 12.9898 + k * 78.233).sin() * 43758.547).fract().abs();
        commands.spawn((
            Streak { angle: h(1.0) * std::f32::consts::TAU, radius: 3.0 + h(2.0) * 30.0, phase: h(3.0) },
            Mesh3d(streak.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::default(),
            WorldPose::default(),
            Visibility::Hidden,
        ));
    }
}

/// Planet markers on the HUD: every planet farther than `NEAR_PLANET`, at its place on screen,
/// with name and distance from the camera; the jump target (`System::effective_target` of the
/// selection, or the running jump's target) larger and coloured. Hidden behind the camera.
pub fn update_nav_markers(
    sys: Res<crate::warp::SystemRes>,
    wd: Res<crate::warp::WarpDrive>,
    origin: Res<RenderOrigin>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut markers: Query<(&NavMarker, &mut Node, &mut Visibility, &Children)>,
    mut boxes: Query<(&mut Node, &mut BorderColor), Without<NavMarker>>,
    mut labels: Query<(&mut Text, &mut TextColor), With<NavLabel>>,
) {
    let Ok((camera, cam_t)) = cam.single() else { return };
    let sys = &sys.0;
    let target = wd.drive.target.unwrap_or_else(|| sys.effective_target(wd.selected, origin.view));
    for (m, mut node, mut vis, children) in &mut markers {
        let p = sys.planet(m.0);
        let to = p.centre() - origin.view;
        let dist = to.length();
        // A point 1 km along the direction from where the camera was drawn (render space).
        let at = cam_t.translation() + (to / dist * 1000.0).as_vec3();
        let screen = camera.world_to_viewport(cam_t, at).ok().filter(|_| cam_t.forward().dot(to.as_vec3()) > 0.0);
        let Some(xy) = screen.filter(|_| dist > NEAR_PLANET) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let is_target = m.0 == target;
        let size = if is_target { 22.0 } else { 14.0 };
        let colour = if is_target { Color::srgb(0.4, 1.0, 0.9) } else { Color::srgba(1.0, 1.0, 1.0, 0.7) };
        node.left = px(xy.x - size * 0.5);
        node.top = px(xy.y - size * 0.5);
        *vis = Visibility::Inherited;
        for c in children.iter() {
            if let Ok((mut b, mut bc)) = boxes.get_mut(c) {
                b.width = px(size);
                b.height = px(size);
                *bc = BorderColor::all(colour);
            }
            if let Ok((mut t, mut tc)) = labels.get_mut(c) {
                t.0 = format!("{}{}  {}", if is_target { "> " } else { "" }, p.name, km_text(dist));
                tc.0 = colour;
            }
        }
    }
}

/// Distance for the HUD: km with thousands separators, metres below 10 km.
fn km_text(d: f64) -> String {
    if d < 10_000.0 {
        return format!("{d:.0} m");
    }
    let km = format!("{:.0}", d / 1000.0);
    let mut out = String::new();
    for (i, c) in km.chars().enumerate() {
        if i > 0 && (km.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out + " km"
}

/// Distant planets: each one is drawn as a sphere at its true direction, at most `IMPOSTOR_FROM`
/// away and scaled to keep its angular size (the camera's far plane stays small).
/// Things placed around the camera get a world pose, not a Transform: the render origin may still
/// move after this system (at 1000 km/s by several km a frame), `WorldPose` follows it.
pub fn update_impostors(
    sys: Res<crate::warp::SystemRes>,
    origin: Res<RenderOrigin>,
    mut q: Query<(&Impostor, &mut WorldPose, &mut Transform, &mut Visibility)>,
) {
    for (imp, mut pose, mut t, mut vis) in &mut q {
        let p = sys.0.planet(imp.0);
        let to = p.centre() - origin.view;
        let dist = to.length();
        if dist < IMPOSTOR_FROM {
            *vis = Visibility::Hidden;
            continue;
        }
        let shown = IMPOSTOR_FROM;
        pose.pos = origin.view + to / dist * shown;
        t.scale = Vec3::splat((p.radius * shown / dist) as f32);
        *vis = Visibility::Inherited;
    }
}

/// The course marker, 1 km ahead along the drive's path start, a ring facing the camera's side.
pub fn update_aim_marker(
    wd: Res<crate::warp::WarpDrive>,
    origin: Res<RenderOrigin>,
    mut q: Query<(&mut WorldPose, &mut Transform, &mut Visibility), With<AimMarker>>,
) {
    use warp_core::Phase;
    let Ok((mut pose, mut t, mut vis)) = q.single_mut() else { return };
    let dir = match (wd.drive.phase, wd.drive.path()) {
        (Phase::Spooling | Phase::Calibrating, Some(path)) => path.start_dir(),
        _ => {
            *vis = Visibility::Hidden;
            return;
        }
    };
    pose.pos = origin.view + dir * 1000.0;
    pose.rot = DQuat::from_rotation_arc(DVec3::Y, dir);
    t.scale = Vec3::splat(if wd.drive.warning { 14.0 } else { 10.0 });
    *vis = Visibility::Inherited;
}

/// Tunnel look keyed to the drive's speed: streaks on a tube around the camera along the
/// direction of travel, a tint of the sky.
pub fn update_tunnel(
    time: Res<Time>,
    wd: Res<crate::warp::WarpDrive>,
    origin: Res<RenderOrigin>,
    mut q: Query<(&Streak, &mut WorldPose, &mut Transform, &mut Visibility)>,
    mut clear: ResMut<ClearColor>,
) {
    let level = wd.drive.tunnel() as f32;
    let t = time.elapsed_secs();
    let course = wd.drive.pose().map(|(_, v)| v.normalize_or_zero()).filter(|d| *d != DVec3::ZERO);
    let (Some(dir), true) = (course, level > 0.0) else {
        for (_, _, _, mut vis) in &mut q {
            *vis = Visibility::Hidden;
        }
        return;
    };
    let rot = DQuat::from_rotation_arc(DVec3::NEG_Z, dir);
    for (s, mut pose, mut tf, mut vis) in &mut q {
        *vis = Visibility::Inherited;
        let z = 60.0 - ((s.phase + t * (0.6 + level)) % 1.0) * 180.0;
        let len = 2.0 + 60.0 * level;
        pose.pos = origin.view + rot * DVec3::new((s.angle.cos() * s.radius) as f64, (s.angle.sin() * s.radius) as f64, z as f64);
        pose.rot = rot;
        tf.scale = Vec3::new(1.0, 1.0, len);
    }
    if level > 0.0 {
        clear.0 = clear.0.mix(&Color::srgb(0.05, 0.10, 0.30), level * 0.8);
    }
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

/// O: orbit camera on or off (fixed step, where taps live).
pub fn orbit_toggle(mut actions: ResMut<Actions>, mut view: ResMut<ViewState>) {
    if actions.take_tap(Tap::OrbitCamera) {
        view.orbit = !view.orbit;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_camera(
    time: Res<Time>,
    fixed: Res<Time<Fixed>>,
    planet: Res<PlanetRes>,
    mut origin: ResMut<RenderOrigin>,
    mut view: ResMut<ViewState>,
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
    wd: Res<crate::warp::WarpDrive>,
    sys: Res<crate::warp::SystemRes>,
) {
    let (Ok(pl), Ok((ship, sp, sv, sr)), Ok(mut text)) = (players.single(), ships.single(), hud.single_mut()) else { return };
    // Ground and altitude only near the planet (millions of metres during a warp say nothing).
    let near = |p: DVec3| (p - planet.centre).length() < NEAR_PLANET;
    let mode = if pl.seated {
        let height = if near(sp.0) { format!("  ground {:.0} m  alt {:.0} m", planet.above_ground(sp.0), (sp.0 - planet.centre).length() - planet.radius) } else { String::new() };
        format!(
            "SHIP  assist {} (H)  follow {} (L)  {}{}  {} m/s  limit {:.0}{height}",
            if ship.ctl.hover_assist { "on" } else { "off" },
            if ship.ctl.horizon_follow { "on" } else { "off" },
            lag_text(ship),
            if ship.ctl.brake_active { "  BRAKE (X)" } else { "" },
            speed_text(sv.0.length()),
            ship.ctl.forward_speed_limit,
        )
    } else {
        // Velocity relative to the planet centre, and its part along "up" (negative = towards the
        // planet): the cabin carries the ship's velocity (own ship), outside it is the walker's own.
        let (v, pos) = if pl.ship.is_some() { (sv.0 + sr.0 * pl.w.vel, sp.0) } else { (pl.w.vel, pl.w.pos) };
        let speed = if near(pos) {
            let radial = v.dot(planet.up(pos));
            format!("speed {} m/s, vertical {:+.2} m/s, altitude {:.0} m", speed_text(v.length()), radial, (pos - planet.centre).length() - planet.radius)
        } else {
            format!("speed {} m/s", speed_text(v.length()))
        };
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
    text.0.push('\n');
    text.0.push_str(&warp_line(&wd, &sys.0, sp.0));
    if let Some(n) = net {
        text.0.push('\n');
        text.0.push_str(&n.hud_line());
    }
}

/// Quantum drive status: phase, gauge and aim while calibrating, speed on rails. Target and
/// distance come from one rule: the running jump's target, else `System::effective_target`.
fn warp_line(wd: &crate::warp::WarpDrive, sys: &warp_core::System, ship: DVec3) -> String {
    use warp_core::Phase;
    let d = &wd.drive;
    let id = d.target.unwrap_or_else(|| sys.effective_target(wd.selected, ship));
    let target = sys.planet(id).name.as_str();
    let dist = km_text(sys.planet(id).centre().distance(ship));
    match d.phase {
        Phase::Idle => {
            let refused = wd.last_abort.map(|a| format!("  last: {a:?}")).unwrap_or_default();
            format!("QUANTUM  ready  J: warp to {target} ({dist})  N: select{refused}")
        }
        Phase::Spooling => format!("QUANTUM  spooling {:.1}/{:.0} s  -> {target} ({dist})  aim {:.1} deg  (J cancels)", d.timer, d.cfg.spool_time, d.angle),
        Phase::Calibrating => format!(
            "QUANTUM  calibrating {:.0} %  aim {:.1} deg{}  -> {target} ({dist})  (J cancels)",
            d.gauge * 100.0,
            d.angle,
            if d.warning { "  WARNING: hold the course" } else { "" }
        ),
        Phase::PreRamp => format!("QUANTUM  engaging  -> {target} ({dist})"),
        Phase::RampUp | Phase::Cruise | Phase::RampDown => format!(
            "QUANTUM  {:?} stage {}  {:.0} km/s  tunnel {:.0} %  {dist} to {target}  (hold J: emergency exit{})",
            d.phase,
            d.stage(),
            d.speed() / 1000.0,
            d.tunnel() * 100.0,
            if d.exit_hold > 0.0 { format!(" {:.0} %", 100.0 * d.exit_hold / d.cfg.emergency_hold_time) } else { String::new() }
        ),
        Phase::EmergencyDrop => format!("QUANTUM  EMERGENCY EXIT  {:.0} km/s  {dist} to {target}", d.speed() / 1000.0),
        Phase::PostRampDown if d.drop_point().is_some() => "QUANTUM  dropped out".to_string(),
        Phase::PostRampDown => "QUANTUM  arrived".to_string(),
        Phase::Cooldown => format!("QUANTUM  cooldown {:.1} s", (d.cfg.cooldown - d.timer).max(0.0)),
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
