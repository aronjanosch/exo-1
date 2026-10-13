//! Items (C1 of the interaction model; concept repo `design/interaction.md`): small things the
//! player collects into the action bar, holds, drops and throws. `item_core` holds the rules and
//! the request path; this is the glue.
//!
//! - Every change goes as a request from the local `Client` to the `Host` (`Items::request`).
//!   Host and client run in this process for now; `Items::delay` holds each message back as the
//!   internet would (the scenario plays with 75 ms each way). The client shows its request at
//!   once; the world (loose item entities) follows the events the client receives.
//! - A loose item is a `cargo::Crate` body flagged `item`: it falls, slides, sleeps and lives in
//!   the ship's frame like a crate, but thrust does not push it around.
//! - Keys: F collects (the interact target), 1 to 8 take a slot into the hand, G drops (a held
//!   crate is set down instead), R throws (the crate first), Tab opens the inventory with a mouse
//!   cursor: a click takes a slot into the hand, a drag moves, stacks or swaps.
use crate::cargo::{item_body_bundle, Crate};
use crate::crew_hud::{Dash, DashFrame};
use crate::controls::{Actions, Controls, Tap};
use crate::env::PlanetRes;
use crate::grab::Grab;
use crate::ship::Ship;
use crate::walker::{cabin_frame, CabinFloor, Player, EYE_HEIGHT};
use avian3d::prelude::*;
use bevy::math::DVec3;
use bevy::prelude::*;
use grab_core::CrateSize;
use item_core::container::Stack;
use item_core::{Action, Client, Event, FrameRef, Host, ItemDefs, LooseId, Placement, PlayerId, Request};
use walker_core::Frame;

/// The local player in the session (the host's own).
pub const LOCAL: PlayerId = 1;
/// A dropped item appears this far in front of the eye (m).
const DROP_AHEAD: f64 = 0.6;

pub fn plugin(app: &mut App) {
    app.insert_resource(Items::load()).init_resource::<InventoryUi>().init_resource::<HudWindow>();
    app.add_systems(
        FixedUpdate,
        (item_input, deliver)
            .chain()
            .after(crate::interact::interaction)
            .after(crate::displays::use_display)
            .before(crate::walker::walker_step)
            .in_set(crate::phases::Fx::Walker),
    );
}

/// Window only: item looks, the held item, the action bar.
pub fn window_plugin(app: &mut App) {
    use crate::phases::Frame;
    app.add_systems(Startup, spawn_bar);
    app.add_systems(Update, (dress_items, hide_items).chain().in_set(Frame::Sync));
    app.add_systems(Update, held_item.in_set(Frame::World));
    app.add_systems(Update, (window_size, draw_bar).chain().in_set(Frame::Hud));
}

/// Every item file under `content/items/`.
pub fn load_defs() -> ItemDefs {
    let files = crate::content_files("items");
    ItemDefs::from_files(files.iter().map(|f| (f.path.as_str(), f.text.as_str()))).unwrap_or_else(|e| panic!("items: {e}"))
}

#[derive(Resource)]
pub struct Items {
    pub defs: ItemDefs,
    pub host: Host,
    pub client: Client,
    /// One-way delay of every message (s): 0 in one process, the internet's in tests.
    pub delay: f64,
    /// Actions on the way to the host and events on the way back, with their arrival time.
    outbox: Vec<(f64, Action)>,
    inbox: Vec<(f64, Event)>,
    /// Every event the local client received (scenario checks).
    pub received: Vec<Event>,
}

impl Items {
    pub fn load() -> Items {
        let defs = load_defs();
        Items { host: Host::new(defs.clone()), client: Client::new(LOCAL, defs.clone()), defs, delay: 0.0, outbox: Vec::new(), inbox: Vec::new(), received: Vec::new() }
    }

    /// The local player asks; it shows at once. The action's number, or why it cannot work.
    pub fn request(&mut self, now: f64, req: Request) -> Result<u32, item_core::Reject> {
        let a = self.client.request(req)?;
        let seq = a.seq;
        self.outbox.push((now + self.delay, a));
        Ok(seq)
    }

    /// Another player's action arriving at the host (the network later; scenarios now).
    pub fn remote(&mut self, now: f64, a: Action) {
        self.outbox.push((now, a));
    }

    /// The host puts an item into the world (level content).
    pub fn spawn(&mut self, now: f64, stack: Stack, at: Placement) {
        let ev = self.host.spawn(stack, at, None);
        self.inbox.push((now + self.delay, ev));
    }

    /// Delivers what has arrived: actions to the host, its events to the client. The events the
    /// client applied, in order.
    fn step(&mut self, now: f64) -> Vec<Event> {
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.outbox).into_iter().partition(|(t, _)| *t <= now);
        self.outbox = later;
        for (_, a) in due {
            for ev in self.host.handle(&a) {
                self.inbox.push((now + self.delay, ev));
            }
        }
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.inbox).into_iter().partition(|(t, _)| *t <= now);
        self.inbox = later;
        let evs: Vec<Event> = due.into_iter().map(|(_, e)| e).collect();
        for e in &evs {
            self.client.apply(e);
        }
        self.received.extend(evs.iter().cloned());
        evs
    }

    /// Hands the cargo in the hands takes (0 without).
    pub fn in_hand(&self, carrying: u8) -> Option<&Stack> {
        self.client.view.in_hand(&self.defs, carrying)
    }

    pub fn name(&self, s: &Stack) -> String {
        self.defs.of(s).map_or_else(|| s.item.kind.to_string(), |d| d.name.clone())
    }
}

/// A loose item entity: the host's id once known; a drop shown before the host confirmed it
/// carries the number of its action instead.
#[derive(Component, Clone, Debug)]
pub struct LooseItem {
    pub id: Option<LooseId>,
    pub def: String,
    pub count: u32,
    pub pending: Option<u32>,
}

/// The inventory screen (Tab) and a drag in it.
#[derive(Resource, Default, Debug, Clone)]
pub struct InventoryUi {
    pub open: bool,
    /// Slot under the pointer when the button went down.
    pub drag_from: Option<usize>,
    was_down: bool,
    /// A slot key, drop or throw was pressed since the HUD last looked (it wakes the dimmed bar).
    pub touched: bool,
}

/// The window's size in logical pixels, for the slot layout (headless runs keep the default).
#[derive(Resource, Clone, Copy, Debug)]
pub struct HudWindow(pub Vec2);

impl Default for HudWindow {
    fn default() -> Self {
        HudWindow(Vec2::new(1600.0, 900.0))
    }
}

/// Slot `i` of the action bar on screen: top left and size in window pixels. Closed it sits at
/// the bottom; open (Tab) it is larger, in the middle. The sizes are `hud.bar` in
/// `content/ui/displays.json` (Crew HUD v3).
pub fn slot_rect(i: usize, open: bool, window: Vec2, look: &display_core::hud::BarLook) -> (Vec2, f32) {
    let (at, size) = display_core::hud::bar_slot(i, item_core::BAR_SLOTS, open, window.as_dvec2(), look);
    (at.as_vec2(), size as f32)
}

/// The slot under a window point.
pub fn slot_at(p: Vec2, open: bool, window: Vec2, look: &display_core::hud::BarLook) -> Option<usize> {
    (0..item_core::BAR_SLOTS).find(|&i| {
        let (at, s) = slot_rect(i, open, window, look);
        p.x >= at.x && p.x <= at.x + s && p.y >= at.y && p.y <= at.y + s
    })
}

/// The shape of an item as a body (`grab_core`'s crate size; one holder).
pub fn body_size(defs: &ItemDefs, id: &str) -> Option<CrateSize> {
    let d = defs.get(id)?;
    Some(CrateSize { name: d.id.clone(), extents: d.shape.extents, mass: d.shape.mass, hands: d.hands, holders: 1 })
}

/// Where the walker's hand is, as a placement in its frame (the cabin or the planet), moving at
/// `vel` (frame coordinates).
fn hand_placement(pl: &Player, f: &Frame, planet: &PlanetRes, throw: Option<(f64, &grab_core::GrabConfig)>) -> Placement {
    let in_cabin = pl.ship.is_some();
    let (eye, look, up) = if in_cabin {
        (pl.w.pos + pl.cabin_up * EYE_HEIGHT, f.rot.inverse() * pl.world_look(*f), pl.cabin_up)
    } else {
        (pl.w.pos + pl.up * EYE_HEIGHT, pl.world_look(Frame::IDENTITY), pl.up)
    };
    let pos = eye + look * DROP_AHEAD - up * 0.25;
    let vel = match throw {
        Some((mass, cfg)) => grab_core::throw_velocity(cfg, mass, pl.w.vel, look),
        None => pl.w.vel,
    };
    let fwd = (look - up * look.dot(up)).try_normalize().unwrap_or(up.any_orthonormal_vector());
    Placement { frame: if in_cabin { FrameRef::Ship(LOCAL) } else { FrameRef::Planet(planet.id.0 as u32) }, pos: pos.to_array(), vel: vel.to_array(), forward: fwd.to_array() }
}

/// The keys and the inventory screen.
#[allow(clippy::too_many_arguments)]
pub fn item_input(
    mut commands: Commands,
    time: Res<Time>,
    tuning: Res<crate::tuning::Tuning>,
    planet: Res<PlanetRes>,
    mut actions: ResMut<Actions>,
    mut controls: ResMut<Controls>,
    mut items: ResMut<Items>,
    mut ui: ResMut<InventoryUi>,
    mut grab: ResMut<Grab>,
    window: Res<HudWindow>,
    look: Res<crate::displays::DisplayLook>,
    dev: Option<Res<crate::dev::DevMenu>>,
    active: Res<crate::displays::ActiveDisplay>,
    players: Query<&Player>,
    ships: Query<(Entity, &Position, &Rotation), With<Ship>>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
) {
    let now = time.elapsed_secs_f64();
    let Ok(pl) = players.single() else { return };
    let Some((ship_e, sp, sr)) = ships.iter().next() else { return };
    let frame = cabin_frame(ship_e, (sp, sr), &floors);
    let busy = pl.seated || pl.fly || active.using().is_some();
    if actions.take_tap(Tap::Inventory) && !busy {
        ui.open = !ui.open;
    }
    if busy {
        ui.open = false;
    }
    if !dev.is_some_and(|d| d.open) {
        for (i, t) in Tap::SLOTS.iter().enumerate() {
            if actions.take_tap(*t) && !busy {
                ui.touched = true;
                let _ = items.request(now, Request::Select { slot: i });
            }
        }
    }
    let carrying = grab.held.map_or(0, |h| h.hands);
    let drop = actions.take_tap(Tap::Drop);
    // R throws the held crate first (`grab_step` takes it then).
    let throw = grab.held.is_none() && actions.take_tap(Tap::Throw);
    ui.touched |= drop || throw;
    if drop && grab.held.is_some() {
        grab.release();
    } else if (drop || throw) && !busy {
        let slot = items.client.view.selected;
        if let Some(s) = items.in_hand(carrying).cloned() {
            let mass = items.defs.of(&s).map_or(1.0, |d| d.shape.mass);
            let at = hand_placement(pl, &frame, &planet, throw.then_some((mass, &tuning.grab)));
            let req = if throw { Request::Throw { slot, at } } else { Request::Drop { slot, at } };
            if let Ok(seq) = items.request(now, req) {
                let li = LooseItem { id: None, def: s.item.kind.to_string(), count: s.count, pending: Some(seq) };
                spawn_loose(&mut commands, &items.defs, li, &at, ship_e);
            }
        }
    }
    // The inventory screen: the cursor is free; a click takes a slot into the hand, a drag moves.
    controls.free_cursor |= ui.open;
    let down = controls.pointer_down || controls.click;
    if ui.open {
        let under = controls.pointer.and_then(|p| slot_at(p, true, window.0, &look.0.hud.bar));
        if down && !ui.was_down {
            ui.drag_from = under;
        }
        if !down && ui.was_down
            && let Some(from) = ui.drag_from.take()
        {
            let _ = match under {
                Some(to) if to != from => items.request(now, Request::Move { from, to }),
                Some(_) => items.request(now, Request::Select { slot: from }),
                None => Ok(0),
            };
        }
        controls.click = false;
    } else {
        ui.drag_from = None;
    }
    ui.was_down = ui.open && down;
}

/// A loose item entity at a placement.
fn spawn_loose(commands: &mut Commands, defs: &ItemDefs, li: LooseItem, at: &Placement, ship_e: Entity) {
    let Some(size) = body_size(defs, &li.def) else { return };
    let ship = matches!(at.frame, FrameRef::Ship(_)).then_some(ship_e);
    commands.spawn((li, item_body_bundle(&size, ship, DVec3::from_array(at.pos), DVec3::from_array(at.vel), DVec3::from_array(at.forward))));
}

/// Messages arrive; the world follows what the client hears.
pub fn deliver(mut commands: Commands, time: Res<Time>, mut items: ResMut<Items>, ships: Query<Entity, With<Ship>>, mut loose: Query<(Entity, &mut LooseItem)>) {
    let evs = items.step(time.elapsed_secs_f64());
    let Some(ship_e) = ships.iter().next() else { return };
    for ev in evs {
        match ev {
            Event::Spawned { item, loose: l, by } => {
                let own = by.filter(|(p, _)| *p == LOCAL).map(|(_, seq)| seq);
                if let Some(seq) = own
                    && let Some((_, mut li)) = loose.iter_mut().find(|(_, li)| li.pending == Some(seq))
                {
                    li.id = Some(item);
                    li.pending = None;
                    continue;
                }
                let li = LooseItem { id: Some(item), def: l.stack.item.kind.to_string(), count: l.stack.count, pending: None };
                spawn_loose(&mut commands, &items.defs, li, &l.at, ship_e);
            }
            Event::Removed { item } => {
                for (e, li) in &loose {
                    if li.id == Some(item) {
                        commands.entity(e).despawn();
                    }
                }
            }
            Event::Rejected { player: LOCAL, seq, .. } => {
                // A drop the host refused: its item was never there.
                for (e, li) in &loose {
                    if li.pending == Some(seq) {
                        commands.entity(e).despawn();
                    }
                }
            }
            _ => {}
        }
    }
}

/// Whether the local player sees and can reach this loose item (a pending collect hides it).
pub fn visible(items: &Items, li: &LooseItem) -> bool {
    li.id.is_none_or(|id| !items.client.hidden.contains(&id))
}

/// Window: a box in the item's colour.
fn dress_items(mut commands: Commands, items: Res<Items>, new: Query<(Entity, &LooseItem), Added<LooseItem>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for (e, li) in &new {
        let Some(d) = items.defs.get(&li.def) else { continue };
        let [x, y, z] = d.shape.extents.map(|v| v as f32);
        let [r, g, b] = d.shape.colour;
        commands.entity(e).insert((
            Mesh3d(meshes.add(Cuboid::new(x, y, z))),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.6, ..default() })),
            crate::origin::WorldPose::default(),
        ));
    }
}

fn hide_items(items: Res<Items>, mut q: Query<(&LooseItem, &mut Visibility), With<Crate>>) {
    for (li, mut v) in &mut q {
        let want = if visible(&items, li) { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
    }
}

/// The item in the hand, in front of the camera (lower right).
#[derive(Component)]
struct HeldModel(Option<String>);

#[allow(clippy::too_many_arguments)]
fn held_item(
    mut commands: Commands,
    items: Res<Items>,
    grab: Res<Grab>,
    cam: Query<&crate::origin::WorldPose, (With<crate::view::MainCamera>, Without<HeldModel>)>,
    players: Query<&Player>,
    mut held: Query<(Entity, &mut HeldModel, &mut crate::origin::WorldPose, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let carrying = grab.held.map_or(0, |h| h.hands);
    let seated = players.single().map(|p| p.seated).unwrap_or(true);
    let want = if seated { None } else { items.in_hand(carrying).map(|s| s.item.kind.to_string()) };
    let Ok(c) = cam.single() else { return };
    let Some((e, mut m, mut pose, mut vis)) = held.iter_mut().next() else {
        commands.spawn((HeldModel(None), crate::origin::WorldPose::default(), Transform::default(), Visibility::Hidden));
        return;
    };
    if m.0 != want {
        m.0 = want.clone();
        if let Some(d) = want.as_deref().and_then(|id| items.defs.get(id)) {
            let [x, y, z] = d.shape.extents.map(|v| v as f32);
            let [r, g, b] = d.shape.colour;
            commands.entity(e).insert((Mesh3d(meshes.add(Cuboid::new(x, y, z))), MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.6, ..default() }))));
        }
    }
    *vis = if m.0.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    pose.pos = c.pos + c.rot * DVec3::new(0.34, -0.34, -0.7);
    pose.rot = c.rot;
}

fn window_size(windows: Query<&Window, With<bevy::window::PrimaryWindow>>, mut size: ResMut<HudWindow>) {
    if let Ok(w) = windows.single() {
        let s = Vec2::new(w.width(), w.height());
        if size.0 != s {
            size.0 = s;
        }
    }
}

// ---------- the action bar on screen (Crew HUD v3) ----------
//
// Eight 56 px slots at the bottom centre. The held slot lifts and has a white outline; a slot
// shows the item's colour, its stack count and "2H" for two hands; only the held item's name is
// written, under the bar at the left. After a few idle seconds everything but the held slot drops
// to half contrast; while the hands carry cargo the slots are dashed and dimmed. Tab: the slots
// grow, the hovered or held name stands at the left above them, the keys at the right.

#[derive(Component)]
struct BarSlot(usize);
#[derive(Component)]
struct BarKey(usize);
#[derive(Component)]
struct BarSwatch(usize);
#[derive(Component)]
struct BarCount(usize);
#[derive(Component)]
struct BarHands(usize);
/// The held item's name: under the bar, or above the open bar at the left.
#[derive(Component)]
struct BarName;
/// The keys of the open inventory, right-aligned above the slots.
#[derive(Component)]
struct BarKeys;
#[derive(Component)]
struct BarKeysText;

fn spawn_bar(mut commands: Commands, look: Res<crate::displays::DisplayLook>, assets: Res<AssetServer>) {
    use crate::crew_hud::{col, col_a, hud_text, spawn_dashes};
    let theme = &look.0;
    let (k, h, b) = (&theme.colours, &theme.hud, &theme.hud.bar);
    let (display, mono) = (assets.load(&theme.fonts.display), assets.load(&theme.fonts.mono));
    let abs = || Node { position_type: PositionType::Absolute, ..default() };
    for i in 0..item_core::BAR_SLOTS {
        commands
            .spawn((
                BarSlot(i),
                Node { border: UiRect::all(Val::Px(b.outline_px as f32)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..abs() },
                BorderColor::all(col_a(k.hud_dim, 0.6)),
                BackgroundColor(col_a(k.bg_0, 0.6)),
            ))
            .with_children(|p| {
                p.spawn((BarKey(i), Node { left: Val::Px(4.0), top: Val::Px(2.0), ..abs() }, hud_text(&mono, &format!("{}", i + 1), b.count_size - 1.0, col(k.fg_3), h)));
                p.spawn((BarHands(i), Node { right: Val::Px(4.0), top: Val::Px(2.0), ..abs() }, hud_text(&mono, "", b.count_size - 2.0, col(k.accent), h)));
                p.spawn((BarCount(i), Node { right: Val::Px(4.0), bottom: Val::Px(2.0), ..abs() }, hud_text(&mono, "", b.count_size, col(k.fg_1), h)));
                p.spawn((BarSwatch(i), Node { width: Val::Px(b.swatch as f32), height: Val::Px(b.swatch as f32), border: UiRect::all(Val::Px(1.0)), border_radius: BorderRadius::all(Val::Px(3.0)), display: Display::None, ..default() }, BackgroundColor(Color::WHITE), BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.5))));
                spawn_dashes(p, i as u32, (b.slot as f32, b.slot as f32), b.outline_px as f32, b.outline_px as f32, h, col(k.hud_dim));
            });
    }
    commands.spawn((BarName, Node { display: Display::None, ..abs() }, hud_text(&display, "", b.name_size, col(k.fg_1), h)));
    commands.spawn((BarKeys, Node { display: Display::None, width: Val::Px(700.0), justify_content: JustifyContent::FlexEnd, ..abs() })).with_children(|p| {
        p.spawn((BarKeysText, hud_text(&display, "", b.name_size - 1.0, col(k.fg_1), h)));
    });
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_bar(
    items: Res<Items>,
    ui: Res<InventoryUi>,
    window: Res<HudWindow>,
    grab: Res<Grab>,
    controls: Res<Controls>,
    (look, hud, bindings): (Res<crate::displays::DisplayLook>, Res<crate::crew_hud::CrewHud>, Res<crate::controls::Bindings>),
    mut slots: Query<(&BarSlot, &mut Node, &mut BorderColor, &mut BackgroundColor), (Without<BarSwatch>, Without<BarName>, Without<BarKeys>, Without<DashFrame>, Without<Dash>)>,
    mut keys: Query<(&BarKey, &mut TextColor), (Without<BarCount>, Without<BarHands>)>,
    mut swatches: Query<(&BarSwatch, &mut Node, &mut BackgroundColor), (Without<BarSlot>, Without<BarName>, Without<BarKeys>, Without<DashFrame>, Without<Dash>)>,
    mut counts: Query<(&BarCount, &mut Text, &mut TextColor), (Without<BarKey>, Without<BarHands>, Without<BarName>, Without<BarKeysText>)>,
    mut hands: Query<(&BarHands, &mut Text, &mut TextColor), (Without<BarKey>, Without<BarCount>, Without<BarName>, Without<BarKeysText>)>,
    mut name: Query<(&mut Node, &mut Text), (With<BarName>, Without<BarSlot>, Without<BarSwatch>, Without<BarKeys>, Without<DashFrame>, Without<BarCount>, Without<BarHands>, Without<BarKeysText>)>,
    mut keys_box: Query<&mut Node, (With<BarKeys>, Without<BarSlot>, Without<BarSwatch>, Without<BarName>, Without<DashFrame>)>,
    mut keys_text: Query<&mut Text, (With<BarKeysText>, Without<BarName>, Without<BarCount>, Without<BarHands>)>,
    mut frames: Query<(&DashFrame, &mut Node), (Without<BarSlot>, Without<BarSwatch>, Without<BarName>, Without<BarKeys>)>,
    mut dashes: Query<(&Dash, &mut BackgroundColor), (Without<BarSlot>, Without<BarSwatch>)>,
) {
    use crate::crew_hud::{col_a, set_dashes};
    let (theme, b) = (&look.0, &look.0.hud.bar);
    let k = &theme.colours;
    let view = &items.client.view;
    let carrying = grab.held.map_or(0, |h| h.hands);
    let win = window.0;
    let hover = controls.pointer.and_then(|p| slot_at(p, true, win, b)).filter(|_| ui.open);
    let contrast_of = |sel: bool| -> f64 {
        if ui.open {
            1.0
        } else if carrying > 0 {
            b.carry_contrast
        } else if sel {
            1.0
        } else {
            hud.contrast
        }
    };
    let set_c = |c: &mut TextColor, want: Color| {
        if c.0 != want {
            c.0 = want;
        }
    };
    for (s, mut n, mut border, mut bg) in &mut slots {
        let (at, size) = slot_rect(s.0, ui.open, win, b);
        let selected = view.selected == s.0;
        let lift = if selected && !ui.open { b.lift as f32 } else { 0.0 };
        n.left = Val::Px(at.x);
        n.top = Val::Px(at.y - lift);
        n.width = Val::Px(size);
        n.height = Val::Px(size);
        let c = contrast_of(selected);
        let dashed = carrying > 0 && !ui.open;
        let want = BorderColor::all(if dashed {
            Color::NONE
        } else if selected {
            col_a(k.fg_title, 1.0)
        } else {
            col_a(k.hud_dim, c)
        });
        if *border != want {
            *border = want;
        }
        let lit = hover == Some(s.0) || ui.drag_from == Some(s.0);
        bg.0 = if lit { col_a(k.bg_3, 0.85) } else { col_a(k.bg_0, 0.6 * c) };
        set_dashes(s.0 as u32, dashed, col_a(k.hud_dim, c), &mut frames, &mut dashes);
    }
    let scale = |open: bool| if open { b.open_slot / b.slot } else { 1.0 };
    for (s, mut n, mut bg) in &mut swatches {
        let stack = view.slot(s.0);
        let def = stack.and_then(|st| items.defs.of(st));
        let want = if def.is_some() { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
        let edge = Val::Px((b.swatch * scale(ui.open)) as f32);
        if n.width != edge {
            n.width = edge;
            n.height = edge;
        }
        if let Some(d) = def {
            let [r, g, bl] = d.shape.colour;
            let c = contrast_of(view.selected == s.0);
            bg.0 = Color::srgba(r, g, bl, c as f32);
        }
    }
    for (kx, mut c) in &mut keys {
        set_c(&mut c, col_a(k.fg_3, contrast_of(view.selected == kx.0)));
    }
    for (i, mut t, mut c) in &mut counts {
        let s = view.slot(i.0).filter(|st| st.count > 1).map(|st| format!("x{}", st.count)).unwrap_or_default();
        if t.0 != s {
            t.0 = s;
        }
        set_c(&mut c, col_a(k.fg_1, contrast_of(view.selected == i.0)));
    }
    for (i, mut t, mut c) in &mut hands {
        let s = if view.slot(i.0).and_then(|st| items.defs.of(st)).is_some_and(|d| d.hands == 2) { "2H" } else { "" };
        if t.0 != s {
            t.0 = s.to_string();
        }
        set_c(&mut c, col_a(k.accent, contrast_of(view.selected == i.0)));
    }
    // The name: under the bar at the left, or above the open bar the hovered one (else the held).
    let (first, _) = slot_rect(0, ui.open, win, b);
    let (last, size) = slot_rect(item_core::BAR_SLOTS - 1, ui.open, win, b);
    let shown = if ui.open { hover.and_then(|i| view.slot(i)).map(|st| items.name(st)).unwrap_or_else(|| hud.held_name.clone()) } else { hud.held_name.clone() };
    if let Ok((mut n, mut t)) = name.single_mut() {
        let want = if shown.is_empty() { Display::None } else { Display::Flex };
        if n.display != want {
            n.display = want;
        }
        if t.0 != shown {
            t.0 = shown;
        }
        n.left = Val::Px(first.x);
        n.top = Val::Px(if ui.open { first.y - 30.0 } else { first.y + size + 8.0 });
    }
    if let Ok(mut n) = keys_box.single_mut() {
        let want = if ui.open { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
        n.left = Val::Px(last.x + size - 700.0);
        n.top = Val::Px(first.y - 28.0);
    }
    if let Ok(mut t) = keys_text.single_mut() {
        let s = format!(
            "1-8 hand   {} drop   {} throw   drag: move   {} close",
            crate::interact::key_label(&bindings, crate::controls::Tap::Drop),
            crate::interact::key_label(&bindings, crate::controls::Tap::Throw),
            crate::interact::key_label(&bindings, crate::controls::Tap::Inventory)
        );
        if t.0 != s {
            t.0 = s;
        }
    }
}
