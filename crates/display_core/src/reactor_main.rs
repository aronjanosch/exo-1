//! The reactor's main screen `screen_main` (#317, from the Claude Design "ReactorMain", second version): 1280 x 720
//! panel pixels, tier L, bound to the `screen_main` quad of `reactor.glb`. Running: a header (REACTOR, the status word,
//! the CONTROL / HISTORY tabs and AUTO / MANUAL), then either the CONTROL tab (left INJECTION: chamber heat with the sim's
//! bands, the gauge of the actual injection, the slider of the setting; right FIELD CONVERTER: output against demand, its
//! gauge and slider) or the HISTORY tab (the last 60 s of output and demand).
//!
//! Every number is the sim's (`ReactorNews`, from `reactor_core`); the design's JS is only a preview. The screen draws no
//! target mark of its own, no heat history and no heat trend (settled, "less hand-holding"). The gauges show the ACTUAL
//! value (it lags) and carry no number; the SET readout and the sliders show the engineer's SETTING, active in MANUAL only.
//!
//! The faces: BOOT (the start-up rows) and PRIME (the magnetic trap's charge) have the header without buttons; RUN-UP is the
//! RUNNING layout with no grid yet (AUTO, sliders disabled, no banners). SCRAMMED, REACTOR OFF, IMPLOSION and CORE EJECTED are
//! a centred word without a header (and a clock for the last two); the cold start (and an empty battery with the reactor off)
//! is a dark screen. The tab is UI state of the client at the screen: the glue passes it in.
use crate::canvas::{Ink, Shape, TextFace};
use crate::{button, canvas, column, row, slider, space, Item, Panel, Tone};
use item_core::wire::{ControlGauge, Lamp, ReactorNews, StartStage, GRAPH_POINTS};

pub const W: f64 = 1280.0;
pub const H: f64 = 720.0;

pub const AUTO: &str = "mode_auto";
pub const MANUAL: &str = "mode_manual";
pub const INJECTION: &str = "inj_set";
pub const CONVERTER: &str = "conv_set";
pub const TAB_CONTROL: &str = "tab_control";
pub const TAB_HISTORY: &str = "tab_history";

/// Which tab of the running screen is shown. Client-side UI state: it changes nothing in the sim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Control,
    History,
}

/// The panel's border (the drawn 2 px frame), so the content is `W - 2 B` by `H - 2 B`.
const B: f64 = 2.0;
const HEADER: f64 = 70.0;
const GAP: f64 = 12.0;
const CONTENT_W: f64 = W - 2.0 * B;
/// The body below the header: padding 12 round the panels.
const BODY_W: f64 = CONTENT_W - 2.0 * GAP;
const BODY_H: f64 = H - 2.0 * B - HEADER - 2.0 * GAP;
/// One of the two control panels: width, the 20 px padding inside the 2 px border, the inner width.
const HALF_W: f64 = (BODY_W - GAP) / 2.0;
const PAD: f64 = 20.0;
const IN_X: f64 = B + PAD;
const IN_W: f64 = HALF_W - 2.0 * IN_X;
const ROW_SLIDER: f64 = 88.0;
const ROW_FOOT: f64 = 14.0;
const TOP_H: f64 = BODY_H - ROW_SLIDER - ROW_FOOT;
/// The panel header: its height with its closing line.
const HEAD: f64 = 36.0;

/// The gauge: 270 degrees from the bottom left, clockwise on screen; the design's 260 x 236 box scaled by 0.95.
const ARC_START: f64 = 135.0;
const ARC_SWEEP: f64 = 270.0;
const K: f64 = 0.95;
pub const G_W: f64 = 260.0 * K;
pub const G_H: f64 = 236.0 * K;
const G_C: f64 = 130.0 * K;
const G_R: f64 = 100.0 * K;

/// The output is called short of the demand by more than this (kW).
const GRID_SLACK_KW: f64 = 10.0;
/// The history's step: the news carry `GRAPH_POINTS` samples, `graph_interval_s` apart (2.5 s in the shipped tuning, so 60 s).
const HISTORY_SPAN_S: f64 = 60.0;

/// The face the screen shows. RUN-UP is the `run_up` argument of `panel` (the sim's start stage), a look of `Running`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Face {
    /// The ship's master is off (the reactor's cold start, `StartStage::Cold`): a dark screen.
    NoPower,
    /// The power is off with the chamber still warm: after a SCRAM.
    Scrammed,
    /// The power is off and the chamber cold.
    Off,
    /// The countdown to the implosion runs (seconds), or the core has imploded (None).
    Implosion(Option<f64>),
    /// The core is out; the countdown, if it runs on outside the ship, in seconds (the news carry the outside one while ejected).
    /// TODO(initiator): placeholder look.
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
    if r.start == StartStage::Cold || battery_dead {
        Face::NoPower
    } else if r.imploded {
        Face::Implosion(None)
    } else if r.ejected {
        Face::CoreEjected(r.implosion_eta_s)
    } else if let Some(s) = r.implosion_eta_s.filter(|s| s.is_finite() && r.lamps[Lamp::Implosion.index()]) {
        // The countdown takes the screen over at the implosion lamp (the timer past half its delay, #325). Earlier it would hide
        // AUTO and the sliders the engineer needs to bring the heat down. TODO(initiator): when it takes over, and the
        // placeholder look (the word and the clock) until the designed state arrives.
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

/// The heat's warning word and tone from the sim's lamps: critical (bad) before overheat, then cold, then low heat (warn).
fn heat_warning(r: &ReactorNews) -> Option<(&'static str, Tone)> {
    let lamp = |l: Lamp| r.lamps[l.index()];
    if lamp(Lamp::CritHeat) {
        Some(("CRITICAL HEAT", Tone::Bad))
    } else if lamp(Lamp::Hot) {
        Some(("OVERHEAT", Tone::Warn))
    } else if lamp(Lamp::Cold) {
        Some(("COLD CHAMBER", Tone::Warn))
    } else if lamp(Lamp::LowHeat) {
        Some(("LOW HEAT", Tone::Warn))
    } else {
        None
    }
}

/// What the engineer does about a heat warning, for its banner.
fn heat_action(word: &str) -> &'static str {
    match word {
        "CRITICAL HEAT" => "CUT INJECTION",
        "OVERHEAT" => "LOWER INJECTION",
        _ => "RAISE INJECTION",
    }
}

/// The output's warning word and tone from the sim's lamps.
fn output_warning(r: &ReactorNews) -> Option<(&'static str, Tone)> {
    let lamp = |l: Lamp| r.lamps[l.index()];
    if lamp(Lamp::CritOut) {
        Some(("CRITICAL OUTPUT", Tone::Bad))
    } else if lamp(Lamp::HighOut) {
        Some(("HIGH OUTPUT", Tone::Warn))
    } else {
        None
    }
}

/// The header's status word: the warnings joined with " · " (heat first, then the output), RUNNING in
/// the good tone when there are none, RUN-UP in the title tone while the output is still below the
/// demand during the run-up. A critical one makes the word bad, any other warn.
pub fn status_word(r: &ReactorNews, run_up: bool) -> (String, Tone) {
    let words: Vec<(&str, Tone)> = [heat_warning(r), output_warning(r)].into_iter().flatten().collect();
    if !words.is_empty() {
        let critical = words.iter().any(|w| w.1 == Tone::Bad);
        let text: Vec<&str> = words.iter().map(|w| w.0).collect();
        return (text.join(" \u{b7} "), if critical { Tone::Bad } else { Tone::Warn });
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

fn ink(t: Tone) -> Ink {
    match t {
        Tone::Normal => Ink::Fg1,
        Tone::Dim => Ink::Fg2,
        Tone::Micro => Ink::Fg3,
        Tone::Title => Ink::FgTitle,
        Tone::Good => Ink::Good,
        Tone::Warn => Ink::Warn,
        Tone::Bad => Ink::Bad,
    }
}

// ---------- the pieces ----------

fn text(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, ink: Ink) -> Shape {
    Shape::Text { x, y, w, h, text: s.to_string(), size, face, ink, centred: false }
}

fn text_c(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, ink: Ink) -> Shape {
    Shape::Text { x, y, w, h, text: s.to_string(), size, face, ink, centred: true }
}

fn text_r(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, ink: Ink) -> Shape {
    Shape::TextRight { x, y, w, h, text: s.to_string(), size, face, ink }
}

fn rect(x: f64, y: f64, w: f64, h: f64, ink: Ink) -> Shape {
    Shape::Rect { x, y, w, h, fill: Some(ink), outline: None }
}

fn boxed(x: f64, y: f64, w: f64, h: f64, fill: Ink, edge: Ink, b: f64) -> Shape {
    Shape::Rect { x, y, w, h, fill: Some(fill), outline: Some((edge, b)) }
}

/// The shapes moved by `(dx, dy)`.
fn shifted(shapes: Vec<Shape>, dx: f64, dy: f64) -> Vec<Shape> {
    shapes
        .into_iter()
        .map(|s| match s {
            Shape::Arc { cx, cy, r, from, to, stroke, ink, dash } => Shape::Arc { cx: cx + dx, cy: cy + dy, r, from, to, stroke, ink, dash },
            Shape::Line { a, b, width, ink, dash } => Shape::Line { a: [a[0] + dx, a[1] + dy], b: [b[0] + dx, b[1] + dy], width, ink, dash },
            Shape::Circle { cx, cy, r, fill, stroke } => Shape::Circle { cx: cx + dx, cy: cy + dy, r, fill, stroke },
            Shape::Rect { x, y, w, h, fill, outline } => Shape::Rect { x: x + dx, y: y + dy, w, h, fill, outline },
            Shape::Hatch { x, y, w, h, ink } => Shape::Hatch { x: x + dx, y: y + dy, w, h, ink },
            Shape::Text { x, y, w, h, text, size, face, ink, centred } => Shape::Text { x: x + dx, y: y + dy, w, h, text, size, face, ink, centred },
            Shape::TextRight { x, y, w, h, text, size, face, ink } => Shape::TextRight { x: x + dx, y: y + dy, w, h, text, size, face, ink },
            other => other,
        })
        .collect()
}

/// The border of a box `w` by `h` as four strips (the ground shows through).
fn frame(w: f64, h: f64, ink: Ink) -> Vec<Shape> {
    vec![rect(0.0, 0.0, w, B, ink), rect(0.0, h - B, w, B, ink), rect(0.0, 0.0, B, h, ink), rect(w - B, 0.0, B, h, ink)]
}

/// A panel's header: its square lamp and title, closed by a line (the box is `w` wide, 2 px border).
fn panel_head(w: f64, title: &str) -> Vec<Shape> {
    vec![rect(B, HEAD - B, w - 2.0 * B, B, Ink::Line1), rect(IN_X, 13.0, 10.0, 10.0, Ink::Hud), text(IN_X + 20.0, B, w - 80.0, HEAD - 2.0 * B, title, 20.0, TextFace::DisplayBold, Ink::FgTitle)]
}

fn border_ink(t: Option<Tone>) -> Ink {
    match t {
        Some(Tone::Bad) => Ink::Bad,
        Some(_) => Ink::Warn,
        None => Ink::Line2,
    }
}

fn point(a: f64, r: f64) -> [f64; 2] {
    let t = a.to_radians();
    [G_C + r * t.cos(), G_C + r * t.sin()]
}

fn angle(v: f64) -> f64 {
    ARC_START + ARC_SWEEP * pct(v) / 100.0
}

fn arc(from: f64, to: f64, ink: Ink) -> Shape {
    Shape::Arc { cx: G_C, cy: G_C, r: G_R, from: angle(from), to: angle(to), stroke: 18.0 * K, ink, dash: None }
}

/// The gauge of one control: the track in bad, the sim's allowed band (warn) and optimal band (good)
/// over it, ticks every 10 (long every 50), the needle at the ACTUAL value. No number. In a box `G_W` x `G_H`.
pub fn gauge_shapes(g: &ControlGauge) -> Vec<Shape> {
    let mut s = Vec::new();
    for i in 0..=10 {
        let a = angle(i as f64 * 10.0);
        let (p, q) = (point(a, 114.0 * K), point(a, if i % 5 == 0 { 126.0 * K } else { 121.0 * K }));
        s.push(Shape::Line { a: p, b: q, width: 2.0 * K, ink: Ink::HudDim, dash: None });
    }
    // Barotrauma's zones (client Reactor.cs DrawMeter): everything outside the allowed band is bad.
    s.push(arc(0.0, 100.0, Ink::Bad));
    if band_ok(g.allowed) {
        s.push(arc(g.allowed[0], g.allowed[1], Ink::Warn));
    }
    if band_ok(g.optimal) {
        s.push(arc(g.optimal[0], g.optimal[1], Ink::Good));
    }
    s.push(Shape::Line { a: [G_C, G_C], b: point(angle(g.actual), 76.0 * K), width: 5.0 * K, ink: Ink::FgTitle, dash: None });
    s.push(Shape::Circle { cx: G_C, cy: G_C, r: 9.0 * K, fill: Some(Ink::Bg0), stroke: Some((Ink::Hud, 3.0 * K)) });
    s
}

/// The band strip (10 px, bg 2) above a bar: the allowed band (warn) and the optimal band (good), each with a 3 px top edge;
/// bands as 0..100 of the width `w`.
fn band_strip(x: f64, y: f64, w: f64, allowed: [f64; 2], optimal: [f64; 2]) -> Vec<Shape> {
    let at = |v: f64| pct(v) / 100.0 * w;
    let mut s = vec![rect(x, y, w, 10.0, Ink::Bg2)];
    for (band, fill, edge) in [(allowed, Ink::WarnDim, Ink::Warn), (optimal, Ink::GoodDim, Ink::Good)] {
        if band_ok(band) {
            let (a, b) = (at(band[0]), at(band[1]));
            s.push(rect(x + a, y, b - a, 10.0, fill));
            s.push(rect(x + a, y, b - a, 3.0, edge));
        }
    }
    s
}

/// A bar of 26 px: the track, the fill to `fill` (0..100) in `ink`, and the marker (4 px, 4 px taller each way) at `marker`.
fn bar(x: f64, y: f64, w: f64, fill: f64, ink: Ink, marker: f64) -> Vec<Shape> {
    let mx = (x + w * pct(marker) / 100.0 - 2.0).clamp(x, x + w - 4.0);
    vec![boxed(x, y, w, 26.0, Ink::Bg3, Ink::HudDim, 2.0), rect(x + 2.0, y + 2.0, (w - 4.0) * pct(fill) / 100.0, 22.0, ink), rect(mx, y - 4.0, 4.0, 34.0, Ink::FgTitle)]
}

/// The warning banner laid over a panel's top label row.
fn banner(y: f64, h: f64, word: &str, tone: Tone, right: Option<&str>) -> Vec<Shape> {
    let (fill, edge) = if tone == Tone::Bad { (Ink::BadDim, Ink::Bad) } else { (Ink::WarnDim, Ink::Warn) };
    let mut s = vec![boxed(IN_X, y, IN_W, h, fill, edge, 2.0), text(IN_X + 14.0, y, IN_W - 28.0, h, word, 18.0, TextFace::Display, ink(tone))];
    if let Some(d) = right {
        s.push(text_r(IN_X + 14.0, y, IN_W - 28.0, h, d, 20.0, TextFace::Mono, Ink::Fg2));
    }
    s
}

/// The shared lower part of a control panel's top canvas: the gauge and the SET row.
fn gauge_and_set(g: &ControlGauge, label_text: &str, manual: bool) -> Vec<Shape> {
    let gy = 209.0;
    let mut s = shifted(gauge_shapes(g), IN_X + (IN_W - G_W) * 0.5, gy);
    let set_tone = if manual { Tone::Normal } else { Tone::Dim };
    s.push(text(IN_X, 480.0, IN_W * 0.6, 40.0, label_text, 20.0, TextFace::Display, Ink::Fg2));
    s.push(text_r(IN_X + IN_W * 0.4, 476.0, IN_W * 0.6, 44.0, &format!("{:.0} %", pct(g.setting)), 40.0, TextFace::Mono, ink(set_tone)));
    s
}

/// One control panel: its top canvas (everything down to the SET row), the slider and the foot, side by side with the border.
fn control_panel(border: Ink, mut top: Vec<Shape>, title: &str, g: &ControlGauge, action: &str, set_label: &str, manual: bool, up: bool) -> Item {
    let mut all = frame(HALF_W, TOP_H, border);
    all.extend(panel_head(HALF_W, title));
    all.append(&mut top);
    all.extend(gauge_and_set(g, set_label, manual && !up));
    // The slider row: the border strips at the sides, the track between (the thumb overhangs it by 20 px into the padding).
    let edge = |x: f64| canvas(IN_X + PAD, ROW_SLIDER, vec![rect(x, 0.0, B, ROW_SLIDER, border)], None).fixed(IN_X + PAD);
    let foot = vec![rect(0.0, 0.0, B, ROW_FOOT, border), rect(HALF_W - B, 0.0, B, ROW_FOOT, border), rect(0.0, ROW_FOOT - B, HALF_W, B, border)];
    column(0.0, vec![
        canvas(HALF_W, TOP_H, all, None).fixed(TOP_H),
        row(0.0, vec![edge(0.0), slider(pct(g.setting) / 100.0, "", action).grip().enabled(manual && !up).fixed(IN_W - 2.0 * PAD), edge(IN_X + PAD - B)]).fixed(ROW_SLIDER),
        canvas(HALF_W, ROW_FOOT, foot, None).fixed(ROW_FOOT),
    ])
    .fixed(HALF_W)
}

fn left_panel(r: &ReactorNews, up: bool) -> Item {
    let heat = fin(r.heat);
    let warning = if up { None } else { heat_warning(r) };
    let tone = warning.map_or(Tone::Normal, |w| w.1);
    let mut s = Vec::new();
    s.push(text(IN_X, 58.0, IN_W, 24.0, "CHAMBER HEAT", 20.0, TextFace::Display, Ink::Fg2));
    if let Some((word, t)) = warning {
        s.extend(banner(54.0, 30.0, &format!("{word} - {}", heat_action(word)), t, None));
    }
    s.push(text(IN_X, 82.0, IN_W, 48.0, &format!("{:.0} \u{b0}C", heat_degrees(heat)), 40.0, TextFace::Mono, ink(tone)));
    s.extend(band_strip(IN_X, 134.0, IN_W, r.allowed, r.optimal));
    let fill = match tone {
        Tone::Bad => Ink::Bad,
        Tone::Warn => Ink::Warn,
        _ => Ink::Good,
    };
    s.extend(bar(IN_X, 148.0, IN_W, heat, fill, heat));
    control_panel(border_ink(warning.map(|w| w.1)), s, "INJECTION", &r.injection_gauge(), INJECTION, "INJECTION SET", r.manual, up)
}

/// The band `lo..hi` times the demand as percent of the reactor's maximum output (`None`: no maximum).
fn demand_band(demand: f64, max_kw: f64, lo: f64, hi: f64) -> [f64; 2] {
    let m = fin(max_kw);
    if m <= 0.0 {
        return [0.0, 0.0];
    }
    [fin(demand) * lo / m * 100.0, fin(demand) * hi / m * 100.0]
}

fn right_panel(r: &ReactorNews, max_kw: f64, run_up: bool, up: bool) -> Item {
    let (out, demand) = (fin(r.output_kw), fin(r.load_kw));
    let (word, grid_tone) = grid_word(out, demand, run_up);
    let short = grid_tone == Tone::Warn;
    let demand_text = format!("DEMAND {demand:.0} kW");
    let banner_word: Option<(String, Tone)> = if up {
        None
    } else if let Some((w, t)) = output_warning(r) {
        Some((format!("{w} - LOWER CONVERTER"), t))
    } else if short {
        let cold = band_ok(r.optimal) && fin(r.heat) < r.optimal[0];
        Some((if cold { "GRID SHORT - CHAMBER TOO COLD" } else { "GRID SHORT - RAISE CONVERTER" }.to_string(), Tone::Warn))
    } else {
        None
    };
    let mut s = vec![text(IN_X, 58.0, IN_W * 0.5, 26.0, "OUTPUT", 20.0, TextFace::Display, Ink::Fg2), text_r(IN_X + IN_W * 0.4, 58.0, IN_W * 0.6, 26.0, &demand_text, 24.0, TextFace::Mono, Ink::Fg2)];
    if let Some((w, t)) = &banner_word {
        s.extend(banner(54.0, 32.0, w, *t, Some(&demand_text)));
    }
    let out_tone = if up { Tone::Dim } else if short { Tone::Warn } else { Tone::Normal };
    s.push(text(IN_X, 82.0, IN_W * 0.5, 48.0, &format!("{out:.0} kW"), 40.0, TextFace::Mono, ink(out_tone)));
    s.push(text_r(IN_X + IN_W * 0.4, 82.0, IN_W * 0.6, 48.0, &word, 20.0, TextFace::Display, ink(grid_tone)));
    s.extend(band_strip(IN_X, 134.0, IN_W, demand_band(demand, max_kw, 0.5, 1.5), demand_band(demand, max_kw, 0.9, 1.1)));
    let m = fin(max_kw);
    let (fill, marker) = if m > 0.0 { (out / m * 100.0, demand / m * 100.0) } else { (0.0, 0.0) };
    let fill_ink = if up { Ink::Fg2 } else if short { Ink::Warn } else { Ink::Good };
    s.extend(bar(IN_X, 148.0, IN_W, fill, fill_ink, marker));
    control_panel(border_ink(banner_word.map(|b| b.1)), s, "FIELD CONVERTER", &r.converter_gauge(), CONVERTER, "CONVERTER SET", r.manual, up)
}

/// The output (solid) and the demand (dashed) over the history, the oldest at the left, in a plot `w` by `h`. The news carry newest
/// first; the scale is the larger of the reactor's maximum and the highest sample, returned with the shapes.
pub fn graph_shapes(r: &ReactorNews, max_kw: f64, w: f64, h: f64, ink: Ink) -> (Vec<Shape>, f64) {
    let mut s = vec![boxed(0.0, 0.0, w, h, Ink::Bg0, Ink::Line1, 2.0)];
    for i in 1..5 {
        let y = h * i as f64 / 5.0;
        s.push(rect(2.0, y - 1.0, w - 4.0, 1.0, Ink::Line1));
    }
    let n = (r.graph_points as usize).min(GRAPH_POINTS);
    let top = r.graph_output[..n].iter().chain(&r.graph_load[..n]).copied().max().unwrap_or(0) as f64;
    let scale = fin(max_kw).max(top).max(1.0);
    if n < 2 {
        return (s, scale);
    }
    let step = (w - 4.0) / (GRAPH_POINTS - 1) as f64;
    let pt = |curve: &[u16; GRAPH_POINTS], i: usize| [w - 2.0 - step * i as f64, h - 4.0 - curve[i] as f64 / scale * (h - 8.0)];
    for i in 0..n - 1 {
        s.push(Shape::Line { a: pt(&r.graph_load, i + 1), b: pt(&r.graph_load, i), width: 2.0, ink: Ink::FgTitle, dash: Some((10.0, 5.0)) });
    }
    for i in 0..n - 1 {
        s.push(Shape::Line { a: pt(&r.graph_output, i + 1), b: pt(&r.graph_output, i), width: 3.0, ink, dash: None });
    }
    (s, scale)
}

fn history_panel(r: &ReactorNews, max_kw: f64, run_up: bool, up: bool) -> Item {
    let (out, demand) = (fin(r.output_kw), fin(r.load_kw));
    let (word, grid_tone) = grid_word(out, demand, run_up);
    let short = grid_tone == Tone::Warn;
    let warning = if up { None } else { output_warning(r).map(|w| w.1).or(short.then_some(Tone::Warn)) };
    let out_tone = if up { Tone::Dim } else if short { Tone::Warn } else { Tone::Normal };
    let line_ink = if up { Ink::Fg2 } else if short { Ink::Warn } else { Ink::Good };
    let mut s = frame(BODY_W, BODY_H, border_ink(warning));
    let w = BODY_W;
    let inner = w - 2.0 * IN_X;
    s.extend(panel_head(w, "OUTPUT HISTORY"));
    s.push(text(IN_X, 46.0, 200.0, 76.0, "OUTPUT", 20.0, TextFace::Display, Ink::Fg2));
    s.push(text(IN_X + 120.0, 46.0, 360.0, 76.0, &format!("{out:.0} kW"), 56.0, TextFace::Mono, ink(out_tone)));
    s.push(text_r(IN_X + inner - 700.0, 46.0, 360.0, 76.0, &format!("DEMAND {demand:.0} kW"), 28.0, TextFace::Mono, Ink::Fg2));
    s.push(text_r(IN_X + inner - 320.0, 46.0, 320.0, 76.0, &word, 24.0, TextFace::Display, ink(grid_tone)));
    let (ax, plot_x, plot_y, plot_h) = (IN_X, IN_X + 92.0, 130.0, 420.0);
    let plot_w = inner - 92.0;
    let (plot, scale) = graph_shapes(r, max_kw, plot_w, plot_h, line_ink);
    s.extend(shifted(plot, plot_x, plot_y));
    for i in 0..5 {
        let frac = 1.0 - i as f64 / 4.0;
        let y = plot_y + 2.0 + (plot_h - 4.0) * i as f64 / 4.0;
        s.push(text_r(ax, y - 12.0, 80.0, 24.0, &format!("{:.0}", scale * frac), 16.0, TextFace::Mono, Ink::Fg2));
    }
    let ly = plot_y + plot_h + 6.0;
    s.push(text(plot_x, ly, 200.0, 24.0, &format!("-{HISTORY_SPAN_S:.0} s"), 16.0, TextFace::Display, Ink::Fg2));
    s.push(text_c(plot_x, ly, plot_w, 24.0, "OUTPUT SOLID - DEMAND DASHED", 16.0, TextFace::Display, Ink::Fg2));
    s.push(text_r(plot_x + plot_w - 200.0, ly, 200.0, 24.0, "NOW", 16.0, TextFace::Display, Ink::Fg2));
    canvas(BODY_W, BODY_H, s, None).flex(1.0)
}

fn mode_buttons(manual: bool) -> Item {
    let tone = |on: bool| if on { Tone::Good } else { Tone::Normal };
    row(0.0, vec![button("AUTO", AUTO).tone(tone(!manual)), button("MANUAL", MANUAL).tone(tone(manual))]).fixed(320.0)
}

fn tab_buttons(tab: Tab) -> Item {
    let tone = |on: bool| if on { Tone::Good } else { Tone::Normal };
    row(0.0, vec![button("CONTROL", TAB_CONTROL).tone(tone(tab == Tab::Control)), button("HISTORY", TAB_HISTORY).tone(tone(tab == Tab::History))]).fixed(250.0)
}

/// The header: REACTOR at the left, the status word at the right, and with `controls` (the running faces) the tab and mode buttons.
fn header(word: &str, tone: Tone, size: f64, controls: Option<(Tab, bool)>) -> Item {
    let left = 200.0;
    let side = 24.0;
    let gap = 20.0;
    let fixed = 2.0 * side + left + gap + if controls.is_some() { 250.0 + 320.0 + 2.0 * gap } else { 0.0 };
    let wide = CONTENT_W - fixed;
    let l = vec![rect(0.0, 29.0, 10.0, 10.0, Ink::Hud), text(24.0, 0.0, left - 24.0, HEADER - B, "REACTOR", 28.0, TextFace::DisplayBold, Ink::FgTitle)];
    let status = vec![text_r(0.0, 0.0, wide, HEADER - B, word, size, TextFace::DisplayBold, ink(tone))];
    let mut items = vec![space().fixed(side), canvas(left, HEADER - B, l, None).fixed(left), space().fixed(gap), canvas(wide, HEADER - B, status, None).fixed(wide)];
    match controls {
        Some((tab, manual)) => {
            let buttons = |b: Item| column(0.0, vec![space().fixed(10.0), b.fixed(48.0), space().fixed(8.0)]);
            items.extend([space().fixed(gap), buttons(tab_buttons(tab)).fixed(250.0), space().fixed(gap), buttons(mode_buttons(manual)).fixed(320.0), space().fixed(side)]);
        }
        None => items.push(space().fixed(side)),
    }
    let rule = canvas(CONTENT_W, B, vec![rect(0.0, 0.0, CONTENT_W, B, Ink::Line1)], None).fixed(B);
    column(0.0, vec![row(0.0, items).fixed(HEADER - B), rule]).fixed(HEADER)
}

/// The run-up is on and the output is still short of the demand: the screen dims the grid, the graph and the heat's warnings, and
/// the mode is AUTO with the sliders disabled.
fn run_up_active(r: &ReactorNews, run_up: bool) -> bool {
    run_up && fin(r.output_kw) < fin(r.load_kw) - GRID_SLACK_KW
}

/// Header, then the body (padding 12 round it).
fn with_body(head: Item, body: Item) -> Item {
    column(0.0, vec![head, space().fixed(GAP), row(0.0, vec![space().fixed(GAP), body, space().fixed(GAP)]).fixed(BODY_H), space().fixed(GAP)])
}

/// A word face: the word (72 px) centred, with a mono clock (96 px) under it, no header.
fn word_face(word: &str, tone: Ink, clock_text: Option<&str>) -> Item {
    let (w, h) = (CONTENT_W, H - 2.0 * B);
    let group = 80.0 + clock_text.map_or(0.0, |_| 28.0 + 110.0);
    let top = (h - group) * 0.5;
    let mut s = vec![text_c(0.0, top, w, 80.0, word, 72.0, TextFace::DisplayBold, tone)];
    if let Some(t) = clock_text {
        s.push(text_c(0.0, top + 80.0 + 28.0, w, 110.0, t, 96.0, TextFace::Mono, Ink::Fg2));
    }
    canvas(w, h, s, None).flex(1.0)
}

fn body(r: &ReactorNews, max_kw: f64, run_up: bool, tab: Tab, f: Face) -> Item {
    match f {
        Face::NoPower => canvas(CONTENT_W, H - 2.0 * B, Vec::new(), None).flex(1.0),
        Face::Scrammed => word_face("SCRAMMED", Ink::Bad, None),
        Face::Off => word_face("REACTOR OFF", Ink::Fg2, None),
        Face::Implosion(eta) => word_face("IMPLOSION", Ink::Bad, eta.map(clock).as_deref()),
        Face::CoreEjected(eta) => word_face("CORE EJECTED", Ink::Warn, eta.map(clock).as_deref()),
        Face::Boot => with_body(header("BOOT", Tone::Title, 28.0, None), boot_panel(r)),
        Face::Prime => with_body(header("PRIME", Tone::Title, 28.0, None), prime_panel(r)),
        Face::Running => {
            let up = run_up_active(r, run_up);
            let (word, tone) = status_word(r, run_up);
            let size = if word.contains('\u{b7}') { 22.0 } else { 28.0 };
            let head = header(&word, tone, size, Some((tab, r.manual && !up)));
            let content = match tab {
                Tab::Control => row(GAP, vec![left_panel(r, up), right_panel(r, max_kw, run_up, up)]).fixed(BODY_H),
                Tab::History => history_panel(r, max_kw, run_up, up),
            };
            with_body(head, content)
        }
    }
}

// ---------- BOOT and PRIME ----------

/// A full-width plate of the start faces: border (line 2), the title header, then `shapes`.
fn start_panel(title: &str, mut shapes: Vec<Shape>) -> Item {
    let mut all = frame(BODY_W, BODY_H, Ink::Line2);
    all.extend(panel_head(BODY_W, title));
    all.append(&mut shapes);
    canvas(BODY_W, BODY_H, all, None).flex(1.0)
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

fn boot_panel(r: &ReactorNews) -> Item {
    let rows = boot_rows(r);
    let open = rows.iter().filter(|(_, _, met)| !met).count();
    let (head, tone) = if open == 0 { ("READY - START LEVER TO PRIME".to_string(), Ink::Good) } else { (format!("START NOT READY - {open} OPEN"), Ink::Warn) };
    let mut s = vec![text(IN_X, HEAD + 20.0, BODY_W - 2.0 * IN_X, 40.0, &head, 40.0, TextFace::DisplayBold, tone)];
    let top = HEAD + 20.0 + 40.0 + 14.0;
    for (i, (name, value, met)) in rows.iter().enumerate() {
        let y = top + i as f64 * 66.0;
        let tone = if *met { Ink::Good } else { Ink::Bad };
        s.push(rect(IN_X, y, BODY_W - 2.0 * IN_X, 60.0, Ink::Bg2));
        s.push(text(IN_X + 24.0, y, 700.0, 60.0, name, 28.0, TextFace::Display, Ink::Fg1));
        s.push(text_r(BODY_W - IN_X - 24.0 - 400.0, y, 400.0, 60.0, value, 32.0, TextFace::Mono, tone));
    }
    start_panel("START-UP", s)
}

/// The trap's charge as whole percent.
fn trap_pct(r: &ReactorNews) -> f64 {
    (fin(r.trap).clamp(0.0, 1.0) * 100.0).round()
}

fn prime_panel(r: &ReactorNews) -> Item {
    let charged = fin(r.trap) >= 1.0;
    let (x, w) = (IN_X + 4.0, BODY_W - 2.0 * IN_X - 8.0);
    // Head 44, gap 28, row 80, gap 28, bar 52: centred in the body under the header.
    let group = 44.0 + 28.0 + 80.0 + 28.0 + 52.0;
    let top = HEAD + (BODY_H - HEAD - ROW_FOOT - group) * 0.5;
    let mut s = Vec::new();
    if charged {
        s.push(text(x, top, w, 44.0, "TRAP CHARGED", 40.0, TextFace::DisplayBold, Ink::Good));
    } else {
        s.push(text(x, top, w, 44.0, "TRAP CHARGING", 20.0, TextFace::Display, Ink::Fg2));
    }
    let y = top + 44.0 + 28.0;
    s.push(text(x, y, w * 0.5, 80.0, &format!("{:.0} %", trap_pct(r)), 72.0, TextFace::Mono, if charged { Ink::Good } else { Ink::Fg2 }));
    if !charged {
        s.push(text_r(x + w * 0.5, y, w * 0.5, 80.0, &format!("{:.0} s", fin(r.trap_left_s).max(0.0).ceil()), 40.0, TextFace::Mono, Ink::Fg2));
    }
    let by = y + 80.0 + 28.0;
    s.push(boxed(x, by, w, 52.0, Ink::Bg3, Ink::HudDim, 2.0));
    s.push(rect(x + 2.0, by + 2.0, (w - 4.0) * trap_pct(r) / 100.0, 48.0, if charged { Ink::Good } else { Ink::Hud }));
    start_panel("MAGNETIC TRAP", s)
}

/// The main screen for the news (`None`: a dead screen). `max_kw` is the reactor's maximum output
/// (the history's scale); `run_up` is the sim's run-up stage; `tab` the client's tab (CONTROL or HISTORY).
pub fn panel(r: Option<&ReactorNews>, max_kw: f64, run_up: bool, tab: Tab) -> Panel {
    let f = face(r);
    let default = ReactorNews::default();
    let root = body(r.unwrap_or(&default), max_kw, run_up, tab, f);
    Panel::new(W, H, root).with_padding(B).with_ground(if f == Face::NoPower { Ink::Bg0 } else { Ink::Bg1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Pointer, Widget};
    use glam::DVec2;

    fn running() -> ReactorNews {
        let mut r = ReactorNews { powered: true, power_on: true, manual: true, heat: 50.0, injection: 46.0, set_injection: 46.0, converter: 64.0, set_converter: 64.0, output_kw: 1280.0, load_kw: 1280.0, ..Default::default() };
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
    fn the_panel_builds_for_every_state_and_both_tabs_with_finite_shapes() {
        let base = running();
        let mut cases = vec![("none", None), ("running", Some(base))];
        cases.push(("cold start", Some(ReactorNews { start: StartStage::Cold, ..base })));
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
        cases.push(("zero max kw", Some(base)));
        for run_up in [false, true] {
            for tab in [Tab::Control, Tab::History] {
                for (name, r) in &cases {
                    let max_kw = if *name == "zero max kw" { 0.0 } else { 2000.0 };
                    let p = panel(r.as_ref(), max_kw, run_up, tab);
                    assert_eq!(p.size, DVec2::new(W, H), "{name}");
                    assert!(p.overflowing().is_empty(), "{name} {tab:?}: {:?}", p.overflowing());
                    assert!(all_finite(&p), "{name} {tab:?}");
                    // Every canvas is laid out at the size it was drawn for.
                    for w in p.layout() {
                        if let Widget::Canvas { w: cw, h: ch, .. } = w.widget {
                            assert!((w.rect.size.x - cw).abs() < 0.5 && (w.rect.size.y - ch).abs() < 0.5, "{name} {tab:?}: canvas {cw}x{ch} laid out {:?}", w.rect.size);
                        }
                    }
                }
            }
        }
        for max_kw in [f64::NAN, f64::INFINITY, -5.0] {
            for tab in [Tab::Control, Tab::History] {
                let p = panel(Some(&base), max_kw, false, tab);
                assert!(p.overflowing().is_empty() && all_finite(&p), "max {max_kw} {tab:?}");
            }
        }
    }

    #[test]
    fn the_faces_follow_the_news() {
        let r = running();
        assert_eq!(face(Some(&r)), Face::Running);
        assert_eq!(face(None), Face::NoPower);
        assert_eq!(face(Some(&ReactorNews { start: StartStage::Cold, ..r })), Face::NoPower);
        assert_eq!(face(Some(&ReactorNews { powered: false, output_kw: 0.0, ..r })), Face::Running, "no output is no reason to go dark");
        assert_eq!(face(Some(&ReactorNews { power_on: false, ..r })), Face::Off);
        assert_eq!(face(Some(&ReactorNews { imploded: true, ..r })), Face::Implosion(None));
        assert_eq!(face(Some(&ReactorNews { ejected: true, ..r })), Face::CoreEjected(None));
        let mut late = ReactorNews { implosion_eta_s: Some(50.0), ..r };
        assert_eq!(face(Some(&late)), Face::Running, "a timer just started leaves the controls");
        late.lamps[Lamp::Implosion.index()] = true;
        assert_eq!(face(Some(&late)), Face::Implosion(Some(50.0)), "the countdown takes the screen at the implosion lamp");
        assert_eq!(face(Some(&ReactorNews { ejected: true, implosion_eta_s: Some(40.0), ..r })), Face::CoreEjected(Some(40.0)));
        let mut bad = ReactorNews { implosion_eta_s: Some(f64::NAN), ..r };
        bad.lamps[Lamp::Implosion.index()] = true;
        assert_eq!(face(Some(&bad)), Face::Running, "a bad eta is no countdown");
    }

    #[test]
    fn the_sliders_exist_only_in_the_running_face_and_work_in_manual_only() {
        let manual = panel(Some(&running()), 2000.0, false, Tab::Control);
        let auto = panel(Some(&ReactorNews { manual: false, ..running() }), 2000.0, false, Tab::Control);
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
        assert!(panel(Some(&ReactorNews { power_on: false, ..running() }), 2000.0, false, Tab::Control).find(INJECTION).is_none());
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
        let up = panel(Some(&ReactorNews { start: StartStage::RunUp, output_kw: 300.0, ..running() }), 2000.0, true, Tab::Control);
        for a in [INJECTION, CONVERTER] {
            let mut ptr = Pointer::default();
            let at = up.find(a).unwrap().rect.centre();
            assert!(ptr.step(&up, Some(at), true).is_empty(), "{a} is disabled in the run-up");
        }
        // Done (output at demand): the sliders work again in MANUAL.
        let done = panel(Some(&ReactorNews { start: StartStage::RunUp, ..running() }), 2000.0, true, Tab::Control);
        let mut ptr = Pointer::default();
        let at = done.find(INJECTION).unwrap().rect.centre();
        assert!(!ptr.step(&done, Some(at), true).is_empty());
    }

    #[test]
    fn the_tabs_are_buttons_and_the_history_tab_has_no_sliders() {
        let control = panel(Some(&running()), 2000.0, false, Tab::Control);
        let history = panel(Some(&running()), 2000.0, false, Tab::History);
        for p in [&control, &history] {
            for a in [TAB_CONTROL, TAB_HISTORY, AUTO, MANUAL] {
                assert!(p.hit(p.find(a).unwrap().rect.centre()).is_some(), "{a} answers");
            }
        }
        assert!(control.find(INJECTION).is_some() && history.find(INJECTION).is_none() && history.find(CONVERTER).is_none());
        // The selected tab is drawn pressed (tone Good).
        let tone = |p: &Panel, a: &str| match p.find(a).unwrap().widget {
            Widget::Button { tone, .. } => *tone,
            _ => unreachable!(),
        };
        assert_eq!((tone(&control, TAB_CONTROL), tone(&control, TAB_HISTORY)), (Tone::Good, Tone::Normal));
        assert_eq!((tone(&history, TAB_CONTROL), tone(&history, TAB_HISTORY)), (Tone::Normal, Tone::Good));
    }

    #[test]
    fn the_slider_value_follows_the_pointer_along_the_track() {
        let p = panel(Some(&running()), 2000.0, false, Tab::Control);
        let r = p.find(INJECTION).unwrap().rect;
        let mut ptr = Pointer::default();
        let clicks = ptr.step(&p, Some(r.at(DVec2::new(0.25, 0.5))), true);
        assert!(clicks.iter().any(|c| c.action == INJECTION && (c.value.unwrap() - 0.25).abs() < 1e-9));
    }

    #[test]
    fn the_word_faces_have_no_header_and_no_buttons() {
        let mut scram = ReactorNews { power_on: false, ..running() };
        scram.lamps[Lamp::Scram.index()] = true;
        for r in [scram, ReactorNews { power_on: false, ..running() }, ReactorNews { imploded: true, ..running() }, ReactorNews { ejected: true, implosion_eta_s: Some(61.0), ..running() }] {
            let p = panel(Some(&r), 2000.0, false, Tab::History);
            assert!(p.find(AUTO).is_none() && p.find(TAB_HISTORY).is_none());
        }
    }
}
