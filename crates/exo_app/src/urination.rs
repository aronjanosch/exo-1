//! Local walker need and droplets in planet or cabin frames, including LAG and warp.
use crate::{env::PlanetRes, walker::{Player, CabinFloor, cabin_frame}};
use avian3d::prelude::{Position, Rotation, LinearVelocity, AngularVelocity, ColliderTransform, SpatialQuery, SpatialQueryFilter};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use walker_core::{Frame, bladder::Jet};
use walker_core::urine::{Emitter, Particle, Status, World};
use flight_core::PlanetEnv;
use crate::ship::{Ship, RemoteShip, cabin_contains};

#[derive(Component)]
pub struct UrineParticle(pub Particle, pub Option<Entity>);

#[derive(Resource, Default)]
pub struct UrineParticles {
    emitter: Emitter,
    pub emitted: u64,
    pub hits: u64,
    pub expired: u64,
}

struct ParticleWorld<'a, 'w, 's> {
    planet: &'a PlanetRes,
    spatial: &'a SpatialQuery<'w, 's>,
    cabins: Vec<Cabin>,
    /// The particle's simulation coordinates; None means world space.
    frame: Option<Frame>,
    filter: SpatialQueryFilter,
}

struct Cabin {
    entity: Entity,
    frame: Frame,
    lag: flight_core::Lag,
    velocity: DVec3,
    angular: DVec3,
}

impl World for ParticleWorld<'_, '_, '_> {
    fn gravity_at(&self, position: DVec3) -> DVec3 {
        let position = self.frame.map_or(position, |f| f.to_world(position));
        let gravity = match self.cabins.iter().find(|c| cabin_contains(c.frame.to_local(position), 0.0)) {
            Some(c) => crate::walker::cabin_gravity(&c.lag, &c.frame, self.planet, position),
            None => self.planet.gravity_at(position),
        };
        self.frame.map_or(gravity, |f| f.rot.inverse() * gravity)
    }
    fn hits(&self, from: DVec3, to: DVec3) -> bool {
        let from = self.frame.map_or(from, |f| f.to_world(from));
        let to = self.frame.map_or(to, |f| f.to_world(to));
        let motion = to - from;
        let distance = motion.length();
        if distance > 1e-12 {
            let direction = Dir3::new((motion / distance).as_vec3()).unwrap();
            if self.spatial.cast_ray(from, direction, distance, true, &self.filter).is_some() { return true; }
        }
        // Ground is still present when its asynchronous collision patch is not loaded.
        self.planet.above_ground(to) <= 0.0
    }
}

#[allow(clippy::too_many_arguments)]
pub fn step_particles(
    mut commands: Commands,
    time: Res<Time>,
    planet: Res<PlanetRes>,
    spatial: SpatialQuery,
    jet: Res<UrineJet>,
    mut state: ResMut<UrineParticles>,
    mut particles: Query<(Entity, &mut UrineParticle)>,
    players: Query<&Player>,
    ships: Query<(Entity, &Position, &Rotation, &LinearVelocity, &AngularVelocity, Option<&Ship>, Option<&RemoteShip>), Or<(With<Ship>, With<RemoteShip>)>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    warp: Res<crate::warp::WarpDrive>,
) {
    let dt = time.delta_secs_f64();
    let mut world = ParticleWorld {
        planet: &planet, spatial: &spatial,
        cabins: ships.iter().map(|(e, p, r, v, angular, own, remote)| Cabin {
            entity: e, frame: cabin_frame(e, (p, r), &floors),
            lag: own.map(|s| s.lag).unwrap_or_else(|| flight_core::Lag { level: remote.unwrap().lag, ..flight_core::Lag::full() }),
            velocity: if own.is_some() && warp.drive.phase.on_rails() { warp.drive.pose().map_or(v.0, |(_, v)| v) } else { v.0 },
            angular: angular.0,
        }).collect(),
        frame: None,
        filter: SpatialQueryFilter::from_mask([crate::Layer::World, crate::Layer::Ship, crate::Layer::Ramp, crate::Layer::Remote]),
    };
    for (e, mut p) in &mut particles {
        world.frame = p.1.and_then(|ship| world.cabins.iter().find(|c| c.entity == ship).map(|c| c.frame));
        if p.1.is_some() && world.frame.is_none() {
            // The menu can replace the walker and ship when starting a different slot.
            commands.entity(e).despawn();
            continue;
        }
        match p.0.step(dt, &world) {
            Status::Alive => {
                if let Some(c) = p.1.and_then(|ship| world.cabins.iter().find(|c| c.entity == ship))
                    && !cabin_contains(p.0.position, 0.0) {
                    p.0.previous = c.frame.to_world(p.0.previous);
                    p.0.position = c.frame.to_world(p.0.position);
                    p.0.velocity = c.frame.rot * p.0.velocity + c.velocity + c.angular.cross(p.0.position - c.frame.origin);
                    p.1 = None;
                }
            },
            Status::Hit => { state.hits += 1; commands.entity(e).despawn(); },
            Status::Expired => { state.expired += 1; commands.entity(e).despawn(); },
        }
    }
    // Cabin-local droplets ride their frame even at warp speed, like the walker and crates.
    let player = players.single().ok();
    let ship = player.and_then(|p| p.ship).filter(|e| world.cabins.iter().any(|c| c.entity == *e));
    world.frame = ship.and_then(|e| world.cabins.iter().find(|c| c.entity == e).map(|c| c.frame));
    let jet = jet.0.map(|j| world.frame.map_or(j, |f| Jet { start: f.to_local(j.start), direction: f.rot.inverse() * j.direction, up: f.rot.inverse() * j.up }));
    let carrier = player.map_or(DVec3::ZERO, |p| p.w.vel);
    for (mut p, remaining) in state.emitter.emit(jet, carrier, dt) {
        state.emitted += 1;
        match p.step(remaining, &world) {
            Status::Alive => { commands.spawn(UrineParticle(p, ship)); },
            Status::Hit => state.hits += 1,
            Status::Expired => state.expired += 1,
        }
    }
}

#[derive(Component, Default)]
pub struct Bladder(pub walker_core::bladder::Bladder);

/// Simulation pose also available in headless scenarios.
#[derive(Resource, Default)]
pub struct UrineJet(pub Option<Jet>);

pub fn step(
    time: Res<Time>,
    mut players: Query<(&Player, &mut Bladder)>,
    ships: Query<(&Position, &Rotation)>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    tuning: Res<crate::tuning::Tuning>,
    effects: Res<crate::ship::CameraEffects>,
    mut jet: ResMut<UrineJet>,
) {
    let Ok((p, mut bladder)) = players.single_mut() else { return };
    bladder.0.step(time.delta_secs_f64());
    jet.0 = None;
    if !bladder.0.urinating() { return; }
    let frame = p.ship.and_then(|e| ships.get(e).ok().map(|(p, r)| cabin_frame(e, (p, r), &floors))).unwrap_or(Frame::IDENTITY);
    let up = p.world_up(frame);
    let look = if p.seated {
        frame.rot * DQuat::from_rotation_y(effects.0.look.y) * DQuat::from_rotation_x(tuning.camera.chase_pitch_deg.to_radians() + effects.0.look.x) * DVec3::NEG_Z
    } else {
        p.world_look(frame)
    };
    jet.0 = Some(Jet::new(p.world_pos(frame), up, look));
}
