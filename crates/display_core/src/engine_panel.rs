//! The engine room's wall panel (#313, the airlock panel's kind, #308): a portrait 400 x 640 px texture
//! (200 x 320 panel pixels at the world's texture scale 2). One page: one large field (LIGHTS ON / OFF,
//! the only thing to press) and two read-only rows, a word and a short bar each: HEAT and AIR. Actual values
//! only. Unpowered: dark glass with a dim NO POWER. Design units are texture pixels; `U` turns them into panel pixels.
//! All words, colours and lines are starting values. TODO(initiator): look, words and the lines below.
use crate::raster::{self as r, Prim};
use crate::{canvas, column, Item, Panel, Shape, TextFace};

/// Panel pixels per design pixel.
const U: f64 = 0.5;
pub const W: f64 = 200.0;
pub const H: f64 = 320.0;
/// The press on the light field.
pub const LIGHTS: &str = "lights";

/// Design heights of the two bands of the page (they add up to the 640 of the texture). Read from 1.5 m (#337): no title
/// band (the cursor prompt names the panel), the type 26 to 60 mm tall on the 0.20 x 0.32 m glass, the words as large as their width allows.
const FIELD_H: f64 = 232.0;
const ROW_H: f64 = 204.0;

/// The air's share (0..1) below which the word is THIN (`monitor::o2_tone`'s warn line) and below which it is NONE.
/// TODO(initiator): the lines.
pub const AIR_THIN_BELOW: f64 = 0.6;
pub const AIR_NONE_BELOW: f64 = 0.05;

const BG: [u8; 3] = [0x03, 0x05, 0x07];
const LINE2: [u8; 3] = [0x4d, 0x66, 0x76];
const BLUE: [u8; 3] = [0x6f, 0xb8, 0xff];
const PLATE: [u8; 3] = [0x1a, 0x25, 0x2e];
const TRACK: [u8; 3] = [0x11, 0x19, 0x20];
const FG2: [u8; 3] = [0x8f, 0xa9, 0xa1];
const WHITE: [u8; 3] = [0xff, 0xff, 0xff];
const OK: [u8; 3] = [0x6f, 0xe0, 0xa0];
const WARN: [u8; 3] = [0xff, 0xb0, 0x30];
const BAD: [u8; 3] = [0xff, 0x4a, 0x3a];

/// The chamber heat in the main screen's bands: above the optimal band's top it is HOT, above the allowed band's top CRITICAL.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Heat {
    Ok,
    Hot,
    Critical,
}

impl Heat {
    pub fn of(heat: f64, optimal_hi: f64, allowed_hi: f64) -> Heat {
        if heat > allowed_hi {
            Heat::Critical
        } else if heat > optimal_hi {
            Heat::Hot
        } else {
            Heat::Ok
        }
    }
    pub fn word(self) -> &'static str {
        match self {
            Heat::Ok => "OK",
            Heat::Hot => "HOT",
            Heat::Critical => "CRITICAL",
        }
    }
    fn rgb(self) -> [u8; 3] {
        match self {
            Heat::Ok => OK,
            Heat::Hot => WARN,
            Heat::Critical => BAD,
        }
    }
}

/// The room's oxygen fraction in words.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Air {
    Ok,
    Thin,
    None,
}

impl Air {
    /// A share that is not a number reads as NONE.
    pub fn of(share: f64) -> Air {
        if !(share >= AIR_NONE_BELOW) {
            Air::None
        } else if share < AIR_THIN_BELOW {
            Air::Thin
        } else {
            Air::Ok
        }
    }
    pub fn word(self) -> &'static str {
        match self {
            Air::Ok => "OK",
            Air::Thin => "THIN",
            Air::None => "NONE",
        }
    }
    fn rgb(self) -> [u8; 3] {
        match self {
            Air::Ok => OK,
            Air::Thin => WARN,
            Air::None => BAD,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Face {
    /// Powered: the lamps are on or switched off by hand; the heat as a word and a bar fill (0..1 of the gauge), the air the same.
    Live { lights_on: bool, heat: Heat, heat_fill: f64, air: Air, air_fill: f64 },
    /// The room's junction box passes no power: dead glass, the field does nothing.
    Unpowered,
}

fn text(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, rgb: [u8; 3], tracking: f64, centred: bool) -> Shape {
    Shape::SpacedText { x: x * U, y: y * U, w: w * U, h: h * U, text: s.to_string(), size: size * U, face, rgb, tracking, centred }
}

/// One band of the page: its box in design pixels, the side lines of the frame, and the shapes on top.
fn band(h: f64, mut prims: Vec<Prim>, mut shapes: Vec<Shape>, action: Option<&str>) -> Item {
    let mut all = vec![r::rect(0.0, 0.0, 400.0, h, LINE2), r::rect(2.0, 0.0, 396.0, h, BG)];
    all.append(&mut prims);
    let mut layers = vec![Shape::Vector { x: 0.0, y: 0.0, w: W, h: h * U, unit: U, prims: all, spin: 0.0 }];
    layers.append(&mut shapes);
    canvas(W, h * U, layers, action).fixed(h * U)
}

/// The largest size (design px) at which `word` fits `width`, up to `max`: a bold display character is about 0.66 of its size wide with tracking.
fn fit(word: &str, width: f64, max: f64) -> f64 {
    (width / (word.chars().count().max(1) as f64 * 0.66)).min(max)
}

fn field(on: bool) -> Item {
    let (word, rgb, edge) = if on { ("ON", WHITE, BLUE) } else { ("OFF", FG2, LINE2) };
    let prims = vec![r::rect(16.0, 16.0, 368.0, FIELD_H - 24.0, edge), r::rect(22.0, 22.0, 356.0, FIELD_H - 36.0, PLATE)];
    let shapes = vec![
        text(16.0, 30.0, 368.0, 64.0, "LIGHTS", 56.0, TextFace::DisplayBold, WHITE, 0.08, true),
        text(16.0, 96.0, 368.0, 120.0, word, 112.0, TextFace::DisplayBold, rgb, 0.06, true),
    ];
    band(FIELD_H, prims, shapes, Some(LIGHTS))
}

/// One read-only row: the label, the word in its colour and a short bar with its fill.
fn read_row(y: f64, label: &str, word: &str, rgb: [u8; 3], fill: f64, prims: &mut Vec<Prim>, shapes: &mut Vec<Shape>) {
    let fill = if fill.is_finite() { fill.clamp(0.0, 1.0) } else { 0.0 };
    shapes.push(text(24.0, y + 8.0, 352.0, 60.0, label, 52.0, TextFace::DisplayBold, WHITE, 0.06, false));
    shapes.push(text(24.0, y + 66.0, 352.0, 100.0, word, fit(word, 352.0, 88.0), TextFace::DisplayBold, rgb, 0.04, false));
    prims.push(r::rect(24.0, y + 170.0, 352.0, 26.0, LINE2));
    prims.push(r::rect(26.0, y + 172.0, 348.0, 22.0, TRACK));
    if fill > 0.0 {
        prims.push(r::rect(26.0, y + 172.0, 348.0 * fill, 22.0, rgb));
    }
}

fn rows(heat: Heat, heat_fill: f64, air: Air, air_fill: f64) -> Item {
    let h = 2.0 * ROW_H;
    let mut prims = vec![r::rect(0.0, ROW_H - 2.0, 400.0, 2.0, LINE2), r::rect(0.0, h - 2.0, 400.0, 2.0, LINE2)];
    let mut shapes = Vec::new();
    read_row(0.0, "HEAT", heat.word(), heat.rgb(), heat_fill, &mut prims, &mut shapes);
    read_row(ROW_H, "AIR", air.word(), air.rgb(), air_fill, &mut prims, &mut shapes);
    band(h, prims, shapes, None)
}

fn unpowered() -> Item {
    let prims = vec![r::rect(0.0, 0.0, 400.0, 2.0, LINE2), r::rect(0.0, 638.0, 400.0, 2.0, LINE2), r::ring(200.0, 292.0, 24.0, 5.0, None, [0x1c, 0x2a, 0x33])];
    let shapes = vec![text(16.0, 336.0, 368.0, 36.0, "NO POWER", 28.0, TextFace::DisplayBold, [0x33, 0x45, 0x4f], 0.14, true)];
    band(640.0, prims, shapes, None)
}

/// The panel for a face. Only a powered room has the LIGHTS action; the field is its hit area.
pub fn panel(face: Face) -> Panel {
    let root = match face {
        Face::Live { lights_on, heat, heat_fill, air, air_fill } => column(0.0, vec![field(lights_on), rows(heat, heat_fill, air, air_fill)]),
        Face::Unpowered => column(0.0, vec![unpowered()]),
    };
    Panel::new(W, H, root).with_padding(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(lights_on: bool) -> Face {
        Face::Live { lights_on, heat: Heat::Ok, heat_fill: 0.45, air: Air::Ok, air_fill: 0.9 }
    }

    #[test]
    fn the_page_has_one_press_on_the_field_and_fits() {
        for on in [true, false] {
            let p = panel(live(on));
            let f = p.find(LIGHTS).expect("LIGHTS");
            assert_eq!(f.rect.size.x, W);
            assert!(f.rect.size.y < H * 0.5, "the field is one band, not the plate");
            assert!(p.overflowing().is_empty(), "{:?}", p.overflowing());
        }
        assert_ne!(panel(live(true)), panel(live(false)), "the field shows the state");
    }

    #[test]
    fn dead_glass_has_no_press_and_fits() {
        let p = panel(Face::Unpowered);
        assert!(p.find(LIGHTS).is_none());
        assert!(p.overflowing().is_empty(), "{:?}", p.overflowing());
    }

    #[test]
    fn the_rows_follow_the_values_and_read_no_trend() {
        let ok = panel(live(true));
        for (heat, air) in [(Heat::Hot, Air::Ok), (Heat::Critical, Air::Thin), (Heat::Ok, Air::None)] {
            let p = panel(Face::Live { lights_on: true, heat, heat_fill: 0.8, air, air_fill: 0.1 });
            assert_ne!(p, ok);
            assert!(p.find(LIGHTS).is_some() && p.overflowing().is_empty());
        }
        // A fill that is not a number or out of range draws nothing odd.
        for fill in [f64::NAN, -3.0, 9.0] {
            let p = panel(Face::Live { lights_on: true, heat: Heat::Ok, heat_fill: fill, air: Air::Ok, air_fill: fill });
            assert!(p.overflowing().is_empty());
        }
    }

    #[test]
    fn the_words_come_from_the_bands() {
        assert_eq!((Heat::of(40.0, 60.0, 80.0), Heat::of(60.0, 60.0, 80.0)), (Heat::Ok, Heat::Ok), "at the line is still OK");
        assert_eq!((Heat::of(70.0, 60.0, 80.0), Heat::of(80.0, 60.0, 80.0)), (Heat::Hot, Heat::Hot));
        assert_eq!(Heat::of(80.1, 60.0, 80.0), Heat::Critical);
        assert_eq!((Heat::Ok.word(), Heat::Hot.word(), Heat::Critical.word()), ("OK", "HOT", "CRITICAL"));
        assert_eq!((Air::of(1.0), Air::of(0.3), Air::of(0.0), Air::of(f64::NAN)), (Air::Ok, Air::Thin, Air::None, Air::None));
        assert_eq!((Air::Ok.word(), Air::Thin.word(), Air::None.word()), ("OK", "THIN", "NONE"));
    }
}
