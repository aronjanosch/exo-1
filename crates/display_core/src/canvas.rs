//! Angles are degrees on screen: 0 points right, 90 down (y grows downwards), so an increasing
//! angle turns clockwise as the player sees it. A point is `(cx + r cos a, cy + r sin a)`.
use crate::theme::Rgb;
use crate::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Hud,
    HudDim,
    Line1,
    Line2,
    Bg0,
    Bg1,
    Bg3,
    Fg1,
    Fg2,
    Fg3,
    FgTitle,
    Good,
    Warn,
    Bad,
    GoodDim,
    WarnDim,
    BadDim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFace {
    Display,
    DisplayBold,
    Mono,
}

/// One mark. `dash` is (on, off) in pixels along the stroke, None for a solid stroke.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A stroke along a circle of radius `r` from angle `from` to `to` (degrees; to may be above
    /// 360 for a sweep past the top).
    Arc { cx: f64, cy: f64, r: f64, from: f64, to: f64, stroke: f64, ink: Ink, dash: Option<(f64, f64)> },
    Line { a: [f64; 2], b: [f64; 2], width: f64, ink: Ink, dash: Option<(f64, f64)> },
    Circle { cx: f64, cy: f64, r: f64, fill: Option<Ink>, stroke: Option<(Ink, f64)> },
    Rect { x: f64, y: f64, w: f64, h: f64, fill: Option<Ink>, outline: Option<(Ink, f64)> },
    Hatch { x: f64, y: f64, w: f64, h: f64, ink: Ink },
    /// A text in a box, vertically centred; `centred` centres it across the box, else left.
    Text { x: f64, y: f64, w: f64, h: f64, text: String, size: f64, face: TextFace, ink: Ink, centred: bool },
}

/// The extent of a shape: (min x, min y, max x, max y), in canvas pixels.
pub type Extent = (f64, f64, f64, f64);

impl Shape {
    pub fn extent(&self) -> Extent {
        match self {
            Shape::Arc { cx, cy, r, stroke, .. } => (cx - r - stroke * 0.5, cy - r - stroke * 0.5, cx + r + stroke * 0.5, cy + r + stroke * 0.5),
            Shape::Line { a, b, width, .. } => (a[0].min(b[0]) - width * 0.5, a[1].min(b[1]) - width * 0.5, a[0].max(b[0]) + width * 0.5, a[1].max(b[1]) + width * 0.5),
            // The outline lies inside the radius.
            Shape::Circle { cx, cy, r, .. } => (cx - r, cy - r, cx + r, cy + r),
            Shape::Rect { x, y, w, h, .. } | Shape::Hatch { x, y, w, h, .. } | Shape::Text { x, y, w, h, .. } => (*x, *y, x + w, y + h),
        }
    }

    /// Whether the shape lies inside a canvas of `w` by `h` pixels (with a half pixel of slack for
    /// strokes at the edge).
    pub fn inside(&self, w: f64, h: f64) -> bool {
        let (x0, y0, x1, y1) = self.extent();
        let e = 0.5;
        x0 >= -e && y0 >= -e && x1 <= w + e && y1 <= h + e
    }
}

impl Theme {
    pub fn ink(&self, i: Ink) -> Rgb {
        let c = &self.colours;
        match i {
            Ink::Hud => c.hud,
            Ink::HudDim => c.hud_dim,
            Ink::Line1 => c.line_1,
            Ink::Line2 => c.line_2,
            Ink::Bg0 => c.bg_0,
            Ink::Bg1 => c.bg_1,
            Ink::Bg3 => c.bg_3,
            Ink::Fg1 => c.fg_1,
            Ink::Fg2 => c.fg_2,
            Ink::Fg3 => c.fg_3,
            Ink::FgTitle => c.fg_title,
            Ink::Good => c.good,
            Ink::Warn => c.warn,
            Ink::Bad => c.bad,
            Ink::GoodDim => c.good_dim,
            Ink::WarnDim => c.warn_dim,
            Ink::BadDim => c.bad_dim,
        }
    }
}

/// The points of an arc, for a check or a polyline: `n` steps from `from` to `to`.
pub fn arc_points(cx: f64, cy: f64, r: f64, from: f64, to: f64, n: usize) -> Vec<[f64; 2]> {
    (0..=n)
        .map(|i| {
            let a = (from + (to - from) * i as f64 / n.max(1) as f64).to_radians();
            [cx + r * a.cos(), cy + r * a.sin()]
        })
        .collect()
}
