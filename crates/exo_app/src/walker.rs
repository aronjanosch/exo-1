//! The walker: walker_core on top of Avian spatial queries, frame changes between planet and
//! ship cabin, sit/stand, CPU safety net with rescue counting.
use crate::controls::Controls;
use crate::env::PlanetRes;
use crate::ring::Ring;
use crate::ship::{cabin_contains, RemoteShip, Ship, SEAT_POS};
use crate::Layer;
use avian3d::character_controller::move_and_slide::DepenetrationConfig;
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec2, DVec3};
use bevy::prelude::*;
use walker_core::{Frame, Hit, WalkInput, Walker, World};

const MOUSE_SENSITIVITY: f64 = 0.0025;
pub const EYE_HEIGHT: f64 = 1.7;

#[derive(Component)]
pub struct Player {
    pub w: Walker,
    /// Ship whose cabin the walker is in.
    pub ship: Option<Entity>,
    pub seated: bool,
    pub pitch: f64,
    /// Debug fly mode (V): no gravity, no collision.
    pub fly: bool,
}

/// Marks the cabin floor collider; its pose is the frame the walker queries in.
#[derive(Component)]
pub struct CabinFloor;

#[derive(Resource, Default, Debug, Clone)]
pub struct WalkStats {
    pub steps: u64,
    pub grounded: u64,
    pub rescues: u32,
    /// Steps where only the CPU height function held the walker (no patch under it).
    pub net_only: u64,
    pub uncovered: u64,
    pub depenetrations: u64,
}

struct AvianWorld<'a, 'w, 's> {
    mas: &'a MoveAndSlide<'w, 's>,
    shape: Collider,
    half_height: f64,
    filter: SpatialQueryFilter,
}

impl World for AvianWorld<'_, '_, '_> {
    fn sweep(&self, feet: DVec3, up: DVec3, motion: DVec3) -> Option<Hit> {
        let len = motion.length();
        let dir = Dir3::new((motion / len).as_vec3()).ok()?;
        let cfg = ShapeCastConfig { max_distance: len, ignore_origin_penetration: true, ..default() };
        let rot = DQuat::from_rotation_arc(DVec3::Y, up);
        let hit = self.mas.spatial_query.cast_shape(&self.shape, feet + up * self.half_height, rot, dir, &cfg, &self.filter)?;
        Some(Hit { distance: hit.distance, normal: hit.normal1 })
    }
    fn depenetrate(&self, feet: DVec3, up: DVec3) -> DVec3 {
        let rot = DQuat::from_rotation_arc(DVec3::Y, up);
        // Skin below walker_core's 1 cm gap: only real overlaps push, contacts at the gap do not.
        let cfg = DepenetrationConfig { skin_width: 0.002, ..default() };
        self.mas.depenetrate(&self.shape, feet + up * self.half_height, rot, &cfg, &self.filter)
    }
}

pub fn spawn_player(commands: &mut Commands, planet: &PlanetRes, offset_x: f64) -> Entity {
    let up = (DVec3::Y * planet.radius + DVec3::new(offset_x, 0.0, 0.0)).normalize();
    let pos = planet.centre + up * (planet.surface(up) + 2.0);
    commands.spawn((Player { w: Walker::new(pos, DVec3::NEG_Z), ship: None, seated: false, pitch: 0.0, fly: false }, crate::urination::Bladder::default())).id()
}

pub fn ship_frame(pos: &Position, rot: &Rotation) -> Frame {
    Frame { origin: pos.0, rot: rot.0 }
}

impl Player {
    /// Feet in world space, given the pose of the ship (used only when in the cabin).
    pub fn world_pos(&self, ship: Frame) -> DVec3 {
        if self.ship.is_some() { ship.to_world(self.w.pos) } else { self.w.pos }
    }
    /// Up in world space.
    pub fn world_up(&self, ship: Frame, planet: &PlanetRes) -> DVec3 {
        if self.ship.is_some() { ship.rot * DVec3::Y } else { planet.up(self.w.pos) }
    }
}

/// Frame of a ship (own or remote) as the cabin colliders have it. Avian moves child colliders to
/// the body pose only at the start of the next physics step (update_child_collider_position in
/// PhysicsStepSystems::First). Between steps the cabin colliders sit one tick behind the body
/// (6.7 m at 400 m/s), so the walker works in the frame the colliders are in. Local coordinates
/// are ship-relative either way.
fn cabin_frame(
    e: Entity,
    body: (&Position, &Rotation),
    floors: &Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
) -> Frame {
    match floors.iter().find(|(c, ..)| c.parent() == e) {
        Some((_, p, r, ct)) => {
            let rot = r.0 * ct.rotation.0.inverse();
            Frame { origin: p.0 - rot * ct.translation, rot }
        }
        None => ship_frame(body.0, body.1),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn walker_step(
    mut commands: Commands,
    time: Res<Time>,
    planet: Res<PlanetRes>,
    mut controls: ResMut<Controls>,
    mas: MoveAndSlide,
    mut ring: ResMut<Ring>,
    mut stats: ResMut<WalkStats>,
    mut players: Query<&mut Player>,
    mut ships: Query<(Entity, &mut Ship, &Position, &Rotation, &LinearVelocity)>,
    remotes: Query<(Entity, &Position, &Rotation, &LinearVelocity), With<RemoteShip>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
) {
    let dt = time.delta_secs_f64();
    let Ok(mut pl) = players.single_mut() else { return };
    let Some((ship_e, _, sp, sr, own_v)) = ships.iter().next().map(|(e, s, p, r, v)| (e, s.parked, *p, *r, *v)) else { return };
    let own_frame = cabin_frame(ship_e, (&sp, &sr), &floors);
    // The cabin the walker is in: the own ship, or the proxy of another player's ship.
    let cur_e = pl.ship.unwrap_or(ship_e);
    let (frame_ship, slv) = match remotes.get(cur_e) {
        Ok((e, p, r, v)) => (cabin_frame(e, (p, r), &floors), *v),
        Err(_) => (own_frame, own_v),
    };

    // F: sit at the seat or stand up.
    if controls.take_tap(KeyCode::KeyF) {
        let (_, mut ship, ..) = ships.get_mut(ship_e).unwrap();
        if pl.seated {
            pl.seated = false;
            ship.piloted = false; // hover assist now holds the ship
            pl.w.pos = DVec3::new(0.0, 0.32, SEAT_POS.z + 1.0);
            pl.w.vel = DVec3::ZERO;
        } else if pl.ship == Some(ship_e) && pl.w.pos.distance(SEAT_POS) < 1.8 {
            pl.seated = true;
            ship.piloted = true;
            if ship.parked {
                ship.parked = false;
                commands.entity(ship_e).insert(RigidBody::Dynamic);
            }
            pl.w.pos = SEAT_POS - DVec3::new(0.0, 0.3, 0.0);
            pl.w.vel = DVec3::ZERO;
        }
    }
    if controls.take_tap(KeyCode::KeyV) && !pl.seated {
        pl.fly = !pl.fly;
        pl.w.vel = DVec3::ZERO;
    }
    let world_pos = if pl.ship.is_some() { frame_ship.to_world(pl.w.pos) } else { pl.w.pos };
    ring.anchors = vec![(world_pos, if pl.ship.is_some() { slv.0 } else { pl.w.vel }), (sp.0, own_v.0)];
    if pl.seated {
        return;
    }

    let m = std::mem::take(&mut controls.mouse);
    let yaw = -m.x as f64 * MOUSE_SENSITIVITY;
    pl.pitch = (pl.pitch - m.y as f64 * MOUSE_SENSITIVITY).clamp(-1.5, 1.5);
    let input = WalkInput {
        dir: DVec2::new(controls.axis(KeyCode::KeyD, KeyCode::KeyA), controls.axis(KeyCode::KeyW, KeyCode::KeyS)),
        run: controls.pressed(KeyCode::ShiftLeft),
        jump: controls.pressed(KeyCode::Space),
        yaw,
    };

    if pl.fly {
        let up = planet.up(pl.w.pos);
        pl.w.align(up, yaw);
        let right = pl.w.forward.cross(up);
        let look = DQuat::from_axis_angle(right, pl.pitch) * pl.w.forward;
        let d = right * input.dir.x + look * input.dir.y + up * controls.axis(KeyCode::Space, KeyCode::ControlLeft);
        let speed = if input.run { 200.0 } else { 50.0 };
        if pl.ship.is_none() {
            pl.w.pos += d.normalize_or_zero() * speed * dt;
        }
        return;
    }

    let cfg = pl.w.cfg;
    let world = AvianWorld {
        mas: &mas,
        shape: Collider::capsule(cfg.radius, cfg.height - 2.0 * cfg.radius),
        half_height: cfg.height * 0.5,
        filter: SpatialQueryFilter::from_mask([Layer::World, Layer::Ship, Layer::Ramp, Layer::Remote]),
    };
    let (frame, up, g) = match pl.ship {
        // In the cabin gravity points to the cabin floor (spike 3 assumption), fallback 9.81.
        Some(_) => (frame_ship, DVec3::Y, 9.81),
        None => {
            let g = planet.as_ref();
            (Frame::IDENTITY, planet.up(pl.w.pos), flight_core::PlanetEnv::gravity_at(g, pl.w.pos).length())
        }
    };
    let info = pl.w.step(&frame, up, g, &input, &world, dt);
    stats.steps += 1;
    stats.grounded += pl.w.grounded as u64;
    stats.depenetrations += (info.depenetrated > 0.0) as u64;

    if pl.ship.is_none() {
        // Safety net: count real fall-throughs where a patch exists,
        // hold the feet on the CPU height function where none exists yet.
        let rel = pl.w.pos - planet.centre;
        let dist = rel.length();
        let dir = rel / dist;
        let surface = planet.surface(dir);
        let covered = ring.has_patch_near(pl.w.pos);
        if !covered {
            stats.uncovered += 1;
        }
        let mut snap = false;
        if dist < surface - 0.3 {
            snap = true;
            if covered {
                stats.rescues += 1;
            }
        } else if !covered && (dist < surface || (pl.w.grounded && dist - surface < 0.5 && !input.jump)) {
            snap = true;
            stats.net_only += 1;
        }
        if snap {
            pl.w.pos = planet.centre + dir * surface;
            let v = pl.w.vel;
            pl.w.vel = v - dir * v.dot(dir);
            pl.w.grounded = true;
        }
    }

    // Enter or leave the cabin: box test with hysteresis. The cabin can
    // be the own ship or the proxy of another player's ship (spike 4: walker in a foreign ship).
    match pl.ship {
        None => {
            let own = (ship_e, own_frame, own_v);
            let others = remotes.iter().map(|(e, p, r, v)| (e, cabin_frame(e, (p, r), &floors), *v));
            if let Some((e, f, v)) = std::iter::once(own).chain(others).find(|(_, f, _)| cabin_contains(f.to_local(pl.w.pos), -0.2)) {
                pl.w.change_frame(&Frame::IDENTITY, &f, -v.0);
                pl.ship = Some(e);
            }
        }
        Some(_) if !cabin_contains(pl.w.pos, 0.3) => {
            pl.w.change_frame(&frame_ship, &Frame::IDENTITY, slv.0);
            pl.ship = None;
        }
        _ => {}
    }
}
