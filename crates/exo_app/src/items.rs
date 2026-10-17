use crate::dev_area::hazards::{spicy_light, SPICY_ITEM};
use std::collections::{BTreeMap, BTreeSet};
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
use item_core::wire::{BotStation, FireNews, PublicStatus, ReactorAsk, ReactorNews, RepairAsk, RoomAsk, RoomNews, SprayAsk, Vitals};
use item_core::{Action, Client, Event, FrameRef, Host, ItemDefs, LooseId, Placement, PlayerId, Reject, Request};
use walker_core::Frame;

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

pub fn window_plugin(app: &mut App) {
    use crate::phases::Frame;
    app.add_systems(Update, (dress_items, hide_items).chain().in_set(Frame::Sync));
    app.add_systems(Update, held_item.in_set(Frame::World));
    app.add_systems(Update, (window_size, draw_bar).chain().after(crate::crew_hud::build_hud).in_set(Frame::Hud));
}

pub fn load_defs() -> ItemDefs {
    let files = crate::content_files("items");
    ItemDefs::from_files(files.iter().map(|f| (f.path.as_str(), f.text.as_str()))).unwrap_or_else(|e| panic!("items: {e}"))
}

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

#[derive(Clone, Debug, PartialEq)]
pub enum NetOut {
    Action(Action),
    /// Host to the clients: one player (an answer to their action) or all (`to` None).
    Event { to: Option<PlayerId>, ev: Event },
    Vitals { to: PlayerId, v: Vitals },
    Room { to: Option<PlayerId>, r: RoomNews },
    Ask(RoomAsk),
    Reactor { to: Option<PlayerId>, r: ReactorNews },
    ReactorAsk(ReactorAsk),
    Fire { to: Option<PlayerId>, f: FireNews },
    SprayAsk(SprayAsk),
    RepairAsk(RepairAsk),
    /// Host to one client: its repair session ended, the machine is full.
    RepairDone { to: PlayerId },
    Status { to: Option<PlayerId>, s: Vec<(PlayerId, PublicStatus)> },
    /// Client to host: the treatment hold on this player, or none.
    Treat(Option<PlayerId>),
    /// Client to host: the CPR hold on this player, or none.
    Rescue(Option<PlayerId>),
    /// Client to host: the coolant refill hold is running (true) or has stopped (false).
    Refill(bool),
    /// Client to host: the brake on the reactor; the host knows the sender.
    Pull,
    RepairRefused { to: PlayerId },
    /// Client to host: the station the active display shows, or none (r43, the bots' occupancy).
    Using(Option<BotStation>),
    /// Host to every client: the crew is dead (#254).
    GameOver,
}

#[derive(Resource)]
pub struct Items {
    pub defs: ItemDefs,
    pub host: Host,
    pub client: Client,
    /// One-way delay of every message (s): 0 in one process, the internet's in tests.
    pub delay: f64,
    pub role: Role,
    outbox: Vec<(f64, Action)>,
    inbox: Vec<(f64, Event)>,
    pub net_out: Vec<NetOut>,
    pub received: Vec<Event>,
    /// system applies the effect.
    pub used: Vec<(PlayerId, PlayerId, String)>,
    pub refills: u32,
    pub rod_loads: Vec<RodLoad>,
    pub vitals_in: Option<Vitals>,
    /// The engine room as the host has it: the host sets it, a client takes it from the wire.
    pub room: RoomNews,
    pub room_asks: Vec<RoomAsk>,
    /// the wire. None until the first one.
    pub reactor: Option<ReactorNews>,
    pub reactor_asks: Vec<ReactorAsk>,
    /// wire. None until the first one.
    pub fire: Option<FireNews>,
    pub spray_asks: Vec<(PlayerId, SprayAsk)>,
    pub repair_asks: Vec<(PlayerId, RepairAsk)>,
    pub repair_done: bool,
    /// The host said the crew is dead; `gameover` takes it (#254).
    pub game_over_seen: bool,
    /// client takes it from the wire. Sorted by player.
    pub public: Vec<(PlayerId, PublicStatus)>,
    /// Treatment holds by player, to apply here (host and one process); a client sends its own.
    pub treat_asks: Vec<(PlayerId, Option<PlayerId>)>,
    /// Brakes by player, to apply here; a client sends its own. The rod goes to that player.
    pub pull_asks: Vec<PlayerId>,
    pub rescue_asks: Vec<(PlayerId, Option<PlayerId>)>,
    /// Host: the coolant refill holds (the player, running or not), for the public status.
    pub refill_asks: Vec<(PlayerId, bool)>,
    /// The players whose refill hold runs (coolant or the O2 shelf), here, whatever the role.
    pub refilling: BTreeSet<PlayerId>,
    /// The station each player's active display shows (r43): a station with a player on it is not a
    /// bot's. Host and local keep it; a client does not need it.
    pub using: BTreeMap<PlayerId, BotStation>,
    /// Host: the masks put on or taken off (the player, the slot the mask is in), from the uses.
    pub mask_uses: Vec<(PlayerId, usize)>,
}

/// Whether the host can simulate an action's placement: a cabin is the host's ship. (Other
/// players' ships are proxies here, with no body to rest an item on.)
fn placement_ok(a: &Action) -> bool {
    match a.req.drop_at() {
        Some(at) => !matches!(at.frame, FrameRef::Ship(n) if n != LOCAL),
        None => true,
    }
}

impl Items {
    pub fn load() -> Items {
        let defs = load_defs();
        Items { host: Host::new(defs.clone()), client: Client::new(LOCAL, defs.clone()), defs, delay: 0.0, role: Role::Local, outbox: Vec::new(), inbox: Vec::new(), net_out: Vec::new(), received: Vec::new(), used: Vec::new(), refills: 0, rod_loads: Vec::new(), vitals_in: None, room: RoomNews::default(), room_asks: Vec::new(), reactor: None, reactor_asks: Vec::new(), fire: None, spray_asks: Vec::new(), repair_asks: Vec::new(), repair_done: false, public: Vec::new(), treat_asks: Vec::new(), pull_asks: Vec::new(), rescue_asks: Vec::new(), refill_asks: Vec::new(), refilling: BTreeSet::new(), using: BTreeMap::new(), mask_uses: Vec::new(), game_over_seen: false }
    }

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

    pub fn remote(&mut self, now: f64, a: Action) {
        self.outbox.push((now, a));
    }

    /// the connection's, not what the message says: anything else is dropped.
    pub fn from_peer(&mut self, now: f64, slot: PlayerId, a: Action) -> bool {
        if self.role != Role::Host || a.player != slot {
            return false;
        }
        self.outbox.push((now, a));
        true
    }

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

    pub fn ask_room(&mut self, a: RoomAsk) {
        self.room_asks.push(a);
    }

    pub fn ask_reactor(&mut self, a: ReactorAsk) {
        self.reactor_asks.push(a);
    }

    pub fn ask_spray(&mut self, a: SprayAsk) {
        if self.role == Role::Client {
            self.net_out.push(NetOut::SprayAsk(a));
        } else {
            self.spray_asks.push((self.me(), a));
        }
    }

    pub fn ask_repair(&mut self, a: RepairAsk) {
        if self.role == Role::Client {
            self.net_out.push(NetOut::RepairAsk(a));
        } else {
            self.repair_asks.push((self.me(), a));
        }
    }

    pub fn ask_pull(&mut self) {
        if self.role == Role::Client {
            self.net_out.push(NetOut::Pull);
        } else {
            self.pull_asks.push(self.me());
        }
    }

    /// The coolant refill hold, running or stopped: only a networked role tells the others.
    pub fn ask_refill(&mut self, running: bool) {
        match self.role {
            Role::Client => self.net_out.push(NetOut::Refill(running)),
            Role::Host => {
                self.refill_asks.push((self.me(), running));
                self.note_refill(self.me(), running);
            }
            Role::Local => self.note_refill(self.me(), running),
        }
    }

    /// Host: a player's refill hold runs or stopped (`refilling`, read by the mask's shelf).
    pub fn note_refill(&mut self, who: PlayerId, running: bool) {
        if running {
            self.refilling.insert(who);
        } else {
            self.refilling.remove(&who);
        }
    }

    /// The station this player's active display shows (r43), or none. Every role keeps its own entry; a
    /// client also tells the host.
    pub fn ask_using(&mut self, s: Option<BotStation>) {
        match self.role {
            Role::Client => self.net_out.push(NetOut::Using(s)),
            Role::Host | Role::Local => self.note_using(self.me(), s),
        }
    }

    /// Host: a player's station (`None` for no station). Read by the bots' occupancy.
    pub fn note_using(&mut self, who: PlayerId, s: Option<BotStation>) {
        match s {
            Some(s) => {
                self.using.insert(who, s);
            }
            None => {
                self.using.remove(&who);
            }
        }
    }

    pub fn ask_treat(&mut self, target: Option<PlayerId>) {
        if self.role == Role::Client {
            self.net_out.push(NetOut::Treat(target));
        } else {
            self.treat_asks.push((self.me(), target));
        }
    }

    pub fn ask_rescue(&mut self, patient: Option<PlayerId>) {
        if self.role == Role::Client {
            self.net_out.push(NetOut::Rescue(patient));
        } else {
            self.rescue_asks.push((self.me(), patient));
        }
    }

    /// The host sends a player its vitals, over the reliable channel.
    pub fn send_vitals(&mut self, to: PlayerId, v: Vitals) {
        self.net_out.push(NetOut::Vitals { to, v });
    }

    pub fn send_game_over(&mut self) {
        self.net_out.push(NetOut::GameOver);
    }

    pub fn take_refills(&mut self) -> u32 {
        std::mem::take(&mut self.refills)
    }

    pub fn take_rod_loads(&mut self) -> Vec<RodLoad> {
        std::mem::take(&mut self.rod_loads)
    }

    /// Host: a stack goes into a player's inventory (a fuel rod the brake pulled, or one the
    /// reactor had no room for). Gives it back when the inventory has no room.
    pub fn give(&mut self, now: f64, player: PlayerId, stack: Stack) -> Result<(), Stack> {
        if self.role == Role::Client {
            return Ok(());
        }
        let ev = self.host.give(player, stack)?;
        self.publish(now, ev, None);
        Ok(())
    }

    pub fn take_uses(&mut self) -> Vec<(PlayerId, PlayerId, String)> {
        std::mem::take(&mut self.used)
    }

    /// Host: a rack on the wall gets its stock. A client does not: the host's events bring it.
    pub fn add_rack(&mut self, now: f64, id: item_core::RackId, tag: &str, slots: usize, stock: Vec<Stack>) {
        if self.role == Role::Client {
            return;
        }
        let ev = self.host.add_rack(id, tag, slots, stock);
        self.publish(now, ev, None);
    }

    /// Host: a cabinet (a container of the storage) stands with its stock. A client does not: the
    /// host's events bring it.
    pub fn add_container(&mut self, now: f64, id: item_core::RackId, contents: item_core::container::Container) {
        if self.role == Role::Client {
            return;
        }
        let ev = self.host.add_container(id, contents);
        self.publish(now, ev, None);
    }

    /// Host: a player left; the containers it had open are free again.
    pub fn release_locks(&mut self, now: f64, player: PlayerId) {
        if self.role == Role::Client {
            return;
        }
        for ev in self.host.release_player(player) {
            self.publish(now, ev, None);
        }
    }

    /// Host: the item in a player's slot wears by `amount` percent (the spray); the change
    /// goes out when a whole percent changes or the item runs out.
    pub fn wear(&mut self, now: f64, player: PlayerId, slot: usize, amount: f64) {
        if self.role == Role::Client {
            return;
        }
        if let Some(ev) = self.host.wear(player, slot, amount) {
            self.publish(now, ev, None);
        }
    }

    /// Host: the worn suit's canister drains by `amount` percent (the body breathes from it).
    pub fn drain_worn(&mut self, now: f64, player: PlayerId, amount: f64) {
        if self.role == Role::Client {
            return;
        }
        if let Some(ev) = self.host.drain_worn(player, amount) {
            self.publish(now, ev, None);
        }
    }

    /// Host: the air canister in a container place fills by `amount` percent (a canister box).
    pub fn fill_in_rack(&mut self, now: f64, rack: item_core::RackId, slot: usize, amount: f64) {
        if self.role == Role::Client {
            return;
        }
        if let Some(ev) = self.host.fill_in_rack(rack, slot, amount) {
            self.publish(now, ev, None);
        }
    }

    pub fn rest(&mut self, now: f64, item: LooseId, at: Placement, rot: [f64; 4], t: f64) {
        if let Some(ev) = self.host.rest(item, at, rot, t) {
            self.publish(now, ev, None);
        }
    }

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
            // What the slot held before the use: a fuel rod's kind and life go with it.
            let held = match &a.req {
                Request::Use { slot, .. } => self.host.inventory(a.player).slot(*slot).map(|s| (s.item.kind.as_str().to_string(), s.item.condition.unwrap_or(0.0))),
                _ => None,
            };
            let used_slot = match &a.req {
                Request::Use { slot, .. } => Some(*slot),
                _ => None,
            };
            let evs = if placement_ok(&a) { self.host.handle(&a) } else { vec![Event::Rejected { player: a.player, seq: a.seq, why: Reject::Invalid }] };
            for ev in evs {
                if let Event::Used { player, target, effect } = &ev {
                    self.used.push((*player, *target, effect.clone()));
                    if effect == crate::dev_area::engineering::coolant::REFILL {
                        self.refills += 1;
                    }
                    if crate::dev_area::gear::is_gear_use(effect)
                        && let Some(slot) = used_slot
                    {
                        self.mask_uses.push((*player, slot));
                    }
                    if effect == crate::dev_area::engineering::rods::LOAD
                        && let Some((item, life)) = held.clone()
                    {
                        self.rod_loads.push(RodLoad { player: *player, item, life });
                    }
                }
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

#[derive(Clone, Debug, PartialEq)]
pub struct RodLoad {
    pub player: PlayerId,
    /// The rod's item id (`fuel_rod`, `fuel_rod_hot`, `fuel_rod_spicy`).
    pub item: String,
    /// Its life when it was loaded (0..100).
    pub life: f64,
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

#[derive(Resource, Default, Debug, Clone)]
pub struct InventoryUi {
    pub open: bool,
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
    (window, look, mut container): (Res<HudWindow>, Res<crate::displays::DisplayLook>, ResMut<crate::container_window::ContainerWindow>),
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
    // One Esc closes the Tab inventory and, with it, the container window (it closes on `!ui.open`),
    // also mid-drag: a drag only moves on release, so the stack stays where it was.
    if ui.open && actions.take_tap(Tap::DisplayLeave) {
        ui.open = false;
        ui.drag_from = None;
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
    let mut drop = actions.take_tap(Tap::Drop);
    // Drags that ended outside every panel, and G on the place under the pointer in the open window:
    // that place's stack drops into the world in front of the walker.
    let mut asks = std::mem::take(&mut container.drops);
    if drop
        && ui.open
        && let Some(p) = container.hover
        && crate::container_window::occupied(&items, container.rack, p)
    {
        asks.push((p, container.rack));
        drop = false;
    }
    if !busy {
        let at = hand_placement(pl, &frame, &planet, owner, None);
        for (place, rack) in asks {
            if let Some((req, def, count)) = crate::container_window::drop_of(&items, rack, place, at)
                && let Ok(seq) = items.request(now, req)
            {
                ui.touched = true;
                let li = LooseItem { id: None, def, count, pending: Some(seq) };
                spawn_loose(&mut commands, &items.defs, li, &at, ship_e, items.role == Role::Client);
            }
        }
    }
    // R throws the held crate first (`grab_step` takes it then).
    let throw = grab.held.is_none() && actions.take_tap(Tap::Throw);
    ui.touched |= drop || throw;
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
    controls.free_cursor |= ui.open;
    let down = controls.pointer_down || controls.click;
    // The container window (container_window) takes the pointer while it is open.
    if ui.open && container.rack.is_none() {
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
                // Let go outside the panel: the stack drops into the world.
                None => {
                    if controls.pointer.is_some_and(|p| crate::container_window::outside_bar_panel(p, window.0, &look.0.hud.bar)) {
                        container.drops.push((crate::container_window::Place::Bar(from), None));
                    }
                    Ok(0)
                }
            };
        }
        controls.click = false;
    } else if !ui.open {
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
        if li.def == SPICY_ITEM {
            commands.entity(e).insert(spicy_light());
        }
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
        if want.as_deref() == Some(SPICY_ITEM) {
            commands.entity(e).insert(spicy_light());
        } else {
            commands.entity(e).remove::<PointLight>();
        }
        if let Some(d) = want.as_deref().and_then(|id| items.defs.get(id)) {
            let k = held_scale(d.hands) as f32;
            let [x, y, z] = d.shape.extents.map(|v| v as f32 * k);
            let [r, g, b] = d.shape.colour;
            commands.entity(e).insert((Mesh3d(meshes.add(Cuboid::new(x, y, z))), MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.6, ..default() }))));
        }
    }
    *vis = if m.0.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    let hands = m.0.as_deref().and_then(|id| items.defs.get(id)).map_or(1, |d| d.hands);
    pose.pos = c.pos + c.rot * held_offset(hands);
    pose.rot = c.rot;
}

/// How big a held item is drawn, against its own size: one hand at full size; two hands at 0.6,
/// The item condition's tone on its slot bar (percent): warn below half, bad below a fifth.
fn condition_tone(c: f64) -> display_core::Tone {
    if c < 20.0 {
        display_core::Tone::Bad
    } else if c < 50.0 {
        display_core::Tone::Warn
    } else {
        display_core::Tone::Normal
    }
}

fn held_scale(hands: u8) -> f64 {
    if hands >= 2 { 0.6 } else { 1.0 }
}

/// Where a held item sits in the view, camera space (x right, y up, -z forward): one hand low
fn held_offset(hands: u8) -> DVec3 {
    if hands >= 2 { DVec3::new(0.16, -0.40, -0.99) } else { DVec3::new(0.34, -0.34, -0.7) }
}

fn window_size(windows: Query<&Window, With<bevy::window::PrimaryWindow>>, mut size: ResMut<HudWindow>) {
    if let Ok(w) = windows.single() {
        let s = Vec2::new(w.width(), w.height());
        if size.0 != s {
            size.0 = s;
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum BarPart {
    Slot(usize),
    Key(usize),
    CodeBox(usize),
    Code(usize),
    Count(usize),
    Hands(usize),
    Condition(usize),
    Float(u32),
    Ghost,
    GhostCode,
    GhostCount,
    Row,
    Name,
    Keys,
    HintBox(usize),
    HintCap(usize),
    HintKey(usize),
    HintText(usize),
}

/// The dashed frame ids of the dragged slot and the slot a drop would land on (the slots 0 to 7
/// use their own index, the prompt 100).
const ORIGIN_DASHES: u32 = 101;
const DROP_DASHES: u32 = 102;
const TAB_HINTS: usize = 5;

pub(crate) fn spawn_bar(commands: &mut Commands, theme: &display_core::Theme, fonts: &crate::crew_hud::HudFonts) {
    use crate::crew_hud::{abs, col, col_a, hud_caps, hud_text, spawn_dashes, HudPart};
    let (k, h, b) = (&theme.colours, &theme.hud, &theme.hud.bar);
    let soft = |a: f64| col_a(h.soft, a);
    let (cl, op) = (&b.closed, &b.open);
    for i in 0..item_core::BAR_SLOTS {
        commands
            .spawn((
                HudPart,
                BarPart::Slot(i),
                Node { border: UiRect::all(Val::Px(cl.border as f32)), ..abs() },
                BorderColor::all(soft(b.filled_border)),
                BackgroundColor(col_a(k.bg_0, b.filled_bg)),
                BoxShadow::default(),
            ))
            .with_children(|p| {
                p.spawn((BarPart::Key(i), Node { left: Val::Px(cl.num_x as f32), top: Val::Px(cl.num_y as f32), ..abs() }, hud_text(&fonts.mono, &format!("{}", i + 1), cl.num_size, soft(cl.num_alpha), h)));
                p.spawn((BarPart::CodeBox(i), Node { left: Val::Px(0.0), top: Val::Px(cl.code_top as f32), width: percent(100.0), justify_content: JustifyContent::Center, ..abs() })).with_children(|c| {
                    c.spawn((BarPart::Code(i), hud_caps(&fonts.bold, "", cl.code_size, col(k.fg_title), h, 0.04)));
                });
                p.spawn((BarPart::Hands(i), Node { left: Val::Px(cl.tag_pad as f32), bottom: Val::Px(cl.tag_pad as f32 * 0.5), ..abs() }, hud_text(&fonts.mono, "", cl.tag_size, soft(0.85), h)));
                p.spawn((BarPart::Count(i), Node { right: Val::Px(cl.tag_pad as f32), bottom: Val::Px(cl.tag_pad as f32 * 0.5), ..abs() }, hud_text(&fonts.mono, "", cl.tag_size, col(k.fg_1), h)));
                p.spawn((BarPart::Condition(i), Node { left: Val::Px(cl.tag_pad as f32), bottom: Val::Px(cl.tag_pad as f32 * 0.25), height: Val::Px(3.0), width: percent(0.0), display: Display::None, ..abs() }, BackgroundColor(col(k.accent))));
                spawn_dashes(p, i as u32, (cl.size as f32, cl.size as f32), cl.border as f32, cl.border as f32, h, soft(b.carry_border));
            });
    }
    for (id, edge, thick, colour) in [(ORIGIN_DASHES, op.size, op.border, soft(b.origin_alpha)), (DROP_DASHES, op.size + 2.0 * b.drop_offset, op.border, col(k.accent))] {
        commands.spawn((HudPart, BarPart::Float(id), Node { display: Display::Flex, ..abs() })).with_children(|p| {
            spawn_dashes(p, id, (edge as f32, edge as f32), 0.0, thick as f32, h, colour);
        });
    }
    commands.spawn((HudPart, BarPart::Row, Node { height: Val::Px(b.name_h as f32), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..abs() })).with_children(|row| {
        row.spawn((BarPart::Name, hud_caps(&fonts.bold, "", b.name_size, col(k.fg_title), h, 0.1)));
        row.spawn((BarPart::Keys, Node { align_items: AlignItems::Center, column_gap: Val::Px(18.0), display: Display::None, ..default() })).with_children(|keys| {
            let hi = &h.hints;
            for i in 0..TAB_HINTS {
                keys.spawn((BarPart::HintBox(i), Node { align_items: AlignItems::Center, column_gap: Val::Px(8.0), display: Display::None, ..default() })).with_children(|hint| {
                    hint.spawn((
                        BarPart::HintCap(i),
                        Node { min_width: Val::Px(hi.key_min_w as f32), height: Val::Px(hi.key_h as f32), padding: UiRect::horizontal(Val::Px(7.0)), border: UiRect::all(Val::Px(hi.key_border as f32)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                        BackgroundColor(col_a(k.bg_0, hi.key_bg_alpha)),
                        BorderColor::all(soft(hi.key_alpha)),
                    ))
                    .with_children(|cap| {
                        cap.spawn((BarPart::HintKey(i), hud_text(&fonts.mono, "", hi.key_size, col(k.fg_title), h)));
                    });
                    hint.spawn((BarPart::HintText(i), hud_caps(&fonts.display, "", b.row_size, col(k.fg_1), h, hi.spacing)));
                });
            }
        });
    });
    // The drag's ghost, last so it draws over everything.
    commands
        .spawn((
            HudPart,
            BarPart::Ghost,
            Node { width: Val::Px(op.size as f32), height: Val::Px(op.size as f32), border: UiRect::all(Val::Px(op.border_selected as f32)), display: Display::None, ..abs() },
            BackgroundColor(col_a(b.ghost_bg, b.ghost_bg_alpha)),
            BorderColor::all(col(k.fg_title)),
            BoxShadow::new(Color::srgba(0.0, 0.0, 0.0, 0.6), Val::Px(0.0), Val::Px(8.0), Val::Px(0.0), Val::Px(16.0)),
            UiTransform { rotation: Rot2::degrees(-b.ghost_tilt_deg as f32), ..UiTransform::IDENTITY },
            GlobalZIndex(5),
        ))
        .with_children(|g| {
            g.spawn(Node { left: Val::Px(0.0), top: Val::Px(op.code_top as f32 - op.border_selected as f32), width: percent(100.0), justify_content: JustifyContent::Center, ..abs() }).with_children(|c| {
                c.spawn((BarPart::GhostCode, hud_caps(&fonts.bold, "", op.code_size, col(k.fg_title), h, 0.04)));
            });
            g.spawn((BarPart::GhostCount, Node { right: Val::Px(op.tag_pad as f32), bottom: Val::Px(op.tag_pad as f32 * 0.5), ..abs() }, hud_text(&fonts.mono, "", op.tag_size, col(k.fg_1), h)));
        });
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_bar(
    items: Res<Items>,
    ui: Res<InventoryUi>,
    window: Res<HudWindow>,
    grab: Res<Grab>,
    controls: Res<Controls>,
    (look, hud, bindings, fonts): (Res<crate::displays::DisplayLook>, Res<crate::crew_hud::CrewHud>, Res<crate::controls::Bindings>, Option<Res<crate::crew_hud::HudFonts>>),
    container: Res<crate::container_window::ContainerWindow>,
    mut nodes: Query<(&BarPart, &mut Node)>,
    mut borders: Query<(&BarPart, &mut BorderColor)>,
    mut fills: Query<(&BarPart, &mut BackgroundColor)>,
    mut shadows: Query<(&BarPart, &mut BoxShadow)>,
    mut texts: Query<(&BarPart, &mut Text, &mut TextColor, &mut TextFont)>,
    mut frames: Query<(&DashFrame, &mut Node), Without<BarPart>>,
    mut dashes: Query<(&Dash, &mut BackgroundColor), Without<BarPart>>,
    mut moved: Local<bool>,
) {
    use crate::crew_hud::{col, col_a, dark, edit, set_bg, set_border, set_dashes, set_display, set_font, set_text, set_text_colour};
    use display_core::hud::{tab_hints, TabKey, TabMode};
    let Some(fonts) = fonts else { return };
    let (theme, b) = (&look.0, &look.0.hud.bar);
    let k = &theme.colours;
    let soft = |a: f64| col_a(theme.hud.soft, a);
    let ink = |a: f64| col_a(k.bg_0, a);
    let view = &items.client.view;
    let carrying = grab.held.map_or(0, |h| h.hands);
    let (win, open) = (window.0, ui.open);
    let sl = if open { &b.open } else { &b.closed };
    let hover = controls.pointer.and_then(|p| slot_at(p, true, win, b)).filter(|_| open);
    // A drag shows once the pointer has left the slot it began on; an empty slot has nothing to drag.
    if open && ui.drag_from.is_some() {
        *moved |= hover != ui.drag_from;
    } else {
        *moved = false;
    }
    let drag = ui.drag_from.filter(|f| open && *moved && controls.pointer.is_some() && view.slot(*f).is_some());
    // With a container open the window draws the places under a drag (accept or refuse).
    let drop_on = drag.and(hover).filter(|h| Some(*h) != drag).filter(|_| container.rack.is_none());
    let contrast_of = |sel: bool| -> f64 {
        if open {
            1.0
        } else if carrying > 0 {
            b.carry_contrast
        } else if sel {
            1.0
        } else {
            hud.contrast
        }
    };
    let dashed_closed = carrying > 0 && !open;
    let selected = view.selected;
    let accent = |a: f64| col_a(k.accent, a);
    let stack_of = |i: usize| view.slot(i);
    let code_of = |i: usize| stack_of(i).and_then(|st| items.defs.of(st)).map(|d| d.short_code()).unwrap_or_default();
    let count_of = |i: usize| stack_of(i).filter(|st| st.count > 1).map(|st| format!("×{}", st.count)).unwrap_or_default();
    let mode = if drag.is_some() || container.dragging() {
        TabMode::Drag
    } else if container.rack.is_some() {
        TabMode::Container
    } else if carrying > 0 {
        TabMode::Carrying
    } else {
        TabMode::Browse
    };
    let hints = tab_hints(mode);
    let (first, size) = slot_rect(0, open, win, b);
    let (last, _) = slot_rect(item_core::BAR_SLOTS - 1, open, win, b);
    let row_top = display_core::hud::bar_row_top(open, win.as_dvec2(), &theme.hud.bar) as f32;

    for (part, mut n) in &mut nodes {
        match *part {
            BarPart::Slot(i) => {
                let (at, edge) = slot_rect(i, open, win, b);
                let lift = if i == selected && !open { b.lift as f32 } else { 0.0 };
                let border = if i == selected && open { sl.border_selected } else { sl.border } as f32;
                edit(&mut n, |n| {
                    n.left = Val::Px(at.x);
                    n.top = Val::Px(at.y - lift);
                    n.width = Val::Px(edge);
                    n.height = Val::Px(edge);
                    n.border = UiRect::all(Val::Px(border));
                });
            }
            BarPart::Key(i) => edit(&mut n, |n| {
                n.left = Val::Px(sl.num_x as f32);
                n.top = Val::Px(sl.num_y as f32 - if i == selected && !open { 0.0 } else { 0.0 });
            }),
            BarPart::CodeBox(_) => edit(&mut n, |n| n.top = Val::Px(sl.code_top as f32 - sl.border as f32)),
            BarPart::Count(_) => edit(&mut n, |n| {
                n.right = Val::Px(sl.tag_pad as f32);
                n.bottom = Val::Px(sl.tag_pad as f32 * 0.5);
            }),
            BarPart::Hands(_) => edit(&mut n, |n| {
                n.left = Val::Px(sl.tag_pad as f32);
                n.bottom = Val::Px(sl.tag_pad as f32 * 0.5);
            }),
            BarPart::Condition(i) => {
                let cond = stack_of(i).and_then(|st| st.item.condition);
                edit(&mut n, |n| {
                    n.display = if cond.is_some() { Display::Flex } else { Display::None };
                    n.width = percent(cond.unwrap_or(0.0).clamp(0.0, 100.0) as f32);
                });
            }
            BarPart::Float(id) => {
                let (at, show) = match (id, drag, drop_on) {
                    (ORIGIN_DASHES, Some(f), _) => (slot_rect(f, true, win, b).0, true),
                    (DROP_DASHES, _, Some(t)) => (slot_rect(t, true, win, b).0 - Vec2::splat(b.drop_offset as f32), true),
                    _ => (Vec2::ZERO, false),
                };
                edit(&mut n, |n| {
                    n.display = if show { Display::Flex } else { Display::None };
                    n.left = Val::Px(at.x);
                    n.top = Val::Px(at.y);
                });
            }
            BarPart::Row => {
                let width = last.x + size - first.x;
                edit(&mut n, |n| {
                    n.left = Val::Px(first.x);
                    n.top = Val::Px(row_top);
                    n.width = Val::Px(width);
                    n.height = Val::Px(if open { b.row_h } else { b.name_h } as f32);
                });
            }
            BarPart::Keys => set_display(&mut n, if open { Display::Flex } else { Display::None }),
            BarPart::HintBox(i) => set_display(&mut n, if open && i < hints.len() { Display::Flex } else { Display::None }),
            BarPart::Ghost => {
                let at = controls.pointer.unwrap_or_default() - Vec2::splat(b.open.size as f32 * 0.5);
                edit(&mut n, |n| {
                    n.display = if drag.is_some() { Display::Flex } else { Display::None };
                    n.left = Val::Px(at.x);
                    n.top = Val::Px(at.y);
                });
            }
            _ => {}
        }
    }

    for (part, mut border) in &mut borders {
        if let BarPart::Slot(i) = *part {
            let c = contrast_of(i == selected);
            let want = if drag == Some(i) || dashed_closed {
                Color::NONE
            } else if open && hover == Some(i) {
                accent(1.0)
            } else if i == selected {
                col(k.fg_title)
            } else if stack_of(i).is_some() {
                soft(b.filled_border * c)
            } else {
                soft(b.empty_border * c)
            };
            set_border(&mut border, want);
        }
    }
    for (part, mut bg) in &mut fills {
        if let BarPart::Condition(i) = *part {
            let tone = stack_of(i).and_then(|st| st.item.condition).map_or(display_core::Tone::Normal, condition_tone);
            set_bg(&mut bg, match tone {
                display_core::Tone::Warn => col(k.warn),
                display_core::Tone::Bad => col(k.bad),
                _ => col(k.accent),
            });
        } else if let BarPart::Slot(i) = *part {
            let c = contrast_of(i == selected);
            let want = if drag == Some(i) {
                ink(b.empty_bg)
            } else if dashed_closed {
                ink(b.carry_bg * c)
            } else if open && hover == Some(i) {
                accent(b.hover_bg)
            } else if i == selected {
                col_a(k.fg_title, b.selected_bg)
            } else if stack_of(i).is_some() {
                ink(b.filled_bg * c)
            } else {
                ink(b.empty_bg * c)
            };
            set_bg(&mut bg, want);
        }
    }
    for (part, mut shadow) in &mut shadows {
        if let BarPart::Slot(i) = *part {
            let want = if i == selected && !open && !dashed_closed {
                vec![ShadowStyle { color: dark(0.55), x_offset: Val::Px(0.0), y_offset: Val::Px(4.0), spread_radius: Val::Px(0.0), blur_radius: Val::Px(10.0) }]
            } else {
                Vec::new()
            };
            if shadow.0 != want {
                shadow.0 = want;
            }
        }
    }
    set_dashes(ORIGIN_DASHES, drag.is_some(), soft(b.origin_alpha), &mut frames, &mut dashes);
    set_dashes(DROP_DASHES, drop_on.is_some(), accent(1.0), &mut frames, &mut dashes);
    for i in 0..item_core::BAR_SLOTS {
        set_dashes(i as u32, dashed_closed, soft(b.carry_border * contrast_of(i == selected)), &mut frames, &mut dashes);
    }

    let held_name = &hud.held_name;
    let box_stack = |p: Option<crate::container_window::Place>| match (p, container.rack) {
        (Some(crate::container_window::Place::Box(i)), Some(r)) => items.client.racks.get(&r).and_then(|c| c.slot(i)),
        _ => None,
    };
    let (shown, shown_accent) = if let Some(f) = drag {
        (stack_of(f).map(|st| items.name(st)).unwrap_or_default(), false)
    } else if let Some(st) = box_stack(container.drag.filter(|_| container.dragging())) {
        (items.name(st), false)
    } else if let Some(st) = box_stack(container.hover) {
        (items.name(st), true)
    } else if open && let Some(st) = hover.and_then(stack_of) {
        (items.name(st), hover != Some(selected))
    } else {
        (held_name.clone(), false)
    };
    let name_a = if open || b.idle_contrast >= 1.0 { 1.0 } else { 1.0 - (1.0 - hud.contrast) / (1.0 - b.idle_contrast) * (1.0 - b.name_idle_contrast) };
    for (part, mut t, mut tc, mut f) in &mut texts {
        match *part {
            BarPart::Key(i) => {
                let c = contrast_of(i == selected);
                set_font(&mut f, &fonts.mono, sl.num_size);
                set_text_colour(&mut tc, if i == selected { col_a(k.fg_title, c) } else { soft(sl.num_alpha * c) });
            }
            BarPart::Code(i) => {
                let hidden = drag == Some(i);
                set_text(&mut t, &code_of(i));
                set_font(&mut f, &fonts.bold, sl.code_size);
                set_text_colour(&mut tc, col_a(k.fg_title, if hidden { 0.0 } else { contrast_of(i == selected) }));
            }
            BarPart::Count(i) => {
                set_text(&mut t, &count_of(i));
                set_font(&mut f, &fonts.mono, sl.tag_size);
                set_text_colour(&mut tc, col_a(k.fg_1, if drag == Some(i) { 0.0 } else { contrast_of(i == selected) }));
            }
            BarPart::Hands(i) => {
                let two = stack_of(i).and_then(|st| items.defs.of(st)).is_some_and(|d| d.hands == 2);
                set_text(&mut t, if two { "2H" } else { "" });
                set_font(&mut f, &fonts.mono, sl.tag_size);
                set_text_colour(&mut tc, soft(0.85 * if drag == Some(i) { 0.0 } else { contrast_of(i == selected) }));
            }
            BarPart::Name => {
                set_text(&mut t, &shown.to_uppercase());
                set_text_colour(&mut tc, if shown_accent { accent(1.0) } else { col_a(k.fg_title, name_a) });
            }
            BarPart::HintKey(i) => {
                if let Some((key, _)) = hints.get(i) {
                    set_text(&mut t, &match key {
                        TabKey::Click => "CLICK".to_string(),
                        TabKey::Drag => "DRAG".to_string(),
                        TabKey::Drop => crate::interact::key_cap(&bindings, crate::controls::Tap::Drop),
                        TabKey::Throw => crate::interact::key_cap(&bindings, crate::controls::Tap::Throw),
                        TabKey::Close => crate::interact::key_cap(&bindings, crate::controls::Tap::Inventory),
                        TabKey::CloseAll => format!("{} / ESC / {}", crate::interact::key_cap(&bindings, crate::controls::Tap::Interact), crate::interact::key_cap(&bindings, crate::controls::Tap::Inventory)),
                        TabKey::Cancel => "ESC".to_string(),
                        TabKey::Release => "RELEASE OUTSIDE".to_string(),
                    });
                }
            }
            BarPart::HintText(i) => {
                if let Some((_, text)) = hints.get(i) {
                    set_text(&mut t, text);
                }
            }
            BarPart::GhostCode => {
                if let Some(f) = drag {
                    set_text(&mut t, &code_of(f));
                }
            }
            BarPart::GhostCount => {
                if let Some(f) = drag {
                    set_text(&mut t, &count_of(f));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brake_is_asked_by_its_sender_not_by_the_message() {
        // A pull names no player; the host takes the sender (the local player's id here, a
        // remote one's slot from its connection, `items_net`), so the rod goes to the one who asked.
        let mut host = Items::load();
        host.role = Role::Host;
        host.ask_pull();
        assert_eq!(host.pull_asks, vec![host.me()]);
        let mut client = Items::load();
        client.role = Role::Client;
        client.ask_pull();
        assert!(client.pull_asks.is_empty(), "a client sends it, it does not apply it");
        assert!(client.net_out.iter().any(|n| matches!(n, NetOut::Pull)));
    }
}
