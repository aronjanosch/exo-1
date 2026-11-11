//! The dispatch chute's slate (#334): 440 x 322 panel pixels (the slate is about 0.30 x 0.22 m), read-only.
//! Three faces: the current order (a strip `ORDER 4` / `DONE 3`, up to three lines `3 GRUMBLITE` each with a box at its
//! right that holds a tick once the chute has enough; a long name goes on two rows), the DONE flash after a send, and the
//! order again with the unmet lines blinking after a refused send. The glue turns the host's news into a `Face`.
use crate::canvas::{Ink, Shape, TextFace};
use crate::{canvas, Panel};

pub const W: f64 = 440.0;
pub const H: f64 = 322.0;
const PAD: f64 = 18.0;
/// Width of one mono character as a share of the text size (the fit's estimate).
const CHAR_W: f64 = 0.62;
/// Read from 1.5 m (#337): at 1.47 panel pixels per mm the order lines are 26 mm and more, the strip above them 24 mm.
const STRIP_H: f64 = 52.0;
const ROW_Y: f64 = 58.0;
const ROW_H: f64 = 88.0;
const BOX: f64 = 52.0;
/// The size of a line's words; a name too long for one line at `WRAP_BELOW` goes on two.
const LINE_MAX: f64 = 48.0;
const WRAP_BELOW: f64 = 33.0;
const STRIP_SIZE: f64 = 36.0;

/// One order line: the words (`3 GRUMBLITE`) and whether the chute holds enough.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub met: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Face {
    /// The order to deliver; `done` orders are sent so far (the order's number is one more).
    Order { number: u32, lines: Vec<Line>, done: u32 },
    /// A send went through: the order's number.
    Done { number: u32, done: u32 },
    /// A send was refused: the order, its unmet lines drawn only while `blink_on`.
    Missing { number: u32, lines: Vec<Line>, done: u32, blink_on: bool },
}

fn fit(text: &str, width: f64, max: f64) -> f64 {
    (width / (text.chars().count().max(1) as f64 * CHAR_W)).min(max)
}

fn centred(y: f64, h: f64, text: &str, max: f64, face: TextFace, ink: Ink) -> Shape {
    let w = W - 2.0 * PAD;
    Shape::Text { x: PAD, y, w, h, text: text.to_string(), size: fit(text, w, max), face, ink, centred: true }
}

/// The strip on top: the order's number on the left, the orders sent so far on the right.
fn strip(s: &mut Vec<Shape>, number: u32, done: u32) {
    let w = (W - 2.0 * PAD) * 0.5;
    s.push(Shape::Text { x: PAD, y: 4.0, w, h: STRIP_H, text: format!("ORDER {number}"), size: STRIP_SIZE, face: TextFace::DisplayBold, ink: Ink::Fg1, centred: false });
    s.push(Shape::TextRight { x: PAD + w, y: 4.0, w, h: STRIP_H, text: format!("DONE {done}"), size: STRIP_SIZE, face: TextFace::Display, ink: Ink::Fg1 });
}

/// The words of a line: one row of text, or two when the name is too long for one at a readable size (split at the space
/// that balances the halves best).
fn words_of(text: &str, w: f64) -> Vec<(String, f64)> {
    let one = fit(text, w, LINE_MAX);
    let spaces: Vec<usize> = text.match_indices(' ').map(|(i, _)| i).collect();
    if one >= WRAP_BELOW || spaces.is_empty() {
        return vec![(text.to_string(), one)];
    }
    let mid = text.chars().count() / 2;
    let cut = spaces.into_iter().min_by_key(|i| (text[..*i].chars().count() as i64 - mid as i64).abs()).unwrap();
    let (a, b) = (text[..cut].to_string(), text[cut + 1..].to_string());
    let size = fit(if a.chars().count() >= b.chars().count() { &a } else { &b }, w, WRAP_BELOW + 4.0);
    vec![(a, size), (b, size)]
}

/// One row: the words on the left, the box with its tick on the right.
fn row(s: &mut Vec<Shape>, i: usize, line: &Line, unmet_ink: Ink, show: bool) {
    let y = ROW_Y + i as f64 * ROW_H;
    let bx = W - PAD - BOX;
    let by = y + (ROW_H - BOX) * 0.5;
    let ink = if line.met { Ink::Good } else { unmet_ink };
    s.push(Shape::Rect { x: bx, y: by, w: BOX, h: BOX, fill: Some(if line.met { Ink::GoodDim } else { Ink::Bg2 }), outline: Some((ink, 4.0)) });
    if line.met {
        s.push(Shape::Line { a: [bx + 10.0, by + 27.0], b: [bx + 22.0, by + 40.0], width: 5.0, ink: Ink::Good, dash: None });
        s.push(Shape::Line { a: [bx + 22.0, by + 40.0], b: [bx + 43.0, by + 12.0], width: 5.0, ink: Ink::Good, dash: None });
    }
    if show {
        let w = W - 2.0 * PAD - BOX - 14.0;
        let rows = words_of(&line.text, w);
        let h = ROW_H / rows.len() as f64;
        for (k, (text, size)) in rows.into_iter().enumerate() {
            s.push(Shape::Text { x: PAD, y: y + k as f64 * h, w, h, text, size, face: TextFace::Mono, ink: if line.met { Ink::Fg1 } else { unmet_ink }, centred: false });
        }
    }
}

fn shapes(face: &Face) -> Vec<Shape> {
    let mut s = vec![Shape::Rect { x: 0.0, y: 0.0, w: W, h: H, fill: Some(Ink::Bg0), outline: Some((Ink::Line2, 2.0)) }];
    match face {
        Face::Order { number, lines, done } => {
            strip(&mut s, *number, *done);
            for (i, l) in lines.iter().take(MAX_LINES).enumerate() {
                row(&mut s, i, l, Ink::Fg1, true);
            }
        }
        Face::Done { number, done } => {
            s.push(centred(30.0, 140.0, "DONE", 120.0, TextFace::DisplayBold, Ink::Good));
            s.push(centred(180.0, 56.0, &format!("ORDER {number} SENT"), 40.0, TextFace::Mono, Ink::Fg1));
            s.push(centred(H - 62.0, 50.0, &format!("DONE {done}"), 38.0, TextFace::Display, Ink::Fg1));
        }
        Face::Missing { number, lines, done, blink_on } => {
            strip(&mut s, *number, *done);
            for (i, l) in lines.iter().take(MAX_LINES).enumerate() {
                row(&mut s, i, l, Ink::Bad, l.met || *blink_on);
            }
        }
    }
    s
}

/// The most lines the slate has room for.
pub const MAX_LINES: usize = 3;

pub fn panel(face: &Face) -> Panel {
    Panel::new(W, H, canvas(W, H, shapes(face), None).flex(1.0)).with_padding(0.0)
}

/// Every word on the face, in drawing order (for the scenario and the tests).
pub fn words(face: &Face) -> Vec<String> {
    shapes(face).into_iter().filter_map(|s| match s {
        Shape::Text { text, .. } | Shape::TextRight { text, .. } => Some(text),
        _ => None,
    }).collect()
}

/// How many tick strokes the face draws: one pair per met line (the scenario reads the live ticks off the screen).
pub fn ticks(face: &Face) -> usize {
    shapes(face).iter().filter(|s| matches!(s, Shape::Line { width, .. } if *width == 5.0)).count() / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(t: &str, met: bool) -> Line {
        Line { text: t.into(), met }
    }

    fn faces() -> Vec<Face> {
        let lines = vec![line("3 WOBBLE QUARTZ", true), line("1 ANTIMATTER CANISTER", false), line("2 FIZZ JAR", false)];
        vec![
            Face::Order { number: 1, lines: vec![line("3 GRUMBLITE", false)], done: 0 },
            Face::Order { number: 12, lines: lines.clone(), done: 11 },
            Face::Done { number: 4, done: 4 },
            Face::Missing { number: 4, lines: lines.clone(), done: 3, blink_on: true },
            Face::Missing { number: 4, lines, done: 3, blink_on: false },
        ]
    }

    #[test]
    fn every_face_fits_its_screen_and_has_no_button() {
        for f in faces() {
            let p = panel(&f);
            assert!(p.overflowing().is_empty(), "{f:?}: {:?}", p.overflowing());
            assert!(p.layout().iter().all(|w| w.widget.action().is_none()), "{f:?}: the slate is read-only");
        }
    }

    #[test]
    fn the_faces_say_what_the_chute_wants() {
        let has = |f: &Face, w: &str| words(f).iter().any(|t| t == w);
        let order = Face::Order { number: 4, lines: vec![line("3 GRUMBLITE", true), line("1 FUSE", false)], done: 3 };
        assert!(has(&order, "ORDER 4") && has(&order, "3 GRUMBLITE") && has(&order, "1 FUSE") && has(&order, "DONE 3"));
        assert_eq!(ticks(&order), 1, "one line is met");
        let done = Face::Done { number: 4, done: 4 };
        assert!(has(&done, "DONE") && has(&done, "ORDER 4 SENT") && has(&done, "DONE 4"));
    }

    #[test]
    fn a_missing_line_blinks_and_a_met_one_stays() {
        let lines = vec![line("3 GRUMBLITE", true), line("1 FUSE", false)];
        let has = |blink_on, w: &str| words(&Face::Missing { number: 1, lines: lines.clone(), done: 0, blink_on }).iter().any(|t| t == w);
        assert!(has(true, "1 FUSE") && !has(false, "1 FUSE"));
        assert!(has(true, "3 GRUMBLITE") && has(false, "3 GRUMBLITE"));
    }
}
