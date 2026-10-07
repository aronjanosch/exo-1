//! Spike 10 step 6: the contact problem of spike 4 again. Two independent Avian worlds, each owns
//! one dynamic ship (client authority) and shows the other ship as a delayed kinematic proxy.
//! Head-on at 15 m/s. Nothing here decides who owns a contact; it only measures the disagreement.
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

fn world(own_x: f64, ghost_x: f64, speed: f64) -> World2 {
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
        .spawn((RemoteShip { owner: 2 }, RigidBody::Kinematic, Position(DVec3::new(ghost_x, 100.0, 0.0)), Rotation::default(), SleepingDisabled, Transform::default()))
        .id();
    let mut commands = app.world_mut().commands();
    add_hull(&mut commands, own);
    add_hull(&mut commands, ghost);
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

pub struct Outcome {
    pub lag: (usize, usize),
    pub contact_ticks: (i32, i32),
    pub max_disagreement: f64,
    pub final_vel: (f64, f64),
    pub final_x: (f64, f64),
    /// Where world 0 ends up with ship A against where world 1 shows it (proxy), at the end.
    pub final_a_disagreement: f64,
}

/// `lag` in ticks: how old the other ship's pose is in each world (spike 4: 9 and 3).
pub fn head_on(lag: (usize, usize), ticks: usize) -> Outcome {
    let mut w = [world(-12.0, 12.0, 15.0), world(12.0, -12.0, -15.0)];
    let mut first = [-1i32; 2];
    let mut max_dis = 0.0f64;
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
            let ghost = w[k].app.world().get::<Position>(w[k].ghost).unwrap().0;
            max_dis = max_dis.max(ghost.distance(w[1 - k].own_pose().0));
        }
    }
    // Ship A is owned by world 0 and shown as a proxy in world 1 (lag.1 ticks old).
    let a_auth = w[0].own_pose().0;
    let a_proxy = w[1].app.world().get::<Position>(w[1].ghost).unwrap().0;
    Outcome {
        lag,
        contact_ticks: (first[0], first[1]),
        max_disagreement: max_dis,
        final_vel: (w[0].own_vel().x, w[1].own_vel().x),
        final_x: (w[0].own_pose().0.x, w[1].own_pose().0.x),
        final_a_disagreement: a_auth.distance(a_proxy),
    }
}

#[test]
fn head_on_contact_disagreement() {
    let mut rows = Vec::new();
    for lag in [(0, 0), (3, 3), (9, 9), (9, 3), (18, 18), (18, 3)] {
        let o = head_on(lag, 180);
        println!(
            "lag {:?} ticks: contact at ticks {:?} ({} ticks apart), max proxy/authority disagreement {:.2} m, final vx {:.2} / {:.2}, final x {:.2} / {:.2}, A authority vs proxy {:.2} m",
            o.lag, o.contact_ticks, (o.contact_ticks.0 - o.contact_ticks.1).abs(), o.max_disagreement, o.final_vel.0, o.final_vel.1, o.final_x.0, o.final_x.1, o.final_a_disagreement
        );
        rows.push(o);
    }
    if let Ok(out) = std::env::var("CONTACTS_OUT") {
        let mut s = String::from("[\n");
        for (i, o) in rows.iter().enumerate() {
            s += &format!(
                "  {{\"lag_ticks\":[{},{}],\"contact_ticks\":[{},{}],\"max_disagreement_m\":{:.3},\"final_vx\":[{:.3},{:.3}],\"final_x\":[{:.3},{:.3}],\"final_a_authority_vs_proxy_m\":{:.3}}}{}\n",
                o.lag.0, o.lag.1, o.contact_ticks.0, o.contact_ticks.1, o.max_disagreement, o.final_vel.0, o.final_vel.1, o.final_x.0, o.final_x.1, o.final_a_disagreement,
                if i + 1 < rows.len() { "," } else { "" }
            );
        }
        s += "]\n";
        std::fs::write(out, s).unwrap();
    }
    // Both authorities detect the contact (spike 4: ticks 54 and 41).
    let spike4 = rows.iter().find(|o| o.lag == (9, 3)).unwrap();
    assert!(spike4.contact_ticks.0 >= 0 && spike4.contact_ticks.1 >= 0, "both worlds detect the contact");
}
