//! Text keys used by the glue layer (exo_app) that must be in the text table (#131).
//! TODO(initiator): text key placeholders.
use gameplay_core::text::TextTable;

/// Keys the glue layer (`exo_app`) uses that must exist in the text table; keep in step with
/// `grep -rhoE '"(notice|track)\.[a-z_.]+"' crates/exo_app/src`.
pub const GLUE_KEYS: &[&str] = &[
    "track.wallet.name",
    "track.freight_xp.name",
    "notice.job.accepted",
    "notice.job.picked_up",
    "notice.job.delivered",
    "notice.job.completed",
    "notice.job.abandon_ask",
    "notice.exam.honours",
    "notice.licence.needed",
    "notice.order.placed",
    "notice.reward.base",
    "notice.reward.standing",
    "notice.reward.xp",
    "notice.refused.too_many",
    "notice.refused.not_available",
    "notice.refused.cannot_afford",
];

/// Checks that all glue keys exist in the text table.
pub fn check_glue_keys(table: &TextTable) -> Vec<String> {
    let mut errors = Vec::new();
    for key in GLUE_KEYS {
        if !table.has(key) {
            errors.push(format!("glue: no text '{key}'"));
        }
    }
    errors
}
