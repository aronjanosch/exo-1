//! The fabricator's built-in screen (#326): 480 x 280 panel pixels (the screen is about 0.24 x 0.14 m),
//! read-only. One face at a time, only what the machine is doing now: LOAD ORE, the recipe the load
//! makes and PUSH TO START, NO RECIPE, the product with a bar and the seconds left, NO POWER, TAKE
//! and the product. The glue turns the host's news into a `Face` with the item names.
use crate::canvas::{Ink, Shape, TextFace};
use crate::{canvas, Panel};

pub const W: f64 = 480.0;
pub const H: f64 = 280.0;
const PAD: f64 = 20.0;
/// Width of one mono or display character as a share of the text size (the fit's estimate).
const CHAR_W: f64 = 0.62;

#[derive(Clone, Debug, PartialEq)]
pub enum Face {
    /// Idle, nothing in the input places.
    Load,
    /// Idle, the load makes this: `from` the inputs ("2 GRUMBLITE") and `to` the product ("FUSE").
    Ready { from: String, to: String },
    NoRecipe,
    /// Running: the product, how far (0..1) and the seconds left.
    Running { product: String, fraction: f64, left_s: f64 },
    NoPower,
    /// The output place holds a product.
    Take { product: String },
}

/// The largest text size up to `max` at which `text` is about `width` wide.
fn fit(text: &str, width: f64, max: f64) -> f64 {
    (width / (text.chars().count().max(1) as f64 * CHAR_W)).min(max)
}

fn line(y: f64, h: f64, text: &str, max: f64, face: TextFace, ink: Ink) -> Shape {
    let w = W - 2.0 * PAD;
    Shape::Text { x: PAD, y, w, h, text: text.to_string(), size: fit(text, w, max), face, ink, centred: true }
}

fn bar(x: f64, y: f64, w: f64, h: f64, fraction: f64) -> Vec<Shape> {
    let f = if fraction.is_finite() { fraction.clamp(0.0, 1.0) } else { 0.0 };
    vec![
        Shape::Rect { x, y, w, h, fill: Some(Ink::Bg2), outline: Some((Ink::HudDim, 2.0)) },
        Shape::Rect { x: x + 4.0, y: y + 4.0, w: (w - 8.0) * f, h: h - 8.0, fill: Some(Ink::Good), outline: None },
    ]
}

fn shapes(face: &Face) -> Vec<Shape> {
    let mut s = vec![Shape::Rect { x: 0.0, y: 0.0, w: W, h: H, fill: Some(Ink::Bg0), outline: Some((Ink::Line2, 2.0)) }];
    s.push(line(14.0, 30.0, "FABRICATOR", 24.0, TextFace::Display, Ink::Fg3));
    match face {
        Face::Load => s.push(line(80.0, 120.0, "LOAD ORE", 64.0, TextFace::DisplayBold, Ink::Fg1)),
        Face::Ready { from, to } => {
            // A long recipe goes on two lines so it stays readable.
            let one = format!("{from} > {to}");
            if one.chars().count() <= 20 {
                s.push(line(70.0, 70.0, &one, 40.0, TextFace::Mono, Ink::Fg1));
            } else {
                s.push(line(56.0, 56.0, from, 38.0, TextFace::Mono, Ink::Fg1));
                s.push(line(114.0, 56.0, &format!("> {to}"), 38.0, TextFace::Mono, Ink::Fg1));
            }
            s.push(line(196.0, 50.0, "PUSH TO START", 38.0, TextFace::DisplayBold, Ink::Good));
        }
        Face::NoRecipe => s.push(line(80.0, 120.0, "NO RECIPE", 64.0, TextFace::DisplayBold, Ink::Warn)),
        Face::Running { product, fraction, left_s } => {
            s.push(line(62.0, 60.0, product, 40.0, TextFace::DisplayBold, Ink::Fg1));
            s.extend(bar(PAD, 138.0, W - 2.0 * PAD, 36.0, *fraction));
            let left = if left_s.is_finite() { left_s.max(0.0).ceil() } else { 0.0 };
            s.push(line(192.0, 60.0, &format!("{left:.0} S"), 48.0, TextFace::Mono, Ink::Good));
        }
        Face::NoPower => s.push(line(80.0, 120.0, "NO POWER", 64.0, TextFace::DisplayBold, Ink::Bad)),
        Face::Take { product } => {
            s.push(line(70.0, 60.0, "TAKE", 44.0, TextFace::DisplayBold, Ink::Good));
            s.push(line(140.0, 80.0, product, 48.0, TextFace::DisplayBold, Ink::Fg1));
        }
    }
    s
}

pub fn panel(face: &Face) -> Panel {
    Panel::new(W, H, canvas(W, H, shapes(face), None).flex(1.0)).with_padding(0.0)
}

/// Every word on the face, in drawing order (for the scenario and the tests).
pub fn words(face: &Face) -> Vec<String> {
    shapes(face).into_iter().filter_map(|s| if let Shape::Text { text, .. } = s { Some(text) } else { None }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn faces() -> Vec<Face> {
        vec![
            Face::Load,
            Face::Ready { from: "2 GRUMBLITE".into(), to: "FUSE".into() },
            Face::Ready { from: "3 WOBBLE QUARTZ".into(), to: "ANTIMATTER CANISTER".into() },
            Face::NoRecipe,
            Face::Running { product: "COOLANT CANISTER".into(), fraction: 0.4, left_s: 9.2 },
            Face::Running { product: "FUSE".into(), fraction: f64::NAN, left_s: f64::NAN },
            Face::NoPower,
            Face::Take { product: "ANTIMATTER CANISTER".into() },
        ]
    }

    #[test]
    fn every_face_fits_its_screen_and_has_no_button() {
        for f in faces() {
            let p = panel(&f);
            assert!(p.overflowing().is_empty(), "{f:?}: {:?}", p.overflowing());
            assert!(p.layout().iter().all(|w| w.widget.action().is_none()), "{f:?}: the screen is read-only");
        }
    }

    #[test]
    fn the_faces_say_what_the_machine_does() {
        let has = |f: &Face, w: &str| words(f).iter().any(|t| t == w);
        assert!(has(&Face::Load, "LOAD ORE") && has(&Face::NoRecipe, "NO RECIPE") && has(&Face::NoPower, "NO POWER"));
        let ready = Face::Ready { from: "2 GRUMBLITE".into(), to: "FUSE".into() };
        assert!(has(&ready, "2 GRUMBLITE > FUSE") && has(&ready, "PUSH TO START"));
        let run = Face::Running { product: "FUSE".into(), fraction: 0.5, left_s: 5.2 };
        assert!(has(&run, "FUSE") && has(&run, "6 S"), "{:?}", words(&run));
        assert!(has(&Face::Take { product: "FUSE".into() }, "TAKE"));
    }
}
