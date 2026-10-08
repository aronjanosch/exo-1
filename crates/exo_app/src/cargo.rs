//! Crates (milestone C, #80): `grab_core::CrateBody` on top of Avian shape casts. A crate lives in
//! a frame like the walker: the planet, or the own ship's cabin (ship-local, so a flying ship
//! carries it without a velocity of its own). Leaving or entering the cabin keeps the world pose
//! and hands over the velocity. While the warp drive holds the ship, crates in its cabin are held
//! too (the drive sets the ship pose at up to 1e6 m/s).
//!
//! Crates have no collider of their own: they sweep against the world, hulls and ramps, but the
//! walker walks through them and they do not stack yet.
use crate::env::PlanetRes;
use crate::ring::Ring;
use crate::ship::{cabin_contains, Ship};
use crate::walker::{cabin_frame, cabin_gravity, up_from, CabinFloor};
use crate::Layer;
use avian3d::character_controller::move_and_slide::DepenetrationConfig;
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use grab_core::{BoxWorld, CrateBody, CrateTable};
use walker_core::{Frame, Hit};

pub const CRATES: &str = include_str!("../../../content/cargo/crates.json");

/// The standard crate sizes (`content/cargo/crates.json`).
#[derive(Resource, Clone, Debug)]
pub struct Crates(pub CrateTable);

impl Default for Crates {
    fn default() -> Self {
        Crates(CrateTable::from_json(CRATES).unwrap_or_else(|e| panic!("cargo: {e}")))
    }
}

#[derive(Component)]
pub struct Crate {
    /// Row in the crate table.
    pub size: usize,
    pub body: CrateBody,
    /// Ship whose cabin the crate is in; None: the planet.
    pub ship: Option<Entity>,
    /// Acceleration from holders this tick (frame coordinates), set before `crate_step`, cleared by it.
    pub push: DVec3,
    /// Turn rate about up this tick (rad/s), set before `crate_step`, cleared by it.
    pub turn: f64,
    shape: Collider,
}

/// Pose in the crate's frame before and after the last step, for rendering between steps.
#[derive(Component, Default)]
pub struct CrateInterp {
    pub prev: (DVec3, DQuat),
    pub curr: (DVec3, DQuat),
}

/// A crate changed its frame (scenario checks).
#[derive(Clone, Copy, Debug)]
pub struct Handover {
    pub crate_e: Entity,
    /// True: out of the cabin onto the planet.
    pub out: bool,
    /// World velocity just before and just after the hand-over.
    pub before: DVec3,
    pub after: DVec3,
    pub ship_vel: DVec3,
}

#[derive(Resource, Default, Debug)]
pub struct CargoStats {
    pub steps: u64,
    /// Crate steps skipped because the crate slept.
    pub asleep: u64,
    /// Steps held to the ship by the warp drive.
    pub held: u64,
    /// The CPU height function caught a crate below the ground (no collision patch there).
    pub rescues: u32,
    pub handovers: Vec<Handover>,
}

/// A crate of size `size` at `pos` in its frame (`ship`: the cabin, None: the planet).
pub fn crate_bundle(table: &CrateTable, size: &str, ship: Option<Entity>, pos: DVec3, forward: DVec3) -> impl Bundle {
    let i = table.sizes.iter().position(|s| s.name == size).unwrap_or_else(|| panic!("no crate size {size}"));
    let s = &table.sizes[i];
    let body = CrateBody::new(s, pos, forward);
    let rot = body.rot();
    (
        Crate { size: i, shape: Collider::cuboid(s.extents[0], s.extents[1], s.extents[2]), body, ship, push: DVec3::ZERO, turn: 0.0 },
        CrateInterp { prev: (pos, rot), curr: (pos, rot) },
        Transform::default(),
        Visibility::default(),
    )
}

/// A crate on the cabin floor at (x, z) ship-local, resting.
pub fn cabin_floor_pos(table: &CrateTable, size: &str, x: f64, z: f64) -> DVec3 {
    let h = table.get(size).expect("crate size").extents[1] * 0.5;
    DVec3::new(x, 0.3 + h + 0.006, z)
}

struct AvianBoxWorld<'a, 'w, 's> {
    mas: &'a MoveAndSlide<'w, 's>,
    shape: &'a Collider,
    filter: SpatialQueryFilter,
}

impl BoxWorld for AvianBoxWorld<'_, '_, '_> {
    fn sweep(&self, center: DVec3, _half: DVec3, rot: DQuat, motion: DVec3) -> Option<Hit> {
        let len = motion.length();
        let dir = Dir3::new((motion / len).as_vec3()).ok()?;
        let cfg = ShapeCastConfig { max_distance: len, ignore_origin_penetration: true, ..default() };
        let hit = self.mas.spatial_query.cast_shape(self.shape, center, rot, dir, &cfg, &self.filter)?;
        Some(Hit { distance: hit.distance, normal: hit.normal1, velocity: DVec3::ZERO })
    }
    fn depenetrate(&self, center: DVec3, _half: DVec3, rot: DQuat) -> DVec3 {
        let cfg = DepenetrationConfig { skin_width: 0.002, ..default() };
        self.mas.depenetrate(self.shape, center, rot, &cfg, &self.filter)
    }
}

/// Bottom centre of a crate in ship space (the cabin box test uses the walker's feet rule).
fn bottom(b: &CrateBody) -> DVec3 {
    b.pos - DVec3::Y * b.half.y
}

#[allow(clippy::too_many_arguments)]
pub fn crate_step(
    time: Res<Time>,
    planet: Res<PlanetRes>,
    tuning: Res<crate::tuning::Tuning>,
    wd: Res<crate::warp::WarpDrive>,
    ring: Res<Ring>,
    mas: MoveAndSlide,
    mut stats: ResMut<CargoStats>,
    mut crates: Query<(Entity, &mut Crate)>,
    ships: Query<(Entity, &Ship, &Position, &Rotation, &LinearVelocity)>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
) {
    let dt = time.delta_secs_f64();
    let cfg = &tuning.grab;
    let Some((ship_e, ship, sp, sr, sv)) = ships.iter().next() else { return };
    let ship_frame = cabin_frame(ship_e, (sp, sr), &floors);
    let held = wd.drive.phase.holds_ship();
    let filter = SpatialQueryFilter::from_mask([Layer::World, Layer::Ship, Layer::Ramp]);
    for (e, mut c) in &mut crates {
        let c = c.as_mut();
        let (push, turn) = (std::mem::take(&mut c.push), std::mem::take(&mut c.turn));
        if c.ship.is_some() && held {
            stats.held += 1;
            continue;
        }
        if c.body.asleep && push == DVec3::ZERO && turn == 0.0 {
            stats.asleep += 1;
            continue;
        }
        stats.steps += 1;
        let world = AvianBoxWorld { mas: &mas, shape: &c.shape, filter: filter.clone() };
        let (frame, up, g) = match c.ship {
            Some(_) => {
                let at = ship_frame.to_world(c.body.pos);
                let gv = cabin_gravity(&ship.lag, &ship_frame, &planet, at);
                let up = ship_frame.rot.inverse() * up_from(gv, ship_frame.rot * DVec3::Y);
                (ship_frame, up, gv.length())
            }
            None => (Frame::IDENTITY, planet.up(c.body.pos), flight_core::PlanetEnv::gravity_at(planet.as_ref(), c.body.pos).length()),
        };
        c.body.step(cfg, &frame, up, g, push, turn, &world, dt);

        match c.ship {
            None => {
                // Safety net as for the walker: the CPU height function holds crates where no
                // collision patch exists (the ring follows only the walker and the ship).
                let rel = c.body.pos - planet.centre;
                let dir = rel.normalize();
                let floor = planet.surface(dir) + c.body.half.y;
                if rel.length() < floor - 0.05 || !ring.has_patch_near(c.body.pos) && rel.length() < floor {
                    stats.rescues += ring.has_patch_near(c.body.pos) as u32;
                    c.body.pos = planet.centre + dir * floor;
                    let v = c.body.vel;
                    c.body.vel = v - dir * v.dot(dir).min(0.0);
                    c.body.grounded = true;
                }
                if cabin_contains(bottom_world(&ship_frame, &c.body), -0.2) {
                    let before = c.body.vel;
                    c.body.change_frame(&Frame::IDENTITY, &ship_frame, -sv.0);
                    c.ship = Some(ship_e);
                    stats.handovers.push(Handover { crate_e: e, out: false, before, after: sv.0 + ship_frame.rot * c.body.vel, ship_vel: sv.0 });
                }
            }
            Some(_) if !cabin_contains(bottom(&c.body), 0.3) => {
                let before = sv.0 + ship_frame.rot * c.body.vel;
                c.body.change_frame(&ship_frame, &Frame::IDENTITY, sv.0);
                c.ship = None;
                stats.handovers.push(Handover { crate_e: e, out: true, before, after: c.body.vel, ship_vel: sv.0 });
            }
            Some(_) => {}
        }
    }
}

/// Bottom centre of a planet-frame crate in the ship's space.
fn bottom_world(ship: &Frame, b: &CrateBody) -> DVec3 {
    ship.to_local(b.pos - b.up * b.half.y)
}

pub fn record_crate_interp(mut q: Query<(&Crate, &mut CrateInterp)>) {
    for (c, mut i) in &mut q {
        i.prev = i.curr;
        i.curr = (c.body.pos, c.body.rot());
    }
}

/// World pose of a crate now (no interpolation), for checks.
pub fn crate_world(c: &Crate, ship_frame: &Frame) -> (DVec3, DQuat) {
    match c.ship {
        Some(_) => (ship_frame.to_world(c.body.pos), ship_frame.rot * c.body.rot()),
        None => (c.body.pos, c.body.rot()),
    }
}

/// Colour of each crate size: loud, so a crate reads from across a landing pad.
const CRATE_COLOURS: [(f32, f32, f32); 3] = [(0.95, 0.8, 0.15), (0.2, 0.75, 0.65), (0.6, 0.3, 0.85)];

pub fn add_crate_visuals(mut commands: Commands, q: Query<(Entity, &Crate), Added<Crate>>, table: Res<Crates>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for (e, c) in &q {
        let s = &table.0.sizes[c.size];
        let (r, g, b) = CRATE_COLOURS[c.size % CRATE_COLOURS.len()];
        let size = Vec3::new(s.extents[0] as f32, s.extents[1] as f32, s.extents[2] as f32);
        let body = materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.9, ..default() });
        let band = materials.add(StandardMaterial { base_color: Color::srgb(r * 0.35, g * 0.35, b * 0.35), perceptual_roughness: 0.9, ..default() });
        commands.entity(e).insert((Mesh3d(meshes.add(Cuboid::from_size(size))), MeshMaterial3d(body), crate::origin::WorldPose::default())).with_children(|p| {
            // Two dark bands around the crate, so it reads as a crate and shows its turn.
            for x in [-0.3f32, 0.3] {
                p.spawn((Mesh3d(meshes.add(Cuboid::from_size(Vec3::new(size.x * 0.08, size.y * 1.02, size.z * 1.02)))), MeshMaterial3d(band.clone()), Transform::from_xyz(x * size.x, 0.0, 0.0)));
            }
        });
    }
}

/// Crates in a cabin ride with the ship's interpolated pose; on the planet they interpolate alone.
pub fn update_crate_visuals(
    fixed: Res<Time<Fixed>>,
    ships: Query<&crate::origin::BodyInterp, With<Ship>>,
    mut q: Query<(&Crate, &CrateInterp, &mut crate::origin::WorldPose)>,
) {
    let f = fixed.overstep_fraction_f64();
    for (c, i, mut pose) in &mut q {
        let p = i.prev.0.lerp(i.curr.0, f);
        let r = i.prev.1.slerp(i.curr.1, f);
        (pose.pos, pose.rot) = match c.ship.and_then(|s| ships.get(s).ok()) {
            Some(si) => {
                let (sp, sr) = si.at(f);
                (sp + sr * p, sr * r)
            }
            None => (p, r),
        };
    }
}

/// Scenario watch on one crate: largest drift from its start in its frame and whether it ever
/// left the cabin, checked every tick (also during the steps that fly and warp).
#[derive(Resource)]
pub struct CrateWatch {
    pub e: Entity,
    pub start: DVec3,
    pub max_drift: f64,
    pub left_cabin: bool,
    pub ticks: u64,
}

pub fn crate_watch(mut watch: ResMut<CrateWatch>, crates: Query<&Crate>) {
    let Ok(c) = crates.get(watch.e) else { return };
    watch.ticks += 1;
    let drift = c.body.pos.distance(watch.start);
    watch.max_drift = watch.max_drift.max(drift);
    watch.left_cabin |= c.ship.is_none() || !cabin_contains(bottom(&c.body), 0.0);
}
