//! The breathing mask's item (r38): a use that keeps the item, and the tank (its condition)
//! that the shelf refills and the wearing drains. Breathing itself is in `health_core::mask`.
use item_core::authority::{Action, Event, FrameRef, Host, Placement, Request};
use item_core::container::Stack;
use item_core::defs::{Shape, Take};
use item_core::{ItemDef, ItemDefs};

fn defs() -> ItemDefs {
    let mask = ItemDef {
        id: "o2_mask".into(),
        name: "O2 Mask".into(),
        code: String::new(),
        take: Take::Collect,
        hands: 1,
        stack: 1,
        slots: 0,
        use_effect: Some("wear_mask".into()),
        use_s: None,
        keeps_on_use: true,
        tags: vec!["mask".into()],
        condition: true,
        model: None,
        shape: Shape { extents: [0.2, 0.2, 0.2], mass: 1.0, colour: [0.2, 0.2, 0.9] },
    };
    ItemDefs([(mask.id.clone(), mask)].into_iter().collect())
}

fn at() -> Placement {
    Placement { frame: FrameRef::Ship(1), pos: [0.0; 3], vel: [0.0; 3], forward: [0.0, 0.0, -1.0] }
}

fn held_mask(host: &mut Host) {
    host.spawn(Stack::new(defs().get("o2_mask").unwrap().item(), 1), at(), None);
    let id = *host.loose.keys().next().unwrap();
    host.handle(&Action { player: 1, seq: 1, req: Request::Collect { item: id } });
}

#[test]
fn using_the_mask_keeps_it_in_its_slot() {
    let mut host = Host::new(defs());
    held_mask(&mut host);
    let evs = host.handle(&Action { player: 1, seq: 2, req: Request::Use { slot: 0, target: 1 } });
    assert!(evs.iter().any(|e| matches!(e, Event::Used { effect, .. } if effect == "wear_mask")));
    assert_eq!(host.inventory(1).count("o2_mask"), 1, "a toggle does not use the mask up");
    assert_eq!(host.condition(1, 0), Some(100.0), "a new tank is full");
}

#[test]
fn the_tank_refills_at_most_to_full_and_drains_to_empty() {
    let mut host = Host::new(defs());
    held_mask(&mut host);
    host.wear(1, 0, 60.0);
    host.wear(1, 0, -30.0);
    assert_eq!(host.condition(1, 0), Some(70.0), "a negative wear refills");
    host.wear(1, 0, -500.0);
    assert_eq!(host.condition(1, 0), Some(100.0), "never over full");
    host.wear(1, 0, 500.0);
    assert_eq!(host.condition(1, 0), Some(0.0), "never below empty");
}

#[test]
fn the_mask_file_is_a_valid_item_that_keeps_on_use() {
    let text = include_str!("../../../content/items/o2_mask.json");
    let defs = ItemDefs::from_files([("o2_mask.json", text)]).expect("the content file parses");
    let d = defs.get("o2_mask").expect("the item is there");
    assert!(d.keeps_on_use && d.condition, "the mask keeps its place and wears as its tank");
    assert_eq!(d.use_effect.as_deref(), Some("wear_mask"));
}
