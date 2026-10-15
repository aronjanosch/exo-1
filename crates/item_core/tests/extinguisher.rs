//! Condition: an item that wears keeps its condition through the inventory, the floor, the
//! wire, and a rack swap takes the best spare of its kind.
use item_core::authority::{Action, Client, Event, FrameRef, Host, Placement, Request};
use item_core::container::{Item, Stack};
use item_core::defs::{Shape, Take};
use item_core::wire;
use item_core::{ItemDef, ItemDefs};

fn defs() -> ItemDefs {
    let ext = ItemDef {
        id: "extinguisher".into(),
        name: "Extinguisher".into(),
        code: String::new(),
        take: Take::Collect,
        hands: 2,
        stack: 1,
        slots: 0,
        use_effect: Some("extinguish".into()),
        use_s: None,
        tags: vec!["extinguisher".into()],
        condition: true,
        shape: Shape { extents: [0.2, 0.5, 0.2], mass: 6.0, colour: [1.0, 0.0, 0.0] },
    };
    let mut ex = ext.clone();
    ex.id = "bandage".into();
    ex.stack = 4;
    ex.condition = false;
    ex.tags = vec![];
    ex.use_effect = None;
    ItemDefs([(ext.id.clone(), ext), (ex.id.clone(), ex)].into_iter().collect())
}

fn at() -> Placement {
    Placement { frame: FrameRef::Ship(1), pos: [0.0; 3], vel: [0.0; 3], forward: [0.0, 0.0, -1.0] }
}

fn ext(percent: f64) -> Stack {
    Stack::new(defs().get("extinguisher").unwrap().item().with_condition(percent), 1)
}

fn act(host: &mut Host, player: u32, seq: u32, req: Request) -> Vec<Event> {
    host.handle(&Action { player, seq, req })
}

#[test]
fn a_new_extinguisher_is_at_full_condition_and_wears_down() {
    let d = defs();
    assert_eq!(d.get("extinguisher").unwrap().item().condition, Some(100.0));
    assert_eq!(d.get("bandage").unwrap().item().condition, None);
    let mut host = Host::new(d);
    host.spawn(ext(100.0), at(), None);
    let id = *host.loose.keys().next().unwrap();
    act(&mut host, 1, 1, Request::Collect { item: id });
    assert_eq!(host.condition(1, 0), Some(100.0));
    let ev = host.wear(1, 0, 2.5).expect("a whole percent went");
    assert!(matches!(ev, Event::Inventory { player: 1, .. }));
    assert_eq!(host.condition(1, 0), Some(97.5));
    assert!(host.wear(1, 0, 0.2).is_none(), "no whole percent changed, no event");
    assert_eq!(host.condition(1, 0), Some(97.3));
}

#[test]
fn wear_stops_at_zero_and_says_so_once() {
    let mut host = Host::new(defs());
    host.spawn(ext(1.0), at(), None);
    let id = *host.loose.keys().next().unwrap();
    act(&mut host, 1, 1, Request::Collect { item: id });
    assert!(host.wear(1, 0, 5.0).is_some());
    assert_eq!(host.condition(1, 0), Some(0.0));
    assert!(host.wear(1, 0, 5.0).is_none(), "an empty one does not tell again");
    assert_eq!(host.condition(1, 0), Some(0.0));
}

#[test]
fn condition_survives_drop_and_pick_up_and_the_wire() {
    let mut host = Host::new(defs());
    host.spawn(ext(40.0), at(), None);
    let id = *host.loose.keys().next().unwrap();
    act(&mut host, 1, 1, Request::Collect { item: id });
    act(&mut host, 1, 2, Request::Drop { slot: 0, at: at() });
    let (&loose_id, loose) = host.loose.iter().next().unwrap();
    assert_eq!(loose.stack.item.condition, Some(40.0), "the loose item keeps it");
    // The loose item is sent over the wire and read back.
    let ev = Event::Spawned { item: loose_id, loose: loose.clone(), by: None };
    let back: Event = wire::decode(&wire::encode(&ev)).expect("round trip");
    assert_eq!(back, ev);
    act(&mut host, 1, 3, Request::Collect { item: loose_id });
    assert_eq!(host.condition(1, 0), Some(40.0), "picked up again at 40 %");
    let inv_ev = Event::Inventory { player: 1, inv: host.inventory(1) };
    let back: Event = wire::decode(&wire::encode(&inv_ev)).expect("round trip");
    assert_eq!(back, inv_ev);
}

#[test]
fn different_conditions_do_not_stack() {
    let mut host = Host::new(defs());
    host.spawn(ext(40.0), at(), None);
    host.spawn(ext(90.0), at(), None);
    let ids: Vec<u64> = host.loose.keys().copied().collect();
    act(&mut host, 1, 1, Request::Collect { item: ids[0] });
    act(&mut host, 1, 2, Request::Collect { item: ids[1] });
    assert_eq!(host.inventory(1).count("extinguisher"), 2);
    assert_ne!(Item::<()>::new("x", &[], 1).with_condition(1.0), Item::<()>::new("x", &[], 1).with_condition(2.0));
}

#[test]
fn a_client_sees_the_wear_it_is_told_about() {
    let d = defs();
    let mut host = Host::new(d.clone());
    host.inventories.insert(1, {
        let mut inv = item_core::Inventory::default();
        inv.collect(&d, ext(100.0)).unwrap();
        inv
    });
    let mut client = Client::new(1, d);
    for ev in host.state() {
        client.apply(&ev);
    }
    assert_eq!(client.confirmed.slot(0).unwrap().item.condition, Some(100.0));
    let ev = host.wear(1, 0, 10.0).unwrap();
    client.apply(&ev);
    assert_eq!(client.confirmed.slot(0).unwrap().item.condition, Some(90.0));
}

fn rack_host(spares: Vec<f64>) -> Host {
    let mut host = Host::new(defs());
    let stock = spares.into_iter().map(ext).collect();
    host.add_rack(7, "extinguisher", 3, stock);
    host
}

#[test]
fn a_swap_hangs_the_empty_one_and_takes_the_best_spare() {
    let mut host = rack_host(vec![60.0, 95.0]);
    host.spawn(ext(0.0), at(), None);
    let id = *host.loose.keys().next().unwrap();
    act(&mut host, 1, 1, Request::Collect { item: id });
    let evs = act(&mut host, 1, 2, Request::Swap { slot: 0, rack: 7 });
    assert!(matches!(evs.last(), Some(Event::Confirmed { seq: 2, .. })), "{evs:?}");
    assert_eq!(host.inventory(1).slot(0).unwrap().item.condition, Some(95.0));
    let rack = host.racks.get(&7).unwrap();
    let conds: Vec<Option<f64>> = (0..3).map(|i| rack.slot(i).and_then(|s| s.item.condition)).collect();
    assert!(conds.contains(&Some(0.0)) && conds.contains(&Some(60.0)), "the empty one hangs in the rack: {conds:?}");
    assert_eq!(conds.iter().filter(|c| c.is_some()).count(), 2);
}

#[test]
fn an_empty_hand_takes_a_spare_and_an_empty_rack_refuses() {
    let mut host = rack_host(vec![80.0]);
    let evs = act(&mut host, 1, 1, Request::Swap { slot: 0, rack: 7 });
    assert!(matches!(evs.last(), Some(Event::Confirmed { .. })));
    assert_eq!(host.inventory(1).slot(0).unwrap().item.condition, Some(80.0));
    let evs = act(&mut host, 1, 2, Request::Swap { slot: 1, rack: 7 });
    assert_eq!(evs, vec![Event::Rejected { player: 1, seq: 2, why: item_core::Reject::NoSpare }]);
    let evs = act(&mut host, 1, 3, Request::Swap { slot: 0, rack: 99 });
    assert_eq!(evs, vec![Event::Rejected { player: 1, seq: 3, why: item_core::Reject::NoRack }]);
}

#[test]
fn a_swap_with_the_wrong_kind_changes_nothing() {
    let mut host = rack_host(vec![80.0]);
    let bandage = defs().get("bandage").unwrap().stack_of(2);
    host.inventories.insert(1, {
        let mut inv = item_core::Inventory::default();
        inv.collect(&defs(), bandage).unwrap();
        inv
    });
    let before = host.racks.get(&7).cloned();
    let evs = act(&mut host, 1, 1, Request::Swap { slot: 0, rack: 7 });
    assert!(matches!(evs.as_slice(), [Event::Rejected { .. }]), "{evs:?}");
    assert_eq!(host.racks.get(&7).cloned(), before);
    assert_eq!(host.inventory(1).count("bandage"), 2);
}

#[test]
fn a_client_predicts_the_swap_and_the_rack_reaches_it_as_an_event() {
    let mut host = rack_host(vec![70.0]);
    let mut client = Client::new(1, defs());
    for ev in host.state() {
        client.apply(&ev);
    }
    assert!(client.racks.contains_key(&7));
    host.spawn(ext(0.0), at(), None);
    let id = *host.loose.keys().next().unwrap();
    let mut inv = item_core::Inventory::default();
    inv.collect(&defs(), ext(0.0)).unwrap();
    host.inventories.insert(1, inv.clone());
    client.apply(&Event::Inventory { player: 1, inv });
    client.request(Request::Swap { slot: 0, rack: 7 }).expect("predicted");
    assert_eq!(client.view.slot(0).unwrap().item.condition, Some(70.0));
    let _ = id;
}
