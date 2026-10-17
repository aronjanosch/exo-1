//! Storage in the dev area: cabinets in the STORAGE zone, each a container on the host's map
//! (item_core racks, so every move goes through the authority). F opens the container window
//! (`container_window`, #310): the places inside, the bar below, click or drag to move stacks. The
//! rules are `item_core::container` (Barotrauma's containers: stack caps, tag filters, locks); a
//! cabinet's slot rules are its own (a mask cabinet takes masks, an extinguisher cabinet
//! takes extinguishers), and the supply crate is the shared preset.
use super::*;
use crate::container_window::{ContainerSpec, ContainerSpecs};
use crate::items::Items;
use crate::ship::Ship;
use item_core::container::{Access, Container, SlotRule};
use item_core::{ItemDefs, RackId};

pub const MASKS: RackId = 101;
pub const EXTINGUISHERS: RackId = 102;
pub const CRATE: RackId = 103;

/// The row of cabinets, metres north of the storage zone's centre: against the north edge, facing
/// south onto a walkway. TODO(initiator): the places.
pub const ROW_Y: f64 = 3.0;

/// A cabinet: its rack, its place and look, and its window's name and columns.
#[derive(Debug, Clone)]
pub struct Cabinet {
    pub id: &'static str,
    pub rack: RackId,
    pub name: &'static str,
    pub x: f64,
    pub colour: [f32; 3],
    pub size: [f64; 3],
    /// Columns of the window's grid.
    pub cols: usize,
}

/// The cabinets as they stand: a tall one for masks and extinguishers, a low crate.
pub fn cabinets() -> Vec<Cabinet> {
    let cab = |id, rack, name, x, colour, size, cols| Cabinet { id, rack, name, x, colour, size, cols };
    vec![
        cab("store_masks", MASKS, "Mask cabinet", -3.5, [0.45, 0.50, 0.55], [1.0, 2.0, 0.5], 3),
        cab("store_extinguishers", EXTINGUISHERS, "Extinguisher cabinet", 0.0, [0.85, 0.12, 0.08], [1.0, 2.0, 0.5], 4),
        cab("store_crate", CRATE, "Supply crate", 3.5, [0.55, 0.60, 0.35], [1.2, 0.8, 0.8], 4),
    ]
}

/// A cabinet's slot rules and its stock. TODO(initiator): the slot counts, the caps and what is
/// stocked.
pub fn contents(rack: RackId, defs: &ItemDefs) -> Container {
    let mut c: Container = match rack {
        MASKS => Container::new(vec![SlotRule::tagged(1, &["mask"], &[]); 6], Access::Open),
        EXTINGUISHERS => Container::new(vec![SlotRule::tagged(1, &["extinguisher"], &[]); 4], Access::Open),
        // 12 places (4x3) as the window's design has it; the preset's 20 is the ship's own crate.
        _ => Container::new(vec![SlotRule::tagged(32, &["small", "medium"], &["carrier"]); 12], Access::Open),
    };
    match rack {
        MASKS => {
            put(&mut c, defs, "o2_mask", 3);
        }
        EXTINGUISHERS => put(&mut c, defs, "extinguisher", 2),
        _ => {
            put(&mut c, defs, "fuel_rod", 1);
            put(&mut c, defs, "bandage", 4);
            put(&mut c, defs, "oxy", 2);
        }
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
            devices.add(PlaceholderBox { id: c.id.into(), zone: STORAGE, local_pos: [c.x, ROW_Y], size: c.size, turn_deg: 0.0, colour: c.colour, panel: None });
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
    for rack in [MASKS, EXTINGUISHERS, CRATE] {
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
    fn every_cabinet_holds_its_stock() {
        let d = defs();
        for c in cabinets() {
            let stock = contents(c.rack, &d);
            assert!(stock.capacity().items > 0, "{}: the cabinet holds its stock", c.id);
        }
    }

    #[test]
    fn the_stock_is_where_the_cabinets_say() {
        let d = defs();
        assert_eq!(contents(MASKS, &d).count(&item_core::container::ItemKind::new("o2_mask")), 3);
        assert_eq!(contents(EXTINGUISHERS, &d).count(&item_core::container::ItemKind::new("extinguisher")), 2);
        assert_eq!(contents(CRATE, &d).count(&item_core::container::ItemKind::new("fuel_rod")), 1);
        assert!(!contents(MASKS, &d).rule(0).accepts(&d.get("wrench").unwrap().item()), "a mask cabinet takes no wrench");
    }
}
