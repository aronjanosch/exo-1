//! The dev area: a small level rectangle on Hearth (`content/place/dev_area.json`) to try the
//! mechanics without the finished ship. Area frame: metres, x east, y north, from the place's
//! centre (`town::Frame`). Two parts: in the south half the mini ship (`layout`), in the north half
//! the open role zones (`ROLE_ZONES`); the landing pad lies east of the ship. Each owner module
//! (`hull`, `engineering`, `bridge`, `turrets`, `lab`, `medbay`, `crew`, `gear`, `storage`,
//! `hazards`, `fire`, `dev`, with `ship` the shared step) has its own plugin; a zone's boxes go into
//! `DevAreaDevices` (a box with an optional display panel). Boxes are static Avian cuboids on
//! `Layer::World` (headless too); their look is window-only.
use crate::displays::DisplayPanel;
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
pub mod dev;
pub mod equipment;
pub mod engineering;
pub mod fire;
pub mod gear;
pub mod hazards;
pub mod hull;
pub mod lab;
pub mod layout;
pub mod medbay;
pub mod screens;
pub mod ship;
pub mod storage;
pub mod turrets;

pub const PLACE: &str = "dev_area";
/// Half the rectangle's size, east and north (m). The place's flatten edit reaches a little further.
pub const HALF_EXTENT: [f64; 2] = [24.0, 20.0];
pub const PAD_MAIN: [f64; 2] = [17.0, -12.0];

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

/// The open role zones in the area's north half, two rows of four: each shows its role's machines
/// to look at quickly (the role rooms' screens and dummy housings, `layout::ROOM_ZONES`), plus what
/// stands only outside (the captain's consoles, the lab's devices, the storage, the hazards, the dev
/// tools). TODO(initiator): sizes and order are greybox values (#306).
const ZONE_HALF: [f64; 2] = [5.5, 4.0];
pub const ENGINEERING: Zone = Zone { name: "engineering", centre: [-18.0, 6.0], half: ZONE_HALF };
pub const BRIDGE_ZONE: Zone = Zone { name: "bridge", centre: [-6.0, 6.0], half: ZONE_HALF };
pub const TURRETS: Zone = Zone { name: "turrets", centre: [6.0, 6.0], half: ZONE_HALF };
pub const LAB: Zone = Zone { name: "lab", centre: [18.0, 6.0], half: ZONE_HALF };
pub const MEDBAY: Zone = Zone { name: "medbay", centre: [-18.0, 15.5], half: ZONE_HALF };
pub const STORAGE: Zone = Zone { name: "storage", centre: [-6.0, 15.5], half: ZONE_HALF };
pub const HAZARDS: Zone = Zone { name: "hazards", centre: [6.0, 15.5], half: ZONE_HALF };
pub const DEV: Zone = Zone { name: "dev", centre: [18.0, 15.5], half: ZONE_HALF };
/// The landing pad east of the ship, by the airlock's outer door: the flight ship parks here
/// (`content/place/dev_area.json`, pad "main").
pub const PADS: Zone = Zone { name: "pads", centre: [17.0, -12.0], half: [7.0, 7.0] };
pub const ROLE_ZONES: [Zone; 8] = [ENGINEERING, BRIDGE_ZONE, TURRETS, LAB, MEDBAY, STORAGE, HAZARDS, DEV];
pub const ZONES: [Zone; 9] = [ENGINEERING, BRIDGE_ZONE, TURRETS, LAB, MEDBAY, STORAGE, HAZARDS, DEV, PADS];

/// A display mounted on a box: its id and name, its size, its centre in the box's frame (x right,
/// y up from the ground, z towards the box's front) and how far it leans back.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelSpec {
    pub id: PanelId,
    pub name: String,
    pub size_m: [f64; 2],
    pub at: [f64; 3],
    pub tilt_deg: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaceholderBox {
    pub id: String,
    pub zone: Zone,
    /// Centre on the ground relative to the zone's centre (m, x east, y north).
    pub local_pos: [f64; 2],
    /// Width, height and depth (m): x right, y up, z towards the front.
    pub size: [f64; 3],
    /// Counter-clockwise from above; 0 faces south (the pads), 180 north.
    pub turn_deg: f64,
    pub colour: [f32; 3],
    pub panel: Option<PanelSpec>,
}

#[derive(Resource, Default, Debug)]
pub struct DevAreaDevices {
    pub boxes: Vec<PlaceholderBox>,
    /// The ids of the boxes that are a role zone's mirror of a box in a ship room (`add_mirror`).
    mirrors: std::collections::BTreeSet<String>,
}

impl DevAreaDevices {
    /// Adds a device; an id used twice is a mistake in the code and panics.
    pub fn add(&mut self, b: PlaceholderBox) {
        assert!(self.boxes.iter().all(|x| x.id != b.id), "dev area: device id '{}' used twice", b.id);
        self.boxes.push(b);
    }

    /// Adds the mirror of a device that stands in a ship room: the same box and panel (the same
    /// panel id, so the same machine) in a role zone outside, at `local_pos` there, its id with
    /// `_outside`. The mirror carries `layout::Mirror`, so a lookup by panel id finds the one inside.
    pub fn add_mirror(&mut self, inside: &PlaceholderBox, zone: Zone, local_pos: [f64; 2]) {
        let b = PlaceholderBox { id: format!("{}_outside", inside.id), zone, local_pos, ..inside.clone() };
        self.mirrors.insert(b.id.clone());
        self.add(b);
    }

    pub fn is_mirror(&self, id: &str) -> bool {
        self.mirrors.contains(id)
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
    app.add_plugins((crew::plugin, gear::plugin, equipment::plugin, fire::plugin, lab::plugin, medbay::plugin, storage::plugin, hazards::plugin, dev::plugin));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (dress_boxes, mark_zones).after(spawn_devices).in_set(crate::phases::Frame::World));
    hull::window_plugin(app);
    crew::window_plugin(app);
    fire::window_plugin(app);
    bridge::window_plugin(app);
}

pub fn box_pose(frame: &Frame, planet: &PlanetRes, b: &PlaceholderBox) -> ((DVec3, DQuat), Option<DisplayPanel>) {
    let p = b.zone.point(b.local_pos);
    let (ground, up) = frame.ground(planet, p[0], p[1], 0.0);
    let rot = frame.rotation(up, b.turn_deg);
    let centre = ground + up * (b.size[1] - SINK) * 0.5;
    let panel = b.panel.as_ref().map(|s| DisplayPanel {
        id: s.id.clone(),
        name: s.name.clone(),
        size_m: DVec2::from(s.size_m),
        centre: ground + rot * DVec3::from(s.at),
        rot: rot * DQuat::from_rotation_x(-s.tilt_deg.to_radians()),
    });
    ((centre, rot), panel)
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
        let ((centre, rot), panel) = box_pose(&frame, &planet, b);
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
        let mirror = devices.is_mirror(&b.id);
        if mirror {
            commands.entity(e).insert(layout::Mirror);
        }
        if let Some(d) = panel {
            let p = commands.spawn(d).id();
            if mirror {
                commands.entity(p).insert(layout::Mirror);
            }
            area.entities.push(p);
        }
    }
}

fn dress_boxes(mut commands: Commands, new: Query<(Entity, &DevBox), Added<DevBox>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for (e, b) in &new {
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
        "bridge" => Color::srgb(0.25, 0.4, 0.85),
        "turrets" => Color::srgb(0.8, 0.25, 0.25),
        "lab" => Color::srgb(0.2, 0.75, 0.6),
        "medbay" => Color::srgb(0.9, 0.9, 0.95),
        "engineering" => Color::srgb(0.95, 0.55, 0.15),
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
        let b = PlaceholderBox { id: "x".into(), zone: LAB, local_pos: [0.0, 0.0], size: [1.0, 1.0, 1.0], turn_deg: 0.0, colour: [1.0; 3], panel: None };
        let mut d = DevAreaDevices::default();
        d.add(b.clone());
        d.add(b);
    }
}
