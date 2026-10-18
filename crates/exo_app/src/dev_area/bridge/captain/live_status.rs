//! The captain's view of the real ship (#288, phase 3, ported onto our `ReactorNews` in r44): the
//! reactor snapshot the host sends (`Items::reactor`) laid into `crew_core::ShipStatus`, so the ship
//! state display, the overview's alarms and the alert follow the engine room.
//!
//! - Reactor: heat and its band from the reactor's own bands, fill, mode, output and load.
//! - System bars: engineering from the pump's and the reactor's condition, with the engine room's
//!   fire; weapons and life support from their breaker group's share of its demand (`factor`).
//!   Science has no source in the engine room yet: its bar shows "(-)".
//! - Shields and hull from `ReactorNews::shield` and the wall sections (`display_core::captain`).
//! - Rooms: the engine room, the corridor and the bridge, with oxygen, fire and breach. Crew counts:
//!   no source yet, so 0.
//! - Oxygen: the mean over the three rooms; its change per second is smoothed (as in the phase-3 glue).
//!
//! `crew_core` knows nothing of `item_core`; the mapping lives here.
use super::*;
use crew_core::orders::OrderKind;
use crew_core::status::{ReactorMode, ReactorStatus, RoomStatus, ShipSystem, ShipStatus, SystemBar, HeatBand, Gauge, Oxygen, SYSTEM_MAX};
use crew_core::{DeviceId, RoomId, StationId, Target};
use display_core::captain::{self, Threat};
use item_core::wire::{BreakerGroup, Lamp, PartNews, ReactorNews, RoomAlarmKind};

/// Room ids of the engine room, the corridor and the bridge (the three rooms of `ReactorNews::rooms`).
pub const ENGINE_ROOM: &str = "engine_room";
pub const CORRIDOR: &str = "corridor";
pub const BRIDGE: &str = "bridge";
/// Device ids of the devices `ReactorNews` has (the parts, `display_core::captain::SYSTEM_NAMES`).
pub const REACTOR: &str = "reactor";
pub const PUMP: &str = "pump";
/// The three junction boxes (#322): JB-1 engine room, JB-2 corridor, JB-3 turret room.
pub const JUNCTIONS: [&str; 3] = ["junction_1", "junction_2", "junction_3"];
pub const OXYGEN_GENERATOR: &str = "oxygen_generator";
/// Station id of the reactor's main screen (`crate::dev_area::engineering::reactor_room::MAIN_ID`), the one an operate order points at.
pub const REACTOR_STATION: &str = "reactor_main";
/// Smoothing of the oxygen change per second (s).
const O2_SMOOTH_S: f64 = 1.0;
/// Air in the engine room below this share runs the generator's repair order (a start value).
const AIR_LOW_FOR_REPAIR: f64 = 0.9;
/// A device below this share is WORN on the overview's alarm list. TODO(initiator): start value.
const WORN_BELOW: f64 = 0.5;

/// The ship as the captain sees it this tick.
#[derive(Resource, Default, Debug, Clone)]
pub struct LiveShip {
    pub status: Option<ShipStatus>,
    /// The ship's oxygen last tick and its smoothed change per second.
    prev_o2: Option<f64>,
    pub o2_delta: f64,
}

impl LiveShip {
    /// The status, or a cold ship before the first reactor news.
    pub fn get(&self) -> ShipStatus {
        self.status.clone().unwrap_or_else(|| status(&ReactorNews::default(), 0.0))
    }
}

/// A breaker group's share of its demand, 0..1: 0 when the breaker is off, 1 when nothing is asked.
pub fn factor(r: &ReactorNews, g: BreakerGroup) -> f64 {
    let i = g.index();
    if !r.breakers[i] {
        0.0
    } else if r.demand_kw[i] <= 0.0 {
        1.0
    } else {
        (r.delivered_kw[i] / r.demand_kw[i]).clamp(0.0, 1.0)
    }
}

fn bar(c: f64) -> Option<u8> {
    Some((c.clamp(0.0, 1.0) * SYSTEM_MAX as f64).round() as u8)
}

/// The mean oxygen share (0..1) over the ship's rooms.
fn mean_oxygen(r: &ReactorNews) -> f64 {
    r.rooms.iter().map(|room| room.o2).sum::<f64>() / r.rooms.len() as f64
}

/// The heat band from the reactor's own bands (`ReactorNews::optimal`, `allowed`).
pub fn heat_band(r: &ReactorNews) -> HeatBand {
    if r.heat >= r.allowed[1] {
        HeatBand::Critical
    } else if r.heat >= r.optimal[1] {
        HeatBand::Hot
    } else if r.heat < r.optimal[0] {
        HeatBand::Cold
    } else {
        HeatBand::Normal
    }
}

/// The ship snapshot from one reactor news. `o2_delta` is the oxygen's change per second, smoothed by
/// the caller (`update`).
pub fn status(r: &ReactorNews, o2_delta: f64) -> ShipStatus {
    let view = captain::captain_view(r);
    let band = heat_band(r);
    let condition = |p: &PartNews| if p.working { p.share } else { 0.0 };
    let engineering = condition(&r.pump).min(condition(&r.reactor));
    let fill: f64 = r.canisters.iter().map(|t| t.life / 100.0).sum::<f64>() / r.canisters.len() as f64;
    let mode = if r.imploded || r.ejected || r.lamps[Lamp::Scram.index()] {
        ReactorMode::Shutdown
    } else if !r.power_on {
        ReactorMode::Off
    } else if matches!(band, HeatBand::Hot | HeatBand::Critical) {
        ReactorMode::Overheating
    } else {
        ReactorMode::Running
    };
    let systems = [
        SystemBar { system: ShipSystem::Engineering, condition: bar(engineering), fire: r.rooms[0].fire },
        SystemBar { system: ShipSystem::Weapons, condition: bar(factor(r, BreakerGroup::Weapons)), fire: false },
        SystemBar { system: ShipSystem::LifeSupport, condition: bar(factor(r, BreakerGroup::LifeSupport) * (r.rooms[0].o2 / AIR_LOW_FOR_REPAIR).min(1.0)), fire: false },
        SystemBar { system: ShipSystem::Science, condition: None, fire: false },
    ];
    let room = |id: &str, name: &str, i: usize, system: Option<ShipSystem>, condition: f64| RoomStatus {
        id: RoomId::new(id),
        name: gameplay_core::TextKey::new(format!("room.{name}")),
        system,
        condition,
        oxygen: r.rooms[i].o2,
        fire: r.rooms[i].fire,
        breach: r.alarms[i].kind == RoomAlarmKind::Breach,
        crew: 0,
    };
    ShipStatus {
        systems,
        shields: Gauge { value: r.shield * 100.0, max: 100.0 },
        hull: Gauge { value: view.hull * 100.0, max: 100.0 },
        oxygen: Oxygen { level: mean_oxygen(r), delta_per_s: o2_delta },
        reactor: ReactorStatus { heat: r.heat / 100.0, band, fill, mode, output_kw: Some(r.output_kw), load_kw: Some(r.load_kw) },
        // TODO(initiator): the hull share stands in for the room condition; the news has none yet.
        rooms: vec![
            room(ENGINE_ROOM, ENGINE_ROOM, 0, Some(ShipSystem::Engineering), engineering),
            room(CORRIDOR, CORRIDOR, 1, None, view.hull),
            room(BRIDGE, BRIDGE, 2, None, view.hull),
        ],
        clock_left_s: None,
        mission: None,
    }
}

/// Whether the ship is in trouble, as the captain's threat word says (not NONE). It raises the automatic
/// yellow.
pub fn threat(r: &ReactorNews) -> bool {
    captain::captain_view(r).threat != Threat::None
}

/// One active alarm on the captain's overview: what, where, and the order it suggests.
#[derive(Clone, Debug, PartialEq)]
pub struct AlarmRow {
    pub text: String,
    pub room: &'static str,
    pub order: Option<(OrderKind, Target)>,
}

/// The ship's alarms: fires, breaches, thin air, the reactor's lamps, broken or worn devices. Most
/// urgent first, in the order of the lists below.
pub fn alarms(r: &ReactorNews) -> Vec<AlarmRow> {
    let mut out = Vec::new();
    for (i, id) in [ENGINE_ROOM, CORRIDOR].into_iter().enumerate() {
        if r.rooms[i].fire {
            out.push(AlarmRow { text: "FIRE".into(), room: id, order: Some((OrderKind::Extinguish, Target::Room(RoomId::new(id)))) });
        }
    }
    for (i, id) in [ENGINE_ROOM, CORRIDOR].into_iter().enumerate() {
        match r.alarms[i].kind {
            RoomAlarmKind::Breach => out.push(AlarmRow { text: "BREACH".into(), room: id, order: Some((OrderKind::FixBreach, Target::Room(RoomId::new(id)))) }),
            RoomAlarmKind::LowO2 => out.push(AlarmRow { text: format!("AIR LOW {:.0} %", r.rooms[i].o2 * 100.0), room: id, order: Some((OrderKind::Repair, Target::Device(DeviceId::new(OXYGEN_GENERATOR)))) }),
            RoomAlarmKind::Fire | RoomAlarmKind::Clear => {}
        }
    }
    let console = || Some((OrderKind::Operate, Target::Station(StationId::new(REACTOR_STATION))));
    if r.lamps[Lamp::CritHeat.index()] {
        out.push(AlarmRow { text: format!("REACTOR CRIT {:.0}", r.heat), room: ENGINE_ROOM, order: console() });
    } else if r.lamps[Lamp::Hot.index()] {
        out.push(AlarmRow { text: format!("REACTOR HOT {:.0}", r.heat), room: ENGINE_ROOM, order: console() });
    }
    if r.lamps[Lamp::Scram.index()] {
        out.push(AlarmRow { text: "SCRAM".into(), room: ENGINE_ROOM, order: console() });
    }
    if r.lamps[Lamp::FillLow.index()] {
        out.push(AlarmRow { text: "FILL LOW".into(), room: ENGINE_ROOM, order: console() });
    }
    let devices = [
        (r.reactor, REACTOR, "REACTOR", ENGINE_ROOM),
        (r.pump, PUMP, "PUMP", ENGINE_ROOM),
        (r.junctions[0], JUNCTIONS[0], "JB-1", ENGINE_ROOM),
        (r.junctions[1], JUNCTIONS[1], "JB-2", CORRIDOR),
        (r.junctions[2], JUNCTIONS[2], "JB-3", CORRIDOR),
        (r.oxygen_generator, OXYGEN_GENERATOR, "O2 GEN", ENGINE_ROOM),
    ];
    for (p, id, name, room) in devices {
        if !p.working || p.share < WORN_BELOW {
            let state = if p.working { "WORN" } else { "BROKEN" };
            out.push(AlarmRow { text: format!("{name} {state} {:.0} %", p.share * 100.0), room, order: Some((OrderKind::Repair, Target::Device(DeviceId::new(id)))) });
        }
    }
    out
}

/// Each tick: the live status from the reactor news (the oxygen's change smoothed over a second).
pub fn update(time: Res<Time>, items: Res<crate::items::Items>, mut live: ResMut<LiveShip>) {
    let Some(r) = items.reactor.as_ref() else { return };
    let dt = time.delta_secs_f64();
    let o2 = mean_oxygen(r);
    if let Some(prev) = live.prev_o2
        && dt > 0.0
    {
        let k = (dt / O2_SMOOTH_S).min(1.0);
        live.o2_delta += ((o2 - prev) / dt - live.o2_delta) * k;
    }
    live.prev_o2 = Some(o2);
    live.status = Some(status(r, live.o2_delta));
}

#[cfg(test)]
mod tests {
    use super::*;
    use item_core::wire::{AlarmNews, CanisterSlot};

    /// A ship with nothing wrong: whole, full shield, breakers on and asking nothing, air full.
    fn healthy() -> ReactorNews {
        let part = PartNews { share: 1.0, working: true };
        let mut r = ReactorNews { reactor: part, pump: part, junctions: [part; 3], oxygen_generator: part, shield: 1.0, power_on: true, ..ReactorNews::default() };
        r.breakers = [true; item_core::wire::BREAKER_GROUPS];
        r.canisters = [CanisterSlot { canister: None, life: 80.0 }; item_core::wire::CANISTER_SLOTS];
        r.optimal = [20.0, 60.0];
        r.allowed = [10.0, 80.0];
        for room in r.rooms.iter_mut() {
            room.o2 = 1.0;
        }
        r
    }

    #[test]
    fn a_sound_ship_reads_whole_and_science_has_no_source() {
        let s = status(&healthy(), 0.0);
        assert_eq!(s.system(ShipSystem::Engineering).condition, Some(SYSTEM_MAX));
        assert_eq!(s.system(ShipSystem::Science).condition, None, "no source yet: (-)");
        assert_eq!(s.reactor.mode, ReactorMode::Running);
        assert!((s.hull.fraction() - 1.0).abs() < 1e-12);
        assert!(alarms(&healthy()).is_empty());
        assert!(!threat(&healthy()));
    }

    #[test]
    fn a_fire_marks_engineering_and_orders_an_extinguish() {
        let mut r = healthy();
        r.rooms[0].fire = true;
        let s = status(&r, 0.0);
        assert!(s.system(ShipSystem::Engineering).fire);
        assert!(s.room(&RoomId::new(ENGINE_ROOM)).unwrap().fire);
        let a = alarms(&r);
        assert_eq!(a[0].text, "FIRE");
        assert_eq!(a[0].order, Some((OrderKind::Extinguish, Target::Room(RoomId::new(ENGINE_ROOM)))));
        assert!(threat(&r));
    }

    #[test]
    fn a_broken_pump_drags_engineering_down_and_orders_a_repair() {
        let mut r = healthy();
        r.pump = PartNews { share: 0.0, working: false };
        assert_eq!(status(&r, 0.0).system(ShipSystem::Engineering).condition, Some(0));
        let a = alarms(&r);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].text, "PUMP BROKEN 0 %");
        assert_eq!(a[0].order, Some((OrderKind::Repair, Target::Device(DeviceId::new(PUMP)))));
    }

    #[test]
    fn a_switched_off_breaker_leaves_its_system_at_zero() {
        let mut r = healthy();
        r.breakers[BreakerGroup::Weapons.index()] = false;
        assert_eq!(status(&r, 0.0).system(ShipSystem::Weapons).condition, Some(0));
    }

    #[test]
    fn a_scram_or_a_breach_shows_in_the_mode_and_the_alarm_list() {
        let mut r = healthy();
        r.lamps[Lamp::Scram.index()] = true;
        assert_eq!(status(&r, 0.0).reactor.mode, ReactorMode::Shutdown);
        assert!(alarms(&r).iter().any(|a| a.text == "SCRAM"));

        let mut b = healthy();
        b.alarms[1] = AlarmNews { kind: RoomAlarmKind::Breach, sounding: true, silenced: false };
        assert!(status(&b, 0.0).room(&RoomId::new(CORRIDOR)).unwrap().breach);
        assert!(alarms(&b).iter().any(|a| a.text == "BREACH" && a.room == CORRIDOR));
    }
}
