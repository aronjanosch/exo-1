//! display_core: what a display panel shows and how a pointer works on it, without engine types
//! (#285). A `Panel` is a size in panel pixels and a tree of `Widget`s (labels, buttons, bars,
//! toggles, sliders in rows and columns). `Panel::layout` stacks them into rectangles, a `Pointer`
//! turns a position and a held button into hover, press and `Click`s with the widget's action id,
//! and `ray_to_panel` finds where a ray (the view) meets a display's quad in the world.
//!
//! Panel pixels: origin top left, x right, y down (like screen UI). A role builds its panel each
//! tick from its own state; widgets are plain data, so the panel can be compared, drawn on the
//! screen and hit-tested headless the same way.
use glam::{DQuat, DVec2, DVec3};
use std::borrow::Cow;

// ---------- ids ----------

/// Which panel: a short stable name (`test_console`). Const-constructible for role code.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PanelId(pub Cow<'static, str>);

impl PanelId {
    pub const fn new(s: &'static str) -> PanelId {
        PanelId(Cow::Borrowed(s))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for PanelId {
    fn from(s: String) -> PanelId {
        PanelId(Cow::Owned(s))
    }
}

impl std::fmt::Display for PanelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ---------- widgets ----------

/// Colour role of a label. The glue maps it to colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Normal,
    Dim,
    Title,
    Good,
    Warn,
    Bad,
}

/// A bar's state against its thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Normal,
    Warn,
    Crit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    /// Text; `size` is the font size in panel pixels.
    Label { text: String, tone: Tone, size: f64 },
    /// A press and release over it gives a click with `action` (no value). Disabled: no hover, no click.
    Button { label: String, enabled: bool, action: String },
    /// A gauge 0..1 with optional thresholds (`level`).
    Bar { value: f64, label: String, warn: Option<f64>, crit: Option<f64> },
    /// A click gives `action` with the new state as the value (1 on, 0 off).
    Toggle { on: bool, label: String, action: String },
    /// Pressed and dragged it gives `action` with the value 0..1 under the pointer, at the press
    /// and at every change while held.
    Slider { value: f64, label: String, action: String },
    Row { gap: f64, items: Vec<Item> },
    Column { gap: f64, items: Vec<Item> },
    /// Takes room, draws nothing.
    Space,
}

impl Widget {
    /// The action id of a widget the pointer can use (enabled buttons, toggles, sliders).
    pub fn action(&self) -> Option<&str> {
        match self {
            Widget::Button { enabled: true, action, .. } | Widget::Toggle { action, .. } | Widget::Slider { action, .. } => Some(action),
            _ => None,
        }
    }
}

/// A bar's level: with `warn <= crit` high values are bad (heat), with `warn > crit` low values are
/// bad (fuel). One threshold alone counts as high-is-bad.
pub fn level(value: f64, warn: Option<f64>, crit: Option<f64>) -> Level {
    let low_is_bad = matches!((warn, crit), (Some(w), Some(c)) if w > c);
    let past = |t: Option<f64>| t.is_some_and(|t| if low_is_bad { value <= t } else { value >= t });
    if past(crit) {
        Level::Crit
    } else if past(warn) {
        Level::Warn
    } else {
        Level::Normal
    }
}

/// Room along the parent's stacking axis: fixed pixels, or a share of what is left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    Fixed(f64),
    Flex(f64),
}

/// A widget with its size in the parent's stack.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub widget: Widget,
    pub size: Size,
}

impl Item {
    pub fn fixed(mut self, px: f64) -> Item {
        self.size = Size::Fixed(px);
        self
    }
    pub fn flex(mut self, weight: f64) -> Item {
        self.size = Size::Flex(weight);
        self
    }
    /// A button that can or cannot be pressed (other widgets ignore it).
    pub fn enabled(mut self, on: bool) -> Item {
        if let Widget::Button { enabled, .. } = &mut self.widget {
            *enabled = on;
        }
        self
    }
    /// A bar's thresholds (other widgets ignore them).
    pub fn thresholds(mut self, warn_at: Option<f64>, crit_at: Option<f64>) -> Item {
        if let Widget::Bar { warn, crit, .. } = &mut self.widget {
            (*warn, *crit) = (warn_at, crit_at);
        }
        self
    }
    /// A label's colour role (other widgets ignore it).
    pub fn tone(mut self, t: Tone) -> Item {
        if let Widget::Label { tone, .. } = &mut self.widget {
            *tone = t;
        }
        self
    }
}

fn item(widget: Widget) -> Item {
    Item { widget, size: Size::Flex(1.0) }
}

/// Body text, 20 px.
pub fn label(text: impl Into<String>) -> Item {
    item(Widget::Label { text: text.into(), tone: Tone::Normal, size: 20.0 })
}
/// A heading, 30 px.
pub fn title(text: impl Into<String>) -> Item {
    item(Widget::Label { text: text.into(), tone: Tone::Title, size: 30.0 })
}
pub fn button(label: impl Into<String>, action: impl Into<String>) -> Item {
    item(Widget::Button { label: label.into(), enabled: true, action: action.into() })
}
pub fn bar(value: f64, label: impl Into<String>) -> Item {
    item(Widget::Bar { value, label: label.into(), warn: None, crit: None })
}
pub fn toggle(on: bool, label: impl Into<String>, action: impl Into<String>) -> Item {
    item(Widget::Toggle { on, label: label.into(), action: action.into() })
}
pub fn slider(value: f64, label: impl Into<String>, action: impl Into<String>) -> Item {
    item(Widget::Slider { value, label: label.into(), action: action.into() })
}
pub fn row(gap: f64, items: Vec<Item>) -> Item {
    item(Widget::Row { gap, items })
}
pub fn column(gap: f64, items: Vec<Item>) -> Item {
    item(Widget::Column { gap, items })
}
pub fn space() -> Item {
    item(Widget::Space)
}

// ---------- layout ----------

/// A rectangle in panel pixels (or screen pixels for `Fit`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: DVec2,
    pub size: DVec2,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { min: DVec2::new(x, y), size: DVec2::new(w, h) }
    }
    pub fn max(&self) -> DVec2 {
        self.min + self.size
    }
    pub fn centre(&self) -> DVec2 {
        self.min + self.size * 0.5
    }
    /// Inside or on the edge.
    pub fn contains(&self, p: DVec2) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max()).all()
    }
    /// The point at `t` (0..1 per axis) inside the rectangle.
    pub fn at(&self, t: DVec2) -> DVec2 {
        self.min + self.size * t
    }
}

/// A leaf widget and where it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed<'a> {
    pub rect: Rect,
    pub widget: &'a Widget,
}

/// What a display shows: a size in panel pixels, the padding inside the edge, and the widgets.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub size: DVec2,
    pub padding: f64,
    pub root: Item,
}

impl Panel {
    /// A panel of `w` by `h` panel pixels with 16 px padding.
    pub fn new(w: f64, h: f64, root: Item) -> Panel {
        Panel { size: DVec2::new(w, h), padding: 16.0, root }
    }

    /// Every leaf widget with its rectangle, in tree order (rows and columns are not listed).
    pub fn layout(&self) -> Vec<Placed<'_>> {
        let mut out = Vec::new();
        let p = self.padding;
        place(&self.root.widget, Rect::new(p, p, (self.size.x - 2.0 * p).max(0.0), (self.size.y - 2.0 * p).max(0.0)), &mut out);
        out
    }

    /// The usable widget under a point, if any.
    pub fn hit(&self, p: DVec2) -> Option<Placed<'_>> {
        self.layout().into_iter().find(|w| w.widget.action().is_some() && w.rect.contains(p))
    }

    /// The first widget with this action (enabled or not).
    pub fn find(&self, action: &str) -> Option<Placed<'_>> {
        self.layout().into_iter().find(|w| match w.widget {
            Widget::Button { action: a, .. } | Widget::Toggle { action: a, .. } | Widget::Slider { action: a, .. } => a == action,
            _ => false,
        })
    }
}

fn place<'a>(w: &'a Widget, r: Rect, out: &mut Vec<Placed<'a>>) {
    let (items, gap, axis) = match w {
        Widget::Row { gap, items } => (items, *gap, 0),
        Widget::Column { gap, items } => (items, *gap, 1),
        Widget::Space => return,
        _ => {
            out.push(Placed { rect: r, widget: w });
            return;
        }
    };
    let length = if axis == 0 { r.size.x } else { r.size.y };
    let gaps = gap * items.len().saturating_sub(1) as f64;
    let fixed: f64 = items.iter().map(|i| if let Size::Fixed(px) = i.size { px } else { 0.0 }).sum();
    let weights: f64 = items.iter().map(|i| if let Size::Flex(f) = i.size { f.max(0.0) } else { 0.0 }).sum();
    let free = (length - gaps - fixed).max(0.0);
    let mut at = 0.0;
    for i in items {
        let len = match i.size {
            Size::Fixed(px) => px,
            Size::Flex(f) if weights > 0.0 => free * f.max(0.0) / weights,
            Size::Flex(_) => 0.0,
        };
        let rect = if axis == 0 { Rect::new(r.min.x + at, r.min.y, len, r.size.y) } else { Rect::new(r.min.x, r.min.y + at, r.size.x, len) };
        place(&i.widget, rect, out);
        at += len + gap;
    }
}

// ---------- pointer ----------

/// A use of a widget: its action id and, for toggles and sliders, the value.
#[derive(Clone, Debug, PartialEq)]
pub struct Click {
    pub action: String,
    pub value: Option<f64>,
}

/// The pointer on one panel: feed it every tick with the position (panel pixels, None when off the
/// panel) and whether the button is held.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pointer {
    pub at: Option<DVec2>,
    /// The usable widget under the pointer.
    pub hover: Option<String>,
    /// The widget the button went down on, while it is held.
    pub pressed: Option<String>,
    down: bool,
    slider_last: Option<f64>,
}

/// A slider's value at a pointer position (0..1 along it, clamped).
pub fn slider_value(rect: &Rect, p: DVec2) -> f64 {
    if rect.size.x <= 0.0 {
        return 0.0;
    }
    ((p.x - rect.min.x) / rect.size.x).clamp(0.0, 1.0)
}

impl Pointer {
    /// One tick. A button or toggle clicks when the button goes down and comes up over the same
    /// widget; a slider reports its value at the press and whenever it changes while held.
    pub fn step(&mut self, panel: &Panel, at: Option<DVec2>, down: bool) -> Vec<Click> {
        self.at = at;
        let hit = at.and_then(|p| panel.hit(p));
        self.hover = hit.and_then(|h| h.widget.action()).map(str::to_string);
        let mut out = Vec::new();
        match (self.down, down) {
            (false, true) => {
                self.pressed = self.hover.clone();
                self.slider_last = None;
            }
            (true, false) => {
                if let Some(p) = self.pressed.take()
                    && let Some(h) = hit
                    && h.widget.action() == Some(p.as_str())
                {
                    match h.widget {
                        Widget::Button { .. } => out.push(Click { action: p, value: None }),
                        Widget::Toggle { on, .. } => out.push(Click { action: p, value: Some(if *on { 0.0 } else { 1.0 }) }),
                        _ => {}
                    }
                }
                self.slider_last = None;
            }
            _ => {}
        }
        // A held slider follows the pointer, also beside it (clamped).
        if down
            && let (Some(p), Some(at)) = (&self.pressed, at)
            && let Some(s) = panel.find(p).filter(|s| matches!(s.widget, Widget::Slider { .. }))
        {
            let v = slider_value(&s.rect, at);
            if self.slider_last.is_none_or(|l| (l - v).abs() > 1e-9) {
                self.slider_last = Some(v);
                out.push(Click { action: p.clone(), value: Some(v) });
            }
        }
        self.down = down;
        out
    }
}

// ---------- screen and world ----------

/// A panel drawn on the screen: its rectangle in screen pixels and the scale from panel pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    pub rect: Rect,
    pub scale: f64,
}

impl Fit {
    /// The panel centred on a screen, `share` of the screen's height tall (narrower if the screen
    /// is not wide enough: then 95 % of its width).
    pub fn centred(screen: DVec2, panel: DVec2, share: f64) -> Fit {
        let mut scale = screen.y * share / panel.y.max(1.0);
        if panel.x * scale > screen.x * 0.95 {
            scale = screen.x * 0.95 / panel.x.max(1.0);
        }
        let size = panel * scale;
        Fit { rect: Rect { min: (screen - size) * 0.5, size }, scale }
    }
    pub fn to_panel(&self, screen: DVec2) -> DVec2 {
        (screen - self.rect.min) / self.scale
    }
    pub fn to_screen(&self, panel: DVec2) -> DVec2 {
        self.rect.min + panel * self.scale
    }
}

/// Where a ray meets a panel's quad: texture coordinates (u right, v down, 0..1) and the distance
/// along the ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelHit {
    pub uv: DVec2,
    pub distance: f64,
}

/// A ray against a panel's quad in the world. The quad is centred on `centre`, `size_m` wide (local
/// x, right as seen from the front) and tall (local y, up); its front faces local +z. Only the front
/// counts; a ray from behind, parallel or pointing away misses.
pub fn ray_to_panel(origin: DVec3, dir: DVec3, centre: DVec3, rot: DQuat, size_m: DVec2) -> Option<PanelHit> {
    let n = rot * DVec3::Z;
    let along = dir.dot(n);
    if along >= -1e-12 {
        return None;
    }
    let t = (centre - origin).dot(n) / along;
    if t < 0.0 {
        return None;
    }
    let local = rot.inverse() * (origin + dir * t - centre);
    let uv = DVec2::new(local.x / size_m.x + 0.5, 0.5 - local.y / size_m.y);
    if !(0.0..=1.0).contains(&uv.x) || !(0.0..=1.0).contains(&uv.y) {
        return None;
    }
    Some(PanelHit { uv, distance: t * dir.length() })
}

/// The world point of texture coordinates on a panel's quad (the inverse of `ray_to_panel`).
pub fn panel_point(centre: DVec3, rot: DQuat, size_m: DVec2, uv: DVec2) -> DVec3 {
    centre + rot * DVec3::new((uv.x - 0.5) * size_m.x, (0.5 - uv.y) * size_m.y, 0.0)
}

#[cfg(test)]
mod tests;
