//! The reactor's slim supply screen `screen_supply` (#318, from the Claude Design "ReactorSupply"): 1280 x 140
//! panel pixels, read-only (no widget answers, the host gives it no cursor). Four cells in a row: SLOT 1 and
//! SLOT 2 (the canister in the slot, its fill in %), BATTERY (charge, what it takes or gives) and COOLANT (the
//! pump's flow). Each cell: name and state word on top, a big mono value, a low-is-bad bar below.
//!
//! Every number is the sim's (`ReactorNews`); the design's JS is only a preview. The screen goes dark where
//! the main screen does (`reactor_main::face`), so one rule says when the console has no power.
use crate::canvas::{Ink, Shape, TextFace};
use crate::reactor_main::{self, Face};
use crate::{canvas, Panel, Tone};
use item_core::wire::{CanisterSlot, ReactorNews, CANISTER_SLOTS};

pub const W: f64 = 1280.0;
pub const H: f64 = 140.0;

const GAP: f64 = 2.0;
const PAD_X: f64 = 18.0;
const PAD_Y: f64 = 14.0;
const CELLS: usize = 4;
const CELL_W: f64 = (W - GAP * (CELLS as f64 - 1.0)) / CELLS as f64;
const TICKS: usize = 24;

/// A fill under this many percent is LOW.
const LOW_PCT: f64 = 20.0;
const SLOT_WARN: f64 = 20.0;
const SLOT_CRIT: f64 = 5.0;
const BATTERY_WARN: f64 = 20.0;
const BATTERY_CRIT: f64 = 5.0;
const COOLANT_WARN: f64 = 30.0;
const COOLANT_CRIT: f64 = 10.0;
/// Below this flow (the sim's 0..1) the coolant is no flow at all.
const NO_FLOW: f64 = 0.005;
/// Under this many kW the battery counts as neither charging nor carrying.
const KW_EPS: f64 = 0.5;

/// What one cell shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub name: String,
    pub state: String,
    pub state_tone: Tone,
    pub value: String,
    pub value_tone: Tone,
    /// 0..100.
    pub fill: f64,
    pub warn: f64,
    pub crit: f64,
    pub enabled: bool,
}

fn fin(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

fn pct(v: f64) -> f64 {
    fin(v).clamp(0.0, 100.0)
}

pub fn slot_cell(index: usize, slot: &CanisterSlot) -> Cell {
    let name = format!("SLOT {}", index + 1);
    if slot.canister.is_none() {
        return Cell { name, state: "EMPTY".into(), state_tone: Tone::Bad, value: "-".into(), value_tone: Tone::Dim, fill: 0.0, warn: SLOT_WARN, crit: SLOT_CRIT, enabled: false };
    }
    let fill = pct(slot.life);
    let low = fill < LOW_PCT;
    Cell {
        name,
        state: if low { "LOW" } else { "SEATED" }.into(),
        state_tone: if low { Tone::Warn } else { Tone::Dim },
        value: format!("{fill:.0} %"),
        value_tone: if low { Tone::Warn } else { Tone::Normal },
        fill,
        warn: SLOT_WARN,
        crit: SLOT_CRIT,
        enabled: true,
    }
}

pub fn battery_cell(r: &ReactorNews) -> Cell {
    let charge = pct(fin(r.battery_charge) * 100.0);
    let (out, input) = (fin(r.battery_kw).max(0.0), fin(r.battery_in_kw).max(0.0));
    // The battery is the only source when it gives while the reactor's output is nothing.
    let alone = out > KW_EPS && fin(r.output_kw) <= KW_EPS;
    let (state, tone) = if alone {
        ("ON BATTERY".to_string(), Tone::Warn)
    } else if out > KW_EPS {
        (format!("OUT -{out:.0} kW"), Tone::Warn)
    } else if input > KW_EPS {
        (format!("IN +{input:.0} kW"), Tone::Dim)
    } else {
        ("IDLE".to_string(), Tone::Dim)
    };
    let value_tone = if charge < BATTERY_CRIT {
        Tone::Bad
    } else if charge < BATTERY_WARN {
        Tone::Warn
    } else {
        Tone::Normal
    };
    Cell { name: "BATTERY".into(), state, state_tone: tone, value: format!("{charge:.0} %"), value_tone, fill: charge, warn: BATTERY_WARN, crit: BATTERY_CRIT, enabled: true }
}

pub fn coolant_cell(r: &ReactorNews) -> Cell {
    let flow = fin(r.flow).max(0.0);
    if flow < NO_FLOW {
        return Cell { name: "COOLANT".into(), state: "NO COOLING".into(), state_tone: Tone::Bad, value: "0 L/s".into(), value_tone: Tone::Bad, fill: 0.0, warn: COOLANT_WARN, crit: COOLANT_CRIT, enabled: true };
    }
    // The sim's flow is 0..1 of the pump's full flow; the old pump screen reads it as flow x 100.
    let lps = pct(flow * 100.0);
    Cell { name: "COOLANT".into(), state: "FLOW".into(), state_tone: Tone::Dim, value: format!("{lps:.0} L/s"), value_tone: Tone::Normal, fill: lps, warn: COOLANT_WARN, crit: COOLANT_CRIT, enabled: true }
}

/// The four cells, left to right.
pub fn cells(r: &ReactorNews) -> [Cell; CELLS] {
    let [a, b] = std::array::from_fn::<_, CANISTER_SLOTS, _>(|i| slot_cell(i, &r.canisters[i]));
    [a, b, battery_cell(r), coolant_cell(r)]
}

fn ink_of(t: Tone, plain: Ink) -> Ink {
    match t {
        Tone::Bad => Ink::Bad,
        Tone::Warn => Ink::Warn,
        Tone::Good => Ink::Good,
        Tone::Dim => Ink::Fg3,
        _ => plain,
    }
}

fn text(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, ink: Ink, centred: bool) -> Shape {
    Shape::Text { x, y, w, h, text: s.to_string(), size, face, ink, centred }
}

/// The bar of the Bar widget's look, low is bad: 24 ticks, every fifth full height, the lit ones in the
/// zone colour of their position, marks under the bar at the warn and critical lines.
fn bar_shapes(x: f64, y: f64, w: f64, h: f64, c: &Cell) -> Vec<Shape> {
    let marks = 6.0;
    let bh = h - marks;
    let mut s = vec![
        Shape::Rect { x, y, w: 2.0, h: bh, fill: Some(Ink::HudDim), outline: None },
        Shape::Rect { x: x + w - 2.0, y, w: 2.0, h: bh, fill: Some(Ink::HudDim), outline: None },
        Shape::Rect { x, y: y + bh - 2.0, w, h: 2.0, fill: Some(Ink::HudDim), outline: None },
    ];
    let inner = w - 4.0 - 6.0;
    let tw = (inner - 2.0 * (TICKS as f64 - 1.0)) / TICKS as f64;
    let f = c.fill / 100.0;
    for i in 0..TICKS {
        let t = (i as f64 + 0.5) / TICKS as f64 * 100.0;
        let (zone, share) = if t <= c.crit {
            (Ink::Bad, 1.0)
        } else if t <= c.warn {
            (Ink::Warn, 0.8)
        } else {
            (Ink::Good, 0.6)
        };
        let share = if i % 5 == 0 { 1.0 } else { share };
        let th = (bh - 5.0) * share;
        let lit = c.enabled && (i as f64 + 0.5) / TICKS as f64 <= f;
        let ink = if lit { zone } else { Ink::Bg3 };
        s.push(Shape::Rect { x: x + 5.0 + i as f64 * (tw + 2.0), y: y + bh - 2.0 - th, w: tw, h: th, fill: Some(ink), outline: None });
    }
    if c.enabled {
        for (t, ink) in [(c.warn, Ink::Warn), (c.crit, Ink::Bad)] {
            s.push(Shape::Rect { x: x + t / 100.0 * w - 3.0, y: y + bh + 1.0, w: 6.0, h: marks - 1.0, fill: Some(ink), outline: None });
        }
    }
    s
}

fn cell_shapes(i: usize, c: &Cell) -> Vec<Shape> {
    let x0 = i as f64 * (CELL_W + GAP);
    let inner = CELL_W - 2.0 * PAD_X;
    let mut s = vec![Shape::Rect { x: x0, y: 0.0, w: CELL_W, h: H, fill: Some(Ink::Bg1), outline: None }];
    s.push(text(x0 + PAD_X, PAD_Y, inner * 0.5, 24.0, &c.name, 20.0, TextFace::Display, Ink::Fg2, false));
    // The state word sits at the right: a box from the middle of the cell, right aligned by its own width estimate.
    let word_w = (c.state.chars().count() as f64 * 12.5).min(inner);
    s.push(text(x0 + PAD_X + inner - word_w, PAD_Y, word_w, 24.0, &c.state, 20.0, TextFace::Display, ink_of(c.state_tone, Ink::Fg1), true));
    s.push(text(x0 + PAD_X, PAD_Y + 26.0, inner, 46.0, &c.value, 40.0, TextFace::Mono, ink_of(c.value_tone, Ink::Fg1), false));
    s.extend(bar_shapes(x0 + PAD_X, PAD_Y + 76.0, inner, 36.0, c));
    s
}

/// The screen for the news (`None`, or a dead main screen: dark).
pub fn panel(r: Option<&ReactorNews>) -> Panel {
    let shapes = match r.filter(|r| reactor_main::face(Some(r)) != Face::NoPower) {
        Some(r) => cells(r).iter().enumerate().flat_map(|(i, c)| cell_shapes(i, c)).collect(),
        None => Vec::new(),
    };
    Panel::new(W, H, canvas(W, H, shapes, None).flex(1.0)).with_padding(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Widget;
    use item_core::wire::{CanisterKind, StartStage};

    fn running() -> ReactorNews {
        let mut r = ReactorNews { powered: true, power_on: true, output_kw: 1200.0, flow: 0.8, battery_charge: 0.5, ..Default::default() };
        r.canisters = [CanisterSlot { canister: Some(CanisterKind::Hot), life: 80.0 }, CanisterSlot { canister: Some(CanisterKind::Spicy), life: 12.0 }];
        r
    }

    fn all_finite(p: &Panel) -> bool {
        p.layout().iter().all(|w| match w.widget {
            Widget::Canvas { shapes, .. } => shapes.iter().all(|s| {
                let (a, b, c, d) = s.extent();
                a.is_finite() && b.is_finite() && c.is_finite() && d.is_finite()
            }),
            _ => true,
        })
    }

    #[test]
    fn the_panel_builds_for_every_state() {
        let base = running();
        let nan = f64::NAN;
        let inf = f64::INFINITY;
        let cases = [
            ("none", None),
            ("running", Some(base)),
            ("cold start", Some(ReactorNews { start: StartStage::Cold, ..base })),
            ("no canisters", Some(ReactorNews { canisters: Default::default(), ..base })),
            ("empty battery", Some(ReactorNews { battery_charge: 0.0, ..base })),
            ("stopped pump", Some(ReactorNews { flow: 0.0, pump_on: false, ..base })),
            ("nan", Some(ReactorNews { flow: nan, battery_charge: nan, battery_kw: nan, battery_in_kw: nan, output_kw: nan, canisters: [CanisterSlot { canister: Some(CanisterKind::Hot), life: nan }; 2], ..base })),
            ("inf", Some(ReactorNews { flow: inf, battery_charge: inf, battery_kw: inf, battery_in_kw: -inf, canisters: [CanisterSlot { canister: Some(CanisterKind::Hot), life: inf }; 2], ..base })),
        ];
        for (name, r) in cases {
            let p = panel(r.as_ref());
            assert_eq!(p.size, glam::DVec2::new(W, H), "{name}");
            assert!(p.overflowing().is_empty(), "{name}: {:?}", p.overflowing());
            assert!(all_finite(&p), "{name}");
        }
    }

    #[test]
    fn the_cells_say_what_the_sim_says() {
        let r = running();
        let c = cells(&r);
        assert_eq!((c[0].state.as_str(), c[0].state_tone, c[0].value.as_str()), ("SEATED", Tone::Dim, "80 %"));
        assert_eq!((c[1].state.as_str(), c[1].state_tone, c[1].value_tone), ("LOW", Tone::Warn, Tone::Warn));
        let empty = slot_cell(0, &CanisterSlot::default());
        assert_eq!((empty.state.as_str(), empty.state_tone, empty.value.as_str(), empty.enabled), ("EMPTY", Tone::Bad, "-", false));
        assert_eq!(battery_cell(&r).state, "IDLE");
        assert_eq!(battery_cell(&ReactorNews { battery_in_kw: 60.0, ..r }).state, "IN +60 kW");
        assert_eq!(battery_cell(&ReactorNews { battery_kw: 120.0, ..r }).state, "OUT -120 kW");
        let alone = battery_cell(&ReactorNews { battery_kw: 120.0, output_kw: 0.0, ..r });
        assert_eq!((alone.state.as_str(), alone.state_tone), ("ON BATTERY", Tone::Warn));
        assert_eq!(battery_cell(&ReactorNews { battery_charge: 0.03, ..r }).value_tone, Tone::Bad);
        assert_eq!(battery_cell(&ReactorNews { battery_charge: 0.15, ..r }).value_tone, Tone::Warn);
        assert_eq!(coolant_cell(&r).value, "80 L/s");
        let none = coolant_cell(&ReactorNews { flow: 0.0, ..r });
        assert_eq!((none.state.as_str(), none.value.as_str(), none.value_tone), ("NO COOLING", "0 L/s", Tone::Bad));
    }

    #[test]
    fn the_screen_is_dark_without_power_and_has_nothing_to_press() {
        let r = running();
        let dark = panel(Some(&ReactorNews { start: StartStage::Cold, ..r }));
        assert!(dark.layout().iter().all(|w| !matches!(w.widget, Widget::Canvas { shapes, .. } if !shapes.is_empty())));
        let lit = panel(Some(&r));
        assert!(lit.layout().iter().all(|w| matches!(w.widget, Widget::Canvas { action: None, .. } | Widget::Row { .. } | Widget::Column { .. } | Widget::Space)));
    }
}
