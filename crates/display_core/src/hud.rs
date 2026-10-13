//! The crew HUD's rules without engine types (Crew HUD v3, Claude Design project "EXO-1 Ship
//! Displays"; decision "Minimal HUD" in the concept repo). Six elements: compass, vitals, the
//! interaction prompt, the action bar, the key hints and the reticle, plus the credits that show
//! for a moment after a change. This file holds what can be tested without a window: the sizes
//! (`HudLook`, the `hud` section of `content/ui/displays.json`), the compass strip, the vitals
//! lines, the credits flash, the bar's idle fade, the bar's layout, the key hints and the prompt's
//! anchor. The glue (`exo_app::crew_hud`) draws what these return.
use crate::Tone;
use glam::{DVec2, DVec3};
use serde::Deserialize;

// ---------- the look (data) ----------

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HudLook {
    /// Space between the screen edge and the HUD (px).
    pub margin: f64,
    pub compass: CompassLook,
    pub vitals: VitalsLook,
    pub prompt: PromptLook,
    pub bar: BarLook,
    pub hints: HintsLook,
    pub reticle: ReticleLook,
    pub credits: CreditsLook,
    pub text: TextLook,
}

/// The heading strip at the top centre.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CompassLook {
    /// Strip width (px) and the degrees it shows.
    pub width: f64,
    pub span_deg: f64,
    /// A small tick every this many degrees, a label every `label_every_deg` (a multiple).
    pub tick_every_deg: f64,
    pub label_every_deg: f64,
    pub tick_px: f64,
    pub major_tick_px: f64,
    pub label_size: f64,
    /// The current heading as a number.
    pub heading_size: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VitalsLook {
    /// Width of every bar and the thickness of the health bar (px).
    pub bar_w: f64,
    pub bar_h: f64,
    /// Thickness of the oxygen bar and the burn line.
    pub thin_h: f64,
    pub gap: f64,
    /// Below this share health and oxygen read as bad (status colour with glyph in the glue).
    pub low: f64,
    pub label_size: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PromptLook {
    /// The prompt's bottom edge sits this far above its target (px).
    pub gap: f64,
    pub key_px: f64,
    pub text_size: f64,
    /// Dashed outlines (blocked key, carry frames): dash and gap length (px).
    pub dash: f64,
    pub dash_gap: f64,
    /// The amber line under a blocked prompt.
    pub line_px: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BarLook {
    /// Closed action bar: slot edge, gap, how far the held slot lifts, distance of the slots from
    /// the screen's bottom edge (leaves room for the held item's name).
    pub slot: f64,
    pub gap: f64,
    pub lift: f64,
    pub bottom: f64,
    /// Open (Tab): slot edge and gap.
    pub open_slot: f64,
    pub open_gap: f64,
    /// Seconds without activity until everything but the held slot dims, how long the dimming
    /// takes, and the contrast it dims to (0..1).
    pub idle_s: f64,
    pub idle_fade_s: f64,
    pub idle_contrast: f64,
    /// Contrast of the slots while the hands carry cargo (dashed frames).
    pub carry_contrast: f64,
    pub outline_px: f64,
    pub count_size: f64,
    pub name_size: f64,
    /// Edge of the item's colour swatch in a closed slot (px).
    pub swatch: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HintsLook {
    pub key_px: f64,
    pub size: f64,
    pub gap: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReticleLook {
    pub dot_px: f64,
    pub ring_px: f64,
    pub ring_stroke_px: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreditsLook {
    /// Seconds shown after a change, then the fade.
    pub show_s: f64,
    pub fade_s: f64,
    pub size: f64,
    pub delta_size: f64,
}

/// The dark shadow behind every HUD text, for legibility on bright ground.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TextLook {
    pub shadow_px: f64,
    pub shadow_alpha: f64,
}

impl HudLook {
    /// Every size finite and not negative, the compass grid consistent.
    pub fn check(&self) -> Result<(), String> {
        let (c, v, p, b, h, r, cr, t) = (&self.compass, &self.vitals, &self.prompt, &self.bar, &self.hints, &self.reticle, &self.credits, &self.text);
        let all = [
            self.margin, c.width, c.span_deg, c.tick_every_deg, c.label_every_deg, c.tick_px, c.major_tick_px, c.label_size, c.heading_size, v.bar_w, v.bar_h, v.thin_h, v.gap, v.low, v.label_size, p.gap, p.key_px,
            p.text_size, p.dash, p.dash_gap, p.line_px, b.slot, b.gap, b.lift, b.bottom, b.open_slot, b.open_gap, b.idle_s, b.idle_fade_s, b.idle_contrast, b.carry_contrast, b.outline_px, b.count_size,
            b.name_size, b.swatch, h.key_px, h.size, h.gap, r.dot_px, r.ring_px, r.ring_stroke_px, cr.show_s, cr.fade_s, cr.size, cr.delta_size, t.shadow_px, t.shadow_alpha,
        ];
        if all.iter().any(|x| !(*x >= 0.0 && x.is_finite())) {
            return Err("displays.json: hud sizes must be finite and not negative".into());
        }
        if c.tick_every_deg <= 0.0 || c.span_deg <= 0.0 || c.span_deg > 360.0 {
            return Err("displays.json: hud.compass needs tick_every_deg > 0 and 0 < span_deg <= 360".into());
        }
        let k = c.label_every_deg / c.tick_every_deg;
        if c.label_every_deg <= 0.0 || (k - k.round()).abs() > 1e-9 {
            return Err("displays.json: hud.compass.label_every_deg must be a multiple of tick_every_deg".into());
        }
        if b.idle_contrast > 1.0 || b.carry_contrast > 1.0 || v.low > 1.0 || t.shadow_alpha > 1.0 {
            return Err("displays.json: hud shares (contrast, low, shadow_alpha) are at most 1".into());
        }
        Ok(())
    }
}

// ---------- compass ----------

/// What the compass measures against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference {
    /// The planet's north (on a planet, also in a landed ship).
    True,
    /// The ship's bow, 0 degrees forward (a flying ship, and space).
    Bow,
}

/// Which frame the compass uses: on a planet (the walker outside near one, or anyone in or on a
/// landed ship) the true heading, in a flying ship and in space the heading from the bow.
pub fn reference(inside_ship: bool, ship_landed: bool, near_planet: bool) -> Reference {
    let on_ground = if inside_ship { ship_landed } else { near_planet };
    if on_ground { Reference::True } else { Reference::Bow }
}

/// Degrees into 0..360.
pub fn normalize_deg(d: f64) -> f64 {
    let m = d.rem_euclid(360.0);
    if m >= 360.0 { 0.0 } else { m }
}

/// The heading of `look` against `forward` on the plane around `up`, degrees clockwise (seen from
/// above) in 0..360: 0 where `forward` points, 90 to its right. `forward` is the planet's north
/// or the ship's bow. `None` when the look points straight along `up` (no heading).
pub fn heading_deg(look: DVec3, up: DVec3, forward: DVec3) -> Option<f64> {
    let up = up.try_normalize()?;
    let flat = |v: DVec3| (v - up * v.dot(up)).try_normalize();
    let (l, f) = (flat(look)?, flat(forward)?);
    let right = f.cross(up);
    Some(normalize_deg(l.dot(right).atan2(l.dot(f)).to_degrees()))
}

/// North on the planet's surface at `up` (the spin axis is +y): the direction along the ground
/// towards the pole. At a pole there is none; +x stands in.
pub fn north_at(up: DVec3) -> DVec3 {
    let axis = DVec3::Y;
    (axis - up * axis.dot(up)).try_normalize().unwrap_or(DVec3::X)
}

/// The number shown for a heading: `042°`.
pub fn heading_text(h: f64) -> String {
    format!("{:03}°", (normalize_deg(h).round() as i64) % 360)
}

/// One mark of the strip.
#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    /// Degrees right (+) or left (-) of the current heading.
    pub offset_deg: f64,
    /// The heading this mark stands for, 0..360.
    pub deg: u32,
    /// Position across the strip, -0.5 (left edge) to 0.5 (right edge).
    pub x: f64,
    /// The label (N E S W on a true heading, degrees otherwise), bold for the four main ones.
    pub label: Option<String>,
    pub bold: bool,
    /// Fades towards the edges, 0..1.
    pub alpha: f64,
}

/// The marks of the strip around `heading`.
pub fn compass_ticks(heading: f64, reference: Reference, c: &CompassLook) -> Vec<Tick> {
    let half = c.span_deg * 0.5;
    let first = ((heading - half) / c.tick_every_deg).ceil() as i64;
    let last = ((heading + half) / c.tick_every_deg).floor() as i64;
    let label_k = (c.label_every_deg / c.tick_every_deg).round() as i64;
    (first..=last)
        .map(|k| {
            let at = k as f64 * c.tick_every_deg;
            let offset = at - heading;
            let deg = normalize_deg(at);
            let labelled = k.rem_euclid(label_k) == 0;
            let main = (deg % 90.0).abs() < 1e-9;
            let label = labelled.then(|| match (reference, deg as u32) {
                (Reference::True, 0) => "N".to_string(),
                (Reference::True, 90) => "E".to_string(),
                (Reference::True, 180) => "S".to_string(),
                (Reference::True, 270) => "W".to_string(),
                (_, d) => d.to_string(),
            });
            let edge = (offset.abs() / half).clamp(0.0, 1.0);
            Tick { offset_deg: offset, deg: deg as u32, x: offset / c.span_deg, label, bold: labelled && main, alpha: 1.0 - edge * edge }
        })
        .collect()
}

// ---------- vitals ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VitalKind {
    Health,
    Oxygen,
    Burn,
}

/// One line of the vitals block: a bar filled to `frac` (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VitalLine {
    pub kind: VitalKind,
    pub frac: f64,
    /// Normal; Bad when low (health, oxygen); Warn for the burn line.
    pub tone: Tone,
}

/// The lines to show now: health always, oxygen only while it is set (it drops), the burn line
/// only while burning.
pub fn vital_lines(health: f64, oxygen: Option<f64>, burn: Option<f64>, v: &VitalsLook) -> Vec<VitalLine> {
    let share = |x: f64| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
    let low = |x: f64| if x < v.low { Tone::Bad } else { Tone::Normal };
    let mut out = vec![VitalLine { kind: VitalKind::Health, frac: share(health), tone: low(share(health)) }];
    if let Some(o) = oxygen {
        out.push(VitalLine { kind: VitalKind::Oxygen, frac: share(o), tone: low(share(o)) });
    }
    if let Some(b) = burn {
        out.push(VitalLine { kind: VitalKind::Burn, frac: share(b), tone: Tone::Warn });
    }
    out
}

// ---------- credits ----------

/// What the credits element shows right now.
#[derive(Clone, Debug, PartialEq)]
pub struct CreditsView {
    pub value: i64,
    /// The change since the element appeared (adds up while it stays).
    pub delta: i64,
    pub alpha: f64,
}

/// The credits show only for a moment after a change: the value and the delta, then a fade. The
/// first value is only recorded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CreditsFlash {
    last: Option<i64>,
    delta: i64,
    left: f64,
}

impl CreditsFlash {
    /// One step of `dt` seconds with the current wallet.
    pub fn step(&mut self, wallet: i64, dt: f64, c: &CreditsLook) -> Option<CreditsView> {
        match self.last {
            None => {}
            Some(prev) if prev != wallet => {
                if self.left <= 0.0 {
                    self.delta = 0;
                }
                self.delta += wallet - prev;
                self.left = c.show_s + c.fade_s;
            }
            Some(_) => self.left -= dt,
        }
        self.last = Some(wallet);
        if self.left <= 0.0 {
            self.delta = 0;
            return None;
        }
        let alpha = if c.fade_s > 0.0 && self.left < c.fade_s { self.left / c.fade_s } else { 1.0 };
        Some(CreditsView { value: wallet, delta: self.delta, alpha })
    }
}

/// `1 250 cr`: thousands separated by a thin gap, the unit after.
pub fn credits_text(v: i64) -> String {
    let digits = v.unsigned_abs().to_string();
    let mut s = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            s.push(' ');
        }
        s.push(ch);
    }
    format!("{}{s} cr", if v < 0 { "-" } else { "" })
}

/// `+300` or `-40`.
pub fn delta_text(d: i64) -> String {
    format!("{}{}", if d < 0 { "-" } else { "+" }, d.unsigned_abs())
}

// ---------- the action bar ----------

/// The bar's contrast clock: full while something happens, `idle_contrast` after `idle_s` without
/// it (the held slot stays at full contrast in the glue).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarIdle {
    idle: f64,
}

impl BarIdle {
    /// Something happened (a slot taken, a pickup, a drop, Tab).
    pub fn touch(&mut self) {
        self.idle = 0.0;
    }
    pub fn step(&mut self, dt: f64) {
        self.idle += dt;
    }
    /// 1 down to `idle_contrast`, over `idle_fade_s` after `idle_s`.
    pub fn contrast(&self, b: &BarLook) -> f64 {
        let k = if b.idle_fade_s > 0.0 { ((self.idle - b.idle_s) / b.idle_fade_s).clamp(0.0, 1.0) } else { (self.idle >= b.idle_s) as u8 as f64 };
        1.0 - k * (1.0 - b.idle_contrast)
    }
}

/// Slot `i` of `n` on the screen: top left and edge (px), at rest (the held slot's lift is the
/// glue's). Closed it sits at the bottom with room for the held name; open (Tab) it is larger, in
/// the middle.
pub fn bar_slot(i: usize, n: usize, open: bool, window: DVec2, b: &BarLook) -> (DVec2, f64) {
    let (size, gap) = if open { (b.open_slot, b.open_gap) } else { (b.slot, b.gap) };
    let n = n as f64;
    let width = n * size + (n - 1.0) * gap;
    let x = (window.x - width) * 0.5 + i as f64 * (size + gap);
    let y = if open { (window.y - size) * 0.5 } else { window.y - size - b.bottom };
    (DVec2::new(x, y), size)
}

// ---------- key hints ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintAction {
    /// The left mouse button uses the held item.
    Use,
    Drop,
    Throw,
}

/// One key hint; `dim` when the action exists but the item does nothing yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hint {
    pub action: HintAction,
    pub dim: bool,
}

/// The hints bottom right: for the held item use, drop and throw; while the hands carry cargo only
/// drop and throw; with nothing in the hands none.
pub fn hints(held_item: bool, item_usable: bool, carrying: bool) -> Vec<Hint> {
    if carrying {
        return vec![Hint { action: HintAction::Drop, dim: false }, Hint { action: HintAction::Throw, dim: false }];
    }
    if !held_item {
        return Vec::new();
    }
    vec![Hint { action: HintAction::Use, dim: !item_usable }, Hint { action: HintAction::Drop, dim: false }, Hint { action: HintAction::Throw, dim: false }]
}

// ---------- the prompt ----------

/// What F does at the target; the glue draws each its own way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptKind {
    #[default]
    Use,
    Collect,
    /// Stand up, leave a screen.
    Leave,
    /// F does nothing now (hands full, no licence): dashed key, amber line.
    Blocked,
}

/// Where the prompt's bottom centre goes: `gap` above the target's screen point, kept inside the
/// margins; without a point on screen (no world target, behind the camera) below the reticle.
pub fn prompt_anchor(target: Option<DVec2>, window: DVec2, look: &HudLook) -> DVec2 {
    let m = look.margin;
    match target {
        Some(p) if p.x.is_finite() && p.y.is_finite() => DVec2::new(p.x.clamp(m, (window.x - m).max(m)), (p.y - look.prompt.gap).clamp(m, (window.y - m).max(m))),
        _ => DVec2::new(window.x * 0.5, window.y * 0.58),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> HudLook {
        crate::Theme::from_json(include_str!("../../../content/ui/displays.json")).unwrap().hud
    }

    #[test]
    fn the_shipped_hud_look_is_consistent() {
        look().check().unwrap();
    }

    #[test]
    fn a_bad_compass_grid_is_refused() {
        let mut l = look();
        l.compass.label_every_deg = 32.0;
        assert!(l.check().is_err());
        let mut l = look();
        l.bar.idle_contrast = 1.5;
        assert!(l.check().is_err());
    }

    #[test]
    fn the_true_frame_on_a_planet_and_the_bow_in_flight() {
        // Walking outside near a planet, and in a landed ship: true. Flying and in space: bow.
        assert_eq!(reference(false, false, true), Reference::True);
        assert_eq!(reference(true, true, true), Reference::True);
        assert_eq!(reference(true, false, true), Reference::Bow);
        assert_eq!(reference(true, false, false), Reference::Bow);
        assert_eq!(reference(false, false, false), Reference::Bow);
    }

    #[test]
    fn heading_runs_clockwise_from_north_and_ignores_pitch() {
        let (up, north) = (DVec3::Y, DVec3::NEG_Z);
        let h = |look: DVec3| heading_deg(look, up, north).unwrap();
        assert!(h(DVec3::NEG_Z).abs() < 1e-9);
        assert!((h(DVec3::X) - 90.0).abs() < 1e-9, "east is 90");
        assert!((h(DVec3::Z) - 180.0).abs() < 1e-9);
        assert!((h(DVec3::NEG_X) - 270.0).abs() < 1e-9);
        assert!((h(DVec3::new(1.0, 5.0, -1.0)) - 45.0).abs() < 1e-9, "looking up does not change it");
        assert!(heading_deg(DVec3::Y, up, north).is_none(), "straight up has no heading");
    }

    #[test]
    fn north_follows_the_ground_towards_the_pole() {
        // On the equator at +x, north is +y; at 45 degrees it leans back towards the pole.
        let n = north_at(DVec3::X);
        assert!((n - DVec3::Y).length() < 1e-9);
        let up = DVec3::new(1.0, 1.0, 0.0).normalize();
        assert!(north_at(up).dot(up).abs() < 1e-9);
        assert!(north_at(up).y > 0.0);
        assert_eq!(north_at(DVec3::Y), DVec3::X, "no north at the pole");
    }

    #[test]
    fn the_heading_number_wraps_at_360() {
        assert_eq!(heading_text(42.0), "042°");
        assert_eq!(heading_text(359.6), "000°");
        assert_eq!(heading_text(-10.0), "350°");
    }

    #[test]
    fn the_strip_labels_every_30_and_names_the_four_directions() {
        let c = look().compass;
        let t = compass_ticks(10.0, Reference::True, &c);
        // A 120 degree strip around 10 shows -50 (310) to 70, with ticks every 5.
        assert!(t.iter().all(|k| k.offset_deg.abs() <= c.span_deg * 0.5 + 1e-9));
        let labels: Vec<_> = t.iter().filter_map(|k| k.label.as_deref()).collect();
        assert_eq!(labels, ["330", "N", "30", "60"]);
        assert!(t.iter().find(|k| k.deg == 0).unwrap().bold && !t.iter().find(|k| k.deg == 30).unwrap().bold);
        assert!(t.iter().find(|k| k.deg == 15).unwrap().label.is_none());
        // Across the wrap, in the bow frame the main marks are numbers.
        let t = compass_ticks(355.0, Reference::Bow, &c);
        let zero = t.iter().find(|k| k.deg == 0).unwrap();
        assert_eq!(zero.label.as_deref(), Some("0"));
        assert!((zero.offset_deg - 5.0).abs() < 1e-9 && zero.bold);
        // The mark under the heading sits in the middle, the edges fade.
        let t = compass_ticks(90.0, Reference::True, &c);
        let e = t.iter().find(|k| k.deg == 90).unwrap();
        assert!(e.x.abs() < 1e-9 && (e.alpha - 1.0).abs() < 1e-9 && e.label.as_deref() == Some("E"));
        assert!(t.first().unwrap().alpha < 0.05);
    }

    #[test]
    fn vitals_show_health_always_and_the_rest_only_while_active() {
        let v = look().vitals;
        let l = vital_lines(1.0, None, None, &v);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].kind, VitalKind::Health);
        assert_eq!(l[0].tone, Tone::Normal);
        let l = vital_lines(0.9, Some(0.6), Some(0.5), &v);
        assert_eq!(l.iter().map(|x| x.kind).collect::<Vec<_>>(), [VitalKind::Health, VitalKind::Oxygen, VitalKind::Burn]);
        assert_eq!(l[2].tone, Tone::Warn);
        let l = vital_lines(0.1, Some(0.1), None, &v);
        assert!(l.iter().all(|x| x.tone == Tone::Bad), "low health and oxygen read bad");
        let l = vital_lines(f64::NAN, Some(2.0), None, &v);
        assert_eq!((l[0].frac, l[1].frac), (0.0, 1.0));
    }

    #[test]
    fn the_credits_flash_for_a_moment_after_a_change_and_fade() {
        let c = look().credits;
        let mut f = CreditsFlash::default();
        assert!(f.step(500, 0.1, &c).is_none(), "the first value is only recorded");
        assert!(f.step(500, 0.1, &c).is_none());
        let v = f.step(800, 0.1, &c).unwrap();
        assert_eq!((v.value, v.delta, v.alpha), (800, 300, 1.0));
        // A second change while it shows adds up and restarts the time.
        let v = f.step(750, 0.1, &c).unwrap();
        assert_eq!(v.delta, 250);
        // Full for show_s, then fading, then gone.
        let mut t = 0.0;
        let mut last = None;
        while let Some(v) = f.step(750, 0.05, &c) {
            t += 0.05;
            last = Some(v);
            assert!(t < 10.0);
        }
        assert!((t - (c.show_s + c.fade_s)).abs() < 0.11, "gone after {t} s");
        assert!(last.unwrap().alpha < 0.3);
        assert!(f.step(750, 0.05, &c).is_none());
        // A new change starts a new delta.
        assert_eq!(f.step(700, 0.05, &c).unwrap().delta, -50);
    }

    #[test]
    fn credits_read_with_separators_and_signs() {
        assert_eq!(credits_text(1250), "1 250 cr");
        assert_eq!(credits_text(12), "12 cr");
        assert_eq!(credits_text(-1000), "-1 000 cr");
        assert_eq!(delta_text(300), "+300");
        assert_eq!(delta_text(-40), "-40");
    }

    #[test]
    fn the_bar_drops_to_half_contrast_after_three_idle_seconds_and_wakes_on_a_touch() {
        let b = look().bar;
        let mut i = BarIdle::default();
        assert_eq!(i.contrast(&b), 1.0);
        i.step(b.idle_s - 0.1);
        assert_eq!(i.contrast(&b), 1.0);
        i.step(b.idle_fade_s * 0.5 + 0.1);
        let mid = i.contrast(&b);
        assert!(mid < 1.0 && mid > b.idle_contrast);
        i.step(10.0);
        assert!((i.contrast(&b) - b.idle_contrast).abs() < 1e-9);
        assert_eq!(b.idle_s, 3.0);
        assert_eq!(b.idle_contrast, 0.5);
        i.touch();
        assert_eq!(i.contrast(&b), 1.0);
    }

    #[test]
    fn the_bar_is_eight_56_px_slots_at_the_bottom_and_larger_in_the_middle_when_open() {
        let b = look().bar;
        let w = DVec2::new(1600.0, 900.0);
        let (a0, s) = bar_slot(0, 8, false, w, &b);
        let (a7, _) = bar_slot(7, 8, false, w, &b);
        assert_eq!(s, 56.0);
        assert!((a0.x + (a7.x + s)) * 0.5 - 800.0 < 1e-9, "centred");
        assert!(a0.y + s <= w.y - b.bottom + 1e-9);
        let (o0, os) = bar_slot(0, 8, true, w, &b);
        assert!(os > s && (o0.y + os * 0.5 - 450.0).abs() < 1e-9);
    }

    #[test]
    fn hints_follow_the_hands() {
        use HintAction::*;
        assert!(hints(false, false, false).is_empty());
        let h = hints(true, true, false);
        assert_eq!(h.iter().map(|x| x.action).collect::<Vec<_>>(), [Use, Drop, Throw]);
        assert!(hints(true, false, false)[0].dim, "an item with no use shows the hint dimmed");
        let h = hints(true, true, true);
        assert_eq!(h.iter().map(|x| x.action).collect::<Vec<_>>(), [Drop, Throw], "carrying: G and R only");
    }

    #[test]
    fn the_prompt_sits_16_px_above_its_target() {
        let l = look();
        let w = DVec2::new(1600.0, 900.0);
        assert_eq!(l.prompt.gap, 16.0);
        let a = prompt_anchor(Some(DVec2::new(700.0, 400.0)), w, &l);
        assert_eq!(a, DVec2::new(700.0, 384.0));
        // Off the screen's edge it stays inside the margin; without a point it sits below the reticle.
        let a = prompt_anchor(Some(DVec2::new(-50.0, 2.0)), w, &l);
        assert_eq!(a, DVec2::new(l.margin, l.margin));
        assert_eq!(prompt_anchor(None, w, &l), DVec2::new(800.0, 900.0 * 0.58));
    }
}
