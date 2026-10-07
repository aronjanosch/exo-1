//! Render origin. Physics runs in f64 world space (Avian `f64`), so bodies never move for an
//! origin shift. Only what the GPU sees (f32 `Transform`) is relative to `RenderOrigin`. The
//! origin jumps in whole metres when the view gets farther than `threshold` from it.
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

#[derive(Resource)]
pub struct RenderOrigin {
    pub origin: DVec3,
    /// 0 disables shifting (origin stays where it started).
    pub threshold: f64,
    pub shifts: u32,
    /// World position of the active camera, written by the view each frame.
    pub view: DVec3,
}

/// Static render entity at a fixed world position (terrain chunks, water, markers).
#[derive(Component)]
pub struct WorldPos(pub DVec3);

/// Moving render entity without a body (the camera): world pose, written every frame.
#[derive(Component, Default)]
pub struct WorldPose {
    pub pos: DVec3,
    pub rot: DQuat,
}

/// Pose of a body before and after the last physics step, for rendering between steps.
#[derive(Component, Default)]
pub struct BodyInterp {
    pub prev: (DVec3, DQuat),
    pub curr: (DVec3, DQuat),
}

impl BodyInterp {
    pub fn at(&self, f: f64) -> (DVec3, DQuat) {
        (self.prev.0.lerp(self.curr.0, f), self.prev.1.slerp(self.curr.1, f))
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        PostUpdate,
        (shift_origin, (sync_world_pos, sync_bodies, sync_world_pose))
            .chain()
            .before(TransformSystems::Propagate),
    );
    app.add_systems(FixedLast, record_interp);
}

fn record_interp(mut q: Query<(&Position, &Rotation, &mut BodyInterp)>) {
    for (p, r, mut i) in &mut q {
        i.prev = i.curr;
        i.curr = (p.0, r.0);
    }
}

fn sync_world_pose(origin: Res<RenderOrigin>, mut q: Query<(&WorldPose, &mut Transform)>) {
    for (wp, mut t) in &mut q {
        t.translation = (wp.pos - origin.origin).as_vec3();
        t.rotation = wp.rot.as_quat();
    }
}

fn shift_origin(mut origin: ResMut<RenderOrigin>, mut q: Query<(&WorldPos, &mut Transform)>) {
    if origin.threshold <= 0.0 || (origin.view - origin.origin).length() <= origin.threshold {
        return;
    }
    origin.origin = origin.view.round();
    let o = origin.origin;
    for (wp, mut t) in &mut q {
        t.translation = (wp.0 - o).as_vec3();
    }
    origin.shifts += 1;
}

fn sync_world_pos(origin: Res<RenderOrigin>, mut q: Query<(&WorldPos, &mut Transform), Added<WorldPos>>) {
    for (wp, mut t) in &mut q {
        t.translation = (wp.0 - origin.origin).as_vec3();
    }
}

/// Root rigid bodies: Transform from the f64 Position relative to the origin. Avian's own
/// Position <-> Transform sync is switched off in `build_app`.
pub fn sync_bodies(
    origin: Res<RenderOrigin>,
    fixed: Res<Time<Fixed>>,
    mut q: Query<(&Position, &Rotation, Option<&BodyInterp>, &mut Transform), (With<RigidBody>, Without<ChildOf>)>,
) {
    let f = fixed.overstep_fraction_f64();
    for (p, r, interp, mut t) in &mut q {
        let (pos, rot) = interp.map(|i| i.at(f)).unwrap_or((p.0, r.0));
        t.translation = (pos - origin.origin).as_vec3();
        t.rotation = rot.as_quat();
    }
}
