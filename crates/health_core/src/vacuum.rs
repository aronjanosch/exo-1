//! Vacuum and the suit (r41): a body in vacuum without a suit takes decompression, fast.
//!
//! Barotrauma's pressure rule (`Characters/Character.cs:3545-3572`): a character that needs air and
//! is not protected from the pressure (`IsProtectedFromPressure`, `:5705`), and is outside the hull
//! (`CurrentHull == null`), fills its `PressureTimer` by `100 / PressureKillDelay` a second
//! (`:3562-3563`, a null hull counts as 100; `Health/CharacterHealth.cs:146`: 5 s). At 100 it
//! implodes (`:3566-3571`). Before that, organ damage grows from a tenth of the delay (`:3548-3553`).
//! The protection is the worn suit's `PressureProtection` (`Items/Diving/divinggear.xml:135`,
//! diving suit 4000; the suit's `OxygenAvailable` -100 with `UseHullOxygen` false is its air, as the
//! mask's is in `mask.rs`). It does not depend on the suit's air.
//!
//! Ours: the decompression affliction (`health/health.json`, max 100) takes `RATE_PER_S` a second in
//! vacuum and nothing suited. Its band takes the vitality from 0 at strength 0 to 2 maximum at 100,
//! so the body is unconscious at half (5 s) and dead at 100 (10 s). The rate is ours: Barotrauma's
//! 5 s doubled to the 10 s the initiator wants (TODO(initiator): confirm 10 s in play).
//!
//! Vacuum is a room whose oxygen fraction is below `VACUUM_BELOW` (fire_core's `Room::pressure()` is
//! the same fraction, `fire_core/src/lib.rs:123`). Bad air above that is the slow suffocation of
//! `oxygen_low`, not this. The units: the strength is points of the affliction (0..100).

/// Strength the decompression takes a second in vacuum, unsuited (100 points in 10 s).
pub const RATE_PER_S: f64 = 10.0;
/// Strength healed a second out of vacuum (TODO(initiator): 0.5, 60 s from full). Not in the band:
/// the band's own tick would take it inside the vacuum too.
pub const HEAL_PER_S: f64 = 0.5;
/// Below this oxygen fraction (= pressure, fire_core) a room is vacuum (TODO(initiator): 5 %).
pub const VACUUM_BELOW: f64 = 0.05;

/// The decompression a body takes in `dt` seconds: `RATE_PER_S` x dt in vacuum without a suit,
/// nothing otherwise. The air the body breathes does not enter here.
pub fn vacuum_damage(dt: f64, in_vacuum: bool, suited: bool) -> f64 {
    if in_vacuum && !suited { RATE_PER_S * dt.max(0.0) } else { 0.0 }
}

/// The change of the decompression in `dt` seconds: the damage in vacuum, else the heal.
pub fn decompression_change(dt: f64, in_vacuum: bool, suited: bool) -> f64 {
    match vacuum_damage(dt, in_vacuum, suited) {
        d if d > 0.0 => d,
        _ => -HEAL_PER_S * dt.max(0.0),
    }
}

/// Whether a room with this pressure (oxygen fraction, 0..1) is vacuum.
pub fn in_vacuum(pressure: f64) -> bool {
    pressure < VACUUM_BELOW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsuited_in_vacuum_takes_the_rate_and_nothing_else_does() {
        assert_eq!(vacuum_damage(1.0, true, false), RATE_PER_S);
        assert_eq!(vacuum_damage(0.5, true, false), RATE_PER_S * 0.5);
        assert_eq!(vacuum_damage(1.0, true, true), 0.0, "suited");
        assert_eq!(vacuum_damage(1.0, false, false), 0.0, "not in vacuum");
        assert_eq!(vacuum_damage(1.0, false, true), 0.0);
        assert_eq!(vacuum_damage(0.0, true, false), 0.0);
    }

    #[test]
    fn only_near_zero_pressure_is_vacuum() {
        assert!(in_vacuum(0.0));
        assert!(in_vacuum(0.04));
        assert!(!in_vacuum(0.05), "5 % is not vacuum");
        assert!(!in_vacuum(1.0));
    }
}
