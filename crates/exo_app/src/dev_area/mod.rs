//! The dev area: a small level rectangle on Hearth (`content/place/dev_area.json`) where
//! each role's placeholder devices and displays stand, to try mechanics without the finished ship
//! Area frame: metres, x east, y north, from the place's centre (`town::Frame`). Named zones split
//! it: `CAPTAIN`, `LAB` and `ENGINEERING` side by side from west to east, `STORAGE` behind them
//! beyond one line: each has its own module `dev_area/<role>.rs` whose plugin adds its
//! `PlaceholderBox`es to `DevAreaDevices` (a box with an optional display panel) and its own
//! systems; the line below `ROLES` adds that plugin. Boxes are static Avian cuboids on
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

pub mod airlock;
pub mod blast;
pub mod bots;
pub mod captain_screen;
pub mod coolant;
pub mod engine_room;
pub mod extinguisher;
pub mod fire;
pub mod look_samples;
pub mod medbay;
pub mod mask;
pub mod hazards;
pub mod item_table;
pub mod reactor;
pub mod reactor_screen;
pub mod startup_screen;
pub mod rods;
pub mod science;
pub mod status_monitor;
pub mod test_console;

pub const PLACE: &str = "dev_area";
/// Half the rectangle's size, east and north (m). The place's flatten edit reaches a little further.
pub const HALF_EXTENT: [f64; 2] = [24.0, 20.0];
pub const PAD_MAIN: [f64; 2] = [0.0, -12.0];

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

/// TODO(initiator): the zones' sizes and order. Each role zone is 14 x 12 m; the centres of
/// neighbouring zones are 14 m apart.
pub const CAPTAIN: Zone = Zone { name: "captain", centre: [-14.0, 6.0], half: [7.0, 6.0] };
pub const LAB: Zone = Zone { name: "lab", centre: [0.0, 6.0], half: [7.0, 6.0] };
pub const ENGINEERING: Zone = Zone { name: "engineering", centre: [14.0, 6.0], half: [7.0, 6.0] };
pub const STORAGE: Zone = Zone { name: "storage", centre: [0.0, 16.0], half: [21.0, 4.0] };
pub const PADS: Zone = Zone { name: "pads", centre: [0.0, -12.0], half: [24.0, 8.0] };
pub const ZONES: [Zone; 5] = [CAPTAIN, LAB, ENGINEERING, STORAGE, PADS];

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

/// Boxes reach this far into the ground, so a sloped edge does not show a gap (m).
const SINK: f64 = 0.2;

pub fn plugin(app: &mut App) {
    app.init_resource::<DevAreaDevices>().init_resource::<DevArea>();
    app.add_systems(Update, spawn_devices.in_set(crate::phases::Frame::World));
    // ROLES: one line per role module.
    app.add_plugins(test_console::plugin);
    app.add_plugins(look_samples::plugin);
    app.add_plugins(item_table::plugin);
    app.add_plugins(hazards::plugin);
    app.add_plugins(engine_room::plugin);
    app.add_plugins(coolant::plugin);
    app.add_plugins(rods::plugin);
    app.add_plugins(reactor::plugin);
    app.add_plugins(reactor_screen::plugin);
    app.add_plugins(fire::plugin);
    app.add_plugins(extinguisher::plugin);
    app.add_plugins(mask::plugin);
    app.add_plugins(science::plugin);
    app.add_plugins(medbay::plugin);
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (dress_boxes, mark_zones).after(spawn_devices).in_set(crate::phases::Frame::World));
    engine_room::window_plugin(app);
    fire::window_plugin(app);
    // ROLES (window): one line per role module with a window part.
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
        if let Some(d) = panel {
            area.entities.push(commands.spawn(d).id());
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
        "captain" => Color::srgb(0.25, 0.4, 0.85),
        "lab" => Color::srgb(0.2, 0.75, 0.6),
        "engineering" => Color::srgb(0.95, 0.55, 0.15),
        "storage" => Color::srgb(0.7, 0.7, 0.62),
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
