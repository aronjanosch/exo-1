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
    /// A request about the engine room (#296): the host decides.
    Room(RoomAsk),
}

/// Host to client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToClient {
    Event(Event),
    /// The player's own vitals, as the host has them (#295); not an item event, but it travels
    /// the same reliable channel.
    Vitals(Vitals),
    /// The engine room's shared state as the host has it (#296): every player gets it.
    Room(RoomNews),
}

/// The alarm level of the ship (#296). A dev toggle for now; the reactor drives it later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alarm {
    #[default]
    None,
    Warn,
    Danger,
}

impl Alarm {
    /// The dev toggle's next step: None, Warn, Danger, and None again.
    pub fn next(self) -> Alarm {
        match self {
            Alarm::None => Alarm::Warn,
            Alarm::Warn => Alarm::Danger,
            Alarm::Danger => Alarm::None,
        }
    }
}

/// A request to the host about the engine room (#296).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomAsk {
    /// Open the door if it is shut, shut it if it is open.
    ToggleDoor,
    /// The dev toggle: the next alarm level.
    CycleAlarm,
}

/// The engine room's state (#296): the door's target and the alarm level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomNews {
    pub door_open: bool,
    pub alarm: Alarm,
}

/// A player's vitals for their own display (#295). Shares are 0..1 of the maximum before any
/// headache; `None` where the affliction is not there.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vitals {
    /// Vitality's share of the maximum before any headache.
    pub health: f64,
    /// The maximum now as a share of that (a headache lowers it); 1 without.
    pub max_share: f64,
    /// Oxygen low's share (bad air), None when there is none.
    pub oxygen: Option<f64>,
    /// The burn's level, 1 to 3 by its band; 0 without burn (or only building up).
    pub burn_level: u8,
    /// The burning buildup, 0..1 (the hot plate fills it).
    pub buildup: f64,
    /// Whether the body is on fire now.
    pub burning: bool,
    /// Radiation's share, 0 without.
    pub radiation: f64,
    /// Radiation makes nausea (the HUD's SICK).
    pub sick: bool,
    /// The walk's share of the walking speed (nausea), and whether it may sprint.
    pub walk: f64,
    pub sprint: bool,
    /// Seconds until the respawn while the player is down; None while standing.
    pub down: Option<f64>,
    /// What each injury costs the health now, in vitality points: the burn, the oxygen low and
    /// the headache's part of the maximum (#295).
    pub burn_cost: f64,
    pub oxygen_cost: f64,
    pub headache_cost: f64,
}

impl Vitals {
    /// Calm and healthy: nothing to show but the health, walking at full speed.
    pub fn healthy() -> Vitals {
        Vitals { health: 1.0, max_share: 1.0, oxygen: None, burn_level: 0, buildup: 0.0, burning: false, radiation: 0.0, sick: false, walk: 1.0, sprint: true, down: None, burn_cost: 0.0, oxygen_cost: 0.0, headache_cost: 0.0 }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_news_and_asks_travel_as_they_are() {
        let news = RoomNews { door_open: true, alarm: Alarm::Danger };
        let back: RoomNews = match decode::<ToClient>(&encode(&ToClient::Room(news))).expect("decodes") {
            ToClient::Room(r) => r,
            _ => panic!("the room message"),
        };
        assert_eq!(back, news);
        let ask: RoomAsk = match decode::<ToHost>(&encode(&ToHost::Room(RoomAsk::CycleAlarm))).expect("decodes") {
            ToHost::Room(a) => a,
            _ => panic!("the room request"),
        };
        assert_eq!(ask, RoomAsk::CycleAlarm);
    }

    #[test]
    fn the_alarm_cycles_none_warn_danger() {
        assert_eq!(Alarm::None.next(), Alarm::Warn);
        assert_eq!(Alarm::Warn.next(), Alarm::Danger);
        assert_eq!(Alarm::Danger.next(), Alarm::None);
    }

    #[test]
    fn vitals_travel_as_they_are() {
        let v = Vitals { burn_level: 2, buildup: 0.4, burning: true, sick: true, walk: 0.7, sprint: false, max_share: 0.8, burn_cost: 12.5, oxygen_cost: 3.0, headache_cost: 20.0, ..Vitals::healthy() };
        let back: Vitals = decode(&encode(&ToClient::Vitals(v)).as_slice()).map(|m: ToClient| match m {
            ToClient::Vitals(v) => v,
            _ => panic!("the vitals message"),
        }).expect("decodes");
        assert_eq!(back, v);
    }
}
