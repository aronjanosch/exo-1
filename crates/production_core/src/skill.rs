//! How a worker's skill (0..100) shapes a lab job (#287): a speed factor on every device time and
//! bonus quality tiers on the finished batch. Seeded dice keep the rolls repeatable.
//!
//! The shape follows a crafting machine of another crew game: the margin of skill over the
//! requirement maps to a speed that is 1 at the requirement, doubles far above it and drops
//! quadratically below it; two bonus tiers unlock at fixed skills, with a chance that grows from a
//! target between the requirement and a cap. Our curve has no jump at the requirement.
use serde::{Deserialize, Serialize};

use crate::devices::SkillTuning;

/// The first bonus tier's target: this share of the way from the requirement to 100.
const PLUS_ONE_LERP: f64 = 0.2;
/// The second tier's target: this share of the way from the requirement to 125.
const PLUS_TWO_LERP: f64 = 0.4;
const PLUS_TWO_CAP: f64 = 125.0;

/// The speed factor on a device's time (a duration divides by it). 1 at the requirement, up to
/// `fastest` (100 points above), falling to `slowest` (100 points below).
pub fn speed(skill: f64, required: f64, s: &SkillTuning) -> f64 {
    let d = ((skill - required + 100.0) / 200.0).clamp(0.0, 1.0);
    let t = if d < 0.5 { (2.0 * d).powi(2) } else { 2.0 * d };
    t.clamp(s.slowest, s.fastest)
}

/// The chances (0..1) of the first bonus tier and of the second, given the first.
pub fn bonus_chances(skill: f64, required: f64, s: &SkillTuning) -> (f64, f64) {
    let chance = |from: f64, target: f64| {
        if skill < from || target >= 100.0 {
            0.0
        } else {
            ((skill - target) / (100.0 - target)).clamp(0.0, 1.0)
        }
    };
    let t1 = required + (100.0 - required) * PLUS_ONE_LERP;
    let t2 = required + (PLUS_TWO_CAP - required) * PLUS_TWO_LERP;
    (chance(s.plus_one_from, t1), chance(s.plus_two_from, t2))
}

/// Rolls the bonus tiers (0, 1 or 2): the second is rolled only after the first succeeded.
pub fn bonus_tiers(skill: f64, required: f64, s: &SkillTuning, dice: &mut Dice) -> u8 {
    let (p1, p2) = bonus_chances(skill, required, s);
    if !dice.chance(p1) {
        0
    } else if dice.chance(p2) {
        2
    } else {
        1
    }
}

/// The skill gained by one finished job: `base` divided by the current skill to the power 1.5
/// (skill below 1 counts as 1), so early gains are fast and late ones slow. Never past 100.
pub fn gain(skill: f64, base: f64) -> f64 {
    (base / skill.max(1.0).powf(1.5)).min((100.0 - skill).max(0.0))
}

/// A small seeded random source (splitmix64), saved with its state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dice {
    state: u64,
}

impl Dice {
    pub fn new(seed: u64) -> Dice {
        Dice { state: seed }
    }

    /// A number in 0..1.
    pub fn roll(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }

    /// True with probability `p` (0 never, 1 always).
    pub fn chance(&mut self, p: f64) -> bool {
        self.roll() < p
    }
}
