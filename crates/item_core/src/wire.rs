//! The item messages as bytes (C2). They travel over a reliable, ordered channel
//! (`net_core::reliable`), so none is lost, doubled or overtaken; the encoding is compact JSON
//! (serde), which is easy to read in a capture and plenty for the few messages items send.
//! Loose items' poses while they move are not here: they are datagrams (`net_core::loose`).
use serde::{Deserialize, Serialize};

use crate::authority::{Action, Event};

/// Biggest message accepted on receipt (bytes); a late joiner's whole state comes as many events.
pub const MAX_MESSAGE: usize = 32 * 1024;

/// Client to host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToHost {
    Action(Action),
}

/// Host to client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToClient {
    Event(Event),
    /// The player's own vitals, as the host has them (#295); not an item event, but it travels
    /// the same reliable channel.
    Vitals(Vitals),
}

/// A player's vitals for their own display (#295). Shares are 0..1 of the maximum; `None` where
/// the affliction is not there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vitals {
    pub health: f64,
    /// Oxygen low's share (bad air), None when there is none.
    pub oxygen: Option<f64>,
    /// Burn's band: 1 (I), 2 (II) or 3 (III); None without burn.
    pub burn: Option<u8>,
    /// Radiation's share, 0 without.
    pub radiation: f64,
    /// Seconds until the respawn while the player is down; None while standing.
    pub down: Option<f64>,
}

pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    serde_json::to_vec(msg).expect("item messages are plain data")
}

/// None for anything that is not a message of this type or too long.
pub fn decode<T: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Option<T> {
    if bytes.len() > MAX_MESSAGE {
        return None;
    }
    serde_json::from_slice(bytes).ok()
}
