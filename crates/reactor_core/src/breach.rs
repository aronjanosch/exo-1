//! The engine room's hull breach (r37). Barotrauma (`Structure.cs:53`, `:1340-1352`): a wall
//! section leaks once its damage is above `LeakThreshold` (0.1), and the gap opens with the
//! damage. Ours is one straight line from the threshold to fully open at damage 1 (Barotrauma
//! goes in two steps, with its big-gap threshold at 0.7). The openness feeds the air's leak
//! (`fire_core::Deck::set_leak`) in the glue. The wrench's hold lowers the damage.
use serde::{Deserialize, Serialize};

/// Barotrauma's LeakThreshold: a wall at or below this damage does not leak.
pub const LEAK_BELOW: f64 = 0.1;

/// The damage of a breach, 0 (sound) .. 1 (wall gone).
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Breach {
    damage: f64,
}

impl Breach {
    pub fn new(damage: f64) -> Breach {
        Breach { damage: damage.clamp(0.0, 1.0) }
    }

    pub fn damage(&self) -> f64 {
        self.damage
    }

    /// Sets the damage (the blast and the dev fault): 0..1, a value outside is clamped.
    pub fn set(&mut self, damage: f64) {
        self.damage = damage.clamp(0.0, 1.0);
    }

    /// A repair takes `amount` of the damage off, never below 0.
    pub fn mend(&mut self, amount: f64) {
        self.damage = (self.damage - amount.max(0.0)).max(0.0);
    }

    /// The leak's openness, 0 (closed) .. 1 (fully open).
    pub fn open(&self) -> f64 {
        if self.damage <= LEAK_BELOW {
            0.0
        } else {
            ((self.damage - LEAK_BELOW) / (1.0 - LEAK_BELOW)).clamp(0.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_dent_does_not_leak() {
        assert_eq!(Breach::new(0.05).open(), 0.0);
        assert_eq!(Breach::new(LEAK_BELOW).open(), 0.0, "at the threshold the wall still holds");
    }

    #[test]
    fn the_leak_opens_with_the_damage_and_is_fully_open_at_one() {
        assert!((Breach::new(0.55).open() - 0.5).abs() < 1e-12, "0.55 is half open");
        assert!((Breach::new(1.0).open() - 1.0).abs() < 1e-12);
        assert!((Breach::new(0.6).open() - 0.5 / 0.9).abs() < 1e-12, "the blast's 0.6 is about 56 % open");
    }

    #[test]
    fn a_repaired_breach_closes_the_leak() {
        let mut b = Breach::new(0.55);
        b.mend(0.3);
        assert!((b.damage() - 0.25).abs() < 1e-12);
        assert!((b.open() - 0.15 / 0.9).abs() < 1e-12, "the leak shrinks as the wall mends");
        b.mend(1.0);
        assert_eq!(b.damage(), 0.0, "mending never goes below sound");
        assert_eq!(b.open(), 0.0, "a repaired breach closes the leak");
    }

    #[test]
    fn set_clamps_to_the_range() {
        let mut b = Breach::default();
        b.set(3.0);
        assert_eq!(b.damage(), 1.0);
        b.set(-1.0);
        assert_eq!(b.damage(), 0.0);
    }
}
