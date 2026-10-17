//! Cabinets: a container on the host's map that a player stores into and fetches from. Nothing is
//! lost or duplicated, and a refusal changes nothing.
use item_core::authority::{Action, Client, Event, Host, Reject, Request};
use item_core::container::{Access, Container, Stack, SlotRule, Tag};
use item_core::inventory::BAR_SLOTS;
use item_core::container::ItemKind;
use item_core::ItemDefs;

const MASK: &str = r#"{ "id": "o2_mask", "name": "O2 mask", "take": "collect", "hands": 1, "stack": 1, "use": "wear_mask", "keeps_on_use": true, "tags": ["mask", "tool"], "condition": true, "shape": { "extents": [0.2, 0.2, 0.2], "mass": 0.4, "colour": [0.2, 0.2, 0.2] } }"#;
const WRENCH: &str = r#"{ "id": "wrench", "name": "Wrench", "take": "collect", "hands": 1, "stack": 1, "use": "repair_mechanical", "tags": ["small", "tool"], "shape": { "extents": [0.06, 0.04, 0.3], "mass": 0.8, "colour": [0.6, 0.6, 0.65] } }"#;
const CARD: &str = r#"{ "id": "id_card", "name": "ID card", "take": "collect", "hands": 1, "stack": 1, "tags": ["id_card"], "shape": { "extents": [0.09, 0.01, 0.06], "mass": 0.05, "colour": [0.9, 0.9, 0.9] } }"#;

const RACK: u32 = 5;

fn defs() -> ItemDefs {
    ItemDefs::from_files([("o2_mask.json", MASK), ("wrench.json", WRENCH), ("id_card.json", CARD)]).unwrap()
}

fn stack(id: &str, n: u32) -> Stack {
    defs().get(id).unwrap().stack_of(n)
}

fn act(host: &mut Host, player: u32, seq: u32, req: Request) -> Vec<Event> {
    host.handle(&Action { player, seq, req })
}

fn cabinet(access: Access) -> Container {
    Container::new(vec![SlotRule::tagged(1, &["mask"], &[]); 3], access)
}

/// A host with one cabinet and player 1 holding `held` in the bar.
fn host_with(access: Access, held: &[Stack]) -> Host {
    let mut host = Host::new(defs());
    host.add_container(RACK, cabinet(access));
    for s in held {
        host.give(1, s.clone()).expect("the bar takes it");
    }
    host
}

/// Everything the player and the cabinet hold, counted by kind.
fn total(host: &Host, kind: &str) -> u32 {
    let inv = host.inventory(1);
    let bar: u32 = (0..BAR_SLOTS).filter_map(|i| inv.slot(i)).filter(|s| s.item.kind.as_str() == kind).map(|s| s.count).sum();
    bar + host.racks[&RACK].count(&ItemKind::new(kind))
}

#[test]
fn a_stack_goes_into_the_cabinet_and_comes_back_and_nothing_is_lost() {
    let mut host = host_with(Access::Open, &[stack("o2_mask", 1), stack("o2_mask", 1)]);
    assert_eq!(total(&host, "o2_mask"), 2);

    let evs = act(&mut host, 1, 1, Request::Store { slot: 0, rack: RACK });
    assert!(matches!(evs.last(), Some(Event::Confirmed { .. })), "stored: {evs:?}");
    assert!(host.inventory(1).slot(0).is_none(), "the slot is empty now");
    assert_eq!(host.racks[&RACK].slot(0).map(|s| s.count), Some(1));
    assert_eq!(total(&host, "o2_mask"), 2);

    let evs = act(&mut host, 1, 2, Request::Fetch { rack: RACK, rack_slot: 0 });
    assert!(matches!(evs.last(), Some(Event::Confirmed { .. })), "fetched: {evs:?}");
    assert!(host.racks[&RACK].slot(0).is_none(), "the cabinet's slot is empty again");
    assert_eq!(total(&host, "o2_mask"), 2, "still two masks");
}

#[test]
fn a_cabinet_that_does_not_take_the_item_refuses_and_changes_nothing() {
    let mut host = host_with(Access::Open, &[stack("wrench", 1)]);
    let before = host.clone();
    let evs = act(&mut host, 1, 1, Request::Store { slot: 0, rack: RACK });
    assert_eq!(evs, vec![Event::Rejected { player: 1, seq: 1, why: Reject::NotAllowed }]);
    assert_eq!(host.inventory(1), before.inventory(1));
    assert_eq!(host.racks, before.racks);
}

#[test]
fn a_full_cabinet_gives_the_whole_stack_back_to_the_inventory() {
    let masks = [stack("o2_mask", 1), stack("o2_mask", 1), stack("o2_mask", 1), stack("o2_mask", 1)];
    let mut host = host_with(Access::Open, &masks);
    for (seq, slot) in [(1, 0), (2, 1), (3, 2)] {
        act(&mut host, 1, seq, Request::Store { slot, rack: RACK });
    }
    let before = host.clone();
    let evs = act(&mut host, 1, 4, Request::Store { slot: 3, rack: RACK });
    assert_eq!(evs, vec![Event::Rejected { player: 1, seq: 4, why: Reject::Full }]);
    assert_eq!(host.inventory(1), before.inventory(1));
    assert_eq!(host.racks, before.racks);
}

#[test]
fn a_locked_cabinet_opens_only_for_a_carried_key_tag() {
    let key = Access::NeedsKey(Tag::new("id_card"));
    let mut host = host_with(key.clone(), &[stack("o2_mask", 1)]);
    assert_eq!(act(&mut host, 1, 1, Request::Store { slot: 0, rack: RACK }), vec![Event::Rejected { player: 1, seq: 1, why: Reject::Locked }]);

    let mut host = host_with(key, &[stack("o2_mask", 1), stack("id_card", 1)]);
    let evs = act(&mut host, 1, 1, Request::Store { slot: 0, rack: RACK });
    assert!(matches!(evs.last(), Some(Event::Confirmed { .. })), "with the card in the bar it opens: {evs:?}");
}

#[test]
fn a_fetch_into_a_bar_without_room_refuses_and_keeps_the_cabinet() {
    let mut full = cabinet(Access::Open);
    full.try_insert(&[], stack("o2_mask", 1)).unwrap();
    let mut host = Host::new(defs());
    host.add_container(RACK, full);
    for _ in 0..BAR_SLOTS {
        host.give(1, stack("wrench", 1)).expect("room");
    }
    let before = host.clone();
    let evs = act(&mut host, 1, 1, Request::Fetch { rack: RACK, rack_slot: 0 });
    assert!(matches!(evs.as_slice(), [Event::Rejected { why: Reject::Full, .. }]), "{evs:?}");
    assert_eq!(host.racks, before.racks);
}

#[test]
fn a_player_sees_a_store_at_once_and_a_store_to_a_missing_cabinet_is_refused_here() {
    let host = host_with(Access::Open, &[stack("o2_mask", 1)]);
    let mut client = Client::new(1, defs());
    for ev in host.state() {
        client.apply(&ev);
    }
    client.request(Request::Store { slot: 0, rack: RACK }).expect("the cabinet is known");
    assert!(client.view.slot(0).is_none(), "the view shows the mask gone before the host answers");
    assert!(client.request(Request::Store { slot: 0, rack: 99 }).is_err(), "no such cabinet");
}

#[test]
fn store_and_fetch_messages_survive_json() {
    let a = Action { player: 1, seq: 7, req: Request::Store { slot: 2, rack: RACK } };
    let b = Action { player: 1, seq: 8, req: Request::Fetch { rack: RACK, rack_slot: 1 } };
    for x in [a, b] {
        let text = serde_json::to_string(&x).unwrap();
        let back: Action = serde_json::from_str(&text).unwrap();
        assert_eq!(back, x);
    }
}

// ---- the container window (#310): drags to a chosen place, and one player per container ----

fn mixed_total(host: &Host, players: &[u32], kind: &str) -> u32 {
    let bars: u32 = players.iter().map(|p| host.inventory(*p).count(kind)).sum();
    bars + host.racks[&RACK].count(&ItemKind::new(kind))
}

#[test]
fn a_drag_to_a_chosen_place_and_back_to_a_chosen_slot_loses_nothing() {
    let mut host = host_with(Access::Open, &[stack("o2_mask", 1)]);
    let evs = act(&mut host, 1, 1, Request::StoreAt { slot: 0, rack: RACK, rack_slot: 2 });
    assert!(matches!(evs.last(), Some(Event::Confirmed { .. })), "{evs:?}");
    assert!(host.racks[&RACK].slot(2).is_some() && host.racks[&RACK].slot(0).is_none());
    let evs = act(&mut host, 1, 2, Request::MoveIn { rack: RACK, from: 2, to: 1 });
    assert!(matches!(evs.last(), Some(Event::Confirmed { .. })), "{evs:?}");
    assert!(host.racks[&RACK].slot(1).is_some() && host.racks[&RACK].slot(2).is_none());
    let evs = act(&mut host, 1, 3, Request::FetchTo { rack: RACK, rack_slot: 1, slot: 5 });
    assert!(matches!(evs.last(), Some(Event::Confirmed { .. })), "{evs:?}");
    assert!(host.inventory(1).slot(5).is_some());
    assert_eq!(mixed_total(&host, &[1], "o2_mask"), 1);
}

#[test]
fn a_drop_that_is_refused_changes_nothing() {
    let mut host = host_with(Access::Open, &[stack("wrench", 1), stack("o2_mask", 1), stack("o2_mask", 1)]);
    act(&mut host, 1, 1, Request::StoreAt { slot: 1, rack: RACK, rack_slot: 0 });
    let before = host.clone();
    // A wrench does not go in; a mask onto the occupied place of equal items without room (cap 1);
    // a fetch onto an occupied bar slot of other items.
    for (seq, req) in [
        (2, Request::StoreAt { slot: 0, rack: RACK, rack_slot: 1 }),
        (3, Request::StoreAt { slot: 2, rack: RACK, rack_slot: 0 }),
        (4, Request::FetchTo { rack: RACK, rack_slot: 0, slot: 0 }),
        (5, Request::StoreAt { slot: 2, rack: RACK, rack_slot: 9 }),
    ] {
        let evs = act(&mut host, 1, seq, req);
        assert!(matches!(evs.as_slice(), [Event::Rejected { .. }]), "{evs:?}");
        assert_eq!(host.racks, before.racks);
        assert_eq!(host.inventory(1), before.inventory(1));
    }
}

#[test]
fn only_one_player_holds_a_container_and_the_other_cannot_move_items() {
    let mut host = host_with(Access::Open, &[stack("o2_mask", 1)]);
    host.give(2, stack("o2_mask", 1)).unwrap();
    let evs = act(&mut host, 1, 1, Request::Open { rack: RACK });
    assert!(evs.contains(&Event::Lock { rack: RACK, by: Some(1) }), "{evs:?}");
    let evs = act(&mut host, 2, 1, Request::Open { rack: RACK });
    assert_eq!(evs, vec![Event::Rejected { player: 2, seq: 1, why: Reject::InUse }]);
    for (seq, req) in [
        (2, Request::Store { slot: 0, rack: RACK }),
        (3, Request::StoreAt { slot: 0, rack: RACK, rack_slot: 0 }),
        (4, Request::Fetch { rack: RACK, rack_slot: 0 }),
        (5, Request::MoveIn { rack: RACK, from: 0, to: 1 }),
    ] {
        let evs = act(&mut host, 2, seq, req);
        assert_eq!(evs, vec![Event::Rejected { player: 2, seq, why: Reject::InUse }]);
    }
    // The holder moves; a stranger's Close does not free it.
    let evs = act(&mut host, 1, 2, Request::Store { slot: 0, rack: RACK });
    assert!(matches!(evs.last(), Some(Event::Confirmed { .. })));
    act(&mut host, 2, 6, Request::Close { rack: RACK });
    assert_eq!(host.locks.get(&RACK), Some(&1));
    assert_eq!(mixed_total(&host, &[1, 2], "o2_mask"), 2);

    let evs = act(&mut host, 1, 3, Request::Close { rack: RACK });
    assert!(evs.contains(&Event::Lock { rack: RACK, by: None }));
    let evs = act(&mut host, 2, 7, Request::Open { rack: RACK });
    assert!(evs.contains(&Event::Lock { rack: RACK, by: Some(2) }));
}

#[test]
fn a_leaving_player_frees_its_containers() {
    let mut host = host_with(Access::Open, &[]);
    act(&mut host, 1, 1, Request::Open { rack: RACK });
    assert!(host.release_player(2).is_empty());
    assert_eq!(host.release_player(1), vec![Event::Lock { rack: RACK, by: None }]);
    assert!(host.locks.is_empty());
}

#[test]
fn a_client_knows_the_lock_and_a_late_joiner_hears_it() {
    let mut host = host_with(Access::Open, &[stack("o2_mask", 1)]);
    act(&mut host, 1, 1, Request::Open { rack: RACK });
    let mut other = Client::new(2, defs());
    for ev in host.state() {
        other.apply(&ev);
    }
    assert_eq!(other.holder(RACK), Some(1));
    assert_eq!(other.request(Request::Open { rack: RACK }).unwrap_err(), Reject::InUse);
    assert_eq!(other.would(&Request::Fetch { rack: RACK, rack_slot: 0 }), Err(Reject::InUse));
}

#[test]
fn the_window_messages_survive_json() {
    for req in [
        Request::StoreAt { slot: 1, rack: RACK, rack_slot: 3 },
        Request::FetchTo { rack: RACK, rack_slot: 3, slot: 1 },
        Request::MoveIn { rack: RACK, from: 0, to: 1 },
        Request::Open { rack: RACK },
        Request::Close { rack: RACK },
    ] {
        let a = Action { player: 1, seq: 9, req };
        let back: Action = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(back, a);
    }
    let ev = Event::Lock { rack: RACK, by: Some(2) };
    assert_eq!(serde_json::from_str::<Event>(&serde_json::to_string(&ev).unwrap()).unwrap(), ev);
}
