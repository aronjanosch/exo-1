//! The airlock wall panel's screen (#308, from the Claude Design "Airlock Wall Panel"): a portrait
//! 400 x 640 px texture (200 x 320 panel pixels at the world's texture scale 2) with four faces:
//! rest inside, rest outside (the whole plate is the PUSH button), CYCLING (one looping animation,
//! no button) and UNPOWERED (dead glass). Design units are texture pixels; `U` turns them into panel pixels.
use crate::raster::{self as r, Pt, Prim};
use crate::{canvas, Item, Panel, Shape, TextFace};

/// Panel pixels per design pixel.
const U: f64 = 0.5;
pub const W: f64 = 200.0;
pub const H: f64 = 320.0;
/// The press on the plate.
pub const PUSH: &str = "push";

const BG: [u8; 3] = [0x03, 0x05, 0x07];
const LINE2: [u8; 3] = [0x4d, 0x66, 0x76];
const BLUE: [u8; 3] = [0x6f, 0xb8, 0xff];
const PLATE: [u8; 3] = [0x1a, 0x25, 0x2e];
const STRIPE_A: [u8; 3] = [0x1c, 0x3a, 0x55];
const STRIPE_B: [u8; 3] = [0x11, 0x19, 0x20];
const RIM: [u8; 3] = [0x2b, 0x3c, 0x48];
const FG2: [u8; 3] = [0x8f, 0xa9, 0xa1];
const WHITE: [u8; 3] = [0xff, 0xff, 0xff];
const GLOW: [u8; 3] = [0xdc, 0xf2, 0xea];
const DASH: [u8; 3] = [0x72, 0x85, 0x80];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Face {
    /// At rest; `inside`: the airlock rests inside the ship (PUSH goes out, ▶).
    Idle { inside: bool },
    /// Cycling; `t` seconds drive the loop.
    Cycling { t: f64 },
    Unpowered,
}

fn shift(pts: &[Pt], o: Pt) -> Vec<Pt> {
    pts.iter().map(|p| [p[0] + o[0], p[1] + o[1]]).collect()
}

fn text(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, rgb: [u8; 3], tracking: f64, centred: bool) -> Shape {
    Shape::SpacedText { x: x * U, y: y * U, w: w * U, h: h * U, text: s.to_string(), size: size * U, face, rgb, tracking, centred }
}

fn frame() -> Vec<Prim> {
    vec![
        r::rect(0.0, 0.0, 400.0, 640.0, LINE2),
        r::rect(2.0, 2.0, 396.0, 636.0, BG),
    ]
}

fn triangle(x: f64, y: f64, w: f64, h: f64, right: bool, up: bool, rgb: [u8; 3]) -> Prim {
    let pts: [Pt; 3] = if up {
        [[x, y + h], [x + w, y + h], [x + w * 0.5, y]]
    } else if right {
        [[x, y], [x + w, y + h * 0.5], [x, y + h]]
    } else {
        [[x + w, y], [x, y + h * 0.5], [x + w, y + h]]
    };
    r::fill(&pts, rgb)
}

fn idle(inside: bool) -> Vec<Shape> {
    let o = [16.0, 16.0];
    let mut prims = frame();
    prims.push(r::fill(&shift(&[[0.0, 0.0], [324.0, 0.0], [368.0, 44.0], [368.0, 604.0], [44.0, 604.0], [0.0, 560.0]], o), BLUE));
    prims.push(r::fill(&shift(&[[6.0, 6.0], [322.0, 6.0], [362.0, 46.0], [362.0, 598.0], [46.0, 598.0], [6.0, 558.0]], o), PLATE));
    // The hazard band: 45 degree stripes, 10 px across, cut to the band.
    let band = shift(&[[20.0, 572.0], [362.0, 572.0], [362.0, 598.0], [46.0, 598.0]], o);
    prims.push(r::fill(&band, STRIPE_B));
    let across = 10.0 * std::f64::consts::SQRT_2;
    let mut stripes = Vec::new();
    let mut c = 580.0 + o[0] + o[1] - 400.0;
    while c < 1500.0 {
        // The strip between the lines x + y = c and x + y = c + across.
        let strip: Vec<Pt> = vec![[0.0, c], [c, 0.0], [c + across, 0.0], [0.0, c + across]];
        let cut = r::clip_convex(&strip, &band);
        if cut.len() >= 3 {
            stripes.push(cut);
        }
        c += 2.0 * across;
    }
    prims.push(Prim { polys: stripes, rgb: STRIPE_A, alpha: 255 });
    prims.push(r::stroke(&shift(&[[22.0, 26.0], [318.0, 26.0], [342.0, 50.0], [342.0, 548.0]], o), false, 2.0, None, 0.0, RIM));
    // Chevrons: three triangles, 28 px stack gaps around PUSH.
    let cy = 226.0;
    for i in 0..3 {
        prims.push(triangle(151.0 + i as f64 * 40.0, cy - 10.0, 18.0, 20.0, inside, false, BLUE));
    }
    vec![
        Shape::Vector { x: 0.0, y: 0.0, w: W, h: H, unit: U, prims },
        text(16.0, 280.0, 368.0, 104.0, "PUSH", 104.0, TextFace::DisplayBold, WHITE, 0.06, true),
        text(16.0, 400.0, 368.0, 24.0, "TO BEGIN CYCLING", 24.0, TextFace::Display, FG2, 0.14, true),
    ]
}

fn cycling(t: f64) -> Vec<Shape> {
    let mut prims = frame();
    prims.push(r::rect(16.0, 16.0, 368.0, 604.0, LINE2));
    prims.push(r::rect(18.0, 18.0, 364.0, 600.0, STRIPE_B));
    // The 300 x 300 graphic at (50, 134), centre (200, 284).
    let (cx, cy) = (200.0, 284.0);
    let g = [50.0, 134.0];
    prims.push(r::ring(cx, cy, 120.0, 4.0, None, STRIPE_A));
    for off in [0.0, 0.5] {
        let u = (t / 1.6 + off).rem_euclid(1.0);
        prims.push(r::ring(cx, cy, 30.0 + 100.0 * u, 4.0, None, BLUE).with_alpha(1.0 - u));
    }
    prims.push(r::ring(cx, cy, 138.0, 3.0, Some((4.0, 14.0)), DASH).rotated(cx, cy, -360.0 * t / 8.0));
    prims.push(r::disc(cx, cy, 104.0, STRIPE_B));
    prims.push(r::ring(cx, cy, 104.0, 6.0, None, LINE2));
    prims.push(r::ring(cx, cy, 90.0, 3.0, Some((3.0, 9.0)), RIM));
    let at = |x: f64, y: f64| [g[0] + x, g[1] + y];
    let mut blade = r::bezier(at(150.0, 150.0), at(124.0, 118.0), at(134.0, 66.0), at(184.0, 62.0), 16);
    blade.extend(r::bezier(at(184.0, 62.0), at(206.0, 96.0), at(190.0, 138.0), at(150.0, 150.0), 16).into_iter().skip(1));
    let vein = r::bezier(at(152.0, 140.0), at(140.0, 118.0), at(150.0, 92.0), at(176.0, 80.0), 12);
    let spin = 360.0 * t / 0.9;
    for k in 0..5 {
        let a = spin + 72.0 * k as f64;
        prims.push(r::fill(&blade, BLUE).rotated(cx, cy, a));
        prims.push(r::stroke(&blade, true, 4.0, None, 0.0, STRIPE_B).rotated(cx, cy, a));
        prims.push(r::stroke(&vein, false, 4.0, None, 0.0, STRIPE_A).rotated(cx, cy, a));
    }
    prims.push(r::disc(cx, cy, 20.0, GLOW));
    prims.push(r::ring(cx, cy, 20.0, 5.0, None, STRIPE_B));
    prims.push(r::disc(cx, cy, 7.0, PLATE));
    // "▲ CYCLING": the triangle is drawn (the font has no ▲), the text starts after it.
    prims.push(triangle(89.0, 468.0, 24.0, 22.0, false, true, GLOW));
    vec![
        Shape::Vector { x: 0.0, y: 0.0, w: W, h: H, unit: U, prims },
        text(125.0, 458.0, 260.0, 42.0, "CYCLING", 36.0, TextFace::DisplayBold, GLOW, 0.14, false),
    ]
}

fn unpowered() -> Vec<Shape> {
    let mut prims = frame();
    let rect: Vec<Pt> = vec![[16.0, 16.0], [384.0, 16.0], [384.0, 624.0], [16.0, 624.0]];
    prims.push(r::stroke(&rect, true, 2.0, Some((8.0, 6.0)), 0.0, [0x16, 0x22, 0x2b]));
    prims.push(r::ring(200.0, 292.0, 24.0, 5.0, None, [0x1c, 0x2a, 0x33]));
    vec![
        Shape::Vector { x: 0.0, y: 0.0, w: W, h: H, unit: U, prims },
        text(16.0, 336.0, 368.0, 36.0, "NO POWER", 28.0, TextFace::DisplayBold, [0x33, 0x45, 0x4f], 0.14, true),
    ]
}

/// The panel for a face. Only a resting airlock has the PUSH action; the whole plate is its hit area.
pub fn panel(face: Face) -> Panel {
    let (shapes, action) = match face {
        Face::Idle { inside } => (idle(inside), Some(PUSH)),
        Face::Cycling { t } => (cycling(t), None),
        Face::Unpowered => (unpowered(), None),
    };
    let root: Item = canvas(W, H, shapes, action).flex(1.0);
    Panel::new(W, H, root).with_padding(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_has_one_push_over_the_whole_plate_and_fits() {
        for inside in [true, false] {
            let p = panel(Face::Idle { inside });
            let push = p.find(PUSH).expect("PUSH");
            assert_eq!((push.rect.size.x, push.rect.size.y), (W, H));
            assert!(p.overflowing().is_empty(), "{:?}", p.overflowing());
        }
        assert_ne!(panel(Face::Idle { inside: true }), panel(Face::Idle { inside: false }));
    }

    #[test]
    fn cycling_and_dead_glass_have_no_button_and_fit() {
        for f in [Face::Cycling { t: 1.3 }, Face::Unpowered] {
            let p = panel(f);
            assert!(p.find(PUSH).is_none());
            assert!(p.overflowing().is_empty(), "{:?}", p.overflowing());
        }
    }
}
