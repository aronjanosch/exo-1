//! Rules for the battery: what it takes from the surplus, what it gives to the shortfall.
use super::*;
use crate::engine_room::{EngineRoom, Input, RoomTuning};
use crate::grid::Group;
use crate::reactor::ReactorTuning;
use crate::tuning::Tuning;

const DT: f64 = 1.0;

fn bat() -> Battery {
    Battery::new(BatteryTuning::default())
}

/// A battery at `frac` of its capacity.
fn at(frac: f64) -> Battery {
    let mut b = bat();
    b.set_charge_kwmin(b.tuning.capacity_kwmin * frac);
    b
}

#[test]
fn battery_charges_from_surplus_at_the_set_speed_times_efficiency() {
    let mut b = bat();
    b.set_recharge(0.5);
    // 0.5 of the 200 kW max: 100 kW taken from a 300 kW surplus for 1 s, 95 % of it kept.
    let taken = b.absorb(300.0, DT);
    assert!((taken - 100.0).abs() < 1e-9, "taken {taken}");
    let want = 100.0 * DT / 60.0 * 0.95;
    assert!((b.charge_kwmin() - want).abs() < 1e-9, "charge {}", b.charge_kwmin());
}

#[test]
fn battery_takes_no_more_than_the_surplus() {
    let mut b = bat();
    b.set_recharge(1.0);
    assert!((b.absorb(50.0, DT) - 50.0).abs() < 1e-9);
}

#[test]
fn battery_recharge_zero_never_charges() {
    let mut b = bat();
    b.set_recharge(0.0);
    assert_eq!(b.absorb(400.0, DT), 0.0);
    assert_eq!(b.charge_kwmin(), 0.0);
}

#[test]
fn battery_recharge_setting_is_clamped_to_zero_one() {
    let mut b = bat();
    b.set_recharge(3.0);
    assert_eq!(b.recharge(), 1.0);
    b.set_recharge(-1.0);
    assert_eq!(b.recharge(), 0.0);
}

#[test]
fn battery_stops_charging_at_capacity() {
    let mut b = at(0.999);
    b.set_recharge(1.0);
    b.absorb(400.0, DT);
    assert!(b.charge_kwmin() <= b.tuning.capacity_kwmin + 1e-9, "charge {}", b.charge_kwmin());
}

#[test]
fn battery_supplies_the_shortfall_for_the_step() {
    let mut b = at(1.0);
    let given = b.supply(150.0, DT);
    assert!((given - 150.0).abs() < 1e-9, "given {given}");
    assert!((b.charge_kwmin() - (800.0 - 150.0 / 60.0)).abs() < 1e-9);
    assert!((b.output_kw() - 150.0).abs() < 1e-9);
}

#[test]
fn battery_output_is_capped_at_its_max_output() {
    let mut b = at(1.0);
    assert!((b.supply(10_000.0, DT) - 400.0).abs() < 1e-9);
}

#[test]
fn battery_is_capped_below_ten_percent_charge() {
    // 5 % charge: 0.5 of the max output (400 kW), i.e. 200 kW.
    let mut b = at(0.05);
    assert!((b.supply(10_000.0, DT) - 200.0).abs() < 1e-9);
}

#[test]
fn battery_output_is_limited_by_the_charge_left() {
    // 40 kW·min over a 60 s step: 40 kW is all there is, then it is empty.
    let mut b = at(0.05);
    b.set_charge_kwmin(40.0);
    assert!((b.supply(10_000.0, 60.0) - 40.0).abs() < 1e-9);
    assert!(b.charge_kwmin().abs() < 1e-9);
}

#[test]
fn battery_empty_gives_nothing() {
    let mut b = bat();
    assert_eq!(b.supply(300.0, DT), 0.0);
    assert_eq!(b.output_kw(), 0.0);
}

#[test]
fn battery_feeds_the_grid_when_the_reactor_is_off() {
    // A cold room with the reactor off: the grid's life support and lights run on the battery.
    let t = Tuning::default();
    let mut room = EngineRoom::from_tuning(&t, 5);
    room.battery_mut().set_charge_kwmin(t.battery.capacity_kwmin);
    let before = room.battery().charge_kwmin();
    for _ in 0..30 {
        room.step(DT, Input::auto());
    }
    assert!(room.battery().output_kw() > 0.0, "the battery gives power");
    assert!(room.grid().supply(Group::LifeSupport).on, "life support is fed");
    assert!(room.battery().charge_kwmin() < before, "the charge goes down");
}

#[test]
fn battery_with_no_charge_leaves_the_grid_dark_when_the_reactor_is_off() {
    let mut room = EngineRoom::new(RoomTuning::default(), ReactorTuning::default(), 5);
    for _ in 0..30 {
        room.step(DT, Input::auto());
    }
    assert_eq!(room.battery().output_kw(), 0.0);
    assert!(!room.grid().supply(Group::LifeSupport).on);
}
