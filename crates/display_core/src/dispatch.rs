//! The dispatch chute's slate (#334): 440 x 322 panel pixels (the slate is about 0.30 x 0.22 m), read-only.
//! Three faces: the current order (header `ORDER 4`, up to three lines `3 GRUMBLITE` each with a box at its
//! right that holds a tick once the chute has enough, footer `DONE 3`), the DONE flash after a send, and the
//! order again with the unmet lines blinking after a refused send. The glue turns the host's news into a `Face`.
use crate::canvas::{Ink, Shape, TextFace};
use crate::{canvas, Panel};

pub const W: f64 = 440.0;
pub const H: f64 = 322.0;
const PAD: f64 = 22.0;
/// Width of one mono character as a share of the text size (the fit's estimate).
const CHAR_W: f64 = 0.62;
const ROW_Y: f64 = 78.0;
const ROW_H: f64 = 62.0;
const BOX: f64 = 40.0;

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

/// One row: the words on the left, the box with its tick on the right.
fn row(s: &mut Vec<Shape>, i: usize, line: &Line, unmet_ink: Ink, show: bool) {
    let y = ROW_Y + i as f64 * ROW_H;
    let bx = W - PAD - BOX;
    let by = y + (ROW_H - BOX) * 0.5;
    let ink = if line.met { Ink::Good } else { unmet_ink };
    s.push(Shape::Rect { x: bx, y: by, w: BOX, h: BOX, fill: Some(if line.met { Ink::GoodDim } else { Ink::Bg2 }), outline: Some((ink, 3.0)) });
    if line.met {
        s.push(Shape::Line { a: [bx + 8.0, by + 21.0], b: [bx + 17.0, by + 31.0], width: 5.0, ink: Ink::Good, dash: None });
        s.push(Shape::Line { a: [bx + 17.0, by + 31.0], b: [bx + 33.0, by + 9.0], width: 5.0, ink: Ink::Good, dash: None });
    }
    if show {
        let w = W - 2.0 * PAD - BOX - 16.0;
        s.push(Shape::Text { x: PAD, y, w, h: ROW_H, text: line.text.clone(), size: fit(&line.text, w, 42.0), face: TextFace::Mono, ink: if line.met { Ink::Fg1 } else { unmet_ink }, centred: false });
    }
}

fn shapes(face: &Face) -> Vec<Shape> {
    let mut s = vec![Shape::Rect { x: 0.0, y: 0.0, w: W, h: H, fill: Some(Ink::Bg0), outline: Some((Ink::Line2, 2.0)) }];
    let footer = |s: &mut Vec<Shape>, done: u32| s.push(centred(H - 50.0, 38.0, &format!("DONE {done}"), 30.0, TextFace::Display, Ink::Fg3));
    match face {
        Face::Order { number, lines, done } => {
            s.push(centred(10.0, 60.0, &format!("ORDER {number}"), 48.0, TextFace::DisplayBold, Ink::Fg1));
            for (i, l) in lines.iter().take(MAX_LINES).enumerate() {
                row(&mut s, i, l, Ink::Fg1, true);
            }
            footer(&mut s, *done);
        }
        Face::Done { number, done } => {
            s.push(centred(40.0, 130.0, "DONE", 110.0, TextFace::DisplayBold, Ink::Good));
            s.push(centred(190.0, 50.0, &format!("ORDER {number} SENT"), 34.0, TextFace::Mono, Ink::Fg1));
            footer(&mut s, *done);
        }
        Face::Missing { number, lines, done, blink_on } => {
            s.push(centred(10.0, 60.0, &format!("ORDER {number}"), 48.0, TextFace::DisplayBold, Ink::Fg1));
            for (i, l) in lines.iter().take(MAX_LINES).enumerate() {
                row(&mut s, i, l, Ink::Bad, l.met || *blink_on);
            }
            footer(&mut s, *done);
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
    shapes(face).into_iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text) } else { None }).collect()
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
