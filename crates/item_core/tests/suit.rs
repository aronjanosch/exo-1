//! The worn suit and its air canister (#311): the suit sits in the worn place, not in a bar slot,
//! and holds one canister. Whatever moves (bar, worn place, suit, locker, canister box), nothing is
//! lost or duplicated, a refused move changes nothing, and the worn state reaches a client.
use item_core::container::{Access, Container, SlotRule, Stack};
use item_core::wire::{self, ToClient};
use item_core::*;

const SUIT: &str = r#"{ "id": "suit", "name": "Suit", "take": "collect", "hands": 1, "stack": 1, "tags": ["suit"], "shape": { "extents": [0.4, 0.5, 0.3], "mass": 6.0, "colour": [0.9, 0.7, 0.2] } }"#;
const CANISTER: &str = r#"{ "id": "air_canister", "name": "Air canister", "code": "AIR", "take": "collect", "hands": 1, "stack": 1, "condition": true, "tags": ["small", "air_canister"], "shape": { "extents": [0.1, 0.3, 0.1], "mass": 1.0, "colour": [0.4, 0.7, 0.9] } }"#;
const WELDER: &str = r#"{ "id": "welder", "name": "Welder", "take": "collect", "hands": 1, "stack": 1, "tags": ["small", "tool"], "shape": { "extents": [0.1, 0.2, 0.3], "mass": 1.5, "colour": [0.9, 0.5, 0.1] } }"#;

fn defs() -> ItemDefs {
    ItemDefs::from_files([("suit.json", SUIT), ("air_canister.json", CANISTER), ("welder.json", WELDER)]).unwrap()
}

fn suit(air: Option<f64>) -> Stack {
    let mut s = defs().get("suit").unwrap().stack_of(1);
    s.item.condition = air;
    s
}

fn canister(fill: f64) -> Stack {
    let mut s = defs().get("air_canister").unwrap().stack_of(1);
    s.item.condition = Some(fill);
    s
}

fn put(host: &mut Host, slot: usize, s: Stack) {
    let mut inv = host.inventory(1);
    inv.bar.insert_at(&[], slot, s).unwrap();
    host.inventories.insert(1, inv);
}

fn at() -> Placement {
    Placement { frame: FrameRef::Ship(1), pos: [0.0, 1.0, 0.0], vel: [0.0; 3], forward: [0.0, 0.0, -1.0] }
}

/// Puts a suit on the way the game does: it lies in the world and F picks it up.
fn wear(host: &mut Host, s: Stack) {
    let Event::Spawned { item, .. } = host.spawn(s, at(), None) else { unreachable!() };
    do_(host, Request::CollectWear { item }).unwrap();
}

fn do_(host: &mut Host, req: Request) -> Result<(), Reject> {
    let evs = host.handle(&Action { player: 1, seq: 1, req });
    match evs.iter().find_map(|e| if let Event::Rejected { why, .. } = e { Some(why.clone()) } else { None }) {
        Some(why) => Err(why),
        None => Ok(()),
    }
}

/// Every suit and every bit of canister fill in the world, wherever it sits (bar, worn place, a
/// suit's canister, a locker, a box): what must stay the same under any move.
fn census(host: &Host) -> Vec<(String, Option<i64>)> {
    let mut all: Vec<(String, Option<i64>)> = Vec::new();
    let mut add = |s: &Stack| {
        for _ in 0..s.count {
            match s.item.kind.as_str() {
                "suit" => {
                    all.push(("suit".into(), None));
                    if let Some(c) = s.item.condition {
                        all.push(("air".into(), Some((c * 100.0).round() as i64)));
                    }
                }
                "air_canister" => all.push(("air".into(), s.item.condition.map(|c| (c * 100.0).round() as i64))),
                other => all.push((other.to_string(), None)),
            }
        }
    };
    for inv in host.inventories.values() {
        (0..BAR_SLOTS).filter_map(|i| inv.slot(i)).for_each(&mut add);
        inv.worn.iter().for_each(&mut add);
    }
    for c in host.racks.values() {
        (0..c.len()).filter_map(|i| c.slot(i)).for_each(&mut add);
    }
    all.sort();
    all
}

fn locker(tag: &str, n: usize) -> Container {
    Container::new(vec![SlotRule::tagged(1, &[tag], &[]); n], Access::Open)
}

#[test]
fn a_loose_suit_goes_on_into_the_worn_place_and_only_one_at_a_time() {
    let mut host = Host::new(defs());
    let Event::Spawned { item, .. } = host.spawn(suit(Some(100.0)), at(), None) else { unreachable!() };
    let Event::Spawned { item: second, .. } = host.spawn(suit(Some(40.0)), at(), None) else { unreachable!() };
    assert!(census(&host).is_empty(), "loose items are not in the census");
    do_(&mut host, Request::CollectWear { item }).unwrap();
    let inv = host.inventory(1);
    assert!(inv.worn.is_some() && (0..BAR_SLOTS).all(|i| inv.slot(i).is_none()), "worn, not in the bar");
    assert!(!host.loose.contains_key(&item));
    // A second suit is not put on over it: nothing changes, it stays on the floor.
    let (loose_before, inv_before) = (host.loose.clone(), host.inventory(1));
    assert_eq!(do_(&mut host, Request::CollectWear { item: second }), Err(Reject::Wearing));
    assert_eq!((host.loose.clone(), host.inventory(1)), (loose_before, inv_before));
    assert_eq!(do_(&mut host, Request::CollectWear { item }), Err(Reject::Gone));
    // The suit and its air are in the world once, wherever they sit (the loose ones are not in the census).
    assert_eq!(census(&host).len(), 2, "one suit and its air are on the body");
}

#[test]
fn a_suit_never_ends_up_in_a_bar_slot_and_a_welder_is_no_suit() {
    let mut host = Host::new(defs());
    host.add_container(7, locker("suit", 1));
    host.add_container(8, Container::new(vec![SlotRule::any(1); 3], Access::Open));
    let Event::Spawned { item: loose_suit, .. } = host.spawn(suit(Some(10.0)), at(), None) else { unreachable!() };
    let Event::Spawned { item: welder, .. } = host.spawn(defs().get("welder").unwrap().stack_of(1), at(), None) else { unreachable!() };
    host.racks.get_mut(&7).unwrap().insert_at(&[], 0, suit(Some(20.0))).unwrap();
    host.racks.get_mut(&8).unwrap().insert_at(&[], 1, suit(Some(30.0))).unwrap();
    let before = (census(&host), host.inventory(1), host.racks.clone(), host.loose.clone());
    assert_eq!(do_(&mut host, Request::CollectWear { item: welder }), Err(Reject::NotAllowed), "a welder is no suit");
    assert_eq!(do_(&mut host, Request::Collect { item: loose_suit }), Err(Reject::NotCollectable), "F collects no suit into the bar");
    assert_eq!(do_(&mut host, Request::Fetch { rack: 7, rack_slot: 0 }), Err(Reject::NotAllowed));
    assert_eq!(do_(&mut host, Request::Fetch { rack: 8, rack_slot: 1 }), Err(Reject::NotAllowed));
    for slot in [0, 3, 7] {
        assert_eq!(do_(&mut host, Request::FetchTo { rack: 7, rack_slot: 0, slot }), Err(Reject::NotCollectable));
        assert_eq!(do_(&mut host, Request::FetchTo { rack: 8, rack_slot: 1, slot }), Err(Reject::NotCollectable));
    }
    assert_eq!((census(&host), host.inventory(1), host.racks.clone(), host.loose.clone()), before, "every refused move changed nothing");
    // A suit on a rack (the extinguisher bracket's request) does not reach the bar either.
    host.add_rack(9, "suit", 1, vec![suit(Some(50.0))]);
    assert!(do_(&mut host, Request::Swap { slot: 0, rack: 9 }).is_err());
    assert!((0..BAR_SLOTS).all(|i| host.inventory(1).slot(i).is_none()));
}

#[test]
fn a_clicked_locker_suit_goes_on_and_a_second_swaps_with_the_first() {
    let mut host = Host::new(defs());
    host.add_container(7, locker("suit", 2));
    host.racks.get_mut(&7).unwrap().insert_at(&[], 0, suit(Some(30.0))).unwrap();
    host.racks.get_mut(&7).unwrap().insert_at(&[], 1, suit(Some(90.0))).unwrap();
    let before = census(&host);
    do_(&mut host, Request::WearFrom { rack: 7, rack_slot: 0 }).unwrap();
    do_(&mut host, Request::WearFrom { rack: 7, rack_slot: 1 }).unwrap();
    assert_eq!(host.inventory(1).worn_air(), Some(90.0), "the second suit is worn");
    assert_eq!(host.racks[&7].slot(1).and_then(|s| s.item.condition), Some(30.0), "the first took its place (and its canister)");
    assert_eq!(census(&host), before);
    assert_eq!(do_(&mut host, Request::WearFrom { rack: 7, rack_slot: 5 }), Err(Reject::NoSuchSlot));
}

#[test]
fn a_canister_swaps_with_the_one_in_the_suit_and_nothing_is_lost() {
    let mut host = Host::new(defs());
    wear(&mut host, suit(Some(20.0)));
    put(&mut host, 4, canister(100.0));
    let before = census(&host);
    do_(&mut host, Request::FitCanister { slot: 4 }).unwrap();
    let inv = host.inventory(1);
    assert_eq!(inv.worn_air(), Some(100.0), "the full canister is in the suit");
    assert_eq!(inv.slot(4).and_then(|s| s.item.condition), Some(20.0), "the old one took the dragged canister's slot");
    assert_eq!(census(&host), before);
    // Empty canister back and forth stays 0, not NaN.
    put(&mut host, 6, canister(0.0));
    do_(&mut host, Request::FitCanister { slot: 6 }).unwrap();
    assert_eq!(host.inventory(1).worn_air(), Some(0.0));
    assert!(host.inventory(1).worn_air().unwrap().is_finite());
}

#[test]
fn a_canister_fits_a_suit_without_one_and_comes_out_again() {
    let mut host = Host::new(defs());
    wear(&mut host, suit(None));
    put(&mut host, 3, canister(75.0));
    let before = census(&host);
    do_(&mut host, Request::FitCanister { slot: 3 }).unwrap();
    assert!(host.inventory(1).slot(3).is_none() && host.inventory(1).worn_air() == Some(75.0));
    assert_eq!(census(&host), before);
    do_(&mut host, Request::PullCanister { slot: None }).unwrap();
    assert_eq!(host.inventory(1).worn_air(), None, "no canister in the suit");
    assert_eq!(host.inventory(1).count("air_canister"), 1);
    assert_eq!(census(&host), before);
    assert_eq!(do_(&mut host, Request::PullCanister { slot: None }), Err(Reject::EmptySlot), "nothing left to pull");
}

#[test]
fn refused_canister_moves_change_nothing() {
    let mut host = Host::new(defs());
    put(&mut host, 1, defs().get("welder").unwrap().stack_of(1));
    put(&mut host, 2, canister(10.0));
    // No suit worn yet.
    assert_eq!(do_(&mut host, Request::FitCanister { slot: 2 }), Err(Reject::Invalid));
    wear(&mut host, suit(Some(60.0)));
    let before = (census(&host), host.inventory(1));
    assert_eq!(do_(&mut host, Request::FitCanister { slot: 1 }), Err(Reject::NotAllowed), "AIR CANISTERS ONLY");
    assert_eq!(do_(&mut host, Request::FitCanister { slot: 5 }), Err(Reject::EmptySlot));
    for i in 0..BAR_SLOTS {
        if host.inventory(1).slot(i).is_none() {
            put(&mut host, i, defs().get("welder").unwrap().stack_of(1));
        }
    }
    let full = (census(&host), host.inventory(1));
    assert_eq!(do_(&mut host, Request::PullCanister { slot: None }), Err(Reject::Full));
    assert_eq!((census(&host), host.inventory(1)), full);
    assert_ne!(before.0, full.0, "(the welders were added)");
}

#[test]
fn the_worn_suit_goes_into_a_suit_locker_and_out_again_but_not_into_a_tool_locker() {
    let mut host = Host::new(defs());
    host.add_container(7, locker("suit", 1));
    host.add_container(8, locker("tool", 4));
    host.add_container(9, locker("air_canister", 4));
    wear(&mut host, suit(Some(88.0)));
    let before = census(&host);
    assert_eq!(do_(&mut host, Request::StoreWorn { rack: 8, rack_slot: 0 }), Err(Reject::NotAllowed), "TOOLS ONLY");
    assert_eq!(do_(&mut host, Request::StoreWorn { rack: 9, rack_slot: 0 }), Err(Reject::NotAllowed));
    assert!(host.inventory(1).worn.is_some(), "a refused store keeps the suit on");
    do_(&mut host, Request::StoreWorn { rack: 7, rack_slot: 0 }).unwrap();
    assert!(host.inventory(1).worn.is_none() && host.racks[&7].count(&container::ItemKind::new("suit")) == 1);
    assert_eq!(census(&host), before);
    do_(&mut host, Request::WearFrom { rack: 7, rack_slot: 0 }).unwrap();
    assert_eq!(host.inventory(1).worn_air(), Some(88.0));
    assert!(host.racks[&7].is_empty());
    assert_eq!(census(&host), before);
}

#[test]
fn a_canister_box_fills_to_full_and_never_over() {
    let mut host = Host::new(defs());
    host.add_container(11, locker("air_canister", 4));
    host.racks.get_mut(&11).unwrap().insert_at(&[], 1, canister(0.0)).unwrap();
    // Eleven steps of the box's rate (health_core::mask: 1 percent a second, 0.1 s steps).
    let mut events = 0;
    for _ in 0..11 {
        events += host.fill_in_rack(11, 1, 0.1).is_some() as u32;
    }
    assert_eq!(events, 1, "one event for the whole percent, not one per step");
    let fill = host.racks[&11].slot(1).unwrap().item.condition.unwrap();
    assert!((fill - 1.1).abs() < 1e-9);
    host.fill_in_rack(11, 1, 5000.0);
    assert_eq!(host.racks[&11].slot(1).unwrap().item.condition, Some(100.0));
    assert!(host.fill_in_rack(11, 0, 1.0).is_none(), "an empty place fills nothing");
    host.racks.get_mut(&11).unwrap().insert_at(&[], 2, canister(f64::from(50u8))).unwrap();
    host.fill_in_rack(11, 2, f64::INFINITY);
    assert!(host.racks[&11].slot(2).unwrap().item.condition.unwrap().is_finite());
}

#[test]
fn the_worn_suit_drains_to_empty_and_stays_in_range() {
    let mut host = Host::new(defs());
    wear(&mut host, suit(Some(1.0)));
    assert!(host.drain_worn(1, 0.5).is_none() || host.inventory(1).worn_air() == Some(0.5));
    assert!(host.drain_worn(1, 10.0).is_some(), "the canister running out is told");
    assert_eq!(host.inventory(1).worn_air(), Some(0.0));
    host.drain_worn(1, -1000.0);
    assert_eq!(host.inventory(1).worn_air(), Some(100.0));
    assert!(host.drain_worn(2, 1.0).is_none(), "nobody there");
}

/// A client hears the worn suit and predicts its own moves the way the host answers.
#[test]
fn a_client_sees_the_worn_suit_and_its_canister_over_the_wire() {
    let mut host = Host::new(defs());
    host.add_container(7, locker("suit", 1));
    put(&mut host, 1, canister(100.0));
    let mut client = Client::new(1, defs());
    let tell = |client: &mut Client, evs: Vec<Event>| {
        for ev in evs {
            let bytes = wire::encode(&ToClient::Event(ev));
            let Some(ToClient::Event(ev)) = wire::decode::<ToClient>(&bytes) else { panic!("the event survives the wire") };
            client.apply(&ev);
        }
    };
    let state = host.state();
    tell(&mut client, state);
    assert!(client.view.worn.is_none());
    let Event::Spawned { item, .. } = host.spawn(suit(Some(70.0)), at(), None) else { unreachable!() };
    tell(&mut client, vec![Event::Spawned { item, loose: host.loose[&item].clone(), by: None }]);
    let a = client.request(Request::CollectWear { item }).unwrap();
    assert!(client.view.worn.is_some(), "shown at once");
    assert!(client.hidden.contains(&item));
    assert_eq!(client.request(Request::CollectWear { item }), Err(Reject::Gone), "the pending collect hides it");
    let evs = host.handle(&a);
    tell(&mut client, evs);
    assert_eq!(client.confirmed.worn_air(), Some(70.0), "the host's word: worn, with its canister");
    let a = client.request(Request::FitCanister { slot: 1 }).unwrap();
    assert_eq!(client.view.worn_air(), Some(100.0));
    let evs = host.handle(&a);
    tell(&mut client, evs);
    assert_eq!(client.confirmed, host.inventory(1));
    assert_eq!(client.view, host.inventory(1));
    // A refused canister move is refused without sending.
    assert_eq!(client.request(Request::FitCanister { slot: 3 }), Err(Reject::EmptySlot));
    // Taking it off into the locker.
    let a = client.request(Request::StoreWorn { rack: 7, rack_slot: 0 }).unwrap();
    let evs = host.handle(&a);
    tell(&mut client, evs);
    assert!(client.confirmed.worn.is_none() && client.racks[&7].count(&container::ItemKind::new("suit")) == 1);
    // The count of suits and canisters is the same at both ends.
    assert_eq!(client.view, host.inventory(1));
    assert_eq!(client.racks[&7], host.racks[&7]);
}

#[test]
fn the_suits_canister_goes_into_a_canister_box_and_back_but_not_into_a_suit_locker() {
    let mut host = Host::new(defs());
    host.add_container(7, locker("suit", 1));
    host.add_container(9, locker("air_canister", 4));
    host.racks.get_mut(&9).unwrap().insert_at(&[], 1, canister(100.0)).unwrap();
    wear(&mut host, suit(Some(30.0)));
    let before = census(&host);
    assert_eq!(do_(&mut host, Request::StoreCanister { rack: 7, rack_slot: 0 }), Err(Reject::NotAllowed), "SUIT ONLY");
    assert_eq!(do_(&mut host, Request::StoreCanister { rack: 9, rack_slot: 1 }), Err(Reject::Full), "the place is taken");
    assert_eq!(census(&host), before, "a refused store changes nothing");
    // A drop on the box's full canister swaps: the full one goes in, the 30 % one takes its place.
    do_(&mut host, Request::FitFrom { rack: 9, rack_slot: 1 }).unwrap();
    assert_eq!(host.inventory(1).worn_air(), Some(100.0));
    assert_eq!(host.racks[&9].slot(1).unwrap().item.condition, Some(30.0));
    assert_eq!(census(&host), before);
    do_(&mut host, Request::StoreCanister { rack: 9, rack_slot: 3 }).unwrap();
    assert_eq!(host.inventory(1).worn_air(), None);
    assert_eq!(census(&host), before);
    do_(&mut host, Request::FitFrom { rack: 9, rack_slot: 3 }).unwrap();
    assert_eq!(host.inventory(1).worn_air(), Some(100.0));
    assert!(host.racks[&9].slot(3).is_none(), "a suit without a canister leaves the place empty");
    assert_eq!(census(&host), before);
    do_(&mut host, Request::StoreWorn { rack: 7, rack_slot: 0 }).unwrap();
    assert_eq!(do_(&mut host, Request::FitFrom { rack: 9, rack_slot: 1 }), Err(Reject::Invalid), "no suit worn");
}

#[test]
fn a_canister_swaps_into_a_stored_suit_and_comes_out_again() {
    let mut host = Host::new(defs());
    host.add_container(7, locker("suit", 2));
    host.racks.get_mut(&7).unwrap().insert_at(&[], 0, suit(Some(30.0))).unwrap();
    host.racks.get_mut(&7).unwrap().insert_at(&[], 1, suit(None)).unwrap();
    put(&mut host, 2, canister(100.0));
    put(&mut host, 5, canister(55.0));
    let before = census(&host);
    do_(&mut host, Request::FitStoredCanister { slot: 2, rack: 7, rack_slot: 0 }).unwrap();
    assert_eq!(host.racks[&7].slot(0).and_then(|s| s.item.condition), Some(100.0), "the full canister is in the stored suit");
    assert_eq!(host.inventory(1).slot(2).and_then(|s| s.item.condition), Some(30.0), "the old one took the dragged canister's slot");
    assert_eq!(census(&host), before);
    // Into a suit without one: the slot is free afterwards.
    do_(&mut host, Request::FitStoredCanister { slot: 5, rack: 7, rack_slot: 1 }).unwrap();
    assert_eq!(host.racks[&7].slot(1).and_then(|s| s.item.condition), Some(55.0));
    assert!(host.inventory(1).slot(5).is_none());
    assert_eq!(census(&host), before);
    // Out: to the asked slot, then to the first free one.
    do_(&mut host, Request::PullStoredCanister { rack: 7, rack_slot: 1, slot: Some(6) }).unwrap();
    assert_eq!(host.inventory(1).slot(6).and_then(|s| s.item.condition), Some(55.0));
    assert_eq!(host.racks[&7].slot(1).and_then(|s| s.item.condition), None, "no canister in the stored suit now");
    do_(&mut host, Request::PullStoredCanister { rack: 7, rack_slot: 0, slot: None }).unwrap();
    assert!(host.racks[&7].slot(0).is_some_and(|s| s.item.condition.is_none()));
    assert_eq!(census(&host), before);
    assert!(host.inventory(1).worn.is_none(), "the worn suit was never touched");
}

#[test]
fn refused_stored_canister_moves_change_nothing() {
    let mut host = Host::new(defs());
    host.add_container(7, locker("suit", 2));
    host.add_container(8, Container::new(vec![SlotRule::any(1); 2], Access::Open));
    host.racks.get_mut(&7).unwrap().insert_at(&[], 0, suit(Some(30.0))).unwrap();
    host.racks.get_mut(&7).unwrap().insert_at(&[], 1, suit(None)).unwrap();
    host.racks.get_mut(&8).unwrap().insert_at(&[], 0, defs().get("welder").unwrap().stack_of(1)).unwrap();
    put(&mut host, 0, defs().get("welder").unwrap().stack_of(1));
    put(&mut host, 1, canister(80.0));
    let before = (census(&host), host.inventory(1), host.racks.clone());
    let fit = |slot, rack, rack_slot| Request::FitStoredCanister { slot, rack, rack_slot };
    assert_eq!(do_(&mut host, fit(0, 7, 0)), Err(Reject::NotAllowed), "AIR CANISTERS ONLY");
    assert_eq!(do_(&mut host, fit(4, 7, 0)), Err(Reject::EmptySlot), "nothing in that bar slot");
    assert_eq!(do_(&mut host, fit(1, 8, 0)), Err(Reject::Invalid), "a welder is no suit");
    assert_eq!(do_(&mut host, fit(1, 8, 1)), Err(Reject::EmptySlot), "no suit in that place");
    assert_eq!(do_(&mut host, fit(1, 7, 9)), Err(Reject::NoSuchSlot));
    assert_eq!(do_(&mut host, fit(1, 99, 0)), Err(Reject::NoRack));
    let pull = |rack, rack_slot, slot| Request::PullStoredCanister { rack, rack_slot, slot };
    assert_eq!(do_(&mut host, pull(7, 1, None)), Err(Reject::EmptySlot), "no canister to pull");
    assert_eq!(do_(&mut host, pull(8, 0, None)), Err(Reject::Invalid));
    assert_eq!(do_(&mut host, pull(7, 0, Some(0))), Err(Reject::Full), "the bar slot is taken");
    for i in 0..BAR_SLOTS {
        if host.inventory(1).slot(i).is_none() {
            put(&mut host, i, defs().get("welder").unwrap().stack_of(1));
        }
    }
    let full = (census(&host), host.inventory(1), host.racks.clone());
    assert_eq!(do_(&mut host, pull(7, 0, None)), Err(Reject::Full));
    assert_eq!((census(&host), host.inventory(1), host.racks.clone()), full);
    assert_ne!(before.0, full.0, "(the welders were added)");
    // Another player's window: refused as in use.
    host.locks.insert(7, 2);
    assert_eq!(do_(&mut host, pull(7, 0, Some(1))), Err(Reject::InUse));
    assert_eq!(do_(&mut host, fit(1, 7, 0)), Err(Reject::InUse));
}

#[test]
fn a_client_predicts_a_stored_suits_canister_as_the_host_answers() {
    let mut host = Host::new(defs());
    host.add_container(7, locker("suit", 1));
    host.racks.get_mut(&7).unwrap().insert_at(&[], 0, suit(Some(30.0))).unwrap();
    put(&mut host, 3, canister(100.0));
    let mut client = Client::new(1, defs());
    for ev in host.state() {
        client.apply(&ev);
    }
    let a = client.request(Request::FitStoredCanister { slot: 3, rack: 7, rack_slot: 0 }).unwrap();
    assert_eq!(client.racks[&7].slot(0).and_then(|s| s.item.condition), Some(30.0), "the host's rack is unchanged until it answers");
    let evs = host.handle(&a);
    for ev in evs {
        let bytes = wire::encode(&ToClient::Event(ev));
        let Some(ToClient::Event(ev)) = wire::decode::<ToClient>(&bytes) else { panic!("the event survives the wire") };
        client.apply(&ev);
    }
    assert_eq!(client.racks[&7], host.racks[&7]);
    assert_eq!(client.confirmed, host.inventory(1));
    assert_eq!(client.view, host.inventory(1));
    assert_eq!(client.request(Request::PullStoredCanister { rack: 7, rack_slot: 5, slot: None }), Err(Reject::NoSuchSlot), "refused without sending");
}
