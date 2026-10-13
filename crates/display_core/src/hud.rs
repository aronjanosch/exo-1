//! The crew HUD's rules without engine types (Crew HUD v3, Claude Design project "EXO-1 Ship
//! Displays"; decision "Minimal HUD" in the concept repo). Six elements: compass, vitals, the
//! interaction prompt, the action bar, the key hints and the reticle, plus the credits that show
//! for a moment after a change. This file holds what can be tested without a window: the sizes
//! (`HudLook`, the `hud` section of `content/ui/displays.json`), the compass strip, the vitals
//! lines, the credits flash, the bar's idle fade, the bar's layout, the key hints and the prompt's
//! anchor. The glue (`exo_app::crew_hud`) draws what these return.
use crate::{Rgb, Tone};
use glam::{DVec2, DVec3};
use serde::Deserialize;

// ---------- the look (data) ----------

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HudLook {
    /// Space between the screen edge and the credits, and the limit the prompt keeps (px).
    pub margin: f64,
    /// Distance of the vitals, the key hints and the action bar from the screen's edges (px).
    pub edge: f64,
    /// The pale text and line colour (`rgba(234,247,241,a)` in the design), alpha comes per use.
    pub soft: Rgb,
    pub scrim: ScrimLook,
    pub compass: CompassLook,
    pub vitals: VitalsLook,
    pub prompt: PromptLook,
    pub bar: BarLook,
    pub hints: HintsLook,
    pub reticle: ReticleLook,
    pub credits: CreditsLook,
    pub text: TextLook,
}

/// The dark gradient along the bottom edge that keeps the lower HUD readable.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScrimLook {
    pub colour: Rgb,
    pub height: f64,
    pub alpha: f64,
    /// With the inventory (Tab) open.
    pub open_height: f64,
    pub open_alpha: f64,
}

/// The heading strip at the top centre. All positions are px from the box's top left.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CompassLook {
    /// Box width and height, and its distance from the screen's top.
    pub width: f64,
    pub height: f64,
    pub top: f64,
    /// The degrees the strip shows across its width (so px per degree is `width / span_deg`).
    pub span_deg: f64,
    /// A small tick every this many degrees, a number every `label_every_deg` (a multiple).
    pub tick_every_deg: f64,
    pub label_every_deg: f64,
    /// The strip is opaque in the middle and fades out over this share of the width at each end.
    pub fade: f64,
    /// The current heading as a number.
    pub heading_size: f64,
    /// The pointer triangle (down), its width, height and top.
    pub pointer_w: f64,
    pub pointer_h: f64,
    pub pointer_y: f64,
    /// The base line: top, thickness, alpha.
    pub line_y: f64,
    pub line_px: f64,
    pub line_alpha: f64,
    /// Ticks: top, width, length at multiples of 90, of 15, otherwise, alpha at the minor ones.
    pub tick_y: f64,
    pub tick_w: f64,
    pub tick_major_px: f64,
    pub tick_mid_px: f64,
    pub tick_px: f64,
    pub tick_alpha: f64,
    /// N E S W (bold) and the other labels: size, top, and alpha of the others.
    pub cardinal_size: f64,
    pub cardinal_top: f64,
    pub label_size: f64,
    pub label_top: f64,
    pub label_alpha: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VitalsLook {
    /// Width of the column, the gap between its lines and between a label line and its track.
    pub width: f64,
    pub gap: f64,
    pub label_gap: f64,
    pub track_h: f64,
    pub track_alpha: f64,
    pub label_size: f64,
    pub value_size: f64,
    /// Critical bars glow in the bad colour: blur (px) and alpha.
    pub glow_px: f64,
    pub glow_alpha: f64,
    /// Below these shares health and oxygen read as warn and as bad.
    pub warn_below: f64,
    pub bad_below: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PromptLook {
    /// The prompt's bottom edge sits this far above its target (px).
    pub gap: f64,
    /// The F keycap: edge, border, letter size, background alpha, border alpha when blocked.
    pub key_px: f64,
    pub key_border: f64,
    pub key_size: f64,
    pub key_bg_alpha: f64,
    pub blocked_alpha: f64,
    /// Gap between keycap and text, between the text lines, and above the note line.
    pub key_gap: f64,
    pub line_gap: f64,
    pub note_gap: f64,
    pub verb_size: f64,
    pub verb_alpha: f64,
    pub name_size: f64,
    pub sub_size: f64,
    pub note_size: f64,
    /// Dashed outlines (blocked key, carry frames): dash and gap length (px).
    pub dash: f64,
    pub dash_gap: f64,
}

/// One size of action bar slot: closed (bottom centre) or open (Tab).
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SlotLook {
    pub size: f64,
    pub gap: f64,
    pub border: f64,
    /// The selected slot's border.
    pub border_selected: f64,
    /// The slot's number (top left) and its distance from the corner.
    pub num_size: f64,
    pub num_x: f64,
    pub num_y: f64,
    /// Alpha of the number of an unselected slot (the selected one is white).
    pub num_alpha: f64,
    /// The item's code, centred: size and top.
    pub code_size: f64,
    pub code_top: f64,
    /// Count (bottom right) and two-hand mark (bottom left) size, and their distance from the side
    /// (from the bottom: half of it).
    pub tag_size: f64,
    pub tag_pad: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BarLook {
    pub closed: SlotLook,
    pub open: SlotLook,
    /// How far the held slot lifts (closed).
    pub lift: f64,
    /// Closed bar: its bottom edge's distance from the screen and the held name's line (height,
    /// gap above it, size).
    pub bottom: f64,
    pub name_h: f64,
    pub name_gap: f64,
    pub name_size: f64,
    /// Open bar: bottom distance of the row below the slots (name left, keys right), its height, the gap
    /// between slots and row, text size.
    pub open_bottom: f64,
    pub row_h: f64,
    pub row_gap: f64,
    pub row_size: f64,
    /// The radial scrim behind the open bar: ellipse size, alpha in the middle, at `scrim_mid_at`
    /// and gone at `scrim_end_at` (shares of the ellipse's radius).
    pub scrim_w: f64,
    pub scrim_h: f64,
    pub scrim_alpha: f64,
    pub scrim_mid_alpha: f64,
    pub scrim_mid_at: f64,
    pub scrim_end_at: f64,
    /// Seconds without activity until everything but the held slot dims, how long the dimming
    /// takes, and the contrast it dims to (0..1); the held name dims to `name_idle_contrast`.
    pub idle_s: f64,
    pub idle_fade_s: f64,
    pub idle_contrast: f64,
    pub name_idle_contrast: f64,
    /// Contrast of the slots while the hands carry cargo (dashed frames).
    pub carry_contrast: f64,
    /// Alphas of the slot states (soft border / fill): filled, empty, carrying, selected fill,
    /// hovered fill.
    pub filled_border: f64,
    pub filled_bg: f64,
    pub empty_border: f64,
    pub empty_bg: f64,
    pub carry_border: f64,
    pub carry_bg: f64,
    pub selected_bg: f64,
    pub hover_bg: f64,
    /// A drag: the ghost's fill colour and alpha, its tilt (degrees, counter-clockwise), the drop
    /// outline's distance from the slot, the dragged slot's dash alpha.
    pub ghost_bg: Rgb,
    pub ghost_bg_alpha: f64,
    pub ghost_tilt_deg: f64,
    pub drop_offset: f64,
    pub origin_alpha: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HintsLook {
    /// Gap between the rows, and between a row's text and its keycap.
    pub gap: f64,
    pub key_gap: f64,
    pub size: f64,
    /// Letter spacing in em.
    pub spacing: f64,
    /// Keycap: minimum width, height, border, border alpha, fill alpha, letter size.
    pub key_min_w: f64,
    pub key_h: f64,
    pub key_border: f64,
    pub key_alpha: f64,
    pub key_bg_alpha: f64,
    pub key_size: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReticleLook {
    pub dot_px: f64,
    pub ring_px: f64,
    pub ring_stroke_px: f64,
    /// The dark outline around dot and ring, and its alpha.
    pub outline_px: f64,
    pub outline_alpha: f64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreditsLook {
    /// Seconds shown at full strength after a change, then the fade.
    pub show_s: f64,
    pub fade_s: f64,
    pub size: f64,
    pub unit_size: f64,
    pub unit_alpha: f64,
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
        // Every field is a number or a colour, so the debug text has a minus sign or `NaN` or
        // `inf` exactly when one of them is negative or not finite.
        let dump = format!("{self:?}");
        if dump.contains('-') || dump.contains("NaN") || dump.contains("inf") {
            return Err("displays.json: hud sizes must be finite and not negative".into());
        }
        let (c, v, b, t) = (&self.compass, &self.vitals, &self.bar, &self.text);
        if c.tick_every_deg <= 0.0 || c.span_deg <= 0.0 || c.span_deg > 360.0 || c.width <= 0.0 {
            return Err("displays.json: hud.compass needs tick_every_deg > 0, width > 0 and 0 < span_deg <= 360".into());
        }
        let k = c.label_every_deg / c.tick_every_deg;
        if c.label_every_deg <= 0.0 || (k - k.round()).abs() > 1e-9 {
            return Err("displays.json: hud.compass.label_every_deg must be a multiple of tick_every_deg".into());
        }
        if b.idle_contrast > 1.0 || b.name_idle_contrast > 1.0 || b.carry_contrast > 1.0 || v.warn_below > 1.0 || v.bad_below > v.warn_below || t.shadow_alpha > 1.0 || c.fade > 0.5 {
            return Err("displays.json: hud shares (contrast, warn_below, bad_below, shadow_alpha, fade) are at most 1 (fade 0.5), bad_below under warn_below".into());
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

/// How long a mark's tick is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickLen {
    /// Multiples of 90 degrees.
    Major,
    /// Multiples of 15.
    Mid,
    Minor,
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
    /// The label: N E S W and NE SE SW NW on a true heading, numbers every 30 degrees, in the bow
    /// frame the numbers 0 90 180 270 for the four main ones.
    pub label: Option<String>,
    /// The four main labels (bold, larger); their ticks are white.
    pub bold: bool,
    pub len: TickLen,
    /// Fades towards the ends of the strip, 0..1.
    pub alpha: f64,
}

/// The marks of the strip around `heading`.
pub fn compass_ticks(heading: f64, reference: Reference, c: &CompassLook) -> Vec<Tick> {
    let half = c.span_deg * 0.5;
    let first = ((heading - half) / c.tick_every_deg).ceil() as i64;
    let last = ((heading + half) / c.tick_every_deg).floor() as i64;
    let label_k = (c.label_every_deg / c.tick_every_deg).round() as i64;
    let multiple = |deg: f64, of: f64| (deg / of - (deg / of).round()).abs() < 1e-9;
    (first..=last)
        .map(|k| {
            let at = k as f64 * c.tick_every_deg;
            let offset = at - heading;
            let deg = normalize_deg(at);
            let main = multiple(deg, 90.0);
            let label = if main {
                Some(match (reference, deg as u32) {
                    (Reference::True, 0) => "N".to_string(),
                    (Reference::True, 90) => "E".to_string(),
                    (Reference::True, 180) => "S".to_string(),
                    (Reference::True, 270) => "W".to_string(),
                    (_, d) => d.to_string(),
                })
            } else if reference == Reference::True && multiple(deg, 45.0) {
                Some(["NE", "SE", "SW", "NW"][(deg as u32 / 90) as usize].to_string())
            } else if k.rem_euclid(label_k) == 0 {
                Some((deg as u32).to_string())
            } else {
                None
            };
            let x = offset / c.span_deg;
            let across = 0.5 + x;
            let alpha = if c.fade > 0.0 { (across.min(1.0 - across) / c.fade).clamp(0.0, 1.0) } else { 1.0 };
            let len = if main {
                TickLen::Major
            } else if multiple(deg, 15.0) {
                TickLen::Mid
            } else {
                TickLen::Minor
            };
            Tick { offset_deg: offset, deg: deg as u32, x, label, bold: main, len, alpha }
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

/// One line of the vitals block: a bar filled to `frac` (0..1), or for the burn a text line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VitalLine {
    pub kind: VitalKind,
    pub frac: f64,
    /// Good (calm), Warn or Bad by the share; the burn is Warn, Bad at its third level.
    pub tone: Tone,
}

impl VitalLine {
    /// Calm lines show no label and no value.
    pub fn calm(&self) -> bool {
        self.tone == Tone::Good
    }

    /// The status glyph beside the label: ▲ warn, ■ bad, none when calm.
    pub fn glyph(&self) -> &'static str {
        match self.tone {
            Tone::Warn => "▲",
            Tone::Bad => "■",
            _ => "",
        }
    }

    /// The label line's text: `▲ HEALTH`.
    pub fn label(&self) -> String {
        let name = match self.kind {
            VitalKind::Health => "HEALTH",
            VitalKind::Oxygen => "OXYGEN",
            VitalKind::Burn => return self.burn_text(),
        };
        format!("{} {name}", self.glyph()).trim_start().to_string()
    }

    /// The value beside the label: the share in percent.
    pub fn value(&self) -> String {
        format!("{}", (self.frac * 100.0).round() as i64)
    }

    /// The burn's level 1 to 3 from how hard it is.
    pub fn burn_level(&self) -> u8 {
        (self.frac * 3.0).ceil().clamp(1.0, 3.0) as u8
    }

    /// The burn line: `▲ BURN II`.
    pub fn burn_text(&self) -> String {
        format!("{} BURN {}", self.glyph(), ["I", "II", "III"][self.burn_level() as usize - 1])
    }
}

/// The lines to show now, top to bottom: the burn only while burning, oxygen only while it is set
/// (it drops), health always.
pub fn vital_lines(health: f64, oxygen: Option<f64>, burn: Option<f64>, v: &VitalsLook) -> Vec<VitalLine> {
    let share = |x: f64| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
    let tone = |x: f64| {
        if x < v.bad_below {
            Tone::Bad
        } else if x < v.warn_below {
            Tone::Warn
        } else {
            Tone::Good
        }
    };
    let mut out = Vec::new();
    if let Some(b) = burn {
        let mut l = VitalLine { kind: VitalKind::Burn, frac: share(b), tone: Tone::Warn };
        if l.burn_level() == 3 {
            l.tone = Tone::Bad;
        }
        out.push(l);
    }
    if let Some(o) = oxygen {
        out.push(VitalLine { kind: VitalKind::Oxygen, frac: share(o), tone: tone(share(o)) });
    }
    out.push(VitalLine { kind: VitalKind::Health, frac: share(health), tone: tone(share(health)) });
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

/// `1 250`: thousands separated by a thin gap (the unit CR is drawn beside it).
pub fn credits_number(v: i64) -> String {
    let digits = v.unsigned_abs().to_string();
    let mut s = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            s.push(' ');
        }
        s.push(ch);
    }
    format!("{}{s}", if v < 0 { "-" } else { "" })
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
        1.0 - self.dim(b) * (1.0 - b.idle_contrast)
    }

    /// The held name's alpha: 1 down to `name_idle_contrast` over the same time.
    pub fn name_contrast(&self, b: &BarLook) -> f64 {
        1.0 - self.dim(b) * (1.0 - b.name_idle_contrast)
    }

    /// How far into the dimming, 0..1.
    fn dim(&self, b: &BarLook) -> f64 {
        if b.idle_fade_s > 0.0 {
            ((self.idle - b.idle_s) / b.idle_fade_s).clamp(0.0, 1.0)
        } else {
            (self.idle >= b.idle_s) as u8 as f64
        }
    }
}

/// Slot `i` of `n` on the screen: top left and edge (px), at rest (the held slot's lift is the
/// glue's). Closed it sits at the bottom with room for the held name; open (Tab) it is larger,
/// with the name and keys row below it.
pub fn bar_slot(i: usize, n: usize, open: bool, window: DVec2, b: &BarLook) -> (DVec2, f64) {
    let s = if open { &b.open } else { &b.closed };
    let n = n as f64;
    let width = n * s.size + (n - 1.0) * s.gap;
    let x = (window.x - width) * 0.5 + i as f64 * (s.size + s.gap);
    let y = window.y - s.size - if open { b.open_bottom + b.row_h + b.row_gap } else { b.bottom + b.name_h + b.name_gap };
    (DVec2::new(x, y), s.size)
}

/// Top of the text row under the slots: the held name closed, the name and keys open.
pub fn bar_row_top(open: bool, window: DVec2, b: &BarLook) -> f64 {
    if open {
        window.y - b.open_bottom - b.row_h
    } else {
        window.y - b.bottom - b.name_h
    }
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

/// What the open inventory is doing: the keys it shows follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabMode {
    Browse,
    Drag,
    /// The hands carry cargo.
    Carrying,
}

/// The keys of an open-inventory hint; the glue writes the bound key for the last ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKey {
    Click,
    Drag,
    Drop,
    Throw,
    Close,
    Cancel,
}

/// The hints under the open bar, keycap then text.
pub fn tab_hints(mode: TabMode) -> Vec<(TabKey, &'static str)> {
    match mode {
        TabMode::Browse => vec![(TabKey::Click, "HOLD"), (TabKey::Drag, "MOVE"), (TabKey::Drop, "DROP"), (TabKey::Throw, "THROW"), (TabKey::Close, "CLOSE")],
        TabMode::Drag => vec![(TabKey::Drag, "MOVE"), (TabKey::Cancel, "CANCEL")],
        TabMode::Carrying => vec![(TabKey::Drop, "DROP"), (TabKey::Throw, "THROW"), (TabKey::Close, "CLOSE")],
    }
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

/// What the prompt says: the verb over the target's name, then optionally a status next to the
/// name and a note line under the row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptView {
    pub verb: String,
    pub name: String,
    /// A machine status or a tag beside the name (empty: none).
    pub sub: String,
    pub sub_tone: Tone,
    /// The line under the row (empty: none): `or ESC`, `▲ Hands full · G drop`.
    pub note: String,
    pub note_tone: Tone,
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
        let mut l = look();
        l.vitals.bad_below = 0.9;
        assert!(l.check().is_err(), "bad starts below warn");
        let mut l = look();
        l.compass.pointer_h = -1.0;
        assert!(l.check().is_err());
        l.compass.pointer_h = f64::NAN;
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
    fn the_strip_names_the_eight_directions_and_numbers_every_30() {
        let c = look().compass;
        assert_eq!(c.width / c.span_deg, 5.0, "5 px per degree");
        let t = compass_ticks(10.0, Reference::True, &c);
        // 128 degrees around 10 show 306 to 74, with ticks every 5.
        assert!(t.iter().all(|k| k.offset_deg.abs() <= c.span_deg * 0.5 + 1e-9));
        let labels: Vec<_> = t.iter().filter_map(|k| k.label.as_deref()).collect();
        assert_eq!(labels, ["NW", "330", "N", "30", "NE", "60"]);
        let at = |d: u32| t.iter().find(|k| k.deg == d).unwrap();
        assert!(at(0).bold && !at(30).bold && !at(45).bold);
        assert!(at(15).label.is_none());
        assert_eq!((at(0).len, at(30).len, at(45).len, at(5).len), (TickLen::Major, TickLen::Mid, TickLen::Mid, TickLen::Minor));
        // Across the wrap; in the bow frame the main marks are numbers and there are no letters.
        let t = compass_ticks(355.0, Reference::Bow, &c);
        let zero = t.iter().find(|k| k.deg == 0).unwrap();
        assert_eq!(zero.label.as_deref(), Some("0"));
        assert!((zero.offset_deg - 5.0).abs() < 1e-9 && zero.bold);
        assert!(t.iter().filter_map(|k| k.label.as_deref()).all(|l| l.chars().all(|c| c.is_ascii_digit())));
        // The mark under the heading sits in the middle at full strength; the strip is opaque from
        // 24 % to 76 % of its width and fades to nothing at both ends.
        let t = compass_ticks(90.0, Reference::True, &c);
        let e = t.iter().find(|k| k.deg == 90).unwrap();
        assert!(e.x.abs() < 1e-9 && (e.alpha - 1.0).abs() < 1e-9 && e.label.as_deref() == Some("E"));
        assert!(t.first().unwrap().alpha < 0.15 && t.last().unwrap().alpha < 0.15);
        let near = t.iter().find(|k| k.deg == 85).unwrap();
        assert!((near.alpha - 1.0).abs() < 1e-9);
        let faded = t.iter().find(|k| k.deg == 50).unwrap();
        assert!(faded.alpha > 0.0 && faded.alpha < 1.0, "{}", faded.alpha);
    }

    #[test]
    fn vitals_list_burn_oxygen_health_and_only_health_when_calm() {
        let v = look().vitals;
        let l = vital_lines(1.0, None, None, &v);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].kind, VitalKind::Health);
        assert!(l[0].calm() && l[0].glyph().is_empty(), "healthy reads calm, not green");
        let l = vital_lines(0.9, Some(0.6), Some(0.5), &v);
        assert_eq!(l.iter().map(|x| x.kind).collect::<Vec<_>>(), [VitalKind::Burn, VitalKind::Oxygen, VitalKind::Health], "top to bottom");
        assert_eq!(l[0].tone, Tone::Warn);
        assert_eq!((l[1].tone, l[2].tone), (Tone::Good, Tone::Good), "60 % and up read calm");
        // The design's examples: burn, oxygen and crit.
        let l = vital_lines(0.58, None, Some(0.5), &v);
        assert_eq!((l[0].burn_text(), l[1].label(), l[1].value()), ("▲ BURN II".to_string(), "▲ HEALTH".to_string(), "58".to_string()));
        let l = vital_lines(1.0, Some(0.41), None, &v);
        assert_eq!((l[0].label(), l[0].value(), l[0].tone, l[1].calm()), ("▲ OXYGEN".to_string(), "41".to_string(), Tone::Warn, true));
        let l = vital_lines(0.23, Some(0.18), Some(1.0), &v);
        assert_eq!((l[0].burn_text(), l[0].tone, l[1].label(), l[2].label()), ("■ BURN III".to_string(), Tone::Bad, "■ OXYGEN".to_string(), "■ HEALTH".to_string()));
        let l = vital_lines(f64::NAN, Some(2.0), None, &v);
        assert_eq!((l[1].frac, l[0].frac), (0.0, 1.0));
        assert_eq!(vital_lines(1.0, None, Some(0.1), &v)[0].burn_text(), "▲ BURN I");
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
        assert_eq!(credits_number(1250), "1 250");
        assert_eq!(credits_number(12), "12");
        assert_eq!(credits_number(-1000), "-1 000");
        assert_eq!(delta_text(300), "+300");
        assert_eq!(delta_text(-40), "-40");
    }

    #[test]
    fn the_bar_drops_to_55_percent_after_three_idle_seconds_and_wakes_on_a_touch() {
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
        assert_eq!(b.idle_contrast, 0.55);
        assert!((i.name_contrast(&b) - b.name_idle_contrast).abs() < 1e-9);
        i.touch();
        assert_eq!(i.contrast(&b), 1.0);
    }

    #[test]
    fn the_bar_is_eight_56_px_slots_at_the_bottom_and_88_px_with_a_row_below_when_open() {
        let b = look().bar;
        let w = DVec2::new(1600.0, 900.0);
        let (a0, s) = bar_slot(0, 8, false, w, &b);
        let (a7, _) = bar_slot(7, 8, false, w, &b);
        assert_eq!((s, a0.x), (56.0, 548.0), "8 slots and 7 gaps of 8 are 504 wide, centred");
        assert!((a0.x + (a7.x + s)) * 0.5 - 800.0 < 1e-9);
        // The name line (20 px) under the bar, 8 px below the slots, ends 40 px above the edge.
        assert_eq!(a0.y + s + b.name_gap, bar_row_top(false, w, &b));
        assert_eq!(bar_row_top(false, w, &b) + b.name_h, 860.0);
        let (o0, os) = bar_slot(0, 8, true, w, &b);
        assert_eq!((os, o0.x), (88.0, 420.0));
        assert_eq!(o0.y + os + b.row_gap, bar_row_top(true, w, &b));
        assert_eq!(bar_row_top(true, w, &b) + b.row_h, 900.0 - 44.0);
    }

    #[test]
    fn the_open_bar_shows_the_keys_of_what_it_is_doing() {
        let keys = |m| tab_hints(m).iter().map(|(k, t)| format!("{k:?} {t}")).collect::<Vec<_>>();
        assert_eq!(keys(TabMode::Browse), ["Click HOLD", "Drag MOVE", "Drop DROP", "Throw THROW", "Close CLOSE"]);
        assert_eq!(keys(TabMode::Drag), ["Drag MOVE", "Cancel CANCEL"]);
        assert_eq!(keys(TabMode::Carrying), ["Drop DROP", "Throw THROW", "Close CLOSE"]);
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
