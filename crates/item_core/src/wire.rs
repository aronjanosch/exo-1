//! The item messages as bytes (C2). They travel over a reliable, ordered channel
//! (`net_core::reliable`), so none is lost, doubled or overtaken; the encoding is compact JSON
//! (serde), which is easy to read in a capture and plenty for the few messages items send.
//! Loose items' poses while they move are not here: they are datagrams (`net_core::loose`).
use serde::{Deserialize, Serialize};

use crate::authority::{Action, Event};
use crate::PlayerId;

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
    /// A repair session at a machine (#299): the host decides what it does to the condition.
    Repair(RepairAsk),
    /// The treatment hold (#300): the player is treating this player (`Some`), or has stopped (`None`).
    Treat(Option<PlayerId>),
    /// The CPR hold (#300 polish): the player is pumping this player (`Some`), or has stopped (`None`).
    Rescue(Option<PlayerId>),
    /// The brake (#300): pull the rod with the most life into the sender's hands.
    Pull,
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
    /// The host ended this player's repair session (#299): the machine is full.
    RepairDone,
    /// The host refused this player's repair start (another player works there, the wrong tool,
    /// or the machine is full): the client's ring ends.
    RepairRefused,
    /// Every player's public status (#300), as the host has it: sent when one changes, and to a
    /// client that joins.
    Status(Vec<(PlayerId, PublicStatus)>),
}

/// What a player is doing that the others see (#300).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Doing {
    #[default]
    Nothing,
    /// Holding a treatment on this player (themselves included).
    Treating(PlayerId),
    /// A repair session at this machine.
    Repairing(RepairDevice),
    /// The extinguisher's spray is on.
    Spraying,
    /// Pumping this unconscious player (CPR, #300 polish).
    Reviving(PlayerId),
}

/// A player's status for the others (#300): burning, down (waiting for the respawn), unconscious
/// (vitality at zero or below, awake again above) and what they do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicStatus {
    pub burning: bool,
    pub down: bool,
    pub unconscious: bool,
    pub doing: Doing,
    /// The injuries the others see (#300): what a medic reads when they look at the player.
    pub hurt: Injuries,
}

/// A player's injuries as the others see them (#300): the health and the injuries that treatments
/// reduce, each a whole percent in steps of 5 so a change is a small message. The burn is its level
/// (0 without). Default: healthy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Injuries {
    /// Vitality's share of the maximum before a headache, 0..=100 (steps of 5).
    pub health: u8,
    /// The burn's level 1 to 3; 0 without burn (or only building up).
    pub burn_level: u8,
    /// Radiation's share, 0..=100 (steps of 5); 0 without.
    pub radiation: u8,
    /// Oxygen low's share (bad air), 0..=100 (steps of 5); 0 without.
    pub oxygen_low: u8,
}

impl Default for Injuries {
    fn default() -> Injuries {
        Injuries { health: 100, burn_level: 0, radiation: 0, oxygen_low: 0 }
    }
}

impl Injuries {
    /// The shares (0..1) rounded to steps of 5 per cent; the burn's level as it is.
    pub fn from_shares(health: f64, burn_level: u8, radiation: f64, oxygen_low: f64) -> Injuries {
        let step = |x: f64| ((x.clamp(0.0, 1.0) * 20.0).round() * 5.0) as u8;
        Injuries { health: step(health), burn_level, radiation: step(radiation), oxygen_low: step(oxygen_low) }
    }
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
    /// SCRAM: fission to 0 and the power off at once.
    Scram,
    /// The power switch: on starts the reactor again after a SCRAM.
    SetPower(bool),
    /// A consumer group's breaker on the junction box.
    SetBreaker(BreakerGroup, bool),
    /// The coolant pump's control: Off, Low or High.
    SetPumpMode(PumpMode),
    /// The temperature boost: up (true) or down (false), as the core takes it (#300).
    Boost(bool),
    /// First press of the core eject: the safety cover opens for `eject_arm_s` (the host's timer).
    ArmEject,
    /// Second press, inside the arm window: the core is thrown out (PULSAR's core eject, #300 r22).
    Eject,
    /// The emergency cooling (PULSAR's manual button, #300 r24): the tank pays, the extra cooling runs
    /// for its duration, then the cooldown. Refused when the host has no press to give.
    EmergencyCool,
    /// The AUX screen's switch for Life Support or Lights (`EngineRoom::set_aux_switch`, #300 r32).
    /// Engines is not aux-fed: the host ignores it.
    SetAuxSwitch(BreakerGroup, bool),
}

/// The coolant pump's mode (#300): off, a low or a high stream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PumpMode {
    #[default]
    Off,
    Low,
    High,
}

/// The junction box's consumer groups (the breakers on its screen).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakerGroup {
    Engines,
    LifeSupport,
    Lights,
}

impl BreakerGroup {
    /// Position in `ReactorNews::breakers`.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// A machine the repair minigame works on (#299).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepairDevice {
    Reactor,
    CoolantPump,
    JunctionBox,
}

/// The tool in hand for a repair (#299): the wrench for the reactor and the pump, the screwdriver
/// for the junction box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepairTool {
    Wrench,
    Screwdriver,
}

/// A repair session as the client asks for it (#299). The client runs the timing ring; the host
/// applies what the ring says and keeps the condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RepairAsk {
    /// A session starts on this machine with this tool.
    Start { device: RepairDevice, tool: RepairTool },
    /// LMB inside a zone: a chunk of condition.
    Hit,
    /// LMB outside every zone: a little condition lost.
    Miss,
    /// The session ends (done, looked away, walked off, switched slot, F or ESC).
    Stop,
}

/// The faults the dev console asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReactorFault {
    PumpBreak,
    LoadDrop,
    Surge,
    /// A fire at the reactor (#298).
    Fire,
    /// The coolant tank runs dry (the dev drain for the coolant lane, #300).
    DrainCoolant,
    /// The reactor's warm start (`EngineRoom::warm_start`, #300): settled at once, no supplies used.
    WarmStart,
    /// The aux reactor on or off (`EngineRoom::set_aux`, #300 r32): the host flips it.
    Aux,
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

/// A burning cell of the grid model (#300): its room's index, its index in the room's grid, and
/// its intensity in thousandths (0..=1000).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellNew {
    pub room: u32,
    pub cell: u32,
    pub heat: u16,
}

/// The fires and the sprays (#298), as the host has them. In the grid model (#300, `grid`) the
/// burning cells are in `cells`, and `fires` lists each one as a fire of a cell's width, for the
/// sprays' aim and the steam; the flames are not drawn then.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FireNews {
    pub fires: Vec<FireNew>,
    pub sprays: Vec<SprayNew>,
    pub grid: bool,
    pub cells: Vec<CellNew>,
}

/// A part's condition for the repair ring: its share of full condition (0..1) and whether it works.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartNews {
    pub share: f64,
    pub working: bool,
}

/// The fuel tubes on the readout (#300): one per slot, `FUEL_TUBES` at most.
pub const FUEL_TUBES: usize = 4;

/// The kind of a fuel rod (`reactor.json` `rods`: standard, hot, spicy), as the readout colours it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FuelKind {
    Standard,
    Hot,
    Spicy,
}

/// One fuel tube: the rod in a slot (its kind and life, 0..100), None for an empty slot (#300).
#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FuelTube {
    pub rod: Option<FuelKind>,
    pub life: f64,
}

/// The reactor's snapshot (#297): what the screens show, as the host has it. Temperature,
/// fission and turbine are 0..100; power is kW; the bands are (low, high) on the temperature
/// gauge. The screens show little: no condition values, the pump gives its flow only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReactorNews {
    pub power_on: bool,
    pub manual: bool,
    /// The core melted: the readout shows MELTDOWN.
    pub melted: bool,
    /// Core stability in whole percent, 100 when cold and off, 0 when melted or ejected (#300 r29).
    pub stability: u8,
    pub temp: f64,
    /// The targets as set (Manual) or as Auto has them.
    pub set_fission: f64,
    pub set_turbine: f64,
    /// The actual fission rate and turbine output, 0..100: they follow the set points with inertia
    /// (the reactor hum follows the actual fission, #300 r25).
    pub fission: f64,
    pub turbine: f64,
    pub output_kw: f64,
    pub load_kw: f64,
    pub optimal: [f64; 2],
    pub allowed: [f64; 2],
    /// The fission and turbine bands (optimal, allowed; lo and hi) of the readout's sliders (#300 r30).
    pub fission_optimal: [f64; 2],
    pub fission_allowed: [f64; 2],
    pub turbine_optimal: [f64; 2],
    pub turbine_allowed: [f64; 2],
    /// The machines have power: the junction box works and the reactor delivers.
    pub powered: bool,
    /// The breakers, by `BreakerGroup::index`: engines, life support, lights.
    pub breakers: [bool; 3],
    /// The aux reactor is on (`EngineRoom::aux_enabled`, #300 r32).
    pub aux_on: bool,
    /// The aux switches of Life Support and Lights (on when the aux reactor feeds them).
    pub aux_life_support: bool,
    pub aux_lights: bool,
    /// kW the main reactor gets from the aux output the aux groups leave (`EngineRoom::aux_boost_kw`).
    pub aux_boost_kw: f64,
    pub pump_on: bool,
    /// The pump's mode as set (`pump_on` is `mode != Off`).
    pub pump_mode: PumpMode,
    /// The pump's speed, 0..1: it follows the mode with inertia (the impeller turns with it).
    pub pump_speed: f64,
    /// Coolant flow, 0..1.
    pub flow: f64,
    /// Coolant in the pump's tank, 0..100 %.
    pub coolant: f64,
    /// The tank is below its LOW line (`RoomTuning::coolant_low`).
    pub coolant_low: bool,
    /// Overvoltage at the junction box now.
    pub overvolt: bool,
    /// The engine room's air, 0..1 (the host rounds it to whole percent).
    pub oxygen: f64,
    /// The meltdown blasts the host has applied so far (`ReactorCounts::blasts`). A process that
    /// sees it go up shows the blast (the flash, the notice) once; the host applies the effects (#300).
    pub blasts: u32,
    /// The fuel tubes, one per slot (the first `FUEL_TUBES`): the rod in it, None when empty (#300).
    pub fuel: [FuelTube; FUEL_TUBES],
    pub reactor: PartNews,
    pub pump: PartNews,
    pub junction: PartNews,
    /// The warning lamps that are on, by `Lamp::index` (the status board, #300). Steady here: the
    /// blink is the screen's.
    pub lamps: [bool; Lamp::COUNT],
    /// The temperature boost, degrees (-25..25 at the shipped tuning; 0 when none).
    pub boost: f64,
    /// A boost press would pass now: the core's rule with the power on and the reactor working.
    pub boost_ok: bool,
    /// Seconds to the implosion at the current rate, while the meltdown timer runs; None otherwise.
    pub meltdown_eta_s: Option<f64>,
    /// The status board's graph (#300, Barotrauma's reactor graph): `graph_points` valid points
    /// of output and load in kW, newest first; 0 means no curves (the panel draws none).
    pub graph_points: u8,
    pub graph_output: [u16; GRAPH_POINTS],
    pub graph_load: [u16; GRAPH_POINTS],
    /// The core was ejected: the readout shows NO CORE, the rods are gone (#300 r22).
    pub ejected: bool,
    /// The EJECT safety cover is open (ARM EJECT pressed, the window still running).
    pub eject_armed: bool,
    /// The core ejects the host has applied so far (`ReactorCounts::ejects`). A process that sees
    /// it go up shows the CORE EJECTED notice once, as the blasts do.
    pub ejects: u32,
    /// The emergency cooling runs now (#300 r24).
    pub ecool_active: bool,
    /// Seconds until the emergency cooling can be pressed again (0 when it can).
    pub ecool_ready_in_s: f64,
    /// A press would be taken now: the pump's E-COOL button is on.
    pub ecool_ok: bool,
    /// The emergency coolings the host has applied so far (`ReactorCounts::ecools`). A process that
    /// sees it go up shows the EMERGENCY COOLING notice and the steam once, as the ejects do.
    pub ecools: u32,
}

/// The most points the reactor graph carries (the shipped `graph_points`, 25). A fixed size keeps
/// `ReactorNews` `Copy`.
pub const GRAPH_POINTS: usize = 25;

/// A warning lamp of the reactor's status board (#300), in the order the panel shows them. The
/// conditions are the core's `Warnings`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lamp {
    Hot,
    CritHeat,
    Cold,
    LowTemp,
    HighOut,
    CritOut,
    LowOut,
    FuelLow,
    NoFuel,
    Meltdown,
    Scram,
}

impl Lamp {
    pub const COUNT: usize = 11;

    /// Position in `ReactorNews::lamps`.
    pub fn index(self) -> usize {
        self as usize
    }
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
    /// Unconscious: vitality at zero or below (not dead). Controls frozen, lying down for the others.
    pub unconscious: bool,
    /// What each injury costs the health now, in vitality points: the burn, the oxygen low and
    /// the headache's part of the maximum (#295).
    pub burn_cost: f64,
    pub oxygen_cost: f64,
    pub headache_cost: f64,
}

impl Vitals {
    /// Calm and healthy: nothing to show but the health, walking at full speed.
    pub fn healthy() -> Vitals {
        Vitals { health: 1.0, max_share: 1.0, oxygen: None, burn_level: 0, buildup: 0.0, burning: false, radiation: 0.0, sick: false, walk: 1.0, sprint: true, down: None, unconscious: false, burn_cost: 0.0, oxygen_cost: 0.0, headache_cost: 0.0 }
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
        let v = Vitals { burn_level: 2, buildup: 0.4, burning: true, sick: true, walk: 0.7, sprint: false, max_share: 0.8, unconscious: true, burn_cost: 12.5, oxygen_cost: 3.0, headache_cost: 20.0, ..Vitals::healthy() };
        let back: Vitals = decode(&encode(&ToClient::Vitals(v)).as_slice()).map(|m: ToClient| match m {
            ToClient::Vitals(v) => v,
            _ => panic!("the vitals message"),
        }).expect("decodes");
        assert_eq!(back, v);
    }

    #[test]
    fn reactor_news_travels_as_it_is() {
        let news = ReactorNews {
            reactor: PartNews { share: 0.9, working: true },
            pump: PartNews { share: 0.4, working: true },
            junction: PartNews { share: 1.0, working: true },
            aux_on: true,
            aux_life_support: false,
            aux_lights: true,
            aux_boost_kw: 250.0,
            power_on: true,
            manual: true,
            melted: false,
            stability: 73,
            temp: 61.25,
            set_fission: 40.0,
            set_turbine: 48.0,
            fission: 38.0,
            turbine: 50.0,
            output_kw: 500.0,
            load_kw: 500.0,
            optimal: [35.0, 65.0],
            allowed: [20.0, 80.0],
            fission_optimal: [20.0, 60.0],
            fission_allowed: [10.0, 80.0],
            turbine_optimal: [40.0, 56.0],
            turbine_allowed: [30.0, 66.0],
            powered: true,
            breakers: [true, false, true],
            pump_on: true,
            pump_mode: PumpMode::High,
            pump_speed: 0.5,
            flow: 0.4,
            coolant: 60.0,
            coolant_low: false,
            overvolt: true,
            oxygen: 0.21,
            blasts: 2,
            fuel: [FuelTube { rod: Some(FuelKind::Hot), life: 80.0 }, FuelTube { rod: None, life: 0.0 }, FuelTube::default(), FuelTube { rod: Some(FuelKind::Spicy), life: 12.5 }],
            lamps: [true, false, false, false, false, false, false, true, false, false, true],
            boost: -12.5,
            boost_ok: true,
            meltdown_eta_s: Some(42.0),
            graph_points: 3,
            graph_output: [7; GRAPH_POINTS],
            graph_load: [5; GRAPH_POINTS],
            ecool_active: true,
            ecool_ready_in_s: 42.5,
            ecool_ok: false,
            ecools: 3,
            ejected: true,
            eject_armed: true,
            ejects: 3,
        };
        let back = match decode::<ToClient>(&encode(&ToClient::Reactor(news))).expect("decodes") {
            ToClient::Reactor(r) => r,
            _ => panic!("the reactor message"),
        };
        assert_eq!(back, news);
    }

    #[test]
    fn reactor_asks_travel_as_they_are() {
        let asks = [
            ReactorAsk::SetManual(true),
            ReactorAsk::SetFission(12.5),
            ReactorAsk::SetTurbine(0.0),
            ReactorAsk::Trigger(ReactorFault::LoadDrop),
            ReactorAsk::Reset,
            ReactorAsk::Scram,
            ReactorAsk::SetPower(false),
            ReactorAsk::SetBreaker(BreakerGroup::Lights, false),
            ReactorAsk::SetBreaker(BreakerGroup::Engines, true),
            ReactorAsk::SetPumpMode(PumpMode::Low),
            ReactorAsk::ArmEject,
            ReactorAsk::Eject,
        ];
        for ask in asks {
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
            grid: true,
            cells: vec![CellNew { room: 1, cell: 42, heat: 730 }],
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
    fn injuries_come_in_steps_of_five_per_cent() {
        let i = Injuries::from_shares(0.512, 2, 0.4, 0.03);
        assert_eq!((i.health, i.burn_level, i.radiation, i.oxygen_low), (50, 2, 40, 5));
        assert_eq!(Injuries::from_shares(1.7, 0, -0.2, 0.0), Injuries { health: 100, burn_level: 0, radiation: 0, oxygen_low: 0 });
        assert_eq!(Injuries::default(), Injuries::from_shares(1.0, 0, 0.0, 0.0), "healthy by default");
    }

    #[test]
    fn public_status_travels_as_it_is() {
        let status = vec![
            (1, PublicStatus { burning: true, down: false, unconscious: false, doing: Doing::Treating(2), hurt: Injuries { health: 35, burn_level: 2, radiation: 40, oxygen_low: 0 } }),
            (2, PublicStatus { burning: false, down: true, unconscious: false, doing: Doing::Nothing, hurt: Injuries::default() }),
            (3, PublicStatus { burning: false, down: false, unconscious: false, doing: Doing::Repairing(RepairDevice::CoolantPump), hurt: Injuries::default() }),
            (4, PublicStatus { burning: false, down: false, unconscious: false, doing: Doing::Spraying, hurt: Injuries::default() }),
            (5, PublicStatus { burning: false, down: false, unconscious: true, doing: Doing::Nothing, hurt: Injuries::from_shares(0.0, 0, 0.0, 0.5) }),
        ];
        let back = match decode::<ToClient>(&encode(&ToClient::Status(status.clone()))).expect("decodes") {
            ToClient::Status(s) => s,
            _ => panic!("the status message"),
        };
        assert_eq!(back, status);
        let treat = match decode::<ToHost>(&encode(&ToHost::Treat(Some(3)))).expect("decodes") {
            ToHost::Treat(t) => t,
            _ => panic!("the treat message"),
        };
        assert_eq!(treat, Some(3));
        assert!(decode::<ToHost>(&encode(&ToHost::Treat(None))).is_some_and(|m| matches!(m, ToHost::Treat(None))));
    }

    #[test]
    fn repair_done_travels() {
        assert!(decode::<ToClient>(&encode(&ToClient::RepairDone)).is_some_and(|m| matches!(m, ToClient::RepairDone)));
    }

    #[test]
    fn repair_refused_travels() {
        assert!(decode::<ToClient>(&encode(&ToClient::RepairRefused)).is_some_and(|m| matches!(m, ToClient::RepairRefused)));
    }

    #[test]
    fn repair_asks_travel_as_they_are() {
        for ask in [RepairAsk::Start { device: RepairDevice::JunctionBox, tool: RepairTool::Screwdriver }, RepairAsk::Start { device: RepairDevice::CoolantPump, tool: RepairTool::Wrench }, RepairAsk::Hit, RepairAsk::Miss, RepairAsk::Stop] {
            let back = match decode::<ToHost>(&encode(&ToHost::Repair(ask))).expect("decodes") {
                ToHost::Repair(a) => a,
                _ => panic!("the repair request"),
            };
            assert_eq!(back, ask);
        }
    }

    #[test]
    fn a_reactor_message_is_not_a_room_message() {
        // The two enums keep their variants apart on the wire.
        assert!(decode::<ToHost>(&encode(&ToHost::Room(RoomAsk::CycleAlarm))).is_some_and(|m| matches!(m, ToHost::Room(_))));
        assert!(decode::<ToClient>(&encode(&ToClient::Reactor(ReactorNews::default()))).is_some_and(|m| matches!(m, ToClient::Reactor(_))));
    }
}
