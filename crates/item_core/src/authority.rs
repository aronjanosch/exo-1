//! The path every item change takes (concept repo `decisions/crew-ship.md`, "Interaction model";
//! `decisions/project.md`, "Played over the internet").
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::container::{Access, Container, Refusal, SlotRule, Stack};
use crate::defs::ItemDefs;
use crate::inventory::{Inventory, BAR_SLOTS};

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
    EmptySlot,
    NoSuchSlot,
    Invalid,
    NotUsable,
    NoRack,
    NoSpare,
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
}

#[derive(Clone, Debug, Default)]
pub struct Host {
    pub defs: ItemDefs,
    pub loose: BTreeMap<LooseId, Loose>,
    pub inventories: BTreeMap<PlayerId, Inventory>,
    pub racks: BTreeMap<RackId, Container>,
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
    /// runs. The event (the inventory as now) comes only when the whole percent changes or the
    /// item runs out, so the clients hear it a few times a second at most.
    pub fn wear(&mut self, player: PlayerId, slot: usize, amount: f64) -> Option<Event> {
        let inv = self.inventories.get_mut(&player)?;
        let item = &mut inv.bar.slot_mut(slot)?.item;
        let before = item.condition?;
        let after = (before - amount).max(0.0);
        item.condition = Some(after);
        let changed = before.floor() != after.floor() || (before > 0.0 && after == 0.0);
        changed.then(|| Event::Inventory { player, inv: inv.clone() })
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
        loose.chain(inventories).chain(racks).collect()
    }

    pub fn inventory(&self, p: PlayerId) -> Inventory {
        self.inventories.get(&p).cloned().unwrap_or_default()
    }

    pub fn handle(&mut self, a: &Action) -> Vec<Event> {
        let mut inv = self.inventory(a.player);
        let mut out = Vec::new();
        let result: Result<(), Reject> = match &a.req {
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
            Request::Drop { at, .. } | Request::Throw { at, .. } if !at.sane() => Err(Reject::Invalid),
            Request::Drop { slot, at } | Request::Throw { slot, at } => match inv.take_slot(*slot) {
                Ok(stack) => {
                    out.push(self.spawn(stack, *at, Some((a.player, a.seq))));
                    Ok(())
                }
                Err(r) => Err(r.into()),
            },
            Request::Select { slot } => inv.select(*slot).map_err(Reject::from),
            Request::Move { from, to } => inv.move_slot(*from, *to).map_err(Reject::from),
            Request::Use { slot, target } => use_one(&self.defs, &mut inv, *slot).map(|effect| {
                out.push(Event::Used { player: a.player, target: *target, effect });
            }),
            Request::Swap { slot, rack } => match self.racks.get(rack).cloned() {
                None => Err(Reject::NoRack),
                Some(mut contents) => swap(&mut inv, &mut contents, *slot).map(|()| {
                    self.racks.insert(*rack, contents.clone());
                    out.push(Event::Rack { rack: *rack, contents });
                }),
            },
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

    pub fn apply(&mut self, ev: &Event) {
        match ev {
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
        self.pending.iter().any(|a| a.seq == seq && matches!(a.req, Request::Drop { .. } | Request::Throw { .. }))
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
        Request::Select { slot } => view.select(*slot)?,
        Request::Move { from, to } => view.move_slot(*from, *to)?,
        Request::Use { slot, .. } => {
            use_one(defs, view, *slot)?;
        }
        Request::Swap { slot, rack } => {
            let mut contents = racks.get(rack).cloned().ok_or(Reject::NoRack)?;
            swap(view, &mut contents, *slot)?;
        }
    }
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
    if own.is_some() {
        let mine = trial_inv.take_slot(slot)?;
        trial_rack.insert_at(&[], spare, mine).map_err(|r| Reject::from(r.why))?;
    }
    trial_inv.bar.insert_at(&[], slot, taken).map_err(|r| Reject::from(r.why))?;
    *inv = trial_inv;
    *rack = trial_rack;
    Ok(())
}

fn use_one(defs: &ItemDefs, inv: &mut Inventory, slot: usize) -> Result<String, Reject> {
    let stack = inv.slot(slot).ok_or(if slot < BAR_SLOTS { Reject::EmptySlot } else { Reject::NoSuchSlot })?;
    let effect = defs.of(stack).and_then(|d| d.use_effect.clone()).ok_or(Reject::NotUsable)?;
    inv.bar.take(&[], slot, 1)?;
    Ok(effect)
}
