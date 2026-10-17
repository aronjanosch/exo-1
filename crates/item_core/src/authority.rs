//! The path every item change takes (concept repo `decisions/crew-ship.md`, "Interaction model";
//! `decisions/project.md`, "Played over the internet").
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::container::{Access, Container, Refusal, SlotRule, Stack, Tag};
use crate::defs::ItemDefs;
use crate::inventory::{Inventory, BAR_SLOTS, CANISTER_TAG, SUIT_TAG};

pub type PlayerId = u32;
pub type LooseId = u64;
pub type RackId = u32;

/// The frame a loose item lives in, like the walker: a planet, or a ship's cabin (its owner's
/// slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameRef {
    Planet(u32),
    Ship(u32),
}

/// Where a loose item is: frame, position and velocity in that frame, heading (local -z).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub frame: FrameRef,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub forward: [f64; 3],
}

impl Placement {
    /// Whether the numbers can be a placement: finite, a position within reach of any frame, a
    /// speed a player can throw at, a heading. The host checks every placement a client sends.
    pub fn sane(&self) -> bool {
        let len = |v: &[f64; 3]| v.iter().map(|c| c * c).sum::<f64>().sqrt();
        let finite = |v: &[f64; 3]| v.iter().all(|c| c.is_finite());
        finite(&self.pos) && finite(&self.vel) && finite(&self.forward) && len(&self.pos) <= 1.0e9 && len(&self.vel) <= MAX_THROW_SPEED && len(&self.forward) > 1e-6
    }
}

/// Fastest a placement may move (m/s): well above a throw, far below a ship.
pub const MAX_THROW_SPEED: f64 = 100.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Loose {
    pub stack: Stack,
    pub at: Placement,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    Collect { item: LooseId },
    Drop { slot: usize, at: Placement },
    /// Throw it: the same as a drop, `at` carries the throw's velocity.
    Throw { slot: usize, at: Placement },
    Select { slot: usize },
    Move { from: usize, to: usize },
    /// Use one item of a slot on `target` (a player's slot; the user's own for itself). What the
    /// item does is the host's to apply (`Event::Used`).
    Use { slot: usize, target: PlayerId },
    /// Hang the item of a slot on a rack and take a spare from it (Barotrauma's extinguisher
    /// bracket): the spare with the best condition of the same kind. An empty slot takes one.
    Swap { slot: usize, rack: RackId },
    /// Put the whole stack of an inventory slot into a cabinet (a container of the dev area's
    /// storage): an empty slot or equal items with room. Refused as a whole.
    Store { slot: usize, rack: RackId },
    /// Take the whole stack of a cabinet slot into the inventory (the collect rules apply).
    Fetch { rack: RackId, rack_slot: usize },
    /// Like `Store`, into one place of the container (a drag in the container window): an empty
    /// place or equal items with room for the whole stack.
    StoreAt { slot: usize, rack: RackId, rack_slot: usize },
    /// Like `Fetch`, into one bar slot: an empty slot or equal items with room.
    FetchTo { rack: RackId, rack_slot: usize, slot: usize },
    /// Moves a stack between two places of the same container (a drag inside the window).
    MoveIn { rack: RackId, from: usize, to: usize },
    /// Puts on the suit lying in the world (F on it). Refused while a suit is worn. A suit never
    /// goes into a bar slot, so this is the only way a loose suit is picked up.
    CollectWear { item: LooseId },
    /// Takes the worn suit off into a place of a container (a suit locker).
    StoreWorn { rack: RackId, rack_slot: usize },
    /// Puts on the suit of a container place (a suit locker); a suit already worn goes to that place.
    WearFrom { rack: RackId, rack_slot: usize },
    /// Fits the air canister of a bar slot into the worn suit; the canister it held (if any) takes
    /// the slot (a swap).
    FitCanister { slot: usize },
    /// Takes the canister out of the worn suit into the bar slot `slot`, or the first free one (None).
    PullCanister { slot: Option<usize> },
    /// Takes the canister out of the worn suit into a place of a container (a canister box).
    StoreCanister { rack: RackId, rack_slot: usize },
    /// Fits the canister of a container place into the worn suit; the canister it held takes that place.
    FitFrom { rack: RackId, rack_slot: usize },
    /// Fits the air canister of a bar slot into the suit lying in a container place (a suit locker);
    /// the canister it held (if any) takes the bar slot (a swap).
    FitStoredCanister { slot: usize, rack: RackId, rack_slot: usize },
    /// Takes the canister out of the suit lying in a container place into the bar slot `slot`, or
    /// the first free one (None).
    PullStoredCanister { rack: RackId, rack_slot: usize, slot: Option<usize> },
    /// Drops the worn suit into the world (dragged out of the window, or hovered and G). Always
    /// allowed, also in vacuum.
    DropWorn { at: Placement },
    /// Drops the canister of the worn suit into the world as a canister of its own.
    DropCanister { at: Placement },
    /// Drops the whole stack of a container place into the world.
    DropFrom { rack: RackId, rack_slot: usize, at: Placement },
    /// Drops the canister of the suit lying in a container place into the world.
    DropStoredCanister { rack: RackId, rack_slot: usize, at: Placement },
    /// Opens the container window: the player holds the container until `Close` (one player per
    /// container). Refused while another holds it.
    Open { rack: RackId },
    /// Gives the container back. Whoever does not hold it asks for nothing.
    Close { rack: RackId },
}

impl Request {
    /// Where the request puts an item into the world, if it drops or throws one.
    pub fn drop_at(&self) -> Option<&Placement> {
        match self {
            Request::Drop { at, .. }
            | Request::Throw { at, .. }
            | Request::DropWorn { at }
            | Request::DropCanister { at }
            | Request::DropFrom { at, .. }
            | Request::DropStoredCanister { at, .. } => Some(at),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub player: PlayerId,
    pub seq: u32,
    pub req: Request,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reject {
    Gone,
    Full,
    /// Not something that goes into an inventory (cargo is carried).
    NotCollectable,
    /// A suit is already worn (a loose suit is not put on over it).
    Wearing,
    EmptySlot,
    NoSuchSlot,
    Invalid,
    NotUsable,
    NoRack,
    NoSpare,
    /// The container is locked and the carried items do not open it.
    Locked,
    /// The container's slots do not take this item.
    NotAllowed,
    /// Another player has the container open.
    InUse,
}

impl From<Refusal> for Reject {
    fn from(r: Refusal) -> Reject {
        match r {
            Refusal::Full => Reject::Full,
            Refusal::NotAllowed => Reject::NotCollectable,
            Refusal::EmptySlot => Reject::EmptySlot,
            Refusal::NoSuchSlot => Reject::NoSuchSlot,
            _ => Reject::Invalid,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// A loose item appeared; `by` names the action that dropped it, so that client can keep the
    /// item it already shows instead of adding a second one.
    Spawned { item: LooseId, loose: Loose, by: Option<(PlayerId, u32)> },
    Removed { item: LooseId },
    Inventory { player: PlayerId, inv: Inventory },
    /// An item of `player` was used on `target` (one was taken from the stack); the host applies
    /// `effect` to the target.
    Used { player: PlayerId, target: PlayerId, effect: String },
    /// A loose item came to rest at `at`, turned as the quaternion `rot` (x, y, z, w; a resting
    /// item may lie on its side, which `at.forward` cannot say), at the shared session time `t`
    /// (the host's clock). While an item moves, its poses go out as datagrams, not events; this is
    /// its last word.
    Rest { item: LooseId, at: Placement, rot: [f64; 4], t: f64 },
    Confirmed { player: PlayerId, seq: u32 },
    Rejected { player: PlayerId, seq: u32, why: Reject },
    Rack { rack: RackId, contents: Container },
    /// The container's holder: a player has it open, or nobody (None).
    Lock { rack: RackId, by: Option<PlayerId> },
}

#[derive(Clone, Debug, Default)]
pub struct Host {
    pub defs: ItemDefs,
    pub loose: BTreeMap<LooseId, Loose>,
    pub inventories: BTreeMap<PlayerId, Inventory>,
    pub racks: BTreeMap<RackId, Container>,
    /// Who has a container open: one player each, so two never move items in it at once.
    pub locks: BTreeMap<RackId, PlayerId>,
    next: LooseId,
}

impl Host {
    pub fn new(defs: ItemDefs) -> Host {
        Host { defs, ..Default::default() }
    }

    pub fn spawn(&mut self, stack: Stack, at: Placement, by: Option<(PlayerId, u32)>) -> Event {
        self.next += 1;
        let item = self.next;
        let loose = Loose { stack, at };
        self.loose.insert(item, loose.clone());
        Event::Spawned { item, loose, by }
    }

    /// Puts a container with its stock on the map under `id` (a cabinet of the storage); the
    /// event tells the clients its contents.
    pub fn add_container(&mut self, id: RackId, contents: Container) -> Event {
        self.racks.insert(id, contents.clone());
        Event::Rack { rack: id, contents }
    }

    /// Puts a rack on the wall: `slots` slots taking items of the kind tagged `tag`, and the
    /// spares it starts with. The event tells the clients its stock.
    pub fn add_rack(&mut self, id: RackId, tag: &str, slots: usize, stock: Vec<Stack>) -> Event {
        let mut contents = Container::new(vec![SlotRule::tagged(1, &[tag], &[]); slots], Access::Open);
        for s in stock {
            let _ = contents.try_insert(&[], s);
        }
        self.racks.insert(id, contents.clone());
        Event::Rack { rack: id, contents }
    }

    /// Puts a stack into a player's inventory, as a pulled or refused fuel rod comes back.
    /// The stack comes back when it does not fit.
    pub fn give(&mut self, player: PlayerId, stack: Stack) -> Result<Event, Stack> {
        let mut inv = self.inventory(player);
        match inv.collect(&self.defs, stack.clone()) {
            Ok(()) => {
                self.inventories.insert(player, inv.clone());
                Ok(Event::Inventory { player, inv })
            }
            Err(_) => Err(stack),
        }
    }

    /// The condition (percent) of the item in a slot, if it wears.
    pub fn condition(&self, player: PlayerId, slot: usize) -> Option<f64> {
        self.inventories.get(&player)?.slot(slot)?.item.condition
    }

    /// Wears the item in a slot by `amount` percent (not below 0), as the spray does while it
    /// runs. A negative amount refills it, at most to full (the shelf fills a mask's tank). The event (the inventory as now) comes only when the whole percent changes or the
    /// item runs out, so the clients hear it a few times a second at most.
    pub fn wear(&mut self, player: PlayerId, slot: usize, amount: f64) -> Option<Event> {
        let inv = self.inventories.get_mut(&player)?;
        let item = &mut inv.bar.slot_mut(slot)?.item;
        let before = item.condition?;
        let after = (before - amount).clamp(0.0, 100.0);
        item.condition = Some(after);
        let changed = before.floor() != after.floor() || (before > 0.0 && after == 0.0);
        changed.then(|| Event::Inventory { player, inv: inv.clone() })
    }

    /// The worn suit's canister drains by `amount` percent (not below 0; a negative amount fills,
    /// at most to full). As `wear`: the event comes when the whole percent changes or it runs out.
    pub fn drain_worn(&mut self, player: PlayerId, amount: f64) -> Option<Event> {
        let inv = self.inventories.get_mut(&player)?;
        let item = &mut inv.worn.as_mut()?.item;
        let before = item.condition?;
        let after = (before - amount).clamp(0.0, 100.0);
        item.condition = Some(after);
        let changed = before.floor() != after.floor() || (before > 0.0 && after == 0.0);
        changed.then(|| Event::Inventory { player, inv: inv.clone() })
    }

    /// The air canister in a container place fills by `amount` percent (at most full; a canister box
    /// at work). The event comes when the whole percent changes.
    pub fn fill_in_rack(&mut self, rack: RackId, slot: usize, amount: f64) -> Option<Event> {
        let contents = self.racks.get_mut(&rack)?;
        let item = &mut contents.slot_mut(slot)?.item;
        if !item.has_tag(&Tag::new(CANISTER_TAG)) {
            return None;
        }
        let before = item.condition?;
        let after = (before + amount).clamp(0.0, 100.0);
        item.condition = Some(after);
        (before.floor() != after.floor()).then(|| Event::Rack { rack, contents: contents.clone() })
    }

    /// The host's physics reports where a loose item is now (no event; a client that joins later
    /// gets this placement).
    pub fn track(&mut self, item: LooseId, at: Placement) {
        if let Some(l) = self.loose.get_mut(&item) {
            l.at = at;
        }
    }

    /// A loose item came to rest: remembered, and the event to send (None if it is gone).
    pub fn rest(&mut self, item: LooseId, at: Placement, rot: [f64; 4], t: f64) -> Option<Event> {
        let l = self.loose.get_mut(&item)?;
        l.at = at;
        Some(Event::Rest { item, at, rot, t })
    }

    /// Everything a client that joins now needs to know, as events in the order to apply them:
    /// the loose items where they are, every player's inventory (its own and the others', whose
    /// held items show on their figures).
    pub fn state(&self) -> Vec<Event> {
        let loose = self.loose.iter().map(|(id, l)| Event::Spawned { item: *id, loose: l.clone(), by: None });
        let inventories = self.inventories.iter().map(|(p, inv)| Event::Inventory { player: *p, inv: inv.clone() });
        let racks = self.racks.iter().map(|(id, c)| Event::Rack { rack: *id, contents: c.clone() });
        let locks = self.locks.iter().map(|(id, p)| Event::Lock { rack: *id, by: Some(*p) });
        loose.chain(inventories).chain(racks).chain(locks).collect()
    }

    /// A player left (or its window did): every container it held is free again.
    pub fn release_player(&mut self, player: PlayerId) -> Vec<Event> {
        let held: Vec<RackId> = self.locks.iter().filter(|(_, p)| **p == player).map(|(r, _)| *r).collect();
        held.into_iter()
            .map(|rack| {
                self.locks.remove(&rack);
                Event::Lock { rack, by: None }
            })
            .collect()
    }

    pub fn inventory(&self, p: PlayerId) -> Inventory {
        self.inventories.get(&p).cloned().unwrap_or_default()
    }

    pub fn handle(&mut self, a: &Action) -> Vec<Event> {
        let mut inv = self.inventory(a.player);
        let mut out = Vec::new();
        let busy = rack_of(&a.req).is_some_and(|r| self.locks.get(&r).is_some_and(|p| *p != a.player));
        let result: Result<(), Reject> = match &a.req {
            _ if busy => Err(Reject::InUse),
            Request::Collect { item } => match self.loose.get(item) {
                None => Err(Reject::Gone),
                Some(l) => match inv.collect(&self.defs, l.stack.clone()) {
                    Ok(()) => {
                        self.loose.remove(item);
                        out.push(Event::Removed { item: *item });
                        Ok(())
                    }
                    Err(r) => Err(r.into()),
                },
            },
            req if req.drop_at().is_some_and(|at| !at.sane()) => Err(Reject::Invalid),
            Request::Drop { slot, at } | Request::Throw { slot, at } => match inv.take_slot(*slot) {
                Ok(stack) => {
                    out.push(self.spawn(stack, *at, Some((a.player, a.seq))));
                    Ok(())
                }
                Err(r) => Err(r.into()),
            },
            Request::DropWorn { at } => drop_worn(&mut inv).map(|stack| out.push(self.spawn(stack, *at, Some((a.player, a.seq))))),
            Request::DropCanister { at } => drop_canister(&self.defs, &mut inv).map(|stack| out.push(self.spawn(stack, *at, Some((a.player, a.seq))))),
            Request::DropFrom { rack, rack_slot, at } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => drop_from(&inv, &mut contents, *rack_slot).map(|stack| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                    out.push(self.spawn(stack, *at, Some((a.player, a.seq))));
                }),
            },
            Request::DropStoredCanister { rack, rack_slot, at } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => drop_stored_canister(&self.defs, &inv, &mut contents, *rack_slot).map(|stack| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                    out.push(self.spawn(stack, *at, Some((a.player, a.seq))));
                }),
            },
            Request::Select { slot } => inv.select(*slot).map_err(Reject::from),
            Request::Move { from, to } => inv.move_slot(*from, *to).map_err(Reject::from),
            Request::Use { slot, target } => use_one(&self.defs, &mut inv, *slot).map(|effect| {
                if let Some(effect) = effect {
                    out.push(Event::Used { player: a.player, target: *target, effect });
                }
            }),
            Request::CollectWear { item } => match self.loose.get(item) {
                None => Err(Reject::Gone),
                Some(l) => collect_wear(&mut inv, l.stack.clone()).map(|()| {
                    self.loose.remove(item);
                    out.push(Event::Removed { item: *item });
                }),
            },
            Request::FitCanister { slot } => fit_canister(&mut inv, *slot),
            Request::PullCanister { slot } => pull_canister(&self.defs, &mut inv, *slot),
            Request::StoreWorn { rack, rack_slot } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => store_worn(&mut inv, &mut contents, *rack_slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::WearFrom { rack, rack_slot } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => wear_from(&mut inv, &mut contents, *rack_slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::StoreCanister { rack, rack_slot } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => store_canister(&self.defs, &mut inv, &mut contents, *rack_slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::FitFrom { rack, rack_slot } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => fit_from(&self.defs, &mut inv, &mut contents, *rack_slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::FitStoredCanister { slot, rack, rack_slot } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => fit_stored(&mut inv, &mut contents, *slot, *rack_slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::PullStoredCanister { rack, rack_slot, slot } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => pull_stored(&self.defs, &mut inv, &mut contents, *rack_slot, *slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::Swap { slot, rack } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => swap(&mut inv, &mut contents, *slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::Store { slot, rack } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => store(&mut inv, &mut contents, *slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::Fetch { rack, rack_slot } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => fetch(&self.defs, &mut inv, &mut contents, *rack_slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::StoreAt { slot, rack, rack_slot } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => store_at(&mut inv, &mut contents, *slot, *rack_slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::FetchTo { rack, rack_slot, slot } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => fetch_to(&self.defs, &mut inv, &mut contents, *rack_slot, *slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::MoveIn { rack, from, to } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => move_in(&inv, &mut contents, *from, *to).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
            Request::Open { rack } => {
                if !self.racks.contains_key(rack) {
                    Err(Reject::NoRack)
                } else {
                    if self.locks.insert(*rack, a.player) != Some(a.player) {
                        out.push(Event::Lock { rack: *rack, by: Some(a.player) });
                    }
                    Ok(())
                }
            }
            Request::Close { rack } => {
                if self.locks.get(rack) == Some(&a.player) {
                    self.locks.remove(rack);
                    out.push(Event::Lock { rack: *rack, by: None });
                }
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                self.inventories.insert(a.player, inv.clone());
                out.insert(0, Event::Inventory { player: a.player, inv });
                out.push(Event::Confirmed { player: a.player, seq: a.seq });
            }
            Err(why) => out = vec![Event::Rejected { player: a.player, seq: a.seq, why }],
        }
        out
    }
}

#[derive(Clone, Debug, Default)]
pub struct Client {
    pub player: PlayerId,
    pub defs: ItemDefs,
    pub loose: BTreeMap<LooseId, Loose>,
    pub racks: BTreeMap<RackId, Container>,
    /// Who has which container open, as the host last said.
    pub locks: BTreeMap<RackId, PlayerId>,
    pub confirmed: Inventory,
    /// The other players' inventories as the host last sent them (their held items show).
    pub others: BTreeMap<PlayerId, Inventory>,
    pub pending: Vec<Action>,
    /// What the player sees: `confirmed` with `pending` replayed.
    pub view: Inventory,
    /// Loose items a pending collect took (hidden in the world until the answer).
    pub hidden: BTreeSet<LooseId>,
    /// Rejected actions so far (the glue reports them).
    pub rejected: Vec<(u32, Reject)>,
    seq: u32,
}

impl Client {
    pub fn new(player: PlayerId, defs: ItemDefs) -> Client {
        Client { player, defs, ..Default::default() }
    }

    /// The player does something: it shows at once and the action goes to the host. A request
    /// that cannot work even here (an empty slot, a full inventory) is refused without sending.
    pub fn request(&mut self, req: Request) -> Result<Action, Reject> {
        self.would(&req)?;
        let mut view = self.view.clone();
        let mut hidden = self.hidden.clone();
        predict(&self.defs, &self.loose, &self.racks, &mut view, &mut hidden, &req)?;
        self.seq += 1;
        let a = Action { player: self.player, seq: self.seq, req };
        self.pending.push(a.clone());
        self.view = view;
        self.hidden = hidden;
        Ok(a)
    }

    /// Whether a request would work here and now (the host may still refuse): what the container
    /// window asks to show an accepting or a refusing place. Changes nothing.
    pub fn would(&self, req: &Request) -> Result<(), Reject> {
        if rack_of(req).is_some_and(|r| self.locks.get(&r).is_some_and(|p| *p != self.player)) {
            return Err(Reject::InUse);
        }
        let (mut view, mut hidden) = (self.view.clone(), self.hidden.clone());
        predict(&self.defs, &self.loose, &self.racks, &mut view, &mut hidden, req)
    }

    /// The player that has a container open, if any.
    pub fn holder(&self, rack: RackId) -> Option<PlayerId> {
        self.locks.get(&rack).copied()
    }

    pub fn apply(&mut self, ev: &Event) {
        match ev {
            Event::Lock { rack, by } => match by {
                Some(p) => {
                    self.locks.insert(*rack, *p);
                }
                None => {
                    self.locks.remove(rack);
                }
            },
            Event::Spawned { item, loose, .. } => {
                self.loose.insert(*item, loose.clone());
            }
            Event::Removed { item } => {
                self.loose.remove(item);
            }
            Event::Rest { item, at, .. } => {
                if let Some(l) = self.loose.get_mut(item) {
                    l.at = *at;
                }
            }
            Event::Rack { rack, contents } => {
                self.racks.insert(*rack, contents.clone());
            }
            Event::Inventory { player, inv } if *player == self.player => self.confirmed = inv.clone(),
            Event::Inventory { player, inv } => {
                self.others.insert(*player, inv.clone());
            }
            Event::Confirmed { player, seq } if *player == self.player => self.pending.retain(|a| a.seq != *seq),
            Event::Rejected { player, seq, why } if *player == self.player => {
                self.pending.retain(|a| a.seq != *seq);
                self.rejected.push((*seq, why.clone()));
            }
            Event::Confirmed { .. } | Event::Rejected { .. } | Event::Used { .. } => {}
        }
        self.rebuild();
    }

    /// The view again from the host's word and what is still in flight; a pending action that no
    /// longer works (its item went to someone else) drops out of the view until the host answers.
    fn rebuild(&mut self) {
        let mut view = self.confirmed.clone();
        let mut hidden = BTreeSet::new();
        for a in &self.pending {
            let _ = predict(&self.defs, &self.loose, &self.racks, &mut view, &mut hidden, &a.req);
        }
        self.view = view;
        self.hidden = hidden;
    }

    /// The stack in another player's hand, as the host last told (one hand's worth: the figure
    /// shows what a free-handed player holds).
    pub fn held_by(&self, player: PlayerId) -> Option<&Stack> {
        self.others.get(&player)?.in_hand(&self.defs, 0)
    }

    pub fn pending_drop(&self, seq: u32) -> bool {
        self.pending.iter().any(|a| a.seq == seq && a.req.drop_at().is_some())
    }
}

fn predict(defs: &ItemDefs, loose: &BTreeMap<LooseId, Loose>, racks: &BTreeMap<RackId, Container>, view: &mut Inventory, hidden: &mut BTreeSet<LooseId>, req: &Request) -> Result<(), Reject> {
    match req {
        Request::Collect { item } => {
            let l = loose.get(item).filter(|_| !hidden.contains(item)).ok_or(Reject::Gone)?;
            view.collect(defs, l.stack.clone())?;
            hidden.insert(*item);
        }
        Request::Drop { slot, .. } | Request::Throw { slot, .. } => {
            view.take_slot(*slot)?;
        }
        Request::DropWorn { .. } => {
            drop_worn(view)?;
        }
        Request::DropCanister { .. } => {
            drop_canister(defs, view)?;
        }
        Request::DropFrom { rack, rack_slot, .. } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            drop_from(view, &mut contents, *rack_slot)?;
        }
        Request::DropStoredCanister { rack, rack_slot, .. } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            drop_stored_canister(defs, view, &mut contents, *rack_slot)?;
        }
        Request::Select { slot } => view.select(*slot)?,
        Request::Move { from, to } => view.move_slot(*from, *to)?,
        Request::Use { slot, .. } => {
            use_one(defs, view, *slot)?;
        }
        Request::CollectWear { item } => {
            let l = loose.get(item).filter(|_| !hidden.contains(item)).ok_or(Reject::Gone)?;
            collect_wear(view, l.stack.clone())?;
            hidden.insert(*item);
        }
        Request::FitCanister { slot } => fit_canister(view, *slot)?,
        Request::PullCanister { slot } => pull_canister(defs, view, *slot)?,
        Request::StoreWorn { rack, rack_slot } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            store_worn(view, &mut contents, *rack_slot)?;
        }
        Request::WearFrom { rack, rack_slot } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            wear_from(view, &mut contents, *rack_slot)?;
        }
        Request::StoreCanister { rack, rack_slot } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            store_canister(defs, view, &mut contents, *rack_slot)?;
        }
        Request::FitFrom { rack, rack_slot } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            fit_from(defs, view, &mut contents, *rack_slot)?;
        }
        Request::FitStoredCanister { slot, rack, rack_slot } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            fit_stored(view, &mut contents, *slot, *rack_slot)?;
        }
        Request::PullStoredCanister { rack, rack_slot, slot } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            pull_stored(defs, view, &mut contents, *rack_slot, *slot)?;
        }
        Request::Swap { slot, rack } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            swap(view, &mut contents, *slot)?;
        }
        Request::Store { slot, rack } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            store(view, &mut contents, *slot)?;
        }
        Request::Fetch { rack, rack_slot } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            fetch(defs, view, &mut contents, *rack_slot)?;
        }
        Request::StoreAt { slot, rack, rack_slot } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            store_at(view, &mut contents, *slot, *rack_slot)?;
        }
        Request::FetchTo { rack, rack_slot, slot } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            fetch_to(defs, view, &mut contents, *rack_slot, *slot)?;
        }
        Request::MoveIn { rack, from, to } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            move_in(view, &mut contents, *from, *to)?;
        }
        Request::Open { rack } | Request::Close { rack } => {
            racks.get(rack).ok_or(Reject::NoRack)?;
        }
    }
    Ok(())
}

/// The container a request touches, if it does (the lock applies to these).
fn rack_of(req: &Request) -> Option<RackId> {
    match req {
        Request::Swap { rack, .. }
        | Request::Store { rack, .. }
        | Request::Fetch { rack, .. }
        | Request::StoreAt { rack, .. }
        | Request::FetchTo { rack, .. }
        | Request::MoveIn { rack, .. }
        | Request::StoreWorn { rack, .. }
        | Request::WearFrom { rack, .. }
        | Request::StoreCanister { rack, .. }
        | Request::FitFrom { rack, .. }
        | Request::FitStoredCanister { rack, .. }
        | Request::PullStoredCanister { rack, .. }
        | Request::DropFrom { rack, .. }
        | Request::DropStoredCanister { rack, .. }
        | Request::Open { rack } => Some(*rack),
        _ => None,
    }
}

/// The tags the inventory carries: what a locked container may open for (an ID card, say).
fn carried(inv: &Inventory) -> Vec<Tag> {
    (0..BAR_SLOTS).filter_map(|i| inv.slot(i)).flat_map(|s| s.item.tags.iter().cloned()).collect()
}

fn refusal(r: Refusal) -> Reject {
    match r {
        Refusal::Locked | Refusal::NeedsKey(_) => Reject::Locked,
        Refusal::NotAllowed => Reject::NotAllowed,
        other => other.into(),
    }
}

/// Moves the whole stack of an inventory slot into a container, or nothing.
fn store(inv: &mut Inventory, c: &mut Container, slot: usize) -> Result<(), Reject> {
    let stack = inv.slot(slot).cloned().ok_or(if slot < BAR_SLOTS { Reject::EmptySlot } else { Reject::NoSuchSlot })?;
    let mut trial_c = c.clone();
    match trial_c.try_insert(&carried(inv), stack) {
        Ok(None) => {}
        Ok(Some(_)) => return Err(Reject::Full),
        Err(r) => return Err(refusal(r.why)),
    }
    let mut trial_inv = inv.clone();
    trial_inv.take_slot(slot).map_err(Reject::from)?;
    *inv = trial_inv;
    *c = trial_c;
    Ok(())
}

/// Moves the whole stack of an inventory slot into one place of a container, or nothing.
fn store_at(inv: &mut Inventory, c: &mut Container, slot: usize, rack_slot: usize) -> Result<(), Reject> {
    let stack = inv.slot(slot).cloned().ok_or(if slot < BAR_SLOTS { Reject::EmptySlot } else { Reject::NoSuchSlot })?;
    let mut trial_c = c.clone();
    trial_c.insert_at(&carried(inv), rack_slot, stack).map_err(|r| refusal(r.why))?;
    let mut trial_inv = inv.clone();
    trial_inv.take_slot(slot).map_err(Reject::from)?;
    *inv = trial_inv;
    *c = trial_c;
    Ok(())
}

/// Moves the whole stack of a container place into one bar slot, or nothing.
fn fetch_to(defs: &ItemDefs, inv: &mut Inventory, c: &mut Container, rack_slot: usize, slot: usize) -> Result<(), Reject> {
    let n = c.slot(rack_slot).ok_or(if rack_slot < c.len() { Reject::EmptySlot } else { Reject::NoSuchSlot })?.count;
    let mut trial_c = c.clone();
    let stack = trial_c.take(&carried(inv), rack_slot, n).map_err(refusal)?;
    if !defs.of(&stack).is_some_and(|d| d.take == crate::defs::Take::Collect) || is_suit(&stack) {
        return Err(Reject::NotCollectable);
    }
    let mut trial_inv = inv.clone();
    trial_inv.bar.insert_at(&[], slot, stack).map_err(|r| match r.why {
        Refusal::NoSuchSlot => Reject::NoSuchSlot,
        _ => Reject::Full,
    })?;
    *inv = trial_inv;
    *c = trial_c;
    Ok(())
}

/// Moves a stack between two places of a container (swap, stack or move, as `Container::move_slot`).
fn move_in(inv: &Inventory, c: &mut Container, from: usize, to: usize) -> Result<(), Reject> {
    let mut trial_c = c.clone();
    trial_c.move_slot(&carried(inv), from, to).map_err(|r| match r {
        Refusal::NothingToMove => Reject::Invalid,
        r => refusal(r),
    })?;
    *c = trial_c;
    Ok(())
}

/// Moves the whole stack of a container slot into the inventory, or nothing.
fn fetch(defs: &ItemDefs, inv: &mut Inventory, c: &mut Container, rack_slot: usize) -> Result<(), Reject> {
    let n = c.slot(rack_slot).ok_or(if rack_slot < c.len() { Reject::EmptySlot } else { Reject::NoSuchSlot })?.count;
    let mut trial_c = c.clone();
    let stack = trial_c.take(&carried(inv), rack_slot, n).map_err(refusal)?;
    let mut trial_inv = inv.clone();
    trial_inv.collect(defs, stack).map_err(refusal)?;
    *inv = trial_inv;
    *c = trial_c;
    Ok(())
}

/// Hangs the stack of `slot` on the rack and takes the best spare of its kind from it (or any
/// spare into an empty slot). Nothing changes unless all of it works.
fn swap(inv: &mut Inventory, rack: &mut Container, slot: usize) -> Result<(), Reject> {
    if slot >= BAR_SLOTS {
        return Err(Reject::NoSuchSlot);
    }
    let own = inv.slot(slot).cloned();
    let kind = own.as_ref().map(|s| s.item.kind.clone());
    let spare = (0..rack.len())
        .filter(|i| rack.slot(*i).is_some_and(|s| kind.as_ref().is_none_or(|k| *k == s.item.kind)))
        .max_by(|a, b| {
            let c = |i: &usize| rack.slot(*i).and_then(|s| s.item.condition).unwrap_or(0.0);
            c(a).total_cmp(&c(b)).then(b.cmp(a))
        })
        .ok_or(Reject::NoSpare)?;
    let mut trial_inv = inv.clone();
    let mut trial_rack = rack.clone();
    let taken = trial_rack.take(&[], spare, 1).map_err(Reject::from)?;
    if is_suit(&taken) {
        return Err(Reject::NotCollectable);
    }
    if own.is_some() {
        let mine = trial_inv.take_slot(slot)?;
        trial_rack.insert_at(&[], spare, mine).map_err(|r| Reject::from(r.why))?;
    }
    trial_inv.bar.insert_at(&[], slot, taken).map_err(|r| Reject::from(r.why))?;
    *inv = trial_inv;
    *rack = trial_rack;
    Ok(())
}

/// Uses one item of a slot: the effect the host applies.
fn use_one(defs: &ItemDefs, inv: &mut Inventory, slot: usize) -> Result<Option<String>, Reject> {
    let stack = inv.slot(slot).ok_or(if slot < BAR_SLOTS { Reject::EmptySlot } else { Reject::NoSuchSlot })?;
    let def = defs.of(stack).ok_or(Reject::NotUsable)?;
    let effect = def.use_effect.clone().ok_or(Reject::NotUsable)?;
    if !def.keeps_on_use {
        inv.bar.take(&[], slot, 1)?;
    }
    Ok(Some(effect))
}

fn is_suit(s: &Stack) -> bool {
    s.count == 1 && s.item.has_tag(&Tag::new(SUIT_TAG))
}

fn is_canister(s: &Stack) -> bool {
    s.count == 1 && s.item.has_tag(&Tag::new(CANISTER_TAG))
}

fn bar_stack(inv: &Inventory, slot: usize) -> Result<Stack, Reject> {
    inv.slot(slot).cloned().ok_or(if slot < BAR_SLOTS { Reject::EmptySlot } else { Reject::NoSuchSlot })
}

/// Puts a stack where the bar has room: the slot asked, or the first free one; Full otherwise.
fn put_in_bar(inv: &mut Inventory, slot: Option<usize>, stack: Stack) -> Result<(), Reject> {
    match slot {
        Some(i) => inv.bar.insert_at(&[], i, stack).map_err(|r| match r.why {
            Refusal::NoSuchSlot => Reject::NoSuchSlot,
            _ => Reject::Full,
        }),
        None => match inv.bar.try_insert(&[], stack) {
            Ok(None) => Ok(()),
            _ => Err(Reject::Full),
        },
    }
}

/// A loose suit goes on, whole (a suit never takes a bar slot). Refused while a suit is worn.
fn collect_wear(inv: &mut Inventory, stack: Stack) -> Result<(), Reject> {
    if !is_suit(&stack) {
        return Err(Reject::NotAllowed);
    }
    if inv.worn.is_some() {
        return Err(Reject::Wearing);
    }
    inv.worn = Some(stack);
    Ok(())
}

/// The worn suit goes into a place of a container (a suit locker takes only the suit).
fn store_worn(inv: &mut Inventory, c: &mut Container, rack_slot: usize) -> Result<(), Reject> {
    let mut trial_inv = inv.clone();
    let suit = trial_inv.worn.take().ok_or(Reject::EmptySlot)?;
    let mut trial_c = c.clone();
    trial_c.insert_at(&carried(inv), rack_slot, suit).map_err(|r| refusal(r.why))?;
    *inv = trial_inv;
    *c = trial_c;
    Ok(())
}

/// The suit of a container place goes on; a suit already worn takes its place.
fn wear_from(inv: &mut Inventory, c: &mut Container, rack_slot: usize) -> Result<(), Reject> {
    let mut trial_c = c.clone();
    let mut trial_inv = inv.clone();
    let stack = trial_c.slot(rack_slot).cloned().ok_or(if rack_slot < trial_c.len() { Reject::EmptySlot } else { Reject::NoSuchSlot })?;
    if !is_suit(&stack) {
        return Err(Reject::NotAllowed);
    }
    trial_c.take(&carried(inv), rack_slot, 1).map_err(refusal)?;
    if let Some(old) = trial_inv.worn.take() {
        trial_c.insert_at(&[], rack_slot, old).map_err(|r| refusal(r.why))?;
    }
    trial_inv.worn = Some(stack);
    *inv = trial_inv;
    *c = trial_c;
    Ok(())
}

/// A canister of a bar slot goes into the worn suit; the canister the suit held takes the slot.
fn fit_canister(inv: &mut Inventory, slot: usize) -> Result<(), Reject> {
    let can = bar_stack(inv, slot)?;
    if !is_canister(&can) {
        return Err(Reject::NotAllowed);
    }
    let mut trial = inv.clone();
    let suit = trial.worn.as_mut().ok_or(Reject::Invalid)?;
    let held = suit.item.condition.replace(can.item.condition.unwrap_or(100.0));
    trial.take_slot(slot).map_err(Reject::from)?;
    if let Some(fill) = held {
        let mut old = can.clone();
        old.item.condition = Some(fill);
        put_in_bar(&mut trial, Some(slot), old)?;
    }
    *inv = trial;
    Ok(())
}

/// The worn suit's canister comes out into a bar slot (None: the first free one).
fn pull_canister(defs: &ItemDefs, inv: &mut Inventory, slot: Option<usize>) -> Result<(), Reject> {
    let mut trial = inv.clone();
    let suit = trial.worn.as_mut().ok_or(Reject::Invalid)?;
    let fill = suit.item.condition.take().ok_or(Reject::EmptySlot)?;
    let def = defs.get("air_canister").ok_or(Reject::Invalid)?;
    let mut can = def.stack_of(1);
    can.item.condition = Some(fill);
    put_in_bar(&mut trial, slot, can)?;
    *inv = trial;
    Ok(())
}

/// A canister as an item, with the fill a suit held.
fn canister_of(defs: &ItemDefs, fill: f64) -> Result<Stack, Reject> {
    let mut can = defs.get("air_canister").ok_or(Reject::Invalid)?.stack_of(1);
    can.item.condition = Some(fill);
    Ok(can)
}

/// The worn suit's canister goes into a place of a container (a canister box takes only canisters).
fn store_canister(defs: &ItemDefs, inv: &mut Inventory, c: &mut Container, rack_slot: usize) -> Result<(), Reject> {
    let mut trial_inv = inv.clone();
    let suit = trial_inv.worn.as_mut().ok_or(Reject::Invalid)?;
    let fill = suit.item.condition.take().ok_or(Reject::EmptySlot)?;
    let mut trial_c = c.clone();
    trial_c.insert_at(&carried(inv), rack_slot, canister_of(defs, fill)?).map_err(|r| refusal(r.why))?;
    *inv = trial_inv;
    *c = trial_c;
    Ok(())
}

/// The canister of a container place goes into the worn suit; the canister the suit held takes that place.
fn fit_from(defs: &ItemDefs, inv: &mut Inventory, c: &mut Container, rack_slot: usize) -> Result<(), Reject> {
    let mut trial_c = c.clone();
    let mut trial_inv = inv.clone();
    let can = trial_c.slot(rack_slot).cloned().ok_or(if rack_slot < trial_c.len() { Reject::EmptySlot } else { Reject::NoSuchSlot })?;
    if !is_canister(&can) {
        return Err(Reject::NotAllowed);
    }
    let suit = trial_inv.worn.as_mut().ok_or(Reject::Invalid)?;
    trial_c.take(&carried(inv), rack_slot, 1).map_err(refusal)?;
    if let Some(fill) = suit.item.condition.replace(can.item.condition.unwrap_or(100.0)) {
        trial_c.insert_at(&[], rack_slot, canister_of(defs, fill)?).map_err(|r| refusal(r.why))?;
    }
    *inv = trial_inv;
    *c = trial_c;
    Ok(())
}

/// The suit lying in a container place, taken out for a change of its canister (the container's own
/// access rules apply: `take` and `insert_at`). Put back with `put_back_suit`.
fn take_stored_suit(inv: &Inventory, c: &mut Container, rack_slot: usize) -> Result<Stack, Reject> {
    let stack = c.slot(rack_slot).cloned().ok_or(if rack_slot < c.len() { Reject::EmptySlot } else { Reject::NoSuchSlot })?;
    if !is_suit(&stack) {
        return Err(Reject::Invalid);
    }
    c.take(&carried(inv), rack_slot, 1).map_err(refusal)
}

fn put_back_suit(inv: &Inventory, c: &mut Container, rack_slot: usize, suit: Stack) -> Result<(), Reject> {
    c.insert_at(&carried(inv), rack_slot, suit).map_err(|r| refusal(r.why))
}

/// The canister of a bar slot goes into the suit of a container place; the canister the suit held takes the slot.
fn fit_stored(inv: &mut Inventory, c: &mut Container, slot: usize, rack_slot: usize) -> Result<(), Reject> {
    let can = bar_stack(inv, slot)?;
    if !is_canister(&can) {
        return Err(Reject::NotAllowed);
    }
    let mut trial_c = c.clone();
    let mut suit = take_stored_suit(inv, &mut trial_c, rack_slot)?;
    let held = suit.item.condition.replace(can.item.condition.unwrap_or(100.0));
    put_back_suit(inv, &mut trial_c, rack_slot, suit)?;
    let mut trial_inv = inv.clone();
    trial_inv.take_slot(slot).map_err(Reject::from)?;
    if let Some(fill) = held {
        let mut old = can.clone();
        old.item.condition = Some(fill);
        put_in_bar(&mut trial_inv, Some(slot), old)?;
    }
    *inv = trial_inv;
    *c = trial_c;
    Ok(())
}

/// The canister of the suit in a container place comes out into a bar slot (None: the first free one).
fn pull_stored(defs: &ItemDefs, inv: &mut Inventory, c: &mut Container, rack_slot: usize, slot: Option<usize>) -> Result<(), Reject> {
    let mut trial_c = c.clone();
    let mut suit = take_stored_suit(inv, &mut trial_c, rack_slot)?;
    let fill = suit.item.condition.take().ok_or(Reject::EmptySlot)?;
    put_back_suit(inv, &mut trial_c, rack_slot, suit)?;
    let mut trial_inv = inv.clone();
    put_in_bar(&mut trial_inv, slot, canister_of(defs, fill)?)?;
    *inv = trial_inv;
    *c = trial_c;
    Ok(())
}

/// The worn suit leaves the body (to the world); always allowed.
fn drop_worn(inv: &mut Inventory) -> Result<Stack, Reject> {
    inv.worn.take().ok_or(Reject::EmptySlot)
}

/// The canister of the worn suit leaves it (to the world) as a canister item.
fn drop_canister(defs: &ItemDefs, inv: &mut Inventory) -> Result<Stack, Reject> {
    let suit = inv.worn.as_mut().ok_or(Reject::Invalid)?;
    let fill = suit.item.condition.take().ok_or(Reject::EmptySlot)?;
    canister_of(defs, fill)
}

/// The whole stack of a container place leaves it (to the world).
fn drop_from(inv: &Inventory, c: &mut Container, rack_slot: usize) -> Result<Stack, Reject> {
    let n = c.slot(rack_slot).ok_or(if rack_slot < c.len() { Reject::EmptySlot } else { Reject::NoSuchSlot })?.count;
    let mut trial = c.clone();
    let stack = trial.take(&carried(inv), rack_slot, n).map_err(refusal)?;
    *c = trial;
    Ok(stack)
}

/// The canister of the suit in a container place leaves it (to the world).
fn drop_stored_canister(defs: &ItemDefs, inv: &Inventory, c: &mut Container, rack_slot: usize) -> Result<Stack, Reject> {
    let mut trial = c.clone();
    let mut suit = take_stored_suit(inv, &mut trial, rack_slot)?;
    let fill = suit.item.condition.take().ok_or(Reject::EmptySlot)?;
    put_back_suit(inv, &mut trial, rack_slot, suit)?;
    let can = canister_of(defs, fill)?;
    *c = trial;
    Ok(can)
}
