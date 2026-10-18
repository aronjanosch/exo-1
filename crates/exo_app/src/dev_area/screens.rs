//! The screens' shared kit: the dark NO POWER and OFF faces, the tall S canvas, the drawn lever, the
//! text shape and the oxygen's tone, and `show`, which every owner's panel goes through.
use crate::dev_area::*;
use crate::displays::Panels;
use display_core::{canvas, Ink, Item, Panel, Shape, Tier, TextFace, Tone};
use item_core::wire::ReactorNews;


/// Sets a panel; while the ship's master is off (the reactor's cold start, #319), the screen shows OFF.
pub fn show(panels: &mut Panels, r: Option<&ReactorNews>, id: display_core::PanelId, p: Panel) {
    let live = r.map_or(true, |r| r.start != item_core::wire::StartStage::Cold);
    panels.set(id, if live { p } else { off(p) });
}

pub fn text(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, ink: Ink, centred: bool) -> Shape {
    Shape::Text { x, y, w, h, text: s.to_string(), size, face, ink, centred }
}

pub fn no_power(title: &str, tag: &str, tier: Tier) -> Panel {
    let size = tier.canvas();
    let (w, h) = (size.x - 40.0, size.y - 80.0);
    // A ring above the word (drawn, not a glyph: the display font has no ○).
    let ring = Shape::Circle { cx: w * 0.5, cy: h * 0.5 - 36.0, r: 14.0, fill: None, stroke: Some((Ink::Fg3, 3.0)) };
    let shapes = vec![Shape::Rect { x: 0.0, y: 0.0, w, h, fill: Some(Ink::Bg0), outline: None }, ring, text(0.0, 12.0, w, h - 12.0, "NO POWER", 28.0, TextFace::DisplayBold, Ink::Fg3, true)];
    Panel::screen(tier, title, tag, canvas(w, h, shapes, None).flex(1.0))
}

pub fn powered(r: Option<&ReactorNews>) -> bool {
    r.is_some_and(|r| r.powered)
}

/// A lever's track and handle, px. The room's levers use `LEVER`; the junction box packs six of
/// them on one screen (r39: two rows of three in the same 260 px), so its track is `LEVER_PACKED`.
/// TODO(initiator): the packed look, not seen in play yet.
#[derive(Clone, Copy)]
pub struct Lever {
    pub track: f64,
    pub handle: f64,
}

pub const LEVER: Lever = Lever { track: 124.0, handle: 34.0 };

pub const LEVER_PACKED: Lever = Lever { track: 24.0, handle: 20.0 };

pub fn lever_in(w: f64, geo: Lever, lines: &[&str], on: bool, action: &str, name_ink: Ink) -> Item {
    // The names are set in lines of 17 px, bottom-aligned on the slot's top (y 36): a name too wide
    // for the lever is split over two lines rather than cut.
    let cx = w / 2.0;
    let mut s: Vec<Shape> = lines.iter().enumerate().map(|(i, l)| text(0.0, 36.0 - 17.0 * (lines.len() - i) as f64, w, 17.0, l, 16.0, TextFace::Display, name_ink, true)).collect();
    s.push(Shape::Rect { x: cx - 11.0, y: 36.0, w: 22.0, h: geo.track, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) });
    // The handle sits at the track's top when on, its bottom when off.
    let hy = if on { 36.0 } else { 36.0 + geo.track - geo.handle };
    let ink = if on { Ink::Hud } else { Ink::Warn };
    s.push(Shape::Rect { x: cx - 24.0, y: hy, w: 48.0, h: geo.handle, fill: Some(ink), outline: Some((Ink::FgTitle, 2.0)) });
    s.push(text(cx - 24.0, hy, 48.0, geo.handle, if on { "ON" } else { "OFF" }, 16.0, TextFace::DisplayBold, Ink::Bg0, true));
    canvas(w, 44.0 + geo.track, s, Some(action)).flex(1.0)
}

pub const TALL_CANVAS: f64 = 320.0;

pub fn tall(mut p: Panel) -> Panel {
    p.size.y = TALL_CANVAS;
    p
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

/// The master is off (r42, #268): the screen goes dark under its plate, its status says OFF. Every
/// screen goes through this in `write_panels`.
pub fn off(p: Panel) -> Panel {
    let (w, h) = (p.size.x - 40.0, p.size.y - 80.0);
    let shapes = vec![Shape::Rect { x: 0.0, y: 0.0, w, h, fill: Some(Ink::Bg0), outline: None }, text(0.0, h * 0.5 - 10.0, w, 20.0, "OFF", 22.0, TextFace::DisplayBold, Ink::Fg3, true)];
    let plate = p.plate.map(|pl| display_core::Plate { status: Some(("OFF".to_string(), Tone::Dim)), ..pl });
    Panel { plate, root: canvas(w, h, shapes, None).flex(1.0), ..p }
}
