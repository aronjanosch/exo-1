//! Local avatar need and jet pose; runs even when seated or in debug flight.
use crate::{env::PlanetRes, walker::{Player, ship_frame}};
use avian3d::prelude::{Position, Rotation, LinearVelocity, AngularVelocity, SpatialQuery, SpatialQueryFilter};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use walker_core::{Frame, bladder::{Jet, look_direction}};
use walker_core::urine::{Emitter, Particle, Status, World};
use flight_core::PlanetEnv;
use crate::ship::{Ship, RemoteShip, cabin_contains};

#[derive(Component)]
pub struct UrineParticle(pub Particle);

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
    cabins: Vec<Frame>,
    filter: SpatialQueryFilter,
}

impl World for ParticleWorld<'_, '_, '_> {
    fn gravity_at(&self, position: DVec3) -> DVec3 {
        // Match the walker's existing artificial cabin gravity; elsewhere use the planetary field.
        match self.cabins.iter().find(|f| cabin_contains(f.to_local(position), 0.0)) {
            Some(f) => -(f.rot * DVec3::Y) * 9.81,
            None => self.planet.gravity_at(position),
        }
    }
    fn hits(&self, from: DVec3, to: DVec3) -> bool {
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
    ships: Query<(Entity, &Position, &Rotation, &LinearVelocity, &AngularVelocity), Or<(With<Ship>, With<RemoteShip>)>>,
) {
    let dt = time.delta_secs_f64();
    let world = ParticleWorld {
        planet: &planet, spatial: &spatial,
        cabins: ships.iter().map(|(_, p, r, ..)| ship_frame(p, r)).collect(),
        filter: SpatialQueryFilter::from_mask([crate::Layer::World, crate::Layer::Ship, crate::Layer::Ramp, crate::Layer::Remote]),
    };
    for (e, mut p) in &mut particles {
        match p.0.step(dt, &world) {
            Status::Alive => {},
            Status::Hit => { state.hits += 1; commands.entity(e).despawn(); },
            Status::Expired => { state.expired += 1; commands.entity(e).despawn(); },
        }
    }
    // 8 m/s relative to the moving emitter, including a cabin's translation and rotation.
    let carrier = players.single().ok().map(|p| {
        match p.ship.and_then(|e| ships.get(e).ok()) {
            Some((_, sp, sr, v, angular)) => sr.0 * p.w.vel + v.0 + angular.0.cross(jet.0.map(|j| j.start - sp.0).unwrap_or(DVec3::ZERO)),
            None => p.w.vel,
        }
    }).unwrap_or(DVec3::ZERO);
    for (mut p, remaining) in state.emitter.emit(jet.0, carrier, dt) {
        state.emitted += 1;
        match p.step(remaining, &world) {
            Status::Alive => { commands.spawn(UrineParticle(p)); },
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
    planet: Res<PlanetRes>,
    mut players: Query<(&Player, &mut Bladder)>,
    ships: Query<(&Position, &Rotation)>,
    mut jet: ResMut<UrineJet>,
) {
    let Ok((p, mut bladder)) = players.single_mut() else { return };
    bladder.0.step(time.delta_secs_f64());
    jet.0 = None;
    if !bladder.0.urinating() { return; }
    let frame = p.ship.and_then(|e| ships.get(e).ok()).map(|(p, r)| ship_frame(p, r)).unwrap_or(Frame::IDENTITY);
    let up = p.world_up(frame, &planet);
    let look = if p.seated {
        frame.rot * DQuat::from_rotation_x(flight_core::CHASE_CAMERA_PITCH_DEG.to_radians()) * DVec3::NEG_Z
    } else {
        let forward = if p.ship.is_some() { frame.rot * p.w.forward } else { p.w.forward };
        look_direction(forward, up, p.pitch)
    };
    jet.0 = Some(Jet::new(p.world_pos(frame), up, look));
}
