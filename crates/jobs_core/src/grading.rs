//! Grading and payout (#124): base reward × band(delivered share) × condition factor × hazard
//! factor. XP follows the band only (TODO(initiator): starting rule, the playtest decides).
use crate::template::{JobTemplate, ModifierKind};

/// The factors of one finished job.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grade {
    /// Delivered crates / asked crates, 0..1.
    pub share: f64,
    /// Factor of the highest band reached; 0 below the lowest band.
    pub band: f64,
    /// 1 - weight × (1 - mean condition of the delivered crates); 1 with nothing delivered.
    pub condition: f64,
    /// 1 + the bonuses of the warnings.
    pub hazard: f64,
    /// Money for the crew wallet.
    pub money: i64,
    /// XP for each participant.
    pub xp: i64,
}

/// Grades a job that delivered `delivered` of `asked` crates whose conditions add up to
/// `condition_sum`.
pub fn grade(t: &JobTemplate, delivered: u32, asked: u32, condition_sum: f64) -> Grade {
    let share = if asked == 0 { 0.0 } else { delivered as f64 / asked as f64 };
    // A small tolerance so 4 of 5 reaches a band at 0.8.
    let band = t.grading.bands.iter().rev().find(|b| share + 1e-9 >= b.share).map_or(0.0, |b| b.pays);
    let mean = if delivered == 0 { 1.0 } else { (condition_sum / delivered as f64).clamp(0.0, 1.0) };
    let condition = 1.0 - t.grading.condition_weight * (1.0 - mean);
    let hazard = 1.0 + t.modifiers.iter().filter(|m| m.kind == ModifierKind::Warning).map(|m| m.bonus).sum::<f64>();
    Grade {
        share,
        band,
        condition,
        hazard,
        money: (t.reward as f64 * band * condition * hazard).round() as i64,
        xp: (t.xp as f64 * band).round() as i64,
    }
}
