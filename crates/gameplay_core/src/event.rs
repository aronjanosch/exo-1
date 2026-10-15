use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::id::{ClientId, CommodityId, CrateId, Flag, LocationId, OrderId, TrackId, UnlockId};
use crate::product::ProductSpec;

/// Unique per event: the sender plus the sender's own running number. No coordination needed;
/// a retried or duplicated request keeps its id.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId {
    pub sender: ClientId,
    pub seq: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event<P> {
    pub id: EventId,
    pub payload: P,
}

impl<P> Event<P> {
    pub fn new(sender: ClientId, seq: u64, payload: P) -> Event<P> {
        Event { id: EventId { sender, seq }, payload }
    }

    pub fn sender(&self) -> ClientId {
        self.id.sender
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldEvent {
    CratePickedUp { crate_id: CrateId, at: LocationId },
    /// A crate was set down on the pad of a location; `condition` 0..1.
    CrateDelivered { crate_id: CrateId, at: LocationId, condition: f64 },
    CrateLost { crate_id: CrateId },
    /// Simulated time went by, seconds.
    TimePassed { dt: f64 },
    PlayerJoined,
    UnlockBought { unlock: UnlockId },
    /// A system changes a track, e.g. a payout. `player` names whose personal track; crew tracks
    /// ignore it.
    TrackChanged { track: TrackId, delta: i64, player: Option<ClientId> },
    /// A system states a fact that conditions can ask for (`job_completed:<template>`).
    FlagRaised { flag: Flag },
    /// A customer (`by`: the id used for texts) wants goods: pick up `amount` of `commodity` at `from`, bring them to `to` for
    /// `reward`. The jobs system turns it into an offer.
    OrderPlaced { order: OrderId, by: String, from: LocationId, to: LocationId, commodity: CommodityId, amount: u32, reward: i64, deadline_s: Option<f64> },
    /// The sender's ship left the ground under their hands. The exam asks for it; nothing
    /// here depends on how the ship flies.
    TookOff,
    PadReached { at: LocationId },
    /// The sender's ship touched down, at the pad of a location or elsewhere, with this speed
    /// towards the ground in m/s.
    Landed { at: Option<LocationId>, speed: f64 },
    /// What came of an order: how many crates arrived, how many were asked for, their mean
    /// condition 0..1, and whether it was in time. The jobs system raises it when the job ends.
    OrderSettled { order: OrderId, delivered: u32, asked: u32, condition: f64, in_time: bool },

    /// A station finished a batch: `crates` crates of `product`. `discovered`: the first time the
    /// crew made this mix (it goes into the recipe book).
    Brewed { station: String, product: ProductSpec, crates: u32, discovered: bool },
    GoodsBought { commodity: CommodityId, amount: u32, at: LocationId },
}

/// Which event ids the host has applied. An id seen before is not applied again (co-op retries,
/// duplicates). Per sender it keeps a low-water mark plus the few ids above it, so it stays small.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dedup {
    senders: BTreeMap<ClientId, Window>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Window {
    below: u64,
    above: BTreeSet<u64>,
}

impl Dedup {
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

    pub fn pending(&self) -> usize {
        self.senders.values().map(|w| w.above.len()).sum()
    }
}
