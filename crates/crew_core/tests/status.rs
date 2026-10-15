//! Ship status snapshot and the placeholder generator for the dev area (#288).
use crew_core::status::{Forced, ReactorMode, ShipSystem, TempBand};
use crew_core::*;

fn run(seed: u64, steps: usize) -> ShipStatus {
    let mut p = PlaceholderStatus::new(seed);
    for _ in 0..steps {
        p.step(0.25);
    }
    p.status()
}

#[test]
fn the_same_seed_gives_the_same_ship() {
    assert_eq!(run(42, 400), run(42, 400));
    assert_ne!(run(42, 400), run(43, 400));
    // The values move over time.
    assert_ne!(run(42, 0).shields, run(42, 40).shields);
}

#[test]
fn placeholder_values_stay_in_their_bands() {
    let mut p = PlaceholderStatus::new(7);
    for _ in 0..2400 {
        p.step(0.5);
        let s = p.status();
        assert_eq!(s.systems.len(), 4);
        for (bar, sys) in s.systems.iter().zip(ShipSystem::ALL) {
            assert_eq!(bar.system, sys);
            let c = bar.condition.unwrap();
            assert!((14..=20).contains(&c), "{c}");
            assert!(!bar.fire);
        }
        assert!((70.0..=90.0).contains(&s.shields.value) && s.shields.max == 100.0);
        assert!((89.0..=95.0).contains(&s.hull.value) && s.hull.max == 100.0);
        assert!((0.94..=0.98).contains(&s.oxygen.level));
        assert!(s.oxygen.delta_per_s.abs() < 0.01);
        assert!((0.35..=0.55).contains(&s.reactor.temp));
        assert_eq!(s.reactor.band, TempBand::Normal);
        assert_eq!(s.reactor.mode, ReactorMode::Running);
        assert_eq!(s.rooms.len(), 6);
        assert!(s.rooms.iter().all(|r| !r.fire && !r.breach && (0.8..=1.0).contains(&r.condition)));
    }
    // The system clock: an hour per star system, counting down from the start.
    assert_eq!(p.status().clock_left_s, Some(3600.0 - 1200.0));
}

#[test]
fn forced_states_show_and_go_again() {
    let mut p = PlaceholderStatus::new(1);
    p.step(10.0);
    let calm = p.status();
    let engine = RoomId::new("engine_room");
    let hold = RoomId::new("cargo_hold");

    p.force(Forced::ReactorHot);
    p.force(Forced::Fire(engine.clone()));
    p.force(Forced::Breach(hold.clone()));
    p.force(Forced::SystemDown(ShipSystem::Weapons));
    let s = p.status();
    assert_eq!(s.reactor.temp, 0.95);
    assert_eq!(s.reactor.band, TempBand::Critical);
    assert_eq!(s.reactor.mode, ReactorMode::Overheating);
    let room = s.room(&engine).unwrap();
    assert!(room.fire && !room.breach);
    assert_eq!(room.oxygen, 0.6);
    // A fire in the engine room shows on the engineering bar and costs it 6 points.
    let eng = s.system(ShipSystem::Engineering);
    assert!(eng.fire);
    assert_eq!(eng.condition, Some(calm.system(ShipSystem::Engineering).condition.unwrap() - 6));
    assert_eq!(s.system(ShipSystem::Weapons).condition, Some(0));
    let hold_room = s.room(&hold).unwrap();
    assert!(hold_room.breach);
    assert_eq!(hold_room.oxygen, 0.3);
    assert_eq!(s.hull.value, calm.hull.value - 15.0);
    // Fire and breach both pull the oxygen down.
    assert!((s.oxygen.delta_per_s - (calm.oxygen.delta_per_s - 0.03)).abs() < 1e-12);

    // Forcing twice is one force; releasing one leaves the others.
    p.force(Forced::ReactorHot);
    assert_eq!(p.forced().len(), 4);
    p.release(&Forced::ReactorHot);
    assert_eq!(p.status().reactor.band, TempBand::Normal);
    assert!(p.status().room(&engine).unwrap().fire);
    p.clear();
    assert_eq!(p.status(), calm);
}

#[test]
fn forced_states_lay_over_any_status() {
    let mut p = PlaceholderStatus::new(5);
    p.step(3.0);
    let mut real = p.status_unforced();
    // The glue's real data: the engine room burns for real, the reactor reports its output.
    real.rooms[1].oxygen = 0.5;
    real.reactor.output_kw = Some(800.0);
    p.force(Forced::Breach(RoomId::new("engine_room")));
    p.force(Forced::SystemDown(ShipSystem::Science));
    let mut s = real.clone();
    p.apply_forced(&mut s);
    assert!(s.rooms[1].breach);
    assert_eq!(s.rooms[1].oxygen, 0.3);
    assert_eq!(s.hull.value, real.hull.value - 15.0);
    assert_eq!(s.system(ShipSystem::Science).condition, Some(0));
    assert_eq!(s.reactor.output_kw, Some(800.0));
    // The plain status is the unforced one with the forced states on top.
    let mut again = p.status_unforced();
    p.apply_forced(&mut again);
    assert_eq!(again, p.status());
    assert_eq!(PlaceholderStatus::new(5).status().reactor.load_kw, None);
}
