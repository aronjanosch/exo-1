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
    /// Radiation (#295): a bar, only while it is above zero.
    Radiation,
}

/// The small symbol beside a vital's label (drawn by the glue, 16 px).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VitalSymbol {
    Heart,
    Bubble,
    Trefoil,
    Flame,
}

impl VitalKind {
    pub fn symbol(self) -> VitalSymbol {
        match self {
            VitalKind::Health => VitalSymbol::Heart,
            VitalKind::Oxygen => VitalSymbol::Bubble,
            VitalKind::Radiation => VitalSymbol::Trefoil,
            VitalKind::Burn => VitalSymbol::Flame,
        }
    }
}

/// What each injury takes off the health now, in vitality points (#295): the burn, the oxygen
/// low, and the headache's part of the maximum (the cap).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Costs {
    pub burn: f64,
    pub oxygen: f64,
    pub headache: f64,
}

/// What the vitals block reads, the health system's numbers as the HUD takes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VitalsIn {
    /// Vitality's share of the maximum before any headache.
    pub health: f64,
    /// The maximum now as a share of that (1 without a headache).
    pub max_share: f64,
    /// Air left, None when breathing is no issue.
    pub oxygen: Option<f64>,
    /// The burn's level 1 to 3; 0 without burn.
    pub burn_level: u8,
    /// The burning buildup, 0..1.
    pub buildup: f64,
    /// On fire now.
    pub burning: bool,
    /// Radiation's share, 0 without.
    pub radiation: f64,
    /// Radiation makes nausea.
    pub sick: bool,
    /// What each injury costs the health now.
    pub costs: Costs,
}

/// One line of the vitals block: a bar filled to `frac` (0..1), or for the burn a text line with
/// the buildup as a ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VitalLine {
    pub kind: VitalKind,
    /// The bar's fill: health, air left, radiation; the burn's buildup.
    pub frac: f64,
    /// Good (calm), Warn or Bad by the share; the burn is Warn, Bad at level III or on fire.
    pub tone: Tone,
    /// Where the bar's usable part ends (the health's reduced maximum); 1 otherwise.
    pub cap: f64,
    /// The burn's level 1 to 3; 0 while only building up.
    pub level: u8,
    /// On fire now (the burn line only).
    pub on_fire: bool,
    /// Nausea from radiation (the radiation line only).
    pub sick: bool,
    /// What each injury costs the health now (every line carries them: the health bar's segments
    /// and the cost texts read them).
    pub costs: Costs,
}

impl VitalLine {
    /// Calm lines are plain text, not green.
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

    /// The label line's text: `▲ HEALTH`, `▲ O2`, `■ RAD · SICK`. Every shown line has one.
    pub fn label(&self) -> String {
        let name = match self.kind {
            VitalKind::Health => "HEALTH".to_string(),
            VitalKind::Oxygen => "O2".to_string(),
            VitalKind::Radiation if self.sick => "RAD · SICK".to_string(),
            VitalKind::Radiation => "RAD".to_string(),
            VitalKind::Burn => return self.burn_text(),
        };
        format!("{} {name}", self.glyph()).trim_start().to_string()
    }

    /// The text at the label line's right, `−12`: what the injury costs the health. Oxygen and
    /// health show it only while it costs something (then it stands in for the percent, see
    /// `value`); the burn shows `0` while it costs nothing. The radiation has no cost: it shows
    /// its strength, `8%`.
    pub fn cost_text(&self) -> String {
        match self.kind {
            VitalKind::Burn => cost_points(self.costs.burn),
            VitalKind::Oxygen if self.costs.oxygen.round() < 1.0 => String::new(),
            VitalKind::Oxygen => cost_points(self.costs.oxygen),
            VitalKind::Health if self.costs.headache.round() < 1.0 => String::new(),
            VitalKind::Health => cost_points(self.costs.headache),
            VitalKind::Radiation => format!("{}%", (self.frac * 100.0).round() as i64),
        }
    }

    /// The value at the far right: the share in percent (`85%`) for the health and the air while
    /// they cost nothing. While they cost, the cost text shows instead, so there are never two
    /// numbers side by side. Nothing for the burn and the radiation.
    pub fn value(&self) -> String {
        match self.kind {
            VitalKind::Health | VitalKind::Oxygen if self.cost_text().is_empty() => format!("{}%", (self.frac * 100.0).round() as i64),
            _ => String::new(),
        }
    }

    /// The burn's level 1 to 3 (0 while only building up).
    pub fn burn_level(&self) -> u8 {
        self.level
    }

    /// The burn line: `▲ BURN II`, `▲ BURN` while only building up, `■ BURN III · ON FIRE`.
    pub fn burn_text(&self) -> String {
        let mut s = format!("{} BURN", self.glyph());
        if self.level > 0 {
            s.push(' ');
            s.push_str(["I", "II", "III"][self.level as usize - 1]);
        }
        if self.on_fire {
            s.push_str(" · ON FIRE");
        }
        s.trim_start().to_string()
    }
}

/// Vitality points as text: `−12`, and `0` for none.
fn cost_points(c: f64) -> String {
    let n = c.round() as i64;
    if n == 0 { "0".to_string() } else { format!("\u{2212}{n}") }
}

/// Where the health bar's coloured segments sit, as (from, to) on the bar's 0..1 track. The lost
/// part (between the fill and the cap) is split by cause from the cap inward: burn first (next to
/// the cap), then oxygen low. The grey cap itself is `cap..1` (the headache).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HealthSegments {
    pub burn: (f64, f64),
    pub oxygen: (f64, f64),
}

pub fn health_segments(l: &VitalLine) -> HealthSegments {
    let lost = (l.cap - l.frac).max(0.0);
    let total = l.costs.burn + l.costs.oxygen;
    let (bw, ow) = if total > 0.0 { (lost * l.costs.burn / total, lost * l.costs.oxygen / total) } else { (0.0, 0.0) };
    let burn_from = l.cap - bw;
    HealthSegments { burn: (burn_from, l.cap), oxygen: (burn_from - ow, burn_from) }
}

/// The lines to show now, top to bottom: the burn while it burns or builds up, radiation only
/// above zero (warn above 0, bad above half), oxygen only while it is set (it drops), and health
/// always. The health's bar ends at its reduced maximum (`cap`).
pub fn vital_lines(v: &VitalsIn, look: &VitalsLook) -> Vec<VitalLine> {
    let share = |x: f64| if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 };
    let tone = |x: f64| {
        if x < look.bad_below {
            Tone::Bad
        } else if x < look.warn_below {
            Tone::Warn
        } else {
            Tone::Good
        }
    };
    let costs = v.costs;
    let mut out = Vec::new();
    if v.burn_level > 0 || v.buildup > 0.0 || v.burning {
        let level = v.burn_level.min(3);
        let bad = level == 3 || v.burning;
        out.push(VitalLine { kind: VitalKind::Burn, frac: share(v.buildup), tone: if bad { Tone::Bad } else { Tone::Warn }, cap: 1.0, level, on_fire: v.burning, sick: false, costs });
    }
    let r = share(v.radiation);
    if v.radiation > 0.0 {
        out.push(VitalLine { kind: VitalKind::Radiation, frac: r, tone: if r > 0.5 { Tone::Bad } else { Tone::Warn }, cap: 1.0, level: 0, on_fire: false, sick: v.sick, costs });
    }
    if let Some(o) = v.oxygen {
        out.push(VitalLine { kind: VitalKind::Oxygen, frac: share(o), tone: tone(share(o)), cap: 1.0, level: 0, on_fire: false, sick: false, costs });
    }
    let h = share(v.health);
    out.push(VitalLine { kind: VitalKind::Health, frac: h, tone: tone(h), cap: share(v.max_share), level: 0, on_fire: false, sick: false, costs });
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
    /// A display is in use: F or Escape leaves it.
    Leave,
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

/// The hints while a display is in use: the only key shown is the one that leaves it.
pub fn display_hints() -> Vec<Hint> {
    vec![Hint { action: HintAction::Leave, dim: false }]
}

/// The word beside a key hint.
pub fn hint_label(action: HintAction) -> &'static str {
    match action {
        HintAction::Use => "USE",
        HintAction::Drop => "DROP",
        HintAction::Throw => "THROW",
        HintAction::Leave => "LEAVE",
    }
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

/// How far under the reticle (the window's centre) a display's prompt goes when the display's
/// bottom edge is not on the screen (px).
pub const PROMPT_RETICLE_DROP: f64 = 120.0;

/// The room a prompt needs under a display's edge to fit on the screen (px, its height).
pub const PROMPT_ROOM: f64 = 120.0;

/// The top edge of a display's prompt, for a display in reach: `gap` below the lower of the two
/// projected bottom corners `edge`, kept inside the bottom margin, so it never covers the display.
/// Without room for it on the screen (a corner behind the camera, or the edge too low) it sits
/// `PROMPT_RETICLE_DROP` under the reticle.
pub fn display_prompt_top(edge: Option<[DVec2; 2]>, window: DVec2, look: &HudLook) -> f64 {
    let fallback = window.y * 0.5 + PROMPT_RETICLE_DROP;
    match edge {
        Some([a, b]) if a.is_finite() && b.is_finite() => {
            let top = a.y.max(b.y) + look.prompt.gap;
            if top + PROMPT_ROOM <= window.y - look.margin { top.max(look.margin) } else { fallback }
        }
        _ => fallback,
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

    fn vin(health: f64) -> VitalsIn {
        VitalsIn { health, max_share: 1.0, oxygen: None, burn_level: 0, buildup: 0.0, burning: false, radiation: 0.0, sick: false, costs: Costs::default() }
    }

    #[test]
    fn the_lines_read_top_to_bottom_burn_radiation_oxygen_health() {
        let v = look().vitals;
        let all = VitalsIn { oxygen: Some(0.6), burn_level: 1, buildup: 0.5, radiation: 0.3, health: 0.9, ..vin(0.9) };
        let l = vital_lines(&all, &v);
        assert_eq!(l.iter().map(|x| x.kind).collect::<Vec<_>>(), [VitalKind::Burn, VitalKind::Radiation, VitalKind::Oxygen, VitalKind::Health], "top to bottom: burn, then radiation, oxygen, health at the bottom");
        assert_eq!(l[0].tone, Tone::Warn);
        assert_eq!((l[2].tone, l[3].tone), (Tone::Good, Tone::Good), "60 % and up read calm");
    }

    #[test]
    fn every_shown_line_has_its_label_and_symbol_calm_ones_too() {
        let v = look().vitals;
        let l = vital_lines(&VitalsIn { oxygen: Some(0.9), burn_level: 1, radiation: 0.1, ..vin(1.0) }, &v);
        let got: Vec<(String, VitalSymbol)> = l.iter().map(|x| (x.label(), x.kind.symbol())).collect();
        assert_eq!(got[0], ("▲ BURN I".to_string(), VitalSymbol::Flame));
        assert_eq!(got[1], ("▲ RAD".to_string(), VitalSymbol::Trefoil));
        assert_eq!(got[2], ("O2".to_string(), VitalSymbol::Bubble), "a calm oxygen line still has its label");
        assert_eq!(got[3], ("HEALTH".to_string(), VitalSymbol::Heart));
    }

    #[test]
    fn each_injury_shows_what_it_costs_the_health_and_the_radiation_its_strength() {
        let v = look().vitals;
        let costs = Costs { burn: 12.4, oxygen: 0.0, headache: 20.0 };
        let l = vital_lines(&VitalsIn { burn_level: 2, oxygen: Some(0.9), radiation: 0.08, max_share: 0.8, costs, ..vin(0.6) }, &v);
        assert_eq!(l[0].cost_text(), "\u{2212}12", "the burn costs its points");
        assert_eq!(l[1].cost_text(), "8%", "radiation has no cost: its strength");
        assert_eq!((l[2].cost_text(), l[2].value()), (String::new(), "90%".to_string()), "oxygen costs nothing yet: the percent alone");
        assert_eq!(l[3].cost_text(), "\u{2212}20", "the headache's cap, next to the health");
        assert_eq!((l[3].value(), l[3].cost_text()), (String::new(), "\u{2212}20".to_string()), "a costing health shows the cost in place of the percent");
        let now = vital_lines(&VitalsIn { oxygen: Some(0.2), costs: Costs { oxygen: 66.7, ..Costs::default() }, ..vin(0.5) }, &v);
        assert_eq!((now[0].cost_text(), now[0].value()), ("\u{2212}67".to_string(), String::new()), "oxygen shows its cost in place of the percent once it costs");
        assert_eq!(vital_lines(&vin(1.0), &v)[0].cost_text(), "", "no headache, no cap text");
    }

    #[test]
    fn the_health_bar_splits_its_lost_part_by_cause_from_the_cap_inward() {
        let v = look().vitals;
        let l = vital_lines(&VitalsIn { max_share: 0.8, costs: Costs { burn: 10.0, oxygen: 30.0, headache: 20.0 }, ..vin(0.6) }, &v);
        let seg = health_segments(&l[l.len() - 1]);
        // The lost part is 0.8 - 0.6 = 0.2 of the bar: burn a quarter of it, next to the cap, and
        // oxygen the rest.
        assert!((seg.burn.0 - 0.75).abs() < 1e-9 && (seg.burn.1 - 0.8).abs() < 1e-9, "burn {:?}", seg.burn);
        assert!((seg.oxygen.0 - 0.6).abs() < 1e-9 && (seg.oxygen.1 - 0.75).abs() < 1e-9, "oxygen {:?}", seg.oxygen);
        let none = health_segments(&vital_lines(&vin(0.6), &v)[0]);
        assert_eq!(none.burn, none.oxygen, "no cause, no segment");
    }

    #[test]
    fn the_burn_reads_its_level_the_buildup_and_the_fire() {
        let v = look().vitals;
        let l = vital_lines(&VitalsIn { burn_level: 2, ..vin(0.58) }, &v);
        assert_eq!((l[0].burn_text(), l[1].label(), l[1].value()), ("▲ BURN II".to_string(), "▲ HEALTH".to_string(), "58%".to_string()));
        assert_eq!(l[0].frac, 0.0, "no buildup, the ring is empty");
        let l = vital_lines(&VitalsIn { buildup: 0.4, ..vin(1.0) }, &v);
        assert_eq!((l[0].burn_text(), l[0].frac, l[0].tone), ("▲ BURN".to_string(), 0.4, Tone::Warn), "building up: BURN without a level");
        let l = vital_lines(&VitalsIn { burn_level: 1, burning: true, ..vin(1.0) }, &v);
        assert_eq!((l[0].burn_text(), l[0].tone), ("■ BURN I · ON FIRE".to_string(), Tone::Bad), "on fire is bad");
        let l = vital_lines(&VitalsIn { oxygen: Some(0.41), ..vin(1.0) }, &v);
        assert_eq!((l[0].label(), l[0].value(), l[0].tone), ("▲ O2".to_string(), "41%".to_string(), Tone::Warn));
        let l = vital_lines(&VitalsIn { oxygen: Some(0.18), burn_level: 3, health: 0.23, ..vin(0.23) }, &v);
        assert_eq!((l[0].burn_text(), l[0].tone, l[1].label(), l[2].label()), ("■ BURN III".to_string(), Tone::Bad, "■ O2".to_string(), "■ HEALTH".to_string()));
        let l = vital_lines(&vin(f64::NAN), &v);
        assert_eq!(l[0].frac, 0.0);
    }

    #[test]
    fn a_headache_caps_the_health_bar_at_the_reduced_maximum() {
        let v = look().vitals;
        let l = vital_lines(&VitalsIn { max_share: 0.8, ..vin(0.6) }, &v);
        assert_eq!((l[0].frac, l[0].cap), (0.6, 0.8));
        assert_eq!(vital_lines(&vin(0.6), &v)[0].cap, 1.0, "no headache, no cap");
    }

    #[test]
    fn radiation_is_a_bar_only_above_zero_bad_above_half_and_sick_with_nausea() {
        let v = look().vitals;
        assert!(vital_lines(&vin(1.0), &v).iter().all(|l| l.kind != VitalKind::Radiation));
        let l = vital_lines(&VitalsIn { radiation: 0.3, ..vin(0.8) }, &v);
        assert_eq!((l[0].kind, l[0].label(), l[0].tone), (VitalKind::Radiation, "▲ RAD".to_string(), Tone::Warn));
        let l = vital_lines(&VitalsIn { radiation: 0.6, sick: true, ..vin(0.8) }, &v);
        assert_eq!((l[0].tone, l[0].cost_text(), l[0].label()), (Tone::Bad, "60%".to_string(), "■ RAD · SICK".to_string()));
        assert_eq!(l[0].kind.symbol(), VitalSymbol::Trefoil);
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
    fn a_display_in_use_shows_only_leave() {
        let h = display_hints();
        assert_eq!(h.iter().map(|x| x.action).collect::<Vec<_>>(), [HintAction::Leave]);
        assert!(!h[0].dim);
        assert_eq!(hint_label(HintAction::Leave), "LEAVE");
        assert_eq!(hint_label(HintAction::Use), "USE");
        assert_eq!(hint_label(HintAction::Drop), "DROP");
        assert_eq!(hint_label(HintAction::Throw), "THROW");
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

    #[test]
    fn a_display_prompt_sits_below_the_screen_never_over_it() {
        let l = look();
        let w = DVec2::new(1600.0, 900.0);
        // The screen's bottom edge at y 500 (both corners): the prompt's top is 16 px under it.
        let edge = [DVec2::new(600.0, 500.0), DVec2::new(1000.0, 500.0)];
        assert_eq!(display_prompt_top(Some(edge), w, &l), 516.0);
        // The lower corner decides.
        assert_eq!(display_prompt_top(Some([DVec2::new(600.0, 420.0), DVec2::new(1000.0, 500.0)]), w, &l), 516.0);
        // The edge too low for the prompt to fit: 120 px under the reticle (the centre, y 450).
        let low = [DVec2::new(600.0, 800.0), DVec2::new(1000.0, 800.0)];
        assert_eq!(display_prompt_top(Some(low), w, &l), 450.0 + PROMPT_RETICLE_DROP);
        // A corner behind the camera (no projection) is the same fallback.
        assert_eq!(display_prompt_top(Some([DVec2::new(f64::NAN, 0.0), DVec2::new(1000.0, 500.0)]), w, &l), 450.0 + PROMPT_RETICLE_DROP);
        assert_eq!(display_prompt_top(None, w, &l), 450.0 + PROMPT_RETICLE_DROP);
    }
}
