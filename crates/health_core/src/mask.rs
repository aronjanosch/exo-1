//! The breathing mask (r38): worn on the face, the body breathes from its tank instead of the room.
//!
//! Barotrauma's diving mask and oxygen tank (`divingmask` and `oxygentank`, `Items/Diving/divinggear.xml`):
//! - While the wearer needs air, the tank's condition runs down by 0.3 a second
//!   (`divinggear.xml:38`, a `StatusEffect` `Condition` is multiplied by the frame time,
//!   `StatusEffects/StatusEffect.cs:2819`). Barotrauma's condition runs 0..100, so the drain is
//!   0.3 / 100 = 0.003 of a full tank a second: full to empty in 333 s (5.6 min).
//! - The mask gives the body `OxygenAvailable` 1000 with `UseHullOxygen` false (`divinggear.xml:37`);
//!   the body's oxygen follows that and leaves the hull alone (`Characters/Character.cs:3817-3851`,
//!   `UseHullOxygen` at :3842 and :3851).
//! - Refill: the powered shelf `oxygentankshelf2` (`Items/Containers/containers.xml:271`) adds
//!   `Condition` 1.0 a second to a tank it holds, while it has power. That is 1.0 / 100 = 0.01 of a
//!   full tank a second: empty to full in 100 s.
//! - An empty tank: Barotrauma skips the effect at condition 0 (`Items/Components/ItemContainer.cs:717`,
//!   `ExcludeBroken`), so the body falls back to the hull's air. This module follows the ticket
//!   instead: worn and empty breathes nothing, which is worse than the room. TODO(initiator).
//!
//! The rates are starting values taken from those numbers; the initiator plays them. Units: the
//! tank is a fraction (0..1), the rates are fractions a second.

/// The tank's drain while the body breathes from it (fraction of a full tank a second).
pub const DRAIN_PER_S: f64 = 0.3 / 100.0;
/// The shelf's refill of an empty-ish tank (fraction of a full tank a second).
pub const REFILL_PER_S: f64 = 1.0 / 100.0;

/// The mask on a body: whether it is worn, and the tank's fill (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mask {
    pub worn: bool,
    pub tank: f64,
}

impl Mask {
    /// Whether the body breathes the room's air this step. Worn, it breathes from the mask (good
    /// air or none), so it takes nothing from the room, whatever the tank holds.
    pub fn takes_room_air(&self) -> bool {
        !self.worn
    }

    /// One step of breathing: None when the mask is off (the body breathes the room, the caller
    /// decides). Worn, the body breathes the oxygen fraction this returns: 1.0 while the tank has
    /// air (the tank drains by `dt`), 0.0 once it is empty.
    pub fn breathe(&mut self, dt: f64) -> Option<f64> {
        if !self.worn {
            return None;
        }
        if self.tank <= 0.0 {
            return Some(0.0);
        }
        self.tank = (self.tank - DRAIN_PER_S * dt).max(0.0);
        Some(1.0)
    }

    /// The shelf fills the tank by `dt` (at most full).
    pub fn refill(&mut self, dt: f64) {
        self.tank = (self.tank + REFILL_PER_S * dt).min(1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: f64 = 0.1;

    fn full(worn: bool) -> Mask {
        Mask { worn, tank: 1.0 }
    }

    #[test]
    fn worn_and_full_keeps_the_body_in_good_air() {
        let mut m = full(true);
        // The room's air is bad (0.0 here): the mask's air is what the body breathes.
        assert_eq!(m.breathe(TICK), Some(1.0));
        assert!(m.tank < 1.0, "the tank drains while it is used");
        assert!(!m.takes_room_air());
    }

    #[test]
    fn the_tank_drains_to_empty_in_the_derived_time() {
        let mut m = full(true);
        let mut t = 0.0;
        while m.tank > 0.0 && t < 1000.0 {
            m.breathe(TICK);
            t += TICK;
        }
        // 1 / 0.003 = 333.3 s; the step loop gives it to within one tick.
        assert!((t - 1.0 / DRAIN_PER_S).abs() <= TICK + 1e-9, "empty after {t} s");
        assert_eq!(m.tank, 0.0);
    }

    #[test]
    fn worn_and_empty_breathes_nothing() {
        let mut m = Mask { worn: true, tank: 0.0 };
        assert_eq!(m.breathe(TICK), Some(0.0));
        assert_eq!(m.tank, 0.0);
        assert!(!m.takes_room_air(), "worn, it takes no room air either");
    }

    #[test]
    fn not_worn_uses_the_room() {
        let mut m = full(false);
        assert_eq!(m.breathe(TICK), None);
        assert_eq!(m.tank, 1.0, "a mask off the face keeps its tank");
        assert!(m.takes_room_air());
    }

    #[test]
    fn the_shelf_refills_an_empty_tank_in_the_derived_time_and_stops_full() {
        let mut m = Mask { worn: false, tank: 0.0 };
        m.refill(50.0);
        assert!((m.tank - 0.5).abs() < 1e-12);
        m.refill(1000.0);
        assert_eq!(m.tank, 1.0, "never over full");
        assert!((1.0 / REFILL_PER_S - 100.0).abs() < 1e-9);
    }
}
