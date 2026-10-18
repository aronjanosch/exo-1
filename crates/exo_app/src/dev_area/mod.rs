//! The dev area: a small level rectangle on Hearth (`content/place/dev_area.json`) to try the
//! mechanics without the finished ship. Area frame: metres, x east, y north, from the place's
//! centre (`town::Frame`). Two parts: in the south half the mini ship (`layout`), in the north half
//! the open zones (storage, hazards, dev; the role zones of engineering, bridge, turrets, lab and medbay
//! are gone); the pad zone lies south-east of the ship. Each owner module
//! (`hull`, `engineering`, `bridge`, `turrets`, `crew`, `gear`, `storage`,
//! `hazards`, `fire`, `dev`, with `ship` the shared step) has its own plugin; a zone's boxes go into
//! `DevAreaDevices` (a plain box). Boxes are static Avian cuboids on
//! `Layer::World` (headless too); their look is window-only.
use crate::env::PlanetRes;
use crate::origin::WorldPos;
use crate::town::Frame;
use crate::Layer;
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec2, DVec3};
use bevy::prelude::*;
use display_core::PanelId;

pub mod bridge;
pub mod crew;
pub mod dev_asks;
pub mod dev;
pub mod equipment;
pub mod engineering;
pub mod fire;
pub mod gear;
pub mod hazards;
pub mod hull;
pub mod layout;
pub mod ship;
pub mod storage;
pub mod turrets;

pub const PLACE: &str = "dev_area";
/// Half the rectangle's size, east and north (m). The place's flatten edit reaches a little further.
pub const HALF_EXTENT: [f64; 2] = [24.0, 20.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zone {
    pub name: &'static str,
    pub centre: [f64; 2],
    pub half: [f64; 2],
}

impl Zone {
    pub fn point(&self, local: [f64; 2]) -> [f64; 2] {
        [self.centre[0] + local[0], self.centre[1] + local[1]]
    }
    pub fn contains(&self, p: [f64; 2]) -> bool {
        (p[0] - self.centre[0]).abs() <= self.half[0] && (p[1] - self.centre[1]).abs() <= self.half[1]
    }
}

/// The open zones in the area's north half: the storage cabinets, the hazards and the dev zone (item table). The role
/// zones (engineering, bridge, turrets, lab, medbay) that mirrored the ship's rooms are gone. TODO(initiator): sizes and order are
/// greybox values (#306).
const ZONE_HALF: [f64; 2] = [5.5, 4.0];
pub const STORAGE: Zone = Zone { name: "storage", centre: [-6.0, 15.5], half: ZONE_HALF };
pub const HAZARDS: Zone = Zone { name: "hazards", centre: [6.0, 15.5], half: ZONE_HALF };
pub const DEV: Zone = Zone { name: "dev", centre: [18.0, 15.5], half: ZONE_HALF };
/// Open ground north of the ship (where the engineering zone stood): the scenarios stage tests there.
pub const OPEN_GROUND: [f64; 2] = [-18.0, 6.0];
/// The pad zone in the south-east, clear of the mini ship's airlock (its outer door at x 12.35) and of
/// the lab zone to the north: the flight ship parks on the pad at its centre (`content/place/dev_area.json`,
/// pad "main", radius 7 m). The ship faces west there (its nose along world -z) and its ramp points
/// east; the suit lockers stand beside the ramp (`equipment`). TODO(initiator): the spot and the size.
pub const PAD_ZONE: Zone = Zone { name: "pad", centre: [21.5, -14.5], half: [8.0, 9.0] };
/// The pad where the flight ship parks (and where a downed walker respawns): the pad zone's centre.
pub const PAD_MAIN: [f64; 2] = PAD_ZONE.centre;
pub const ZONES: [Zone; 4] = [STORAGE, HAZARDS, DEV, PAD_ZONE];

#[derive(Clone, Debug, PartialEq)]
pub struct PlaceholderBox {
    pub id: String,
    pub zone: Zone,
    /// Centre on the ground relative to the zone's centre (m, x east, y north).
    pub local_pos: [f64; 2],
    /// Width, height and depth (m): x right, y up, z towards the front.
    pub size: [f64; 3],
    /// Counter-clockwise from above; 0 faces south (the pad zone), 180 north.
    pub turn_deg: f64,
    pub colour: [f32; 3],
}

#[derive(Resource, Default, Debug)]
pub struct DevAreaDevices {
    pub boxes: Vec<PlaceholderBox>,
}

impl DevAreaDevices {
    /// Adds a device; an id used twice is a mistake in the code and panics.
    pub fn add(&mut self, b: PlaceholderBox) {
        assert!(self.boxes.iter().all(|x| x.id != b.id), "dev area: device id '{}' used twice", b.id);
        self.boxes.push(b);
    }
}

#[derive(Component, Clone, Debug)]
pub struct DevBox {
    pub id: String,
    pub colour: [f32; 3],
    /// Collider size (m), with the part sunk into the ground.
    pub size: DVec3,
}

/// Area metres of a world position on the ground under the frame (the inverse of `Frame::ground`,
/// flat enough for the area's 24 m).
pub fn area_of(frame: &Frame, planet: &PlanetRes, p: DVec3) -> [f64; 2] {
    let u = (p - planet.centre).normalize();
    let k = u.dot(frame.dir);
    [planet.radius * u.dot(frame.east) / k, planet.radius * u.dot(frame.north) / k]
}

#[derive(Resource, Default)]
pub struct DevArea {
    pub frame: Option<Frame>,
    for_planet: Option<warp_core::PlanetId>,
    pub entities: Vec<Entity>,
    spawned: usize,
}

impl DevArea {
    pub fn ground(&self, planet: &PlanetRes, p: [f64; 2], lift: f64) -> Option<(DVec3, DVec3)> {
        self.frame.map(|f| f.ground(planet, p[0], p[1], lift))
    }
}

/// The direction from the planet's centre to the walker's start in the mini ship (`layout::SPAWN`).
pub fn spawn_up(planet: &PlanetRes) -> Option<DVec3> {
    let frame = Frame::of_place(planet, PLACE)?;
    let (at, _) = frame.ground(planet, layout::SPAWN[0], layout::SPAWN[1], 0.0);
    Some((at - planet.centre).normalize())
}

/// Boxes reach this far into the ground, so a sloped edge does not show a gap (m).
const SINK: f64 = 0.2;

pub fn plugin(app: &mut App) {
    app.init_resource::<DevAreaDevices>().init_resource::<DevArea>();
    app.add_systems(Update, spawn_devices.in_set(crate::phases::Frame::World));
    // One plugin per owner; the layout spawns the inner ship's walls and screens.
    app.add_plugins((ship::plugin, layout::plugin, hull::plugin, engineering::plugin, bridge::plugin, turrets::plugin));
    app.add_plugins((crew::plugin, dev_asks::plugin, gear::plugin, equipment::plugin, fire::plugin, storage::plugin, hazards::plugin, dev::plugin));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (dress_boxes, mark_zones).after(spawn_devices).in_set(crate::phases::Frame::World));
    hull::window_plugin(app);
    engineering::window_plugin(app);
    engineering::junction_boxes::window_plugin(app);
    crew::window_plugin(app);
    fire::window_plugin(app);
}

pub fn box_pose(frame: &Frame, planet: &PlanetRes, b: &PlaceholderBox) -> (DVec3, DQuat) {
    let p = b.zone.point(b.local_pos);
    let (ground, up) = frame.ground(planet, p[0], p[1], 0.0);
    let rot = frame.rotation(up, b.turn_deg);
    let centre = ground + up * (b.size[1] - SINK) * 0.5;
    (centre, rot)
}

fn spawn_devices(mut commands: Commands, planet: Res<PlanetRes>, devices: Res<DevAreaDevices>, mut area: ResMut<DevArea>) {
    if area.for_planet != Some(planet.id) {
        area.for_planet = Some(planet.id);
        for e in area.entities.drain(..) {
            commands.entity(e).despawn();
        }
        area.spawned = 0;
        area.frame = Frame::of_place(&planet, PLACE);
    }
    let Some(frame) = area.frame else { return };
    while area.spawned < devices.boxes.len() {
        let b = &devices.boxes[area.spawned];
        area.spawned += 1;
        let (centre, rot) = box_pose(&frame, &planet, b);
        let size = DVec3::new(b.size[0], b.size[1] + SINK, b.size[2]);
        let e = commands
            .spawn((
                RigidBody::Static,
                Collider::cuboid(size.x, size.y, size.z),
                Position(centre),
                Rotation(rot),
                CollisionLayers::new(Layer::World, LayerMask::ALL),
                DevBox { id: b.id.clone(), colour: b.colour, size },
            ))
            .id();
        area.entities.push(e);
    }
}

fn dress_boxes(mut commands: Commands, new: Query<(Entity, &DevBox), Added<DevBox>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for (e, b) in &new {
        // The reactor's drum is the model's (`engineering::reactor_room`); its box is only the collision.
        // The junction boxes' volumes are only collision and the repair's aim; the model is drawn by `engineering::junction_boxes`.
        if b.id == layout::REACTOR || layout::is_junction_box(&b.id) {
            continue;
        }
        let [r, g, bl] = b.colour;
        commands.entity(e).insert((
            Mesh3d(meshes.add(Cuboid::new(b.size.x as f32, b.size.y as f32, b.size.z as f32))),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(r, g, bl), perceptual_roughness: 0.85, ..default() })),
        ));
    }
}

/// Zone colours on the ground. TODO(initiator): the look.
fn zone_colour(z: &Zone) -> Color {
    match z.name {
        "storage" => Color::srgb(0.7, 0.7, 0.62),
        "hazards" => Color::srgb(0.9, 0.2, 0.7),
        "dev" => Color::srgb(0.5, 0.5, 0.5),
        _ => Color::srgb(0.85, 0.85, 0.2),
    }
}

/// Window: each zone's outline as strips on the ground (pieces of at most 10 m, so they follow the planet's
/// curve), once per planet.
fn mark_zones(mut commands: Commands, planet: Res<PlanetRes>, area: Res<DevArea>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>, mut done: Local<Option<warp_core::PlanetId>>, mut marks: Local<Vec<Entity>>) {
    if *done == Some(planet.id) {
        return;
    }
    *done = Some(planet.id);
    for e in marks.drain(..) {
        commands.entity(e).despawn();
    }
    let Some(frame) = area.frame else { return };
    const PIECE: f64 = 10.0;
    const WIDTH: f32 = 0.35;
    for z in &ZONES {
        let mat = materials.add(StandardMaterial { base_color: zone_colour(z), perceptual_roughness: 1.0, ..default() });
        // Inset by half a metre, so neighbouring zones show two lines.
        let (hx, hy) = (z.half[0] - 0.5, z.half[1] - 0.5);
        let corners = [[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]];
        for i in 0..4 {
            let (a, b) = (z.point(corners[i]), z.point(corners[(i + 1) % 4]));
            let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            let n = (len / PIECE).ceil().max(1.0) as usize;
            let piece = len / n as f64;
            let turn = (b[1] - a[1]).atan2(b[0] - a[0]);
            let mesh = meshes.add(Cuboid::new(piece as f32, 0.04, WIDTH));
            for k in 0..n {
                let t = (k as f64 + 0.5) / n as f64;
                let (pos, up) = frame.ground(&planet, a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, 0.02);
                // Rotation's +x along the edge: the plan's turn is from south-facing, so +x (east) turned by the edge's angle.
                let rot = frame.rotation(up, turn.to_degrees());
                marks.push(commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_rotation(rot.as_quat()), WorldPos(pos))).id());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "used twice")]
    fn a_device_id_is_unique() {
        let b = PlaceholderBox { id: "x".into(), zone: DEV, local_pos: [0.0, 0.0], size: [1.0, 1.0, 1.0], turn_deg: 0.0, colour: [1.0; 3] };
        let mut d = DevAreaDevices::default();
        d.add(b.clone());
        d.add(b);
    }
}
