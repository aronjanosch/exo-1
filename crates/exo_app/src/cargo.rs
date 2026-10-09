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
pub const BUDGET: &str = include_str!("../../../content/cargo/budget.json");

/// The object budget (`content/cargo/budget.json`, #85).
#[derive(Resource, Clone, Debug)]
pub struct ObjectBudget(pub grab_core::budget::Budget);

impl Default for ObjectBudget {
    fn default() -> Self {
        ObjectBudget(grab_core::budget::Budget::from_json(BUDGET).unwrap_or_else(|e| panic!("cargo: {e}")))
    }
}

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
    /// Mag-locked on the cabin's plates (#84): part of the ship, not stepped. Grabbing unlocks.
    pub locked: bool,
    /// Planet a crate outside any cabin lies on (set on its first step there). Crates on another
    /// planet than the simulated one are frozen until the players come back (#85).
    pub planet: Option<warp_core::PlanetId>,
    /// Simulation time (s) anyone last touched it; None until the budget first sees it.
    pub touched: Option<f64>,
    /// Gravity at the crate in the last step (m/s², along `-body.up`), for the hold's compensation.
    pub g: f64,
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
    /// Crate steps skipped because the crate is locked on the plates.
    pub locked: u64,
    /// Crates that locked onto the plates.
    pub locks: u32,
    /// Steps the ramp field stopped a loose crate at the ramp edge.
    pub field_stops: u64,
    /// Steps the cabin safety net put a loose crate back inside the walls from more than 2 cm out.
    pub wall_catches: u64,
    /// Crate steps skipped because the crate lies on a planet the players left.
    pub frozen: u64,
    /// Crates the object budget removed.
    pub despawned: u32,
}

/// A crate of size `size` at `pos` in its frame (`ship`: the cabin, None: the planet).
impl Crate {
    /// Spike 12: the box collider, for the Avian body.
    pub fn shape_clone(&self) -> Collider {
        self.shape.clone()
    }
}

pub fn crate_bundle(table: &CrateTable, size: &str, ship: Option<Entity>, pos: DVec3, forward: DVec3) -> impl Bundle {
    let i = table.sizes.iter().position(|s| s.name == size).unwrap_or_else(|| panic!("no crate size {size}"));
    let s = &table.sizes[i];
    let body = CrateBody::new(s, pos, forward);
    let rot = body.rot();
    (
        Crate { size: i, shape: Collider::cuboid(s.extents[0], s.extents[1], s.extents[2]), body, ship, push: DVec3::ZERO, turn: 0.0, locked: false, planet: None, touched: None, g: 0.0 },
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

/// Lock grid (#84): floor plates in the cabin, ship space. One plate per smallest crate edge.
/// TODO(initiator): the plate area (rear part of the cabin, clear of the seat) is a starting value.
pub const PLATE: f64 = 0.5;
pub const GRID_X: (f64, f64) = (-1.5, 1.5);
pub const GRID_Z: (f64, f64) = (-1.0, 3.5);
/// Top of the cabin floor, ship space.
pub const FLOOR_Y: f64 = 0.3;
/// The ship's acceleration felt by loose crates in the cabin is capped at this (m/s²).
/// TODO(initiator): starting value; it keeps touchdown bumps from launching crates.
pub const INERTIA_CAP: f64 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Plate {
    #[default]
    Idle,
    /// Under a locked crate.
    Lit,
    /// Under a crate that rests partly off the grid: it cannot lock.
    Blocked,
}

/// State of every plate, row by row along z (`nx` plates per row).
#[derive(Resource, Debug, Clone)]
pub struct LockGrid {
    pub nx: usize,
    pub nz: usize,
    pub plates: Vec<Plate>,
}

impl Default for LockGrid {
    fn default() -> Self {
        let nx = ((GRID_X.1 - GRID_X.0) / PLATE).round() as usize;
        let nz = ((GRID_Z.1 - GRID_Z.0) / PLATE).round() as usize;
        LockGrid { nx, nz, plates: vec![Plate::Idle; nx * nz] }
    }
}

impl LockGrid {
    /// Centre of plate (i along x, j along z) on the floor, ship space.
    pub fn centre(i: usize, j: usize) -> DVec3 {
        DVec3::new(GRID_X.0 + (i as f64 + 0.5) * PLATE, FLOOR_Y, GRID_Z.0 + (j as f64 + 0.5) * PLATE)
    }
    pub fn count(&self, p: Plate) -> usize {
        self.plates.iter().filter(|x| **x == p).count()
    }
}

/// Corners of a crate's footprint on the cabin floor (x, z), ship space.
fn footprint(b: &CrateBody) -> [(f64, f64); 4] {
    let r = b.rot();
    let (ax, az) = (r * DVec3::X * b.half.x, r * DVec3::Z * b.half.z);
    [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)].map(|(sx, sz)| {
        let p = b.pos + ax * sx + az * sz;
        (p.x, p.z)
    })
}

fn on_grid((x, z): (f64, f64)) -> bool {
    let e = 1e-6;
    x >= GRID_X.0 - e && x <= GRID_X.1 + e && z >= GRID_Z.0 - e && z <= GRID_Z.1 + e
}

/// Is the point (x, z) under the crate's footprint?
fn under(b: &CrateBody, x: f64, z: f64) -> bool {
    let r = b.rot();
    let d = DVec3::new(x, b.pos.y, z) - b.pos;
    (d.dot(r * DVec3::X)).abs() < b.half.x && (d.dot(r * DVec3::Z)).abs() < b.half.z
}

/// Inner faces of the cabin, ship space: side walls at |x| = 2.0, front wall at z = -4.0, floor
/// top 0.3, ceiling 2.9 (the hull in `ship::add_hull`). The back (ramp) is open.
const CABIN_X: f64 = 2.0;
const CABIN_FRONT: f64 = -4.0;
const CABIN_CEILING: f64 = 2.9;

/// Puts a crate that got (partly) through a cabin wall back inside; returns how far it moved it.
fn cabin_clamp(b: &mut CrateBody) -> f64 {
    let r = b.rot();
    let ext = |axis: DVec3| (r * DVec3::X).dot(axis).abs() * b.half.x + (r * DVec3::Y).dot(axis).abs() * b.half.y + (r * DVec3::Z).dot(axis).abs() * b.half.z;
    // Back inside with a gap below the sweeps' 5 mm skin (so a crate at rest is left alone) but
    // above zero: at zero gap the next sweep starts "in" the wall and ignores it.
    let gap = 0.004;
    let (ex, ey, ez) = (ext(DVec3::X) + gap, ext(DVec3::Y), ext(DVec3::Z) + gap);
    let mut moved: f64 = 0.0;
    let mut clamp = |p: &mut f64, v: &mut f64, lo: f64, hi: f64| {
        if *p < lo {
            moved = moved.max(lo - *p);
            *p = lo;
            *v = v.max(0.0);
        } else if *p > hi {
            moved = moved.max(*p - hi);
            *p = hi;
            *v = v.min(0.0);
        }
    };
    let (mut px, mut vx) = (b.pos.x, b.vel.x);
    clamp(&mut px, &mut vx, -CABIN_X + ex, CABIN_X - ex);
    let (mut py, mut vy) = (b.pos.y, b.vel.y);
    // The floor without a gap: Avian's cast leaves crates resting about 1 mm above it.
    clamp(&mut py, &mut vy, FLOOR_Y + ey, CABIN_CEILING - ey - gap);
    let (mut pz, mut vz) = (b.pos.z, b.vel.z);
    clamp(&mut pz, &mut vz, CABIN_FRONT + ez, f64::MAX);
    b.pos = DVec3::new(px, py, pz);
    b.vel = DVec3::new(vx, vy, vz);
    moved
}

/// Snaps a crate resting fully on the grid to the plates: heading to a quarter turn, edges onto
/// plate lines. False if the snapped crate would not fit.
fn snap_to_grid(b: &mut CrateBody) -> bool {
    let f = b.forward - DVec3::Y * b.forward.y;
    let angle = f.x.atan2(-f.z);
    let q = (angle / std::f64::consts::FRAC_PI_2).round() * std::f64::consts::FRAC_PI_2;
    let fwd = DVec3::new(q.sin(), 0.0, -q.cos());
    let quarter = (q / std::f64::consts::FRAC_PI_2).round() as i64 % 2 != 0;
    let (ex, ez) = if quarter { (b.half.z, b.half.x) } else { (b.half.x, b.half.z) };
    let snap = |c: f64, half: f64, lo: f64, hi: f64| {
        let edge = lo + ((c - half - lo) / PLATE).round() * PLATE;
        (edge + half).clamp(lo + half, hi - half)
    };
    if 2.0 * ex > GRID_X.1 - GRID_X.0 + 1e-6 || 2.0 * ez > GRID_Z.1 - GRID_Z.0 + 1e-6 {
        return false;
    }
    b.forward = fwd;
    b.up = DVec3::Y;
    b.pos.x = snap(b.pos.x, ex, GRID_X.0, GRID_X.1);
    b.pos.z = snap(b.pos.z, ez, GRID_Z.0, GRID_Z.1);
    b.pos.y = FLOOR_Y + b.half.y + 0.006;
    b.vel = DVec3::ZERO;
    true
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
    mut grid: ResMut<LockGrid>,
    mut prev_vel: Local<Option<DVec3>>,
    mut crates: Query<(Entity, &mut Crate), Without<crate::avian_crates::AvianCrate>>,
    ships: Query<(Entity, &Ship, &Position, &Rotation, &LinearVelocity)>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
) {
    let dt = time.delta_secs_f64();
    let cfg = &tuning.grab;
    let Some((ship_e, ship, sp, sr, sv)) = ships.iter().next() else { return };
    let ship_frame = cabin_frame(ship_e, (sp, sr), &floors);
    // Held from the pre-ramp until the drive has braked the ship back down (PostRampDown).
    let held = wd.drive.phase.holds_ship() || wd.drive.phase == warp_core::Phase::PostRampDown;
    // The ship's acceleration, felt by loose crates in the cabin as a push the other way (#84).
    // Not while the drive holds the ship (it sets the pose) or on the first step after.
    let accel = match *prev_vel {
        Some(v) if !held => (sv.0 - v) / dt,
        _ => DVec3::ZERO,
    };
    *prev_vel = (!held).then_some(sv.0);
    let mut inertia = ship_frame.rot.inverse() * -accel;
    inertia.y = 0.0;
    let inertia = inertia.clamp_length_max(INERTIA_CAP);
    let filter = SpatialQueryFilter::from_mask([Layer::World, Layer::Ship, Layer::Ramp]);
    grid.plates.iter_mut().for_each(|p| *p = Plate::Idle);
    let mut on_plates: Vec<(CrateBody, Plate)> = Vec::new();
    for (e, mut c) in &mut crates {
        let c = c.as_mut();
        let (push, turn) = (std::mem::take(&mut c.push), std::mem::take(&mut c.turn));
        if c.locked {
            stats.locked += 1;
            stats.held += (c.ship.is_some() && held) as u64;
            on_plates.push((c.body.clone(), Plate::Lit));
            continue;
        }
        if c.ship.is_some() && held {
            stats.held += 1;
            continue;
        }
        if c.ship.is_none() {
            let here = planet.id;
            if *c.planet.get_or_insert(here) != here {
                stats.frozen += 1;
                continue;
            }
        }
        let felt = if c.ship.is_some() { inertia } else { DVec3::ZERO };
        if c.body.asleep && push == DVec3::ZERO && turn == 0.0 && felt.length() <= cfg.friction * c.g {
            stats.asleep += 1;
            if c.ship.is_some() {
                on_plates.push((c.body.clone(), Plate::Blocked));
            }
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
        c.g = g;
        c.body.step(cfg, &frame, up, g, push + felt, turn, &world, dt);

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
                    c.planet = None;
                    stats.handovers.push(Handover { crate_e: e, out: false, before, after: sv.0 + ship_frame.rot * c.body.vel, ship_vel: sv.0 });
                }
            }
            Some(_) => {
                // Ramp field: in flight a loose crate (nobody pushes it) does not slide out over
                // the ramp edge. TODO(initiator): a starting rule; the cabin has no rear wall.
                if !ship.lag.landed && push == DVec3::ZERO {
                    let r = c.body.rot();
                    let reach = (r * DVec3::X).z.abs() * c.body.half.x + (r * DVec3::Z).z.abs() * c.body.half.z;
                    let max_z = 4.0 - reach;
                    if c.body.pos.z > max_z {
                        c.body.pos.z = max_z;
                        c.body.vel.z = c.body.vel.z.min(0.0);
                        stats.field_stops += 1;
                    }
                }
                // Safety net: a loose crate stays inside the walls, floor and ceiling. Seen before
                // it existed: after a warp a crate pressed to the front wall at 23 m/s went through
                // it (a sweep that starts touching a wall ignores it). Corrections over 2 cm are
                // counted in `wall_catches`; smaller ones only keep resting contacts apart.
                if push == DVec3::ZERO && cabin_clamp(&mut c.body) > 0.02 {
                    stats.wall_catches += 1;
                }
                if !cabin_contains(bottom(&c.body), 0.3) {
                    let before = sv.0 + ship_frame.rot * c.body.vel;
                    c.body.change_frame(&ship_frame, &Frame::IDENTITY, sv.0);
                    c.ship = None;
                    c.planet = Some(planet.id);
                    stats.handovers.push(Handover { crate_e: e, out: true, before, after: c.body.vel, ship_vel: sv.0 });
                } else if c.body.asleep && push == DVec3::ZERO {
                    // At rest and let go: fully on the plates it locks and becomes part of the ship.
                    let corners = footprint(&c.body);
                    if corners.iter().all(|p| on_grid(*p)) && snap_to_grid(&mut c.body) {
                        c.locked = true;
                        stats.locks += 1;
                        on_plates.push((c.body.clone(), Plate::Lit));
                    } else {
                        on_plates.push((c.body.clone(), Plate::Blocked));
                    }
                }
            }
        }
    }
    // Lit under locked crates; red under crates resting partly on the grid.
    let nx = grid.nx;
    for (b, state) in &on_plates {
        let corners = footprint(b);
        if *state == Plate::Blocked && !corners.iter().any(|p| on_grid(*p)) {
            continue;
        }
        for (k, plate) in grid.plates.iter_mut().enumerate() {
            let c = LockGrid::centre(k % nx, k / nx);
            // A plate counts when its middle or any point near its corners is under the crate.
            let k = PLATE * 0.4;
            let hit = [(0.0, 0.0), (k, k), (k, -k), (-k, k), (-k, -k)].iter().any(|(dx, dz)| under(b, c.x + dx, c.z + dz));
            if hit && *plate != Plate::Lit {
                *plate = *state;
            }
        }
    }
}

/// Bottom centre of a planet-frame crate in the ship's space.
pub(crate) fn bottom_world(ship: &Frame, b: &CrateBody) -> DVec3 {
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

/// One crate under a scenario watch.
#[derive(Clone, Copy, Debug)]
pub struct Watched {
    pub e: Entity,
    /// Position in its frame when the watch started.
    pub start: DVec3,
    pub max_drift: f64,
    pub left_cabin: bool,
}

/// Scenario watch on crates: largest drift from their start in their frame and whether they ever
/// left the cabin, checked every tick (also during the steps that fly and warp).
#[derive(Resource, Default)]
pub struct CrateWatch {
    pub crates: Vec<Watched>,
    pub ticks: u64,
}

impl CrateWatch {
    pub fn get(&self, e: Entity) -> Watched {
        *self.crates.iter().find(|x| x.e == e).expect("watched crate")
    }
}

pub fn crate_watch(mut watch: ResMut<CrateWatch>, crates: Query<&Crate>) {
    watch.ticks += 1;
    for x in &mut watch.crates {
        let Ok(c) = crates.get(x.e) else { continue };
        x.max_drift = x.max_drift.max(c.body.pos.distance(x.start));
        x.left_cabin |= c.ship.is_none() || !cabin_contains(bottom(&c.body), 0.0);
    }
}

/// One floor plate's visual (#84), index into `LockGrid::plates`.
#[derive(Component)]
pub struct PlateVisual(pub usize);

/// Materials of the three plate states.
#[derive(Resource)]
pub struct PlateMaterials([Handle<StandardMaterial>; 3]);

/// Plates on the own ship's cabin floor: thin tiles with a gap, dim until something locks.
pub fn add_lock_plates(mut commands: Commands, ships: Query<Entity, Added<Ship>>, grid: Res<LockGrid>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let Some(ship) = ships.iter().next() else { return };
    let mat = |m: &mut Assets<StandardMaterial>, c: Color, glow: LinearRgba| m.add(StandardMaterial { base_color: c, emissive: glow, perceptual_roughness: 0.6, ..default() });
    let mats = [
        mat(&mut materials, Color::srgb(0.32, 0.34, 0.38), LinearRgba::BLACK),
        mat(&mut materials, Color::srgb(0.3, 0.95, 0.5), LinearRgba::rgb(0.2, 1.6, 0.5)),
        mat(&mut materials, Color::srgb(0.95, 0.25, 0.2), LinearRgba::rgb(1.6, 0.2, 0.15)),
    ];
    let mesh = meshes.add(Cuboid::new(PLATE as f32 - 0.06, 0.012, PLATE as f32 - 0.06));
    commands.entity(ship).with_children(|c| {
        for k in 0..grid.plates.len() {
            let p = LockGrid::centre(k % grid.nx, k / grid.nx);
            c.spawn((PlateVisual(k), Mesh3d(mesh.clone()), MeshMaterial3d(mats[0].clone()), Transform::from_xyz(p.x as f32, p.y as f32 + 0.006, p.z as f32)));
        }
    });
    commands.insert_resource(PlateMaterials(mats));
}

pub fn update_lock_plates(grid: Res<LockGrid>, mats: Option<Res<PlateMaterials>>, mut q: Query<(&PlateVisual, &mut MeshMaterial3d<StandardMaterial>)>) {
    let Some(mats) = mats else { return };
    for (p, mut m) in &mut q {
        let want = &mats.0[grid.plates[p.0] as usize];
        if m.0 != *want {
            m.0 = want.clone();
        }
    }
}

/// Object budget (#85): removes loose crates over the cap, untouched too long, drifting far
/// away, or beyond the persistence cap on a planet the players left. Held, locked and cabin
/// crates never go.
#[allow(clippy::too_many_arguments)]
pub fn budget_step(
    mut commands: Commands,
    time: Res<Time>,
    planet: Res<PlanetRes>,
    budget: Res<ObjectBudget>,
    grab: Res<crate::grab::Grab>,
    mut stats: ResMut<CargoStats>,
    players: Query<&crate::walker::Player>,
    ships: Query<(Entity, &Position, &Rotation), With<Ship>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    mut crates: Query<(Entity, &mut Crate)>,
) {
    let now = time.elapsed_secs_f64();
    let Some((ship_e, sp, sr)) = ships.iter().next() else { return };
    let frame = cabin_frame(ship_e, (sp, sr), &floors);
    let walker = players.single().map(|p| p.world_pos(frame)).unwrap_or(sp.0);
    let held = grab.held.map(|h| h.crate_e);
    let objs: Vec<grab_core::budget::Obj> = crates
        .iter_mut()
        .map(|(e, mut c)| {
            let touched = *c.touched.get_or_insert(now);
            let (pos, _) = crate_world(&c, &frame);
            grab_core::budget::Obj {
                id: e.to_bits(),
                protected: held == Some(e) || c.locked || c.ship.is_some(),
                idle: now - touched,
                distance: pos.distance(walker).min(pos.distance(sp.0)),
                resting: c.body.asleep,
                here: c.ship.is_some() || c.planet.is_none_or(|p| p == planet.id),
            }
        })
        .collect();
    for id in grab_core::budget::over_budget(&budget.0.crates, &objs) {
        commands.entity(Entity::from_bits(id)).despawn();
        stats.despawned += 1;
    }
}
