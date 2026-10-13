//! The path every item change takes (concept repo `decisions/crew-ship.md`, "Interaction model";
//! `decisions/project.md`, "Played over the internet").
//!
//! A player's action is a `Request`. The acting `Client` shows it at once (prediction) and sends
//! it as an `Action` to the `Host`, which owns the shared state (loose items, every inventory),
//! applies it or refuses it, and answers with `Event`s: what changed, then `Confirmed` or
//! `Rejected` for the action. The client keeps the host's last word (`confirmed`) and the actions
//! still in flight (`pending`); its `view` is always the confirmed state with the pending actions
//! replayed on top, so a rejected action simply drops out of it. Barotrauma works the same way
//! over the internet: the server owns items, clients predict their own actions.
//!
//! Host and client may live in one process (C1) or on two machines (C2 routes the same messages
//! through the network); nothing here knows which.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::container::{Refusal, Stack};
use crate::defs::ItemDefs;
use crate::inventory::Inventory;

/// A player's slot in the session (1 is the host's own player).
pub type PlayerId = u32;
/// A loose item in the world, numbered by the host.
pub type LooseId = u64;

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

/// A loose item: what it is and where the host put it (where it lies now is the physics').
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Loose {
    pub stack: Stack,
    pub at: Placement,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// F on a loose item: into the inventory.
    Collect { item: LooseId },
    /// Let the stack of a slot fall at `at`.
    Drop { slot: usize, at: Placement },
    /// Throw it: the same as a drop, `at` carries the throw's velocity.
    Throw { slot: usize, at: Placement },
    /// Take a slot into the hand.
    Select { slot: usize },
    /// Move, stack or swap within the own inventory.
    Move { from: usize, to: usize },
}

/// A request on its way to the host: who, their running number, what.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub player: PlayerId,
    pub seq: u32,
    pub req: Request,
}

/// Why the host refused an action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reject {
    /// The loose item is no longer there (someone else was first).
    Gone,
    /// It does not fit into the inventory.
    Full,
    /// Not something that goes into an inventory (cargo is carried).
    NotCollectable,
    EmptySlot,
    NoSuchSlot,
    /// The move is not possible (same slot, nothing to move).
    Invalid,
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

/// What the host tells every client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// A loose item appeared; `by` names the action that dropped it, so that client can keep the
    /// item it already shows instead of adding a second one.
    Spawned { item: LooseId, loose: Loose, by: Option<(PlayerId, u32)> },
    /// A loose item is gone (collected).
    Removed { item: LooseId },
    /// A player's inventory as the host has it now.
    Inventory { player: PlayerId, inv: Inventory },
    /// A loose item came to rest at `at`, turned as the quaternion `rot` (x, y, z, w; a resting
    /// item may lie on its side, which `at.forward` cannot say), at the shared session time `t`
    /// (the host's clock). While an item moves, its poses go out as datagrams, not events; this is
    /// its last word.
    Rest { item: LooseId, at: Placement, rot: [f64; 4], t: f64 },
    Confirmed { player: PlayerId, seq: u32 },
    Rejected { player: PlayerId, seq: u32, why: Reject },
}

/// The shared state and the one place that changes it.
#[derive(Clone, Debug, Default)]
pub struct Host {
    pub defs: ItemDefs,
    pub loose: BTreeMap<LooseId, Loose>,
    pub inventories: BTreeMap<PlayerId, Inventory>,
    next: LooseId,
}

impl Host {
    pub fn new(defs: ItemDefs) -> Host {
        Host { defs, ..Default::default() }
    }

    /// Puts a loose item into the world (level content, a drop): its id and the event.
    pub fn spawn(&mut self, stack: Stack, at: Placement, by: Option<(PlayerId, u32)>) -> Event {
        self.next += 1;
        let item = self.next;
        let loose = Loose { stack, at };
        self.loose.insert(item, loose.clone());
        Event::Spawned { item, loose, by }
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
        loose.chain(inventories).collect()
    }

    pub fn inventory(&self, p: PlayerId) -> Inventory {
        self.inventories.get(&p).cloned().unwrap_or_default()
    }

    /// Applies one action, or refuses it; the events in the order clients should apply them.
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

/// One player's side: the host's last word, the actions in flight, and the predicted view.
#[derive(Clone, Debug, Default)]
pub struct Client {
    pub player: PlayerId,
    pub defs: ItemDefs,
    /// Loose items as the host announced them.
    pub loose: BTreeMap<LooseId, Loose>,
    /// The own inventory as the host last sent it.
    pub confirmed: Inventory,
    /// The other players' inventories as the host last sent them (their held items show).
    pub others: BTreeMap<PlayerId, Inventory>,
    /// Sent, not yet answered.
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
        predict(&self.defs, &self.loose, &mut view, &mut hidden, &req)?;
        self.seq += 1;
        let a = Action { player: self.player, seq: self.seq, req };
        self.pending.push(a.clone());
        self.view = view;
        self.hidden = hidden;
        Ok(a)
    }

    /// An event from the host.
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
            Event::Inventory { player, inv } if *player == self.player => self.confirmed = inv.clone(),
            Event::Inventory { player, inv } => {
                self.others.insert(*player, inv.clone());
            }
            Event::Confirmed { player, seq } if *player == self.player => self.pending.retain(|a| a.seq != *seq),
            Event::Rejected { player, seq, why } if *player == self.player => {
                self.pending.retain(|a| a.seq != *seq);
                self.rejected.push((*seq, why.clone()));
            }
            Event::Confirmed { .. } | Event::Rejected { .. } => {}
        }
        self.rebuild();
    }

    /// The view again from the host's word and what is still in flight; a pending action that no
    /// longer works (its item went to someone else) drops out of the view until the host answers.
    fn rebuild(&mut self) {
        let mut view = self.confirmed.clone();
        let mut hidden = BTreeSet::new();
        for a in &self.pending {
            let _ = predict(&self.defs, &self.loose, &mut view, &mut hidden, &a.req);
        }
        self.view = view;
        self.hidden = hidden;
    }

    /// The stack in another player's hand, as the host last told (one hand's worth: the figure
    /// shows what a free-handed player holds).
    pub fn held_by(&self, player: PlayerId) -> Option<&Stack> {
        self.others.get(&player)?.in_hand(&self.defs, 0)
    }

    /// A drop or throw of this client still waiting for the host (the glue shows its item).
    pub fn pending_drop(&self, seq: u32) -> bool {
        self.pending.iter().any(|a| a.seq == seq && matches!(a.req, Request::Drop { .. } | Request::Throw { .. }))
    }
}

/// Applies a request to a view, as the host would.
fn predict(defs: &ItemDefs, loose: &BTreeMap<LooseId, Loose>, view: &mut Inventory, hidden: &mut BTreeSet<LooseId>, req: &Request) -> Result<(), Reject> {
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
    }
    Ok(())
}
