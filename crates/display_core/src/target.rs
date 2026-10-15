//! The medic view: what the player sees of the other player they look at, within 3 m. A
//! compact panel beside the reticle: the name, a health bar, the injuries with the vitals' symbols
//! (BURN II, RAD 40 %, O2, UNCONSCIOUS) and the suitable treatment (`BANDAGE ▶ BURN`), the same
//! words the player's own vitals use. The glue (`exo_app::health_view`) finds the target, the
//! treatments in the action bar and draws what these functions return.
//!
//! The medic view's rules as data (`exo_app::health_view` has the research and the glue).
//! An injury others see is one a treatment can reduce (burn, radiation, oxygen low); the lines
//! read the vitals' words (`▲ BURN II`, `■ RAD 40 %`), and the suggestion names the item and the
//! injury (`BANDAGE ▶ BURN`).
use crate::hud::VitalKind;
use crate::Tone;
use serde::Deserialize;

/// The panel's size and place (`hud.target` in `content/ui/displays.json`). Px from the screen's
/// centre: the panel's left edge sits `offset_x` right of it and its top `lift` above it.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TargetLook {
    pub offset_x: f64,
    pub lift: f64,
    pub width: f64,
    pub pad: f64,
    pub name_size: f64,
    pub line_size: f64,
    pub line_gap: f64,
    pub bar_h: f64,
    pub hint_size: f64,
    /// The treatment hint's outline on the action bar's slot: px and alpha.
    pub outline_px: f64,
    pub outline_alpha: f64,
}

/// An injury the others can see and a treatment can reduce, in the order the panel lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Injury {
    Burn,
    Radiation,
    Oxygen,
}

impl Injury {
    /// The word the hint names it by (`BANDAGE ▶ BURN`).
    pub fn name(self) -> &'static str {
        match self {
            Injury::Burn => "BURN",
            Injury::Radiation => "RAD",
            Injury::Oxygen => "O2",
        }
    }

    /// The vital it stands for (its symbol is the vitals' symbol).
    pub fn vital(self) -> VitalKind {
        match self {
            Injury::Burn => VitalKind::Burn,
            Injury::Radiation => VitalKind::Radiation,
            Injury::Oxygen => VitalKind::Oxygen,
        }
    }
}

/// What the glue knows of the player looked at (from their public status).
#[derive(Clone, Debug, PartialEq)]
pub struct TargetIn {
    pub name: String,
    /// Vitality's share of the maximum before a headache, 0..1.
    pub health: f64,
    pub down: bool,
    pub unconscious: bool,
    pub burning: bool,
    /// The burn's level 1 to 3, 0 without.
    pub burn_level: u8,
    /// Radiation's share, 0 without.
    pub radiation: f64,
    /// Oxygen low's share (bad air), 0 without; the air left is the rest.
    pub oxygen_low: f64,
}

impl TargetIn {
    /// The injuries shown, in the panel's order: burn, radiation, oxygen.
    pub fn injuries(&self) -> Vec<Injury> {
        let mut out = Vec::new();
        if self.burn_level > 0 || self.burning {
            out.push(Injury::Burn);
        }
        if self.radiation > 0.0 {
            out.push(Injury::Radiation);
        }
        if self.oxygen_low > 0.0 {
            out.push(Injury::Oxygen);
        }
        out
    }
}

/// One line of the panel: the label at the left, the value at the right, its colour role.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetLine {
    pub label: String,
    pub value: String,
    pub tone: Tone,
}

/// The panel as drawn: the name, the health bar (0..1) with its text, the lines, and the
/// treatment hint when an item in the action bar suits an injury.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetView {
    pub name: String,
    pub health: f64,
    pub health_text: String,
    pub lines: Vec<TargetLine>,
    pub hint: Option<String>,
}

/// The status glyph of a tone (the same as the vitals': ▲ warn, ■ bad, none when calm).
fn glyph(tone: Tone) -> &'static str {
    match tone {
        Tone::Warn => "▲",
        Tone::Bad => "■",
        _ => "",
    }
}

/// `▲ BURN II`, `▲ BURN` while only building up (never shown to others), `■ BURN III · ON FIRE`.
fn burn_label(level: u8, burning: bool) -> (String, Tone) {
    let tone = if level == 3 || burning { Tone::Bad } else { Tone::Warn };
    let mut s = format!("{} BURN", glyph(tone));
    if level > 0 {
        s.push(' ');
        s.push_str(["I", "II", "III"][level.min(3) as usize - 1]);
    }
    if burning {
        s.push_str(" · ON FIRE");
    }
    (s.trim_start().to_string(), tone)
}

/// The panel's lines for the player looked at: DOWN and UNCONSCIOUS first (they are the state),
/// then the injuries in `TargetIn::injuries` order.
pub fn target_lines(t: &TargetIn) -> Vec<TargetLine> {
    let mut out = Vec::new();
    if t.down {
        out.push(TargetLine { label: "■ DOWN".into(), value: String::new(), tone: Tone::Bad });
    } else if t.unconscious {
        out.push(TargetLine { label: "■ UNCONSCIOUS".into(), value: String::new(), tone: Tone::Bad });
    }
    if t.burn_level > 0 || t.burning {
        let (label, tone) = burn_label(t.burn_level, t.burning);
        out.push(TargetLine { label, value: String::new(), tone });
    }
    if t.radiation > 0.0 {
        let tone = if t.radiation > 0.5 { Tone::Bad } else { Tone::Warn };
        let pct = (t.radiation.clamp(0.0, 1.0) * 100.0).round() as i64;
        out.push(TargetLine { label: format!("{} RAD", glyph(tone)), value: format!("{pct} %"), tone });
    }
    if t.oxygen_low > 0.0 {
        let air = (1.0 - t.oxygen_low.clamp(0.0, 1.0)) * 100.0;
        let tone = if air < 25.0 { Tone::Bad } else { Tone::Warn };
        out.push(TargetLine { label: format!("{} O2", glyph(tone)), value: format!("{} %", air.round() as i64), tone });
    }
    out
}

/// The hint that names the suitable treatment: `BANDAGE ▶ BURN`.
pub fn suit_hint(item: &str, injury: Injury) -> String {
    format!("{} ▶ {}", item.to_uppercase(), injury.name())
}

/// The whole panel for the player looked at. `hint` is the suitable item's name and injury, when
/// one of the player's action-bar slots holds a treatment for one of them.
pub fn target_view(t: &TargetIn, hint: Option<String>) -> TargetView {
    let health = t.health.clamp(0.0, 1.0);
    TargetView {
        name: t.name.to_uppercase(),
        health,
        health_text: format!("{} %", (health * 100.0).round() as i64),
        lines: target_lines(t),
        hint,
    }
}

/// Where the panel's box sits, px from the screen's top left, for a window of `window` px: right
/// of the centre by `offset_x`, its top `lift` above the centre.
pub fn panel_at(window: (f64, f64), look: &TargetLook) -> (f64, f64) {
    (window.0 * 0.5 + look.offset_x, window.1 * 0.5 - look.lift)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn burned() -> TargetIn {
        TargetIn { name: "Pilot 1".into(), health: 0.58, down: false, unconscious: false, burning: false, burn_level: 2, radiation: 0.4, oxygen_low: 0.0 }
    }

    #[test]
    fn the_panel_lists_the_injuries_the_way_the_vitals_do() {
        let t = burned();
        let lines = target_lines(&t);
        assert_eq!(lines[0].label, "▲ BURN II");
        assert_eq!(lines[1].label, "▲ RAD");
        assert_eq!(lines[1].value, "40 %", "the radiation's share in percent, as the medic reads it");
        assert_eq!(t.injuries(), vec![Injury::Burn, Injury::Radiation]);
    }

    #[test]
    fn a_burning_body_is_bad_and_says_so_and_a_downed_one_says_down() {
        let t = TargetIn { burning: true, burn_level: 3, ..burned() };
        assert_eq!(target_lines(&t)[0], TargetLine { label: "■ BURN III · ON FIRE".into(), value: String::new(), tone: Tone::Bad });
        let d = TargetIn { down: true, ..burned() };
        assert_eq!(target_lines(&d)[0].label, "■ DOWN");
        let u = TargetIn { unconscious: true, burn_level: 0, radiation: 0.0, ..burned() };
        assert_eq!(target_lines(&u), vec![TargetLine { label: "■ UNCONSCIOUS".into(), value: String::new(), tone: Tone::Bad }]);
    }

    #[test]
    fn bad_air_shows_the_air_left_and_a_healthy_body_shows_no_lines() {
        let t = TargetIn { oxygen_low: 0.8, radiation: 0.0, burn_level: 0, ..burned() };
        let lines = target_lines(&t);
        assert_eq!((lines[0].label.as_str(), lines[0].value.as_str(), lines[0].tone), ("■ O2", "20 %", Tone::Bad));
        let ok = TargetIn { burn_level: 0, radiation: 0.0, ..burned() };
        assert!(target_lines(&ok).is_empty());
    }

    #[test]
    fn the_view_carries_the_name_the_health_and_the_hint() {
        let v = target_view(&burned(), Some(suit_hint("Bandage", Injury::Burn)));
        assert_eq!(v.name, "PILOT 1");
        assert_eq!((v.health, v.health_text.as_str()), (0.58, "58 %"));
        assert_eq!(v.hint.as_deref(), Some("BANDAGE ▶ BURN"));
        assert_eq!(target_view(&burned(), None).hint, None);
    }

    #[test]
    fn the_panel_sits_beside_the_centre_by_its_offsets() {
        let look = TargetLook { offset_x: 40.0, lift: 120.0, width: 240.0, pad: 10.0, name_size: 16.0, line_size: 15.0, line_gap: 3.0, bar_h: 6.0, hint_size: 15.0, outline_px: 2.0, outline_alpha: 0.9 };
        assert_eq!(panel_at((1600.0, 900.0), &look), (840.0, 330.0));
    }
}
