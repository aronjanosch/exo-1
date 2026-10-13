//! Containers (from the dev-area round, #287): stack caps, allowed kinds, access, moving, conservation.
use std::collections::BTreeMap;

use item_core::container::*;

fn tag(s: &str) -> Tag {
    Tag::new(s)
}

/// A small medical item that stacks to 8.
fn gauze() -> Item {
    Item::new("space_gauze", &["small", "medical"], 8)
}

/// A medium tool that does not stack.
fn wrench() -> Item {
    Item::new("bendy_wrench", &["medium", "tool"], 1)
}

/// A carrier (a bag) that only goes into the carrier slots.
fn bag() -> Item {
    Item::new("wobble_bag", &["medium", "carrier"], 1)
}

fn stack(item: Item, count: u32) -> Stack {
    Stack::new(item, count)
}

fn small_box(slots: usize, max_stack: u32) -> Container {
    Container::new(vec![SlotRule::any(max_stack); slots], Access::Open)
}

#[test]
fn a_stack_is_capped_by_the_smaller_of_slot_and_item() {
    // Slot cap 5 below the item cap 8: 12 gauze fill 5 + 5, 2 come back.
    let mut c = small_box(2, 5);
    let left = c.try_insert(&[], stack(gauze(), 12)).unwrap();
    assert_eq!(left.map(|s| s.count), Some(2));
    assert_eq!(c.slot(0).unwrap().count, 5);
    assert_eq!(c.slot(1).unwrap().count, 5);

    // Item cap 8 below the slot cap 64.
    let mut c = small_box(2, 64);
    assert!(c.try_insert(&[], stack(gauze(), 10)).unwrap().is_none());
    assert_eq!(c.slot(0).unwrap().count, 8);
    assert_eq!(c.slot(1).unwrap().count, 2);
}

#[test]
fn equal_items_top_up_a_stack_before_a_new_slot_is_used() {
    let mut c = small_box(3, 64);
    c.try_insert(&[], stack(gauze(), 3)).unwrap();
    c.try_insert(&[], stack(wrench(), 1)).unwrap();
    c.try_insert(&[], stack(gauze(), 4)).unwrap();
    assert_eq!(c.slot(0).unwrap().count, 7, "the second gauze joins the first stack");
    assert_eq!(c.slot(1).unwrap().item, wrench());
    assert!(c.slot(2).is_none());
}

#[test]
fn items_with_different_payloads_do_not_stack() {
    let mut c: Container<u32> = Container::new(vec![SlotRule::any(64); 2], Access::Open);
    let vial = |q: u32| Item::new("fizz_vial", &["small"], 10).with_payload(q);
    c.try_insert(&[], Stack::new(vial(1), 2)).unwrap();
    c.try_insert(&[], Stack::new(vial(2), 2)).unwrap();
    assert_eq!(c.slot(0).unwrap().item.payload, Some(1));
    assert_eq!(c.slot(1).unwrap().item.payload, Some(2));
}

#[test]
fn a_full_container_gives_everything_back() {
    let mut c = small_box(1, 64);
    c.try_insert(&[], stack(wrench(), 1)).unwrap();
    let r = c.try_insert(&[], stack(gauze(), 3)).unwrap_err();
    assert_eq!(r.why, Refusal::Full);
    assert_eq!(r.stack, stack(gauze(), 3));
}

#[test]
fn slots_reject_kinds_by_tag() {
    // The medical cabinet's main slots take small and medium items, but no carriers; the
    // carrier slots take one carrier each.
    let mut c: Container = presets::medical_cabinet();
    let r = c.try_insert(&[], stack(Item::new("big_drum", &["large"], 1), 1)).unwrap_err();
    assert_eq!(r.why, Refusal::NotAllowed);
    assert!(c.try_insert(&[], stack(bag(), 1)).unwrap().is_none());
    let carrier_slot = (0..c.len()).find(|&i| c.slot(i).is_some()).unwrap();
    assert!(c.rule(carrier_slot).allow.contains(&tag("carrier")), "the bag went into a carrier slot");
    assert_eq!(c.capacity().used_slots, 1);
}

#[test]
fn a_locked_container_refuses_insert_take_and_move() {
    let mut c = small_box(2, 64);
    c.try_insert(&[], stack(gauze(), 2)).unwrap();
    c.set_access(Access::Locked);
    assert_eq!(c.try_insert(&[], stack(gauze(), 1)).unwrap_err().why, Refusal::Locked);
    assert_eq!(c.take(&[], 0, 1).unwrap_err(), Refusal::Locked);
    let mut other = small_box(2, 64);
    assert_eq!(move_between(&mut c, &mut other, &[], 0, 1).unwrap_err(), Refusal::Locked);
    assert_eq!(c.slot(0).unwrap().count, 2, "nothing left the locked box");
}

#[test]
fn a_keyed_container_opens_only_for_a_carried_key_tag() {
    let mut c: Container = presets::secure_locker();
    assert_eq!(c.access(), &Access::NeedsKey(tag("id_card")));
    let r = c.try_insert(&[tag("wrench")], stack(gauze(), 1)).unwrap_err();
    assert_eq!(r.why, Refusal::NeedsKey(tag("id_card")));
    assert!(c.try_insert(&[tag("id_card")], stack(gauze(), 1)).unwrap().is_none());
    assert_eq!(c.take(&[], 0, 1).unwrap_err(), Refusal::NeedsKey(tag("id_card")));
    assert_eq!(c.take(&[tag("id_card")], 0, 1).unwrap(), stack(gauze(), 1));
}

#[test]
fn take_gives_at_most_what_the_slot_holds() {
    let mut c = small_box(2, 64);
    c.try_insert(&[], stack(gauze(), 5)).unwrap();
    assert_eq!(c.take(&[], 0, 3).unwrap().count, 3);
    assert_eq!(c.take(&[], 0, 9).unwrap().count, 2, "the rest");
    assert!(c.slot(0).is_none(), "an emptied slot is free");
    assert_eq!(c.take(&[], 0, 1).unwrap_err(), Refusal::EmptySlot);
    assert_eq!(c.take(&[], 7, 1).unwrap_err(), Refusal::NoSuchSlot);
    c.try_insert(&[], stack(gauze(), 1)).unwrap();
    assert_eq!(c.take(&[], 0, 0).unwrap_err(), Refusal::NothingToMove);
}

#[test]
fn a_stack_moves_between_two_containers_and_the_rest_stays() {
    let mut cabinet: Container = presets::medical_cabinet();
    let mut crate_: Container = presets::supply_crate();
    cabinet.try_insert(&[], stack(gauze(), 8)).unwrap();
    // The crate has one free slot only, with 3 gauze in it already: 5 fit.
    for _ in 0..crate_.len() - 1 {
        crate_.try_insert(&[], stack(wrench(), 1)).unwrap();
    }
    crate_.try_insert(&[], stack(gauze(), 3)).unwrap();
    let moved = move_between(&mut cabinet, &mut crate_, &[], 0, 8).unwrap();
    assert_eq!(moved, 5);
    assert_eq!(cabinet.slot(0).unwrap().count, 3, "what did not fit stays where it was");
    assert_eq!(crate_.count(&ItemKind::new("space_gauze")), 8);
    // Nothing fits any more: a refusal and no change.
    assert_eq!(move_between(&mut cabinet, &mut crate_, &[], 0, 3).unwrap_err(), Refusal::Full);
    assert_eq!(cabinet.slot(0).unwrap().count, 3);
}

#[test]
fn capacity_reports_slots_and_items_for_a_panel() {
    let mut c: Container = presets::locker();
    let empty = c.capacity();
    assert_eq!(empty.slots, c.len());
    assert_eq!(empty.used_slots, 0);
    c.try_insert(&[], stack(gauze(), 10)).unwrap();
    let cap = c.capacity();
    assert_eq!(cap.used_slots, 2);
    assert_eq!(cap.items, 10);
    assert_eq!(cap.free_slots(), cap.slots - 2);
}

#[test]
fn a_container_round_trips_through_json() {
    let mut c: Container = presets::supply_crate();
    c.try_insert(&[], stack(gauze(), 4)).unwrap();
    let s = serde_json::to_string(&c).unwrap();
    let back: Container = serde_json::from_str(&s).unwrap();
    assert_eq!(back, c);
}

/// A tiny seeded generator for the property test (xorshift64*).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn totals(cs: &[Container], outside: &[Stack]) -> BTreeMap<String, u64> {
    let mut t = BTreeMap::new();
    for c in cs {
        for i in 0..c.len() {
            if let Some(s) = c.slot(i) {
                *t.entry(s.item.kind.as_str().to_string()).or_insert(0) += s.count as u64;
            }
        }
    }
    for s in outside {
        *t.entry(s.item.kind.as_str().to_string()).or_insert(0) += s.count as u64;
    }
    t
}

fn check_slots(c: &Container) {
    for i in 0..c.len() {
        if let Some(s) = c.slot(i) {
            assert!(s.count >= 1, "no empty stacks are kept");
            assert!(s.count <= s.item.max_stack.min(c.rule(i).max_stack), "slot {i} over its cap");
            assert!(c.rule(i).accepts(&s.item), "slot {i} holds a kind it does not allow");
        }
    }
}

#[test]
fn random_moves_never_lose_or_duplicate_an_item() {
    for seed in 1..=20u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut cs: Vec<Container> = vec![presets::medical_cabinet(), presets::supply_crate(), small_box(3, 5), presets::secure_locker()];
        let key = [tag("id_card")];
        let kinds = [gauze(), wrench(), bag(), Item::new("big_drum", &["large"], 1), Item::new("glow_pill", &["small", "medical"], 64)];
        // Everything starts outside, in a pile of stacks.
        let mut outside: Vec<Stack> = kinds.iter().map(|k| Stack::new(k.clone(), 1 + rng.below(80) as u32 * k.max_stack.min(3))).collect();
        let start = totals(&cs, &outside);
        for _ in 0..400 {
            let carried: &[Tag] = if rng.below(2) == 0 { &key } else { &[] };
            match rng.below(4) {
                0 if !outside.is_empty() => {
                    let i = rng.below(outside.len() as u64) as usize;
                    let s = outside.swap_remove(i);
                    let n = 1 + rng.below(s.count as u64) as u32;
                    let (part, rest) = s.split(n);
                    if let Some(r) = rest {
                        outside.push(r);
                    }
                    let c = rng.below(cs.len() as u64) as usize;
                    match cs[c].try_insert(carried, part) {
                        Ok(None) => {}
                        Ok(Some(left)) => outside.push(left),
                        Err(r) => outside.push(r.stack),
                    }
                }
                1 => {
                    let c = rng.below(cs.len() as u64) as usize;
                    let slot = rng.below(cs[c].len() as u64 + 1) as usize;
                    if let Ok(s) = cs[c].take(carried, slot, rng.below(70) as u32) {
                        outside.push(s);
                    }
                }
                _ => {
                    let a = rng.below(cs.len() as u64) as usize;
                    let b = (a + 1 + rng.below(cs.len() as u64 - 1) as usize) % cs.len();
                    let (from, to) = if a < b {
                        let (x, y) = cs.split_at_mut(b);
                        (&mut x[a], &mut y[0])
                    } else {
                        let (x, y) = cs.split_at_mut(a);
                        (&mut y[0], &mut x[b])
                    };
                    let slot = rng.below(from.len() as u64) as usize;
                    let _ = move_between(from, to, carried, slot, rng.below(70) as u32);
                }
            }
            for c in &cs {
                check_slots(c);
            }
            assert_eq!(totals(&cs, &outside), start, "seed {seed}: an item was lost or duplicated");
        }
    }
}
