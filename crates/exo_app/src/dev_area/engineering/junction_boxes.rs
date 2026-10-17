//! The three junction boxes (#322): wall cabinets with `content/ship/junction_box.glb`, no screen.
//! Contract of the model: `art/ship/junction_box.md`.
//!
//! - A box has one toggle per group (`toggle_1`, `toggle_2`; rest OFF, ON is -50 degrees about local X) with a lamp above it (lit
//!   while the group gets power), a fault lamp (lit while the box needs repair or is broken, blinking when broken) and a door over
//!   the bus bars (`door`, -105 degrees about local Y) that opens by itself while someone repairs the box.
//! - The toggle IS the group's breaker on the grid (`reactor_core::Grid`): F on it sends `ReactorAsk::SetBreaker`, the host's news
//!   (`ReactorNews::breakers`) says where it stands, and every process shows that on the kit's `Machine` (the kit animates the node).
//!   So host and clients cannot disagree, and a breaker set from anywhere else (cold ship, dev faults) moves the toggle.
//! - The door follows the repairs: the others by their public status (`Doing::Repairing(JunctionBox(i))`), the own player by the ring it runs.
use crate::controls_kit::{Binding, KitBindings, Machine};
use crate::dev_area::layout::{JUNCTION_MODEL, JUNCTION_SPOTS};
use crate::dev_area::ship::ShipSet;
use crate::dev_area::DevArea;
use crate::env::PlanetRes;
use crate::items::Items;
use crate::origin::WorldPos;
use crate::repair::RepairPlay;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use controls_core::Controls;
use item_core::wire::{BreakerGroup, Doing, ReactorAsk, RepairDevice, JUNCTION_BOXES};

pub const TOGGLES: [&str; 2] = ["toggle_1", "toggle_2"];
pub const DOOR: &str = "door";
/// The groups on each box, in toggle order (`reactor_core::Group::junction`): JB-1 engines and jump drive, JB-2 life support and
/// lights, JB-3 shields and weapons.
pub const BOX_GROUPS: [[BreakerGroup; 2]; JUNCTION_BOXES] = [
    [BreakerGroup::Engines, BreakerGroup::JumpDrive],
    [BreakerGroup::LifeSupport, BreakerGroup::Lights],
    [BreakerGroup::Shields, BreakerGroup::Weapons],
];
pub const GROUP_NAMES: [[&str; 2]; JUNCTION_BOXES] = [["Engines", "Jump drive"], ["Life support", "Lights"], ["Shields", "Weapons"]];
/// Where a toggle's bat is in the model (m, glTF axes, origin on the wall at floor height): the foot at x -/+ 0.13, y 1.06, and a
/// little out and up along the bat. TODO(initiator): feel of the pick radius.
const TOGGLE_AT: [[f64; 3]; 2] = [[-0.13, 1.09, 0.21], [0.13, 1.09, 0.21]];
const PICK_RADIUS: f64 = 0.08;

/// One box's place in the world (area ground, turned to face out of its wall).
#[derive(Component, Clone, Copy, Debug)]
pub struct JunctionRoom {
    pub index: usize,
    pub origin: DVec3,
    pub rot: DQuat,
}

impl JunctionRoom {
    pub fn world(&self, model: [f64; 3]) -> DVec3 {
        self.origin + self.rot * DVec3::from(model)
    }

    pub fn toggle_at(&self, k: usize) -> DVec3 {
        self.world(TOGGLE_AT[k])
    }

    /// The toggle the view's ray points at: the one nearest the ray within its radius and `reach`.
    pub fn pick(&self, eye: DVec3, look: DVec3, reach: f64) -> Option<usize> {
        let look = look.normalize_or_zero();
        (0..TOGGLES.len())
            .filter_map(|k| {
                let d = self.toggle_at(k) - eye;
                let along = d.dot(look);
                (along > 0.0 && d.length() <= reach).then(|| ((d - look * along).length(), k)).filter(|(off, _)| *off <= PICK_RADIUS)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, k)| k)
    }
}

pub fn machine() -> Machine {
    let mut c = Controls::new();
    c.toggle(TOGGLES[0], None).toggle(TOGGLES[1], None).cover(DOOR);
    Machine { controls: c, ..default() }
}

/// What the kit moves: travel from `art/ship/junction_box.md`.
pub fn bindings() -> Vec<Binding> {
    vec![Binding::rotate(TOGGLES[0], TOGGLES[0], Vec3::X, -50.0), Binding::rotate(TOGGLES[1], TOGGLES[1], Vec3::X, -50.0), Binding::rotate(DOOR, DOOR, Vec3::Y, -105.0)]
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, spawn_rooms.after(crate::dev_area::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(FixedUpdate, mirror.in_set(ShipSet::Effects));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (dress_rooms, drive_lamps).chain().after(crate::dev_area::spawn_devices).in_set(crate::phases::Frame::World));
}

fn spawn_rooms(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    for (index, spot) in JUNCTION_SPOTS.iter().enumerate() {
        let (origin, up) = frame.ground(&planet, spot.at[0], spot.at[1], 0.0);
        let rot = frame.rotation(up, spot.turn);
        let e = commands.spawn((JunctionRoom { index, origin, rot }, machine())).id();
        area.entities.push(e);
    }
}

/// Whether someone repairs box `i` now (the door stands open): the others by their public status (the host sets it only with a
/// network; it has no entry for a solo game), this process's own player by the ring it runs.
pub fn repaired(items: &Items, own: &RepairPlay, i: usize) -> bool {
    items.public.iter().any(|(_, s)| matches!(s.doing, Doing::Repairing(RepairDevice::JunctionBox(b)) if b == i)) || own.0.as_ref().is_some_and(|p| p.device == RepairDevice::JunctionBox(i))
}

/// Every process: the toggles and the door show the news (the breakers) and the public status (the repairs).
fn mirror(items: Res<Items>, own: Res<RepairPlay>, mut rooms: Query<(&JunctionRoom, &mut Machine)>) {
    let Some(n) = items.reactor else { return };
    for (room, mut m) in &mut rooms {
        for (k, g) in BOX_GROUPS[room.index].iter().enumerate() {
            if m.controls.is_on(TOGGLES[k]) != n.breakers[g.index()] {
                m.controls.press(TOGGLES[k], None, |_| true, |_| true);
            }
        }
        if m.controls.is_open(DOOR) != repaired(&items, &own, room.index) {
            m.controls.press(DOOR, None, |_| true, |_| true);
        }
    }
}

/// How a toggle reads on the prompt: the verb and the name.
pub struct Describe {
    pub verb: &'static str,
    pub name: String,
}

pub fn describe(i: usize, k: usize, items: &Items) -> Describe {
    let on = items.reactor.is_some_and(|n| n.breakers[BOX_GROUPS[i][k].index()]);
    Describe { verb: if on { "Switch off" } else { "Switch on" }, name: format!("JB-{} {}", i + 1, GROUP_NAMES[i][k]) }
}

/// F on a toggle: asks the host to flip the group's breaker.
pub fn press(i: usize, k: usize, items: &mut Items) {
    let g = BOX_GROUPS[i][k];
    let on = items.reactor.is_some_and(|n| n.breakers[g.index()]);
    items.ask_reactor(ReactorAsk::SetBreaker(g, !on));
}

// ---------------------------------------------------------------------------------------------
// Window: the model and its lamps.

const LAMPS: &[&str] = &["lamp_group_1", "lamp_group_2", "lamp_fault"];

#[derive(Component)]
pub struct JunctionVisual;

fn dress_rooms(mut commands: Commands, new: Query<(Entity, &JunctionRoom), Added<JunctionRoom>>, assets: Res<AssetServer>) {
    for (e, r) in &new {
        commands.entity(e).insert((
            bevy::world_serialization::WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(JUNCTION_MODEL))),
            Transform::from_rotation(r.rot.as_quat()),
            WorldPos(r.origin),
            KitBindings(bindings()),
            crate::named_material::WantMaterials(LAMPS),
            JunctionVisual,
        ));
    }
}

/// A lamp's emissive and base colour: lit in `colour`, else the dark `lamp_off`.
fn light(m: &mut StandardMaterial, on: bool, colour: Color) {
    let lit = LinearRgba::from(colour);
    m.emissive = if on { lit * 3.0 } else { LinearRgba::BLACK };
    m.base_color = if on { colour } else { Color::srgb(0.02, 0.04, 0.05) };
}

/// The fault lamp: steady while the box needs repair, blinking while it is broken.
pub fn fault_lit(fault: bool, working: bool, t: f64) -> bool {
    fault && (working || t.rem_euclid(1.0) < 0.5)
}

fn drive_lamps(time: Res<Time>, items: Res<Items>, rooms: Query<(&JunctionRoom, &crate::named_material::NamedMaterials), With<JunctionVisual>>, mut mats: ResMut<Assets<StandardMaterial>>) {
    let Some(n) = items.reactor else { return };
    let t = time.elapsed_secs_f64();
    for (room, named) in &rooms {
        let i = room.index;
        for (k, g) in BOX_GROUPS[i].iter().enumerate() {
            // The group gets power: its breaker is on, the box passes it and the grid delivers something.
            let on = n.breakers[g.index()] && n.delivered_kw[g.index()] > 0.0;
            if let Some(mut m) = named.0.get(LAMPS[k]).and_then(|h| mats.get_mut(h)) {
                light(&mut m, on, Color::srgb(0.1, 0.9, 0.25));
            }
        }
        if let Some(mut m) = named.0.get("lamp_fault").and_then(|h| mats.get_mut(h)) {
            light(&mut m, fault_lit(n.junction_fault[i], n.junctions[i].working, t), Color::srgb(1.0, 0.3, 0.1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> JunctionRoom {
        JunctionRoom { index: 0, origin: DVec3::ZERO, rot: DQuat::IDENTITY }
    }

    #[test]
    fn the_view_picks_the_toggle_it_points_at() {
        let r = room();
        let eye = DVec3::new(0.0, 1.6, 1.6);
        for k in 0..2 {
            let look = (r.toggle_at(k) - eye).normalize();
            assert_eq!(r.pick(eye, look, 2.5), Some(k));
        }
        assert_eq!(r.pick(eye, (r.toggle_at(0) - eye).normalize(), 0.3), None, "out of reach");
        assert_eq!(r.pick(eye, DVec3::NEG_Z, 2.5), None, "straight ahead is between and above the toggles");
    }

    /// The box's groups are the ones `reactor_core` hangs on it.
    #[test]
    fn the_box_groups_match_the_core() {
        use reactor_core::Group;
        for g in Group::ALL {
            let name = match g {
                Group::Engines => BreakerGroup::Engines,
                Group::LifeSupport => BreakerGroup::LifeSupport,
                Group::Lights => BreakerGroup::Lights,
                Group::Shields => BreakerGroup::Shields,
                Group::Weapons => BreakerGroup::Weapons,
                Group::JumpDrive => BreakerGroup::JumpDrive,
            };
            assert!(BOX_GROUPS[g.junction()].contains(&name), "{g:?} is on box {}", g.junction());
        }
    }
}
