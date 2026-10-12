//! The captain's view of the real ship (#288, phase 3): the engineer corner's `Engineering`
//! (reactor, power grid, pump, fire deck) laid over the placeholder ship, so the ship state
//! display, the alert and the order targets follow what really happens in the dev area.
//!
//! - Reactor: temperature and its band from the reactor's own bands, fuel, mode, output and load.
//! - System bars: engineering from the pump's and the reactor's condition, weapons from the power
//!   grid's weapons share, life support from its share and the engine room's oxygen; science keeps
//!   the placeholder (no real source yet). Shields: the placeholder value times the shields' share.
//! - Rooms: the engine room and the side room behind the door are real (oxygen, fire and breach
//!   from the engineer's `RoomReadout`); the other rooms stay placeholders. Ship oxygen and its
//!   change per second come from the deck; the hull gauge from the hull sections' condition.
//! - The placeholder's forced states still win while set (`PlaceholderStatus::apply_forced`).
//!
//! `crew_core` knows nothing of `reactor_core` or `fire_core`; the mapping lives here.
use super::*;
use crate::dev_area::engineer::{self, Dev, Engineering};
use crew_core::orders::OrderKind;
use crew_core::status::{ReactorMode, RoomStatus, ShipSystem, ShipStatus, TempBand, SYSTEM_MAX};
use crew_core::{DeviceId, RoomId, StationId};
use reactor_core::{AlarmKind, Group};

/// Room ids of the engineer corner's two real rooms.
pub const ENGINE_ROOM: &str = "engine_room";
pub const SIDE_ROOM: &str = "side_room";
/// Station ids of the engineer corner's consoles (their box ids).
pub const REACTOR_CONSOLE: &str = "engineer_reactor_console";
pub const POWER_CONSOLE: &str = "engineer_power_console";
/// Smoothing of the oxygen change per second (s).
const O2_SMOOTH_S: f64 = 1.0;

/// The ship as the captain sees it this tick.
#[derive(Resource, Default, Debug, Clone)]
pub struct LiveShip {
    pub status: Option<ShipStatus>,
    /// The deck's oxygen fraction last tick and its smoothed change per second.
    prev_o2: Option<f64>,
    pub o2_delta: f64,
}

impl LiveShip {
    /// The status, or the placeholder's when nothing was built yet.
    pub fn get(&self, crew: &CrewState) -> ShipStatus {
        self.status.clone().unwrap_or_else(|| crew.ship.status())
    }
}

/// The deck's oxygen over all its rooms, 0..1.
pub fn deck_oxygen(e: &Engineering) -> f64 {
    let (o, v) = [e.room, e.side_room].iter().fold((0.0, 0.0), |(o, v), r| {
        let room = e.deck.room(*r);
        (o + room.oxygen, v + room.volume)
    });
    if v > 0.0 { o / v } else { 0.0 }
}

pub fn temp_band(e: &Engineering) -> TempBand {
    let (t, b) = (e.reactor.temperature(), e.reactor.bands());
    if t >= b.temp_allowed.hi {
        TempBand::Critical
    } else if t >= b.temp_optimal.hi {
        TempBand::Hot
    } else if t < b.temp_optimal.lo {
        TempBand::Cold
    } else {
        TempBand::Normal
    }
}

fn bar(c: f64) -> Option<u8> {
    Some((c.clamp(0.0, 1.0) * SYSTEM_MAX as f64).round() as u8)
}

/// Lays the engineer corner's real data over a (placeholder) status.
pub fn overlay(s: &mut ShipStatus, e: &Engineering, o2_delta: f64) {
    let r = &e.reactor;
    let fuel: f64 = r.rods().iter().map(|x| x.life).sum::<f64>() / (r.tuning().slots as f64 * 100.0).max(1.0);
    let mode = if r.alarm(AlarmKind::Shutdown) || r.lockout_s() > 0.0 {
        ReactorMode::Shutdown
    } else if !r.power_on() {
        ReactorMode::Off
    } else if r.alarm(AlarmKind::TempHigh) || r.alarm(AlarmKind::TempCritical) {
        ReactorMode::Overheating
    } else {
        ReactorMode::Running
    };
    s.reactor = crew_core::status::ReactorStatus { temp: r.temperature() / 100.0, band: temp_band(e), fuel, mode, output_kw: Some(r.output_kw()), load_kw: Some(e.grid.request_kw()) };

    let o2_room = e.deck.room(e.room).oxygen_fraction();
    let burning = e.deck.fires_in(e.room).next().is_some();
    let devices = e.pump.fraction().min(r.condition().fraction());
    for b in s.systems.iter_mut() {
        match b.system {
            ShipSystem::Engineering => {
                b.condition = bar(devices);
                b.fire = burning;
            }
            ShipSystem::Weapons => b.condition = bar(e.grid.factor(Group::Weapons)),
            ShipSystem::LifeSupport => b.condition = bar(e.grid.factor(Group::LifeSupport) * (o2_room / 0.9).min(1.0)),
            ShipSystem::Science => {}
        }
    }
    s.shields.value *= e.grid.factor(Group::Shields);

    let real = |id: &str, name: &str, room: fire_core::RoomId, condition: f64, system: Option<ShipSystem>, crew: u32| {
        let r = e.room_readout(room);
        RoomStatus { id: RoomId::new(id), name: TextKey::new(format!("room.{name}")), system, condition, oxygen: r.oxygen, fire: r.fire, breach: r.breach, crew }
    };
    let crew = s.rooms.iter().find(|x| x.id.as_str() == ENGINE_ROOM).map_or(1, |x| x.crew);
    let engine = real(ENGINE_ROOM, ENGINE_ROOM, e.room, devices, Some(ShipSystem::Engineering), crew);
    let side = real(SIDE_ROOM, SIDE_ROOM, e.side_room, 1.0, None, 0);
    match s.rooms.iter().position(|x| x.id.as_str() == ENGINE_ROOM) {
        Some(i) => {
            s.rooms[i] = engine;
            s.rooms.insert(i + 1, side);
        }
        None => s.rooms.extend([engine, side]),
    }
    let sections = e.hull.sections();
    if !sections.is_empty() {
        s.hull.value = sections.iter().map(|x| x.condition.fraction()).sum::<f64>() / sections.len() as f64 * s.hull.max;
    }
    s.oxygen.level = deck_oxygen(e);
    s.oxygen.delta_per_s = o2_delta;
}

/// Whether the real ship is in trouble: a reactor alarm (hot, critical, scram) or a fire.
pub fn real_threat(e: &Engineering) -> bool {
    [AlarmKind::TempHigh, AlarmKind::TempCritical, AlarmKind::Shutdown].into_iter().any(|k| e.reactor.alarm(k))
        || !e.deck.fires().is_empty()
        || e.room_readouts().iter().any(|r| r.breach || r.oxygen_alarm)
}

/// One active alarm on the captain's overview: what, where, and the order it suggests.
#[derive(Clone, Debug, PartialEq)]
pub struct AlarmRow {
    pub text: String,
    pub room: &'static str,
    pub order: Option<(OrderKind, Target)>,
}

/// The real ship's alarms: fires, the reactor's alarm list, broken devices. Most urgent first.
pub fn alarms(e: &Engineering) -> Vec<AlarmRow> {
    let mut out = Vec::new();
    for (room, id) in [(e.room, ENGINE_ROOM), (e.side_room, SIDE_ROOM)] {
        let size: f64 = e.deck.fires_in(room).map(|f| f.size).sum();
        if size > 0.0 {
            out.push(AlarmRow { text: format!("FIRE {size:.1} m"), room: id, order: Some((OrderKind::Extinguish, Target::Room(RoomId::new(id)))) });
        }
    }
    for (room, id) in [(e.room, ENGINE_ROOM), (e.side_room, SIDE_ROOM)] {
        let r = e.room_readout(room);
        if r.breach {
            out.push(AlarmRow { text: format!("BREACH {:.0} %", r.breach_open * 100.0), room: id, order: Some((OrderKind::FixBreach, Target::Room(RoomId::new(id)))) });
        }
        if r.oxygen_alarm {
            out.push(AlarmRow { text: format!("AIR LOW {:.0} %", r.oxygen * 100.0), room: id, order: Some((OrderKind::Repair, Target::Device(DeviceId::new(engineer::GENERATOR)))) });
        }
    }
    let console = || Some((OrderKind::Operate, Target::Station(StationId::new(REACTOR_CONSOLE))));
    let r = &e.reactor;
    if r.alarm(AlarmKind::TempCritical) {
        out.push(AlarmRow { text: format!("REACTOR CRIT {:.0}", r.temperature()), room: ENGINE_ROOM, order: console() });
    } else if r.alarm(AlarmKind::TempHigh) {
        out.push(AlarmRow { text: format!("REACTOR HOT {:.0}", r.temperature()), room: ENGINE_ROOM, order: console() });
    }
    if r.alarm(AlarmKind::Shutdown) {
        out.push(AlarmRow { text: format!("SCRAM, locked {:.0} s", r.lockout_s().ceil()), room: ENGINE_ROOM, order: console() });
    }
    if r.alarm(AlarmKind::FuelLow) {
        out.push(AlarmRow { text: "FUEL LOW".into(), room: ENGINE_ROOM, order: console() });
    }
    for (d, id) in [(Dev::Reactor, engineer::REACTOR), (Dev::Pump, engineer::PUMP), (Dev::Generator, engineer::GENERATOR)] {
        let c = e.condition(d);
        if !c.functioning() || c.needs_repair() {
            let state = if c.functioning() { "WORN" } else { "BROKEN" };
            out.push(AlarmRow { text: format!("{} {state} {:.0} %", match d { Dev::Pump => "PUMP", Dev::Reactor => "REACTOR", Dev::Generator => "O2 GEN" }, c.fraction() * 100.0), room: ENGINE_ROOM, order: Some((OrderKind::Repair, Target::Device(DeviceId::new(id)))) });
        }
    }
    out
}

/// Where a target stands in the area (m east, m north of the area's centre), for markers and the
/// bot; none for placeholder targets without a place.
pub fn target_point(t: &Target) -> Option<[f64; 2]> {
    use crate::dev_area::{CAPTAIN, ENGINEERING};
    use engineer::{DOOR_AT, EXTINGUISHER_AT, GENERATOR_AT, POWER_CONSOLE_AT, PUMP_AT, REACTOR_AT, REACTOR_CONSOLE_AT};
    match t {
        Target::Room(r) => match r.as_str() {
            ENGINE_ROOM => Some(ENGINEERING.point([0.0, 7.0])),
            SIDE_ROOM => Some(ENGINEERING.point([8.5, 10.0])),
            s if s == bots::FIRE_ROOM => Some(CAPTAIN.point(bots::FIRE_AT)),
            _ => None,
        },
        Target::Station(s) => match s.as_str() {
            REACTOR_CONSOLE => Some(ENGINEERING.point(REACTOR_CONSOLE_AT)),
            POWER_CONSOLE => Some(ENGINEERING.point(POWER_CONSOLE_AT)),
            s if s == bots::TURRET_STATION => Some(CAPTAIN.point(bots::TURRET_AT)),
            _ => None,
        },
        Target::Device(d) => match d.as_str() {
            engineer::REACTOR => Some(ENGINEERING.point(REACTOR_AT)),
            engineer::PUMP => Some(ENGINEERING.point(PUMP_AT)),
            engineer::EXTINGUISHER => Some(ENGINEERING.point(EXTINGUISHER_AT)),
            engineer::DOOR => Some(ENGINEERING.point(DOOR_AT)),
            engineer::GENERATOR => Some(ENGINEERING.point(GENERATOR_AT)),
            _ => None,
        },
    }
}

/// The marker over the target of the order that applies to the player's role.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct OrderMarker {
    /// Area metres, on the ground below the marker.
    pub at: Option<[f64; 2]>,
}

/// Each tick after the engineer corner stepped: the live status and the order marker.
pub fn update(time: Res<Time>, crew: Res<CrewState>, eng: Option<Res<Engineering>>, role: Res<DevRole>, mut live: ResMut<LiveShip>, mut marker: ResMut<OrderMarker>) {
    let dt = time.delta_secs_f64();
    let mut s = crew.ship.status_unforced();
    if let Some(e) = eng.as_deref() {
        let o2 = deck_oxygen(e);
        if let Some(prev) = live.prev_o2
            && dt > 0.0
        {
            let k = (dt / O2_SMOOTH_S).min(1.0);
            live.o2_delta += ((o2 - prev) / dt - live.o2_delta) * k;
        }
        live.prev_o2 = Some(o2);
        overlay(&mut s, e, live.o2_delta);
    }
    crew.ship.apply_forced(&mut s);
    live.status = Some(s);
    let at = crew.orders.for_role(role.0).and_then(|o| o.target.as_ref()).and_then(target_point);
    if marker.at != at {
        marker.at = at;
    }
}

#[derive(Component)]
pub struct MarkerLook;

/// Window: a turning cyan box 3 m over the order's target.
pub fn draw_marker(mut commands: Commands, marker: Res<OrderMarker>, area: Res<DevArea>, planet: Res<crate::env::PlanetRes>, time: Res<Time>, mut q: Query<(&mut crate::origin::WorldPose, &mut Visibility), With<MarkerLook>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let Some(frame) = area.frame else { return };
    let Ok((mut pose, mut vis)) = q.single_mut() else {
        commands.spawn((
            MarkerLook,
            Mesh3d(meshes.add(Cuboid::new(0.5, 0.5, 0.5))),
            MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.3, 0.9, 1.0), emissive: LinearRgba::rgb(0.2, 0.8, 1.2), ..default() })),
            Transform::default(),
            crate::origin::WorldPose::default(),
            Visibility::Hidden,
        ));
        return;
    };
    match marker.at {
        Some(p) => {
            let bob = (time.elapsed_secs_f64() * 2.0).sin() * 0.2;
            let (pos, up) = frame.ground(&planet, p[0], p[1], 3.0 + bob);
            let rot = frame.rotation(up, time.elapsed_secs_f64().to_degrees() * 1.5) * bevy::math::DQuat::from_rotation_x(std::f64::consts::FRAC_PI_4);
            *pose = crate::origin::WorldPose { pos, rot };
            *vis = Visibility::Visible;
        }
        None => *vis = Visibility::Hidden,
    }
}
