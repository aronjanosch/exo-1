//! Cargo crates on the wire (#332). The host owns the crates (`grab_core::holders` decides who
//! holds one); these are the messages. Poses while a crate moves go as datagrams
//! (`net_core::loose`, the id carries `CRATE_KEY`), the rest is reliable.
use serde::{Deserialize, Serialize};

use crate::authority::{FrameRef, RackId};
use crate::PlayerId;

pub type CrateNetId = u32;

/// Set on a crate's id in a body datagram, so it never meets the id of a loose item.
pub const CRATE_KEY: u64 = 1 << 63;

pub fn key(id: CrateNetId) -> u64 {
    CRATE_KEY | id as u64
}

/// The crate id of a datagram body id, if it is a crate's.
pub fn unkey(k: u64) -> Option<CrateNetId> {
    (k & CRATE_KEY != 0).then(|| (k & !CRATE_KEY) as CrateNetId)
}

/// A crate's pose in its frame (a planet, or the cabin of the ship of that slot).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CratePose {
    pub frame: FrameRef,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    /// Quaternion x, y, z, w.
    pub rot: [f64; 4],
}

impl CratePose {
    /// Finite numbers, a position and a speed that can be, a rotation that is one. The host checks
    /// every pose a client sends.
    pub fn sane(&self) -> bool {
        let len = |v: &[f64]| v.iter().map(|c| c * c).sum::<f64>().sqrt();
        let finite = |v: &[f64]| v.iter().all(|c| c.is_finite());
        finite(&self.pos) && finite(&self.vel) && finite(&self.rot) && len(&self.pos) <= 1.0e9 && len(&self.vel) <= crate::authority::MAX_THROW_SPEED && (0.5..=1.5).contains(&(len(&self.rot).powi(2)))
    }
}

/// Client to host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CrateAsk {
    /// Ask to hold the crate. The client carries it at once and rolls back on `Refused`.
    Take { id: CrateNetId },
    /// Let go, at this pose and velocity (set down or thrown): the host's physics takes over.
    Release { id: CrateNetId, at: CratePose },
}

/// Host to clients.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CrateNews {
    /// A crate exists (also what a late joiner is told for each one). `size` is the row of the
    /// crate table, `rack` the container of an ore tub, `by` who holds it now.
    Spawn { id: CrateNetId, size: String, rack: Option<RackId>, at: CratePose, by: Option<PlayerId> },
    Despawn { id: CrateNetId },
    /// The crate came to rest at `at` (host time `t`); nothing more until it moves again.
    Rest { id: CrateNetId, at: CratePose, t: f64 },
    /// The holder changed: `by` holds it now, or nobody does (let go at `at`, host time `t`).
    Held { id: CrateNetId, by: Option<PlayerId>, at: CratePose, t: f64 },
    /// The host did not give the asker the crate (somebody else holds it, or it is gone).
    Refused { id: CrateNetId },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{decode, encode, ToClient, ToHost};

    fn pose() -> CratePose {
        CratePose { frame: FrameRef::Ship(2), pos: [1.0, 2.5, -3.0], vel: [0.0, -1.0, 0.5], rot: [0.0, 0.0, 0.0, 1.0] }
    }

    #[test]
    fn every_crate_ask_travels_as_it_is() {
        for ask in [CrateAsk::Take { id: 7 }, CrateAsk::Release { id: 7, at: pose() }] {
            assert_eq!(decode::<ToHost>(&encode(&ToHost::Crate(ask.clone()))), Some(ToHost::Crate(ask)));
        }
    }

    #[test]
    fn every_crate_news_travels_as_it_is() {
        let news = [
            CrateNews::Spawn { id: 1, size: "tub".into(), rack: Some(107), at: pose(), by: None },
            CrateNews::Spawn { id: 2, size: "small".into(), rack: None, at: pose(), by: Some(3) },
            CrateNews::Despawn { id: 2 },
            CrateNews::Rest { id: 1, at: pose(), t: 12.5 },
            CrateNews::Held { id: 1, by: Some(2), at: pose(), t: 13.0 },
            CrateNews::Held { id: 1, by: None, at: pose(), t: 14.0 },
            CrateNews::Refused { id: 1 },
        ];
        for n in news {
            assert_eq!(decode::<ToClient>(&encode(&ToClient::Crate(n.clone()))), Some(ToClient::Crate(n)));
        }
    }

    #[test]
    fn a_pose_that_cannot_be_is_not_sane() {
        assert!(pose().sane());
        for bad in [
            CratePose { pos: [f64::NAN, 0.0, 0.0], ..pose() },
            CratePose { vel: [1.0e6, 0.0, 0.0], ..pose() },
            CratePose { rot: [0.0; 4], ..pose() },
            CratePose { pos: [1.0e12, 0.0, 0.0], ..pose() },
        ] {
            assert!(!bad.sane(), "{bad:?}");
        }
    }

    #[test]
    fn crate_ids_in_datagrams_never_meet_loose_ids() {
        assert_eq!(unkey(key(9)), Some(9));
        assert_eq!(unkey(9), None, "a loose item's id is not a crate's");
        assert_ne!(key(1), 1);
    }
}
