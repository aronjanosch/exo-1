//! Storage in the dev area: cabinets and crates in the storage room (`layout::STORAGE_ROOM`, #312), each a container on the host's map
//! (item_core racks, so every move goes through the authority). F opens the container window
//! (`container_window`, #310): the places inside, the bar below, click or drag to move stacks. The
//! rules are `item_core::container` (Barotrauma's containers: stack caps, tag filters, locks); a
//! cabinet's slot rules are its own (a mask cabinet takes masks, an extinguisher cabinet
//! takes extinguishers); the three crates take any small or medium item.
use super::*;
use crate::container_window::{ContainerSpec, ContainerSpecs};
use crate::items::Items;
use crate::ship::Ship;
use item_core::container::{Access, Container, SlotRule};
use item_core::{ItemDefs, RackId};

pub const MASKS: RackId = 101;
pub const EXTINGUISHERS: RackId = 102;
pub const CRATE_A: RackId = 103;
pub const CRATE_B: RackId = 104;
pub const CRATE_C: RackId = 105;

/// A cabinet or crate: its rack, its place and look, and its window's name and columns.
#[derive(Debug, Clone)]
pub struct Cabinet {
    pub id: &'static str,
    pub rack: RackId,
    pub name: &'static str,
    /// Centre relative to the storage room's centre (m, x east, y north).
    pub at: [f64; 2],
    /// The way it faces (`PlaceholderBox::turn_deg`): 180 north, -90 west.
    pub turn: f64,
    pub colour: [f32; 3],
    pub size: [f64; 3],
    /// Columns of the window's grid.
    pub cols: usize,
}

/// What stands in the storage room (#312): three large crates against the south wall, facing north, and the mask and the
/// extinguisher cabinet against the east wall, facing west. TODO(initiator): the places.
pub fn cabinets() -> Vec<Cabinet> {
    let crate_ = |id, rack, name, x| Cabinet { id, rack, name, at: [x, -2.0], turn: 180.0, colour: [0.55, 0.60, 0.35], size: [1.2, 1.0, 0.8], cols: 4 };
    let cab = |id, rack, name, y, colour, cols| Cabinet { id, rack, name, at: [2.7, y], turn: -90.0, colour, size: [1.0, 2.0, 0.5], cols };
    vec![
        crate_("store_crate_a", CRATE_A, "Crate A", -2.0),
        crate_("store_crate_b", CRATE_B, "Crate B", 0.0),
        crate_("store_crate_c", CRATE_C, "Crate C", 2.0),
        cab("store_masks", MASKS, "Mask cabinet", 0.0, [0.45, 0.50, 0.55], 3),
        cab("store_extinguishers", EXTINGUISHERS, "Extinguisher cabinet", 1.2, [0.85, 0.12, 0.08], 4),
    ]
}

/// A cabinet's slot rules and its stock. TODO(initiator): the slot counts, the caps and what is
/// stocked.
pub fn contents(rack: RackId, defs: &ItemDefs) -> Container {
    let mut c: Container = match rack {
        MASKS => Container::new(vec![SlotRule::tagged(1, &["mask"], &[]); 6], Access::Open),
        EXTINGUISHERS => Container::new(vec![SlotRule::tagged(1, &["extinguisher"], &[]); 4], Access::Open),
        // 12 places (4x3) as the window's design has it; the preset's 20 is the ship's own crate.
        CRATE_A | CRATE_B | CRATE_C => Container::new(vec![SlotRule::tagged(32, &["small", "medium"], &["carrier"]); 12], Access::Open),
        _ => panic!("storage: no rack {rack}"),
    };
    match rack {
        MASKS => {
            put(&mut c, defs, "o2_mask", 3);
        }
        EXTINGUISHERS => put(&mut c, defs, "extinguisher", 2),
        CRATE_A => {
            // Three new spares (stack 1: one each) until the station shop exists (#320).
            put(&mut c, defs, "antimatter_canister", 3);
            put(&mut c, defs, "bandage", 4);
            put(&mut c, defs, "oxy", 2);
            // Two spare fuses for the first blown junction boxes (#328).
            put(&mut c, defs, "fuse", 2);
            // A hand lamp for the dark (#329).
            put(&mut c, defs, "hand_lamp", 1);
            // Three coolant canisters for the wall tank (#331): one place, stack 3.
            put(&mut c, defs, "coolant_canister", 3);
        }
        _ => {}
    }
    c
}

/// Puts `n` of an item into a container; the stock fits by construction.
fn put(c: &mut Container, defs: &ItemDefs, id: &str, n: u32) {
    let d = defs.get(id).unwrap_or_else(|| panic!("storage: no item '{id}'"));
    let left = c.try_insert(&[], d.stack_of(n));
    assert!(matches!(left, Ok(None)), "storage: the stock of {id} fits");
}

pub fn plugin(app: &mut App) {
    let cabs = cabinets();
    app.init_resource::<ContainerSpecs>();
    {
        let mut specs = app.world_mut().resource_mut::<ContainerSpecs>();
        for c in &cabs {
            specs.0.push(ContainerSpec { rack: c.rack, name: c.name.into(), box_id: c.id.into(), cols: c.cols });
        }
    }
    {
        let mut devices = app.world_mut().resource_mut::<DevAreaDevices>();
        for c in &cabs {
            devices.add(PlaceholderBox { id: c.id.into(), zone: layout::STORAGE_ROOM, local_pos: c.at, size: c.size, turn_deg: c.turn, colour: c.colour });
        }
    }
    app.add_systems(FixedUpdate, put_cabinets.in_set(crate::phases::Fx::Effects));
}

/// Once, when the ship is there: each cabinet gets its stock (host only; the clients hear it from
/// the host).
fn put_cabinets(time: Res<Time>, mut items: ResMut<Items>, ships: Query<(), With<Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    *done = true;
    let now = time.elapsed_secs_f64();
    for rack in [MASKS, EXTINGUISHERS, CRATE_A, CRATE_B, CRATE_C] {
        let stock = contents(rack, &items.defs);
        items.add_container(now, rack, stock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defs() -> ItemDefs {
        crate::items::load_defs()
    }

    #[test]
    fn every_container_has_its_places() {
        let d = defs();
        for c in cabinets() {
            let stock = contents(c.rack, &d);
            assert!(stock.len() > 0, "{}: the container has its places", c.id);
        }
    }

    #[test]
    fn the_stock_is_where_the_cabinets_say() {
        let d = defs();
        let n = |rack, id| contents(rack, &d).count(&item_core::container::ItemKind::new(id));
        assert_eq!(n(MASKS, "o2_mask"), 3);
        assert_eq!(n(EXTINGUISHERS, "extinguisher"), 2);
        assert_eq!(n(CRATE_A, "antimatter_canister"), 3);
        assert_eq!(n(CRATE_A, "bandage"), 4);
        assert_eq!(n(CRATE_A, "oxy"), 2);
        assert_eq!(n(CRATE_A, "hand_lamp"), 1);
        assert_eq!(n(CRATE_A, "coolant_canister"), 3);
        for rack in [CRATE_B, CRATE_C] {
            assert_eq!(contents(rack, &d).count(&item_core::container::ItemKind::new("oxy")), 0, "B and C start empty");
        }
        assert!(!contents(MASKS, &d).rule(0).accepts(&d.get("wrench").unwrap().item()), "a mask cabinet takes no wrench");
    }

    #[test]
    fn the_crates_take_ore_and_tools() {
        let d = defs();
        for rack in [CRATE_A, CRATE_B, CRATE_C] {
            let c = contents(rack, &d);
            assert_eq!(c.len(), 12);
            for id in ["grumblite", "snoozium", "wobble_quartz", "wrench"] {
                assert!(c.rule(0).accepts(&d.get(id).unwrap().item()), "a crate takes {id}");
            }
        }
    }
}
