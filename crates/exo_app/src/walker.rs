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
use walker_core::{suit_accel, Frame, Hit, SuitConfig, SuitInput, WalkInput, Walker, World};

const MOUSE_SENSITIVITY: f64 = 0.0025;
/// Largest look angle above or below the horizon, radians.
const PITCH_LIMIT: f64 = 1.5;
/// Suit roll rate (Q/E), rad/s. Assumed value.
const SUIT_ROLL_RATE: f64 = 1.5;
/// Righting the view after leaving a tilted cabin in gravity: its up turns to the planet's with
/// this time constant (s), at most `RIGHTING_MAX_RATE` rad/s. Assumed values.
const RIGHTING_TIME: f64 = 0.5;
const RIGHTING_MAX_RATE: f64 = std::f64::consts::FRAC_PI_2;
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
    /// Body orientation while weightless outside a cabin (issue #8), world space, camera axes
    /// (-z looks, +y is the head). Free in all axes; `w.forward` follows it, `pitch` is 0.
    pub body: Option<DQuat>,
    /// Up in the cabin, cabin coordinates: the floor's up with LAG on, the planet's with LAG off.
    pub cabin_up: DVec3,
    /// Up outside a cabin, world space: the planet's (gravity and capsule follow it).
    pub up: DVec3,
    /// The camera's up, world space. Follows `world_up`, except after leaving a tilted cabin in
    /// gravity: then it starts at the cabin's up and rights itself (`RIGHTING_TIME`).
    pub view_up: DVec3,
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
    commands.spawn(Player { w: Walker::new(pos, DVec3::NEG_Z), ship: None, seated: false, pitch: 0.0, fly: false, body: None, cabin_up: DVec3::Y, up, view_up: up }).id()
}

pub fn ship_frame(pos: &Position, rot: &Rotation) -> Frame {
    Frame { origin: pos.0, rot: rot.0 }
}

impl Player {
    /// Feet in world space, given the pose of the ship (used only when in the cabin).
    pub fn world_pos(&self, ship: Frame) -> DVec3 {
        if self.ship.is_some() { ship.to_world(self.w.pos) } else { self.w.pos }
    }
    /// Up in world space as gravity and the capsule see it (the head direction while weightless).
    pub fn world_up(&self, ship: Frame) -> DVec3 {
        match (self.ship, self.body) {
            (Some(_), _) => ship.rot * self.cabin_up,
            (None, Some(b)) => b * DVec3::Y,
            (None, None) => self.up,
        }
    }
    /// Where the walker looks, world space.
    pub fn world_look(&self, ship: Frame) -> DVec3 {
        let fwd = if self.ship.is_some() { ship.rot * self.w.forward } else { self.w.forward };
        walker_core::look_dir(fwd, self.world_up(ship), self.pitch)
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

/// Gravity in a cabin, world space. `lag` is None for another player's ship (its LAG state is not
/// sent yet, issue #11): full ship gravity.
fn cabin_gravity(lag: Option<&flight_core::Lag>, frame: &Frame, planet: &PlanetRes, at: DVec3) -> DVec3 {
    lag.copied().unwrap_or_else(flight_core::Lag::full).gravity(frame.rot * DVec3::Y, flight_core::PlanetEnv::gravity_at(planet, at))
}

/// Up from a gravity vector (world space); weightless keeps `fallback`.
fn up_from(g: DVec3, fallback: DVec3) -> DVec3 {
    if g.length_squared() > 1e-12 { -g.normalize() } else { fallback }
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
    let Some((ship_e, own_lag, sp, sr, own_v)) = ships.iter().next().map(|(e, s, p, r, v)| (e, s.lag, *p, *r, *v)) else { return };
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
            pl.w.halt();
        } else if pl.ship == Some(ship_e) && pl.w.pos.distance(SEAT_POS) < 1.8 {
            pl.seated = true;
            ship.piloted = true;
            if ship.parked {
                ship.parked = false;
                commands.entity(ship_e).insert(RigidBody::Dynamic);
            }
            pl.w.pos = SEAT_POS - DVec3::new(0.0, 0.3, 0.0);
            pl.w.halt();
        }
    }
    // G: cabin gravity by hand, in the own cabin (the ship allows it only while landed).
    if pl.ship == Some(ship_e) && controls.take_tap(KeyCode::KeyG) {
        ships.get_mut(ship_e).unwrap().1.lag.toggle();
    }
    if controls.take_tap(KeyCode::KeyV) && !pl.seated {
        pl.fly = !pl.fly;
        pl.w.halt();
    }
    let world_pos = if pl.ship.is_some() { frame_ship.to_world(pl.w.pos) } else { pl.w.pos };
    ring.anchors = vec![(world_pos, if pl.ship.is_some() { slv.0 } else { pl.w.vel }), (sp.0, own_v.0)];
    if pl.seated {
        return;
    }

    let m = std::mem::take(&mut controls.mouse);
    let yaw = -m.x as f64 * MOUSE_SENSITIVITY;
    let pitch = -m.y as f64 * MOUSE_SENSITIVITY;

    // Weightless outside a cabin: the body turns freely and the suit thrusters move it (issue #8).
    let in_gravity = |at: DVec3| flight_core::PlanetEnv::gravity_at(planet.as_ref(), at) != DVec3::ZERO;
    let weightless = pl.ship.is_none() && !pl.fly && !in_gravity(world_pos);
    if pl.ship.is_none() {
        pl.up = planet.up(pl.w.pos);
    }
    match (weightless, pl.body) {
        // The body takes the view as it is (also the tilt of the cabin just left).
        (true, None) => {
            let b = walker_core::look_rot(pl.world_look(Frame::IDENTITY), pl.view_up);
            pl.body = Some(b);
            pl.w.forward = b * DVec3::NEG_Z;
            pl.pitch = 0.0;
        }
        // Back in gravity: walk about the planet's up, the view rights itself from the head's up.
        (false, Some(b)) => {
            pl.view_up = b * DVec3::Y;
            let up = pl.up;
            keep_look(&mut pl, b * DVec3::NEG_Z, &Frame::IDENTITY, up);
        }
        _ => {}
    }
    let input = if let Some(b) = pl.body {
        let roll = controls.axis(KeyCode::KeyQ, KeyCode::KeyE) * SUIT_ROLL_RATE * dt;
        let b = walker_core::turn_body(b, yaw, pitch, roll);
        pl.body = Some(b);
        pl.w.forward = b * DVec3::NEG_Z;
        let suit = SuitInput {
            thrust: DVec3::new(
                controls.axis(KeyCode::KeyD, KeyCode::KeyA),
                controls.axis(KeyCode::Space, KeyCode::ControlLeft),
                -controls.axis(KeyCode::KeyW, KeyCode::KeyS),
            ),
            boost: controls.pressed(KeyCode::ShiftLeft),
            brake: controls.pressed(KeyCode::KeyX),
        };
        WalkInput { accel: suit_accel(&SuitConfig::default(), b, pl.w.vel, &suit), ..default() }
    } else {
        pl.pitch = (pl.pitch + pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
        WalkInput {
            dir: DVec2::new(controls.axis(KeyCode::KeyD, KeyCode::KeyA), controls.axis(KeyCode::KeyW, KeyCode::KeyS)),
            run: controls.pressed(KeyCode::ShiftLeft),
            jump: controls.pressed(KeyCode::Space),
            yaw,
            ..default()
        }
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
            pl.view_up = up;
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
        // In the cabin: LAG towards the floor, mixed with the planet's while it comes up or goes down.
        Some(e) => {
            let g = cabin_gravity((e == ship_e).then_some(&own_lag), &frame_ship, &planet, frame_ship.to_world(pl.w.pos));
            pl.cabin_up = frame_ship.rot.inverse() * up_from(g, frame_ship.rot * DVec3::Y);
            (frame_ship, pl.cabin_up, g.length())
        }
        // Weightless the capsule stands along the body.
        None => {
            let g = planet.as_ref();
            (Frame::IDENTITY, pl.world_up(Frame::IDENTITY), flight_core::PlanetEnv::gravity_at(g, pl.w.pos).length())
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
                let look = pl.world_look(Frame::IDENTITY);
                let up = up_from(cabin_gravity((e == ship_e).then_some(&own_lag), &f, &planet, pl.w.pos), f.rot * DVec3::Y);
                pl.w.change_frame(&Frame::IDENTITY, &f, -v.0);
                pl.ship = Some(e);
                pl.cabin_up = f.rot.inverse() * up;
                keep_look(&mut pl, look, &f, up);
                pl.view_up = up;
            }
        }
        Some(_) if !cabin_contains(pl.w.pos, 0.3) => {
            // The view keeps the cabin's up (no step); weightless the body takes it, in gravity the
            // view rights itself below. The walker itself walks about the planet's up.
            let look = pl.world_look(frame_ship);
            pl.view_up = frame_ship.rot * pl.cabin_up;
            pl.w.change_frame(&frame_ship, &Frame::IDENTITY, slv.0);
            pl.ship = None;
            pl.up = planet.up(pl.w.pos);
            let up = pl.up;
            keep_look(&mut pl, look, &Frame::IDENTITY, up);
        }
        Some(_) => pl.view_up = frame_ship.rot * pl.cabin_up,
    }
    // Outside a cabin in gravity the view rights itself; weightless it is the body's. Just out of
    // a cabin into zero gravity it stays as it is until the body takes it on the next step.
    if pl.ship.is_none() {
        if let Some(b) = pl.body {
            pl.view_up = b * DVec3::Y;
        } else if in_gravity(pl.w.pos) {
            pl.view_up = walker_core::turn_towards(pl.view_up, pl.up, dt, RIGHTING_TIME, RIGHTING_MAX_RATE);
        }
    }
}

/// After a frame change: heading and pitch about the new up (world space), so the walker keeps
/// looking where it looked (issue #7).
fn keep_look(pl: &mut Player, look: DVec3, frame: &Frame, up: DVec3) {
    let (f, pitch) = walker_core::split_look(look, up, frame.rot * pl.w.forward);
    pl.w.forward = frame.rot.inverse() * f;
    pl.pitch = pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
    pl.body = None;
}
