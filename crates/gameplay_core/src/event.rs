//! Domain events: the messages "something happened" that every system reads. The host applies
//! them; clients send requests. The envelope is generic over its payload, so each system brings its
//! own event kinds and the kernel stays unaware of them.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::id::{ClientId, CommodityId, CrateId, Flag, LocationId, OrderId, TownId, TrackId, UnlockId};
use crate::product::ProductSpec;

/// Unique per event: the sender plus the sender's own running number. No coordination needed;
/// a retried or duplicated request keeps its id.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId {
    pub sender: ClientId,
    pub seq: u64,
}

/// A domain event: id (which names the sender) and what happened.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event<P> {
    pub id: EventId,
    pub payload: P,
}

impl<P> Event<P> {
    pub fn new(sender: ClientId, seq: u64, payload: P) -> Event<P> {
        Event { id: EventId { sender, seq }, payload }
    }

    /// The client that sent it; for events the host raises itself, the host's client.
    pub fn sender(&self) -> ClientId {
        self.id.sender
    }
}

/// Events about the world and the crew's progress, shared by all systems.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldEvent {
    /// A crate left the pad of a location in someone's hands or ship.
    CratePickedUp { crate_id: CrateId, at: LocationId },
    /// A crate was set down on the pad of a location; `condition` 0..1.
    CrateDelivered { crate_id: CrateId, at: LocationId, condition: f64 },
    /// A crate is gone (destroyed, despawned).
    CrateLost { crate_id: CrateId },
    /// Simulated time went by, seconds.
    TimePassed { dt: f64 },
    /// The sender joined the session.
    PlayerJoined,
    /// The sender buys an unlock for the crew.
    UnlockBought { unlock: UnlockId },
    /// A system changes a track, e.g. a payout. `player` names whose personal track; crew tracks
    /// ignore it.
    TrackChanged { track: TrackId, delta: i64, player: Option<ClientId> },
    /// A system states a fact that conditions can ask for (`job_completed:<template>`).
    FlagRaised { flag: Flag },
    /// A buyer (`by`: the id the customers system uses, for texts) wants goods: pick up `amount` of `commodity` at `from`, bring them to `to` for
    /// `reward`. The customers system raises it, the jobs system turns it into an offer (#168).
    OrderPlaced { order: OrderId, by: String, from: LocationId, to: LocationId, commodity: CommodityId, amount: u32, reward: i64, deadline_s: Option<f64> },
    /// The sender's ship left the ground under their hands (#169). The exam asks for it; nothing
    /// here depends on how the ship flies.
    TookOff,
    /// The sender's ship came over the pad of a location.
    PadReached { at: LocationId },
    /// The sender's ship touched down, at the pad of a location or elsewhere, with this speed
    /// towards the ground in m/s.
    Landed { at: Option<LocationId>, speed: f64 },
    /// What came of an order: how many crates arrived, how many were asked for, their mean
    /// condition 0..1, and whether it was in time. The jobs system raises it when the job ends;
    /// the customers system reads it.
    OrderSettled { order: OrderId, delivered: u32, asked: u32, condition: f64, in_time: bool },

    // ---- Round 6, the smuggling run. Each system below owns the events it raises; the others
    // only read them. Requests (the sender asks) are marked; the rest are facts.

    // Traffic (`traffic_core`): airspace, the tower, clearance, landing grades.
    /// The sender's ship came into a town's airspace.
    AirspaceEntered { town: TownId },
    /// The sender's ship left a town's airspace.
    AirspaceLeft { town: TownId },
    /// Request: the sender calls the town's tower for a landing.
    Hailed { town: TownId },
    /// The tower gives the sender a pad to land on within `window_s`. `inspection`: the pad is
    /// the inspection pad (high heat); a scan there is certain.
    ClearanceGranted { town: TownId, pad: LocationId, window_s: f64, inspection: bool },
    /// The tower puts the sender in a holding pattern; it answers again after `wait_s`.
    ClearanceHeld { town: TownId, wait_s: f64 },
    /// The sender's clearance ran out before landing.
    ClearanceExpired { town: TownId },
    /// The sender broke a rule of a town's airspace.
    Violation { town: TownId, kind: ViolationKind },
    /// A landing on a town pad, graded 0 (rough) to 3 (perfect); `on_time`: inside the clearance
    /// window on the assigned pad.
    LandingGraded { town: TownId, at: LocationId, grade: u8, on_time: bool },

    // Heat (`heat_core`): the wanted level, scans and their outcome.
    /// The crew's heat crossed a star; `stars` is the new level 0..=5.
    HeatStars { stars: u8 },
    /// A scan begins on a pad: `visible` contraband crates aboard or on the pad, `hidden` in the
    /// hidden hold. The glue raises it after asking `heat_core` whether a scan happens.
    ScanStarted { scan: u64, at: LocationId, visible: u32, hidden: u32 },
    /// Request: the sender answers a scan.
    ScanAnswered { scan: u64, choice: ScanChoice },
    /// What came of a scan. `confiscate`: the visible contraband is taken; `fine` is charged
    /// (also as a wallet `TrackChanged`); `heat` is the change in heat points.
    ScanResolved { scan: u64, choice: ScanChoice, confiscate: bool, fine: i64, heat: f64 },
    /// Request: the sender pays heat down at a fixer, `amount` money.
    HeatPaidDown { amount: i64 },

    // Production (`production_core`): brewing and packing.
    /// A station finished a batch: `crates` crates of `product`. `discovered`: the first time the
    /// crew made this mix (it goes into the recipe book).
    Brewed { station: String, product: ProductSpec, crates: u32, discovered: bool },
    /// Request: the sender buys `amount` crates of a good at a counter (ingredients, supplies).
    GoodsBought { commodity: CommodityId, amount: u32, at: LocationId },

    // Buyers (`customers_core`): product orders and dealing.
    /// A buyer wants `amount` crates of a product with these effects (at least `min_quality`),
    /// handed over at `at` in `town`, for `price` in total.
    ProductOrderPlaced { order: OrderId, by: String, town: TownId, at: LocationId, wants: Vec<crate::id::EffectId>, min_quality: f64, amount: u32, price: i64, deadline_s: Option<f64> },
    /// Request: the sender asks for `ask_pct` percent more on an order.
    OrderCountered { order: OrderId, ask_pct: u32 },
    /// The buyer's answer to a counter-offer: the price that now holds.
    CounterAnswered { order: OrderId, accepted: bool, price: i64 },
    /// Request: the sender hands crates of these products to the buyer of an order.
    ProductHandedOver { order: OrderId, products: Vec<ProductSpec> },
    /// What the buyer paid and how well the goods matched (0..1).
    ProductOrderSettled { order: OrderId, paid: i64, matched: f64 },
    /// A neglected buyer now buys from the rival.
    CustomerPoached { by: String },
    /// A pleased buyer introduced a new one.
    CustomerReferred { by: String, new: String },

    // Empire (`gameplay_core::goals`): the family's rank ladder.
    /// Request: the sender pays tribute to a ladder (the family) to rise.
    TributePaid { ladder: String, amount: i64 },
    /// The crew rose on a ladder to `rank` (0 is the first rank).
    RankReached { ladder: String, rank: u32 },
}

/// A broken airspace rule.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    /// Landed on a town pad without clearance.
    NoClearance,
    /// Landed on another pad than the assigned one.
    WrongPad,
    /// Too fast inside the airspace.
    Speeding,
    /// Left a pad during a scan.
    FledScan,
}

/// The four answers to a scan.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanChoice {
    /// Let them take it: confiscation plus a fine.
    Comply,
    /// Pay them to look away.
    Bribe,
    /// Talk your way out (a chance).
    Talk,
    /// Take off during the scan.
    Flee,
}

/// Which event ids the host has applied. An id seen before is not applied again (co-op retries,
/// duplicates). Per sender it keeps a low-water mark plus the few ids above it, so it stays small.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dedup {
    senders: BTreeMap<ClientId, Window>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Window {
    /// Every seq below this was seen.
    below: u64,
    /// Seen seqs at or above `below`.
    above: BTreeSet<u64>,
}

impl Dedup {
    /// True the first time an id comes by (then it is recorded), false for every repeat.
    pub fn first_time(&mut self, id: EventId) -> bool {
        let w = self.senders.entry(id.sender).or_default();
        if id.seq < w.below || !w.above.insert(id.seq) {
            return false;
        }
        while w.above.remove(&w.below) {
            w.below += 1;
        }
        true
    }

    /// The next number a sender may use: above every id seen from it (0 for an unknown sender).
    /// A restarted client continues from here, so its new events are not taken for duplicates.
    pub fn next_seq(&self, sender: ClientId) -> u64 {
        self.senders.get(&sender).map_or(0, |w| w.above.last().map_or(w.below, |m| m + 1))
    }

    /// Ids held above the low-water marks (a measure of how out of order events arrive).
    pub fn pending(&self) -> usize {
        self.senders.values().map(|w| w.above.len()).sum()
    }
}
