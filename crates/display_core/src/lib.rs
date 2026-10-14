//! display_core: what a display panel shows and how a pointer works on it, without engine types
//! (#285). A `Panel` is a size in panel pixels and a tree of `Widget`s (labels, buttons, bars,
//! toggles, sliders, dials, trends and lists in rows and columns). The widget set and the look
//! come from the design system "EXO-1 Ship Displays" (2026-10-13); colours and fonts are data
//! (`theme`, `content/ui/displays.json`). `Panel::layout` stacks them into rectangles, a `Pointer`
//! turns a position and a held button into hover, press and `Click`s with the widget's action id,
//! screens come in the design system's tiers (S, M, L) on a plate with a header (the tier is the
//! panel's canvas, drawn into the display's texture), and `ray_to_panel` finds where a ray (the
//! view through the cursor) meets a display's quad in the world; `Panel::from_uv` turns that hit
//! into panel pixels.
//!
//! Panel pixels: origin top left, x right, y down (like screen UI). A role builds its panel each
//! tick from its own state; widgets are plain data, so the panel can be compared, drawn on the
//! screen and hit-tested headless the same way.
use glam::{DQuat, DVec2, DVec3};
use std::borrow::Cow;

pub mod canvas;
pub mod hud;
pub mod target;
pub mod theme;
pub use canvas::{arc_points, Ink, Shape, TextFace};
pub use theme::{Rgb, Theme};

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

/// Colour role of a label (and of a button). The glue maps it to colours from the theme
/// (`content/ui/displays.json`). Good, Warn and Bad always draw with their glyph (● ▲ ■) beside the
/// text, so the state never rests on colour alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Normal,
    Dim,
    /// Small caps captions ("CORE TEMP", "MODE").
    Micro,
    Title,
    Good,
    Warn,
    Bad,
}

impl Tone {
    /// The status glyph a tone draws beside its text, if any.
    pub fn glyph(self) -> Option<Glyph> {
        match self {
            Tone::Good => Some(Glyph::Circle),
            Tone::Warn => Some(Glyph::Triangle),
            Tone::Bad => Some(Glyph::Square),
            _ => None,
        }
    }
}

/// The status shapes: ● good, ▲ warn, ■ bad (drawn as shapes, not font glyphs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Circle,
    Triangle,
    Square,
}

/// A bar's state against its thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Normal,
    Warn,
    Crit,
}

impl Level {
    pub fn tone(self) -> Tone {
        match self {
            Level::Normal => Tone::Good,
            Level::Warn => Tone::Warn,
            Level::Crit => Tone::Bad,
        }
    }
}

/// One row of a `List`: a status (Good, Warn, Bad, or anything else for a hollow dim mark), a
/// label and an optional value at the right.
#[derive(Clone, Debug, PartialEq)]
pub struct ListRow {
    pub label: String,
    pub value: String,
    pub status: Tone,
}

pub fn list_row(label: impl Into<String>, value: impl Into<String>, status: Tone) -> ListRow {
    ListRow { label: label.into(), value: value.into(), status }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    /// Text; `size` is the font size in panel pixels; `mono` draws it in the readout font.
    Label { text: String, tone: Tone, size: f64, mono: bool },
    /// A press and release over it gives a click with `action` (no value). Disabled: no hover, no
    /// click. `tone` Warn or Bad colours it (a SCRAM button); Good is the selected look (the active
    /// mode of a row of buttons, drawn like a toggle's accent segment).
    Button { label: String, enabled: bool, action: String, tone: Tone },
    /// A gauge 0..1 with optional thresholds (`level`); `label` is a small caption (may be empty).
    Bar { value: f64, label: String, warn: Option<f64>, crit: Option<f64> },
    /// Two segments (`labels`, off then on); a click gives `action` with the new state as the
    /// value (1 on, 0 off). `label` is a small caption at its left (may be empty).
    Toggle { on: bool, label: String, action: String, labels: [String; 2], enabled: bool },
    /// Pressed and dragged it gives `action` with the value 0..1 under the pointer, at the press
    /// and at every change while held. `label` is a small caption (may be empty).
    Slider { value: f64, label: String, action: String, enabled: bool },
    /// A round gauge 0..1 over 270 degrees with thresholds; `text` is the readout in its middle
    /// (with unit), `label` the caption under it.
    Dial { value: f64, text: String, label: String, warn: Option<f64>, crit: Option<f64> },
    /// The recent values (0..1, oldest first) as a stepped line with threshold lines; the
    /// newest value's level colours it.
    Trend { values: Vec<f64>, warn: Option<f64>, crit: Option<f64> },
    /// Status rows. With an `action` a click on a row gives the row's index as the value.
    List { rows: Vec<ListRow>, selected: Option<usize>, action: Option<String>, row_h: f64 },
    Row { gap: f64, items: Vec<Item> },
    Column { gap: f64, items: Vec<Item> },
    /// Takes room, draws nothing.
    Space,
    /// A drawn picture in its box (`Shape`s in its own pixels from the top left, `w` by `h` as
    /// designed: `Panel::overflowing` checks every shape inside). With an `action` the whole box is
    /// a button: a click gives the action with no value.
    Canvas { w: f64, h: f64, shapes: Vec<Shape>, action: Option<String> },
}

impl Widget {
    /// The action id of a widget the pointer can use (enabled buttons, toggles and sliders, lists
    /// with an action).
    pub fn action(&self) -> Option<&str> {
        match self {
            Widget::Button { enabled: true, action, .. } | Widget::Toggle { enabled: true, action, .. } | Widget::Slider { enabled: true, action, .. } => Some(action),
            Widget::List { action: Some(a), .. } => Some(a),
            Widget::Canvas { action: Some(a), .. } => Some(a),
            _ => None,
        }
    }

    /// The action id, enabled or not.
    fn action_id(&self) -> Option<&str> {
        match self {
            Widget::Button { action, .. } | Widget::Toggle { action, .. } | Widget::Slider { action, .. } => Some(action),
            Widget::List { action: Some(a), .. } => Some(a),
            Widget::Canvas { action: Some(a), .. } => Some(a),
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
    /// A button, toggle or slider that can or cannot be used (other widgets ignore it).
    pub fn enabled(mut self, on: bool) -> Item {
        match &mut self.widget {
            Widget::Button { enabled, .. } | Widget::Toggle { enabled, .. } | Widget::Slider { enabled, .. } => *enabled = on,
            _ => {}
        }
        self
    }
    /// The thresholds of a bar, dial or trend (other widgets ignore them).
    pub fn thresholds(mut self, warn_at: Option<f64>, crit_at: Option<f64>) -> Item {
        match &mut self.widget {
            Widget::Bar { warn, crit, .. } | Widget::Dial { warn, crit, .. } | Widget::Trend { warn, crit, .. } => (*warn, *crit) = (warn_at, crit_at),
            _ => {}
        }
        self
    }
    /// A label's or button's colour role (other widgets ignore it).
    pub fn tone(mut self, t: Tone) -> Item {
        match &mut self.widget {
            Widget::Label { tone, .. } | Widget::Button { tone, .. } => *tone = t,
            _ => {}
        }
        self
    }
    /// A label in the readout font (other widgets ignore it).
    pub fn mono(mut self) -> Item {
        if let Widget::Label { mono, .. } = &mut self.widget {
            *mono = true;
        }
        self
    }
    /// A label's font size in panel pixels (other widgets ignore it).
    pub fn text_size(mut self, px: f64) -> Item {
        if let Widget::Label { size, .. } = &mut self.widget {
            *size = px;
        }
        self
    }
    /// A toggle's segment texts, off then on (other widgets ignore them).
    pub fn labels(mut self, off: impl Into<String>, on: impl Into<String>) -> Item {
        if let Widget::Toggle { labels, .. } = &mut self.widget {
            *labels = [off.into(), on.into()];
        }
        self
    }
    /// A list's highlighted row and the action a row click gives (other widgets ignore them).
    pub fn select(mut self, row: Option<usize>, on_click: Option<&str>) -> Item {
        if let Widget::List { selected, action, .. } = &mut self.widget {
            *selected = row;
            *action = on_click.map(str::to_string);
        }
        self
    }
}

fn item(widget: Widget) -> Item {
    Item { widget, size: Size::Flex(1.0) }
}

/// Font sizes of the design system (panel pixels): small, label, title, value, hero.
pub mod font {
    pub const SMALL: f64 = 16.0;
    pub const LABEL: f64 = 20.0;
    pub const TITLE: f64 = 28.0;
    pub const VALUE: f64 = 40.0;
    pub const HERO: f64 = 72.0;
}

/// Control heights of the design system (panel pixels): buttons, toggles and sliders are 48 tall
/// (40 small, 64 large) so the cursor hits them and they read from a few metres.
pub mod ctl {
    pub const SM: f64 = 40.0;
    pub const MD: f64 = 48.0;
    pub const LG: f64 = 64.0;
    /// A bar with its caption.
    pub const BAR: f64 = 44.0;
}

/// Body text, 20 px.
pub fn label(text: impl Into<String>) -> Item {
    item(Widget::Label { text: text.into(), tone: Tone::Normal, size: font::LABEL, mono: false })
}
/// A heading, 28 px, caps.
pub fn title(text: impl Into<String>) -> Item {
    item(Widget::Label { text: text.into(), tone: Tone::Title, size: font::TITLE, mono: false })
}
/// A small caps caption, 16 px.
pub fn micro(text: impl Into<String>) -> Item {
    item(Widget::Label { text: text.into(), tone: Tone::Micro, size: font::SMALL, mono: false })
}
/// A big readout in the mono font, 40 px ("412 °C").
pub fn readout(text: impl Into<String>) -> Item {
    item(Widget::Label { text: text.into(), tone: Tone::Normal, size: font::VALUE, mono: true })
}
pub fn button(label: impl Into<String>, action: impl Into<String>) -> Item {
    item(Widget::Button { label: label.into(), enabled: true, action: action.into(), tone: Tone::Normal })
}
pub fn bar(value: f64, label: impl Into<String>) -> Item {
    item(Widget::Bar { value, label: label.into(), warn: None, crit: None })
}
/// A two-segment switch, OFF / ON unless `labels` says otherwise.
pub fn toggle(on: bool, label: impl Into<String>, action: impl Into<String>) -> Item {
    item(Widget::Toggle { on, label: label.into(), action: action.into(), labels: ["OFF".into(), "ON".into()], enabled: true })
}
pub fn slider(value: f64, label: impl Into<String>, action: impl Into<String>) -> Item {
    item(Widget::Slider { value, label: label.into(), action: action.into(), enabled: true })
}
pub fn dial(value: f64, text: impl Into<String>, label: impl Into<String>) -> Item {
    item(Widget::Dial { value, text: text.into(), label: label.into(), warn: None, crit: None })
}
pub fn trend(values: Vec<f64>) -> Item {
    item(Widget::Trend { values, warn: None, crit: None })
}
/// Status rows, 40 px each.
pub fn list(rows: Vec<ListRow>) -> Item {
    item(Widget::List { rows, selected: None, action: None, row_h: 40.0 })
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

/// The row of a list under a point, if any (the rectangle is the list's).
pub fn list_row_at(rect: &Rect, row_h: f64, rows: usize, p: DVec2) -> Option<usize> {
    if !rect.contains(p) || row_h <= 0.0 {
        return None;
    }
    let i = ((p.y - rect.min.y) / row_h).floor();
    (i >= 0.0 && (i as usize) < rows).then_some(i as usize)
}

// ---------- screens ----------

/// The design system's screen tiers: S 320 x 240 (one machine: a readout and one or two
/// controls), M 640 x 400 (a control panel), L 1280 x 720 (a bridge overview). A machine's display
/// picks the tier that fits it; the canvas is in panel pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tier {
    S,
    M,
    L,
}

impl Tier {
    pub fn canvas(self) -> DVec2 {
        match self {
            Tier::S => DVec2::new(320.0, 240.0),
            Tier::M => DVec2::new(640.0, 400.0),
            Tier::L => DVec2::new(1280.0, 720.0),
        }
    }
    /// The tier whose canvas is nearest to a size (by height).
    pub fn nearest(size: DVec2) -> Tier {
        [Tier::S, Tier::M, Tier::L].into_iter().min_by(|a, b| (a.canvas().y - size.y).abs().total_cmp(&(b.canvas().y - size.y).abs())).unwrap()
    }
}

/// A screen's plate: the header strip (accent lamp, caps title, machine tag, status at the right)
/// above a recessed well that holds the widgets.
#[derive(Clone, Debug, PartialEq)]
pub struct Plate {
    pub title: String,
    pub tag: String,
    pub status: Option<(String, Tone)>,
}

/// Plate geometry (panel pixels), from the design system: outline width, header height, the
/// well's inset from the plate edge and its own border.
pub mod plate {
    pub const BORDER: f64 = 2.0;
    pub const HEADER: f64 = 32.0;
    pub const INSET: f64 = 4.0;
    pub const WELL_BORDER: f64 = 2.0;
    /// The cut corner (top right).
    pub const CUT: f64 = 16.0;
}

// ---------- layout ----------

/// A rectangle in panel pixels.
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

/// What a display shows: a size in panel pixels, the padding inside the edge (or inside the
/// plate's well), an optional plate, and the widgets.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub size: DVec2,
    pub padding: f64,
    pub plate: Option<Plate>,
    pub root: Item,
}

impl Panel {
    /// Panel pixels from texture coordinates (0..1, v down), as `ray_to_panel` gives them.
    pub fn from_uv(&self, uv: DVec2) -> DVec2 {
        uv * self.size
    }

    /// Texture coordinates (0..1, v down) of a point in panel pixels (the inverse of `from_uv`).
    pub fn to_uv(&self, at: DVec2) -> DVec2 {
        at / self.size
    }

    /// A panel of `w` by `h` panel pixels with 16 px padding and no plate.
    pub fn new(w: f64, h: f64, root: Item) -> Panel {
        Panel { size: DVec2::new(w, h), padding: 16.0, plate: None, root }
    }

    /// A screen of a tier, on a plate with a title and machine tag, 12 px padding in the well.
    pub fn screen(tier: Tier, title: impl Into<String>, tag: impl Into<String>, root: Item) -> Panel {
        Panel { size: tier.canvas(), padding: 12.0, plate: Some(Plate { title: title.into(), tag: tag.into(), status: None }), root }
    }

    /// The plate's status word at the right of the header ("STABLE", "RUN"); no plate: ignored.
    pub fn status(mut self, text: impl Into<String>, tone: Tone) -> Panel {
        if let Some(p) = &mut self.plate {
            p.status = Some((text.into(), tone));
        }
        self
    }

    pub fn with_padding(mut self, px: f64) -> Panel {
        self.padding = px;
        self
    }

    /// The tier nearest to this panel's size.
    pub fn tier(&self) -> Tier {
        Tier::nearest(self.size)
    }

    /// The header strip, with a plate.
    pub fn header(&self) -> Option<Rect> {
        self.plate.as_ref().map(|_| Rect::new(plate::BORDER, plate::BORDER, (self.size.x - 2.0 * plate::BORDER).max(0.0), plate::HEADER))
    }

    /// The recessed well (outer edge of its border), with a plate.
    pub fn well(&self) -> Option<Rect> {
        self.plate.as_ref().map(|_| {
            let x = plate::BORDER + plate::INSET;
            let y = plate::BORDER + plate::HEADER;
            Rect::new(x, y, (self.size.x - 2.0 * x).max(0.0), (self.size.y - y - plate::BORDER - plate::INSET).max(0.0))
        })
    }

    /// Where the widgets go: inside the padding, and inside the well's border on a plate.
    pub fn content(&self) -> Rect {
        let p = self.padding;
        match self.well() {
            Some(w) => {
                let e = plate::WELL_BORDER + p;
                Rect::new(w.min.x + e, w.min.y + e, (w.size.x - 2.0 * e).max(0.0), (w.size.y - 2.0 * e).max(0.0))
            }
            None => Rect::new(p, p, (self.size.x - 2.0 * p).max(0.0), (self.size.y - 2.0 * p).max(0.0)),
        }
    }

    /// Every leaf widget with its rectangle, in tree order (rows and columns are not listed).
    pub fn layout(&self) -> Vec<Placed<'_>> {
        let mut out = Vec::new();
        place(&self.root.widget, self.content(), &mut out);
        out
    }

    /// The usable widget under a point, if any.
    pub fn hit(&self, p: DVec2) -> Option<Placed<'_>> {
        self.layout().into_iter().find(|w| w.widget.action().is_some() && w.rect.contains(p))
    }

    /// The first widget with this action (enabled or not).
    pub fn find(&self, action: &str) -> Option<Placed<'_>> {
        self.layout().into_iter().find(|w| w.widget.action_id() == Some(action))
    }

    /// What does not fit: every leaf widget whose rectangle is not inside the content area, named by
    /// its action (or "label" for a widget without one). Empty when everything fits.
    /// A canvas with a shape outside its designed box counts too (named "shape").
    pub fn overflowing(&self) -> Vec<String> {
        let c = self.content();
        let mut out: Vec<String> = self
            .layout()
            .into_iter()
            .filter(|w| !(w.rect.min.cmpge(c.min).all() && w.rect.max().cmple(c.max()).all()))
            .map(|w| w.widget.action_id().unwrap_or("label").to_string())
            .collect();
        for w in self.layout() {
            if let Widget::Canvas { w: cw, h: ch, shapes, .. } = w.widget {
                out.extend(shapes.iter().filter(|s| !s.inside(*cw, *ch)).map(|_| "shape".to_string()));
            }
        }
        out
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
                        Widget::Canvas { .. } => out.push(Click { action: p, value: None }),
                        Widget::List { rows, row_h, .. } => {
                            if let Some(i) = at.and_then(|at| list_row_at(&h.rect, *row_h, rows.len(), at)) {
                                out.push(Click { action: p, value: Some(i as f64) });
                            }
                        }
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

// ---------- the quad in the world ----------

/// Where a ray meets a panel's quad: texture coordinates (u right, v down, 0..1) and the distance
/// along the ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelHit {
    pub uv: DVec2,
    pub distance: f64,
}

/// A ray against a panel's quad in the world. The quad is centred on `centre`, `size_m` wide (local
/// x, right as seen from the front) and tall (local y, up); its front faces local +z. Only the front
/// counts; a ray from behind, parallel or pointing away misses, and so does one beside the quad.
pub fn ray_to_panel(origin: DVec3, dir: DVec3, centre: DVec3, rot: DQuat, size_m: DVec2) -> Option<PanelHit> {
    ray_to_panel_plane(origin, dir, centre, rot, size_m).filter(|h| (0.0..=1.0).contains(&h.uv.x) && (0.0..=1.0).contains(&h.uv.y))
}

/// The same ray against the quad's whole plane: the texture coordinates may lie outside 0..1. A
/// held slider uses it to follow the cursor beside the screen.
pub fn ray_to_panel_plane(origin: DVec3, dir: DVec3, centre: DVec3, rot: DQuat, size_m: DVec2) -> Option<PanelHit> {
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
    Some(PanelHit { uv, distance: t * dir.length() })
}

/// The view ray through a window position (pixels, y down) for a camera at `pos` looking along its
/// local -z, `fov_y` radians tall, in a window of `window` pixels: the ray's origin and unit
/// direction.
pub fn pointer_ray(pos: DVec3, rot: DQuat, fov_y: f64, window: DVec2, p: DVec2) -> (DVec3, DVec3) {
    let ndc = DVec2::new(p.x / window.x * 2.0 - 1.0, 1.0 - p.y / window.y * 2.0);
    let t = (fov_y * 0.5).tan();
    let local = DVec3::new(ndc.x * t * window.x / window.y, ndc.y * t, -1.0);
    (pos, (rot * local).normalize())
}

/// Where a world point appears in the window (the inverse of `pointer_ray`), or None behind the camera.
pub fn window_point(pos: DVec3, rot: DQuat, fov_y: f64, window: DVec2, target: DVec3) -> Option<DVec2> {
    let local = rot.inverse() * (target - pos);
    if local.z >= -1e-9 {
        return None;
    }
    let t = (fov_y * 0.5).tan();
    let ndc = DVec2::new(local.x / (-local.z * t * window.x / window.y), local.y / (-local.z * t));
    Some(DVec2::new((ndc.x + 1.0) * 0.5 * window.x, (1.0 - ndc.y) * 0.5 * window.y))
}

/// The world point of texture coordinates on a panel's quad (the inverse of `ray_to_panel`).
pub fn panel_point(centre: DVec3, rot: DQuat, size_m: DVec2, uv: DVec2) -> DVec3 {
    centre + rot * DVec3::new((uv.x - 0.5) * size_m.x, (0.5 - uv.y) * size_m.y, 0.0)
}

#[cfg(test)]
mod tests;

/// A drawn picture of `w` by `h` pixels; with `action` the box is a button.
pub fn canvas(w: f64, h: f64, shapes: Vec<Shape>, action: Option<&str>) -> Item {
    item(Widget::Canvas { w, h, shapes, action: action.map(str::to_string) })
}
