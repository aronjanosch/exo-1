//! Spike 12 (`spike/avian-crates`): crates outside, near the player, as Avian rigid bodies.
//! Three states (initiator, 2026-10-09): physics (Avian body, here), frozen (asleep `CrateBody`,
//! pose only) and part of the ship (`CrateBody` in the cabin). `avian_pre` decides the state and
//! applies gravity and the hold before the physics step; `avian_post` copies the pose back into
//! `CrateBody`, so grab, interaction and rendering keep reading `Crate::body`, and hands a crate
//! entering the cabin back to `CrateBody`. Spike code: throwaway.
use crate::cargo::{bottom_world, Crate, CargoStats};
use crate::env::PlanetRes;
use crate::ring::Ring;
use crate::ship::{cabin_contains, Ship};
use crate::walker::{cabin_frame, CabinFloor, Player};
use crate::Layer;
use avian3d::prelude::*;
use bevy::math::DVec3;
use bevy::prelude::*;
use walker_core::Frame;

/// Settings of the spike, from environment variables so a scenario run can compare.
#[derive(Resource, Clone, Debug)]
pub struct AvianCrates {
    /// `EXO_AVIAN_CRATES=0` switches the spike off (old model everywhere).
    pub on: bool,
    /// Within this distance of the walker a crate outside becomes a body (m).
    pub wake_r: f64,
    /// Only on a collision patch (`EXO_AVIAN_NO_PATCH_GUARD=1` drops the guard, for question 4).
    pub require_patch: bool,
    /// Friction of the crate's collider (matches `grab.json` friction 0.8).
    pub friction: f64,
}

impl AvianCrates {
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok();
        AvianCrates {
            on: var("EXO_AVIAN_CRATES").as_deref() != Some("0"),
            wake_r: var("EXO_AVIAN_WAKE_R").and_then(|v| v.parse().ok()).unwrap_or(40.0),
            require_patch: var("EXO_AVIAN_NO_PATCH_GUARD").as_deref() != Some("1"),
            friction: 0.8,
        }
    }
}

/// Marks a crate that is an Avian body now.
#[derive(Component)]
pub struct AvianCrate;

/// Spike measurements.
#[derive(Resource, Default, Debug)]
pub struct AvianStats {
    /// Crates that became bodies, and that went back (to the cabin or frozen).
    pub wakes: u32,
    pub sleeps: u32,
    /// Tilt (rad) a crate had when it entered the cabin and was set upright again.
    pub cabin_snaps: Vec<f64>,
    /// Bodies stepped this tick and their largest drop below the CPU ground (m) seen.
    pub bodies: u32,
    pub worst_sink: f64,
    /// Waking: crate centre height above the CPU ground at waking, and 1 s later (m).
    pub wake_heights: Vec<(Entity, f64)>,
}

/// How hard a held body is turned upright (1/s).
const UPRIGHT_GAIN: f64 = 6.0;
/// Below this speed (m/s) a body far from the walker goes back to a frozen `CrateBody`.
const FREEZE_SPEED: f64 = 0.05;

#[allow(clippy::type_complexity)]
pub fn avian_pre(
    mut commands: Commands,
    cfg: Res<AvianCrates>,
    planet: Res<PlanetRes>,
    ring: Res<Ring>,
    mut stats: ResMut<AvianStats>,
    players: Query<&Player>,
    ships: Query<(Entity, &Position, &Rotation), With<Ship>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    mut crates: Query<(Entity, &mut Crate, Option<Forces>)>,
    marked: Query<(), With<AvianCrate>>,
) {
    if !cfg.on {
        return;
    }
    let Ok(pl) = players.single() else { return };
    let Some((ship_e, sp, sr)) = ships.iter().next() else { return };
    let ship = cabin_frame(ship_e, (sp, sr), &floors);
    let wf = if pl.ship.is_some() { ship } else { Frame::IDENTITY };
    let walker = pl.world_pos(wf);
    stats.bodies = 0;
    for (e, mut c, forces) in &mut crates {
        let outside = c.ship.is_none() && c.planet.is_none_or(|p| p == planet.id) && !c.locked;
        let near = c.body.pos.distance(walker) < cfg.wake_r;
        let patch = !cfg.require_patch || ring.has_patch_near(c.body.pos);
        let is_body = marked.get(e).is_ok();
        let up = planet.up(c.body.pos);
        let g = flight_core::PlanetEnv::gravity_at(planet.as_ref(), c.body.pos).length();
        match (is_body, forces) {
            (false, _) if outside && near && patch => {
                // Wake into a body with the crate's pose and velocity.
                let rot = c.body.rot();
                commands.entity(e).insert((
                    AvianCrate,
                    RigidBody::Dynamic,
                    c.shape_clone(),
                    // Density so the box's own volume gives the crate's mass (and its inertia).
                    ColliderDensity((c.body.mass / (8.0 * c.body.half.x * c.body.half.y * c.body.half.z)) as f32),
                    Position(c.body.pos),
                    Rotation(rot),
                    LinearVelocity(c.body.vel),
                    AngularVelocity::ZERO,
                    CollisionLayers::new(Layer::Crate, [Layer::World, Layer::Ship, Layer::Ramp, Layer::Crate]),
                    Friction::new(cfg.friction),
                    Restitution::new(0.0),
                    SweptCcd::default(),
                ));
                c.body.wake();
                c.g = g;
                stats.wakes += 1;
                let h = planet.above_ground(c.body.pos) - c.body.half.y;
                stats.wake_heights.push((e, h));
            }
            (true, Some(mut f)) => {
                let moving = f.linear_velocity().length() > FREEZE_SPEED;
                if !outside || (!near && !moving && c.push == DVec3::ZERO) {
                    // Back to a `CrateBody` (frozen when asleep), pose already copied by avian_post.
                    commands.entity(e).remove::<(AvianCrate, RigidBody, Collider, ColliderDensity, LinearVelocity, AngularVelocity, CollisionLayers, Friction, Restitution, SweptCcd)>();
                    stats.sleeps += 1;
                    continue;
                }
                stats.bodies += 1;
                c.g = g;
                // Game code that set the velocity since avian_post (a throw) wins.
                if (c.body.vel - f.linear_velocity()).length() > 1e-9 {
                    *f.linear_velocity_mut() = c.body.vel;
                }
                // Planet gravity (does not wake a sleeping body), then the hold (wakes it).
                f.non_waking().apply_linear_acceleration(-up * g);
                let (push, turn) = (std::mem::take(&mut c.push), std::mem::take(&mut c.turn));
                if std::env::var("EXO_AVIAN_DEBUG").is_ok() && push != DVec3::ZERO {
                    let h = planet.above_ground(c.body.pos) - c.body.half.y;
                    let tilt = c.body.up.angle_between(up).to_degrees();
                    println!("avian-debug m={:.0} h={h:.3} v_up={:.3} push_up={:.2} push_side={:.2} g={g:.2} tilt={tilt:.1}", c.body.mass, f.linear_velocity().dot(up), push.dot(up), (push - up * push.dot(up)).length());
                }
                if push != DVec3::ZERO || turn != 0.0 {
                    f.apply_linear_acceleration(push);
                    // Held: upright and turning about up like the old crate.
                    let cu = c.body.up;
                    *f.angular_velocity_mut() = cu.cross(up) * UPRIGHT_GAIN + up * turn;
                }
            }
            _ => {}
        }
    }
}

/// After the physics step: pose back into `CrateBody`; a body whose bottom enters the cabin
/// becomes a cabin crate again (set upright).
#[allow(clippy::type_complexity)]
pub fn avian_post(
    mut commands: Commands,
    planet: Res<PlanetRes>,
    mut stats: ResMut<AvianStats>,
    mut cargo: ResMut<CargoStats>,
    ships: Query<(Entity, &Position, &Rotation, &LinearVelocity), With<Ship>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    mut crates: Query<(Entity, &mut Crate, &Position, &Rotation, &LinearVelocity, Has<Sleeping>), With<AvianCrate>>,
) {
    let Some((ship_e, sp, sr, sv)) = ships.iter().next() else { return };
    let ship = cabin_frame(ship_e, (sp, sr), &floors);
    for (e, mut c, p, r, v, sleeping) in &mut crates {
        c.body.pos = p.0;
        c.body.forward = r.0 * DVec3::NEG_Z;
        c.body.up = r.0 * DVec3::Y;
        c.body.vel = v.0;
        c.body.asleep = sleeping;
        c.body.grounded = false;
        // Ground under the body: count sinking below the CPU height (no patch, tunnelling).
        let sink = -(planet.above_ground(p.0) - c.body.half.y * 0.5);
        if sink > stats.worst_sink {
            stats.worst_sink = sink;
        }
        if cabin_contains(bottom_world(&ship, &c.body), -0.2) {
            let tilt = c.body.up.angle_between(ship.rot * DVec3::Y);
            stats.cabin_snaps.push(tilt);
            let before = c.body.vel;
            c.body.change_frame(&Frame::IDENTITY, &ship, -sv.0);
            c.body.up = DVec3::Y;
            c.ship = Some(ship_e);
            c.planet = None;
            cargo.handovers.push(crate::cargo::Handover { crate_e: e, out: false, before, after: sv.0 + ship.rot * c.body.vel, ship_vel: sv.0 });
            commands.entity(e).remove::<(AvianCrate, RigidBody, Collider, ColliderDensity, LinearVelocity, AngularVelocity, CollisionLayers, Friction, Restitution, SweptCcd)>();
        }
    }
}

/// Spike measurement: prints each new frame handover (velocity jump, world space) and the tilt
/// set upright at the cabin edge, with `EXO_HOLD_STATS`.
pub fn print_handovers(cargo: Res<CargoStats>, stats: Res<AvianStats>, mut seen: Local<(usize, usize)>) {
    if std::env::var("EXO_HOLD_STATS").is_err() {
        return;
    }
    for h in &cargo.handovers[seen.0..] {
        println!("handover out={} before={:.3} after={:.3} jump={:.4} m/s", h.out, h.before.length(), h.after.length(), (h.after - h.before).length());
    }
    for t in &stats.cabin_snaps[seen.1..] {
        println!("cabin-snap tilt={:.2} deg", t.to_degrees());
    }
    *seen = (cargo.handovers.len(), stats.cabin_snaps.len());
}

/// Spike measurement: per crate and tick, world position against the prediction from the last
/// tick (pos + vel dt) and the velocity change. Prints the worst values in the 5 ticks after a
/// state change (frame or body) next to the worst values elsewhere, with `EXO_HOLD_STATS`.
#[allow(clippy::type_complexity)]
pub fn track_continuity(
    time: Res<Time>,
    ships: Query<(Entity, &Position, &Rotation, &LinearVelocity), With<Ship>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    crates: Query<(Entity, &Crate, Has<AvianCrate>)>,
    mut last: Local<std::collections::HashMap<Entity, (DVec3, DVec3, bool, bool, u32)>>,
    mut worst: Local<(f64, f64)>,
) {
    if std::env::var("EXO_HOLD_STATS").is_err() {
        return;
    }
    let dt = time.delta_secs_f64();
    let Some((ship_e, sp, sr, sv)) = ships.iter().next() else { return };
    let ship = cabin_frame(ship_e, (sp, sr), &floors);
    for (e, c, body) in &crates {
        let (pos, vel) = match c.ship {
            Some(_) => (ship.to_world(c.body.pos), sv.0 + ship.rot * c.body.vel),
            None => (c.body.pos, c.body.vel),
        };
        let in_ship = c.ship.is_some();
        if let Some(&(p0, v0, s0, b0, since)) = last.get(&e) {
            let err = (pos - (p0 + v0 * dt)).length();
            let dv = (vel - v0).length();
            let changed = s0 != in_ship || b0 != body;
            let since = if changed { 0 } else { since + 1 };
            if since < 5 {
                println!("transition tick+{since} ship={in_ship} body={body} pos_err={err:.4} m dv={dv:.3} m/s");
            } else if !c.body.asleep {
                worst.0 = worst.0.max(err);
                worst.1 = worst.1.max(dv);
            }
            last.insert(e, (pos, vel, in_ship, body, since));
        } else {
            last.insert(e, (pos, vel, in_ship, body, 99));
        }
    }
    println!("continuity-elsewhere pos_err_max={:.4} dv_max={:.3}", worst.0, worst.1);
}

/// Spike 12: `EXO_AVIAN_RAMP_HIT=1` adds crates to the hull's and the ramp's filters.
pub fn ramp_hit() -> bool {
    std::env::var("EXO_AVIAN_RAMP_HIT").as_deref() == Ok("1")
}
