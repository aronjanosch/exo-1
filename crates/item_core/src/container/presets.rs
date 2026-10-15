//! ship's cabinets, TODO(initiator): counts, caps and the tag names (`small`, `medium`,
//! `carrier`, `id_card`).
//! Tags: `small` and `medium` items go into cabinets and crates; `large` ones do not. A `carrier`
//! (a bag or box that holds things itself) only goes into a cabinet's few carrier slots, one each.
use super::{Access, Container, SlotRule, Tag};

pub const ID_CARD: &str = "id_card";

fn cabinet<P: Clone + PartialEq>(main: usize, carriers: usize, access: Access) -> Container<P> {
    let mut rules = vec![SlotRule::tagged(64, &["small", "medium"], &["carrier"]); main];
    rules.extend(vec![SlotRule::tagged(1, &["carrier"], &[]); carriers]);
    Container::new(rules, access)
}

/// Medical cabinet: 35 slots of 64 for small and medium items, 5 carrier slots. Open.
/// TODO(initiator): should it take only `medical` items?
pub fn medical_cabinet<P: Clone + PartialEq>() -> Container<P> {
    cabinet(35, 5, Access::Open)
}

pub fn locker<P: Clone + PartialEq>() -> Container<P> {
    cabinet(35, 5, Access::Open)
}

/// Secure locker: 20 slots and 3 carrier slots, opens for whoever carries an `id_card`.
pub fn secure_locker<P: Clone + PartialEq>() -> Container<P> {
    cabinet(20, 3, Access::NeedsKey(Tag::new(ID_CARD)))
}

/// Supply crate: 20 slots of 32 for small and medium items, no carriers. Open.
pub fn supply_crate<P: Clone + PartialEq>() -> Container<P> {
    Container::new(vec![SlotRule::tagged(32, &["small", "medium"], &["carrier"]); 20], Access::Open)
}
