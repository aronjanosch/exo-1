//! Captain orders (#288): the order wheel, slot and crew orders, expiry, free-text orders.
use crew_core::*;

fn room(s: &str) -> Option<Target> {
    Some(Target::Room(RoomId::new(s)))
}

#[test]
fn only_the_captain_gives_orders_with_fitting_targets() {
    let mut o = Orders::default();
    let to = Addressee::Slot(Role::Engineer);
    assert_eq!(o.give(Role::Pilot, to, OrderKind::GoTo, room("engine_room"), OrderPriority::Normal, 0.0, None), Err(Refusal::NotCaptain));
    // A repair needs a device, a go-to needs a room, follow-me needs none.
    assert_eq!(o.give(Role::Captain, to, OrderKind::Repair, room("engine_room"), OrderPriority::Normal, 0.0, None), Err(Refusal::WrongTarget));
    assert_eq!(o.give(Role::Captain, to, OrderKind::GoTo, None, OrderPriority::Normal, 0.0, None), Err(Refusal::WrongTarget));
    assert_eq!(o.give(Role::Captain, to, OrderKind::FollowMe, room("bridge"), OrderPriority::Normal, 0.0, None), Err(Refusal::WrongTarget));
    // The captain gives no order to the captain's own slot.
    assert_eq!(o.give(Role::Captain, Addressee::Slot(Role::Captain), OrderKind::Stay, None, OrderPriority::Normal, 0.0, None), Err(Refusal::NoSuchSlot));
    assert!(o.for_role(Role::Engineer).is_none());

    let dev = Some(Target::Device(DeviceId::new("oxygen_generator")));
    let ev = o.give(Role::Captain, to, OrderKind::Repair, dev.clone(), OrderPriority::Urgent, 3.0, Some(60.0)).unwrap();
    let OrderEvent::Given { order, replaced } = ev else { panic!("{ev:?}") };
    assert_eq!(replaced, None);
    assert_eq!(order.kind, OrderKind::Repair);
    assert_eq!(order.expires_s, Some(63.0));
    assert_eq!(o.for_role(Role::Engineer).unwrap().target, dev);
}

#[test]
fn a_slot_order_wins_over_the_crew_order() {
    let mut o = Orders::default();
    o.give(Role::Captain, Addressee::Crew, OrderKind::FixBreach, room("cargo_hold"), OrderPriority::Normal, 0.0, None).unwrap();
    o.give(Role::Captain, Addressee::Slot(Role::Tactical), OrderKind::Operate, Some(Target::Station(StationId::new("turret_port"))), OrderPriority::Normal, 1.0, None).unwrap();
    assert_eq!(o.for_role(Role::Tactical).unwrap().kind, OrderKind::Operate);
    assert_eq!(o.for_role(Role::Pilot).unwrap().kind, OrderKind::FixBreach);
    assert_eq!(o.for_role(Role::Captain).unwrap().kind, OrderKind::FixBreach);

    // The HUD line: a text key, the target and a marker id (the order's serial).
    let line = o.line_for(Role::Pilot).unwrap();
    assert_eq!(line.key.as_str(), "order.fix_breach");
    assert_eq!(line.target, room("cargo_hold"));
    assert_eq!(line.marker, o.for_role(Role::Pilot).unwrap().serial);
    assert_eq!(line.text, None);

    // A new crew order replaces the old one and names it, so the glue drops the old marker.
    let old = o.crew().unwrap().serial;
    let OrderEvent::Given { replaced, .. } = o.give(Role::Captain, Addressee::Crew, OrderKind::Stay, None, OrderPriority::Low, 2.0, None).unwrap() else { panic!() };
    assert_eq!(replaced, Some(old));

    assert_eq!(o.clear(Role::Captain, Addressee::Slot(Role::Tactical)).unwrap(), Some(OrderEvent::Cleared { to: Addressee::Slot(Role::Tactical), serial: 1 }));
    assert_eq!(o.for_role(Role::Tactical).unwrap().kind, OrderKind::Stay);
    assert_eq!(o.clear(Role::Captain, Addressee::Slot(Role::Tactical)).unwrap(), None);
}

#[test]
fn orders_expire() {
    let mut o = Orders::default();
    o.give(Role::Captain, Addressee::Crew, OrderKind::Report, None, OrderPriority::Normal, 10.0, Some(20.0)).unwrap();
    o.give(Role::Captain, Addressee::Slot(Role::Pilot), OrderKind::Stay, None, OrderPriority::Normal, 10.0, None).unwrap();
    assert!(o.tick(29.9).is_empty());
    let ev = o.tick(30.0);
    assert_eq!(ev, vec![OrderEvent::Expired { to: Addressee::Crew, serial: 0 }]);
    assert!(o.crew().is_none());
    assert_eq!(o.for_role(Role::Pilot).unwrap().kind, OrderKind::Stay);
}

#[test]
fn free_text_orders_are_a_bounded_list() {
    let mut o = Orders::default();
    assert_eq!(o.set_custom(Role::Engineer, 0, "dance"), Err(Refusal::NotCaptain));
    assert_eq!(o.set_custom(Role::Captain, orders::MAX_CUSTOM, "dance"), Err(Refusal::NoCustomSlot));
    let long = "x".repeat(orders::MAX_CUSTOM_CHARS + 1);
    assert_eq!(o.set_custom(Role::Captain, 0, &long), Err(Refusal::TextTooLong));
    o.set_custom(Role::Captain, 1, "  Everyone to the snack bar  ").unwrap();
    assert_eq!(o.custom()[1].as_deref(), Some("Everyone to the snack bar"));
    assert_eq!(o.custom().len(), orders::MAX_CUSTOM);

    // An empty slot cannot be given; a filled one can, without a target.
    assert_eq!(o.give(Role::Captain, Addressee::Crew, OrderKind::Custom(0), None, OrderPriority::Normal, 0.0, None), Err(Refusal::NoCustomSlot));
    o.give(Role::Captain, Addressee::Crew, OrderKind::Custom(1), None, OrderPriority::Normal, 0.0, None).unwrap();
    let line = o.line_for(Role::Scientist).unwrap();
    assert_eq!(line.key.as_str(), "order.custom");
    assert_eq!(line.text.as_deref(), Some("Everyone to the snack bar"));
    assert_eq!(OrderKind::WHEEL.len(), 8);
}
