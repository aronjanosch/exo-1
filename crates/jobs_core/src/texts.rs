//! Text keys used by the glue layer (exo_app) that must be in the text table (#131).
//! TODO(initiator): text key placeholders.
use gameplay_core::text::TextTable;

/// Keys the glue layer uses that must exist in the text table.
pub const GLUE_KEYS: &[&str] = &[
    "track.wallet.name",
    "track.freight_xp.name",
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
