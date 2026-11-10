//! The ore tub (#327): a cargo crate that holds ore. Two stand in the storage room. The tub is an
//! ordinary two-handed crate (`cargo`, `grab`); its container is a rack on the host's item map
//! (`item_core`), bound to the crate by `Tub::rack`. Standing, F opens the container window
//! (`container_window`, reach checked against the tub's place); the pick-up key (`Tap::Carry`)
//! takes it in both hands. Carried, nobody can open it: while a tub is in the hands the host's
//! lock on its rack belongs to `CARRIER`, so an Open from any player is refused as "in use" by the
//! authority itself. The load makes the carrier slower (`grab_core::load_speed_share`) and is saved
//! beside the gameplay sections.
use crate::cargo::{crate_bundle, crate_world, Crate, Crates};
use crate::container_window::{ContainerSpec, ContainerSpecs, ContainerWindow};
use crate::dev_area::{layout::STORAGE_ROOM, DevArea};
use crate::env::PlanetRes;
use crate::grab::Grab;
use crate::items::{InventoryUi, Items, Role};
use crate::ship::Ship;
use crate::walker::{cabin_frame, CabinFloor};
use avian3d::prelude::*;
use bevy::math::DVec3;
use bevy::prelude::*;
use gameplay_core::save::{Envelope, SaveError};
use item_core::container::{Access, Container, SlotRule};
use item_core::{Action, PlayerId, RackId, Request};
use std::collections::BTreeSet;

/// 107 and 108: the storage block (101-105 cabinets and crates, 106 the fabricator); 200-299 are the equipment room's.
pub const TUB_A: RackId = 107;
pub const TUB_B: RackId = 108;
pub const TUB_RACKS: [RackId; 2] = [TUB_A, TUB_B];
/// The crate-table row of the tub (`content/cargo/crates.json`).
pub const SIZE: &str = "tub";
pub const NAME: &str = "Ore tub";
pub const PLACES: usize = 6;
pub const COLS: usize = 3;
/// The player id the host's lock on a carried tub is held under; no real player has it.
pub const CARRIER: PlayerId = PlayerId::MAX;
/// Share of the walk speed left with every place used (empty: 1). TODO(initiator): starting value.
pub const FULL_SPEED_SHARE: f64 = 0.7;
/// Where the tubs stand: storage room metres from its centre (x east, y north) and the way they
/// face. On the floor flanking the doorway's inner side, clear of the crates (south wall), the
/// cabinets (east wall) and the west wall. TODO(initiator): the places.
pub const SPOTS: [(RackId, [f64; 2], f64); 2] = [(TUB_A, [-1.4, 1.8], 0.0), (TUB_B, [1.4, 1.8], 0.0)];
/// The ore tub's worn yellow and the dark inset of its open top. TODO(initiator): look.
const BODY: (f32, f32, f32) = (0.80, 0.62, 0.10);
const INSET: (f32, f32, f32) = (0.16, 0.12, 0.05);

/// A cargo crate whose container is the rack `rack`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Tub {
    pub rack: RackId,
}

#[derive(Clone, Copy, Debug)]
pub struct TubSpot {
    pub rack: RackId,
    pub entity: Entity,
    /// World position (the crate's centre).
    pub pos: DVec3,
    /// In someone's hands: it cannot be opened.
    pub carried: bool,
}

/// Where the tubs are now, for the interaction and the window (they look at positions, not at crates).
#[derive(Resource, Default, Debug)]
pub struct TubSpots(pub Vec<TubSpot>);

impl TubSpots {
    /// The place of a tub that stands (not carried), by its rack.
    pub fn standing(&self, rack: RackId) -> Option<DVec3> {
        self.0.iter().find(|s| s.rack == rack && !s.carried).map(|s| s.pos)
    }

    pub fn is_tub(&self, e: Entity) -> bool {
        self.0.iter().any(|s| s.entity == e)
    }

    pub fn rack_of(&self, e: Entity) -> Option<RackId> {
        self.0.iter().find(|s| s.entity == e).map(|s| s.rack)
    }
}

/// The tub's container: six places, ore only, each as high as the item's own stack allows.
pub fn contents() -> Container {
    Container::new(vec![SlotRule::tagged(99, &["ore"], &[]); PLACES], Access::Open)
}

/// Places in use of a container.
pub fn used_places(c: &Container) -> usize {
    (0..c.len()).filter(|i| c.slot(*i).is_some()).count()
}

/// Walk-speed share the carried tub's load leaves (1 empty).
pub fn load_share(c: &Container) -> f64 {
    grab_core::load_speed_share(used_places(c), c.len(), FULL_SPEED_SHARE)
}

pub fn plugin(app: &mut App) {
    app.init_resource::<TubSpots>().init_resource::<ContainerSpecs>();
    {
        let mut specs = app.world_mut().resource_mut::<ContainerSpecs>();
        for rack in TUB_RACKS {
            specs.0.push(ContainerSpec { rack, name: NAME.into(), box_id: String::new(), cols: COLS });
        }
    }
    app.add_systems(FixedUpdate, (spawn_tubs, tub_step).chain().before(crate::interact::interaction).in_set(crate::phases::Fx::Walker));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, add_tub_visuals.before(crate::cargo::update_crate_visuals).in_set(crate::phases::Frame::Sync));
}

/// Once, when the area and the ship are there: the two tubs stand on the floor and their
/// containers are on the host's map (a loaded save may have filled them first). Host only; a
/// client's crates are its own.
#[allow(clippy::too_many_arguments)]
fn spawn_tubs(
    mut commands: Commands,
    time: Res<Time>,
    area: Res<DevArea>,
    planet: Res<PlanetRes>,
    table: Res<Crates>,
    mut items: ResMut<Items>,
    mut spots: ResMut<TubSpots>,
    ships: Query<(), With<Ship>>,
    mut done: Local<bool>,
) {
    if *done || ships.is_empty() || items.role == Role::Client {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = true;
    let now = time.elapsed_secs_f64();
    let half_y = table.0.get(SIZE).unwrap_or_else(|| panic!("ore tub: no crate size '{SIZE}'")).extents[1] * 0.5;
    for (rack, at, turn) in SPOTS {
        let Some((pos, up)) = area.ground(&planet, STORAGE_ROOM.point(at), half_y + 0.01) else { continue };
        let fwd = frame.rotation(up, turn) * DVec3::NEG_Z;
        let e = commands.spawn((crate_bundle(&table.0, SIZE, None, pos, fwd), Tub { rack })).id();
        spots.0.push(TubSpot { rack, entity: e, pos, carried: false });
        if !items.host.racks.contains_key(&rack) {
            items.add_container(now, rack, contents());
        }
    }
}

/// Every tick: the tubs' places and who carries one; the window and the lock follow the hands;
/// the carrier's walk speed follows the load.
#[allow(clippy::too_many_arguments)]
fn tub_step(
    time: Res<Time>,
    mut items: ResMut<Items>,
    mut cw: ResMut<ContainerWindow>,
    mut ui: ResMut<InventoryUi>,
    mut grab: ResMut<Grab>,
    mut spots: ResMut<TubSpots>,
    tubs: Query<(Entity, &Crate, &Tub)>,
    ships: Query<(Entity, &Position, &Rotation), With<Ship>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    mut carried_before: Local<BTreeSet<RackId>>,
) {
    let now = time.elapsed_secs_f64();
    let frame = ships.iter().next().map_or(walker_core::Frame::IDENTITY, |(e, p, r)| cabin_frame(e, (p, r), &floors));
    let held = grab.held.map(|h| h.crate_e);
    spots.0 = tubs.iter().map(|(e, c, t)| TubSpot { rack: t.rack, entity: e, pos: crate_world(c, &frame).0, carried: held == Some(e) }).collect();
    let carried: BTreeSet<RackId> = spots.0.iter().filter(|s| s.carried).map(|s| s.rack).collect();
    let alive: BTreeSet<RackId> = spots.0.iter().map(|s| s.rack).collect();
    if items.role != Role::Client {
        for rack in carried.difference(&carried_before) {
            // Picked up: whoever has it open lets go, then the carrier holds it.
            if cw.rack == Some(*rack) {
                cw.close(now, &mut items, &mut ui);
            }
            if let Some(p) = items.host.locks.get(rack).copied().filter(|p| *p != CARRIER) {
                items.release_locks(now, p);
            }
            items.remote(now, Action { player: CARRIER, seq: 0, req: Request::Open { rack: *rack } });
        }
        // Set down (or gone): the carrier gives the rack back.
        for rack in carried_before.iter().filter(|r| !carried.contains(r)) {
            items.remote(now, Action { player: CARRIER, seq: 0, req: Request::Close { rack: *rack } });
        }
    }
    // A window on a tub that is carried or gone closes, whatever the way it got open.
    if let Some(rack) = cw.rack.filter(|r| TUB_RACKS.contains(r)) {
        if carried.contains(&rack) || !alive.contains(&rack) {
            cw.close(now, &mut items, &mut ui);
        }
    }
    *carried_before = carried;
    // The load slows the carrier; nothing in the hands (or another crate) costs nothing extra.
    let load = grab.held.and_then(|h| tubs.get(h.crate_e).ok()).and_then(|(_, _, t)| items.host.racks.get(&t.rack).or_else(|| items.client.racks.get(&t.rack))).map_or(1.0, load_share);
    grab.load_loss = 1.0 - load;
}

/// The tub's look over the plain crate's: worn yellow with a dark inset top (an open trough).
/// Greybox. TODO(initiator): a modelled trough.
fn add_tub_visuals(
    mut commands: Commands,
    q: Query<(Entity, &Crate), Added<Tub>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (e, c) in &q {
        let h = c.body.half;
        let mat = |m: &mut Assets<StandardMaterial>, (r, g, b): (f32, f32, f32)| m.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.95, ..default() });
        let body = mat(&mut materials, BODY);
        let inset = mat(&mut materials, INSET);
        let size = Vec3::new(h.x as f32 * 2.0, h.y as f32 * 2.0, h.z as f32 * 2.0);
        commands.entity(e).insert((Mesh3d(meshes.add(Cuboid::from_size(size))), MeshMaterial3d(body), crate::origin::WorldPose::default())).with_children(|p| {
            // The dark top, a little inside the rim and a hair above it.
            p.spawn((Mesh3d(meshes.add(Cuboid::new(size.x * 0.8, 0.02, size.z * 0.7))), MeshMaterial3d(inset), Transform::from_xyz(0.0, size.y * 0.5 + 0.002, 0.0)));
        });
    }
}

// ---- save ----

/// The save section: each tub's container (its load). Kept apart from the gameplay sections so an
/// older save, which has none, loads with empty tubs.
pub const SECTION: &str = "ore_tubs";
pub const VERSION: u32 = 1;

/// The loads of the tubs on the host's map, as plain data: `[{rack, contents}]`.
pub fn save_section(items: &Items, env: &mut Envelope) {
    let loads: Vec<serde_json::Value> = TUB_RACKS
        .iter()
        .filter_map(|r| items.host.racks.get(r).map(|c| serde_json::json!({ "rack": r, "contents": c })))
        .collect();
    env.put(SECTION, VERSION, &loads);
}

/// Reads the section (read first: a refused section changes nothing).
pub fn read_section(env: &Envelope) -> Result<Vec<(RackId, Container)>, SaveError> {
    let Some(loads) = env.get::<Vec<serde_json::Value>>(SECTION, VERSION)? else { return Ok(Vec::new()) };
    loads
        .into_iter()
        .map(|v| {
            let rack = v.get("rack").and_then(serde_json::Value::as_u64).and_then(|r| RackId::try_from(r).ok()).filter(|r| TUB_RACKS.contains(r));
            let contents = v.get("contents").cloned().and_then(|c| serde_json::from_value::<Container>(c).ok());
            match (rack, contents) {
                (Some(r), Some(c)) if c.len() == PLACES => Ok((r, c)),
                _ => Err(SaveError::Parse(format!("section '{SECTION}': a tub does not read"))),
            }
        })
        .collect()
}

/// Puts the saved loads into the tubs' racks (host or one process).
pub fn apply_section(items: &mut Items, now: f64, loads: Vec<(RackId, Container)>) {
    for (rack, c) in loads {
        items.add_container(now, rack, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> item_core::ItemDefs {
        crate::items::load_defs()
    }

    #[test]
    fn the_tub_takes_ore_and_nothing_else() {
        let d = defs();
        let c = contents();
        assert_eq!((c.len(), used_places(&c)), (PLACES, 0));
        for id in ["grumblite", "snoozium", "wobble_quartz"] {
            assert!(c.rule(0).accepts(&d.get(id).unwrap().item()), "the tub takes {id}");
        }
        for id in ["wrench", "extinguisher", "o2_mask"] {
            assert!(!c.rule(0).accepts(&d.get(id).unwrap().item()), "the tub refuses {id}");
        }
    }

    #[test]
    fn a_full_tub_is_as_slow_as_set_and_an_empty_one_is_not() {
        let d = defs();
        let mut c = contents();
        assert_eq!(load_share(&c), 1.0);
        let ore = d.get("grumblite").unwrap();
        // Six places of three: eighteen chunks fill it; the nineteenth does not go in.
        let left = c.try_insert(&[], ore.stack_of(18));
        assert!(matches!(left, Ok(None)), "eighteen chunks fit");
        assert_eq!(used_places(&c), PLACES);
        assert!((load_share(&c) - FULL_SPEED_SHARE).abs() < 1e-12);
        assert!(matches!(c.try_insert(&[], ore.stack_of(1)), Ok(Some(_)) | Err(_)), "a full tub does not take more");
    }

    #[test]
    fn a_saved_load_reads_back_and_a_bad_one_is_refused() {
        let d = defs();
        let mut c = contents();
        assert!(matches!(c.try_insert(&[], d.get("grumblite").unwrap().stack_of(4)), Ok(None)));
        let mut items = Items::load();
        items.add_container(0.0, TUB_A, c.clone());
        items.add_container(0.0, TUB_B, contents());
        let mut env = Envelope::new();
        save_section(&items, &mut env);
        let back = read_section(&Envelope::from_json(&env.to_json()).unwrap()).unwrap();
        assert_eq!(back, vec![(TUB_A, c), (TUB_B, contents())]);
        assert_eq!(read_section(&Envelope::new()).unwrap(), Vec::new(), "an older save has no section");
        let mut bad = Envelope::new();
        bad.put(SECTION, VERSION, &vec![serde_json::json!({ "rack": 7, "contents": 3 })]);
        assert!(read_section(&bad).is_err());
    }
}
