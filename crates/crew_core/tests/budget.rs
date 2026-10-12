//! Crew budget (#288): the captain releases money per role in steps of 100.
use crew_core::*;
use gameplay_core::{TrackId, WorldEvent};

fn spent(amount: i64) -> WorldEvent {
    WorldEvent::TrackChanged { track: TrackId::new(gameplay_core::content::WALLET), delta: -amount, player: None }
}

#[test]
fn the_captain_releases_in_steps_of_100() {
    let mut b = Budget::default();
    let wallet = 1000;
    assert_eq!(b.release(Role::Engineer, Role::Engineer, wallet), Err(Refusal::NotCaptain));
    assert_eq!(b.release(Role::Captain, Role::Captain, wallet), Err(Refusal::NoSuchSlot));
    for _ in 0..3 {
        b.release(Role::Captain, Role::Engineer, wallet).unwrap();
    }
    assert_eq!(b.release(Role::Captain, Role::Tactical, wallet), Ok(100));
    b.release(Role::Captain, Role::Tactical, wallet).unwrap();
    assert_eq!(b.remaining(Role::Engineer), 300);
    assert_eq!(b.remaining(Role::Pilot), 0);
    // White: everything the crew has; blue: what the captain has not released yet.
    assert_eq!(b.pool(wallet), budget::Pool { total: 1000, available: 500 });

    // Not enough unreleased money for another step.
    assert_eq!(b.pool(550).available, 50);
    assert_eq!(b.release(Role::Captain, Role::Pilot, 550), Err(Refusal::NotEnoughAvailable { available: 50 }));
    // Money spent elsewhere can drop the wallet under the released sum: available stays at 0.
    assert_eq!(b.pool(300), budget::Pool { total: 300, available: 0 });

    assert_eq!(b.withdraw(Role::Captain, Role::Engineer), Ok(200));
    assert_eq!(b.withdraw(Role::Pilot, Role::Engineer), Err(Refusal::NotCaptain));
}

#[test]
fn spending_stays_inside_the_allowance() {
    let mut b = Budget::default();
    let mut wallet = 1000;
    for _ in 0..3 {
        b.release(Role::Captain, Role::Engineer, wallet).unwrap();
    }
    assert_eq!(b.try_spend(Role::Engineer, 0, wallet), Err(Refusal::BadAmount));
    assert_eq!(b.try_spend(Role::Engineer, 250, wallet), Ok(spent(250)));
    wallet -= 250;
    assert_eq!(b.remaining(Role::Engineer), 50);
    assert_eq!(b.try_spend(Role::Engineer, 100, wallet), Err(Refusal::OverAllowance { remaining: 50, asked: 100 }));
    assert_eq!(b.try_spend(Role::Pilot, 10, wallet), Err(Refusal::OverAllowance { remaining: 0, asked: 10 }));
    assert_eq!(b.pool(wallet), budget::Pool { total: 750, available: 700 });

    // The captain spends from the unreleased money.
    assert_eq!(b.try_spend(Role::Captain, 701, wallet), Err(Refusal::NotEnoughAvailable { available: 700 }));
    assert_eq!(b.try_spend(Role::Captain, 700, wallet), Ok(spent(700)));
    wallet -= 700;
    // The crew's money runs out before the allowance does.
    assert_eq!(b.try_spend(Role::Engineer, 50, 20), Err(Refusal::NotEnoughCredits { wallet: 20, asked: 50 }));
    assert_eq!(b.try_spend(Role::Engineer, 50, wallet), Ok(spent(50)));
}

#[test]
fn without_limits_everyone_spends_from_the_pool() {
    let mut b = Budget::default();
    assert!(b.limits());
    assert_eq!(b.set_limits(Role::Pilot, false), Err(Refusal::NotCaptain));
    b.set_limits(Role::Captain, false).unwrap();
    b.release(Role::Captain, Role::Engineer, 500).unwrap();
    assert_eq!(b.try_spend(Role::Pilot, 400, 500), Ok(spent(400)));
    assert_eq!(b.try_spend(Role::Pilot, 401, 500), Err(Refusal::NotEnoughAvailable { available: 400 }));
    // An allowance still counts first for its role.
    assert_eq!(b.try_spend(Role::Engineer, 100, 500), Ok(spent(100)));
    assert_eq!(b.remaining(Role::Engineer), 0);
}
