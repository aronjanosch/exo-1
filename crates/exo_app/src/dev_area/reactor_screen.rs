//!   (TODO(initiator): confirm).
//! TODO(initiator): the glow of the critical text and the scanlines' look are not drawn yet.
use super::*;
use crate::displays::{ActiveDisplay, DisplayAction, Panels};
use crate::dev_area::reactor::{lamp_blink_on, ReactorConfig};
use crate::items::Items;
use display_core::{button, canvas, column, ctl, font, label, micro, readout, row, slider, space, toggle, Ink, Item, Panel, Rect, Shape, Tier, TextFace, Tone, Widget};
use item_core::wire::{BreakerGroup, FuelKind, FuelTube, Lamp, PumpMode, ReactorAsk, ReactorFault, ReactorNews, FUEL_TUBES};

const LAMPS: [Lamp; Lamp::COUNT] = [Lamp::Hot, Lamp::CritHeat, Lamp::Cold, Lamp::LowTemp, Lamp::HighOut, Lamp::CritOut, Lamp::LowOut, Lamp::FuelLow, Lamp::NoFuel, Lamp::Meltdown, Lamp::Scram];
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
pub const AUX_NAME: &str = "AUX";
pub const DOOR_ID: PanelId = PanelId::new("engine_door_screen");
pub const DOOR_NAME: &str = "REACTOR ROOM";
pub const FAULTS_ID: PanelId = PanelId::new("reactor_faults");
pub const FAULTS_NAME: &str = "Dev faults";
pub const STATUS_ID: PanelId = PanelId::new("reactor_status");
pub const STATUS_NAME: &str = "RX STATUS";
/// The dev faults screen's mount point (area metres, the engine room's north wall, the room side).
pub const FAULTS_AT: [f64; 2] = [-17.5, -11.04];

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

pub fn plugin(app: &mut App) {
    app.init_resource::<ReactorHelp>();
    app.add_systems(FixedUpdate, (read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
}

// ---------- the gauge's geometry ----------

/// The arc's start (bottom left) and its sweep (270 degrees, clockwise on screen).
const ARC_START: f64 = 135.0;
const ARC_SWEEP: f64 = 270.0;

/// The angle of a value 0..100 on the arc.
fn angle(v: f64) -> f64 {
    ARC_START + ARC_SWEEP * v.clamp(0.0, 100.0) / 100.0
}

/// The temperature zones of the sim's bands: `Good` is the optimal band, `Low` and `High` the
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

fn temp_gauge(temp: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Item {
    let (cx, cy, r, sw) = (108.0, 70.0, 60.0, 14.0);
    let z = zone(temp, optimal, allowed);
    let t = temp.clamp(0.0, 100.0);
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
    s.push(text(0.0, 74.0, 216.0, 28.0, &format!("{temp:.0} °C"), 28.0, TextFace::Mono, zone_ink(z), true));
    s.push(text(0.0, 124.0, 216.0, 20.0, "CORE TEMP", 16.0, TextFace::Display, Ink::HudDim, true));
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

fn fuel(r: Option<&ReactorNews>) -> Item {
    let tubes = r.map_or([FuelTube::default(); FUEL_TUBES], |r| r.fuel);
    let mut s = Vec::new();
    for (i, t) in tubes.iter().enumerate() {
        let x = 12.0 + 40.0 * i as f64;
        let (top, h) = (10.0, 100.0);
        let fill = t.rod.is_some().then_some(Ink::Bg3);
        s.push(Shape::Rect { x, y: top, w: 26.0, h, fill, outline: Some((Ink::HudDim, 2.0)) });
        let fill_h = h * (t.life / 100.0).clamp(0.0, 1.0);
        let fy = top + h - fill_h;
        if let Some(kind) = t.rod.filter(|_| fill_h > 0.0) {
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
    s.push(text(0.0, 136.0, 176.0, 16.0, "FUEL", 16.0, TextFace::Display, Ink::HudDim, true));
    canvas(176.0, 152.0, s, None).flex(1.0)
}

/// A rod kind's colour on the tube (the palette has no magenta: spicy takes the danger red,
/// TODO(initiator): a colour of its own in `display_core`).
fn tube_ink(kind: FuelKind) -> Ink {
    match kind {
        FuelKind::Standard => Ink::Good,
        FuelKind::Hot => Ink::Warn,
        FuelKind::Spicy => Ink::Bad,
    }
}

fn text(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, ink: Ink, centred: bool) -> Shape {
    Shape::Text { x, y, w, h, text: s.to_string(), size, face, ink, centred }
}

pub fn reactor_status(r: Option<&ReactorNews>) -> (&'static str, Tone) {
    match r {
        Some(r) if r.ejected => ("NO CORE", Tone::Dim),
        Some(r) if r.power_on => match zone(r.temp, r.optimal, r.allowed) {
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

/// until a repair brings it back (`Reactor::melted`).
fn meltdown_view() -> Panel {
    let word = canvas(624.0, 200.0, vec![text(0.0, 60.0, 624.0, 72.0, "MELTDOWN", 56.0, TextFace::DisplayBold, Ink::Bad, true)], None).fixed(200.0);
    Panel::screen(Tier::M, READOUT_NAME, "RX-01", column(8.0, vec![hazard(624.0), word, hazard(624.0)])).status("MELTDOWN", Tone::Bad).with_padding(8.0)
}

/// The slider track's width under the readout (the bottom-left column: 608 - 16 - 200), and the band
/// strip's height. The strip sits right under its slider, as wide as the track, so the 0..100 scale matches.
pub const STRIP_W: f64 = 392.0;
pub const STRIP_H: f64 = 6.0;

/// The wobble of the fission needle (Barotrauma's jitter, with a sine sum for its Perlin noise): within
/// 5 of the allowed top, the needle moves by up to half of the set point's distance over the top (at
/// most 10 / 2). Deterministic in `t` (the display time, seconds).
pub fn fission_wobble(r: &ReactorNews, t: f64) -> f64 {
    let top = r.fission_allowed[1];
    if r.fission <= top - 5.0 {
        return 0.0;
    }
    let amount = (r.set_fission - top).clamp(0.0, 10.0);
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
    if r.is_some_and(|r| r.melted) {
        return meltdown_view();
    }
    let on = r.is_some_and(|r| r.power_on);
    let manual = r.is_some_and(|r| r.manual);
    let (fis, tur) = r.map_or((0.0, 0.0), |r| (r.set_fission, r.set_turbine));
    let (fis_act, tur_act) = r.map_or((0.0, 0.0), |r| (r.fission, r.turbine));
    let (word, tone) = reactor_status(r);
    let zero = [0.0; 2];
    let (fis_opt, fis_all, tur_opt, tur_all) = r.map_or((zero, zero, zero, zero), |r| (r.fission_optimal, r.fission_allowed, r.turbine_optimal, r.turbine_allowed));
    let fis_needle = r.map(|r| (r.fission + fission_wobble(r, t)).clamp(0.0, 100.0));
    // Critical: the stripe is as wide as the bottom-left column (624 content - 16 gap - 200 switches)
    // and the spacing tightens by 18 px so the stripe fits (TODO(initiator): the readout's height budget).
    let critical = crate::effects::is_critical(r);
    // The two strips take 12 px of the column's 184: the gaps give it back (10 -> 7, and 6 -> 3 when critical).
    let (filler, gap) = if critical { (hazard(408.0), 3.0) } else { (space(), 7.0) };
    let track = |s: Item, strip: Item| column(0.0, vec![s, strip]).fixed(ctl::MD + STRIP_H);
    let bottom_left = column(
        gap,
        vec![
            row(8.0, vec![micro("FISSION RATE").flex(1.0), set_and_actual(fis, fis_act, manual)]).fixed(24.0),
            track(slider(fis / 100.0, "", "fis_set").enabled(manual), band_strip(fis_opt, fis_all, fis_needle)),
            row(8.0, vec![micro("TURBINE OUTPUT").flex(1.0), set_and_actual(tur, tur_act, manual)]).fixed(24.0),
            track(slider(tur / 100.0, "", "tur_set").enabled(manual), band_strip(tur_opt, tur_all, r.map(|r| r.turbine))),
            filler,
        ],
    );
    let switches = column(
        8.0,
        vec![
            toggle(manual, "", "mode").labels("AUTO", "MAN").fixed(ctl::MD),
            toggle(on, "", "power").labels("OFF", "ON").fixed(ctl::MD),
            // The help button sits beside SCRAM: the header plate has no room for buttons.
            row(8.0, vec![button("SCRAM", "scram").tone(Tone::Bad).flex(1.0), button("?", HELP_ACTION).fixed(ctl::SM)]).fixed(ctl::LG),
        ],
    );
    let (optimal, allowed) = bands(r);
    let gauges = row(8.0, vec![temp_gauge(r.map_or(0.0, |r| r.temp), optimal, allowed), power_gauge(r, max_kw), fuel(r)]).fixed(152.0);
    let body = column(if critical { 4.0 } else { 8.0 }, vec![gauges, row(16.0, vec![bottom_left.flex(1.0), switches.fixed(200.0)]).flex(1.0)]);
    Panel::screen(Tier::M, READOUT_NAME, "RX-01", body).status(word, tone).with_padding(8.0)
}

// ---------- the help view ----------

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

const CALLOUTS: [(f64, f64, [&str; 2], Target); 8] = [
    (LEFT, 4.0, ["keep the needle in the green", "red too long: core goes bang"], Target::Leaf(0)),
    (RIGHT, 4.0, ["what you make", "vs. what the ship wants"], Target::Leaf(1)),
    (RIGHT, 104.0, ["rods; pull one in a pinch", ""], Target::Leaf(2)),
    (LEFT, 152.0, ["how hard the atoms argue", ""], Target::Action("fis_set")),
    (LEFT, 252.0, ["how much steam makes power", "and how much makes cooling"], Target::Action("tur_set")),
    (RIGHT, 152.0, ["AUTO: the ship has the dial", "MAN: your hand, your fault"], Target::Action("mode")),
    (RIGHT, 200.0, ["the big on switch", "nothing turns without it"], Target::Action("power")),
    (RIGHT, 292.0, ["the big red no", ""], Target::Action("scram")),
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

/// A shape moved by (dx, dy) and dimmed. A text box is cut at `right` (the content's width): the fuel's
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
    if r.is_some_and(|r| r.melted) {
        return base;
    }
    let c = base.content();
    let o = c.min;
    let leaves = base.layout();
    let mut shapes = Vec::new();
    for w in &leaves {
        dimmed_leaf(w.widget, w.rect, o, c.size.x, &mut shapes);
    }
    for (x, y, lines, target) in CALLOUTS {
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

pub fn no_power(title: &str, tag: &str, tier: Tier) -> Panel {
    let size = tier.canvas();
    let (w, h) = (size.x - 40.0, size.y - 80.0);
    // A ring above the word (drawn, not a glyph: the display font has no ○).
    let ring = Shape::Circle { cx: w * 0.5, cy: h * 0.5 - 36.0, r: 14.0, fill: None, stroke: Some((Ink::Fg3, 3.0)) };
    let shapes = vec![Shape::Rect { x: 0.0, y: 0.0, w, h, fill: Some(Ink::Bg0), outline: None }, ring, text(0.0, 12.0, w, h - 12.0, "NO POWER", 28.0, TextFace::DisplayBold, Ink::Fg3, true)];
    Panel::screen(tier, title, tag, canvas(w, h, shapes, None).flex(1.0))
}

fn powered(r: Option<&ReactorNews>) -> bool {
    r.is_some_and(|r| r.powered)
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
    // E-COOL is the fourth cell: the panel has no room for a row of its own (see `ecool_button`).
    row(6.0, vec![mode_button("OFF", "pump_off", PumpMode::Off).flex(1.0), mode_button("LOW", "pump_low", PumpMode::Low).flex(1.0), mode_button("HIGH", "pump_high", PumpMode::High).flex(1.0), ecool_button(r).flex(1.4)]).fixed(ctl::MD)
}

/// when a press would be taken, the cooldown as m:ss greyed while it runs (TODO(initiator): the
/// label "E-COOL 0:42" does not fit a cell of this row), COOL greyed while the extra cooling runs.
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
    pump_panel_at(r, 0.0)
}

pub fn pump_panel_at(r: Option<&ReactorNews>, angle: f64) -> Panel {
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
                    coolant_tube(&r).fixed(60.0),
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
fn coolant_tube(r: &ReactorNews) -> Item {
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

fn lever(lines: &[&str], on: bool, action: &str) -> Item {
    lever_in(94.0, lines, on, action, if on { Ink::Fg1 } else { Ink::Fg3 })
}

fn lever_in(w: f64, lines: &[&str], on: bool, action: &str, name_ink: Ink) -> Item {
    // The names are set in lines of 17 px, bottom-aligned on the slot's top (y 36): a name too wide
    // for the lever is split over two lines rather than cut.
    let cx = w / 2.0;
    let mut s: Vec<Shape> = lines.iter().enumerate().map(|(i, l)| text(0.0, 36.0 - 17.0 * (lines.len() - i) as f64, w, 17.0, l, 16.0, TextFace::Display, name_ink, true)).collect();
    s.push(Shape::Rect { x: cx - 11.0, y: 36.0, w: 22.0, h: 124.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) });
    let hy = if on { 36.0 } else { 126.0 };
    let ink = if on { Ink::Hud } else { Ink::Warn };
    s.push(Shape::Rect { x: cx - 24.0, y: hy, w: 48.0, h: 34.0, fill: Some(ink), outline: Some((Ink::FgTitle, 2.0)) });
    s.push(text(cx - 24.0, hy, 48.0, 34.0, if on { "ON" } else { "OFF" }, 16.0, TextFace::DisplayBold, Ink::Bg0, true));
    canvas(w, 168.0, s, Some(action)).flex(1.0)
}

pub const TALL_CANVAS: f64 = 320.0;

fn tall(mut p: Panel) -> Panel {
    p.size.y = TALL_CANVAS;
    p
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
    column(
        4.0,
        vec![
            lever(names, r.breakers[i], action).fixed(168.0),
            group_bar(r.delivered_kw[i], r.demand_kw[i], r.limits[i]),
            slider(r.limits[i], "", limit).fixed(40.0),
        ],
    )
    .fixed(94.0)
}

pub fn junction_panel(r: Option<&ReactorNews>, max_kw: f64) -> Panel {
    if !powered(r) {
        return tall(no_power(JUNCTION_NAME, "JB-01", Tier::S));
    }
    let r = r.copied().unwrap_or_default();
    let levers = row(
        5.0,
        vec![
            junction_lever(&r, BreakerGroup::Engines, &["ENGINES"], "br_engines", "lim_engines"),
            junction_lever(&r, BreakerGroup::LifeSupport, &["LIFE", "SUPPORT"], "br_life", "lim_life"),
            junction_lever(&r, BreakerGroup::Lights, &["LIGHTS"], "br_lights", "lim_lights"),
        ],
    )
    .fixed(234.0);
    let total = r.delivered_kw.iter().sum::<f64>();
    let body = column(8.0, vec![levers, total_bar(total, max_kw)]);
    tall(Panel::screen(Tier::S, JUNCTION_NAME, "JB-01", body).with_padding(6.0))
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
        vec![lever_in(76.0, &["LIFE", "SUPPORT"], r.aux_life_support, "aux_life", name), lever_in(76.0, &["LIGHTS"], r.aux_lights, "aux_lights", name)],
    )
    .fixed(152.0);
    let boost = column(4.0, vec![micro("AUX OUTPUT").fixed(14.0), readout(format!("BOOST +{:.0} kW", r.aux_boost_kw)).text_size(font::SMALL).tone(Tone::Normal)]);
    let body = column(0.0, vec![space().flex(1.0), row(10.0, vec![switches, boost.flex(1.0)]).fixed(168.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, AUX_NAME, "AX-01", body)).with_padding(6.0)
}

pub fn oxygen_tone(o: f64) -> Tone {
    let p = o * 100.0;
    if p <= 40.0 {
        Tone::Bad
    } else if p <= 70.0 {
        Tone::Warn
    } else {
        Tone::Good
    }
}

fn band(temp: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Item {
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
    let m = temp.clamp(0.0, 100.0);
    s.push(Shape::Rect { x: x(m) - 2.0, y: 0.0, w: 4.0, h: 22.0, fill: Some(Ink::FgTitle), outline: None });
    canvas(200.0, 22.0, s, None).fixed(200.0)
}

pub fn door_panel(r: Option<&ReactorNews>) -> Panel {
    if !powered(r) {
        return no_power(DOOR_NAME, "ER-01", Tier::S);
    }
    let r = r.copied().unwrap_or_default();
    let (optimal, allowed) = bands(Some(&r));
    let z = zone_tone(zone(r.temp, optimal, allowed));
    let (word, word_tone) = reactor_status(Some(&r));
    let temp = row(6.0, vec![readout(format!("{:.0}", r.temp)).text_size(72.0).tone(z), readout("°C").text_size(28.0).tone(z)]).fixed(70.0);
    let bandrow = row(8.0, vec![band(r.temp, optimal, allowed), readout(word).text_size(font::SMALL).tone(word_tone).flex(1.0)]).fixed(18.0);
    let air = row(
        8.0,
        vec![
            column(0.0, vec![micro("O₂").fixed(14.0), readout(format!("{:.0}%", r.oxygen * 100.0)).text_size(font::VALUE).tone(oxygen_tone(r.oxygen))]).flex(1.0),
            column(0.0, vec![micro("LIGHT").fixed(14.0), toggle(r.breakers[BreakerGroup::Lights.index()], "", "br_lights").labels("OFF", "ON").fixed(48.0)]).fixed(112.0),
        ],
    )
    .fixed(62.0);
    let body = column(4.0, vec![micro("REACTOR").fixed(14.0), temp, bandrow, air, space()]);
    Panel::screen(Tier::S, DOOR_NAME, "ER-01", body).with_padding(8.0)
}

pub fn faults_panel(r: Option<&ReactorNews>) -> Panel {
    let aux = if r.is_some_and(|r| r.aux_on) { "AUX ON" } else { "AUX OFF" };
    let col = |items: Vec<_>| column(4.0, items);
    Panel::screen(
        Tier::S,
        FAULTS_NAME,
        "DEV-02",
        row(
            6.0,
            vec![
                col(vec![
                    button("Pump break", "pump_break").fixed(28.0),
                    button("Load drop", "load_drop").fixed(28.0),
                    button("Surge", "surge").fixed(28.0),
                    button("Drain tank", "drain_coolant").fixed(28.0),
                    space(),
                ]),
                col(vec![
                    button("Start fire", "fire").fixed(28.0),
                    button("Fire mode", "fire_model").fixed(28.0),
                    button("Warm start", "warm_start").fixed(28.0),
                    button(aux, "aux").fixed(28.0),
                    button("Reset RX", "reset").fixed(28.0),
                ]),
            ],
        ),
    )
}

fn lamp_word(l: Lamp) -> (&'static str, Ink) {
    match l {
        Lamp::Hot => ("HOT", Ink::Warn),
        Lamp::CritHeat => ("CRIT HEAT", Ink::Bad),
        Lamp::Cold => ("COLD", Ink::Warn),
        Lamp::LowTemp => ("LOW TEMP", Ink::Warn),
        Lamp::HighOut => ("HIGH OUT", Ink::Warn),
        Lamp::CritOut => ("CRIT OUT", Ink::Bad),
        Lamp::LowOut => ("LOW OUT", Ink::Warn),
        Lamp::FuelLow => ("FUEL LOW", Ink::Warn),
        Lamp::NoFuel => ("NO FUEL", Ink::Bad),
        Lamp::Meltdown => ("MELTDOWN", Ink::Bad),
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
    let countdown = match r.meltdown_eta_s {
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
        _ => None,
    }
}

pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>, mut help: ResMut<ReactorHelp>) {
    for c in clicks.read() {
        let snap = items.reactor;
        let new_state = |current: bool| c.value.map(|v| v >= 0.5).unwrap_or(!current);
        let ask = if c.panel == READOUT_ID {
            match c.action.as_str() {
                HELP_ACTION => {
                    help.toggle();
                    None
                }
                "mode" => snap.map(|r| ReactorAsk::SetManual(new_state(r.manual))),
                "power" => snap.map(|r| ReactorAsk::SetPower(new_state(r.power_on))),
                "scram" => Some(ReactorAsk::Scram),
                "fis_set" => c.value.map(|v| ReactorAsk::SetFission(v as f64 * 100.0)),
                "tur_set" => c.value.map(|v| ReactorAsk::SetTurbine(v as f64 * 100.0)),
                _ => None,
            }
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
        } else if c.panel == JUNCTION_ID || c.panel == DOOR_ID {
            let group = match c.action.as_str() {
                "br_engines" => Some(BreakerGroup::Engines),
                "br_life" => Some(BreakerGroup::LifeSupport),
                "br_lights" => Some(BreakerGroup::Lights),
                _ => None,
            };
            group.and_then(|g| snap.map(|r| ReactorAsk::SetBreaker(g, new_state(r.breakers[g.index()]))))
        } else if c.panel == AUX_ID {
            match c.action.as_str() {
                "aux_life" => snap.map(|r| ReactorAsk::SetAuxSwitch(BreakerGroup::LifeSupport, new_state(r.aux_life_support))),
                "aux_lights" => snap.map(|r| ReactorAsk::SetAuxSwitch(BreakerGroup::Lights, new_state(r.aux_lights))),
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
        } else if c.panel == FAULTS_ID {
            match c.action.as_str() {
                "pump_break" => Some(ReactorAsk::Trigger(ReactorFault::PumpBreak)),
                "load_drop" => Some(ReactorAsk::Trigger(ReactorFault::LoadDrop)),
                "surge" => Some(ReactorAsk::Trigger(ReactorFault::Surge)),
                "fire" => Some(ReactorAsk::Trigger(ReactorFault::Fire)),
                "reset" => Some(ReactorAsk::Reset),
                "drain_coolant" => Some(ReactorAsk::Trigger(ReactorFault::DrainCoolant)),
                "warm_start" => Some(ReactorAsk::Trigger(ReactorFault::WarmStart)),
                "aux" => Some(ReactorAsk::Trigger(ReactorFault::Aux)),
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

/// The impeller's angle is kept in steps of 6 degrees, so the panel is only redrawn when it shows.
const IMPELLER_STEP: f64 = 6.0;

/// Every panel from the state, every tick (`Panels::set` skips an unchanged one). The impeller
pub fn write_panels(items: Res<Items>, cfg: Res<ReactorConfig>, time: Res<Time>, active: Res<ActiveDisplay>, mut help: ResMut<ReactorHelp>, mut spin: Local<f64>, mut panels: ResMut<Panels>) {
    if active.using().is_none() {
        help.0 = false;
    }
    let speed = items.reactor.map_or(0.0, |r| r.pump_speed);
    *spin = crate::effects::impeller_angle(*spin, speed, time.delta_secs_f64());
    let angle = (*spin / IMPELLER_STEP).round() * IMPELLER_STEP;
    let max_kw = cfg.0.reactor.max_output_kw;
    let readout = if help.0 { readout_help_panel(items.reactor.as_ref(), max_kw) } else { readout_panel_at(items.reactor.as_ref(), max_kw, time.elapsed_secs_f64()) };
    panels.set(READOUT_ID, readout);
    panels.set(PUMP_ID, pump_panel_at(items.reactor.as_ref(), angle));
    panels.set(JUNCTION_ID, junction_panel(items.reactor.as_ref(), max_kw));
    panels.set(AUX_ID, aux_panel(items.reactor.as_ref()));
    panels.set(DOOR_ID, door_panel(items.reactor.as_ref()));
    panels.set(FAULTS_ID, faults_panel(items.reactor.as_ref()));
    panels.set(STATUS_ID, status_panel(items.reactor.as_ref(), cfg.0.reactor.max_output_kw, lamp_blink_on(time.elapsed_secs_f64())));
}

#[cfg(test)]
mod tests {
    use item_core::wire::{FuelKind, FuelTube};

    fn tube(life: f64) -> FuelTube {
        FuelTube { rod: Some(FuelKind::Standard), life }
    }

    use super::*;

    fn shipped_panel(r: Option<&ReactorNews>) -> Panel {
        let shipped = reactor_core::Tuning::parse(include_str!("../../../../content/gameplay/reactor/reactor.json")).expect("the shipped reactor.json");
        readout_panel(r, shipped.reactor.max_output_kw)
    }

    fn header(p: &Panel) -> Option<(String, Tone)> {
        p.plate.as_ref()?.status.clone()
    }

    fn news() -> ReactorNews {
        ReactorNews { powered: true, power_on: true, pump_on: true, pump_mode: PumpMode::High, pump_speed: 1.0, flow: 1.0, breakers: [true; 3], optimal: [35.0, 65.0], allowed: [20.0, 80.0], temp: 54.0, output_kw: 500.0, load_kw: 500.0, fuel: [tube(80.0), tube(12.0), Default::default(), Default::default()], ..Default::default() }
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

    #[test]
    fn the_manual_sliders_are_only_on_when_manual() {
        let auto = shipped_panel(Some(&news()));
        assert!(matches!(auto.find("fis_set").unwrap().widget, display_core::Widget::Slider { enabled: false, .. }));
        let man = shipped_panel(Some(&ReactorNews { manual: true, ..news() }));
        assert!(matches!(man.find("fis_set").unwrap().widget, display_core::Widget::Slider { enabled: true, .. }));
        assert!(man.find("mode").is_some() && man.find("reset").is_none());
        assert!(faults_panel(None).find("reset").is_some() && faults_panel(None).find("pump_break").is_some());
    }

    #[test]
    fn the_critical_readout_keeps_its_controls_and_says_critical() {
        let hot = ReactorNews { temp: 85.0, ..news() };
        assert!(crate::effects::is_critical(Some(&hot)));
        let crit = shipped_panel(Some(&hot));
        for a in ["scram", "power", "mode", "fis_set", "tur_set"] {
            assert!(crit.find(a).is_some(), "{a} while critical");
        }
        assert_eq!(header(&crit), Some(("CRITICAL".to_string(), Tone::Bad)));
        assert!(crit.overflowing().is_empty());
        let off = ReactorNews { power_on: false, ..hot };
        assert!(!crate::effects::is_critical(Some(&off)));
        let normal = shipped_panel(Some(&off));
        assert!(normal.find("fis_set").is_some(), "scrammed: the normal view");
        assert_eq!(reactor_status(Some(&off)), ("OFF", Tone::Dim));
        assert_eq!(header(&normal), Some(("OFF".to_string(), Tone::Dim)));
    }

    #[test]
    fn critical_is_the_same_at_every_temperature_on_the_readout_and_the_lamp() {
        for (allowed, temp) in [([20.0, 80.0], 85.0), ([10.0, 90.0], 85.0), ([10.0, 90.0], 95.0)] {
            let r = ReactorNews { allowed, temp, ..news() };
            let p = shipped_panel(Some(&r));
            let critical = crate::effects::is_critical(Some(&r));
            assert_eq!(header(&p).is_some_and(|h| h.0 == "CRITICAL"), critical, "allowed {allowed:?}, temp {temp}");
            assert_eq!(reactor_status(Some(&r)).0 == "CRITICAL", critical, "word at {temp} with {allowed:?}");
            assert!(p.find("fis_set").is_some() && p.find("scram").is_some(), "the controls stay at {temp}");
        }
    }

    #[test]
    fn a_melted_core_says_meltdown_without_a_button() {
        let p = shipped_panel(Some(&ReactorNews { melted: true, ..news() }));
        assert!(p.find("scram").is_none() && p.find("power").is_none() && p.find("fis_set").is_none());
        assert_eq!(header(&p), Some(("MELTDOWN".to_string(), Tone::Bad)));
        assert!(p.overflowing().is_empty());
    }
}
