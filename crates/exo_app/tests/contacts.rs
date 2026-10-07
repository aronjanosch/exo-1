//! Ship-ship contact under client authority. Two independent Avian worlds, each owns one dynamic
//! ship and shows the other ship as a delayed kinematic proxy, head-on at 15 m/s. Who owns a
//! contact is open (spike 10 report); for now proxies do not touch ships at all.
use avian3d::physics_transform::PhysicsTransformConfig;
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use exo_app::ship::{add_hull, RemoteShip};
use exo_app::TICK;

struct World2 {
    app: App,
    own: Entity,
    ghost: Entity,
    history: Vec<(DVec3, DQuat)>,
}

fn world(own_x: f64, ghost_x: f64, speed: f64, ghost_layer: exo_app::Layer) -> World2 {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, bevy::asset::AssetPlugin::default(), bevy::mesh::MeshPlugin));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(TICK));
    app.insert_resource(Time::<Fixed>::from_duration(TICK));
    app.add_plugins(PhysicsPlugins::default())
        .insert_resource(Gravity(DVec3::ZERO))
        .insert_resource(PhysicsTransformConfig { transform_to_position: false, position_to_transform: false, ..default() });
    let (m, w, h, d) = (2000.0f32, 4.6f32, 3.2f32, 8.3f32);
    let own = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Position(DVec3::new(own_x, 100.0, 0.0)),
            Rotation::default(),
            LinearVelocity(DVec3::new(speed, 0.0, 0.0)),
            Mass(m),
            AngularInertia::new(Vec3::new(m / 12.0 * (h * h + d * d), m / 12.0 * (w * w + d * d), m / 12.0 * (w * w + h * h))),
            CenterOfMass(Vec3::new(0.0, 1.4, -0.3)),
            (NoAutoMass, NoAutoAngularInertia, NoAutoCenterOfMass),
            (LinearDamping(0.0), AngularDamping(0.0), SweptCcd::default(), SleepingDisabled),
            Transform::default(),
        ))
        .id();
    let ghost = app
        .world_mut()
        .spawn((RemoteShip { owner: 2, lag: 1.0 }, RigidBody::Kinematic, Position(DVec3::new(ghost_x, 100.0, 0.0)), Rotation::default(), SleepingDisabled, Transform::default()))
        .id();
    let mut commands = app.world_mut().commands();
    // The proxy hull layer is what the test varies.
    add_hull(&mut commands, own, exo_app::Layer::Ship);
    add_hull(&mut commands, ghost, ghost_layer);
    app.world_mut().flush();
    app.finish();
    app.cleanup();
    World2 { app, own, ghost, history: Vec::new() }
}

impl World2 {
    fn own_pose(&self) -> (DVec3, DQuat) {
        let w = self.app.world();
        (w.get::<Position>(self.own).unwrap().0, w.get::<Rotation>(self.own).unwrap().0)
    }
    fn own_vel(&self) -> DVec3 {
        self.app.world().get::<LinearVelocity>(self.own).unwrap().0
    }
    fn set_ghost(&mut self, pose: (DVec3, DQuat), vel: DVec3) {
        let mut e = self.app.world_mut().entity_mut(self.ghost);
        e.get_mut::<Position>().unwrap().0 = pose.0;
        e.get_mut::<Rotation>().unwrap().0 = pose.1;
        e.get_mut::<LinearVelocity>().map(|mut v| v.0 = vel);
    }
}

struct Outcome {
    contact_ticks: (i32, i32),
    final_vel: (f64, f64),
}

/// `lag` in ticks: how old the other ship's pose is in each world (spike 4: 9 and 3).
fn head_on_with(lag: (usize, usize), ticks: usize, layer: exo_app::Layer) -> Outcome {
    let mut w = [world(-12.0, 12.0, 15.0, layer), world(12.0, -12.0, -15.0, layer)];
    let mut first = [-1i32; 2];
    for i in 0..ticks {
        for k in 0..2 {
            let pose = w[k].own_pose();
            w[k].history.push(pose);
        }
        for k in 0..2 {
            let other = 1 - k;
            let l = if k == 0 { lag.0 } else { lag.1 };
            let h = &w[other].history;
            if h.len() > l {
                let pose = h[h.len() - 1 - l];
                let prev = if h.len() > l + 1 { h[h.len() - 2 - l] } else { pose };
                let vel = (pose.0 - prev.0) * 60.0;
                w[k].set_ghost(pose, vel);
            }
        }
        for k in 0..2 {
            w[k].app.update();
        }
        for k in 0..2 {
            // First tick the own ship's velocity differs from its start by more than 1 m/s (a
            // kinematic proxy can also push it back to its own speed in one tick, so a speed
            // threshold would miss that).
            if first[k] < 0 && (w[k].own_vel().x - if k == 0 { 15.0 } else { -15.0 }).abs() > 1.0 {
                first[k] = i as i32;
            }
        }
    }
    Outcome { contact_ticks: (first[0], first[1]), final_vel: (w[0].own_vel().x, w[1].own_vel().x) }
}

/// Decided for the first playable (initiator, 2026-10-07): no ship-ship contact. The proxy hull
/// is on `Layer::Remote`: ships fly through it, nothing changes their velocity.
#[test]
fn remote_hull_does_not_touch_ships() {
    let o = head_on_with((9, 3), 180, exo_app::Layer::Remote);
    assert_eq!(o.contact_ticks, (-1, -1), "no velocity change in either world");
    assert!((o.final_vel.0 - 15.0).abs() < 1e-6 && (o.final_vel.1 + 15.0).abs() < 1e-6, "{:?}", o.final_vel);
}
