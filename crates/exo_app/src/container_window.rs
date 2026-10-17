//! The container window (#310): F on a locker, box or crate opens an inventory window over the
//! view, the container's places on top and the Tab bar below. A click moves a whole stack to the
//! other side, a drag puts it on a chosen place (accepted or refused with the reason), Esc cancels
//! a drag, F, Esc or Tab closes. Every move is a request through the host's authority
//! (`item_core`), so it works for clients; one player has a container open at a time (the host's
//! lock), a second one's prompt says IN USE. Layout and colours follow the design in the
//! concept repo's container-window spec (reference screen 1920x1080, sizes in logical pixels).
use crate::controls::{Actions, Controls, Tap};
use crate::crew_hud::{abs, edit, hud_caps, hud_text, set_bg, set_border, set_dashes, set_display, set_text, set_text_colour, spawn_dashes, HudFonts, HudPart};
use crate::dev_area::DevBox;
use crate::items::{slot_at, slot_rect, HudWindow, InventoryUi, Items};
use crate::walker::Player;
use avian3d::prelude::Position;
use bevy::prelude::*;
use item_core::container::Container;
use item_core::{RackId, Reject, Request, BAR_SLOTS};
use walker_core::Frame;

/// Place edge and gap (px).
const PLACE: f32 = 88.0;
const GAP: f32 = 8.0;
/// The most places a window shows (the supply crate has 20).
pub const MAX_PLACES: usize = 24;
/// The container panel's width (px).
const PANEL_W: f32 = 412.0;
const PAD: f32 = 16.0;
const BORDER: f32 = 2.0;
/// Header (title, status, the line under it), rule line and the gaps between them (px).
const HEAD_H: f32 = 38.0;
const RULE_H: f32 = 18.0;
const SECTION_GAP: f32 = 12.0;
/// The bar panel: padding, its small header and the gap under it.
const BAR_HEAD_H: f32 = 20.0;
const BAR_HEAD_GAP: f32 = 8.0;
const BAR_PAD_BOTTOM: f32 = 8.0;
/// The gap between the container panel and the bar panel.
const STACK_GAP: f32 = 16.0;
/// How long a refused click or drop shows its reason (s).
const FLASH_S: f32 = 1.2;
/// The walker leaves the window when farther than this from the container (m).
const REACH: f64 = crate::displays::USE_RANGE * 1.6;
/// A refused place's tag: width of the box that centres it above the place (px).
const TAG_W: f32 = 300.0;

const DASHES: u32 = 200;

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::srgb_u8(r, g, b)
}
fn ink() -> Color {
    Color::srgba(5.0 / 255.0, 9.0 / 255.0, 12.0 / 255.0, 0.86)
}
fn fg() -> Color {
    rgb(0xd9, 0xe6, 0xee)
}
fn dim() -> Color {
    rgb(0x9d, 0xb1, 0xbe)
}
fn edge() -> Color {
    rgb(0x8e, 0xa3, 0xb0)
}
fn line() -> Color {
    rgb(0x3b, 0x4e, 0x5a)
}
fn accent() -> Color {
    rgb(0x6b, 0xc4, 0xff)
}
fn amber() -> Color {
    rgb(0xf2, 0xa9, 0x3b)
}
fn red() -> Color {
    rgb(0xff, 0x6b, 0x57)
}

/// What a container is called and how it is laid out; the glue that stands a container on the map
/// registers it (the rack is the host's id).
#[derive(Clone, Debug)]
pub struct ContainerSpec {
    pub rack: RackId,
    pub name: String,
    /// The `DevBox` that carries it in the world (F looks at it).
    pub box_id: String,
    pub cols: usize,
}

#[derive(Resource, Default, Debug)]
pub struct ContainerSpecs(pub Vec<ContainerSpec>);

impl ContainerSpecs {
    pub fn get(&self, rack: RackId) -> Option<&ContainerSpec> {
        self.0.iter().find(|s| s.rack == rack)
    }
}

/// A place in the window: a slot of the bar or a place of the container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Bar(usize),
    Box(usize),
}

#[derive(Resource, Default, Debug)]
pub struct ContainerWindow {
    /// The container that is open.
    pub rack: Option<RackId>,
    /// The number of the Open request: if the host refuses it the window closes.
    open_seq: Option<u32>,
    /// The place a drag began on (the button is down).
    pub drag: Option<Place>,
    /// The pointer has left that place: it is a drag, not a click.
    pub moved: bool,
    pub hover: Option<Place>,
    /// The place under a drag and why it refuses (None: it accepts).
    pub over: Option<(Place, Option<Reject>)>,
    /// A refused click or drop: the place, why, seconds left.
    pub flash: Option<(Place, Reject, f32)>,
    was_down: bool,
}

impl ContainerWindow {
    /// Asks for the container (the host's lock) and opens the window at once.
    pub fn open(&mut self, now: f64, rack: RackId, items: &mut Items, ui: &mut InventoryUi) {
        let Ok(seq) = items.request(now, Request::Open { rack }) else { return };
        *self = ContainerWindow { rack: Some(rack), open_seq: Some(seq), ..Default::default() };
        ui.open = true;
        ui.drag_from = None;
    }

    /// Gives the container back and closes the window.
    pub fn close(&mut self, now: f64, items: &mut Items, ui: &mut InventoryUi) {
        if let Some(rack) = self.rack {
            let _ = items.request(now, Request::Close { rack });
        }
        self.shut(ui);
    }

    /// Closes without telling the host (it took the container back itself).
    fn shut(&mut self, ui: &mut InventoryUi) {
        *self = ContainerWindow::default();
        ui.open = false;
        ui.drag_from = None;
    }

    /// Whether a drag is under way (the pointer left its place).
    pub fn dragging(&self) -> bool {
        self.drag.is_some() && self.moved
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<ContainerWindow>().init_resource::<ContainerSpecs>();
    app.add_systems(FixedUpdate, container_input.after(crate::items::item_input).before(crate::items::deliver).in_set(crate::phases::Fx::Walker));
}

pub fn window_plugin(app: &mut App) {
    use crate::phases::Frame;
    app.add_systems(Update, (build_window, draw_window).chain().after(crate::crew_hud::build_hud).in_set(Frame::Hud));
}

// ---- geometry ----

struct Geo {
    panel: (Vec2, Vec2),
    places: Vec<Vec2>,
    bar_panel: (Vec2, Vec2),
}

fn geometry(window: Vec2, n: usize, cols: usize, bar: &display_core::hud::BarLook) -> Geo {
    let cols = cols.clamp(1, MAX_PLACES);
    let (first, size) = slot_rect(0, true, window, bar);
    let (last, _) = slot_rect(BAR_SLOTS - 1, true, window, bar);
    let bar_w = last.x + size - first.x + 2.0 * PAD;
    let bar_top = first.y - PAD - BAR_HEAD_H - BAR_HEAD_GAP;
    let bar_h = PAD + BAR_HEAD_H + BAR_HEAD_GAP + size + BAR_PAD_BOTTOM;
    let rows = n.div_ceil(cols).max(1);
    let grid_w = cols as f32 * PLACE + (cols as f32 - 1.0) * GAP;
    let grid_h = rows as f32 * PLACE + (rows as f32 - 1.0) * GAP;
    let panel_h = 2.0 * BORDER + 2.0 * PAD + HEAD_H + SECTION_GAP + RULE_H + SECTION_GAP + grid_h;
    let panel_top = bar_top - STACK_GAP - panel_h;
    let panel_left = (window.x - PANEL_W) * 0.5;
    let inner_w = PANEL_W - 2.0 * (BORDER + PAD);
    let x0 = panel_left + BORDER + PAD + (inner_w - grid_w) * 0.5;
    let y0 = panel_top + BORDER + PAD + HEAD_H + SECTION_GAP + RULE_H + SECTION_GAP;
    let places = (0..n).map(|i| Vec2::new(x0 + (i % cols) as f32 * (PLACE + GAP), y0 + (i / cols) as f32 * (PLACE + GAP))).collect();
    Geo { panel: (Vec2::new(panel_left, panel_top), Vec2::new(PANEL_W, panel_h)), places, bar_panel: (Vec2::new(first.x - PAD, bar_top), Vec2::new(bar_w, bar_h)) }
}

fn inside(p: Vec2, at: Vec2, size: f32) -> bool {
    p.x >= at.x && p.x <= at.x + size && p.y >= at.y && p.y <= at.y + size
}

/// The middle of a place on screen, for scripts that drive the pointer.
pub fn place_centre(window: Vec2, n: usize, cols: usize, bar: &display_core::hud::BarLook, p: Place) -> Vec2 {
    match p {
        Place::Bar(i) => {
            let (at, size) = slot_rect(i, true, window, bar);
            at + Vec2::splat(size * 0.5)
        }
        Place::Box(i) => geometry(window, n, cols, bar).places.get(i).copied().unwrap_or(Vec2::ZERO) + Vec2::splat(PLACE * 0.5),
    }
}

fn place_at(p: Vec2, geo: &Geo, window: Vec2, bar: &display_core::hud::BarLook) -> Option<Place> {
    if let Some(i) = slot_at(p, true, window, bar) {
        return Some(Place::Bar(i));
    }
    geo.places.iter().position(|at| inside(p, *at, PLACE)).map(Place::Box)
}

// ---- words ----

/// What a container takes, for the header: the tags its places allow ("MASK / SUIT ONLY"), or
/// "ANY ITEM" when a place takes anything.
fn rule_text(c: &Container) -> String {
    let mut tags: Vec<String> = Vec::new();
    for i in 0..c.len() {
        let r = c.rule(i);
        if r.allow.is_empty() {
            return "ANY ITEM".into();
        }
        for t in &r.allow {
            let t = t.as_str().replace('_', " ").to_uppercase();
            if !tags.contains(&t) {
                tags.push(t);
            }
        }
    }
    if tags.is_empty() { "ANY ITEM".into() } else { format!("{} ONLY", tags.join(" / ")) }
}

/// Why a place refuses, in the tag's words; `place` is the container place asked (None for a click
/// that looks for any).
fn reason_text(why: &Reject, c: Option<&Container>, place: Option<usize>) -> String {
    match why {
        Reject::Full => "NO FREE PLACE".into(),
        Reject::NotAllowed => match c {
            Some(c) => {
                let one = place.filter(|i| *i < c.len()).map(|i| c.rule(i)).filter(|r| !r.allow.is_empty());
                match one {
                    Some(r) => format!("{} ONLY", r.allow.iter().map(|t| t.as_str().replace('_', " ").to_uppercase()).collect::<Vec<_>>().join(" / ")),
                    None => match rule_text(c) {
                        t if t.ends_with("ONLY") => t,
                        _ => "NOT ALLOWED".into(),
                    },
                }
            }
            None => "NOT ALLOWED".into(),
        },
        Reject::NotCollectable => "CANNOT CARRY".into(),
        Reject::Locked => "LOCKED".into(),
        Reject::InUse => "IN USE".into(),
        Reject::EmptySlot | Reject::NoSuchSlot => "NOTHING THERE".into(),
        _ => "REFUSED".into(),
    }
}

/// The header's status: used/total places, with FULL and EMPTY.
fn status_text(c: &Container) -> (String, Color) {
    let cap = c.capacity();
    if cap.used_slots == 0 {
        (format!("0/{} EMPTY", cap.slots), dim())
    } else if cap.used_slots == cap.slots {
        (format!("{0}/{0} FULL", cap.slots), amber())
    } else {
        (format!("{}/{}", cap.used_slots, cap.slots), fg())
    }
}

// ---- input ----

/// The request a click on a place sends: the stack goes to the other side.
fn click_request(rack: RackId, at: Place) -> Request {
    match at {
        Place::Bar(slot) => Request::Store { slot, rack },
        Place::Box(rack_slot) => Request::Fetch { rack, rack_slot },
    }
}

/// The request a drop of the stack on `from` onto `to` sends.
fn drop_request(rack: RackId, from: Place, to: Place) -> Request {
    match (from, to) {
        (Place::Bar(from), Place::Bar(to)) => Request::Move { from, to },
        (Place::Bar(slot), Place::Box(rack_slot)) => Request::StoreAt { slot, rack, rack_slot },
        (Place::Box(rack_slot), Place::Bar(slot)) => Request::FetchTo { rack, rack_slot, slot },
        (Place::Box(from), Place::Box(to)) => Request::MoveIn { rack, from, to },
    }
}

fn occupied(items: &Items, rack: RackId, at: Place) -> bool {
    match at {
        Place::Bar(i) => items.client.view.slot(i).is_some(),
        Place::Box(i) => items.client.racks.get(&rack).is_some_and(|c| c.slot(i).is_some()),
    }
}

#[allow(clippy::too_many_arguments)]
fn container_input(
    time: Res<Time>,
    mut cw: ResMut<ContainerWindow>,
    mut ui: ResMut<InventoryUi>,
    mut items: ResMut<Items>,
    mut actions: ResMut<Actions>,
    mut controls: ResMut<Controls>,
    (window, look): (Res<HudWindow>, Res<crate::displays::DisplayLook>),
    specs: Res<ContainerSpecs>,
    players: Query<&Player>,
    boxes: Query<(&DevBox, &Position)>,
) {
    let now = time.elapsed_secs_f64();
    let Some(rack) = cw.rack else {
        if cw.flash.is_some() || cw.drag.is_some() {
            *cw = ContainerWindow::default();
        }
        return;
    };
    // Why the window goes: the bar closed (Tab, seated, ...), the host refused or took the
    // container back, the container is gone, the walker went away.
    let refused = cw.open_seq.is_some_and(|s| items.client.rejected.iter().any(|(r, _)| *r == s));
    let taken = items.client.holder(rack).is_some_and(|p| p != items.me());
    let gone = !items.client.racks.contains_key(&rack);
    let far = specs.get(rack).and_then(|s| boxes.iter().find(|(b, _)| b.id == s.box_id)).zip(players.single().ok()).is_some_and(|((_, at), pl)| pl.world_pos(Frame::IDENTITY).distance(at.0) > REACH);
    if refused || taken || gone {
        cw.shut(&mut ui);
        return;
    }
    if !ui.open || far {
        cw.close(now, &mut items, &mut ui);
        return;
    }
    if let Some((_, _, left)) = cw.flash.as_mut() {
        *left -= time.delta_secs();
    }
    if cw.flash.as_ref().is_some_and(|(_, _, left)| *left <= 0.0) {
        cw.flash = None;
    }
    if actions.take_tap(Tap::DisplayLeave) {
        if cw.drag.take().is_some() {
            cw.moved = false;
            cw.over = None;
            ui.drag_from = None;
        } else {
            cw.close(now, &mut items, &mut ui);
            return;
        }
    }
    let n = items.client.racks.get(&rack).map_or(0, |c| c.len()).min(MAX_PLACES);
    let cols = specs.get(rack).map_or(4, |s| s.cols);
    let bar = &look.0.hud.bar;
    let geo = geometry(window.0, n, cols, bar);
    let under = controls.pointer.and_then(|p| place_at(p, &geo, window.0, bar));
    cw.hover = under;
    let down = controls.pointer_down || controls.click;
    if down && !cw.was_down {
        cw.drag = under.filter(|p| occupied(&items, rack, *p));
        cw.moved = false;
        ui.drag_from = match cw.drag {
            Some(Place::Bar(i)) => Some(i),
            _ => None,
        };
    }
    if down && let Some(from) = cw.drag {
        cw.moved |= under != Some(from);
    }
    cw.over = match (cw.drag, cw.moved, under) {
        (Some(from), true, Some(to)) if to != from => Some((to, items.client.would(&drop_request(rack, from, to)).err())),
        _ => None,
    };
    if !down && cw.was_down {
        let from = cw.drag.take();
        cw.moved = false;
        cw.over = None;
        ui.drag_from = None;
        if let Some(from) = from {
            let (req, at) = match under {
                Some(to) if to == from => (Some(click_request(rack, from)), from),
                Some(to) => (Some(drop_request(rack, from, to)), to),
                None => (None, from),
            };
            if let Some(req) = req
                && let Err(why) = items.request(now, req)
            {
                cw.flash = Some((at, why, FLASH_S));
            }
        }
    }
    cw.was_down = down;
    controls.click = false;
}

// ---- the picture ----

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum CwPart {
    Panel,
    Header,
    Title,
    Status,
    Rule,
    BarPanel,
    BarHeader,
    /// Place `i` (`Place::index`): the container's places and a layer over the bar's slots.
    Place(usize),
    Code(usize),
    Count(usize),
    Cross(usize),
    Tag(usize),
    TagText(usize),
    Ghost,
    GhostCode,
    GhostCount,
}

fn build_window(mut commands: Commands, fonts: Option<Res<HudFonts>>, look: Res<crate::displays::DisplayLook>, built: Query<(), With<CwPart>>) {
    let Some(fonts) = fonts else { return };
    if !built.is_empty() {
        return;
    }
    let h = &look.0.hud;
    let none = Display::None;
    commands
        .spawn((HudPart, CwPart::Panel, Node { border: UiRect::all(Val::Px(BORDER)), display: none, ..abs() }, BackgroundColor(ink()), BorderColor::all(edge()), GlobalZIndex(2)))
        .with_children(|p| {
            p.spawn((
                CwPart::Header,
                Node {
                    left: Val::Px(PAD),
                    top: Val::Px(PAD),
                    width: Val::Px(PANEL_W - 2.0 * (BORDER + PAD)),
                    padding: UiRect::bottom(Val::Px(10.0)),
                    border: UiRect::bottom(Val::Px(BORDER)),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..abs()
                },
                BorderColor::all(line()),
            ))
            .with_children(|r| {
                r.spawn((CwPart::Title, hud_caps(&fonts.bold, "", 20.0, fg(), h, 0.15)));
                r.spawn((CwPart::Status, hud_caps(&fonts.mono, "", 14.0, fg(), h, 0.07)));
            });
            p.spawn((CwPart::Rule, Node { left: Val::Px(PAD), top: Val::Px(PAD + HEAD_H + SECTION_GAP), ..abs() }, hud_caps(&fonts.semibold, "", 13.0, dim(), h, 0.15)));
        });
    commands
        .spawn((HudPart, CwPart::BarPanel, Node { display: none, ..abs() }, BackgroundColor(ink()), GlobalZIndex(2)))
        .with_children(|p| {
            p.spawn((
                CwPart::BarHeader,
                Node { left: Val::Px(PAD), top: Val::Px(PAD), height: Val::Px(BAR_HEAD_H), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..abs() },
            ))
            .with_children(|r| {
                r.spawn(hud_caps(&fonts.bold, "BAR", 14.0, fg(), h, 0.2));
                r.spawn(hud_text(&fonts.mono, "KEYS 1–8", 14.0, dim(), h));
            });
        });
    for i in 0..BAR_SLOTS + MAX_PLACES {
        commands
            .spawn((HudPart, CwPart::Place(i), Node { width: Val::Px(PLACE), height: Val::Px(PLACE), border: UiRect::all(Val::Px(BORDER)), display: none, ..abs() }, BackgroundColor(Color::NONE), BorderColor::all(Color::NONE), GlobalZIndex(3)))
            .with_children(|p| {
                p.spawn((CwPart::Code(i), Node { left: Val::Px(0.0), top: Val::Px(28.0), width: percent(100.0), justify_content: JustifyContent::Center, ..abs() })).with_children(|c| {
                    c.spawn(hud_caps(&fonts.bold, "", 28.0, fg(), h, 0.04)).insert(CwPart::Code(i));
                });
                p.spawn((CwPart::Count(i), Node { right: Val::Px(7.0), bottom: Val::Px(5.0), ..abs() }, hud_text(&fonts.mono, "", 15.0, fg(), h)));
                p.spawn((CwPart::Cross(i), Node { left: Val::Px(0.0), top: Val::Px(6.0), width: percent(100.0), justify_content: JustifyContent::Center, display: none, ..abs() }, hud_text(&fonts.bold, "", 56.0, red(), h)));
                p.spawn((CwPart::Tag(i), Node { left: Val::Px(-(TAG_W - PLACE) * 0.5 - BORDER), top: Val::Px(-34.0), width: Val::Px(TAG_W), justify_content: JustifyContent::Center, display: none, ..abs() })).with_children(|t| {
                    t.spawn((Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), border: UiRect::all(Val::Px(BORDER)), ..default() }, BackgroundColor(rgb(0x2e, 0x17, 0x13)), BorderColor::all(red()))).with_children(|b| {
                        b.spawn((CwPart::TagText(i), hud_caps(&fonts.bold, "", 14.0, rgb(0xff, 0xb4, 0xa8), h, 0.11)));
                    });
                });
                spawn_dashes(p, DASHES + i as u32, (PLACE, PLACE), BORDER, BORDER, h, accent());
            });
    }
    commands
        .spawn((
            HudPart,
            CwPart::Ghost,
            Node { width: Val::Px(PLACE), height: Val::Px(PLACE), border: UiRect::all(Val::Px(BORDER)), display: none, ..abs() },
            BackgroundColor(rgb(0x23, 0x38, 0x45)),
            BorderColor::all(fg()),
            UiTransform { rotation: Rot2::degrees(-4.0), ..UiTransform::IDENTITY },
            GlobalZIndex(6),
        ))
        .with_children(|g| {
            g.spawn(Node { left: Val::Px(0.0), top: Val::Px(28.0), width: percent(100.0), justify_content: JustifyContent::Center, ..abs() }).with_children(|c| {
                c.spawn((CwPart::GhostCode, hud_caps(&fonts.bold, "", 28.0, fg(), h, 0.04)));
            });
            g.spawn((CwPart::GhostCount, Node { right: Val::Px(7.0), bottom: Val::Px(5.0), ..abs() }, hud_text(&fonts.mono, "", 15.0, fg(), h)));
        });
}

/// How a place looks this frame.
#[derive(Clone, Debug, PartialEq)]
enum Look {
    Idle,
    Hover,
    /// The dragged stack's own place: shown empty while it travels.
    Lifted,
    Accept,
    Refuse(String),
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_window(
    items: Res<Items>,
    cw: Res<ContainerWindow>,
    window: Res<HudWindow>,
    controls: Res<Controls>,
    look: Res<crate::displays::DisplayLook>,
    specs: Res<ContainerSpecs>,
    fonts: Option<Res<HudFonts>>,
    mut nodes: Query<(&CwPart, &mut Node)>,
    mut borders: Query<(&CwPart, &mut BorderColor)>,
    mut fills: Query<(&CwPart, &mut BackgroundColor)>,
    mut texts: Query<(&CwPart, &mut Text, &mut TextColor)>,
    mut frames: Query<(&crate::crew_hud::DashFrame, &mut Node), Without<CwPart>>,
    mut dashes: Query<(&crate::crew_hud::Dash, &mut BackgroundColor), Without<CwPart>>,
) {
    let Some(_fonts) = fonts else { return };
    let container = cw.rack.and_then(|r| items.client.racks.get(&r).map(|c| (r, c)));
    let show = container.is_some();
    let (n, cols) = container.map_or((0, 4), |(r, c)| (c.len().min(MAX_PLACES), specs.get(r).map_or(4, |s| s.cols)));
    let bar = &look.0.hud.bar;
    let geo = geometry(window.0, n, cols, bar);
    let dragging = cw.dragging();
    let name_of = |s: &item_core::container::Stack| items.defs.of(s).map(|d| d.short_code()).unwrap_or_default();
    let stack_at = |p: Place| match p {
        Place::Bar(i) => items.client.view.slot(i),
        Place::Box(i) => container.and_then(|(_, c)| c.slot(i)),
    };
    let reason = |why: &Reject, p: Place| reason_text(why, container.map(|(_, c)| c), if let Place::Box(i) = p { Some(i) } else { None });
    let look_of = |p: Place| -> Look {
        if dragging && cw.drag == Some(p) {
            return Look::Lifted;
        }
        if let Some((at, why)) = &cw.over
            && *at == p
        {
            return match why {
                Some(w) => Look::Refuse(reason(w, p)),
                None => Look::Accept,
            };
        }
        if let Some((at, why, _)) = &cw.flash
            && *at == p
        {
            return Look::Refuse(reason(why, p));
        }
        if !dragging && cw.hover == Some(p) { Look::Hover } else { Look::Idle }
    };
    let place_of = |idx: usize| if idx < BAR_SLOTS { Place::Bar(idx) } else { Place::Box(idx - BAR_SLOTS) };
    let live = |idx: usize| show && (idx < BAR_SLOTS || idx - BAR_SLOTS < n);

    for (part, mut node) in &mut nodes {
        match *part {
            CwPart::Panel => {
                edit(&mut node, |nd| {
                    nd.display = if show { Display::Flex } else { Display::None };
                    nd.left = Val::Px(geo.panel.0.x);
                    nd.top = Val::Px(geo.panel.0.y);
                    nd.width = Val::Px(geo.panel.1.x);
                    nd.height = Val::Px(geo.panel.1.y);
                });
            }
            CwPart::BarPanel => {
                edit(&mut node, |nd| {
                    nd.display = if show { Display::Flex } else { Display::None };
                    nd.left = Val::Px(geo.bar_panel.0.x);
                    nd.top = Val::Px(geo.bar_panel.0.y);
                    nd.width = Val::Px(geo.bar_panel.1.x);
                    nd.height = Val::Px(geo.bar_panel.1.y);
                });
            }
            CwPart::BarHeader => edit(&mut node, |nd| nd.width = Val::Px(geo.bar_panel.1.x - 2.0 * PAD)),
            CwPart::Place(idx) => {
                let at = match place_of(idx) {
                    Place::Bar(i) => slot_rect(i, true, window.0, bar).0,
                    Place::Box(i) => geo.places.get(i).copied().unwrap_or(Vec2::ZERO),
                };
                let l = look_of(place_of(idx));
                let visible = live(idx) && (idx >= BAR_SLOTS || matches!(l, Look::Refuse(_) | Look::Accept));
                edit(&mut node, |nd| {
                    nd.display = if visible { Display::Flex } else { Display::None };
                    nd.left = Val::Px(at.x);
                    nd.top = Val::Px(at.y);
                });
            }
            CwPart::Cross(idx) => {
                let refused = live(idx) && matches!(look_of(place_of(idx)), Look::Refuse(_));
                set_display(&mut node, if refused { Display::Flex } else { Display::None });
            }
            CwPart::Tag(idx) => {
                let refused = live(idx) && matches!(look_of(place_of(idx)), Look::Refuse(_));
                set_display(&mut node, if refused { Display::Flex } else { Display::None });
            }
            CwPart::Ghost => {
                let from = cw.drag.filter(|_| dragging).filter(|p| matches!(p, Place::Box(_)));
                let at = controls.pointer.unwrap_or_default() - Vec2::splat(PLACE * 0.5);
                edit(&mut node, |nd| {
                    nd.display = if from.is_some() && show { Display::Flex } else { Display::None };
                    nd.left = Val::Px(at.x);
                    nd.top = Val::Px(at.y);
                });
            }
            _ => {}
        }
    }
    for (part, mut border) in &mut borders {
        if let CwPart::Place(idx) = *part {
            let p = place_of(idx);
            let want = match look_of(p) {
                Look::Refuse(_) => red(),
                Look::Hover => accent(),
                Look::Accept | Look::Lifted if idx < BAR_SLOTS => Color::NONE,
                Look::Accept => Color::NONE,
                Look::Lifted => line(),
                Look::Idle if idx < BAR_SLOTS => Color::NONE,
                Look::Idle if stack_at(p).is_some() => edge(),
                Look::Idle => line(),
            };
            set_border(&mut border, want);
        }
    }
    for (part, mut bg) in &mut fills {
        if let CwPart::Place(idx) = *part {
            let p = place_of(idx);
            let want = match look_of(p) {
                Look::Refuse(_) => rgb(0x2e, 0x17, 0x13),
                Look::Hover | Look::Accept if idx >= BAR_SLOTS => rgb(0x15, 0x30, 0x3f),
                Look::Hover | Look::Accept => Color::NONE,
                Look::Lifted => rgb(0x0b, 0x13, 0x18),
                Look::Idle if idx < BAR_SLOTS => Color::NONE,
                Look::Idle if stack_at(p).is_some() => rgb(0x1a, 0x2a, 0x33),
                Look::Idle => rgb(0x0b, 0x13, 0x18),
            };
            set_bg(&mut bg, want);
        }
    }
    for i in 0..BAR_SLOTS + MAX_PLACES {
        let accepting = live(i) && look_of(place_of(i)) == Look::Accept;
        set_dashes(DASHES + i as u32, accepting, accent(), &mut frames, &mut dashes);
    }
    let (title, rule, status) = match container {
        Some((r, c)) => (specs.get(r).map_or("CONTAINER".to_string(), |s| s.name.to_uppercase()), rule_text(c), status_text(c)),
        None => (String::new(), String::new(), (String::new(), fg())),
    };
    let ghost = cw.drag.filter(|_| dragging).and_then(stack_at);
    for (part, mut text, mut colour) in &mut texts {
        match *part {
            CwPart::Title => set_text(&mut text, &title.to_uppercase()),
            CwPart::Rule => set_text(&mut text, &rule),
            CwPart::Status => {
                set_text(&mut text, &status.0.to_uppercase());
                set_text_colour(&mut colour, status.1);
            }
            CwPart::Code(idx) => {
                let p = place_of(idx);
                let on = idx >= BAR_SLOTS && live(idx) && !matches!(look_of(p), Look::Lifted | Look::Refuse(_));
                set_text(&mut text, &if on { stack_at(p).map(name_of).unwrap_or_default() } else { String::new() });
            }
            CwPart::Count(idx) => {
                let p = place_of(idx);
                let on = idx >= BAR_SLOTS && live(idx) && !matches!(look_of(p), Look::Lifted | Look::Refuse(_));
                set_text(&mut text, &if on { stack_at(p).filter(|s| s.count > 1).map(|s| format!("×{}", s.count)).unwrap_or_default() } else { String::new() });
            }
            CwPart::Cross(idx) => set_text(&mut text, if live(idx) && matches!(look_of(place_of(idx)), Look::Refuse(_)) { "×" } else { "" }),
            CwPart::TagText(idx) => {
                if let Look::Refuse(t) = look_of(place_of(idx)) {
                    set_text(&mut text, &t);
                }
            }
            CwPart::GhostCode => set_text(&mut text, &ghost.map(name_of).unwrap_or_default()),
            CwPart::GhostCount => set_text(&mut text, &ghost.filter(|s| s.count > 1).map(|s| format!("×{}", s.count)).unwrap_or_default()),
            _ => {}
        }
    }
}
