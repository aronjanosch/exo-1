//! Items (C1 of the interaction model; concept repo `design/interaction.md`): small things the
//! player collects into the action bar, holds, drops and throws. `item_core` holds the rules and
//! the request path; this is the glue.
//!
//! - Every change goes as a request from the local `Client` to the `Host` (`Items::request`).
//!   Host and client run in this process alone (`Role::Local`); `Items::delay` holds each message
//!   back as the internet would (the scenario plays with 75 ms each way). In a network session
//!   (C2, `items_net`) the host process runs the `Host` and sends its events to the clients, a
//!   client process sends its actions to the host and hears the events (`Role`). The client shows
//!   its request at once; the world (loose item entities) follows the events the client receives.
//! - A loose item is a `cargo::Crate` body flagged `item`: it falls, slides, sleeps and lives in
//!   the ship's frame like a crate, but thrust does not push it around.
//! - Keys: F collects (the interact target), 1 to 8 take a slot into the hand, G drops (a held
//!   crate is set down instead), R throws (the crate first), Tab opens the inventory with a mouse
//!   cursor: a click takes a slot into the hand, a drag moves, stacks or swaps.
use crate::cargo::{item_body_bundle, Crate};
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
use item_core::{Action, Client, Event, FrameRef, Host, ItemDefs, LooseId, Placement, PlayerId, Reject, Request};
use walker_core::Frame;

/// The local player in a one-process game, and the host's own in a session. Slot of the ship
/// everybody boards: items in a cabin live in the host's ship.
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

/// Which side of a session this process is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Role {
    /// No network: host and client in this process.
    #[default]
    Local,
    /// The host (slot 1): runs the `Host`, its own player is the local client, other players'
    /// actions come in over the network and its events go out to them.
    Host,
    /// A client: sends its actions to the host, hears the host's events. The `host` field is idle.
    Client,
}

/// What `Items` hands to the network glue (`items_net`).
#[derive(Clone, Debug, PartialEq)]
pub enum NetOut {
    /// Client to host.
    Action(Action),
    /// Host to the clients: one player (an answer to their action) or all (`to` None).
    Event { to: Option<PlayerId>, ev: Event },
}

#[derive(Resource)]
pub struct Items {
    pub defs: ItemDefs,
    pub host: Host,
    pub client: Client,
    /// One-way delay of every message (s): 0 in one process, the internet's in tests.
    pub delay: f64,
    pub role: Role,
    /// Actions on the way to the host and events on the way back, with their arrival time.
    outbox: Vec<(f64, Action)>,
    inbox: Vec<(f64, Event)>,
    /// Messages for the network, drained by `items_net`.
    pub net_out: Vec<NetOut>,
    /// Every event the local client received (scenario checks).
    pub received: Vec<Event>,
}

/// Whether the host can simulate an action's placement: a cabin is the host's ship. (Other
/// players' ships are proxies here, with no body to rest an item on.)
fn placement_ok(a: &Action) -> bool {
    match &a.req {
        Request::Drop { at, .. } | Request::Throw { at, .. } => !matches!(at.frame, FrameRef::Ship(n) if n != LOCAL),
        _ => true,
    }
}

impl Items {
    pub fn load() -> Items {
        let defs = load_defs();
        Items { host: Host::new(defs.clone()), client: Client::new(LOCAL, defs.clone()), defs, delay: 0.0, role: Role::Local, outbox: Vec::new(), inbox: Vec::new(), net_out: Vec::new(), received: Vec::new() }
    }

    /// The local player's slot.
    pub fn me(&self) -> PlayerId {
        self.client.player
    }

    /// The local player asks; it shows at once. The action's number, or why it cannot work.
    pub fn request(&mut self, now: f64, req: Request) -> Result<u32, item_core::Reject> {
        let a = self.client.request(req)?;
        let seq = a.seq;
        self.outbox.push((now + self.delay, a));
        Ok(seq)
    }

    /// Another player's action arriving at the host (scenarios; the network uses `from_peer`).
    pub fn remote(&mut self, now: f64, a: Action) {
        self.outbox.push((now, a));
    }

    /// An action from the client in `slot`, arriving over the network at the host. The player is
    /// the connection's, not what the message says: anything else is dropped.
    pub fn from_peer(&mut self, now: f64, slot: PlayerId, a: Action) -> bool {
        if self.role != Role::Host || a.player != slot {
            return false;
        }
        self.outbox.push((now, a));
        true
    }

    /// An event from the host arriving at a client over the network.
    pub fn hear(&mut self, now: f64, ev: Event) {
        self.inbox.push((now, ev));
    }

    /// The host puts an item into the world (level content). A client does not: the host's
    /// events bring the world.
    pub fn spawn(&mut self, now: f64, stack: Stack, at: Placement) {
        if self.role == Role::Client {
            return;
        }
        let ev = self.host.spawn(stack, at, None);
        self.publish(now, ev, None);
    }

    /// A loose item came to rest (host, from the physics): remembered, and told to everybody.
    pub fn rest(&mut self, now: f64, item: LooseId, at: Placement, rot: [f64; 4], t: f64) {
        if let Some(ev) = self.host.rest(item, at, rot, t) {
            self.publish(now, ev, None);
        }
    }

    /// An event of the host: to the local client, and over the network to the others.
    /// `actor`: the player whose action it answers.
    fn publish(&mut self, now: f64, ev: Event, actor: Option<PlayerId>) {
        self.inbox.push((now + self.delay, ev.clone()));
        if self.role != Role::Host {
            return;
        }
        // An answer goes to the player who asked; the host's own never leaves the process.
        let to = match (&ev, actor) {
            (Event::Confirmed { .. } | Event::Rejected { .. }, Some(p)) => Some(p),
            _ => None,
        };
        if to != Some(self.me()) {
            self.net_out.push(NetOut::Event { to, ev });
        }
    }

    /// What a client that joins now needs: the world and every inventory as events, for it alone.
    pub fn send_state(&mut self, to: PlayerId) {
        for ev in self.host.state() {
            self.net_out.push(NetOut::Event { to: Some(to), ev });
        }
    }

    /// Delivers what has arrived: actions to the host (a client sends them on), its events to the
    /// client. The events the client applied, in order.
    fn step(&mut self, now: f64) -> Vec<Event> {
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.outbox).into_iter().partition(|(t, _)| *t <= now);
        self.outbox = later;
        for (_, a) in due {
            if self.role == Role::Client {
                self.net_out.push(NetOut::Action(a));
                continue;
            }
            let evs = if placement_ok(&a) { self.host.handle(&a) } else { vec![Event::Rejected { player: a.player, seq: a.seq, why: Reject::Invalid }] };
            for ev in evs {
                self.publish(now, ev, Some(a.player));
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
/// the bottom; open (Tab) it is larger, in the middle.
pub fn slot_rect(i: usize, open: bool, window: Vec2) -> (Vec2, f32) {
    let (size, gap) = if open { (96.0, 10.0) } else { (58.0, 6.0) };
    let n = item_core::BAR_SLOTS as f32;
    let width = n * size + (n - 1.0) * gap;
    let x = (window.x - width) * 0.5 + i as f32 * (size + gap);
    let y = if open { (window.y - size) * 0.5 } else { window.y - size - 18.0 };
    (Vec2::new(x, y), size)
}

/// The slot under a window point.
pub fn slot_at(p: Vec2, open: bool, window: Vec2) -> Option<usize> {
    (0..item_core::BAR_SLOTS).find(|&i| {
        let (at, s) = slot_rect(i, open, window);
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
fn hand_placement(pl: &Player, f: &Frame, planet: &PlanetRes, ship_owner: PlayerId, throw: Option<(f64, &grab_core::GrabConfig)>) -> Placement {
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
    Placement { frame: if in_cabin { FrameRef::Ship(ship_owner) } else { FrameRef::Planet(planet.id.0 as u32) }, pos: pos.to_array(), vel: vel.to_array(), forward: fwd.to_array() }
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
    dev: Option<Res<crate::dev::DevMenu>>,
    active: Res<crate::displays::ActiveDisplay>,
    players: Query<&Player>,
    ships: Query<(Entity, &Position, &Rotation), With<Ship>>,
    remotes: Query<(Entity, &Position, &Rotation, &crate::ship::RemoteShip)>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
) {
    let now = time.elapsed_secs_f64();
    let Ok(pl) = players.single() else { return };
    let Some((own_e, sp, sr)) = ships.iter().next() else { return };
    // The cabin the walker is in: the own ship's, or the proxy of the ship it boarded (its owner's
    // slot names the frame in a session).
    let (ship_e, frame, owner) = match pl.ship.and_then(|e| remotes.get(e).ok()) {
        Some((e, p, r, rs)) => (e, cabin_frame(e, (p, r), &floors), rs.owner),
        None => (own_e, cabin_frame(own_e, (sp, sr), &floors), items.me()),
    };
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
                let _ = items.request(now, Request::Select { slot: i });
            }
        }
    }
    let carrying = grab.held.map_or(0, |h| h.hands);
    let drop = actions.take_tap(Tap::Drop);
    // R throws the held crate first (`grab_step` takes it then).
    let throw = grab.held.is_none() && actions.take_tap(Tap::Throw);
    if drop && grab.held.is_some() {
        grab.release();
    } else if (drop || throw) && !busy {
        let slot = items.client.view.selected;
        if let Some(s) = items.in_hand(carrying).cloned() {
            let mass = items.defs.of(&s).map_or(1.0, |d| d.shape.mass);
            let at = hand_placement(pl, &frame, &planet, owner, throw.then_some((mass, &tuning.grab)));
            let req = if throw { Request::Throw { slot, at } } else { Request::Drop { slot, at } };
            if let Ok(seq) = items.request(now, req) {
                let li = LooseItem { id: None, def: s.item.kind.to_string(), count: s.count, pending: Some(seq) };
                spawn_loose(&mut commands, &items.defs, li, &at, ship_e, items.role == Role::Client);
            }
        }
    }
    // The inventory screen: the cursor is free; a click takes a slot into the hand, a drag moves.
    controls.free_cursor |= ui.open;
    let down = controls.pointer_down || controls.click;
    if ui.open {
        let under = controls.pointer.and_then(|p| slot_at(p, true, window.0));
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

/// A loose item entity at a placement. On a client (`view`) it is no simulated body but a picture
/// of the host's: `items_net` poses it from what the host sends.
fn spawn_loose(commands: &mut Commands, defs: &ItemDefs, li: LooseItem, at: &Placement, ship_e: Entity, view: bool) {
    let Some(size) = body_size(defs, &li.def) else { return };
    let ship = matches!(at.frame, FrameRef::Ship(_)).then_some(ship_e);
    let mut e = commands.spawn((li, item_body_bundle(&size, ship, DVec3::from_array(at.pos), DVec3::from_array(at.vel), DVec3::from_array(at.forward))));
    if view {
        e.insert(crate::items_net::LooseView::new(*at));
    }
}

/// Messages arrive; the world follows what the client hears.
pub fn deliver(mut commands: Commands, time: Res<Time>, mut items: ResMut<Items>, ships: Query<Entity, With<Ship>>, mut loose: Query<(Entity, &mut LooseItem)>) {
    let evs = items.step(time.elapsed_secs_f64());
    let Some(ship_e) = ships.iter().next() else { return };
    let (me, view) = (items.me(), items.role == Role::Client);
    for ev in evs {
        match ev {
            Event::Spawned { item, loose: l, by } => {
                if loose.iter().any(|(_, li)| li.id == Some(item)) {
                    continue; // told twice (the state of a rejoin): the item is there
                }
                let own = by.filter(|(p, _)| *p == me).map(|(_, seq)| seq);
                if let Some(seq) = own
                    && let Some((_, mut li)) = loose.iter_mut().find(|(_, li)| li.pending == Some(seq))
                {
                    li.id = Some(item);
                    li.pending = None;
                    continue;
                }
                let li = LooseItem { id: Some(item), def: l.stack.item.kind.to_string(), count: l.stack.count, pending: None };
                spawn_loose(&mut commands, &items.defs, li, &l.at, ship_e, view);
            }
            Event::Removed { item } => {
                for (e, li) in &loose {
                    if li.id == Some(item) {
                        commands.entity(e).despawn();
                    }
                }
            }
            Event::Rejected { player, seq, .. } if player == me => {
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

pub(crate) fn hide_items(items: Res<Items>, mut q: Query<(&LooseItem, &mut Visibility), With<Crate>>) {
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

#[derive(Component)]
struct BarSlot(usize);
#[derive(Component)]
struct BarText(usize);
#[derive(Component)]
struct BarTitle;

fn spawn_bar(mut commands: Commands) {
    for i in 0..item_core::BAR_SLOTS {
        commands
            .spawn((
                BarSlot(i),
                Node { position_type: PositionType::Absolute, border: UiRect::all(Val::Px(2.0)), padding: UiRect::all(Val::Px(4.0)), ..default() },
                BorderColor::all(Color::srgba(0.8, 0.9, 1.0, 0.4)),
                BackgroundColor(Color::srgba(0.05, 0.07, 0.1, 0.6)),
            ))
            .with_children(|p| {
                p.spawn((BarText(i), Text::new(""), TextFont { font_size: FontSize::Px(12.0), ..default() }, TextColor(Color::srgb(0.9, 0.95, 1.0))));
            });
    }
    commands.spawn((
        BarTitle,
        Text::new("Inventory: click takes a slot into the hand, drag moves (Tab closes)"),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(Color::srgb(0.9, 0.95, 1.0)),
        Node { position_type: PositionType::Absolute, display: Display::None, ..default() },
    ));
}

#[allow(clippy::type_complexity)]
fn draw_bar(
    items: Res<Items>,
    ui: Res<InventoryUi>,
    window: Res<HudWindow>,
    grab: Res<Grab>,
    controls: Res<Controls>,
    mut slots: Query<(&BarSlot, &mut Node, &mut BorderColor, &mut BackgroundColor), Without<BarTitle>>,
    mut texts: Query<(&BarText, &mut Text), Without<BarTitle>>,
    mut title: Query<&mut Node, (With<BarTitle>, Without<BarSlot>)>,
) {
    let view = &items.client.view;
    let carrying = grab.held.map_or(0, |h| h.hands);
    let hover = controls.pointer.and_then(|p| slot_at(p, true, window.0)).filter(|_| ui.open);
    for (s, mut n, mut border, mut bg) in &mut slots {
        let (at, size) = slot_rect(s.0, ui.open, window.0);
        n.left = Val::Px(at.x);
        n.top = Val::Px(at.y);
        n.width = Val::Px(size);
        n.height = Val::Px(size);
        let selected = view.selected == s.0;
        let lowered = selected && view.slot(s.0).is_some() && items.in_hand(carrying).is_none();
        *border = BorderColor::all(match (selected, lowered) {
            (true, false) => Color::srgb(1.0, 0.85, 0.3),
            (true, true) => Color::srgb(0.6, 0.5, 0.3),
            _ => Color::srgba(0.8, 0.9, 1.0, 0.4),
        });
        let lit = hover == Some(s.0) || ui.drag_from == Some(s.0);
        *bg = BackgroundColor(if lit { Color::srgba(0.2, 0.3, 0.45, 0.85) } else { Color::srgba(0.05, 0.07, 0.1, 0.6) });
    }
    for (t, mut text) in &mut texts {
        let s = match view.slot(t.0) {
            Some(st) if st.count > 1 => format!("{}\n{}\nx{}", t.0 + 1, items.name(st), st.count),
            Some(st) => format!("{}\n{}", t.0 + 1, items.name(st)),
            None => format!("{}", t.0 + 1),
        };
        if text.0 != s {
            text.0 = s;
        }
    }
    if let Ok(mut n) = title.single_mut() {
        let (at, _) = slot_rect(0, true, window.0);
        n.display = if ui.open { Display::Flex } else { Display::None };
        n.left = Val::Px(at.x);
        n.top = Val::Px(at.y - 30.0);
    }
}
