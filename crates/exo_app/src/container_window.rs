//! The container window (#310): F on a locker, box or crate opens an inventory window over the
//! view, the container's places on top and the Tab bar below. A click moves a whole stack to the
//! other side, a drag puts it on a chosen place (accepted or refused with the reason), F, Esc or Tab closes (Esc also mid-drag). Every move is a request through the host's authority
//! (`item_core`), so it works for clients; one player has a container open at a time (the host's
//! lock), a second one's prompt says IN USE. Layout and colours follow the design in the
//! concept repo's container-window spec (reference screen 1920x1080, sizes in logical pixels).
use crate::controls::Controls;
use crate::crew_hud::{abs, edit, hud_caps, hud_text, set_bg, set_border, set_dashes, set_display, set_text, set_text_colour, spawn_dashes, HudFonts, HudPart};
use crate::dev_area::gear::{is_gear_use, MaskWorn};
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
/// The most places a window shows (a larger container shows its first 20).
pub const MAX_PLACES: usize = 20;
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
/// The WORN group left of the bar (v2 of the design): two places 22 px apart, then 24 px, a 2 px
/// line and 24 px before the bar's slots.
const WORN_W: f32 = 2.0 * PLACE + 22.0;
const WORN_EXTRA: f32 = WORN_W + 24.0 + 2.0 + 24.0;
/// Parts per place: the bar's slots, the container's places, the worn suit and its canister.
const N_PARTS: usize = BAR_SLOTS + MAX_PLACES + 2;
/// Below this share of air the canister's gauge and caption turn amber.
const LOW_AIR: f64 = 0.2;

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
    /// The worn suit (or mask): the bar slot that holds it, shown here too.
    Suit,
    /// The suit's air canister place: today the suit's own tank as a gauge (the canister item is #311).
    Air,
}

impl Place {
    fn of_part(i: usize) -> Place {
        match i {
            i if i < BAR_SLOTS => Place::Bar(i),
            i if i < BAR_SLOTS + MAX_PLACES => Place::Box(i - BAR_SLOTS),
            i if i == BAR_SLOTS + MAX_PLACES => Place::Suit,
            _ => Place::Air,
        }
    }
}

/// The bar slot of the worn gear (the host's word; a client does not hear it yet).
fn worn_slot(worn: &Option<Res<MaskWorn>>, me: item_core::PlayerId) -> Option<usize> {
    worn.as_ref().and_then(|w| w.0.get(&me).copied())
}

/// The corners of the suit and canister places, left of the bar's first slot.
fn worn_places(window: Vec2, bar: &display_core::hud::BarLook) -> [Vec2; 2] {
    let (first, _) = slot_rect(0, true, window, bar);
    let x0 = first.x - WORN_EXTRA;
    [Vec2::new(x0, first.y), Vec2::new(x0 + PLACE + 22.0, first.y)]
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
    Geo { panel: (Vec2::new(panel_left, panel_top), Vec2::new(PANEL_W, panel_h)), places, bar_panel: (Vec2::new(first.x - PAD - WORN_EXTRA, bar_top), Vec2::new(bar_w + WORN_EXTRA, bar_h)) }
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
        Place::Suit => worn_places(window, bar)[0] + Vec2::splat(PLACE * 0.5),
        Place::Air => worn_places(window, bar)[1] + Vec2::splat(PLACE * 0.5),
    }
}

fn place_at(p: Vec2, geo: &Geo, window: Vec2, bar: &display_core::hud::BarLook) -> Option<Place> {
    if let Some(i) = slot_at(p, true, window, bar) {
        return Some(Place::Bar(i));
    }
    let w = worn_places(window, bar);
    if inside(p, w[0], PLACE) {
        return Some(Place::Suit);
    }
    if inside(p, w[1], PLACE) {
        return Some(Place::Air);
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
fn reason_text(why: &Reject, c: Option<&Container>, place: Option<usize>, at: Place) -> String {
    match (why, at) {
        (_, Place::Suit) => return "SUIT ONLY".into(),
        (_, Place::Air) => return "AIR CANISTERS ONLY".into(),
        _ => {}
    }
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

/// What a click on a place asks: the stack goes to the other side; a click on the worn suit takes
/// it off (its use toggles it). None: nothing to ask.
fn click_request(rack: Option<RackId>, worn: Option<usize>, at: Place) -> Option<Request> {
    match (at, rack) {
        (Place::Bar(slot), Some(rack)) => Some(Request::Store { slot, rack }),
        (Place::Box(rack_slot), Some(rack)) => Some(Request::Fetch { rack, rack_slot }),
        (Place::Suit, _) => worn.map(|slot| Request::Use { slot, target: crate::items::LOCAL }),
        _ => None,
    }
}

/// What a drop of the stack on `from` onto `to` asks (None: that place does not take it).
fn drop_request(rack: Option<RackId>, worn: Option<usize>, from: Place, to: Place) -> Option<Request> {
    match (from, to, rack) {
        (Place::Bar(from), Place::Bar(to), _) => Some(Request::Move { from, to }),
        (Place::Bar(slot), Place::Box(rack_slot), Some(rack)) => Some(Request::StoreAt { slot, rack, rack_slot }),
        (Place::Box(rack_slot), Place::Bar(slot), Some(rack)) => Some(Request::FetchTo { rack, rack_slot, slot }),
        (Place::Box(from), Place::Box(to), Some(rack)) => Some(Request::MoveIn { rack, from, to }),
        // The worn suit comes off by leaving its slot (the host takes gear off that left it).
        (Place::Suit, Place::Bar(to), _) => worn.map(|from| Request::Move { from, to }),
        (Place::Suit, Place::Box(rack_slot), Some(rack)) => worn.map(|slot| Request::StoreAt { slot, rack, rack_slot }),
        // Gear from the bar goes on by its use.
        (Place::Bar(slot), Place::Suit, _) => Some(Request::Use { slot, target: crate::items::LOCAL }),
        _ => None,
    }
}

/// Whether a drop would work, and why not (the request is the host's to decide in the end).
fn verdict(items: &Items, rack: Option<RackId>, worn: Option<usize>, from: Place, to: Place) -> Option<Reject> {
    let Some(req) = drop_request(rack, worn, from, to) else { return Some(Reject::NotAllowed) };
    if let (Place::Bar(slot), Place::Suit) = (from, to) {
        let gear = items.client.view.slot(slot).and_then(|s| items.defs.of(s)).and_then(|d| d.use_effect.as_deref()).is_some_and(is_gear_use);
        if !gear || worn == Some(slot) {
            return Some(Reject::NotAllowed);
        }
    }
    items.client.would(&req).err()
}

fn occupied(items: &Items, rack: Option<RackId>, worn: Option<usize>, at: Place) -> bool {
    match at {
        Place::Bar(i) => items.client.view.slot(i).is_some(),
        Place::Box(i) => rack.and_then(|r| items.client.racks.get(&r)).is_some_and(|c| c.slot(i).is_some()),
        Place::Suit => worn.is_some_and(|i| items.client.view.slot(i).is_some()),
        Place::Air => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn container_input(
    time: Res<Time>,
    mut cw: ResMut<ContainerWindow>,
    mut ui: ResMut<InventoryUi>,
    mut items: ResMut<Items>,
    mut controls: ResMut<Controls>,
    (window, look): (Res<HudWindow>, Res<crate::displays::DisplayLook>),
    specs: Res<ContainerSpecs>,
    worn: Option<Res<MaskWorn>>,
    players: Query<&Player>,
    boxes: Query<(&DevBox, &Position)>,
) {
    let now = time.elapsed_secs_f64();
    let rack = cw.rack;
    if rack.is_none() && !ui.open {
        if cw.flash.is_some() || cw.drag.is_some() || cw.hover.is_some() {
            *cw = ContainerWindow::default();
        }
        return;
    }
    if let Some(rack) = rack {
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
    }
    if let Some((_, _, left)) = cw.flash.as_mut() {
        *left -= time.delta_secs();
    }
    if cw.flash.as_ref().is_some_and(|(_, _, left)| *left <= 0.0) {
        cw.flash = None;
    }
    let me_worn = worn_slot(&worn, items.me());
    let n = rack.and_then(|r| items.client.racks.get(&r)).map_or(0, |c| c.len()).min(MAX_PLACES);
    let cols = rack.and_then(|r| specs.get(r)).map_or(4, |s| s.cols);
    let bar = &look.0.hud.bar;
    let geo = geometry(window.0, n, cols, bar);
    let under = controls.pointer.and_then(|p| place_at(p, &geo, window.0, bar)).filter(|p| rack.is_some() || matches!(p, Place::Suit | Place::Bar(_)));
    cw.hover = under;
    let down = controls.pointer_down || controls.click;
    // Without a container the bar's own gestures stay with the Tab inventory; only the worn suit's are ours.
    let mine = |p: Place| rack.is_some() || p == Place::Suit;
    if down && !cw.was_down {
        cw.drag = under.filter(|p| mine(*p) && occupied(&items, rack, me_worn, *p));
        cw.moved = false;
        ui.drag_from = match (cw.drag, rack) {
            (Some(Place::Bar(i)), Some(_)) => Some(i),
            _ => ui.drag_from,
        };
    }
    if down && let Some(from) = cw.drag {
        cw.moved |= under != Some(from);
    }
    cw.over = match (cw.drag, cw.moved, under) {
        (Some(from), true, Some(to)) if to != from => Some((to, verdict(&items, rack, me_worn, from, to))),
        _ => None,
    };
    if !down && cw.was_down {
        let from = cw.drag.take();
        cw.moved = false;
        cw.over = None;
        if rack.is_some() {
            ui.drag_from = None;
        }
        if let Some(from) = from {
            let (req, at) = match under {
                Some(to) if to == from => (click_request(rack, me_worn, from), from),
                Some(to) => match verdict(&items, rack, me_worn, from, to) {
                    None => (drop_request(rack, me_worn, from, to), to),
                    Some(why) => {
                        cw.flash = Some((to, why, FLASH_S));
                        (None, to)
                    }
                },
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
    if rack.is_some() || cw.drag.is_some() {
        controls.click = false;
    }
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
    WornHeader,
    Sep,
    Link,
    /// Part `i` (`Place::part`): the container's places, the worn places, and a layer over the bar's slots.
    Place(usize),
    Code(usize),
    Count(usize),
    Cross(usize),
    Tag(usize),
    TagText(usize),
    Caption(usize),
    GaugeTrack(usize),
    GaugeFill(usize),
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
            p.spawn((CwPart::WornHeader, Node { left: Val::Px(PAD), top: Val::Px(PAD), width: Val::Px(WORN_W), height: Val::Px(BAR_HEAD_H), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..abs() })).with_children(|r| {
                r.spawn(hud_caps(&fonts.semibold, "WORN", 14.0, dim(), h, 0.2));
                r.spawn(hud_text(&fonts.mono, "SUIT + AIR", 14.0, dim(), h));
            });
            p.spawn((CwPart::Sep, Node { left: Val::Px(PAD + WORN_W + 24.0), top: Val::Px(PAD), width: Val::Px(2.0), ..abs() }, BackgroundColor(line())));
            p.spawn((CwPart::Link, Node { left: Val::Px(PAD + PLACE), top: Val::Px(PAD + BAR_HEAD_H + BAR_HEAD_GAP + PLACE * 0.5 - 1.0), width: Val::Px(22.0), height: Val::Px(2.0), ..abs() }, BackgroundColor(edge())));
            p.spawn((CwPart::BarHeader, Node { left: Val::Px(PAD + WORN_EXTRA), top: Val::Px(PAD), height: Val::Px(BAR_HEAD_H), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..abs() })).with_children(|r| {
                r.spawn(hud_caps(&fonts.bold, "BAR", 14.0, fg(), h, 0.2));
                r.spawn(hud_text(&fonts.mono, "KEYS 1–8", 14.0, dim(), h));
            });
        });
    for i in 0..N_PARTS {
        commands
            .spawn((HudPart, CwPart::Place(i), Node { width: Val::Px(PLACE), height: Val::Px(PLACE), border: UiRect::all(Val::Px(BORDER)), display: none, ..abs() }, BackgroundColor(Color::NONE), BorderColor::all(Color::NONE), GlobalZIndex(3)))
            .with_children(|p| {
                p.spawn((Node { left: Val::Px(0.0), top: Val::Px(28.0), width: percent(100.0), justify_content: JustifyContent::Center, ..abs() },)).with_children(|c| {
                    c.spawn((CwPart::Code(i), hud_caps(&fonts.bold, "", 28.0, fg(), h, 0.04)));
                });
                p.spawn((CwPart::Count(i), Node { right: Val::Px(7.0), bottom: Val::Px(5.0), ..abs() }, hud_text(&fonts.mono, "", 15.0, fg(), h)));
                p.spawn((CwPart::Cross(i), Node { left: Val::Px(0.0), top: Val::Px(6.0), width: percent(100.0), justify_content: JustifyContent::Center, display: none, ..abs() }, hud_text(&fonts.bold, "", 56.0, red(), h)));
                p.spawn((CwPart::Tag(i), Node { left: Val::Px(-(TAG_W - PLACE) * 0.5 - BORDER), top: Val::Px(-34.0), width: Val::Px(TAG_W), justify_content: JustifyContent::Center, display: none, ..abs() })).with_children(|t| {
                    t.spawn((Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), border: UiRect::all(Val::Px(BORDER)), ..default() }, BackgroundColor(rgb(0x2e, 0x17, 0x13)), BorderColor::all(red()))).with_children(|b| {
                        b.spawn((CwPart::TagText(i), hud_caps(&fonts.bold, "", 14.0, rgb(0xff, 0xb4, 0xa8), h, 0.11)));
                    });
                });
                if i >= BAR_SLOTS + MAX_PLACES {
                    // The worn places: a caption under the place, the gauge in it.
                    p.spawn((Node { left: Val::Px(-20.0), top: Val::Px(PLACE + 4.0), width: Val::Px(PLACE + 36.0), justify_content: JustifyContent::Center, ..abs() },)).with_children(|c| {
                        c.spawn((CwPart::Caption(i), hud_text(&fonts.mono, "", 13.0, dim(), h)));
                    });
                    p.spawn((CwPart::GaugeTrack(i), Node { left: Val::Px(4.0), bottom: Val::Px(4.0), width: Val::Px(PLACE - 4.0 * 2.0 - 2.0 * BORDER), height: Val::Px(8.0), border: UiRect::all(Val::Px(1.5)), display: none, ..abs() }, BackgroundColor(rgb(0x0b, 0x13, 0x18)), BorderColor::all(accent()))).with_children(|g| {
                        g.spawn((CwPart::GaugeFill(i), Node { left: Val::Px(0.0), top: Val::Px(0.0), height: percent(100.0), width: percent(0.0), ..abs() }, BackgroundColor(accent())));
                    });
                }
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
    /// A place that has nothing to do now (the canister place without a suit).
    Dim,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_window(
    items: Res<Items>,
    cw: Res<ContainerWindow>,
    ui: Res<InventoryUi>,
    window: Res<HudWindow>,
    controls: Res<Controls>,
    look: Res<crate::displays::DisplayLook>,
    specs: Res<ContainerSpecs>,
    worn: Option<Res<MaskWorn>>,
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
    let show_box = container.is_some();
    let show = ui.open;
    let (n, cols) = container.map_or((0, 4), |(r, c)| (c.len().min(MAX_PLACES), specs.get(r).map_or(4, |s| s.cols)));
    let bar = &look.0.hud.bar;
    let geo = geometry(window.0, n, cols, bar);
    let wp = worn_places(window.0, bar);
    let dragging = cw.dragging();
    let me_worn = worn_slot(&worn, items.me());
    let suit_stack = me_worn.and_then(|i| items.client.view.slot(i));
    let is_suit = suit_stack.is_some_and(|s| s.item.kind.as_str() == crate::dev_area::gear::SUIT_ITEM);
    let code_of = |s: &item_core::container::Stack| items.defs.of(s).map(|d| d.short_code()).unwrap_or_default();
    let stack_at = |p: Place| match p {
        Place::Bar(i) => items.client.view.slot(i),
        Place::Box(i) => container.and_then(|(_, c)| c.slot(i)),
        Place::Suit => suit_stack,
        Place::Air => None,
    };
    let reason = |why: &Reject, p: Place| reason_text(why, container.map(|(_, c)| c), if let Place::Box(i) = p { Some(i) } else { None }, p);
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
        if p == Place::Air && !is_suit {
            return Look::Dim;
        }
        if !dragging && cw.hover == Some(p) && p != Place::Air { Look::Hover } else { Look::Idle }
    };
    let live = |p: Place| match p {
        Place::Bar(_) | Place::Suit | Place::Air => show,
        Place::Box(i) => show_box && i < n,
    };
    // The suit's tank: today's condition of the worn gear (0..1).
    let air = suit_stack.filter(|_| is_suit).and_then(|s| s.item.condition).map(|c| (c / 100.0).clamp(0.0, 1.0));
    let air_colour = |a: f64| if a <= 0.0 { red() } else if a < LOW_AIR { amber() } else { accent() };

    for (part, mut node) in &mut nodes {
        match *part {
            CwPart::Panel => {
                edit(&mut node, |nd| {
                    nd.display = if show_box { Display::Flex } else { Display::None };
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
            CwPart::BarHeader => edit(&mut node, |nd| nd.width = Val::Px(geo.bar_panel.1.x - 2.0 * PAD - WORN_EXTRA)),
            CwPart::Sep => edit(&mut node, |nd| nd.height = Val::Px(geo.bar_panel.1.y - 2.0 * PAD - BAR_PAD_BOTTOM)),
            CwPart::Place(idx) => {
                let p = Place::of_part(idx);
                let at = match p {
                    Place::Bar(i) => slot_rect(i, true, window.0, bar).0,
                    Place::Box(i) => geo.places.get(i).copied().unwrap_or(Vec2::ZERO),
                    Place::Suit => wp[0],
                    Place::Air => wp[1],
                };
                let l = look_of(p);
                let visible = live(p) && (!matches!(p, Place::Bar(_)) || matches!(l, Look::Refuse(_) | Look::Accept));
                edit(&mut node, |nd| {
                    nd.display = if visible { Display::Flex } else { Display::None };
                    nd.left = Val::Px(at.x);
                    nd.top = Val::Px(at.y);
                });
            }
            CwPart::Cross(idx) | CwPart::Tag(idx) => {
                let p = Place::of_part(idx);
                let refused = live(p) && matches!(look_of(p), Look::Refuse(_));
                set_display(&mut node, if refused { Display::Flex } else { Display::None });
            }
            CwPart::GaugeTrack(idx) => {
                let on = live(Place::of_part(idx)) && Place::of_part(idx) == Place::Air && air.is_some();
                set_display(&mut node, if on { Display::Flex } else { Display::None });
            }
            CwPart::GaugeFill(_) => edit(&mut node, |nd| nd.width = percent(air.unwrap_or(0.0) as f32 * 100.0)),
            CwPart::Ghost => {
                let from = cw.drag.filter(|_| dragging).filter(|p| !matches!(p, Place::Bar(_)));
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
        match *part {
            CwPart::Place(idx) => {
                let p = Place::of_part(idx);
                let bar_layer = matches!(p, Place::Bar(_));
                let want = match look_of(p) {
                    Look::Refuse(_) => red(),
                    Look::Hover => accent(),
                    Look::Accept => Color::NONE,
                    Look::Dim => rgb(0x2c, 0x3a, 0x44),
                    _ if bar_layer => Color::NONE,
                    Look::Lifted => line(),
                    Look::Idle if stack_at(p).is_some() || (p == Place::Air && air.is_some()) => edge(),
                    _ => line(),
                };
                set_border(&mut border, want);
            }
            CwPart::GaugeTrack(_) => set_border(&mut border, air_colour(air.unwrap_or(1.0))),
            _ => {}
        }
    }
    for (part, mut bg) in &mut fills {
        match *part {
            CwPart::Place(idx) => {
                let p = Place::of_part(idx);
                let bar_layer = matches!(p, Place::Bar(_));
                let want = match look_of(p) {
                    Look::Refuse(_) => rgb(0x2e, 0x17, 0x13),
                    Look::Hover | Look::Accept if !bar_layer => rgb(0x15, 0x30, 0x3f),
                    Look::Hover | Look::Accept => Color::NONE,
                    Look::Lifted => rgb(0x0b, 0x13, 0x18),
                    Look::Dim => rgb(0x0b, 0x13, 0x18),
                    Look::Idle if bar_layer => Color::NONE,
                    Look::Idle if stack_at(p).is_some() || (p == Place::Air && air.is_some()) => rgb(0x1a, 0x2a, 0x33),
                    Look::Idle => rgb(0x0b, 0x13, 0x18),
                };
                set_bg(&mut bg, want);
            }
            CwPart::GaugeFill(_) => set_bg(&mut bg, air_colour(air.unwrap_or(1.0))),
            _ => {}
        }
    }
    for i in 0..N_PARTS {
        let p = Place::of_part(i);
        let accepting = live(p) && look_of(p) == Look::Accept;
        set_dashes(DASHES + i as u32, accepting, accent(), &mut frames, &mut dashes);
    }
    let (title, rule, status) = match container {
        Some((r, c)) => (specs.get(r).map_or("CONTAINER".to_string(), |s| s.name.to_uppercase()), rule_text(c), status_text(c)),
        None => (String::new(), String::new(), (String::new(), fg())),
    };
    let ghost = cw.drag.filter(|_| dragging).and_then(stack_at);
    let shown = |p: Place| live(p) && !matches!(look_of(p), Look::Lifted | Look::Refuse(_));
    for (part, mut text, mut colour) in &mut texts {
        match *part {
            CwPart::Title => set_text(&mut text, &title.to_uppercase()),
            CwPart::Rule => set_text(&mut text, &rule),
            CwPart::Status => {
                set_text(&mut text, &status.0.to_uppercase());
                set_text_colour(&mut colour, status.1);
            }
            CwPart::Code(idx) => {
                let p = Place::of_part(idx);
                let t = match p {
                    Place::Bar(_) => String::new(),
                    Place::Air => if shown(p) && air.is_some() { "AIR".to_string() } else { String::new() },
                    _ => if shown(p) { stack_at(p).map(code_of).unwrap_or_default() } else { String::new() },
                };
                set_text(&mut text, &t);
            }
            CwPart::Count(idx) => {
                let p = Place::of_part(idx);
                let t = if matches!(p, Place::Bar(_) | Place::Suit | Place::Air) || !shown(p) { String::new() } else { stack_at(p).filter(|s| s.count > 1).map(|s| format!("×{}", s.count)).unwrap_or_default() };
                set_text(&mut text, &t);
            }
            CwPart::Cross(idx) => {
                let p = Place::of_part(idx);
                set_text(&mut text, if live(p) && matches!(look_of(p), Look::Refuse(_)) { "×" } else { "" });
            }
            CwPart::TagText(idx) => {
                if let Look::Refuse(t) = look_of(Place::of_part(idx)) {
                    set_text(&mut text, &t);
                }
            }
            CwPart::Caption(idx) => {
                let (t, c) = match Place::of_part(idx) {
                    Place::Suit => match suit_stack {
                        Some(_) if is_suit => ("SUIT".to_string(), dim()),
                        Some(_) => ("MASK".to_string(), dim()),
                        None => ("NO SUIT".to_string(), dim()),
                    },
                    _ => match air {
                        Some(a) if a <= 0.0 => ("× EMPTY".to_string(), red()),
                        Some(a) if a < LOW_AIR => (format!("! LOW {:.0}%", a * 100.0), amber()),
                        Some(a) => (format!("AIR {:.0}%", a * 100.0), dim()),
                        None if is_suit => ("NO CANISTER".to_string(), dim()),
                        None => (String::new(), dim()),
                    },
                };
                set_text(&mut text, &t);
                set_text_colour(&mut colour, c);
            }
            CwPart::GhostCode => set_text(&mut text, &ghost.map(code_of).unwrap_or_default()),
            CwPart::GhostCount => set_text(&mut text, &ghost.filter(|s| s.count > 1).map(|s| format!("×{}", s.count)).unwrap_or_default()),
            _ => {}
        }
    }
}
