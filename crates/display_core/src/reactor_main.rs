//! The reactor's main screen `screen_main` (#317, from the Claude Design "ReactorMain"): 1280 x 720
//! panel pixels, tier L, bound to the `screen_main` quad of `reactor.glb`. Left INJECTION (chamber
//! heat with the sim's bands, the round gauge of the actual injection, the slider of the setting),
//! right FIELD CONVERTER (output against demand, the history, the gauge of the actual converter, its
//! slider). Header: REACTOR, the status word that turns into the warnings, AUTO / MANUAL.
//!
//! Every number is the sim's (`ReactorNews`, from `reactor_core`); the design's JS is only a preview.
//! The screen draws no target mark of its own, no heat history and no heat trend (settled, "less
//! hand-holding"). The gauges show the ACTUAL value (it lags); the sliders the engineer's SETTING,
//! active in MANUAL only.
//!
//! The faces: BOOT (the start-up conditions as rows) and PRIME (the magnetic trap's charge) are drawn here; RUN-UP is the RUNNING
//! layout dimmed (no grid yet, AUTO, sliders disabled). The other faces (SCRAMMED, REACTOR OFF, IMPLOSION, CORE EJECTED) are a
//! plain centred word, the cold start (and an empty battery with the reactor off) a dark screen.
use crate::canvas::{Ink, Shape, TextFace};
use crate::{button, canvas, column, label, row, slider, space, Item, Panel, Tone};
use item_core::wire::{ControlGauge, Lamp, ReactorNews, StartStage, GRAPH_POINTS};

pub const W: f64 = 1280.0;
pub const H: f64 = 720.0;

pub const AUTO: &str = "mode_auto";
pub const MANUAL: &str = "mode_manual";
pub const INJECTION: &str = "inj_set";
pub const CONVERTER: &str = "conv_set";

/// The panel's border (the drawn 2 px frame), so the content is `W - 2 B` by `H - 2 B`.
const B: f64 = 2.0;
const HEADER: f64 = 72.0;
const GAP: f64 = 12.0;
/// One half (a Panel of the design): its width and height, and the width of its frame at each side
/// (border 2, inset 4, well border 2, padding 12): the content is 580 wide.
const HALF_W: f64 = (W - 2.0 * B - 3.0 * GAP) / 2.0;
const HALF_H: f64 = H - 2.0 * B - HEADER - 2.0 * GAP;
const EDGE: f64 = 20.0;
const INNER_W: f64 = HALF_W - 2.0 * EDGE;
/// A half's rows, top to bottom: the plate's header, the top padding, the heat / output block, the
/// gauge block, the slider, the bottom frame (sum `HALF_H`) and the two gaps between the blocks.
const ROW_HEAD: f64 = 34.0;
const ROW_PAD: f64 = 14.0;
const ROW_TOP: f64 = 232.0;
const ROW_GAUGE: f64 = 240.0;
const ROW_SLIDER: f64 = 48.0;
const ROW_FOOT: f64 = 20.0;
const ROW_GAP: f64 = (HALF_H - ROW_HEAD - ROW_PAD - ROW_TOP - ROW_GAUGE - ROW_SLIDER - ROW_FOOT) / 2.0;

/// The gauge: 270 degrees from the bottom left, clockwise on screen; a 260 x 236 box, centre (130, 130).
const ARC_START: f64 = 135.0;
const ARC_SWEEP: f64 = 270.0;
const G_W: f64 = 260.0;
const G_H: f64 = 236.0;
const G_C: f64 = 130.0;
const G_R: f64 = 100.0;

/// What the history graph draws: its box, as the design has it.
const GRAPH_W: f64 = 580.0;
const GRAPH_H: f64 = 108.0;

/// The output is called short of the demand by more than this (kW).
const GRID_SLACK_KW: f64 = 10.0;

/// The face the screen shows. RUN-UP is the `run_up` argument of `panel` (the sim's start stage), a look of `Running`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Face {
    /// The ship's master is off (the cold start, `StartupStage::live`): a dark screen.
    NoPower,
    /// The power is off with the chamber still warm: after a SCRAM.
    Scrammed,
    /// The power is off and the chamber cold.
    Off,
    /// The countdown to the implosion runs (seconds), or the core has imploded (None).
    Implosion(Option<f64>),
    /// The core is out; the countdown, if it runs on, in seconds.
    CoreEjected(Option<f64>),
    /// Awake on battery, the start-up conditions are listed (#319).
    Boot,
    /// The magnetic trap charges, or waits charged for RUN.
    Prime,
    Running,
}

/// Which face the news make. `None` news is a dead screen.
pub fn face(r: Option<&ReactorNews>) -> Face {
    let Some(r) = r else { return Face::NoPower };
    // `powered` (output above 0) is not the screen's supply: with the converter at 0 or after a SCRAM the engineer still needs it.
    // Dark: the cold start before the three toggles, or an empty battery with nothing else to power the screen. The toggles
    // falling (a SCRAM, an eject) do not darken it.
    let battery_dead = matches!(r.start, StartStage::Boot | StartStage::Prime) && !r.power_on && fin(r.battery_charge) <= 0.0;
    if !r.startup.live() || r.start == StartStage::Cold || battery_dead {
        Face::NoPower
    } else if r.imploded {
        Face::Implosion(None)
    } else if r.ejected {
        Face::CoreEjected(r.implosion_eta_s)
    } else if let Some(s) = r.implosion_eta_s.filter(|_| r.lamps[Lamp::Implosion.index()]) {
        Face::Implosion(Some(s))
    } else if r.start == StartStage::Boot {
        Face::Boot
    } else if r.start == StartStage::Prime {
        Face::Prime
    } else if !r.power_on {
        if r.lamps[Lamp::Scram.index()] { Face::Scrammed } else { Face::Off }
    } else {
        Face::Running
    }
}

/// The chamber heat in degrees for the big readout: a placeholder mapping of the sim's 0..100, the
/// design's x 10. TODO(initiator): the real scale. One function, so it changes in one place.
pub fn heat_degrees(heat: f64) -> f64 {
    heat * 10.0
}

/// The header's status word: the warnings joined with " · " (heat first, then the output), RUNNING in
/// the good tone when there are none, RUN-UP in the title tone while the output is still below the
/// demand during the run-up. A critical one makes the word bad, any other warn.
pub fn status_word(r: &ReactorNews, run_up: bool) -> (String, Tone) {
    let lamp = |l: Lamp| r.lamps[l.index()];
    let mut words = Vec::new();
    let mut critical = false;
    if lamp(Lamp::CritHeat) {
        words.push("CRITICAL HEAT");
        critical = true;
    } else if lamp(Lamp::Hot) {
        words.push("OVERHEAT");
    } else if lamp(Lamp::Cold) {
        words.push("COLD CHAMBER");
    } else if lamp(Lamp::LowHeat) {
        words.push("LOW HEAT");
    }
    if lamp(Lamp::CritOut) {
        words.push("CRITICAL OUTPUT");
        critical = true;
    } else if lamp(Lamp::HighOut) {
        words.push("HIGH OUTPUT");
    }
    if !words.is_empty() {
        return (words.join(" \u{b7} "), if critical { Tone::Bad } else { Tone::Warn });
    }
    if run_up && fin(r.output_kw) < fin(r.load_kw) - GRID_SLACK_KW {
        return ("RUN-UP".into(), Tone::Title);
    }
    ("RUNNING".into(), Tone::Good)
}

/// The grid word under the output: GRID OK, GRID SHORT - n kW, or GRID OFFLINE - n kW during the run-up.
pub fn grid_word(output: f64, demand: f64, run_up: bool) -> (String, Tone) {
    let short = fin(demand) - fin(output);
    if short <= GRID_SLACK_KW {
        ("GRID OK".into(), Tone::Normal)
    } else if run_up {
        (format!("GRID OFFLINE - {short:.0} kW"), Tone::Dim)
    } else {
        (format!("GRID SHORT - {short:.0} kW"), Tone::Warn)
    }
}

/// The heat's tone: good in the sim's optimal band, warn in the rest of the allowed one, bad above it
/// and warn below (a cold chamber is a warning, the sim's COLD lamp is no critical one).
fn heat_tone(heat: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Tone {
    if band_ok(optimal) && heat >= optimal[0] && heat <= optimal[1] {
        Tone::Normal
    } else if band_ok(allowed) && heat > allowed[1] {
        Tone::Bad
    } else if band_ok(allowed) {
        Tone::Warn
    } else {
        Tone::Normal
    }
}

fn band_ok(b: [f64; 2]) -> bool {
    b[0].is_finite() && b[1].is_finite() && b[1] > b[0]
}

/// NaN and infinity read as 0, so one bad number never reaches a shape.
fn fin(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

fn pct(v: f64) -> f64 {
    fin(v).clamp(0.0, 100.0)
}

fn clock(s: f64) -> String {
    let s = fin(s).max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

// ---------- the pieces ----------

fn text(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, ink: Ink, centred: bool) -> Shape {
    Shape::Text { x, y, w, h, text: s.to_string(), size, face, ink, centred }
}

fn rect(x: f64, y: f64, w: f64, h: f64, ink: Ink) -> Shape {
    Shape::Rect { x, y, w, h, fill: Some(ink), outline: None }
}

/// The frame at one side of a half's row of height `h`: border (line 2), inset (bg 1), well border
/// (line 1), then the well's padding.
fn edge(h: f64, left: bool) -> Vec<Shape> {
    let at = |from_outer: f64, w: f64| if left { from_outer } else { EDGE - from_outer - w };
    vec![rect(at(0.0, 2.0), 0.0, 2.0, h, Ink::Line2), rect(at(2.0, 4.0), 0.0, 4.0, h, Ink::Bg1), rect(at(6.0, 2.0), 0.0, 2.0, h, Ink::Line1)]
}

/// A canvas of a half's full width: both side frames, then `shapes` in the content's own pixels
/// (x from the content's left edge).
fn framed(h: f64, mut shapes: Vec<Shape>) -> Item {
    let mut all = edge(h, true);
    all.extend(edge_right(h));
    for s in shapes.drain(..) {
        all.push(shift_x(s, EDGE));
    }
    canvas(HALF_W, h, all, None).fixed(h)
}

fn edge_right(h: f64) -> Vec<Shape> {
    edge(h, false).into_iter().map(|s| shift_x(s, HALF_W - EDGE)).collect()
}

fn shift_x(s: Shape, dx: f64) -> Shape {
    match s {
        Shape::Arc { cx, cy, r, from, to, stroke, ink, dash } => Shape::Arc { cx: cx + dx, cy, r, from, to, stroke, ink, dash },
        Shape::Line { a, b, width, ink, dash } => Shape::Line { a: [a[0] + dx, a[1]], b: [b[0] + dx, b[1]], width, ink, dash },
        Shape::Circle { cx, cy, r, fill, stroke } => Shape::Circle { cx: cx + dx, cy, r, fill, stroke },
        Shape::Rect { x, y, w, h, fill, outline } => Shape::Rect { x: x + dx, y, w, h, fill, outline },
        Shape::Hatch { x, y, w, h, ink } => Shape::Hatch { x: x + dx, y, w, h, ink },
        Shape::Text { x, y, w, h, text, size, face, ink, centred } => Shape::Text { x: x + dx, y, w, h, text, size, face, ink, centred },
        other => other,
    }
}

/// A row of a half that holds a widget (a label, a slider): the frames at its sides, the widget between.
fn edged(h: f64, inner: Item) -> Item {
    row(0.0, vec![canvas(EDGE, h, edge(h, true), None).fixed(EDGE), inner.fixed(INNER_W), canvas(EDGE, h, edge(h, false), None).fixed(EDGE)]).fixed(h)
}

/// A row of just the frame (padding between blocks).
fn gap_row(h: f64) -> Item {
    framed(h, Vec::new())
}

/// The plate's header of a half: the frame's top and sides, the accent lamp and the title.
fn head(title: &str) -> Item {
    let mut s = vec![
        rect(0.0, 0.0, HALF_W, B, Ink::Line2),
        rect(0.0, 0.0, B, ROW_HEAD, Ink::Line2),
        rect(HALF_W - B, 0.0, B, ROW_HEAD, Ink::Line2),
        rect(B, B, HALF_W - 2.0 * B, ROW_HEAD - B, Ink::Bg1),
        rect(12.0, 13.0, 10.0, 10.0, Ink::Hud),
        text(32.0, B, HALF_W - 60.0, 32.0, title, 20.0, TextFace::DisplayBold, Ink::FgTitle, false),
    ];
    // The well's top border, closing the header.
    s.push(rect(B + 4.0, ROW_HEAD - 2.0, HALF_W - 2.0 * (B + 4.0), 2.0, Ink::Line1));
    canvas(HALF_W, ROW_HEAD, s, None).fixed(ROW_HEAD)
}

fn foot() -> Item {
    let s = vec![
        rect(0.0, 0.0, B, ROW_FOOT, Ink::Line2),
        rect(HALF_W - B, 0.0, B, ROW_FOOT, Ink::Line2),
        rect(0.0, ROW_FOOT - B, HALF_W, B, Ink::Line2),
        rect(B, 0.0, 4.0, ROW_FOOT - B, Ink::Bg1),
        rect(HALF_W - B - 4.0, 0.0, 4.0, ROW_FOOT - B, Ink::Bg1),
        rect(B + 4.0, 0.0, 2.0, ROW_FOOT - B - 4.0, Ink::Line1),
        rect(HALF_W - B - 6.0, 0.0, 2.0, ROW_FOOT - B - 4.0, Ink::Line1),
        rect(B + 4.0, ROW_FOOT - B - 6.0, HALF_W - 2.0 * (B + 4.0), 2.0, Ink::Line1),
        rect(B + 4.0, ROW_FOOT - B - 4.0, HALF_W - 2.0 * (B + 4.0), 4.0, Ink::Bg1),
    ];
    canvas(HALF_W, ROW_FOOT, s, None).fixed(ROW_FOOT)
}

fn point(a: f64, r: f64) -> [f64; 2] {
    let t = a.to_radians();
    [G_C + r * t.cos(), G_C + r * t.sin()]
}

fn angle(v: f64) -> f64 {
    ARC_START + ARC_SWEEP * pct(v) / 100.0
}

fn arc(from: f64, to: f64, ink: Ink) -> Shape {
    Shape::Arc { cx: G_C, cy: G_C, r: G_R, from: angle(from), to: angle(to), stroke: 18.0, ink, dash: None }
}

/// The gauge of one control: the track in bad, the sim's allowed band (warn) and optimal band (good)
/// over it, ticks every 10, the needle at the ACTUAL value, and its percent below. In a box `G_W` x `G_H`.
pub fn gauge_shapes(g: &ControlGauge) -> Vec<Shape> {
    let mut s = Vec::new();
    for i in 0..=10 {
        let a = angle(i as f64 * 10.0);
        let (p, q) = (point(a, 114.0), point(a, if i % 5 == 0 { 126.0 } else { 121.0 }));
        s.push(Shape::Line { a: p, b: q, width: 2.0, ink: Ink::HudDim, dash: None });
    }
    // Barotrauma's zones (client Reactor.cs DrawMeter): everything outside the allowed band is bad.
    s.push(arc(0.0, 100.0, Ink::Bad));
    if band_ok(g.allowed) {
        s.push(arc(g.allowed[0], g.allowed[1], Ink::Warn));
    }
    if band_ok(g.optimal) {
        s.push(arc(g.optimal[0], g.optimal[1], Ink::Good));
    }
    s.push(Shape::Line { a: [G_C, G_C], b: point(angle(g.actual), G_R - 24.0), width: 4.0, ink: Ink::FgTitle, dash: None });
    s.push(Shape::Circle { cx: G_C, cy: G_C, r: 8.0, fill: Some(Ink::Bg0), stroke: Some((Ink::Hud, 3.0)) });
    s.push(text(0.0, G_H - 44.0, G_W, 44.0, &format!("{:.0} %", pct(g.actual)), 40.0, TextFace::Mono, Ink::Fg1, true));
    s
}

fn gauge_row(g: &ControlGauge) -> Item {
    let x = (INNER_W - G_W) * 0.5;
    framed(ROW_GAUGE, gauge_shapes(g).into_iter().map(|s| shift_x(s, x)).collect())
}

/// The chamber heat's band strip above its bar: the sim's allowed band (warn) and optimal band (good),
/// both its low and its high side, the marker at the heat; the bar below fills to the heat.
pub fn heat_shapes(heat: f64, optimal: [f64; 2], allowed: [f64; 2], tone: Tone) -> Vec<Shape> {
    let at = |v: f64| pct(v) / 100.0 * INNER_W;
    let mut s = vec![rect(0.0, 0.0, INNER_W, 12.0, Ink::Bg3)];
    if band_ok(allowed) {
        s.push(rect(at(allowed[0]), 0.0, at(allowed[1]) - at(allowed[0]), 12.0, Ink::Warn));
    }
    if band_ok(optimal) {
        s.push(rect(at(optimal[0]), 0.0, at(optimal[1]) - at(optimal[0]), 12.0, Ink::Good));
    }
    // The bar: the track, the fill in the heat's tone, ticks at 0, 50 and 100.
    let fill = match tone {
        Tone::Bad => Ink::Bad,
        Tone::Warn => Ink::Warn,
        _ => Ink::Good,
    };
    s.push(Shape::Rect { x: 0.0, y: 18.0, w: INNER_W, h: 34.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) });
    s.push(rect(2.0, 20.0, (INNER_W - 4.0) * pct(heat) / 100.0, 30.0, fill));
    for v in [0.0, 50.0, 100.0] {
        let x = (at(v)).clamp(1.0, INNER_W - 1.0);
        s.push(Shape::Line { a: [x, 52.0], b: [x, 58.0], width: 2.0, ink: Ink::HudDim, dash: None });
    }
    // The marker over both: a bright tick across the strip and the bar.
    s.push(rect((at(heat) - 2.0).clamp(0.0, INNER_W - 4.0), 0.0, 4.0, 54.0, Ink::FgTitle));
    s
}

/// The output (good, or warn when short) and the demand (dashed) over the last samples, the oldest at
/// the left. Newest first in the news; the scale is the larger of the reactor's maximum and the highest sample.
pub fn graph_shapes(r: &ReactorNews, max_kw: f64, short: bool, dim: bool) -> Vec<Shape> {
    let mut s = vec![Shape::Rect { x: 0.0, y: 0.0, w: GRAPH_W, h: GRAPH_H, fill: Some(Ink::Bg0), outline: Some((Ink::Line1, 2.0)) }];
    for y in [27.0, 54.0, 81.0] {
        s.push(Shape::Line { a: [0.0, y], b: [GRAPH_W, y], width: 1.0, ink: Ink::Line1, dash: None });
    }
    let n = (r.graph_points as usize).min(GRAPH_POINTS);
    if n < 2 {
        return s;
    }
    let top = r.graph_output[..n].iter().chain(&r.graph_load[..n]).copied().max().unwrap_or(0) as f64;
    let scale = fin(max_kw).max(top).max(1.0);
    let pt = |curve: &[u16; GRAPH_POINTS], i: usize| [2.0 + (GRAPH_W - 4.0) * (1.0 - i as f64 / (n - 1) as f64), 104.0 - curve[i] as f64 / scale * 100.0];
    for i in 0..n - 1 {
        s.push(Shape::Line { a: pt(&r.graph_load, i + 1), b: pt(&r.graph_load, i), width: 2.0, ink: Ink::FgTitle, dash: Some((10.0, 5.0)) });
    }
    let ink = if dim { Ink::GoodDim } else if short { Ink::Warn } else { Ink::Good };
    for i in 0..n - 1 {
        s.push(Shape::Line { a: pt(&r.graph_output, i + 1), b: pt(&r.graph_output, i), width: 3.0, ink, dash: None });
    }
    s
}

fn mode_buttons(manual: bool) -> Item {
    let tone = |on: bool| if on { Tone::Good } else { Tone::Normal };
    column(0.0, vec![
        space().fixed(12.0),
        row(8.0, vec![button("AUTO", AUTO).tone(tone(!manual)), button("MANUAL", MANUAL).tone(tone(manual))]).fixed(48.0),
        space().fixed(12.0),
    ])
}

/// A label's width in px, estimated: the status word sits right-aligned next to the buttons, and the
/// label widget draws left aligned from its box.
fn label_w(s: &str, size: f64, mono: bool) -> f64 {
    s.chars().count() as f64 * size * if mono { 0.5 } else { 0.62 } + size * 1.4
}

fn header(r: &ReactorNews, run_up: bool) -> Item {
    let (word, tone) = status_word(r, run_up);
    header_with(word, tone, Some(r.manual && !run_up_active(r, run_up)))
}

/// The header with its word; `mode` is the AUTO / MANUAL buttons' state, None for the faces without them (BOOT, PRIME).
fn header_with(word: String, tone: Tone, mode: Option<bool>) -> Item {
    let t = vec![
        rect(14.0, 30.0, 12.0, 12.0, Ink::Hud),
        text(38.0, 0.0, 240.0, HEADER - B, "REACTOR", 28.0, TextFace::DisplayBold, Ink::FgTitle, false),
    ];
    let w = label_w(&word, 28.0, false).min(520.0);
    let inner = row(0.0, vec![
        canvas(300.0, HEADER - B, t, None).fixed(300.0),
        space(),
        label(word).tone(tone).text_size(28.0).fixed(w),
        space().fixed(24.0),
        mode.map_or_else(|| space().fixed(320.0), |manual| mode_buttons(manual).fixed(320.0)),
        space().fixed(28.0),
    ]);
    let rule = canvas(W - 2.0 * B, B, vec![rect(0.0, 0.0, W - 2.0 * B, B, Ink::Line1)], None).fixed(B);
    column(0.0, vec![inner.fixed(HEADER - B), rule]).fixed(HEADER)
}

/// The run-up is on and the output is still short of the demand: the screen dims the grid, the graph and the heat's warnings, and
/// the mode is AUTO with the sliders disabled.
fn run_up_active(r: &ReactorNews, run_up: bool) -> bool {
    run_up && fin(r.output_kw) < fin(r.load_kw) - GRID_SLACK_KW
}

fn left_half(r: &ReactorNews, up: bool) -> Item {
    let heat = fin(r.heat);
    let tone = if up { Tone::Normal } else { heat_tone(heat, r.optimal, r.allowed) };
    let readout = label(format!("{:.0} \u{b0}C", heat_degrees(heat))).mono().text_size(72.0).tone(tone);
    let block = vec![
        edged(24.0, canvas(INNER_W, 24.0, vec![text(0.0, 0.0, INNER_W, 24.0, "CHAMBER HEAT", 20.0, TextFace::Display, Ink::Fg2, false)], None)),
        edged(80.0, readout),
        gap_row(14.0),
        framed(60.0, heat_shapes(heat, r.optimal, r.allowed, tone)),
        gap_row(ROW_TOP - 24.0 - 80.0 - 14.0 - 60.0),
    ];
    half("INJECTION", block, &r.injection_gauge(), INJECTION, r.manual && !up)
}

fn right_half(r: &ReactorNews, max_kw: f64, run_up: bool, up: bool) -> Item {
    let (out, demand) = (fin(r.output_kw), fin(r.load_kw));
    let (word, tone) = grid_word(out, demand, run_up);
    let short = tone == Tone::Warn;
    let word_tone = if up { Tone::Dim } else if short { Tone::Warn } else { Tone::Good };
    let demand_text = format!("DEMAND {demand:.0} kW");
    let top = row(0.0, vec![label("OUTPUT").tone(Tone::Dim).text_size(20.0), label(demand_text.clone()).mono().tone(Tone::Dim).text_size(28.0).fixed(label_w(&demand_text, 28.0, true))]);
    let big = row(0.0, vec![
        label(format!("{out:.0} kW")).mono().text_size(72.0).tone(if short { Tone::Warn } else { Tone::Normal }),
        label(word.clone()).text_size(20.0).tone(word_tone).fixed(label_w(&word, 20.0, false).min(330.0)),
    ]);
    let graph = framed(GRAPH_H, graph_shapes(r, max_kw, short, up));
    let block = vec![edged(32.0, top), edged(80.0, big), gap_row(8.0), graph, gap_row(ROW_TOP - 32.0 - 80.0 - 8.0 - GRAPH_H)];
    half("FIELD CONVERTER", block, &r.converter_gauge(), CONVERTER, r.manual && !up)
}

/// One half: header, the block on top (heat or output), the gauge of the actual value, the slider of the setting.
fn half(title: &str, mut top: Vec<Item>, g: &ControlGauge, action: &str, manual: bool) -> Item {
    let mut rows = vec![head(title), gap_row(ROW_PAD)];
    rows.append(&mut top);
    rows.push(gap_row(ROW_GAP));
    rows.push(gauge_row(g));
    rows.push(gap_row(ROW_GAP));
    rows.push(edged(ROW_SLIDER, slider(fin(g.setting).clamp(0.0, 100.0) / 100.0, "", action).enabled(manual)));
    rows.push(foot());
    column(0.0, rows).fixed(HALF_W)
}

/// A plain centred word (and a second line under it) for the faces without a design yet.
fn word_face(word: &str, ink: Ink, second: Option<&str>) -> Item {
    let (w, h) = (W - 2.0 * B, H - 2.0 * B);
    let mut s = vec![text(0.0, h * 0.5 - 60.0, w, 90.0, word, 72.0, TextFace::DisplayBold, ink, true)];
    if let Some(t) = second {
        s.push(text(0.0, h * 0.5 + 40.0, w, 110.0, t, 96.0, TextFace::Mono, Ink::FgTitle, true));
    }
    canvas(w, h, s, None).flex(1.0)
}

fn body(r: &ReactorNews, max_kw: f64, run_up: bool, f: Face) -> Item {
    match f {
        Face::NoPower => canvas(W - 2.0 * B, H - 2.0 * B, Vec::new(), None).flex(1.0),
        Face::Scrammed => word_face("SCRAMMED", Ink::Bad, None),
        Face::Off => word_face("REACTOR OFF", Ink::Fg2, None),
        Face::Implosion(eta) => word_face("IMPLOSION", Ink::Bad, eta.map(clock).as_deref()),
        Face::CoreEjected(eta) => word_face("CORE EJECTED", Ink::Warn, eta.map(clock).as_deref()),
        Face::Boot => start_face("BOOT", boot_plate(r)),
        Face::Prime => start_face("PRIME", prime_plate(r)),
        Face::Running => {
            let up = run_up_active(r, run_up);
            let panels = row(GAP, vec![left_half(r, up), right_half(r, max_kw, run_up, up)]).fixed(HALF_H);
            column(0.0, vec![header(r, run_up), space().fixed(GAP), row(0.0, vec![space().fixed(GAP), panels, space().fixed(GAP)]).fixed(HALF_H), space().fixed(GAP)])
        }
    }
}

// ---------- BOOT and PRIME ----------

/// The full-width plate of the start faces: its width and height inside the header and the gaps.
const PLATE_W: f64 = W - 2.0 * B - 2.0 * GAP;

/// A face of the start-up: the header with `word` in the title tone (no mode buttons), then one full-width plate.
fn start_face(word: &str, plate: Item) -> Item {
    column(0.0, vec![header_with(word.into(), Tone::Title, None), space().fixed(GAP), row(0.0, vec![space().fixed(GAP), plate, space().fixed(GAP)]).fixed(HALF_H), space().fixed(GAP)])
}

/// The plate: frame, the title with its accent lamp, then `shapes` in plate pixels.
fn plate(title: &str, mut shapes: Vec<Shape>) -> Item {
    let mut all = vec![
        rect(0.0, 0.0, PLATE_W, HALF_H, Ink::Line2),
        rect(B, B, PLATE_W - 2.0 * B, HALF_H - 2.0 * B, Ink::Bg1),
        rect(12.0, 13.0, 10.0, 10.0, Ink::Hud),
        text(32.0, B, PLATE_W - 60.0, 32.0, title, 20.0, TextFace::DisplayBold, Ink::FgTitle, false),
        rect(B + 4.0, ROW_HEAD - 2.0, PLATE_W - 2.0 * (B + 4.0), 2.0, Ink::Line1),
    ];
    all.append(&mut shapes);
    canvas(PLATE_W, HALF_H, all, None).fixed(HALF_H)
}

/// A mono text right-aligned to `right` (the canvas aligns left or centred only).
fn text_right(right: f64, y: f64, h: f64, s: &str, size: f64, ink: Ink) -> Shape {
    let w = label_w(s, size, true);
    text(right - w, y, w, h, s, size, TextFace::Mono, ink, false)
}

/// The best canister's fill, % (0 with none).
fn best_fill(r: &ReactorNews) -> f64 {
    r.canisters.iter().filter(|c| c.canister.is_some()).map(|c| fin(c.life)).fold(0.0, f64::max).clamp(0.0, 100.0)
}

/// The START-UP rows, as (label, value, met), in the design's order.
pub fn boot_rows(r: &ReactorNews) -> Vec<(&'static str, String, bool)> {
    let onoff = |on: bool| (if on { "ON" } else { "OFF" }.to_string(), on);
    let fill = best_fill(r);
    let battery = fin(r.battery_charge).clamp(0.0, 1.0);
    let lever = match r.lever {
        0 => "OFF",
        1 => "PRIME",
        _ => "RUN",
    };
    let [(bus, bus_on), (trap, trap_on), (inj, inj_on)] = [onoff(r.toggles[0]), onoff(r.toggles[1]), onoff(r.toggles[2])];
    vec![
        ("BATTERY BUS", bus, bus_on),
        ("TRAP", trap, trap_on),
        ("INJECTOR", inj, inj_on),
        ("CANISTER", if fill > 0.0 { format!("{fill:.0} %") } else { "EMPTY".into() }, fill > 0.0),
        ("BATTERY", format!("{:.0} %", battery * 100.0), battery >= fin(r.prime_need)),
        ("START LEVER", lever.into(), r.lever == 0),
    ]
}

fn boot_plate(r: &ReactorNews) -> Item {
    let rows = boot_rows(r);
    let open = rows.iter().filter(|(_, _, met)| !met).count();
    let (head, tone) = if open == 0 { ("READY - START LEVER TO PRIME".to_string(), Ink::Good) } else { (format!("START NOT READY - {open} OPEN"), Ink::Warn) };
    let mut s = vec![text(24.0, ROW_HEAD + 8.0, PLATE_W - 48.0, 40.0, &head, 40.0, TextFace::DisplayBold, tone, false)];
    let (top, row_h, gap) = (ROW_HEAD + 64.0, 80.0, 6.0);
    for (i, (label, value, met)) in rows.iter().enumerate() {
        let y = top + i as f64 * (row_h + gap);
        let ink = if *met { Ink::Good } else { Ink::Bad };
        s.push(rect(16.0, y, PLATE_W - 32.0, row_h, Ink::Bg3));
        s.push(text(16.0 + 20.0, y + 24.0, 600.0, 32.0, label, 28.0, TextFace::Display, Ink::Fg1, false));
        s.push(text_right(PLATE_W - 16.0 - 20.0, y + 20.0, 44.0, value, 40.0, ink));
    }
    plate("START-UP", s)
}

/// The trap's charge as whole percent.
fn trap_pct(r: &ReactorNews) -> f64 {
    (fin(r.trap).clamp(0.0, 1.0) * 100.0).round()
}

fn prime_plate(r: &ReactorNews) -> Item {
    let charged = fin(r.trap) >= 1.0;
    let mut s = Vec::new();
    if charged {
        s.push(text(24.0, ROW_HEAD + 20.0, PLATE_W - 48.0, 44.0, "TRAP CHARGED", 40.0, TextFace::DisplayBold, Ink::Good, false));
    } else {
        s.push(text(24.0, ROW_HEAD + 36.0, PLATE_W - 48.0, 24.0, "TRAP CHARGING", 20.0, TextFace::Display, Ink::Fg2, false));
    }
    let y = ROW_HEAD + 90.0;
    s.push(text(24.0, y, 520.0, 80.0, &format!("{:.0} %", trap_pct(r)), 72.0, TextFace::Mono, Ink::Fg1, false));
    if !charged {
        s.push(text_right(PLATE_W - 24.0, y + 24.0, 44.0, &format!("{:.0} s", fin(r.trap_left_s).max(0.0).ceil()), 40.0, Ink::Fg2));
    }
    // The bar 0..100 with five ticks (one per lamp of the prime column).
    let (bx, by, bw, bh) = (24.0, y + 130.0, PLATE_W - 48.0, 60.0);
    let fill = if charged { Ink::Good } else { Ink::Hud };
    s.push(Shape::Rect { x: bx, y: by, w: bw, h: bh, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) });
    s.push(rect(bx + 2.0, by + 2.0, (bw - 4.0) * trap_pct(r) / 100.0, bh - 4.0, fill));
    for i in 0..=5 {
        let x = bx + bw * i as f64 / 5.0;
        s.push(Shape::Line { a: [x.clamp(bx + 1.0, bx + bw - 1.0), by + bh], b: [x.clamp(bx + 1.0, bx + bw - 1.0), by + bh + 12.0], width: 2.0, ink: Ink::HudDim, dash: None });
    }
    plate("MAGNETIC TRAP", s)
}

/// The main screen for the news (`None`: a dead screen). `max_kw` is the reactor's maximum output
/// (the graph's scale); `run_up` is the sim's run-up stage.
pub fn panel(r: Option<&ReactorNews>, max_kw: f64, run_up: bool) -> Panel {
    let f = face(r);
    let default = ReactorNews::default();
    let root = body(r.unwrap_or(&default), max_kw, run_up, f);
    Panel::new(W, H, root).with_padding(B)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Pointer, Widget};
    use glam::DVec2;
    use item_core::wire::StartupStage;

    fn running() -> ReactorNews {
        let mut r = ReactorNews { powered: true, power_on: true, startup: StartupStage::Ready, manual: true, heat: 50.0, injection: 46.0, set_injection: 46.0, converter: 64.0, set_converter: 64.0, output_kw: 1280.0, load_kw: 1280.0, ..Default::default() };
        r.optimal = [40.0, 60.0];
        r.allowed = [30.0, 70.0];
        r.injection_optimal = [25.0, 70.0];
        r.injection_allowed = [15.0, 90.0];
        r.converter_optimal = [61.5, 66.5];
        r.converter_allowed = [59.0, 69.0];
        r.graph_points = 12;
        r.graph_output = [1280; GRAPH_POINTS];
        r.graph_load = [1280; GRAPH_POINTS];
        r
    }

    /// Every shape of a panel lies on finite numbers.
    fn all_finite(p: &Panel) -> bool {
        let ok = |v: f64| v.is_finite();
        p.layout().iter().all(|w| match w.widget {
            Widget::Canvas { shapes, .. } => shapes.iter().all(|s| {
                let (a, b, c, d) = s.extent();
                ok(a) && ok(b) && ok(c) && ok(d)
            }),
            _ => true,
        })
    }

    #[test]
    fn the_panel_builds_for_every_state_and_fits() {
        let base = running();
        let mut cases = vec![("none", None), ("running", Some(base))];
        cases.push(("cold start", Some(ReactorNews { startup: StartupStage::Cold, ..base })));
        let boot = ReactorNews { start: StartStage::Boot, power_on: false, battery_charge: 0.5, prime_need: 0.08, ..base };
        cases.push(("boot", Some(boot)));
        cases.push(("boot ready", Some(ReactorNews { toggles: [true; 3], ..boot })));
        cases.push(("boot nan", Some(ReactorNews { battery_charge: f64::NAN, prime_need: f64::NAN, ..boot })));
        let prime = ReactorNews { start: StartStage::Prime, toggles: [true; 3], lever: 1, trap: 0.4, trap_left_s: 6.0, ..boot };
        cases.push(("prime", Some(prime)));
        cases.push(("prime charged", Some(ReactorNews { trap: 1.0, trap_left_s: 0.0, ..prime })));
        cases.push(("prime nan", Some(ReactorNews { trap: f64::NAN, trap_left_s: f64::INFINITY, ..prime })));
        cases.push(("run-up", Some(ReactorNews { start: StartStage::RunUp, output_kw: 300.0, ..base })));
        cases.push(("no output", Some(ReactorNews { powered: false, output_kw: 0.0, converter: 0.0, set_converter: 0.0, ..base })));
        cases.push(("off", Some(ReactorNews { power_on: false, ..base })));
        let mut scram = ReactorNews { power_on: false, ..base };
        scram.lamps[Lamp::Scram.index()] = true;
        cases.push(("scrammed", Some(scram)));
        cases.push(("imploded", Some(ReactorNews { imploded: true, ..base })));
        let mut countdown = base;
        countdown.lamps[Lamp::Implosion.index()] = true;
        countdown.implosion_eta_s = Some(75.0);
        cases.push(("countdown", Some(countdown)));
        cases.push(("ejected", Some(ReactorNews { ejected: true, implosion_eta_s: Some(10.0), ..base })));
        cases.push(("no canisters, no bands", Some(ReactorNews { optimal: [0.0; 2], allowed: [0.0; 2], injection_optimal: [0.0; 2], injection_allowed: [0.0; 2], converter_optimal: [0.0; 2], converter_allowed: [0.0; 2], ..base })));
        cases.push(("zero demand", Some(ReactorNews { load_kw: 0.0, output_kw: 0.0, graph_points: 0, ..base })));
        cases.push(("nan", Some(ReactorNews { heat: f64::NAN, injection: f64::NAN, converter: f64::INFINITY, set_injection: f64::NAN, output_kw: f64::NAN, load_kw: f64::NAN, ..base })));
        cases.push(("all lamps", Some(ReactorNews { lamps: [true; Lamp::COUNT], manual: false, ..base })));
        for run_up in [false, true] {
            for (name, r) in &cases {
                let p = panel(r.as_ref(), 2000.0, run_up);
                assert_eq!(p.size, DVec2::new(W, H), "{name}");
                assert!(p.overflowing().is_empty(), "{name}: {:?}", p.overflowing());
                assert!(all_finite(&p), "{name}");
            }
        }
    }

    #[test]
    fn the_faces_follow_the_news() {
        let r = running();
        assert_eq!(face(Some(&r)), Face::Running);
        assert_eq!(face(None), Face::NoPower);
        assert_eq!(face(Some(&ReactorNews { startup: StartupStage::Cold, ..r })), Face::NoPower);
        assert_eq!(face(Some(&ReactorNews { powered: false, output_kw: 0.0, ..r })), Face::Running, "no output is no reason to go dark");
        assert_eq!(face(Some(&ReactorNews { power_on: false, ..r })), Face::Off);
        assert_eq!(face(Some(&ReactorNews { imploded: true, ..r })), Face::Implosion(None));
        assert_eq!(face(Some(&ReactorNews { ejected: true, ..r })), Face::CoreEjected(None));
    }

    #[test]
    fn the_sliders_exist_only_in_the_running_face_and_work_in_manual_only() {
        let manual = panel(Some(&running()), 2000.0, false);
        let auto = panel(Some(&ReactorNews { manual: false, ..running() }), 2000.0, false);
        for a in [INJECTION, CONVERTER] {
            assert!(manual.find(a).is_some(), "{a}");
            let mut ptr = Pointer::default();
            let at = manual.find(a).unwrap().rect.centre();
            let clicks = ptr.step(&manual, Some(at), true);
            assert!(clicks.iter().any(|c| c.action == a), "manual: {a} answers");
            let mut ptr = Pointer::default();
            let at = auto.find(a).unwrap().rect.centre();
            assert!(ptr.step(&auto, Some(at), true).is_empty(), "auto: {a} is disabled");
        }
        for a in [AUTO, MANUAL] {
            assert!(auto.hit(auto.find(a).unwrap().rect.centre()).is_some(), "{a} always answers");
        }
        assert!(panel(Some(&ReactorNews { power_on: false, ..running() }), 2000.0, false).find(INJECTION).is_none());
    }

    #[test]
    fn the_header_word_joins_the_warnings() {
        let mut r = running();
        assert_eq!(status_word(&r, false), ("RUNNING".into(), Tone::Good));
        r.lamps[Lamp::Hot.index()] = true;
        r.lamps[Lamp::HighOut.index()] = true;
        assert_eq!(status_word(&r, false), ("OVERHEAT \u{b7} HIGH OUTPUT".into(), Tone::Warn));
        r.lamps[Lamp::CritHeat.index()] = true;
        assert_eq!(status_word(&r, false), ("CRITICAL HEAT \u{b7} HIGH OUTPUT".into(), Tone::Bad));
        let mut up = running();
        up.output_kw = 300.0;
        assert_eq!(status_word(&up, true), ("RUN-UP".into(), Tone::Title));
        assert_eq!(status_word(&up, false).0, "RUNNING");
    }

    #[test]
    fn the_grid_word_says_short_or_offline() {
        assert_eq!(grid_word(1280.0, 1280.0, false).0, "GRID OK");
        assert_eq!(grid_word(800.0, 1280.0, false).0, "GRID SHORT - 480 kW");
        assert_eq!(grid_word(800.0, 1280.0, true).0, "GRID OFFLINE - 480 kW");
        assert_eq!(grid_word(0.0, 0.0, true).0, "GRID OK");
    }

    #[test]
    fn the_start_faces_follow_the_stage_and_the_dark_rule() {
        let r = running();
        let boot = ReactorNews { start: StartStage::Boot, power_on: false, battery_charge: 0.5, ..r };
        assert_eq!(face(Some(&ReactorNews { start: StartStage::Cold, ..r })), Face::NoPower, "cold start: dark");
        assert_eq!(face(Some(&boot)), Face::Boot);
        assert_eq!(face(Some(&ReactorNews { toggles: [false; 3], ..boot })), Face::Boot, "the toggles fallen after a SCRAM do not darken it");
        assert_eq!(face(Some(&ReactorNews { battery_charge: 0.0, ..boot })), Face::NoPower, "an empty battery: dark");
        assert_eq!(face(Some(&ReactorNews { start: StartStage::Prime, ..boot })), Face::Prime);
        assert_eq!(face(Some(&ReactorNews { start: StartStage::RunUp, ..r })), Face::Running);
        assert_eq!(face(Some(&ReactorNews { start: StartStage::ShutDown, ..r })), Face::Running);
        assert_eq!(face(Some(&ReactorNews { ejected: true, ..boot })), Face::CoreEjected(None));
    }

    #[test]
    fn the_boot_rows_count_what_is_open() {
        let mut r = ReactorNews { start: StartStage::Boot, power_on: false, battery_charge: 0.5, prime_need: 0.08, ..running() };
        r.canisters[0].canister = Some(item_core::wire::CanisterKind::Standard);
        r.canisters[0].life = 80.0;
        let open = |r: &ReactorNews| boot_rows(r).iter().filter(|x| !x.2).count();
        assert_eq!(open(&r), 3, "the three toggles");
        r.toggles = [true; 3];
        assert_eq!(open(&r), 0);
        r.canisters[0].life = 0.0;
        assert_eq!(boot_rows(&r)[3], ("CANISTER", "EMPTY".into(), false));
        r.canisters[0].life = 80.0;
        r.battery_charge = 0.05;
        assert!(!boot_rows(&r)[4].2, "short of the PRIME charge");
        r.battery_charge = 0.5;
        r.lever = 1;
        assert_eq!(boot_rows(&r)[5], ("START LEVER", "PRIME".into(), false), "met only at OFF");
    }

    #[test]
    fn the_run_up_disables_the_sliders_and_shows_auto() {
        let up = panel(Some(&ReactorNews { start: StartStage::RunUp, output_kw: 300.0, ..running() }), 2000.0, true);
        for a in [INJECTION, CONVERTER] {
            let mut ptr = Pointer::default();
            let at = up.find(a).unwrap().rect.centre();
            assert!(ptr.step(&up, Some(at), true).is_empty(), "{a} is disabled in the run-up");
        }
        // Done (output at demand): the sliders work again in MANUAL.
        let done = panel(Some(&ReactorNews { start: StartStage::RunUp, ..running() }), 2000.0, true);
        let mut ptr = Pointer::default();
        let at = done.find(INJECTION).unwrap().rect.centre();
        assert!(!ptr.step(&done, Some(at), true).is_empty());
    }
}
