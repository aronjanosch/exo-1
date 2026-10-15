use item_core::container::Refusal;
use item_core::*;

const WRENCH: &str = r#"{ "id": "wrench", "name": "Wrench", "take": "collect", "hands": 1, "stack": 1, "use": "repair_mechanical", "tags": ["tool"], "shape": { "extents": [0.06, 0.04, 0.3], "mass": 0.8, "colour": [0.6, 0.6, 0.65] } }"#;
const BANDAGE: &str = r#"{ "id": "bandage", "name": "Bandage", "take": "collect", "hands": 1, "stack": 4, "use": "bandage", "tags": ["medical"], "shape": { "extents": [0.1, 0.05, 0.1], "mass": 0.1, "colour": [0.9, 0.9, 0.85] } }"#;
const EXTINGUISHER: &str = r#"{ "id": "extinguisher", "name": "Extinguisher", "take": "collect", "hands": 2, "stack": 1, "use": "extinguish", "shape": { "extents": [0.18, 0.5, 0.18], "mass": 6.0, "colour": [0.85, 0.15, 0.1] } }"#;
const BARREL: &str = r#"{ "id": "barrel", "name": "Barrel", "take": "carry", "hands": 2, "stack": 1, "shape": { "extents": [0.6, 0.9, 0.6], "mass": 60.0, "colour": [0.3, 0.3, 0.3] } }"#;

fn defs() -> ItemDefs {
    ItemDefs::from_files([("wrench.json", WRENCH), ("bandage.json", BANDAGE), ("extinguisher.json", EXTINGUISHER), ("barrel.json", BARREL)]).unwrap()
}

fn here() -> Placement {
    Placement { frame: FrameRef::Planet(0), pos: [0.0; 3], vel: [0.0; 3], forward: [0.0, 0.0, -1.0] }
}

fn stack(id: &str, n: u32) -> item_core::container::Stack {
    defs().get(id).unwrap().stack_of(n)
}

#[test]
fn an_item_has_a_three_letter_code_for_the_bar() {
    // Given in its file, or the first three letters of its id until it gets one.
    let coded = WRENCH.replace("\"name\"", "\"code\": \"WRN\", \"name\"");
    let d = ItemDefs::from_files([("wrench.json", coded.as_str()), ("bandage.json", BANDAGE)]).unwrap();
    assert_eq!(d.get("wrench").unwrap().short_code(), "WRN");
    assert_eq!(d.get("bandage").unwrap().short_code(), "BAN", "no code in the file: the id's first letters");
    for bad in ["wr", "WRNX", "wrn", "W-N"] {
        let t = WRENCH.replace("\"name\"", &format!("\"code\": \"{bad}\", \"name\""));
        assert!(ItemDefs::from_files([("wrench.json", t.as_str())]).unwrap_err().contains("code"), "{bad}");
    }
}

#[test]
fn definitions_are_checked() {
    let d = defs();
    assert_eq!(d.get("extinguisher").unwrap().hands, 2);
    assert_eq!(d.get("wrench").unwrap().use_effect.as_deref(), Some("repair_mechanical"));
    let bad = WRENCH.replace("\"hands\": 1", "\"hands\": 3");
    assert!(ItemDefs::from_files([("wrench.json", bad.as_str())]).unwrap_err().contains("hands"));
    assert!(ItemDefs::from_files([("spanner.json", WRENCH)]).unwrap_err().contains("wrench.json"));
    let unknown = WRENCH.replace("\"stack\": 1", "\"stack\": 1, \"weight\": 3");
    assert!(ItemDefs::from_files([("wrench.json", unknown.as_str())]).is_err(), "unknown fields are rejected");
    let stacked_cargo = BARREL.replace("\"stack\": 1", "\"stack\": 2");
    assert!(ItemDefs::from_files([("barrel.json", stacked_cargo.as_str())]).unwrap_err().contains("does not stack"));
}

#[test]
fn a_collected_item_lands_in_the_empty_hand_then_in_the_first_free_slot() {
    let d = defs();
    let mut inv = Inventory::default();
    inv.select(3).unwrap();
    inv.collect(&d, stack("wrench", 1)).unwrap();
    assert_eq!(inv.slot(3).unwrap().item.kind.as_str(), "wrench", "into the hand");
    inv.collect(&d, stack("extinguisher", 1)).unwrap();
    assert_eq!(inv.slot(0).unwrap().item.kind.as_str(), "extinguisher", "hand busy: first free slot");
}

#[test]
fn bandages_stack_to_four_and_a_full_bar_refuses_without_change() {
    let d = defs();
    let mut inv = Inventory::default();
    inv.collect(&d, stack("bandage", 3)).unwrap();
    inv.select(5).unwrap();
    inv.collect(&d, stack("bandage", 1)).unwrap();
    assert_eq!(inv.slot(0).unwrap().count, 4, "equal stacks are topped up before the empty hand");
    assert!(inv.slot(5).is_none());
    inv.collect(&d, stack("bandage", 2)).unwrap();
    assert_eq!(inv.count("bandage"), 6);
    assert_eq!(inv.slot(5).unwrap().count, 2, "the rest into the empty hand");
    for _ in 0..6 {
        inv.collect(&d, stack("wrench", 1)).unwrap();
    }
    let before = inv.clone();
    assert_eq!(inv.collect(&d, stack("wrench", 1)), Err(Refusal::Full));
    assert_eq!(inv, before, "a refused collect changes nothing");
    assert_eq!(inv.collect(&d, stack("barrel", 1)), Err(Refusal::NotAllowed), "cargo is carried, not collected");
}

#[test]
fn moving_in_the_bar_stacks_or_swaps() {
    let d = defs();
    let mut inv = Inventory::default();
    inv.collect(&d, stack("bandage", 3)).unwrap(); // slot 0 (hand)
    inv.select(7).unwrap();
    inv.collect(&d, stack("wrench", 1)).unwrap(); // slot 7
    inv.move_slot(7, 0).unwrap();
    assert_eq!(inv.slot(0).unwrap().item.kind.as_str(), "wrench");
    assert_eq!(inv.slot(7).unwrap().count, 3, "swapped");
    inv.move_slot(7, 2).unwrap();
    assert!(inv.slot(7).is_none() && inv.slot(2).unwrap().count == 3, "into an empty slot");
    let mut more = inv.clone();
    more.select(4).unwrap();
    more.collect(&d, stack("bandage", 1)).unwrap();
    assert_eq!(more.slot(2).unwrap().count, 4, "topped up, not a new stack");
    assert_eq!(inv.move_slot(5, 6), Err(Refusal::EmptySlot));
}

#[test]
fn hands_limit_what_is_held_next_to_cargo() {
    let d = defs();
    let mut inv = Inventory::default();
    inv.collect(&d, stack("extinguisher", 1)).unwrap();
    assert_eq!(inv.held_hands(&d), 2);
    assert!(!inv.may_carry(&d, 1), "both hands hold the extinguisher");
    assert!(inv.in_hand(&d, 0).is_some());
    assert!(inv.in_hand(&d, 1).is_none(), "carrying lowers a two-handed item");
    inv.select(1).unwrap();
    inv.collect(&d, stack("wrench", 1)).unwrap();
    assert!(inv.may_carry(&d, 1) && !inv.may_carry(&d, 2));
    assert!(inv.in_hand(&d, 1).is_some(), "a wrench fits next to a one-handed crate");
}

struct Session {
    host: Host,
    a: Client,
    b: Client,
}

impl Session {
    fn new() -> Session {
        let mut host = Host::new(defs());
        let (mut a, mut b) = (Client::new(1, defs()), Client::new(2, defs()));
        let ev = host.spawn(stack("wrench", 1), here(), None);
        a.apply(&ev);
        b.apply(&ev);
        Session { host, a, b }
    }
    fn deliver(&mut self, act: &Action) {
        for ev in self.host.handle(act) {
            self.a.apply(&ev);
            self.b.apply(&ev);
        }
    }
}

#[test]
fn a_collect_shows_at_once_and_the_host_confirms_it() {
    let mut s = Session::new();
    let act = s.a.request(Request::Collect { item: 1 }).unwrap();
    assert_eq!(s.a.view.count("wrench"), 1, "predicted before any answer");
    assert!(s.a.hidden.contains(&1), "the loose item hides at once");
    assert_eq!(s.a.confirmed.count("wrench"), 0);
    s.deliver(&act);
    assert_eq!(s.a.confirmed.count("wrench"), 1);
    assert!(s.a.pending.is_empty() && s.a.loose.is_empty() && s.b.loose.is_empty());
    assert_eq!(s.host.inventory(1).count("wrench"), 1);
}

#[test]
fn two_players_collect_the_same_item_in_the_same_tick_and_the_second_rolls_back() {
    let mut s = Session::new();
    let first = s.a.request(Request::Collect { item: 1 }).unwrap();
    let second = s.b.request(Request::Collect { item: 1 }).unwrap();
    assert_eq!(s.a.view.count("wrench") + s.b.view.count("wrench"), 2, "both predict");
    s.deliver(&first);
    s.deliver(&second);
    assert_eq!(s.a.view.count("wrench"), 1, "the first keeps it");
    assert_eq!(s.b.view.count("wrench"), 0, "the second's prediction rolls back");
    assert_eq!(s.b.rejected, vec![(second.seq, Reject::Gone)]);
    assert!(s.b.pending.is_empty() && s.b.hidden.is_empty());
    assert_eq!(s.host.inventory(1).count("wrench") + s.host.inventory(2).count("wrench"), 1, "one wrench in the world");
}

#[test]
fn the_losers_view_drops_the_item_as_soon_as_the_winner_is_confirmed() {
    let mut s = Session::new();
    let first = s.a.request(Request::Collect { item: 1 }).unwrap();
    let _second = s.b.request(Request::Collect { item: 1 }).unwrap();
    s.deliver(&first);
    assert_eq!(s.b.view.count("wrench"), 0, "the item went to the other player: gone from the view before the reject arrives");
}

#[test]
fn a_full_inventory_is_refused_here_and_at_the_host() {
    let mut s = Session::new();
    for k in 0..8 {
        let ev = s.host.spawn(stack("wrench", 1), here(), None);
        s.a.apply(&ev);
        let id = 2 + k;
        let act = s.a.request(Request::Collect { item: id }).unwrap();
        s.deliver(&act);
    }
    assert_eq!(s.a.view.count("wrench"), 8);
    assert_eq!(s.a.request(Request::Collect { item: 1 }), Err(Reject::Full), "refused locally, nothing sent");
    // Sent anyway (a stale client): the host refuses too, and the wrench stays in the world.
    let forged = Action { player: 1, seq: 99, req: Request::Collect { item: 1 } };
    assert_eq!(s.host.handle(&forged), vec![Event::Rejected { player: 1, seq: 99, why: Reject::Full }]);
    assert!(s.host.loose.contains_key(&1));
}

#[test]
fn a_drop_names_its_action_so_the_dropper_keeps_its_own_item() {
    let mut s = Session::new();
    let act = s.a.request(Request::Collect { item: 1 }).unwrap();
    s.deliver(&act);
    let at = Placement { vel: [0.0, 2.0, -6.0], ..here() };
    let throw = s.a.request(Request::Throw { slot: 0, at }).unwrap();
    assert_eq!(s.a.view.count("wrench"), 0);
    assert!(s.a.pending_drop(throw.seq));
    let evs = s.host.handle(&throw);
    let spawned = evs.iter().find_map(|e| if let Event::Spawned { item, loose, by } = e { Some((*item, loose.clone(), *by)) } else { None }).unwrap();
    assert_eq!(spawned.2, Some((1, throw.seq)));
    assert_eq!(spawned.1.at.vel, [0.0, 2.0, -6.0]);
    for e in &evs {
        s.a.apply(e);
        s.b.apply(e);
    }
    assert!(!s.a.pending_drop(throw.seq));
    assert!(s.b.loose.contains_key(&spawned.0), "the other player sees it lie there");
}

#[test]
fn select_and_move_go_the_same_path() {
    let mut s = Session::new();
    let act = s.a.request(Request::Collect { item: 1 }).unwrap();
    s.deliver(&act);
    let sel = s.a.request(Request::Select { slot: 4 }).unwrap();
    let mv = s.a.request(Request::Move { from: 0, to: 4 }).unwrap();
    assert_eq!(s.a.view.selected, 4);
    assert_eq!(s.a.view.slot(4).unwrap().item.kind.as_str(), "wrench");
    s.deliver(&sel);
    s.deliver(&mv);
    assert_eq!(s.host.inventory(1), s.a.view);
    assert_eq!(s.a.request(Request::Move { from: 0, to: 4 }), Err(Reject::EmptySlot));
    assert_eq!(s.a.request(Request::Select { slot: 8 }), Err(Reject::NoSuchSlot));
}

#[test]
fn messages_survive_json_for_the_network_step() {
    let mut s = Session::new();
    let act = s.a.request(Request::Collect { item: 1 }).unwrap();
    let wire = serde_json::to_string(&act).unwrap();
    let back: Action = serde_json::from_str(&wire).unwrap();
    assert_eq!(back, act);
    for ev in s.host.handle(&back) {
        let ev2: Event = serde_json::from_str(&serde_json::to_string(&ev).unwrap()).unwrap();
        assert_eq!(ev2, ev);
    }
}

#[test]
fn the_state_for_a_late_joiner_lists_the_world_and_every_inventory() {
    let mut s = Session::new();
    let act = s.a.request(Request::Collect { item: 1 }).unwrap();
    s.deliver(&act);
    s.host.spawn(stack("bandage", 3), Placement { pos: [2.0, 0.0, 0.0], ..here() }, None);
    let state = s.host.state();
    assert!(state.iter().any(|e| matches!(e, Event::Spawned { loose, by: None, .. } if loose.stack.item.kind.as_str() == "bandage" && loose.stack.count == 3)));
    assert!(state.iter().any(|e| matches!(e, Event::Inventory { player: 1, .. })));
    let mut late = Client::new(3, defs());
    for e in &state {
        late.apply(e);
    }
    assert_eq!(late.loose, s.host.loose, "the loose items as the host has them");
    assert_eq!(late.others.get(&1), Some(&s.host.inventory(1)), "the first player's bar");
    assert_eq!(late.held_by(1).map(|st| st.item.kind.as_str()), Some("wrench"));
}

#[test]
fn a_rest_updates_the_host_and_every_client_and_a_gone_item_has_none() {
    let mut s = Session::new();
    let rest = Placement { pos: [1.0, 2.0, 3.0], ..here() };
    let ev = s.host.rest(1, rest, [0.0, 0.0, 0.0, 1.0], 4.5).expect("the item is there");
    s.a.apply(&ev);
    assert_eq!(s.host.loose[&1].at.pos, [1.0, 2.0, 3.0]);
    assert_eq!(s.a.loose[&1].at.pos, [1.0, 2.0, 3.0]);
    let json = serde_json::to_string(&ev).unwrap();
    assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), ev);
    assert!(s.host.rest(99, rest, [0.0, 0.0, 0.0, 1.0], 5.0).is_none());
}

#[test]
fn a_placement_that_cannot_be_is_refused() {
    let mut s = Session::new();
    let act = s.a.request(Request::Collect { item: 1 }).unwrap();
    s.deliver(&act);
    for bad in [
        Placement { pos: [f64::NAN, 0.0, 0.0], ..here() },
        Placement { vel: [0.0, 1.0e6, 0.0], ..here() },
        Placement { forward: [0.0; 3], ..here() },
        Placement { pos: [1.0e12, 0.0, 0.0], ..here() },
    ] {
        let forged = Action { player: 1, seq: 77, req: Request::Throw { slot: 0, at: bad } };
        assert_eq!(s.host.handle(&forged), vec![Event::Rejected { player: 1, seq: 77, why: Reject::Invalid }]);
    }
    assert_eq!(s.host.inventory(1).count("wrench"), 1, "nothing left the bar");
}

#[test]
fn a_use_takes_one_from_the_stack_and_the_host_names_the_effect() {
    let mut host = Host::new(defs());
    let mut inv = Inventory::default();
    inv.collect(&defs(), stack("bandage", 3)).unwrap();
    host.inventories.insert(2, inv);
    let mut c = Client::new(2, defs());
    c.apply(&Event::Inventory { player: 2, inv: host.inventory(2) });
    let a = c.request(Request::Use { slot: 0, target: 1 }).unwrap();
    assert_eq!(c.view.count("bandage"), 2, "shown at once");
    let evs = host.handle(&a);
    assert!(evs.contains(&Event::Used { player: 2, target: 1, effect: "bandage".into() }), "{evs:?}");
    assert_eq!(host.inventory(2).count("bandage"), 2);
    for e in &evs {
        c.apply(e);
    }
    assert_eq!(c.confirmed.count("bandage"), 2);
    assert!(c.pending.is_empty());
}

#[test]
fn an_empty_slot_or_an_item_without_a_use_is_refused() {
    let mut host = Host::new(defs());
    let a = Action { player: 2, seq: 1, req: Request::Use { slot: 0, target: 2 } };
    assert_eq!(host.handle(&a), vec![Event::Rejected { player: 2, seq: 1, why: Reject::EmptySlot }]);
    const RAG: &str = r#"{ "id": "rag", "name": "Rag", "take": "collect", "hands": 1, "stack": 1, "shape": { "extents": [0.2, 0.01, 0.2], "mass": 0.05, "colour": [0.5, 0.5, 0.5] } }"#;
    let rags = ItemDefs::from_files([("rag.json", RAG)]).unwrap();
    let mut host = Host::new(rags.clone());
    let mut inv = Inventory::default();
    inv.collect(&rags, rags.get("rag").unwrap().stack_of(1)).unwrap();
    host.inventories.insert(2, inv);
    let a = Action { player: 2, seq: 2, req: Request::Use { slot: 0, target: 2 } };
    assert_eq!(host.handle(&a), vec![Event::Rejected { player: 2, seq: 2, why: Reject::NotUsable }]);
    let mut c = Client::new(2, defs());
    assert!(c.request(Request::Use { slot: 5, target: 2 }).is_err(), "an empty slot is refused without sending");
}

#[test]
fn a_given_rod_keeps_its_life_and_a_full_bar_hands_it_back() {
    // The brake gives a pulled rod back to its player with its life (the item condition).
    const ROD: &str = r#"{ "id": "fuel_rod", "name": "Fuel rod", "take": "collect", "hands": 1, "stack": 1, "use": "load_rod", "condition": true, "shape": { "extents": [0.06, 0.6, 0.06], "mass": 3.0, "colour": [0.2, 0.75, 0.3] } }"#;
    let d = ItemDefs::from_files([("fuel_rod.json", ROD), ("wrench.json", WRENCH)]).unwrap();
    let mut host = Host::new(d.clone());
    let rod = item_core::container::Stack::new(d.get("fuel_rod").unwrap().item().with_condition(42.0), 1);
    let ev = host.give(2, rod.clone()).expect("an empty bar takes it");
    match ev {
        Event::Inventory { player, inv } => {
            assert_eq!(player, 2);
            assert_eq!(inv.slot(0).and_then(|s| s.item.condition), Some(42.0));
        }
        other => panic!("the inventory event, got {other:?}"),
    }
    for _ in 1..BAR_SLOTS {
        host.give(2, stack("wrench", 1)).expect("room left");
    }
    assert_eq!(host.give(2, rod.clone()), Err(rod), "a full bar gives the rod back");
}
