//! The equipment room's lockers (#311, station 2): five suit lockers along the south wall, each with
//! a canister box at its foot, and two equipment lockers on the north wall, a walkway between. All
//! are containers on the host's map (`item_core` racks) that open the container window (#310) and
//! belong to anyone. A suit locker holds one place and only the suit, a canister box four places and
//! only air canisters, an equipment locker four places and any tool. Greybox: a locker is one tall
//! box, its canister box a low one in front of its foot. TODO(initiator): sizes, places, stock.
use super::layout::EQUIPMENT;
use super::*;
use crate::container_window::{ContainerSpec, ContainerSpecs};
use crate::dev_area::ship::ReactorSim;
use crate::items::{Items, Role};
use crate::ship::Ship;
use item_core::container::{Access, Container, SlotRule, Stack};
use item_core::{ItemDefs, RackId};

pub const SUIT_LOCKERS: usize = 5;
pub const EQUIPMENT_LOCKERS: usize = 2;
/// Rack ids: the suit lockers 201..205, their canister boxes 211..215, the equipment lockers 221..222
/// (storage keeps 101..103).
pub const SUIT_RACK: RackId = 200;
pub const BOX_RACK: RackId = 210;
pub const LOCKER_RACK: RackId = 220;
/// Places of a canister box and of an equipment locker.
pub const BOX_PLACES: usize = 4;
pub const LOCKER_PLACES: usize = 4;

/// Rack of suit locker `i` (0..5), of its canister box, of equipment locker `i` (0..2).
pub fn suit_rack(i: usize) -> RackId {
    SUIT_RACK + 1 + i as RackId
}
pub fn box_rack(i: usize) -> RackId {
    BOX_RACK + 1 + i as RackId
}
pub fn locker_rack(i: usize) -> RackId {
    LOCKER_RACK + 1 + i as RackId
}

/// Whether a rack is a canister box.
pub fn is_box(rack: RackId) -> bool {
    (box_rack(0)..=box_rack(SUIT_LOCKERS - 1)).contains(&rack)
}

/// Where along the south wall the suit lockers stand, relative to the room's centre (m): one per metre.
fn locker_x(i: usize) -> f64 {
    i as f64 - 2.2
}

const SUIT_SIZE: [f64; 3] = [0.9, 2.0, 0.6];
const BOX_SIZE: [f64; 3] = [0.8, 0.45, 0.35];
const EQUIP_SIZE: [f64; 3] = [1.0, 1.8, 0.6];

/// One container of the room: its rack, name, box and place.
struct Item {
    rack: RackId,
    name: String,
    id: String,
    local_pos: [f64; 2],
    size: [f64; 3],
    turn_deg: f64,
    colour: [f32; 3],
    cols: usize,
}

fn all() -> Vec<Item> {
    let half = EQUIPMENT.half;
    let mut v = Vec::new();
    for i in 0..SUIT_LOCKERS {
        let x = locker_x(i);
        // They face north (turn 180) into the room; the box stands in front of the foot.
        v.push(Item { rack: suit_rack(i), name: format!("Suit locker {}", i + 1), id: format!("equip_suit_{}", i + 1), local_pos: [x, -half[1] + SUIT_SIZE[2] * 0.5], size: SUIT_SIZE, turn_deg: 180.0, colour: [0.45, 0.5, 0.58], cols: 1 });
        v.push(Item { rack: box_rack(i), name: format!("Canister box {}", i + 1), id: format!("equip_box_{}", i + 1), local_pos: [x, -half[1] + SUIT_SIZE[2] + BOX_SIZE[2] * 0.5], size: BOX_SIZE, turn_deg: 180.0, colour: [0.3, 0.55, 0.7], cols: BOX_PLACES });
    }
    for i in 0..EQUIPMENT_LOCKERS {
        let x = (i as f64 - 0.5) * 2.0;
        v.push(Item { rack: locker_rack(i), name: format!("Equipment locker {}", i + 1), id: format!("equip_locker_{}", i + 1), local_pos: [x, half[1] - EQUIP_SIZE[2] * 0.5], size: EQUIP_SIZE, turn_deg: 0.0, colour: [0.6, 0.45, 0.25], cols: LOCKER_PLACES });
    }
    v
}

/// A container's slot rules, empty. Suit locker: the suit only; box: air canisters; equipment locker: tools.
pub fn empty(rack: RackId) -> Container {
    let (n, tag) = if is_box(rack) { (BOX_PLACES, "air_canister") } else if (suit_rack(0)..=suit_rack(SUIT_LOCKERS - 1)).contains(&rack) { (1, "suit") } else { (LOCKER_PLACES, "tool") };
    Container::new(vec![SlotRule::tagged(1, &[tag], &[]); n], Access::Open)
}

/// A container with its start stock: the first two suit lockers hold a suit, the first two canister
/// boxes a full spare canister, the first equipment locker the welder.
pub fn contents(rack: RackId, defs: &ItemDefs) -> Container {
    let mut c = empty(rack);
    let put = |c: &mut Container, id: &str| {
        let d = defs.get(id).unwrap_or_else(|| panic!("equipment: no item '{id}'"));
        let left = c.try_insert(&[], d.stack_of(1));
        assert!(matches!(left, Ok(None)), "equipment: the stock of {id} fits");
    };
    if rack == suit_rack(0) || rack == suit_rack(1) {
        // Each suit starts with a full canister (its condition is the canister's fill).
        let d = defs.get("suit").expect("equipment: the suit item");
        let left = c.try_insert(&[], Stack::new(d.item().with_condition(100.0), 1));
        assert!(matches!(left, Ok(None)), "equipment: the suit fits");
    }
    if rack == box_rack(0) || rack == box_rack(1) {
        put(&mut c, "air_canister");
    }
    if rack == locker_rack(0) {
        put(&mut c, "welder");
    }
    c
}

pub fn plugin(app: &mut App) {
    let items = all();
    app.init_resource::<ContainerSpecs>();
    {
        let mut specs = app.world_mut().resource_mut::<ContainerSpecs>();
        for i in &items {
            specs.0.push(ContainerSpec { rack: i.rack, name: i.name.clone(), box_id: i.id.clone(), cols: i.cols });
        }
    }
    {
        let mut devices = app.world_mut().resource_mut::<DevAreaDevices>();
        for i in &items {
            devices.add(PlaceholderBox { id: i.id.clone(), zone: EQUIPMENT, local_pos: i.local_pos, size: i.size, turn_deg: i.turn_deg, colour: i.colour, panel: None });
        }
    }
    app.add_systems(FixedUpdate, (put_stock, fill_canisters).in_set(crate::phases::Fx::Effects));
}

/// Whether the ship gives the canister boxes power: the master is live, the notion the airlock's
/// panel uses (`ReactorNews::startup`). TODO(initiator): a small consumer of its own on the grid;
/// `reactor_core` has no per-consumer power yet, so a box draws nothing from it.
pub fn box_power(sim: &ReactorSim) -> bool {
    sim.room.master_on()
}

/// How much a canister in a box fills in `dt` seconds (percent): the shelf's rate with power, nothing without.
pub fn fill_percent(powered: bool, dt: f64) -> f64 {
    if powered && dt.is_finite() && dt > 0.0 { health_core::mask::REFILL_PER_S * 100.0 * dt } else { 0.0 }
}

/// Host: a canister in a box fills at the shelf's rate (`health_core::mask::REFILL_PER_S`, about
/// 100 s from empty) while the ship gives power, and not at all without.
fn fill_canisters(time: Res<Time>, mut items: ResMut<Items>, sim: Res<ReactorSim>) {
    if items.role == Role::Client {
        return;
    }
    let (dt, now) = (time.delta_secs_f64(), time.elapsed_secs_f64());
    let percent = fill_percent(box_power(&sim), dt);
    if percent <= 0.0 {
        return;
    }
    for i in 0..SUIT_LOCKERS {
        for slot in 0..BOX_PLACES {
            items.fill_in_rack(now, box_rack(i), slot, percent);
        }
    }
}

/// Once, when the ship is there: each locker and box gets its stock (host only; the clients hear it
/// from the host).
fn put_stock(time: Res<Time>, mut items: ResMut<Items>, ships: Query<(), With<Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    *done = true;
    let now = time.elapsed_secs_f64();
    for i in all() {
        let stock = contents(i.rack, &items.defs);
        items.add_container(now, i.rack, stock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::layout::{self, WALL};
    use item_core::container::ItemKind;

    fn defs() -> ItemDefs {
        crate::items::load_defs()
    }

    #[test]
    fn the_lockers_take_what_their_rule_says() {
        let d = defs();
        let suit = d.get("suit").unwrap().item();
        let canister = d.get("air_canister").unwrap().item();
        let welder = d.get("welder").unwrap().item();
        let wrench = d.get("wrench").unwrap().item();
        assert_eq!(empty(suit_rack(0)).len(), 1);
        assert!(empty(suit_rack(0)).rule(0).accepts(&suit) && !empty(suit_rack(0)).rule(0).accepts(&welder), "a suit locker takes the suit only");
        assert_eq!(empty(box_rack(0)).len(), 4);
        assert!(empty(box_rack(0)).rule(3).accepts(&canister) && !empty(box_rack(0)).rule(3).accepts(&suit), "a canister box takes air canisters only");
        assert_eq!(empty(locker_rack(0)).len(), 4);
        assert!(empty(locker_rack(1)).rule(0).accepts(&welder) && empty(locker_rack(1)).rule(0).accepts(&wrench) && !empty(locker_rack(1)).rule(0).accepts(&suit) && !empty(locker_rack(1)).rule(0).accepts(&canister), "an equipment locker takes tools");
    }

    #[test]
    fn a_box_fills_only_with_power_at_the_shelfs_rate() {
        assert_eq!(fill_percent(false, 1.0), 0.0, "no power, no air");
        assert_eq!(fill_percent(true, f64::NAN), 0.0);
        assert!((fill_percent(true, 100.0) - 100.0).abs() < 1e-9, "empty to full in about 100 s");
    }

    #[test]
    fn the_start_stock_is_two_suits_two_spare_canisters_and_a_welder() {
        let d = defs();
        let count = |kind: &str| -> u32 { (0..SUIT_LOCKERS).map(|i| contents(suit_rack(i), &d).count(&ItemKind::new(kind)) + contents(box_rack(i), &d).count(&ItemKind::new(kind))).sum::<u32>() + (0..EQUIPMENT_LOCKERS).map(|i| contents(locker_rack(i), &d).count(&ItemKind::new(kind))).sum::<u32>() };
        assert_eq!(count("suit"), 2);
        assert_eq!(count("air_canister"), 2);
        assert_eq!(count("welder"), 1);
    }

    #[test]
    fn every_locker_stands_inside_the_room_and_clear_of_the_doors() {
        for i in all() {
            let at = EQUIPMENT.point(i.local_pos);
            assert!(at[0] - i.size[0] * 0.5 >= EQUIPMENT.centre[0] - EQUIPMENT.half[0] && at[0] + i.size[0] * 0.5 <= EQUIPMENT.centre[0] + EQUIPMENT.half[0], "{} stands in the room (x)", i.id);
            assert!(at[1] - i.size[2] * 0.5 >= EQUIPMENT.centre[1] - EQUIPMENT.half[1] - 1e-9 && at[1] + i.size[2] * 0.5 <= EQUIPMENT.centre[1] + EQUIPMENT.half[1] + 1e-9, "{} stands in the room (y)", i.id);
            // The doors' walls stay free: 0.3 m between the wall's face and a locker (the AIRLOCK SHIP panel hangs there).
            assert!(at[0] - i.size[0] * 0.5 >= layout::EQUIP_DOOR_AT[0] + WALL * 0.5 + 0.3 && at[0] + i.size[0] * 0.5 <= layout::INNER_DOOR_AT[0] - WALL * 0.5 - 0.3, "{} keeps off the doors' walls", i.id);
        }
    }
}
