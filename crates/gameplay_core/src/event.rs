//! Domain events: the messages "something happened" that every system reads. The host applies
//! them; clients send requests. The envelope is generic over its payload, so each system brings its
//! own event kinds and the kernel stays unaware of them.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::id::{ClientId, CrateId, Flag, LocationId, TrackId, UnlockId};

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

    /// Ids held above the low-water marks (a measure of how out of order events arrive).
    pub fn pending(&self) -> usize {
        self.senders.values().map(|w| w.above.len()).sum()
    }
}
