//! Crates outside near the walker as Avian rigid bodies (#210, spike 12). Three states: a body
//! (here), a `CrateBody` that sweeps (the cabin, the ramp, far or frozen crates) and a locked one
//! on the plates. `avian_pre` decides the state and puts gravity and the hold in as accelerations;
//! `avian_post` copies the pose back into `CrateBody`, so grab, interaction, budget and rendering
//! keep reading `Crate::body`, and hands a body entering the cabin back to `CrateBody`.
//! The hull is a solid obstacle for bodies; the ramp counts as ship: a crate on or over it stays a `CrateBody`.
use crate::cargo::{bottom_world, CargoStats, Crate};
use crate::env::PlanetRes;
use crate::ring::Ring;
use crate::ship::{cabin_contains, Ship};
use crate::walker::{cabin_frame, CabinFloor, Player};
use crate::Layer;
use avian3d::prelude::*;
use bevy::math::DVec3;
use bevy::prelude::*;
use walker_core::Frame;

/// Marks a crate that is an Avian body now.
#[derive(Component)]
pub struct AvianCrate {
    /// Velocity before the physics step.
    v_pre: DVec3,
    /// An impact under way: its direction, the speed going in and the speed along it after the last step.
    hit: Option<(DVec3, f64, f64)>,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<AvianStats>();
    app.add_systems(FixedUpdate, avian_pre.after(crate::grab::grab_step).before(crate::cargo::crate_step).in_set(crate::phases::Fx::Cargo));
    app.add_systems(FixedLast, avian_post.before(crate::cargo::record_crate_interp));
}

/// Counters for scenarios.
#[derive(Resource, Default, Debug)]
pub struct AvianStats {
    /// Crates that became bodies, and that went back to a `CrateBody` (cabin, ramp, far, no patch).
    pub wakes: u32,
    pub sleeps: u32,
    /// Tilt (rad) a body had when it entered the cabin and was set upright again.
    pub cabin_snaps: Vec<f64>,
    /// Bodies stepped this tick.
    pub bodies: u32,
    /// Largest drop of a body below the CPU ground (m) seen.
    pub worst_sink: f64,
    /// Height of a crate's bottom above the CPU ground at waking (m).
    pub wake_heights: Vec<(Entity, f64)>,
}

/// Within this distance of the walker a crate outside becomes a body (m).
const WAKE_RADIUS: f64 = 40.0;
/// Friction of the crate's collider (`grab.json` friction).
const FRICTION: f64 = 0.8;
/// How hard a held body is turned upright (1/s).
const UPRIGHT_GAIN: f64 = 6.0;
/// Below this speed (m/s) a body far from the walker goes back to a frozen `CrateBody`.
const FREEZE_SPEED: f64 = 0.05;
/// Speed (m/s) a body must lose in one step to count as hitting something; friction and the hold
/// slow it by much less (0.8 g is 0.1 m/s a step).
const IMPACT_STEP: f64 = 1.0;
/// A hit is over when a step takes away less than this (m/s) along its direction.
const IMPACT_SETTLED: f64 = 0.3;
/// Extra distance (m) a crate must be clear of the ramp before it becomes a body, so one held at
/// the edge does not flip every tick.
const RAMP_MARGIN: f64 = 0.4;

/// What a body is made of; removed again when the crate goes back to a `CrateBody`.
type BodyParts = (AvianCrate, RigidBody, Collider, ColliderDensity, LinearVelocity, AngularVelocity, CollisionLayers, Friction, Restitution, SweptCcd);

/// True if a crate centre `p` (ship space) is on, over or at the ramp (the wedge of `add_hull`) or
/// at the cabin's open end, within `margin` of it; `r` is the crate's largest half extent.
fn over_ramp(p: DVec3, r: f64, margin: f64) -> bool {
    let m = r + margin;
    p.x.abs() < 1.5 + m && p.z > 3.0 && p.z < 6.6 + m && p.y > -1.5 - m && p.y < 3.5 + m
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn avian_pre(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    ring: Res<Ring>,
    mut stats: ResMut<AvianStats>,
    players: Query<&Player>,
    ships: Query<(Entity, &Position, &Rotation), With<Ship>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    mut crates: Query<(Entity, &mut Crate, Option<&mut AvianCrate>, Option<Forces>)>,
) {
    let Ok(pl) = players.single() else { return };
    let Some((ship_e, sp, sr)) = ships.iter().next() else { return };
    let ship = cabin_frame(ship_e, (sp, sr), &floors);
    let wf = if pl.ship.is_some() { ship } else { Frame::IDENTITY };
    let walker = pl.world_pos(wf);
    stats.bodies = 0;
    for (e, mut c, marker, forces) in &mut crates {
        let outside = c.ship.is_none() && c.planet.is_none_or(|p| p == planet.id) && !c.locked;
        let near = c.body.pos.distance(walker) < WAKE_RADIUS;
        let patch = ring.has_patch_near(c.body.pos);
        let r = c.body.half.max_element();
        let at_ramp = |margin| over_ramp(ship.to_local(c.body.pos), r, margin);
        let up = planet.up(c.body.pos);
        let g = flight_core::PlanetEnv::gravity_at(planet.as_ref(), c.body.pos).length();
        match (marker, forces) {
            (None, _) if outside && near && patch && !at_ramp(RAMP_MARGIN) => {
                // Wake into a body with the crate's pose and velocity.
                let rot = c.body.rot();
                commands.entity(e).insert((
                    AvianCrate { v_pre: c.body.vel, hit: None },
                    RigidBody::Dynamic,
                    c.collider(),
                    // Density so the box's own volume gives the crate's mass (and its inertia).
                    ColliderDensity((c.body.mass / (8.0 * c.body.half.x * c.body.half.y * c.body.half.z)) as f32),
                    Position(c.body.pos),
                    Rotation(rot),
                    LinearVelocity(c.body.vel),
                    AngularVelocity::ZERO,
                    CollisionLayers::new(Layer::Crate, [Layer::World, Layer::Ship, Layer::Crate]),
                    Friction::new(FRICTION),
                    Restitution::new(0.0),
                    SweptCcd::default(),
                ));
                c.body.wake();
                c.g = g;
                // The hold of this tick goes with the sweep model; the next one is a body's.
                c.push = DVec3::ZERO;
                c.turn = 0.0;
                stats.wakes += 1;
                stats.wake_heights.push((e, planet.above_ground(c.body.pos) - c.body.half.y));
            }
            (Some(mut m), Some(mut f)) => {
                let moving = f.linear_velocity().length() > FREEZE_SPEED;
                if !outside || !patch || at_ramp(0.0) || (!near && !moving && c.push == DVec3::ZERO) {
                    // Back to a `CrateBody` (asleep if it rests), pose already copied by avian_post.
                    commands.entity(e).remove::<BodyParts>();
                    stats.sleeps += 1;
                    continue;
                }
                stats.bodies += 1;
                c.g = g;
                // Game code that placed the crate or set its velocity since avian_post (a scenario
                // hook, a throw) wins.
                if f.position().0.distance(c.body.pos) > 1e-6 {
                    commands.entity(e).insert(Position(c.body.pos)).remove::<Sleeping>();
                }
                if (c.body.vel - f.linear_velocity()).length() > 1e-9 {
                    *f.linear_velocity_mut() = c.body.vel;
                }
                m.v_pre = f.linear_velocity();
                // Planet gravity (does not wake a sleeping body), then the hold (wakes it).
                f.non_waking().apply_linear_acceleration(-up * g);
                let (push, turn) = (std::mem::take(&mut c.push), std::mem::take(&mut c.turn));
                if push != DVec3::ZERO || turn != 0.0 {
                    f.apply_linear_acceleration(push);
                    // Held: upright and turning about up.
                    *f.angular_velocity_mut() = c.body.up.cross(up) * UPRIGHT_GAIN + up * turn;
                }
            }
            _ => {}
        }
    }
}

/// After the physics step: pose back into `CrateBody`, the impact into the crate's condition; a
/// body whose bottom enters the cabin becomes a cabin crate again (set upright).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn avian_post(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    tuning: Res<crate::tuning::Tuning>,
    mut stats: ResMut<AvianStats>,
    mut cargo: ResMut<CargoStats>,
    ships: Query<(Entity, &Position, &Rotation, &LinearVelocity), With<Ship>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    mut crates: Query<(Entity, &mut Crate, &mut AvianCrate, &Position, &Rotation, &LinearVelocity, Has<Sleeping>)>,
) {
    let Some((ship_e, sp, sr, sv)) = ships.iter().next() else { return };
    let ship = cabin_frame(ship_e, (sp, sr), &floors);
    for (e, mut c, mut m, p, r, v, sleeping) in &mut crates {
        c.body.pos = p.0;
        c.body.forward = r.0 * DVec3::NEG_Z;
        c.body.up = r.0 * DVec3::Y;
        c.body.vel = v.0;
        c.body.asleep = sleeping;
        c.body.grounded = false;
        // The speed lost along the way it was going: a hit starts when a step takes away more than
        // `IMPACT_STEP` and ends when the slowing stops (a landing is braked over a few steps by the
        // contact margin). Impact speed is what it lost in between; sliding and the hold lose less.
        c.body.impact = 0.0;
        let speed = m.v_pre.length();
        match m.hit {
            None if speed > 1e-6 && speed - v.0.dot(m.v_pre / speed) > IMPACT_STEP => m.hit = Some((m.v_pre / speed, speed, v.0.dot(m.v_pre / speed))),
            Some((dir, into, along)) => {
                let now = v.0.dot(dir);
                if along - now < IMPACT_SETTLED {
                    c.body.impact = (into - now.max(0.0)).max(0.0);
                    m.hit = None;
                } else {
                    m.hit = Some((dir, into, now));
                }
            }
            None => {}
        }
        c.condition = (c.condition - grab_core::impact_loss(&tuning.grab, c.body.impact)).max(0.0);
        // Count sinking below the CPU height (no patch, tunnelling).
        stats.worst_sink = stats.worst_sink.max(-(planet.above_ground(p.0) - c.body.half.y * 0.5));
        if cabin_contains(bottom_world(&ship, &c.body), -0.2) {
            stats.cabin_snaps.push(c.body.up.angle_between(ship.rot * DVec3::Y));
            let before = c.body.vel;
            c.body.change_frame(&Frame::IDENTITY, &ship, -sv.0);
            c.body.up = DVec3::Y;
            c.ship = Some(ship_e);
            c.planet = None;
            cargo.handovers.push(crate::cargo::Handover { crate_e: e, out: false, before, after: sv.0 + ship.rot * c.body.vel, ship_vel: sv.0 });
            commands.entity(e).remove::<BodyParts>();
        }
    }
}
