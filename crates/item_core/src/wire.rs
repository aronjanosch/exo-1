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
    /// A request about the reactor (#297): the host decides.
    Reactor(ReactorAsk),
    /// The extinguisher's spray, aimed while LMB is held (#298): the host decides.
    Spray(SprayAsk),
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
    /// The reactor's snapshot (#297), about 4 times a second while it changes.
    Reactor(ReactorNews),
    /// The fires and the sprays as the host has them (#298), about 4 times a second while they change.
    Fire(FireNews),
}

/// The alarm level of the ship (#296). A dev toggle for now; the reactor drives it later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

/// A request to the host about the reactor (#297). Set fission and set turbine only count on
/// Manual; a fault and a reset are the dev console's.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ReactorAsk {
    /// Manual when true, Auto when false.
    SetManual(bool),
    /// Fission target, 0..100.
    SetFission(f64),
    /// Turbine target, 0..100.
    SetTurbine(f64),
    /// A dev fault (`engine_room::Fault` on the host).
    Trigger(ReactorFault),
    /// A fresh engine room: cold, the start rods, Auto.
    Reset,
}

/// The faults the dev console asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReactorFault {
    PumpBreak,
    LoadDrop,
    Surge,
    /// A fire at the reactor (#298).
    Fire,
}

/// The extinguisher's spray, as its holder asks for it (#298).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum SprayAsk {
    /// Spraying along this aim now: the nozzle's point and the look direction (world metres, the
    /// direction a unit vector). Sent when the aim moves, about 10 times a second at most.
    Aim { eye: [f64; 3], dir: [f64; 3] },
    /// LMB let go, or the extinguisher put down.
    Stop,
}

/// A fire as the host has it (#298): its id, its centre on the engine room's floor (area metres)
/// and its diameter (m).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FireNew {
    pub id: u64,
    pub pos: [f64; 2],
    pub size: f64,
}

/// A spray in progress: the nozzle's point and the aim (world metres, unit direction).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SprayNew {
    pub eye: [f64; 3],
    pub dir: [f64; 3],
}

/// The fires and the sprays (#298), as the host has them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FireNews {
    pub fires: Vec<FireNew>,
    pub sprays: Vec<SprayNew>,
}

/// A part's condition for the readout: its share of full condition (0..1) and whether it works.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartNews {
    pub share: f64,
    pub working: bool,
}

/// The reactor's snapshot (#297): what the readout shows, as the host has it. Temperature,
/// fission and turbine are 0..100; power is kW; the bands are (low, high) on the temperature
/// gauge; the timers are 0..1 of their way to a fire or a shutdown (meltdown without the safety).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReactorNews {
    pub power_on: bool,
    pub manual: bool,
    pub temp: f64,
    pub fission: f64,
    pub turbine: f64,
    /// The targets as set (Manual) or as Auto has them.
    pub set_fission: f64,
    pub set_turbine: f64,
    pub output_kw: f64,
    pub load_kw: f64,
    pub optimal: [f64; 2],
    pub allowed: [f64; 2],
    pub pump: PartNews,
    pub junction: PartNews,
    pub alarm: Alarm,
    pub fire_timer: f64,
    pub meltdown_timer: f64,
    /// Rods not spent.
    pub rods: u8,
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

    #[test]
    fn reactor_news_travels_as_it_is() {
        let news = ReactorNews {
            power_on: true,
            manual: true,
            temp: 61.25,
            fission: 43.75,
            turbine: 50.0,
            set_fission: 40.0,
            set_turbine: 48.0,
            output_kw: 500.0,
            load_kw: 500.0,
            optimal: [35.0, 65.0],
            allowed: [20.0, 80.0],
            pump: PartNews { share: 0.4, working: true },
            junction: PartNews { share: 1.0, working: true },
            alarm: Alarm::Warn,
            fire_timer: 0.5,
            meltdown_timer: 0.0,
            rods: 2,
        };
        let back = match decode::<ToClient>(&encode(&ToClient::Reactor(news))).expect("decodes") {
            ToClient::Reactor(r) => r,
            _ => panic!("the reactor message"),
        };
        assert_eq!(back, news);
    }

    #[test]
    fn reactor_asks_travel_as_they_are() {
        for ask in [ReactorAsk::SetManual(true), ReactorAsk::SetFission(12.5), ReactorAsk::SetTurbine(0.0), ReactorAsk::Trigger(ReactorFault::LoadDrop), ReactorAsk::Reset] {
            let back = match decode::<ToHost>(&encode(&ToHost::Reactor(ask))).expect("decodes") {
                ToHost::Reactor(a) => a,
                _ => panic!("the reactor request"),
            };
            assert_eq!(back, ask);
        }
    }

    #[test]
    fn fires_and_sprays_travel_as_they_are() {
        let news = FireNews {
            fires: vec![FireNew { id: 7, pos: [-14.0, -15.25], size: 0.75 }, FireNew { id: 9, pos: [-12.5, -16.0], size: 0.1 }],
            sprays: vec![SprayNew { eye: [1.0, 2.0, 3.0], dir: [0.0, 0.0, -1.0] }],
        };
        let back = match decode::<ToClient>(&encode(&ToClient::Fire(news.clone()))).expect("decodes") {
            ToClient::Fire(f) => f,
            _ => panic!("the fire message"),
        };
        assert_eq!(back, news);
        let empty = FireNews::default();
        assert_eq!(decode::<ToClient>(&encode(&ToClient::Fire(empty.clone()))).is_some_and(|m| matches!(m, ToClient::Fire(f) if f == empty)), true);
    }

    #[test]
    fn spray_asks_travel_as_they_are() {
        for ask in [SprayAsk::Aim { eye: [0.5, 1.6, -2.0], dir: [0.6, -0.8, 0.0] }, SprayAsk::Stop] {
            let back = match decode::<ToHost>(&encode(&ToHost::Spray(ask))).expect("decodes") {
                ToHost::Spray(a) => a,
                _ => panic!("the spray request"),
            };
            assert_eq!(back, ask);
        }
        let fault = match decode::<ToHost>(&encode(&ToHost::Reactor(ReactorAsk::Trigger(ReactorFault::Fire)))).expect("decodes") {
            ToHost::Reactor(ReactorAsk::Trigger(f)) => f,
            _ => panic!("the fault"),
        };
        assert_eq!(fault, ReactorFault::Fire);
    }

    #[test]
    fn a_reactor_message_is_not_a_room_message() {
        // The two enums keep their variants apart on the wire.
        assert!(decode::<ToHost>(&encode(&ToHost::Room(RoomAsk::CycleAlarm))).is_some_and(|m| matches!(m, ToHost::Room(_))));
        assert!(decode::<ToClient>(&encode(&ToClient::Reactor(ReactorNews::default()))).is_some_and(|m| matches!(m, ToClient::Reactor(_))));
    }
}
