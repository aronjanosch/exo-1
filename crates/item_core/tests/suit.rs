//! The worn suit and its air canister (#311): the suit sits in the worn place, not in a bar slot,
//! and holds one canister. Whatever moves (bar, worn place, suit, locker, canister box), nothing is
//! lost or duplicated, a refused move changes nothing, and the worn state reaches a client.
use item_core::container::{Access, Container, SlotRule, Stack};
use item_core::wire::{self, ToClient};
use item_core::*;

const SUIT: &str = r#"{ "id": "suit", "name": "Suit", "take": "collect", "hands": 1, "stack": 1, "use": "wear_suit", "keeps_on_use": true, "tags": ["suit"], "shape": { "extents": [0.4, 0.5, 0.3], "mass": 6.0, "colour": [0.9, 0.7, 0.2] } }"#;
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
fn the_suit_goes_on_into_the_worn_place_and_comes_off() {
    let mut host = Host::new(defs());
    put(&mut host, 2, suit(Some(100.0)));
    let before = census(&host);
    do_(&mut host, Request::Wear { slot: 2 }).unwrap();
    let inv = host.inventory(1);
    assert!(inv.slot(2).is_none() && inv.worn.is_some(), "the suit left the bar for the worn place");
    assert_eq!(census(&host), before);
    do_(&mut host, Request::Unwear { slot: Some(5) }).unwrap();
    assert!(host.inventory(1).worn.is_none() && host.inventory(1).slot(5).is_some());
    assert_eq!(census(&host), before);
    // The suit's use does the same as Wear.
    do_(&mut host, Request::Use { slot: 5, target: 1 }).unwrap();
    assert!(host.inventory(1).worn.is_some() && host.inventory(1).slot(5).is_none());
    assert_eq!(census(&host), before);
}

#[test]
fn only_a_suit_goes_in_the_worn_place_and_a_second_suit_swaps() {
    let mut host = Host::new(defs());
    put(&mut host, 0, defs().get("welder").unwrap().stack_of(1));
    put(&mut host, 1, suit(Some(30.0)));
    put(&mut host, 2, suit(Some(90.0)));
    let before = census(&host);
    assert_eq!(do_(&mut host, Request::Wear { slot: 0 }), Err(Reject::NotAllowed), "a welder is no suit");
    assert_eq!(do_(&mut host, Request::Wear { slot: 7 }), Err(Reject::EmptySlot));
    do_(&mut host, Request::Wear { slot: 1 }).unwrap();
    do_(&mut host, Request::Wear { slot: 2 }).unwrap();
    let inv = host.inventory(1);
    assert_eq!(inv.worn_air(), Some(90.0), "the second suit is worn");
    assert_eq!(inv.slot(2).and_then(|s| s.item.condition), Some(30.0), "the first took its slot (and its canister)");
    assert_eq!(census(&host), before);
}

#[test]
fn taking_the_suit_off_with_a_full_bar_changes_nothing() {
    let mut host = Host::new(defs());
    put(&mut host, 0, suit(Some(50.0)));
    do_(&mut host, Request::Wear { slot: 0 }).unwrap();
    for i in 0..BAR_SLOTS {
        put(&mut host, i, defs().get("welder").unwrap().stack_of(1));
    }
    let before = (census(&host), host.inventory(1));
    assert_eq!(do_(&mut host, Request::Unwear { slot: None }), Err(Reject::Full));
    assert_eq!(do_(&mut host, Request::Unwear { slot: Some(3) }), Err(Reject::Full));
    assert_eq!((census(&host), host.inventory(1)), before);
}

#[test]
fn a_canister_swaps_with_the_one_in_the_suit_and_nothing_is_lost() {
    let mut host = Host::new(defs());
    put(&mut host, 0, suit(Some(20.0)));
    do_(&mut host, Request::Wear { slot: 0 }).unwrap();
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
    put(&mut host, 0, suit(None));
    do_(&mut host, Request::Wear { slot: 0 }).unwrap();
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
    put(&mut host, 0, suit(Some(60.0)));
    put(&mut host, 1, defs().get("welder").unwrap().stack_of(1));
    put(&mut host, 2, canister(10.0));
    // No suit worn yet.
    assert_eq!(do_(&mut host, Request::FitCanister { slot: 2 }), Err(Reject::Invalid));
    do_(&mut host, Request::Wear { slot: 0 }).unwrap();
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
    put(&mut host, 0, suit(Some(88.0)));
    do_(&mut host, Request::Wear { slot: 0 }).unwrap();
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
    put(&mut host, 0, suit(Some(1.0)));
    do_(&mut host, Request::Wear { slot: 0 }).unwrap();
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
    put(&mut host, 0, suit(Some(70.0)));
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
    let a = client.request(Request::Wear { slot: 0 }).unwrap();
    assert!(client.view.worn.is_some(), "shown at once");
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
    put(&mut host, 0, suit(Some(30.0)));
    do_(&mut host, Request::Wear { slot: 0 }).unwrap();
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
    do_(&mut host, Request::Unwear { slot: Some(2) }).unwrap();
    assert_eq!(do_(&mut host, Request::FitFrom { rack: 9, rack_slot: 1 }), Err(Reject::Invalid), "no suit worn");
}
