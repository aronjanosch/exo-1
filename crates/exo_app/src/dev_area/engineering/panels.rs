//! Engineering's screens: the reactor's readout (with its help view), the coolant pump, the junction
//! box (breakers and limits), AUX, the battery, the status board RX-02 (lamps, boost, the eject) and
//! the ship's power panel SP-01 (`startup_screen`). Each reads the host's snapshot; a press asks the host.
//! TODO(initiator): the glow of the critical text and the scanlines' look are not drawn yet.
use crate::dev_area::*;
use crate::displays::{ActiveDisplay, DisplayAction, Panels};
use crate::dev_area::screens::*;
use crate::dev_area::ship::{lamp_blink_on, ReactorConfig};
use crate::items::Items;
use display_core::{button, canvas, column, ctl, font, label, micro, readout, row, slider, space, toggle, Ink, Item, Panel, Rect, Shape, Tier, TextFace, Tone, Widget};
use item_core::wire::{BotStation, BreakerGroup, CanisterKind, CanisterSlot, Lamp, PumpMode, ReactorAsk, ReactorNews, CANISTER_SLOTS};

use crate::dev_area::crew::{bot_tagged, manned};
use crate::dev_area::engineering::reactor_room::{MAIN_ID, SUPPLY_ID};
use crate::dev_area::engineering::startup_screen;

pub fn plugin(app: &mut App) {
    app.init_resource::<ReactorHelp>();
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

const LAMPS: [Lamp; Lamp::COUNT] = [Lamp::Hot, Lamp::CritHeat, Lamp::Cold, Lamp::LowHeat, Lamp::HighOut, Lamp::CritOut, Lamp::LowOut, Lamp::FillLow, Lamp::NoFill, Lamp::Implosion, Lamp::Scram];

/// A lamp's cell (px) and the gap between the cells.
const LAMP_W: f64 = 96.0;

const LAMP_H: f64 = 20.0;

const LAMP_GAP: f64 = 3.0;

pub const READOUT_ID: PanelId = PanelId::new("reactor_readout");

pub const READOUT_NAME: &str = "REACTOR";

pub const PUMP_ID: PanelId = PanelId::new("coolant_pump");

pub const PUMP_NAME: &str = "COOLANT PUMP";

pub const JUNCTION_ID: PanelId = PanelId::new("junction_box");

pub const JUNCTION_NAME: &str = "JUNCTION BOX";

pub const AUX_ID: PanelId = PanelId::new("aux_panel");

pub const BATTERY_ID: PanelId = PanelId::new("battery_panel");

pub const BATTERY_NAME: &str = "BATTERY";

pub const AUX_NAME: &str = "AUX";

pub const STATUS_ID: PanelId = PanelId::new("reactor_status");

pub const STATUS_NAME: &str = "RX STATUS";

pub const HELP_ACTION: &str = "help";

/// Whether this player has the readout's help view open. Local UI state: it is not sent, and each
/// process draws its own panels. Closed when the player leaves the display (`write_panels`).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactorHelp(pub bool);

impl ReactorHelp {
    pub fn toggle(&mut self) {
        self.0 = !self.0;
    }
}

/// The arc's start (bottom left) and its sweep (270 degrees, clockwise on screen).
const ARC_START: f64 = 135.0;

const ARC_SWEEP: f64 = 270.0;

/// The angle of a value 0..100 on the arc.
fn angle(v: f64) -> f64 {
    ARC_START + ARC_SWEEP * v.clamp(0.0, 100.0) / 100.0
}

/// The heat zones of the sim's bands: `Good` is the optimal band, `Low` and `High` the
/// rest of the allowed band, `Crit` above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Cold,
    Low,
    Good,
    High,
    Crit,
}

/// The bands `(optimal, allowed)` the screens show: the news' (the sim's, from the operator's
/// skill). Without news the skill-50 bands, so the screens stay readable.
pub fn bands(r: Option<&ReactorNews>) -> ([f64; 2], [f64; 2]) {
    match r {
        Some(r) if r.optimal[1] > r.optimal[0] => (r.optimal, r.allowed),
        _ => (SKILL_50_OPTIMAL, SKILL_50_ALLOWED),
    }
}

/// The sim's bands at skill 50 (`reactor_core::Bands::new`).
const SKILL_50_OPTIMAL: [f64; 2] = [35.0, 65.0];

const SKILL_50_ALLOWED: [f64; 2] = [20.0, 80.0];

pub fn zone(t: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Zone {
    if t > allowed[1] {
        Zone::Crit
    } else if t > optimal[1] {
        Zone::High
    } else if t >= optimal[0] {
        Zone::Good
    } else if t >= allowed[0] {
        Zone::Low
    } else {
        Zone::Cold
    }
}

pub fn zone_tone(z: Zone) -> Tone {
    match z {
        Zone::Cold => Tone::Dim,
        Zone::Low | Zone::High => Tone::Warn,
        Zone::Good => Tone::Good,
        Zone::Crit => Tone::Bad,
    }
}

fn arc(cx: f64, cy: f64, r: f64, from: f64, to: f64, stroke: f64, ink: Ink, dash: Option<(f64, f64)>) -> Shape {
    Shape::Arc { cx, cy, r, from, to, stroke, ink, dash }
}

fn heat_gauge(heat: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Item {
    let (cx, cy, r, sw) = (108.0, 70.0, 60.0, 14.0);
    let z = zone(heat, optimal, allowed);
    let t = heat.clamp(0.0, 100.0);
    let mut s = vec![
        arc(cx, cy, r, angle(0.0), angle(allowed[0]), sw, Ink::Line2, None),
        arc(cx, cy, r, angle(allowed[0]), angle(optimal[0]), sw, Ink::Warn, Some((9.0, 4.0))),
        arc(cx, cy, r, angle(optimal[0]), angle(optimal[1]), sw, Ink::Good, None),
        arc(cx, cy, r, angle(optimal[1]), angle(allowed[1]), sw, Ink::Warn, Some((9.0, 4.0))),
        arc(cx, cy, r, angle(allowed[1]), angle(100.0), sw, Ink::Bad, Some((3.0, 2.0))),
    ];
    let a = angle(t).to_radians();
    s.push(Shape::Line { a: [cx, cy], b: [cx + 46.0 * a.cos(), cy + 46.0 * a.sin()], width: 5.0, ink: Ink::FgTitle, dash: None });
    s.push(Shape::Circle { cx, cy, r: 8.0, fill: Some(Ink::Bg0), stroke: Some((Ink::FgTitle, 3.0)) });
    s.push(text(0.0, 74.0, 216.0, 28.0, &format!("{heat:.0} °C"), 28.0, TextFace::Mono, zone_ink(z), true));
    s.push(text(0.0, 124.0, 216.0, 20.0, "CORE HEAT", 16.0, TextFace::Display, Ink::HudDim, true));
    canvas(216.0, 152.0, s, None).fixed(216.0)
}

fn zone_ink(z: Zone) -> Ink {
    match z {
        Zone::Cold => Ink::Fg2,
        Zone::Low | Zone::High => Ink::Warn,
        Zone::Good => Ink::Good,
        Zone::Crit => Ink::Bad,
    }
}

fn power_gauge(r: Option<&ReactorNews>, max_kw: f64) -> Item {
    let (cx, cy, rr, sw) = (108.0, 70.0, 60.0, 14.0);
    let (out, load) = r.map_or((0.0, 0.0), |r| (r.output_kw, r.load_kw));
    let frac = (out / max_kw * 100.0).clamp(0.0, 100.0);
    let ink = power_ink(out, load);
    let mut s = vec![arc(cx, cy, rr, angle(0.0), angle(100.0), sw, Ink::Bg3, None)];
    if frac > 0.0 {
        s.push(arc(cx, cy, rr, angle(0.0), angle(frac), sw, ink, None));
    }
    let n = (load / max_kw * 100.0).clamp(0.0, 100.0);
    let (a, p0, p1) = (angle(n).to_radians(), 50.0, 66.0);
    s.push(Shape::Line { a: [cx + p0 * a.cos(), cy + p0 * a.sin()], b: [cx + p1 * a.cos(), cy + p1 * a.sin()], width: 4.0, ink: Ink::FgTitle, dash: None });
    s.push(text(0.0, 62.0, 216.0, 32.0, &format!("{out:.0}"), 28.0, TextFace::Mono, ink, true));
    s.push(text(0.0, 98.0, 216.0, 18.0, &format!("LOAD {load:.0}"), 16.0, TextFace::Mono, Ink::Fg2, true));
    s.push(text(0.0, 124.0, 216.0, 20.0, "POWER kW", 16.0, TextFace::Display, Ink::HudDim, true));
    canvas(216.0, 152.0, s, None).fixed(216.0)
}

pub fn power_ink(out: f64, load: f64) -> Ink {
    if out <= 0.0 {
        return Ink::Fg3;
    }
    if load <= 0.0 {
        return Ink::Good;
    }
    let short = (load - out) / load;
    if short <= 0.05 {
        Ink::Good
    } else if short <= 0.40 {
        Ink::Warn
    } else {
        Ink::Bad
    }
}

fn fill(r: Option<&ReactorNews>) -> Item {
    let tubes = r.map_or([CanisterSlot::default(); CANISTER_SLOTS], |r| r.canisters);
    let mut s = Vec::new();
    for (i, t) in tubes.iter().enumerate() {
        let x = 12.0 + 40.0 * i as f64;
        let (top, h) = (10.0, 100.0);
        let fill = t.canister.is_some().then_some(Ink::Bg3);
        s.push(Shape::Rect { x, y: top, w: 26.0, h, fill, outline: Some((Ink::HudDim, 2.0)) });
        let fill_h = h * (t.life / 100.0).clamp(0.0, 1.0);
        let fy = top + h - fill_h;
        if let Some(kind) = t.canister.filter(|_| fill_h > 0.0) {
            if t.life <= 10.0 {
                s.push(Shape::Hatch { x: x + 2.0, y: fy, w: 22.0, h: fill_h, ink: Ink::Bad });
            } else if t.life <= 25.0 {
                s.push(Shape::Hatch { x: x + 2.0, y: fy, w: 22.0, h: fill_h, ink: Ink::Warn });
            } else {
                s.push(Shape::Rect { x: x + 2.0, y: fy, w: 22.0, h: fill_h, fill: Some(tube_ink(kind)), outline: None });
            }
        }
        s.push(text(x - 7.0, 116.0, 40.0, 16.0, &format!("R{}", i + 1), 16.0, TextFace::Mono, Ink::Fg2, true));
    }
    s.push(text(0.0, 136.0, 176.0, 16.0, "FILL", 16.0, TextFace::Display, Ink::HudDim, true));
    canvas(176.0, 152.0, s, None).flex(1.0)
}

/// A canister kind's colour on the tube (the palette has no magenta: spicy takes the danger red,
/// TODO(initiator): a colour of its own in `display_core`).
fn tube_ink(kind: CanisterKind) -> Ink {
    match kind {
        CanisterKind::Standard => Ink::Good,
        CanisterKind::Hot => Ink::Warn,
        CanisterKind::Spicy => Ink::Bad,
    }
}

pub fn reactor_status(r: Option<&ReactorNews>) -> (&'static str, Tone) {
    match r {
        Some(r) if r.ejected => ("NO CORE", Tone::Dim),
        Some(r) if r.power_on => match zone(r.heat, r.optimal, r.allowed) {
            Zone::Crit => ("CRITICAL", Tone::Bad),
            Zone::High => ("HOT", Tone::Warn),
            _ => ("STABLE", Tone::Good),
        },
        _ => ("OFF", Tone::Dim),
    }
}

fn hazard(w: f64) -> Item {
    canvas(w, 14.0, vec![Shape::Rect { x: 0.0, y: 0.0, w, h: 14.0, fill: Some(Ink::Bg0), outline: None }, Shape::Hatch { x: 0.0, y: 0.0, w, h: 14.0, ink: Ink::Bad }], None).fixed(14.0)
}

/// until a repair brings it back (`Reactor::imploded`).
fn implosion_view() -> Panel {
    let word = canvas(624.0, 200.0, vec![text(0.0, 60.0, 624.0, 72.0, "IMPLOSION", 56.0, TextFace::DisplayBold, Ink::Bad, true)], None).fixed(200.0);
    Panel::screen(Tier::M, READOUT_NAME, "RX-01", column(8.0, vec![hazard(624.0), word, hazard(624.0)])).status("IMPLOSION", Tone::Bad).with_padding(8.0)
}

/// The slider track's width under the readout (the bottom-left column: 608 - 16 - 200), and the band
/// strip's height. The strip sits right under its slider, as wide as the track, so the 0..100 scale matches.
pub const STRIP_W: f64 = 392.0;

pub const STRIP_H: f64 = 6.0;

/// The wobble of the injection needle (Barotrauma's jitter, with a sine sum for its Perlin noise): within
/// 5 of the allowed top, the needle moves by up to half of the set point's distance over the top (at
/// most 10 / 2). Deterministic in `t` (the display time, seconds).
pub fn injection_wobble(r: &ReactorNews, t: f64) -> f64 {
    let top = r.injection_allowed[1];
    if r.injection <= top - 5.0 {
        return 0.0;
    }
    let amount = (r.set_injection - top).clamp(0.0, 10.0);
    amount * 0.5 * ((t * 9.0).sin() + (t * 13.7).sin()) * 0.5
}

/// the actual value as a marker. A band with `hi <= lo` (no news) is not drawn.
fn band_strip(optimal: [f64; 2], allowed: [f64; 2], actual: Option<f64>) -> Item {
    let at = |v: f64| v.clamp(0.0, 100.0) / 100.0 * STRIP_W;
    let span = |b: [f64; 2]| (at(b[0]), at(b[1]) - at(b[0]));
    let mut shapes = Vec::new();
    if allowed[1] > allowed[0] {
        let (x, w) = span(allowed);
        shapes.push(Shape::Rect { x, y: 0.0, w, h: 4.0, fill: Some(Ink::Warn), outline: None });
    }
    if optimal[1] > optimal[0] {
        let (x, w) = span(optimal);
        shapes.push(Shape::Rect { x, y: 0.0, w, h: 4.0, fill: Some(Ink::Good), outline: None });
    }
    if let Some(v) = actual {
        // Kept inside the strip's box at the two ends (a marker at 0 or 100 would hang over by a pixel).
        shapes.push(Shape::Rect { x: (at(v) - 1.0).clamp(0.0, STRIP_W - 2.0), y: 0.0, w: 2.0, h: STRIP_H, fill: Some(Ink::FgTitle), outline: None });
    }
    canvas(STRIP_W, STRIP_H, shapes, None).fixed(STRIP_H)
}

/// The arrow is ASCII: the mono font has no U+2192.
fn set_and_actual(set: f64, actual: f64, manual: bool) -> Item {
    row(
        6.0,
        vec![
            readout(format!("{set:.0} %")).text_size(font::LABEL).tone(if manual { Tone::Normal } else { Tone::Dim }).flex(1.0),
            label(format!("-> {actual:.0}")).mono().text_size(font::SMALL).tone(Tone::Dim).flex(1.0),
        ],
    )
}

/// The readout panel; `max_kw` is the scale of the power gauge (the reactor's `max_output_kw`).
pub fn readout_panel(r: Option<&ReactorNews>, max_kw: f64) -> Panel {
    readout_panel_at(r, max_kw, 0.0)
}

pub fn readout_panel_at(r: Option<&ReactorNews>, max_kw: f64, t: f64) -> Panel {
    if r.is_some_and(|r| r.imploded) {
        return implosion_view();
    }
    let on = r.is_some_and(|r| r.power_on);
    let manual = r.is_some_and(|r| r.manual);
    let (inj, conv) = r.map_or((0.0, 0.0), |r| (r.set_injection, r.set_converter));
    let (inj_act, conv_act) = r.map_or((0.0, 0.0), |r| (r.injection, r.converter));
    let (word, tone) = reactor_status(r);
    let zero = [0.0; 2];
    let (inj_opt, inj_all, conv_opt, conv_all) = r.map_or((zero, zero, zero, zero), |r| (r.injection_optimal, r.injection_allowed, r.converter_optimal, r.converter_allowed));
    let inj_needle = r.map(|r| (r.injection + injection_wobble(r, t)).clamp(0.0, 100.0));
    // Critical: the stripe is as wide as the bottom-left column (624 content - 16 gap - 200 switches)
    // and the spacing tightens by 18 px so the stripe fits (TODO(initiator): the readout's height budget).
    let critical = crate::effects::is_critical(r);
    // The two strips take 12 px of the column's 184: the gaps give it back (10 -> 7, and 6 -> 3 when critical).
    let (filler, gap) = if critical { (hazard(408.0), 3.0) } else { (space(), 7.0) };
    let track = |s: Item, strip: Item| column(0.0, vec![s, strip]).fixed(ctl::MD + STRIP_H);
    let bottom_left = column(
        gap,
        vec![
            row(8.0, vec![micro("INJECTION RATE").flex(1.0), set_and_actual(inj, inj_act, manual)]).fixed(24.0),
            track(slider(inj / 100.0, "", "inj_set").enabled(manual), band_strip(inj_opt, inj_all, inj_needle)),
            row(8.0, vec![micro("CONVERTER OUTPUT").flex(1.0), set_and_actual(conv, conv_act, manual)]).fixed(24.0),
            track(slider(conv / 100.0, "", "conv_set").enabled(manual), band_strip(conv_opt, conv_all, r.map(|r| r.converter))),
            filler,
        ],
    );
    let switches = column(
        8.0,
        vec![
            toggle(manual, "", "mode").labels("AUTO", "MAN").fixed(ctl::MD),
            toggle(on, "", "power").labels("OFF", "ON").fixed(ctl::MD),
            button("?", HELP_ACTION).fixed(ctl::LG),
        ],
    );
    let (optimal, allowed) = bands(r);
    let gauges = row(8.0, vec![heat_gauge(r.map_or(0.0, |r| r.heat), optimal, allowed), power_gauge(r, max_kw), fill(r)]).fixed(152.0);
    let body = column(if critical { 4.0 } else { 8.0 }, vec![gauges, row(16.0, vec![bottom_left.flex(1.0), switches.fixed(200.0)]).flex(1.0)]);
    Panel::screen(Tier::M, READOUT_NAME, "RX-01", body).status(word, tone).with_padding(8.0)
}

const CALLOUT_W: f64 = 256.0;

const CALLOUT_H: f64 = 44.0;

const CALLOUT_SIZE: f64 = 14.0;

const LEFT: f64 = 0.0;

const RIGHT: f64 = 352.0;

/// What a callout points at: a leaf of the readout by its place in the layout (the three gauges come
/// first), or a control by its action.
#[derive(Clone, Copy)]
enum Target {
    Leaf(usize),
    Action(&'static str),
}

const CALLOUTS: &[(f64, f64, [&str; 2], Target)] = &[
    (LEFT, 4.0, ["keep the needle in the green", "red too long: core goes bang"], Target::Leaf(0)),
    (RIGHT, 4.0, ["what you make", "vs. what the ship wants"], Target::Leaf(1)),
    (RIGHT, 104.0, ["canisters; pull one in a pinch", ""], Target::Leaf(2)),
    (LEFT, 152.0, ["how hard the atoms argue", ""], Target::Action("inj_set")),
    (LEFT, 252.0, ["how much steam makes power", "and how much makes cooling"], Target::Action("conv_set")),
    (RIGHT, 152.0, ["AUTO: the ship has the dial", "MAN: your hand, your fault"], Target::Action("mode")),
    (RIGHT, 200.0, ["the big on switch", "nothing turns without it"], Target::Action("power")),
];
const CLOSE_AT: (f64, f64) = (LEFT, 104.0);

fn dim_ink(i: Ink) -> Ink {
    match i {
        Ink::Hud | Ink::HudDim => Ink::HudDim,
        Ink::Good | Ink::GoodDim => Ink::GoodDim,
        Ink::Warn | Ink::WarnDim => Ink::WarnDim,
        Ink::Bad | Ink::BadDim => Ink::BadDim,
        Ink::FgTitle | Ink::Fg1 | Ink::Fg2 | Ink::Fg3 => Ink::Fg3,
        Ink::Line1 | Ink::Line2 => Ink::Line2,
        Ink::Bg0 | Ink::Bg1 | Ink::Bg3 => i,
    }
}

/// A shape moved by (dx, dy) and dimmed. A text box is cut at `right` (the content's width): the fill's
/// captions are drawn past its 160 px box, and the help view must keep them inside its picture.
fn shifted_dim(s: &Shape, dx: f64, dy: f64, right: f64) -> Shape {
    let mut t = s.clone();
    match &mut t {
        Shape::Arc { cx, cy, ink, .. } => {
            *cx += dx;
            *cy += dy;
            *ink = dim_ink(*ink);
        }
        Shape::Line { a, b, ink, .. } => {
            a[0] += dx;
            a[1] += dy;
            b[0] += dx;
            b[1] += dy;
            *ink = dim_ink(*ink);
        }
        Shape::Rect { x, y, fill, outline, .. } => {
            *x += dx;
            *y += dy;
            *fill = fill.map(dim_ink);
            *outline = outline.map(|(i, w)| (dim_ink(i), w));
        }
        Shape::Hatch { x, y, ink, .. } => {
            *x += dx;
            *y += dy;
            *ink = dim_ink(*ink);
        }
        Shape::Text { x, y, w, ink, .. } => {
            *x += dx;
            *y += dy;
            *w = w.min(right - *x).max(0.0);
            *ink = dim_ink(*ink);
        }
        Shape::Circle { cx, cy, fill, stroke, .. } => {
            *cx += dx;
            *cy += dy;
            *fill = fill.map(dim_ink);
            *stroke = stroke.map(|(i, w)| (dim_ink(i), w));
        }
        // The airlock panel's pictures are not in the help view's canvases: moved only.
        Shape::SpacedText { x, y, .. } | Shape::Vector { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        Shape::Pulse { cx, cy, .. } => {
            *cx += dx;
            *cy += dy;
        }
    }
    t
}

fn dimmed_leaf(w: &Widget, r: Rect, o: DVec2, right: f64, out: &mut Vec<Shape>) {
    let (x, y, bw, bh) = (r.min.x - o.x, r.min.y - o.y, r.size.x, r.size.y);
    let frame = Shape::Rect { x, y, w: bw, h: bh, fill: None, outline: Some((Ink::Line2, 2.0)) };
    match w {
        Widget::Canvas { shapes, .. } => out.extend(shapes.iter().map(|s| shifted_dim(s, x, y, right))),
        Widget::Label { text, size, mono, .. } => out.push(text_shape(x, y, bw, bh, text, *size, *mono, false)),
        Widget::Button { label, .. } => {
            out.push(frame);
            out.push(text_shape(x, y, bw, bh, label, font::LABEL.min(bh * 0.55), false, true));
        }
        Widget::Toggle { labels, .. } => {
            out.push(frame);
            out.push(text_shape(x, y, bw / 2.0, bh, &labels[0], font::SMALL, false, true));
            out.push(text_shape(x + bw / 2.0, y, bw / 2.0, bh, &labels[1], font::SMALL, false, true));
        }
        Widget::Slider { value, .. } => {
            out.push(frame);
            // The marker's stroke is 3 px wide: at the ends it stays 2 px inside the box.
            let at = x + (bw * value.clamp(0.0, 1.0)).clamp(2.0, bw - 2.0);
            out.push(Shape::Line { a: [at, y], b: [at, y + bh], width: 3.0, ink: Ink::Fg3, dash: None });
        }
        _ => {}
    }
}

fn text_shape(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, mono: bool, centred: bool) -> Shape {
    let face = if mono { TextFace::Mono } else { TextFace::Display };
    Shape::Text { x, y, w, h, text: s.to_string(), size, face, ink: Ink::Fg3, centred }
}

/// One callout: the pointer line from the box's centre to the target's centre (drawn first, so the box
/// covers its start), then the box with its two lines.
fn callout_shapes(x: f64, y: f64, lines: &[&str; 2], target: Rect, o: DVec2, out: &mut Vec<Shape>) {
    let from = DVec2::new(x + CALLOUT_W / 2.0, y + CALLOUT_H / 2.0);
    let to = target.centre() - o;
    out.push(Shape::Line { a: [from.x, from.y], b: [to.x, to.y], width: 2.0, ink: Ink::Hud, dash: None });
    out.push(Shape::Circle { cx: to.x, cy: to.y, r: 4.0, fill: Some(Ink::Hud), stroke: None });
    out.push(Shape::Rect { x, y, w: CALLOUT_W, h: CALLOUT_H, fill: Some(Ink::Bg0), outline: Some((Ink::Hud, 2.0)) });
    for (i, line) in lines.iter().enumerate() {
        let ly = y + 5.0 + i as f64 * 18.0;
        out.push(Shape::Text { x: x + 8.0, y: ly, w: CALLOUT_W - 16.0, h: 18.0, text: line.to_string(), size: CALLOUT_SIZE, face: TextFace::Display, ink: Ink::FgTitle, centred: false });
    }
}

/// a CLOSE box. The whole picture is the close button (`HELP_ACTION`), so a click anywhere closes it.
pub fn readout_help_panel(r: Option<&ReactorNews>, max_kw: f64) -> Panel {
    let base = readout_panel(r, max_kw);
    if r.is_some_and(|r| r.imploded) {
        return base;
    }
    let c = base.content();
    let o = c.min;
    let leaves = base.layout();
    let mut shapes = Vec::new();
    for w in &leaves {
        dimmed_leaf(w.widget, w.rect, o, c.size.x, &mut shapes);
    }
    for &(x, y, lines, target) in CALLOUTS {
        let rect = match target {
            Target::Leaf(i) => leaves[i].rect,
            Target::Action(a) => base.find(a).map(|p| p.rect).unwrap_or(c),
        };
        callout_shapes(x, y, &lines, rect, o, &mut shapes);
    }
    let (cx, cy) = CLOSE_AT;
    shapes.push(Shape::Rect { x: cx, y: cy, w: CALLOUT_W, h: CALLOUT_H, fill: Some(Ink::Bg0), outline: Some((Ink::Warn, 2.0)) });
    shapes.push(Shape::Text { x: cx, y: cy, w: CALLOUT_W, h: CALLOUT_H, text: "CLOSE".into(), size: font::LABEL, face: TextFace::DisplayBold, ink: Ink::Warn, centred: true });
    let mut panel = base;
    panel.root = canvas(c.size.x, c.size.y, shapes, Some(HELP_ACTION)).flex(1.0);
    panel
}

/// The pump's status word and its tone: OFF, LOW or HIGH by the mode, DRY with an empty tank (the
/// column is 60 px: "NO COOLANT" does not fit it).
pub fn pump_status(mode: PumpMode, coolant: f64) -> (&'static str, Tone) {
    match mode {
        PumpMode::Off => ("OFF", Tone::Dim),
        _ if coolant <= 0.0 => ("DRY", Tone::Bad),
        PumpMode::Low => ("LOW", Tone::Normal),
        PumpMode::High => ("HIGH", Tone::Good),
    }
}

fn pump_modes(mode: PumpMode, r: &ReactorNews) -> Item {
    let mode_button = |label: &str, action: &str, m: PumpMode| button(label, action).tone(if mode == m { Tone::Good } else { Tone::Normal });
    // Parked (#306 Parkplatz): the E-COOL button (`ecool_button`) was a fourth cell here, flex 1.4.
    let _ = r;
    row(6.0, vec![mode_button("OFF", "pump_off", PumpMode::Off).flex(1.0), mode_button("LOW", "pump_low", PumpMode::Low).flex(1.0), mode_button("HIGH", "pump_high", PumpMode::High).flex(1.0)]).fixed(ctl::MD)
}

/// when a press would be taken, the cooldown as m:ss greyed while it runs (TODO(initiator): the
/// label "E-COOL 0:42" does not fit a cell of this row), COOL greyed while the extra cooling runs.
#[allow(dead_code)] // parked (#306 Parkplatz)
pub fn ecool_button(r: &ReactorNews) -> Item {
    if r.ecool_active {
        return button("COOL", "ecool").tone(Tone::Good).enabled(false);
    }
    if r.ecool_ready_in_s > 0.0 {
        let s = r.ecool_ready_in_s.ceil() as u32;
        return button(format!("{}:{:02}", s / 60, s % 60), "ecool").enabled(false);
    }
    button("E-COOL", "ecool").enabled(r.ecool_ok)
}

fn pump_picture(flow_on: bool, angle: f64) -> Item {
    let fluid = if flow_on { Ink::Fg1 } else { Ink::Fg3 };
    let (cx, cy) = (76.0, 60.0);
    let mut s = vec![
        Shape::Rect { x: 0.0, y: 44.0, w: 44.0, h: 16.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
        Shape::Line { a: [1.5, 52.0], b: [44.0, 52.0], width: 3.0, ink: fluid, dash: Some((6.0, 4.0)) },
        Shape::Rect { x: 70.0, y: 0.0, w: 14.0, h: 22.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
        Shape::Rect { x: 70.0, y: 0.0, w: 82.0, h: 14.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
        Shape::Line { a: [76.0, 20.0], b: [76.0, 7.0], width: 3.0, ink: fluid, dash: Some((6.0, 4.0)) },
        Shape::Line { a: [76.0, 7.0], b: [150.5, 7.0], width: 3.0, ink: fluid, dash: Some((6.0, 4.0)) },
        Shape::Circle { cx, cy, r: 40.0, fill: Some(Ink::Bg1), stroke: Some((Ink::Hud, 3.0)) },
    ];
    for k in 0..6 {
        let a = (k as f64 * 60.0 + angle).to_radians();
        s.push(Shape::Line { a: [cx, cy], b: [cx + 30.0 * a.cos(), cy + 30.0 * a.sin()], width: 6.0, ink: Ink::Hud, dash: None });
    }
    s.push(Shape::Circle { cx, cy, r: 9.0, fill: Some(Ink::Hud), stroke: None });
    canvas(152.0, 109.0, s, None).fixed(152.0)
}

pub fn pump_panel(r: Option<&ReactorNews>) -> Panel {
    pump_panel_at(r, 0.0, true)
}

/// `blink_on` is the lamps' blink (`lamp_blink_on`): the LEAK word shows on its lit half.
pub fn pump_panel_at(r: Option<&ReactorNews>, angle: f64, blink_on: bool) -> Panel {
    if !powered(r) {
        return no_power(PUMP_NAME, "CP-01", Tier::S);
    }
    let r = r.copied().unwrap_or_default();
    let (word, tone) = pump_status(r.pump_mode, r.coolant);
    let body = column(
        8.0,
        vec![
            row(
                10.0,
                vec![
                    pump_picture(r.pump_on && r.flow > 0.0, angle),
                    column(4.0, vec![micro("FLOW").fixed(14.0), readout(format!("{:.0}", r.flow * 100.0)).text_size(font::LABEL).tone(tone).fixed(46.0), readout(word).text_size(font::LABEL).tone(tone)]).flex(1.0),
                    coolant_tube(&r, blink_on).fixed(60.0),
                ],
            )
            .fixed(109.0),
            pump_modes(r.pump_mode, &r),
            space(),
        ],
    );
    Panel::screen(Tier::S, PUMP_NAME, "CP-01", body).with_padding(6.0)
}

/// fill) and EMPTY at none (the word in bad). The sim's level and line (`ReactorNews::coolant`).
/// A leak blinks LEAK in the word's place (not over EMPTY).
fn coolant_tube(r: &ReactorNews, blink_on: bool) -> Item {
    let (x, top, w, h) = (14.0, 16.0, 32.0, 70.0);
    let mut s = vec![text(0.0, 0.0, 60.0, 14.0, "COOL", 14.0, TextFace::Display, Ink::HudDim, true), Shape::Rect { x, y: top, w, h, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    let fill_h = h * (r.coolant / 100.0).clamp(0.0, 1.0);
    if fill_h > 0.0 {
        let fy = top + h - fill_h;
        if r.coolant_low {
            s.push(Shape::Hatch { x: x + 2.0, y: fy, w: w - 4.0, h: fill_h, ink: Ink::Warn });
        } else {
            s.push(Shape::Rect { x: x + 2.0, y: fy, w: w - 4.0, h: fill_h, fill: Some(Ink::Good), outline: None });
        }
    }
    let (word, ink) = if r.coolant <= 0.0 {
        ("EMPTY", Ink::Bad)
    } else if r.coolant_leak > 0.0 && blink_on {
        ("LEAK", Ink::Warn)
    } else if r.coolant_low {
        ("LOW", Ink::Warn)
    } else {
        ("", Ink::Fg2)
    };
    if !word.is_empty() {
        s.push(text(0.0, 90.0, 60.0, 16.0, word, 16.0, TextFace::DisplayBold, ink, true));
    }
    canvas(60.0, 109.0, s, None)
}

/// A group's bar under its lever (94 wide, 18 high): the fill is `delivered / demand` (0 with no demand)
fn group_bar(delivered: f64, demand: f64, limit: f64) -> Item {
    let (w, top, h) = (94.0, 6.0, 12.0);
    let frac = if demand > 0.0 { (delivered / demand).clamp(0.0, 1.0) } else { 0.0 };
    let mut s = vec![Shape::Rect { x: 0.0, y: top, w, h, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if frac > 0.0 {
        s.push(Shape::Rect { x: 2.0, y: top + 2.0, w: (w - 4.0) * frac, h: h - 4.0, fill: Some(Ink::Hud), outline: None });
    }
    let cx = (limit.clamp(0.0, 1.0) * w).clamp(5.0, w - 5.0);
    // The sides start 1 px down: a 2 px stroke at the canvas's top edge would stick out of it.
    s.push(Shape::Line { a: [cx - 4.0, 1.0], b: [cx, top], width: 2.0, ink: Ink::FgTitle, dash: None });
    s.push(Shape::Line { a: [cx + 4.0, 1.0], b: [cx, top], width: 2.0, ink: Ink::FgTitle, dash: None });
    canvas(w, 18.0, s, None).fixed(18.0)
}

fn total_bar(total: f64, max_kw: f64) -> Item {
    let (w, bar_x, bar_w) = (292.0, 60.0, 140.0);
    let frac = if max_kw > 0.0 { (total / max_kw).clamp(0.0, 1.0) } else { 0.0 };
    let mut s = vec![
        text(0.0, 0.0, 56.0, 18.0, "TOTAL", 16.0, TextFace::Display, Ink::HudDim, false),
        Shape::Rect { x: bar_x, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
    ];
    if frac > 0.0 {
        s.push(Shape::Rect { x: bar_x + 2.0, y: 5.0, w: (bar_w - 4.0) * frac, h: 8.0, fill: Some(Ink::Hud), outline: None });
    }
    s.push(text(210.0, 0.0, 82.0, 18.0, &format!("{total:.0} kW"), 16.0, TextFace::Mono, Ink::Fg2, false));
    canvas(w, 18.0, s, None).fixed(18.0)
}

fn junction_lever(r: &ReactorNews, g: BreakerGroup, names: &[&str], action: &str, limit: &str) -> Item {
    let i = g.index();
    let on = r.breakers[i];
    let name_ink = if on { Ink::Fg1 } else { Ink::Fg3 };
    column(
        3.0,
        vec![
            lever_in(94.0, LEVER_PACKED, names, on, action, name_ink).fixed(44.0 + LEVER_PACKED.track),
            group_bar(r.delivered_kw[i], r.demand_kw[i], r.limits[i]),
            slider(r.limits[i], "", limit).fixed(20.0),
        ],
    )
    .fixed(94.0)
}

/// Parked (#307): the old JB-01 screen. The junction boxes have no screen since #322 (the toggles sit on the boxes) and this is
/// not placed; the central power screen is built from it.
pub fn junction_panel(r: Option<&ReactorNews>, max_kw: f64) -> Panel {
    if !powered(r) {
        return tall(no_power(JUNCTION_NAME, "JB-01", Tier::S));
    }
    let r = r.copied().unwrap_or_default();
    // Two rows of three (r39): the room's three groups, then the r40 groups. Each row is 112 px.
    let row_a = row(
        5.0,
        vec![
            junction_lever(&r, BreakerGroup::Engines, &["ENGINES"], "br_engines", "lim_engines"),
            junction_lever(&r, BreakerGroup::LifeSupport, &["LIFE", "SUPPORT"], "br_life", "lim_life"),
            junction_lever(&r, BreakerGroup::Lights, &["LIGHTS"], "br_lights", "lim_lights"),
        ],
    )
    .fixed(112.0);
    let row_b = row(
        5.0,
        vec![
            junction_lever(&r, BreakerGroup::Shields, &["SHIELDS"], "br_shields", "lim_shields"),
            junction_lever(&r, BreakerGroup::Weapons, &["WEAPONS"], "br_weapons", "lim_weapons"),
            junction_lever(&r, BreakerGroup::JumpDrive, &["JUMP", "DRIVE"], "br_jump", "lim_jump"),
        ],
    )
    .fixed(112.0);
    let levers = column(4.0, vec![row_a, row_b]).fixed(228.0);
    let total = r.delivered_kw.iter().sum::<f64>();
    let body = column(6.0, vec![levers, total_bar(total, max_kw)]);
    tall(Panel::screen(Tier::S, JUNCTION_NAME, "JB-01", body).with_padding(6.0))
}

/// BT-01 (r38, Barotrauma PowerContainer): the battery behind the reactor: its charge, the kW it
/// gives the grid now, and the recharge slider (the share of its max charge speed taken from the surplus).
pub fn battery_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    let bar_w = 200.0;
    let pct = format!("{:.0} %", r.battery_charge.clamp(0.0, 1.0) * 100.0);
    let mut s = vec![Shape::Rect { x: 0.0, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if r.battery_charge > 0.0 {
        s.push(Shape::Rect { x: 2.0, y: 5.0, w: (bar_w - 4.0) * r.battery_charge.clamp(0.0, 1.0), h: 8.0, fill: Some(Ink::Hud), outline: None });
    }
    s.push(text(210.0, 0.0, 82.0, 18.0, &pct, 16.0, TextFace::Mono, Ink::Fg2, false));
    let charge = column(4.0, vec![micro("CHARGE").fixed(14.0), canvas(292.0, 18.0, s, None).fixed(18.0)]);
    let out = readout(format!("OUTPUT {:.0} kW", r.battery_kw)).text_size(font::SMALL).tone(Tone::Normal);
    let rate = column(4.0, vec![micro("RECHARGE").fixed(14.0), slider(r.battery_recharge, "", "bat_rate").fixed(40.0)]);
    let body = column(0.0, vec![space().flex(1.0), column(10.0, vec![charge, out.fixed(20.0), rate]).fixed(120.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, BATTERY_NAME, "BT-01", body)).with_padding(6.0)
}

/// power: the aux reactor has its own supply. TODO(initiator): confirm.
pub fn aux_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    if !r.aux_on {
        let (w, h) = (292.0, 264.0);
        let shapes = vec![text(0.0, h * 0.5 - 10.0, w, 20.0, "AUX OFFLINE", 22.0, TextFace::DisplayBold, Ink::Fg3, true)];
        return tall(Panel::screen(Tier::S, AUX_NAME, "AX-01", canvas(w, h, shapes, None).flex(1.0)));
    }
    let name = Ink::Fg1;
    let switches = row(
        0.0,
        vec![lever_in(76.0, LEVER, &["LIFE", "SUPPORT"], r.aux_life_support, "aux_life", name), lever_in(76.0, LEVER, &["LIGHTS"], r.aux_lights, "aux_lights", name)],
    )
    .fixed(152.0);
    let boost = column(4.0, vec![micro("AUX OUTPUT").fixed(14.0), readout(format!("BOOST +{:.0} kW", r.aux_boost_kw)).text_size(font::SMALL).tone(Tone::Normal)]);
    let body = column(0.0, vec![space().flex(1.0), row(10.0, vec![switches, boost.flex(1.0)]).fixed(168.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, AUX_NAME, "AX-01", body)).with_padding(6.0)
}

pub fn band(heat: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Item {
    let x = |v: f64| v * 2.0;
    let seg = |a: f64, b: f64, ink: Option<Ink>, fill: Ink| {
        let mut v = vec![Shape::Rect { x: x(a), y: 7.0, w: x(b) - x(a), h: 8.0, fill: Some(fill), outline: None }];
        if let Some(i) = ink {
            v.push(Shape::Hatch { x: x(a), y: 7.0, w: x(b) - x(a), h: 8.0, ink: i });
        }
        v
    };
    let mut s = Vec::new();
    s.extend(seg(0.0, allowed[0], None, Ink::Line2));
    s.extend(seg(allowed[0], optimal[0], Some(Ink::Warn), Ink::Bg3));
    s.extend(seg(optimal[0], optimal[1], None, Ink::GoodDim));
    s.extend(seg(optimal[1], allowed[1], Some(Ink::Warn), Ink::Bg3));
    s.extend(seg(allowed[1], 100.0, Some(Ink::Bad), Ink::Bg3));
    let m = heat.clamp(0.0, 100.0);
    s.push(Shape::Rect { x: x(m) - 2.0, y: 0.0, w: 4.0, h: 22.0, fill: Some(Ink::FgTitle), outline: None });
    canvas(200.0, 22.0, s, None).fixed(200.0)
}

fn lamp_word(l: Lamp) -> (&'static str, Ink) {
    match l {
        Lamp::Hot => ("HOT", Ink::Warn),
        Lamp::CritHeat => ("CRIT HEAT", Ink::Bad),
        Lamp::Cold => ("COLD", Ink::Warn),
        Lamp::LowHeat => ("LOW HEAT", Ink::Warn),
        Lamp::HighOut => ("HIGH OUT", Ink::Warn),
        Lamp::CritOut => ("CRIT OUT", Ink::Bad),
        Lamp::LowOut => ("LOW OUT", Ink::Warn),
        Lamp::FillLow => ("FILL LOW", Ink::Warn),
        Lamp::NoFill => ("NO FILL", Ink::Bad),
        Lamp::Implosion => ("IMPLOSION", Ink::Bad),
        Lamp::Scram => ("SCRAM", Ink::Warn),
    }
}

fn lamp_cell(l: Lamp, lit: bool) -> Item {
    let (word, ink) = lamp_word(l);
    let shapes = if lit {
        vec![Shape::Rect { x: 0.0, y: 0.0, w: LAMP_W, h: LAMP_H, fill: Some(ink), outline: None }, text(0.0, 0.0, LAMP_W, LAMP_H, word, 14.0, TextFace::Display, Ink::Bg0, true)]
    } else {
        vec![Shape::Rect { x: 0.0, y: 0.0, w: LAMP_W, h: LAMP_H, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }, text(0.0, 0.0, LAMP_W, LAMP_H, word, 14.0, TextFace::Display, Ink::Fg3, true)]
    };
    canvas(LAMP_W, LAMP_H, shapes, None).flex(1.0)
}

/// The graph's cell (px) on the M panel (640 x 400): the width less the well and padding, and the
/// height the lamps, boost, presses, countdown and the stability bar leave (TODO(initiator): the status
/// board's budget: the bar took 25 px of it).
const GRAPH_W: f64 = 600.0;

const GRAPH_H: f64 = 125.0;

/// The graph's shapes (Barotrauma's `DrawGraph`, `Reactor.cs:543-575`): output in the good tone,
/// load in the neutral one, both on one scale, the larger of half the room's maximum and the
/// highest load shown (Barotrauma uses 10000 kW with a 5000 kW reactor). Newest at the right.
/// No curves (`graph_points` 0) draws the frame only.
pub fn graph_shapes(r: Option<&ReactorNews>, max_kw: f64, w: f64, h: f64) -> Vec<Shape> {
    let mut s = vec![Shape::Rect { x: 0.0, y: 0.0, w, h, fill: Some(Ink::Bg0), outline: Some((Ink::HudDim, 1.0)) }];
    let Some(r) = r else { return s };
    let n = (r.graph_points as usize).min(item_core::wire::GRAPH_POINTS);
    if n < 2 {
        return s;
    }
    let top = r.graph_load[..n].iter().copied().max().unwrap_or(0) as f64;
    let scale = (max_kw * 0.5).max(top).max(1.0);
    // Index 0 is the newest sample, at the right edge; the oldest is at the left.
    let point = |curve: &[u16; item_core::wire::GRAPH_POINTS], i: usize| -> [f64; 2] {
        let x = w * (1.0 - i as f64 / (n - 1) as f64);
        let y = h - (curve[i] as f64 / scale).clamp(0.0, 1.0) * (h - 2.0) - 1.0;
        [x, y]
    };
    for (curve, ink) in [(&r.graph_output, Ink::Good), (&r.graph_load, Ink::Fg2)] {
        for i in 0..n - 1 {
            s.push(Shape::Line { a: point(curve, i + 1), b: point(curve, i), width: 2.0, ink, dash: None });
        }
    }
    s
}

fn graph_item(r: Option<&ReactorNews>, max_kw: f64) -> Item {
    canvas(GRAPH_W, GRAPH_H, graph_shapes(r, max_kw, GRAPH_W, GRAPH_H), None).fixed(GRAPH_H)
}

const BAR_H: f64 = 8.0;

/// The stability bar's fill (0..1, the percent) and tone: good above 75 %, warn from 40 to 75 %, bad
/// below 40 % (PULSAR's core stability). Pure, for the tests.
pub fn stability_bar(pct: u8) -> (f64, Tone) {
    let tone = if pct > 75 {
        Tone::Good
    } else if pct >= 40 {
        Tone::Warn
    } else {
        Tone::Bad
    };
    (f64::from(pct.min(100)) / 100.0, tone)
}

/// status board's strip. No news yet: the percent is a dash and the bar is empty.
fn stability_item(r: Option<&ReactorNews>) -> Item {
    let (fill, tone, word) = match r {
        Some(r) => {
            let (fill, tone) = stability_bar(r.stability);
            (fill, tone, format!("{}%", r.stability.min(100)))
        }
        None => (0.0, Tone::Dim, "--".to_string()),
    };
    let ink = match tone {
        Tone::Good => Ink::Good,
        Tone::Warn => Ink::Warn,
        _ => Ink::Bad,
    };
    let head = row(8.0, vec![micro("STABILITY").flex(1.0), readout(word).text_size(font::SMALL).tone(tone)]).fixed(16.0);
    let bar = vec![
        Shape::Rect { x: 0.0, y: 0.0, w: GRAPH_W, h: BAR_H, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 1.0)) },
        Shape::Rect { x: 0.0, y: 0.0, w: GRAPH_W * fill, h: BAR_H, fill: Some(ink), outline: None },
    ];
    column(3.0, vec![head, canvas(GRAPH_W, BAR_H, bar, None).fixed(BAR_H)]).fixed(16.0 + 3.0 + BAR_H)
}

pub fn clock(s: f64) -> String {
    let s = s.max(0.0).ceil() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn status_panel(r: Option<&ReactorNews>, max_kw: f64, blink_on: bool) -> Panel {
    // No NO POWER here: like the readout, the panel has its own start-up supply (the module doc), so
    // EJECT works with the machines dark. The boost is refused without power, so its buttons are off.
    let r = r.copied().unwrap_or_default();
    let lit = |l: Lamp| r.lamps[l.index()] && (l == Lamp::Scram || blink_on);
    let mut grid = Vec::new();
    for chunk in LAMPS.chunks(3) {
        let mut cells: Vec<Item> = chunk.iter().map(|&l| lamp_cell(l, lit(l))).collect();
        while cells.len() < 3 {
            cells.push(space());
        }
        grid.push(row(LAMP_GAP, cells).fixed(LAMP_H));
    }
    let lamps = column(LAMP_GAP, grid).fixed(4.0 * LAMP_H + 3.0 * LAMP_GAP);
    let boost = row(8.0, vec![micro("BOOST").flex(1.0), readout(format!("{:+.0}", r.boost)).text_size(font::SMALL).tone(Tone::Normal), cover_word(&r)]).fixed(16.0);
    let countdown = match r.implosion_eta_s {
        Some(s) => readout(format!("IMPLOSION {}", clock(s))).text_size(font::LABEL).tone(Tone::Bad).fixed(22.0),
        None => space().fixed(22.0),
    };
    let presses = row(6.0, vec![button("UP", "boost_up").enabled(r.boost_ok).flex(1.0), button("DOWN", "boost_down").enabled(r.boost_ok).flex(1.0), eject_button(&r).flex(1.0)]).fixed(ctl::SM);
    let stability = stability_item(Some(&r));
    let graph = graph_item(Some(&r), max_kw);
    Panel::screen(Tier::M, STATUS_NAME, "RX-02", column(5.0, vec![lamps, boost, presses, countdown, stability, graph])).with_padding(6.0)
}

pub fn eject_button(r: &ReactorNews) -> Item {
    let press = if r.eject_armed { button("EJECT", "eject").tone(Tone::Bad) } else { button("ARM EJECT", "arm_eject") };
    press.enabled(!r.ejected)
}

pub fn cover_word(r: &ReactorNews) -> Item {
    let (word, tone) = if r.ejected {
        ("NO CORE", Tone::Dim)
    } else if r.eject_armed {
        ("COVER OPEN", Tone::Bad)
    } else {
        ("COVER SHUT", Tone::Dim)
    };
    readout(word).text_size(font::SMALL).tone(tone)
}

pub fn limit_group(action: &str) -> Option<BreakerGroup> {
    match action {
        "lim_engines" => Some(BreakerGroup::Engines),
        "lim_life" => Some(BreakerGroup::LifeSupport),
        "lim_lights" => Some(BreakerGroup::Lights),
        "lim_shields" => Some(BreakerGroup::Shields),
        "lim_weapons" => Some(BreakerGroup::Weapons),
        "lim_jump" => Some(BreakerGroup::JumpDrive),
        _ => None,
    }
}

/// The impeller's angle is kept in steps of 6 degrees, so the panel is only redrawn when it shows.
const IMPELLER_STEP: f64 = 6.0;

/// The ask of a press on the reactor's main screen. The sliders send the setting; the host takes it in MANUAL only
/// (`asks::apply`), and the panel has them disabled in AUTO, so no press arrives.
pub fn main_ask(action: &str, value: Option<f32>) -> Option<ReactorAsk> {
    use display_core::reactor_main as m;
    match action {
        m::AUTO => Some(ReactorAsk::SetManual(false)),
        m::MANUAL => Some(ReactorAsk::SetManual(true)),
        m::INJECTION => value.map(|v| ReactorAsk::SetInjection(v as f64 * 100.0)),
        m::CONVERTER => value.map(|v| ReactorAsk::SetConverter(v as f64 * 100.0)),
        _ => None,
    }
}

/// The presses on engineering's screens become the host's asks.
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>, mut help: ResMut<ReactorHelp>) {
    for c in clicks.read() {
        let snap = items.reactor;
        let new_state = |current: bool| c.value.map(|v| v >= 0.5).unwrap_or(!current);
        let ask = if c.panel == startup_screen::ID {
            startup_screen::ask(c, snap.as_ref())
        } else if c.panel == READOUT_ID {
            match c.action.as_str() {
                HELP_ACTION => {
                    help.toggle();
                    None
                }
                "mode" => snap.map(|r| ReactorAsk::SetManual(new_state(r.manual))),
                "power" => snap.map(|r| ReactorAsk::SetPower(new_state(r.power_on))),
                "inj_set" => c.value.map(|v| ReactorAsk::SetInjection(v as f64 * 100.0)),
                "conv_set" => c.value.map(|v| ReactorAsk::SetConverter(v as f64 * 100.0)),
                _ => None,
            }
        } else if c.panel == MAIN_ID {
            main_ask(&c.action, c.value)
        } else if c.panel == PUMP_ID {
            match c.action.as_str() {
                "pump_off" => Some(ReactorAsk::SetPumpMode(PumpMode::Off)),
                "pump_low" => Some(ReactorAsk::SetPumpMode(PumpMode::Low)),
                "pump_high" => Some(ReactorAsk::SetPumpMode(PumpMode::High)),
                "ecool" => Some(ReactorAsk::EmergencyCool),
                _ => None,
            }
        } else if c.panel == JUNCTION_ID && let Some(g) = limit_group(&c.action) {
            c.value.map(|v| ReactorAsk::SetLimit(g, v as f64))
        } else if c.panel == JUNCTION_ID {
            let group = match c.action.as_str() {
                "br_engines" => Some(BreakerGroup::Engines),
                "br_life" => Some(BreakerGroup::LifeSupport),
                "br_lights" => Some(BreakerGroup::Lights),
                "br_shields" => Some(BreakerGroup::Shields),
                "br_weapons" => Some(BreakerGroup::Weapons),
                "br_jump" => Some(BreakerGroup::JumpDrive),
                _ => None,
            };
            group.and_then(|g| snap.map(|r| ReactorAsk::SetBreaker(g, new_state(r.breakers[g.index()]))))
        } else if c.panel == AUX_ID {
            match c.action.as_str() {
                "aux_life" => snap.map(|r| ReactorAsk::SetAuxSwitch(BreakerGroup::LifeSupport, new_state(r.aux_life_support))),
                "aux_lights" => snap.map(|r| ReactorAsk::SetAuxSwitch(BreakerGroup::Lights, new_state(r.aux_lights))),
                _ => None,
            }
        } else if c.panel == BATTERY_ID {
            match c.action.as_str() {
                "bat_rate" => c.value.map(|v| ReactorAsk::SetBatteryRecharge(v as f64)),
                _ => None,
            }
        } else if c.panel == STATUS_ID {
            match c.action.as_str() {
                "boost_up" => Some(ReactorAsk::Boost(true)),
                "boost_down" => Some(ReactorAsk::Boost(false)),
                "arm_eject" => Some(ReactorAsk::ArmEject),
                "eject" => Some(ReactorAsk::Eject),
                _ => None,
            }
        } else {
            None
        };
        if let Some(a) = ask {
            items.ask_reactor(a);
        }
    }
}

/// Engineering's panels from the snapshot, every tick (`Panels::set` skips an unchanged one). The
/// impeller's angle is kept here; the readout's help closes when the player leaves the display.
#[allow(clippy::too_many_arguments)]
pub fn write_panels(items: Res<Items>, cfg: Res<ReactorConfig>, time: Res<Time>, active: Res<ActiveDisplay>, mut help: ResMut<ReactorHelp>, mut spin: Local<f64>, mut panels: ResMut<Panels>) {
    if active.using().is_none() {
        help.0 = false;
    }
    let r = items.reactor.as_ref();
    let speed = items.reactor.map_or(0.0, |r| r.pump_speed);
    *spin = crate::effects::impeller_angle(*spin, speed, time.delta_secs_f64());
    let angle = (*spin / IMPELLER_STEP).round() * IMPELLER_STEP;
    let max_kw = cfg.0.reactor.max_output_kw;
    let blink = lamp_blink_on(time.elapsed_secs_f64());
    // SP-01 is on whatever the master; the others show OFF while the master is off (`show`).
    panels.set(startup_screen::ID, startup_screen::panel(r));
    let readout = if help.0 { readout_help_panel(r, max_kw) } else { readout_panel_at(r, max_kw, time.elapsed_secs_f64()) };
    // The main screen draws its own dark and word faces (NO POWER, ...), so it does not go through `show`. TODO(#319): the run-up flag.
    panels.set(MAIN_ID, display_core::reactor_main::panel(r, max_kw, false));
    // The supply screen goes dark by the main screen's rule (`reactor_main::face`), inside its panel.
    panels.set(SUPPLY_ID, display_core::reactor_supply::panel(r));
    show(&mut panels, r, READOUT_ID, bot_tagged(readout, manned(&items, BotStation::Reactor)));
    show(&mut panels, r, PUMP_ID, bot_tagged(pump_panel_at(r, angle, blink), manned(&items, BotStation::Reactor)));
    show(&mut panels, r, AUX_ID, aux_panel(r));
    show(&mut panels, r, BATTERY_ID, battery_panel(r));
    show(&mut panels, r, STATUS_ID, status_panel(r, max_kw, blink));
}

#[cfg(test)]
mod tests {
    use item_core::wire::{CanisterKind, CanisterSlot};

    fn tube(life: f64) -> CanisterSlot {
        CanisterSlot { canister: Some(CanisterKind::Standard), life }
    }

    use super::*;

    fn shipped_panel(r: Option<&ReactorNews>) -> Panel {
        let shipped = reactor_core::Tuning::shipped();
        readout_panel(r, shipped.reactor.max_output_kw)
    }

    fn header(p: &Panel) -> Option<(String, Tone)> {
        p.plate.as_ref()?.status.clone()
    }

    fn news() -> ReactorNews {
        ReactorNews { powered: true, power_on: true, pump_on: true, pump_mode: PumpMode::High, pump_speed: 1.0, flow: 1.0, breakers: [true; 6], optimal: [35.0, 65.0], allowed: [20.0, 80.0], heat: 54.0, output_kw: 500.0, load_kw: 500.0, canisters: [tube(80.0), tube(12.0)], ..Default::default() }
    }

    fn status_news() -> ReactorNews {
        ReactorNews { lamps: [false; Lamp::COUNT], boost_ok: true, ..news() }
    }

    #[test]
    fn the_status_board_has_its_own_supply_so_eject_works_with_the_machines_dark() {
        let off = status_panel(Some(&ReactorNews { powered: false, boost_ok: false, ..status_news() }), 2000.0, true);
        for a in ["boost_up", "boost_down"] {
            assert!(matches!(off.find(a).unwrap().widget, display_core::Widget::Button { enabled: false, .. }), "{a}");
        }
        assert!(off.overflowing().is_empty());
        assert!(matches!(off.find("arm_eject").expect("ARM EJECT with no power").widget, display_core::Widget::Button { enabled: true, .. }));
        let dark = status_panel(Some(&ReactorNews { powered: false, eject_armed: true, ..status_news() }), 2000.0, true);
        assert!(dark.find("eject").is_some(), "EJECT with the cover open and no power");
        assert!(status_panel(None, 2000.0, true).find("arm_eject").is_some(), "no snapshot yet: the button still works");
    }

    /// The main screen's sliders move the sim's set points in MANUAL; AUTO ignores them (#317).
    #[test]
    fn the_main_screens_sliders_move_the_set_points_in_manual_and_auto_ignores_them() {
        use crate::dev_area::ship::{ReactorCounts, ReactorSim};
        let cfg = reactor_core::Tuning::default();
        let mut s = ReactorSim::new(&cfg);
        let mut counts = ReactorCounts::default();
        let mut press = |s: &mut ReactorSim, action: &str, v: Option<f32>| {
            if let Some(a) = main_ask(action, v) {
                crate::dev_area::engineering::asks::apply(s, a, &cfg, &mut counts);
            }
        };
        press(&mut s, display_core::reactor_main::MANUAL, None);
        assert!(s.manual);
        press(&mut s, display_core::reactor_main::INJECTION, Some(0.7));
        press(&mut s, display_core::reactor_main::CONVERTER, Some(0.25));
        assert!((s.injection - 70.0).abs() < 1e-4 && (s.converter - 25.0).abs() < 1e-4, "{} {}", s.injection, s.converter);
        press(&mut s, display_core::reactor_main::AUTO, None);
        assert!(!s.manual);
        press(&mut s, display_core::reactor_main::INJECTION, Some(0.1));
        press(&mut s, display_core::reactor_main::CONVERTER, Some(0.9));
        assert!((s.injection - 70.0).abs() < 1e-4 && (s.converter - 25.0).abs() < 1e-4, "AUTO ignores the sliders");
    }

    #[test]
    fn the_manual_sliders_are_only_on_when_manual() {
        let auto = shipped_panel(Some(&news()));
        assert!(matches!(auto.find("inj_set").unwrap().widget, display_core::Widget::Slider { enabled: false, .. }));
        let man = shipped_panel(Some(&ReactorNews { manual: true, ..news() }));
        assert!(matches!(man.find("inj_set").unwrap().widget, display_core::Widget::Slider { enabled: true, .. }));
        assert!(man.find("mode").is_some() && man.find("reset").is_none());
    }

    #[test]
    fn the_critical_readout_keeps_its_controls_and_says_critical() {
        let hot = ReactorNews { heat: 85.0, ..news() };
        assert!(crate::effects::is_critical(Some(&hot)));
        let crit = shipped_panel(Some(&hot));
        for a in ["power", "mode", "inj_set", "conv_set"] {
            assert!(crit.find(a).is_some(), "{a} while critical");
        }
        assert_eq!(header(&crit), Some(("CRITICAL".to_string(), Tone::Bad)));
        assert!(crit.overflowing().is_empty());
        let off = ReactorNews { power_on: false, ..hot };
        assert!(!crate::effects::is_critical(Some(&off)));
        let normal = shipped_panel(Some(&off));
        assert!(normal.find("inj_set").is_some(), "powered off: the normal view");
        assert_eq!(reactor_status(Some(&off)), ("OFF", Tone::Dim));
        assert_eq!(header(&normal), Some(("OFF".to_string(), Tone::Dim)));
    }

    #[test]
    fn critical_is_the_same_at_every_heat_on_the_readout_and_the_lamp() {
        for (allowed, heat) in [([20.0, 80.0], 85.0), ([10.0, 90.0], 85.0), ([10.0, 90.0], 95.0)] {
            let r = ReactorNews { allowed, heat, ..news() };
            let p = shipped_panel(Some(&r));
            let critical = crate::effects::is_critical(Some(&r));
            assert_eq!(header(&p).is_some_and(|h| h.0 == "CRITICAL"), critical, "allowed {allowed:?}, heat {heat}");
            assert_eq!(reactor_status(Some(&r)).0 == "CRITICAL", critical, "word at {heat} with {allowed:?}");
            assert!(p.find("inj_set").is_some() && p.find("power").is_some(), "the controls stay at {heat}");
        }
    }

    #[test]
    fn a_imploded_core_says_implosion_without_a_button() {
        let p = shipped_panel(Some(&ReactorNews { imploded: true, ..news() }));
        assert!(p.find("power").is_none() && p.find("inj_set").is_none());
        assert_eq!(header(&p), Some(("IMPLOSION".to_string(), Tone::Bad)));
        assert!(p.overflowing().is_empty());
    }
}

