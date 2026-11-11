//! The ship's simulation glue: the one `reactor_core::EngineRoom` the host steps (`ReactorSim`), its
//! tuning (`ReactorConfig`), the event counters and the order of a ship tick (`ShipSet`). Each owner
//! module takes its own asks (`host_asks`) and reacts to the room's events (`ShipEvents`); this
//! module steps the room, runs the incident director, builds the news and sends them.
//! - Values: `content/gameplay/{engineering,bridge,gunnery,hull,crew,incidents}/` (`reactor_core::Tuning`, strict loader, one file per owner).
//! - TODO(initiator): the incident director stays in the ship's step until it has an owner of its own (#306, Annahmen).
use crate::dev_area::*;
use crate::dev_area::engineering::canisters;
use crate::dev_area::fire::{FireGrid, FireModel};
use crate::dev_area::hull::ShipAir;
use crate::items::{Items, NetOut, Role};
use item_core::wire::{Alarm, BotNews, BreakerGroup, EnemyNews, EnemyOutcome, CanisterSlot, JumpStage, Lamp, PartNews, PumpMode, ReactorAsk, ReactorFault, ReactorNews, StartStage, GRAPH_POINTS, ROOMS};
use reactor_core::bots::{Bots, Row};
use reactor_core::director::{Incident, Situation};
use reactor_core::engine_room::EngineSave;
use reactor_core::RoomAlarm;
use reactor_core::{AlarmLevel, Director, EngineRoom, Fault, Group, Input, Outcome, ReactorEvent, RoomEvent, Tuning, Warnings};

pub const SEND_EVERY_S: f64 = 0.25;
/// The room's seed: the same room every start (its rolls are seeded).
pub const SEED: u64 = 0x297;

#[derive(Resource, Clone, Debug)]
pub struct ReactorConfig(pub Tuning);

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReactorCounts {
    pub fires: u32,
    pub sparks: u32,
    pub implosions: u32,
    pub blasts: u32,
    pub ejects: u32,
    /// The ejected cores that imploded outside the ship (#325).
    pub core_implosions: u32,
    /// The SCRAMs of the physical button (#316).
    pub scrams: u32,
    pub ecools: u32,
    /// The jumps the drive has made (r40, #252): the news counter the flash and the notice follow.
    pub jumps: u32,
}

#[derive(Resource, Debug)]
pub struct ReactorSim {
    pub room: EngineRoom,
    pub manual: bool,
    pub injection: f64,
    pub converter: f64,
    pub alarmed: bool,
    pub sent: Option<ReactorNews>,
    pub since_send: f64,
    /// The incident director (r38): the host steps it in `reactor_step`; the toggle sets it.
    pub director: Director,
    /// The rooms' alarms (r40, #165), by the index of `ReactorNews::rooms`: the engine room, the corridor,
    /// the bridge (r44). The host steps them in `reactor_step`; SILENCE mutes them.
    pub alarms: [RoomAlarm; ROOMS],
    /// GN-01's CLEAR JAM hold is on (r41): the host counts it in `tick_jam` while the gun is jammed.
    pub clearing: bool,
    /// The bots (r42, #253): the count and the captain's list survive a reset; the host steps them in `bots::bots_step`.
    pub bots: Bots,
}

/// The engine room's save section (r42, #128) from the host's state: the room, its air, and the
/// switches the host keeps beside the room.
pub fn engine_section(sim: &ReactorSim, air: &ShipAir) -> EngineSave {
    EngineSave { room: sim.room.to_save(), air: air.to_save(), vents: [true; ROOMS], manual: sim.manual, injection: sim.injection, converter: sim.converter, director: sim.director.to_save() }
}

/// Puts a loaded section into the room, the air and the switches. The alarms and the timers are not
/// in the save: they keep their values.
pub fn apply_engine_section(sec: &EngineSave, cfg: &Tuning, sim: &mut ReactorSim, air: &mut ShipAir) {
    sim.room = EngineRoom::from_save(cfg, &sec.room);
    sim.manual = sec.manual;
    sim.injection = sec.injection;
    sim.converter = sec.converter;
    sim.director = Director::from_save(&cfg.director, &sec.director);
    *air = ShipAir::from_save(&cfg.airlock, &sec.air);
}

/// `--warm-reactor`: the room is warm-started when it is created (`ReactorSim::created`), not run 150 s
/// on Auto.
#[derive(Resource)]
pub struct WarmAtCreation;

impl ReactorSim {
    pub fn new(cfg: &Tuning) -> ReactorSim {
        ReactorSim::created(cfg, false)
    }

    pub fn created(cfg: &Tuning, warm: bool) -> ReactorSim {
        let mut room = EngineRoom::from_tuning(cfg, SEED);
        // The ship starts ready: the warm-up wears nothing (`dev::Wear` turns wear on later).
        room.set_wear(false);
        room.set_power(true);
        if warm {
            room.warm_start();
        } else {
            for _ in 0..1500 {
                let _ = room.step(0.1, Input::auto());
            }
        }
        // The warm-up runs the tank down; a fresh room starts with a full one (TODO(initiator): later
        // the tank may start where the ship's last run left it).
        room.pump_mut().refill(100.0);
        // The battery starts charged (PRIME and a SCRAM's restart live off it) and charges by itself while the reactor runs: no switch (#319).
        let cap = room.battery().tuning().capacity_kwmin;
        let start = room.battery().tuning().start_charge.clamp(0.0, 1.0);
        room.battery_mut().set_charge_kwmin(cap * start);
        room.battery_mut().set_recharge(1.0);
        ReactorSim { room, manual: false, injection: 0.0, converter: 0.0, alarmed: false, sent: None, since_send: f64::MAX, director: Director::new(&cfg.director, SEED), alarms: [RoomAlarm::new(cfg.alarm); ROOMS], clearing: false, bots: Bots::new(&cfg.bots) }
    }

    /// The CLEAR JAM hold's time (r41): while it is on, the gun's jam clears after its hold.
    /// The gun's skill and clearing speed follow the bots (r42): a bot at GN-01 has its skill and clears at
    /// its own speed; a player's gun has skill 0 and clears at full speed.
    /// The reactor's implosion timer runs: aboard, or the ejected core's outside.
    pub fn countdown_running(&self) -> bool {
        let r = self.room.reactor();
        r.outside_eta_s().is_some() || implosion_eta(r.timers().1, r.tuning().implosion_delay_s, r.imploded()).is_some_and(f64::is_finite)
    }

    pub fn tick_jam(&mut self, dt: f64) {
        let w = self.room.weapons_mut();
        w.set_skill(self.bots.gun_skill());
        if self.clearing {
            w.clear_jam(dt * self.bots.clear_scale());
        }
    }


    /// A fresh room (the dev reset): the alarm the old room set stays until the new room says
    /// otherwise, and the operator's settings stay: the director's toggle and the bots' count and
    /// list (with no bot at a station).
    pub fn reset(&mut self, cfg: &Tuning) {
        let alarmed = self.alarmed;
        let director = self.director.enabled();
        *self = ReactorSim { sent: None, since_send: f64::MAX, alarmed, bots: self.bots.fresh(), ..ReactorSim::new(cfg) };
        self.director.set_enabled(director);
    }

    /// The snapshot as the wire carries it; `oxygen` is the engine room's air (0..1).
    pub fn news(&self, oxygen: f64) -> ReactorNews {
        let r = self.room.reactor();
        let b = r.bands();
        let (set_injection, set_converter) = r.targets();
        let grid = self.room.grid();
        ReactorNews {
            power_on: r.power_on(),
            manual: self.manual,
            imploded: r.imploded(),
            stability: (r.stability() * 100.0).round() as u8,
            heat: r.heat(),
            set_injection,
            set_converter,
            injection: r.injection(),
            converter: r.converter(),
            output_kw: r.output_kw(),
            load_kw: self.room.main_load_kw(),
            optimal: [b.heat_optimal.lo, b.heat_optimal.hi],
            allowed: [b.heat_allowed.lo, b.heat_allowed.hi],
            injection_optimal: [b.injection_optimal.lo, b.injection_optimal.hi],
            injection_allowed: [b.injection_allowed.lo, b.injection_allowed.hi],
            converter_optimal: [b.converter_optimal.lo, b.converter_optimal.hi],
            converter_allowed: [b.converter_allowed.lo, b.converter_allowed.hi],
            powered: self.room.powered(),
            breakers: Group::ALL.map(|g| grid.breaker(g)),
            limits: Group::ALL.map(|g| grid.limit(g)),
            delivered_kw: Group::ALL.map(|g| grid.supply(g).delivered_kw),
            demand_kw: Group::ALL.map(|g| grid.group(g).demand_kw),
            lights_factor: grid.factor(Group::Lights),
            engine_lamps_off: self.room.engine_lamps_off(),
            aux_on: self.room.aux_enabled(),
            aux_life_support: self.room.aux_switch(Group::LifeSupport),
            aux_lights: self.room.aux_switch(Group::Lights),
            aux_boost_kw: self.room.aux_boost_kw(),
            pump_on: self.room.pump().on(),
            pump_mode: wire_pump(self.room.pump().mode()),
            pump_speed: self.room.pump().speed(),
            flow: self.room.flow(),
            coolant: self.room.pump().tank(),
            coolant_low: self.room.pump().tank() < self.room.tuning().coolant_low,
            coolant_leak: self.room.pump_leak_rate(),
            overvolt: self.room.junctions().iter().any(|j| j.overvolted()),
            // The host's blast count is set by `reactor_step` (the sim does not count).
            blasts: 0,
            oxygen,
            canisters: std::array::from_fn(|i| r.canisters().iter().find(|c| c.slot == i).map_or_else(CanisterSlot::default, |x| CanisterSlot { canister: Some(canisters::canister_kind(&x.kind)), life: x.life })),
            reactor: part(r.condition().fraction(), r.condition().functioning()),
            pump: part(self.room.pump().condition().fraction(), self.room.pump().condition().functioning()),
            junctions: std::array::from_fn(|i| part(self.room.junction(i).condition().fraction(), self.room.junction(i).condition().functioning())),
            junction_fault: std::array::from_fn(|i| self.room.junction(i).condition().needs_repair() || !self.room.junction(i).condition().functioning()),
            junction_blown: std::array::from_fn(|i| self.room.junction(i).blown()),
            oxygen_generator: part(self.room.oxygen_generator().condition().fraction(), self.room.oxygen_generator().condition().functioning()),
            breaches: std::array::from_fn(|i| self.room.hull().damage(i)),
            lamps: lamps_of(&self.room.warnings()),
            boost: r.heat_boost(),
            boost_ok: r.power_on() && r.condition().functioning() && r.boost_allowed(),
            // An ejected core's countdown is the one outside (#325); the reactor's own timer is clear then.
            implosion_eta_s: r.outside_eta_s().or_else(|| implosion_eta(r.timers().1, r.tuning().implosion_delay_s, r.imploded())),
            outside_eta_s: r.outside_eta_s(),
            core_implosions: 0,
            ejected: r.ejected(),
            // The host's count is set by `reactor_step`, as the blasts are.
            ejects: 0,
            // The physical parts' state is filled in by `reactor_room` (the host's `Machine`).
            scram_cover: false,
            eject_cover: false,
            scrams: 0,
            ecool_active: self.room.ecool_active(),
            ecool_ready_in_s: self.room.ecool_ready_in_s(),
            ecool_ok: self.room.ecool_allowed(),
            ecools: 0,
            // The host sets the rooms in `reactor_step` (they come from the air and the fire).
            rooms: Default::default(),
            // The host sets the alarms in `reactor_step` too (r40).
            alarms: Default::default(),
            // The player-facing vents are gone: every vent stays open (the wire still carries the field).
            vents: [true; ROOMS],
            throttle: self.room.engine_throttle(),
            engine_thrust: self.room.engine_thrust(),
            graph_points: self.room.history().size().min(GRAPH_POINTS) as u8,
            graph_output: graph_row(&self.room.history().output),
            graph_load: graph_row(&self.room.history().load),
            battery_charge: self.room.battery().fraction(),
            battery_recharge: self.room.battery().recharge(),
            battery_kw: self.room.battery().output_kw(),
            battery_in_kw: self.room.battery().input_kw(),
            director: self.director.enabled(),
            shield: self.room.shield().fraction(),
            quantum_shield: self.room.shield().quantum_up(),
            shield_regen: self.room.shield().regen_per_s() / self.room.shield().tuning().max.max(1e-9) * 100.0,
            weapon_shots: self.room.weapons().shots(),
            weapon_charge: self.room.weapons().charge(),
            weapon_reload_s: self.room.weapons().reload_left(),
            weapon_heat: self.room.weapons().heat(),
            weapon_jammed: self.room.weapons().jammed(),
            weapon_overheated: self.room.weapons().overheated(),
            jump_stage: wire_jump(self.room.jump().stage),
            jump_charge: self.room.jump().charge,
            // The host's count is set by `reactor_step`, as the ejects are.
            jumps: 0,
            // The host sets the airlock and the vacuum in `reactor_step` (they come from the air).
            airlock: Default::default(),
            vacuum: false,
            enemy: self.room.encounter().map(|e| EnemyNews { shield: e.shield_fraction(), hull: e.hull_fraction(), outcome: enemy_outcome(e.outcome()), shots: e.shots(), section: e.last_section() as u8 }),
            // The bots (r42): a station a bot runs shows its BOT tag; the captain's levels are on CP-01.
            bots: std::array::from_fn(|i| {
                let row = Row::ALL[i];
                BotNews { manned: self.bots.manned_row(row), level: self.bots.level(row) }
            }),
            bot_count: self.bots.count(),
            start: wire_start(self.room.start().stage()),
            // The stand's toggles and lever are the host's controls: `reactor_room` fills them in.
            toggles: [false; 3],
            lever: 0,
            trap: self.room.start().trap(),
            trap_left_s: self.room.start().prime_left_s(),
            prime_need: self.room.prime_need_fraction(),
        }
    }
}

/// The encounter's end as the wire carries it.
pub fn enemy_outcome(o: Outcome) -> EnemyOutcome {
    match o {
        Outcome::None => EnemyOutcome::None,
        Outcome::Destroyed => EnemyOutcome::Destroyed,
        Outcome::Escaped => EnemyOutcome::Escaped,
    }
}

/// The reactor's start stage as the wire carries it (#319).
pub fn wire_start(s: reactor_core::StartStage) -> StartStage {
    use reactor_core::StartStage as Core;
    match s {
        Core::Cold => StartStage::Cold,
        Core::Boot => StartStage::Boot,
        Core::Prime => StartStage::Prime,
        Core::RunUp => StartStage::RunUp,
        Core::Running => StartStage::Running,
        Core::ShutDown => StartStage::ShutDown,
    }
}

/// The jump drive's stage as the wire carries it.
pub fn wire_jump(s: reactor_core::JumpStage) -> JumpStage {
    use reactor_core::JumpStage as Core;
    match s {
        Core::ColdStart => JumpStage::ColdStart,
        Core::Prepping => JumpStage::Prepping,
        Core::Paused => JumpStage::Paused,
        Core::Ready => JumpStage::Ready,
        Core::Active => JumpStage::Active,
    }
}

/// The first `GRAPH_POINTS` of a graph row (newest first), in whole kW.
fn graph_row(row: &[f64]) -> [u16; GRAPH_POINTS] {
    std::array::from_fn(|i| row.get(i).map_or(0, |v| v.round().clamp(0.0, f64::from(u16::MAX)) as u16))
}

pub fn lamps_of(w: &Warnings) -> [bool; Lamp::COUNT] {
    let mut lamps = [false; Lamp::COUNT];
    for (lamp, on) in [
        (Lamp::Hot, w.overheat),
        (Lamp::CritHeat, w.critical_heat),
        (Lamp::Cold, w.cold),
        (Lamp::LowHeat, w.low_heat),
        (Lamp::HighOut, w.high_output),
        (Lamp::CritOut, w.critical_output),
        (Lamp::LowOut, w.low_output),
        (Lamp::FillLow, w.low_fill),
        (Lamp::NoFill, w.fill_out),
        (Lamp::Implosion, w.implosion),
        (Lamp::Scram, w.scram),
    ] {
        lamps[lamp.index()] = on;
    }
    lamps
}

/// Seconds to the implosion at the current rate: the implosion timer (0..1 of its delay) has
/// `(1 - timer) * delay` left. None while the timer is at 0 and once the core has imploded.
pub fn implosion_eta(timer: f64, delay_s: f64, imploded: bool) -> Option<f64> {
    (timer > 0.0 && !imploded).then(|| (1.0 - timer.clamp(0.0, 1.0)) * delay_s)
}

/// Every process: a news counter (`ejects`, `ecools`) against the last one this process saw. True when
/// it went up (the notice shows); the first news only sets the baseline, so a late joiner does not
/// replay old events.
pub fn counter_fresh(seen: &mut Option<u32>, ejects: u32) -> bool {
    let fresh = matches!(*seen, Some(s) if ejects > s);
    *seen = Some(ejects);
    fresh
}

/// Whether the blinking lamps are lit at a time: 2 Hz, lit for the first quarter of each half
/// second (Barotrauma's `TotalTime % 0.5 < 0.25`).
pub fn lamp_blink_on(t: f64) -> bool {
    t.rem_euclid(0.5) < 0.25
}

pub fn group_of(g: BreakerGroup) -> Group {
    match g {
        BreakerGroup::Engines => Group::Engines,
        BreakerGroup::LifeSupport => Group::LifeSupport,
        BreakerGroup::Lights => Group::Lights,
        BreakerGroup::Shields => Group::Shields,
        BreakerGroup::Weapons => Group::Weapons,
        BreakerGroup::JumpDrive => Group::JumpDrive,
    }
}

pub fn pump_of(m: PumpMode) -> reactor_core::engine_room::PumpMode {
    use reactor_core::engine_room::PumpMode as Core;
    match m {
        PumpMode::Off => Core::Off,
        PumpMode::Low => Core::Low,
        PumpMode::High => Core::High,
    }
}

pub fn wire_pump(m: reactor_core::engine_room::PumpMode) -> PumpMode {
    use reactor_core::engine_room::PumpMode as Core;
    match m {
        Core::Off => PumpMode::Off,
        Core::Low => PumpMode::Low,
        Core::High => PumpMode::High,
    }
}

pub fn fault(f: ReactorFault) -> Option<Fault> {
    match f {
        ReactorFault::PumpBreak => Some(Fault::PumpBreak),
        ReactorFault::OxygenGeneratorBreak => Some(Fault::OxygenGeneratorBreak),
        ReactorFault::LoadDrop => Some(Fault::LoadDrop),
        ReactorFault::Surge => Some(Fault::Surge),
        ReactorFault::HullBreach => Some(Fault::HullBreach),
        ReactorFault::HitLight => Some(Fault::HitLight),
        ReactorFault::HitHeavy => Some(Fault::HitHeavy),
        ReactorFault::Fire | ReactorFault::DrainCoolant | ReactorFault::WarmStart | ReactorFault::Aux | ReactorFault::ShieldHit | ReactorFault::JumpFull => None,
    }
}

pub fn alarm_of(l: AlarmLevel) -> Alarm {
    match l {
        AlarmLevel::None => Alarm::None,
        AlarmLevel::Warn => Alarm::Warn,
        AlarmLevel::Danger => Alarm::Danger,
    }
}

pub fn route(e: RoomEvent, counts: &mut ReactorCounts) {
    match e {
        RoomEvent::Reactor(ReactorEvent::Fire { size }) => {
            counts.fires += 1;
            info!("reactor: fire of {size} m at the reactor");
        }
        RoomEvent::Fire { size, source } => {
            counts.fires += 1;
            info!("engine room: fire of {size} m from {source:?}");
        }
        RoomEvent::HitFire { size, at } => {
            counts.fires += 1;
            info!("engine room: fire of {size} m at the hit on {at:?}");
        }
        RoomEvent::EnemyShot { section } => info!("enemy: shot at the wall, section {section}"),
        RoomEvent::Sparks(i) => {
            counts.sparks += 1;
            info!("sparks at junction box {}", i + 1);
        }
        RoomEvent::Reactor(ReactorEvent::Implosion) => {
            counts.implosions += 1;
            warn!("reactor: IMPLOSION");
        }
        RoomEvent::Reactor(ReactorEvent::Blast(_)) => {
            counts.blasts += 1;
        }
        RoomEvent::Reactor(ReactorEvent::CoreEjected) => {
            counts.ejects += 1;
            info!("reactor: CORE EJECTED");
        }
        RoomEvent::Reactor(ReactorEvent::CoreImplodedOutside) => {
            counts.core_implosions += 1;
            warn!("reactor: the ejected core IMPLODED OUTSIDE");
        }
        RoomEvent::EmergencyCool => {
            counts.ecools += 1;
            info!("reactor: EMERGENCY COOLING");
        }
        RoomEvent::Device { source, event } => info!("engine room: {source:?} {event:?} (the machine's condition crossed a line)"),
        _ => {}
    }
}

/// The order of a ship tick in `FixedUpdate` (`Fx::Input`): the bots, the owners' asks, the room's
/// step, the owners' reactions to its events, the counts and the alarm, the news, the owners' parts
/// of the news, the send, the effects every process shows, and the rooms' doors and air.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShipSet {
    Crew,
    Asks,
    Step,
    React,
    Settle,
    News,
    Fill,
    Send,
    Effects,
    Rooms,
}

/// The room's events of this tick, host only: the owners read them in `ShipSet::React`, the counts
/// take them in `ShipSet::Settle`. A dev fault may add one before the step (`ShipSet::Asks`).
#[derive(Resource, Default, Debug)]
pub struct ShipEvents(pub Vec<RoomEvent>);

/// Host only: takes the asks an owner handles out of the queue and leaves the rest. A client keeps
/// them all, `ship_step` sends them to the host.
pub fn host_asks(items: &mut Items, mine: fn(&ReactorAsk) -> bool) -> Vec<ReactorAsk> {
    if items.role == Role::Client {
        return Vec::new();
    }
    let (taken, rest) = std::mem::take(&mut items.reactor_asks).into_iter().partition(mine);
    items.reactor_asks = rest;
    taken
}

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let tuning = Tuning::load(&files).unwrap_or_else(|e| panic!("ship: {e:?}"));
    let warm = app.world().contains_resource::<WarmAtCreation>();
    app.insert_resource(ReactorSim::created(&tuning, warm)).insert_resource(ReactorConfig(tuning)).init_resource::<ReactorCounts>().init_resource::<ShipEvents>();
    use ShipSet::*;
    app.configure_sets(FixedUpdate, (Crew, Asks, Step, React, Settle, News, Fill, Send, Effects, Rooms).chain().in_set(crate::phases::Fx::Input));
    app.add_systems(FixedUpdate, ship_step.in_set(Step));
    app.add_systems(FixedUpdate, count_events.in_set(Settle));
    app.add_systems(FixedUpdate, ship_news.in_set(News));
    app.add_systems(FixedUpdate, ship_send.in_set(Send));
}

/// The room's step on the host: what is left of the asks has no owner; the room steps on the
/// reactor's controls, the director may break a device, and the events go to `ShipEvents`. A client
/// sends its asks to the host.
#[allow(clippy::too_many_arguments)]
fn ship_step(time: Res<Time>, mut items: ResMut<Items>, mut sim: ResMut<ReactorSim>, mut events: ResMut<ShipEvents>, air: Res<ShipAir>, grid: Res<FireGrid>, model: Res<FireModel>, wear: Res<crate::dev::Wear>, fires: Res<crate::dev_area::fire::FireConfig>) {
    let asks = std::mem::take(&mut items.reactor_asks);
    if items.role == Role::Client {
        for a in asks {
            items.net_out.push(NetOut::ReactorAsk(a));
        }
        return;
    }
    for a in asks {
        warn!("ship: no owner takes {a:?}");
    }
    let dt = time.delta_secs_f64();
    let input = if sim.manual { Input::manual(sim.injection, sim.converter) } else { Input::auto() };
    sim.room.set_wear(wear.0);
    let stepped = sim.room.step(dt, input);
    events.0.extend(stepped);
    // The incident director (#328): its report is the blown boxes, the fires and the countdown; it starts at most one
    // incident here.
    let situation = situation_of(&sim, fire_count(&air, &grid, &model));
    if let Some(incident) = sim.director.step(dt, &situation) {
        events.0.extend(incident_events(&mut sim.room, incident, fires.0.start_size));
    }
}

/// Fires burning in the rooms the crew is in (the engine room, the corridor, the bridge).
pub fn fire_count(air: &ShipAir, grid: &FireGrid, model: &FireModel) -> u32 {
    let n = match *model {
        FireModel::Point => [air.engine, air.corridor, air.bridge].into_iter().map(|room| air.deck.fires_in(room).count()).sum::<usize>(),
        FireModel::Grid => grid.0.burning().count(),
    };
    n as u32
}

/// What the director is told.
pub fn situation_of(sim: &ReactorSim, fires: u32) -> Situation {
    Situation { blown: std::array::from_fn(|i| sim.room.junction(i).blown()), fires, countdown: sim.countdown_running() }
}

/// What an incident does to the room: the box breaks (sparks, maybe a fire), or a machine catches fire. The fires go out as
/// the room's events, so the fire module places them (`layout::ignition_at`).
pub fn incident_events(room: &mut EngineRoom, incident: Incident, machine_fire: f64) -> Vec<RoomEvent> {
    info!("ship: director {incident:?}");
    match incident {
        Incident::JunctionFault { index, fire } => room.blow_junction(index, fire),
        Incident::MachineFire(source) => vec![RoomEvent::Fire { size: machine_fire, source }],
    }
}

/// Host: the tick's events are counted (and logged), after the owners have reacted to them.
fn count_events(mut events: ResMut<ShipEvents>, mut counts: ResMut<ReactorCounts>) {
    for e in events.0.drain(..) {
        route(e, &mut counts);
    }
}

/// Host: the snapshot from the room and the counts. The hull fills in the air, the airlock and the
/// room alarms next (`ShipSet::Fill`).
fn ship_news(mut items: ResMut<Items>, sim: Res<ReactorSim>, counts: Res<ReactorCounts>) {
    if items.role == Role::Client {
        return;
    }
    let mut now = sim.news(0.0);
    now.blasts = counts.blasts;
    now.ejects = counts.ejects;
    now.core_implosions = counts.core_implosions;
    now.ecools = counts.ecools;
    now.jumps = counts.jumps;
    items.reactor = Some(now);
}

/// Host: sends the snapshot when it changed, at most every `SEND_EVERY_S`.
fn ship_send(time: Res<Time>, mut items: ResMut<Items>, mut sim: ResMut<ReactorSim>) {
    if items.role == Role::Client {
        return;
    }
    sim.since_send += time.delta_secs_f64();
    let Some(now) = items.reactor else { return };
    if items.role == Role::Host && sim.sent != Some(now) && sim.since_send >= SEND_EVERY_S {
        items.net_out.push(NetOut::Reactor { to: None, r: now });
        sim.sent = Some(now);
        sim.since_send = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev_area::hull::asks::alarm_news;
    use crate::dev_area::{bridge, crew, engineering, hull, turrets};
    use item_core::wire::{AlarmNews, RoomAlarmKind};

    /// The asks as the owners take them on the host, in one place for these tests.
    fn apply_any(s: &mut ReactorSim, a: ReactorAsk, cfg: &Tuning, counts: &mut ReactorCounts) {
        let taken = engineering::asks::apply(s, a) || bridge::apply(s, a, counts) || turrets::apply(s, a) || hull::asks::apply_sim(s, a) || crew::apply(s, a) || dev_asks::apply_sim(s, a, cfg, counts);
        assert!(taken, "no owner takes {a:?}");
    }
    use item_core::wire::BotStation;
    use reactor_core::bots::Station;

    fn cfg() -> Tuning {
        Tuning::default()
    }

    /// r40, #165: SILENCE mutes both room alarms on the host, and the news carries the kind as it is.
    #[test]
    fn silence_mutes_the_room_alarms_and_the_news_carries_them() {
        let c = cfg();
        let mut s = ReactorSim::new(&c);
        let mut counts = ReactorCounts::default();
        s.alarms[0].step(true, false, 0.9, 0.1);
        assert!(s.alarms[0].state().sounding, "a fire sounds");
        apply_any(&mut s, ReactorAsk::SilenceAlarms, &c, &mut counts);
        assert_eq!(alarm_news(s.alarms[0].state()), AlarmNews { kind: RoomAlarmKind::Fire, sounding: false, silenced: true });
        assert_eq!(alarm_news(s.alarms[1].state()), AlarmNews::default(), "the corridor is clear");
    }

    #[test]
    fn news_carries_the_graph_of_the_room() {
        let mut s = ReactorSim::new(&cfg());
        assert_eq!(s.news(1.0).graph_points, cfg().graph_points as u8);
        for _ in 0..40 {
            s.room.step(0.25, reactor_core::Input::auto());
        }
        let n = s.news(1.0);
        assert_eq!(n.graph_points, 25);
        assert!(n.graph_load[0] > 0, "the grid asks for power");
        assert_eq!(n.graph_load[0] as f64, s.room.history().load[0].round());
        assert!(n.graph_output[..10].iter().any(|&v| v > 0));
    }

    fn settle(s: &mut ReactorSim, done: impl Fn(&ReactorNews) -> bool) {
        for _ in 0..3000 {
            if done(&s.news(1.0)) {
                return;
            }
            s.room.step(0.1, Input::auto());
        }
        panic!("the snapshot never settled");
    }

    #[test]
    fn the_news_carries_the_lamps_the_boost_and_the_countdown() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::SetPower(false), &cfg(), &mut k);
        let n = s.news(1.0);
        assert!(n.lamps[Lamp::Scram.index()], "warm with the power off");
        assert!(!n.lamps[Lamp::Hot.index()] && !n.lamps[Lamp::Implosion.index()]);
        assert!(!n.boost_ok, "no boost with the power off");
        assert_eq!(n.implosion_eta_s, None);
        apply_any(&mut s, ReactorAsk::SetPower(true), &cfg(), &mut k);
        apply_any(&mut s, ReactorAsk::Boost(true), &cfg(), &mut k);
        let amount = s.room.reactor().tuning().boost_amount;
        let n = s.news(1.0);
        assert_eq!(n.boost, amount, "the boost is up");
        assert!(!n.boost_ok, "a boost at its amount is not allowed again");
        apply_any(&mut s, ReactorAsk::Boost(false), &cfg(), &mut k);
        assert_eq!(s.news(1.0).boost, amount, "refused: the boost stays up");
    }

    #[test]
    fn the_boost_ask_is_refused_when_the_core_refuses_it() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::SetPower(false), &cfg(), &mut k);
        apply_any(&mut s, ReactorAsk::Boost(true), &cfg(), &mut k);
        assert_eq!(s.room.reactor().heat_boost(), 0.0, "the power is needed");
        assert_eq!(s.news(1.0).boost, 0.0);
    }

    #[test]
    fn the_fire_ask_takes_a_shot_and_the_news_carry_the_capacitor() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        // The app's room is created powered and warmed up (`created`), so the capacitor starts full.
        assert_eq!(s.news(1.0).weapon_shots, 6, "a powered room starts with a full capacitor");
        apply_any(&mut s, ReactorAsk::Fire, &cfg(), &mut k);
        let n = s.news(1.0);
        assert_eq!(n.weapon_shots, 5);
        assert!(n.weapon_reload_s > 0.0, "the reload shows on GN-01");
        apply_any(&mut s, ReactorAsk::Fire, &cfg(), &mut k);
        assert_eq!(s.news(1.0).weapon_shots, 5, "refused while reloading");
    }

    #[test]
    fn the_captains_priority_ask_sets_the_level_and_the_dev_count_clamps() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::SetBotPriority(BotStation::Helm, 4), &cfg(), &mut k);
        apply_any(&mut s, ReactorAsk::SetBots(7), &cfg(), &mut k);
        let n = s.news(1.0);
        assert_eq!(n.bots[BotStation::Helm.index()].level, 4);
        assert_eq!(n.bot_count, 3, "the count is clamped to 3");
        assert_eq!(cfg().bots.levels[3], 1, "the start list is the shipped one");
    }

    /// Ticks the bots (0.5 s rounds) until one stands at GN-01: the bots walk from the pads first.
    fn walk_to_gun(s: &mut ReactorSim) {
        for _ in 0..200 {
            if s.bots.manned(Station::Gunnery) {
                return;
            }
            s.bots.tick(0.5, [false; 4], &s.room.clone(), &crate::dev_area::crew::layout(), &[]);
        }
        panic!("no bot reached GN-01");
    }

    #[test]
    fn a_bot_at_the_gun_has_its_skill_and_the_news_say_it_mans_the_station() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        walk_to_gun(&mut s);
        s.tick_jam(0.0);
        let n = s.news(1.0);
        assert!(n.bots[BotStation::Gunnery.index()].manned, "the bots man GN-01 at the start (two bots, GUNNERY first), once they have walked there");
        assert_eq!(s.room.weapons().jam_chance(), 0.03 * (1.0 - 20.0 / 50.0), "the bot's skill sets the jam chance");
        apply_any(&mut s, ReactorAsk::SetBots(0), &cfg(), &mut k);
        s.bots.tick(0.5, [false; 4], &s.room.clone(), &crate::dev_area::crew::layout(), &[]);
        s.tick_jam(0.0);
        assert_eq!(s.room.weapons().jam_chance(), 0.03, "no bot, no skill: a player's gun");
    }

    #[test]
    fn a_reset_keeps_the_bot_settings_and_no_bot_is_at_a_station() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::SetBotPriority(BotStation::Power, 0), &cfg(), &mut k);
        apply_any(&mut s, ReactorAsk::SetBots(1), &cfg(), &mut k);
        s.bots.tick(0.5, [false; 4], &s.room.clone(), &crate::dev_area::crew::layout(), &[]);
        apply_any(&mut s, ReactorAsk::Reset, &cfg(), &mut k);
        let n = s.news(1.0);
        assert_eq!((n.bot_count, n.bots[BotStation::Power.index()].level), (1, 0));
        assert!(n.bots.iter().all(|b| !b.manned));
    }

    #[test]
    fn the_clear_jam_hold_clears_the_jam_after_its_time_and_a_release_starts_over() {
        let mut c = cfg();
        c.weapons.jam_chance = 1.0;
        let mut s = ReactorSim::new(&c);
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::Fire, &c, &mut k);
        assert!(s.news(1.0).weapon_jammed, "the shot jams (the chance is 1 here)");
        apply_any(&mut s, ReactorAsk::ClearJam(true), &c, &mut k);
        s.tick_jam(2.0);
        apply_any(&mut s, ReactorAsk::ClearJam(false), &c, &mut k);
        apply_any(&mut s, ReactorAsk::ClearJam(true), &c, &mut k);
        s.tick_jam(2.0);
        assert!(s.news(1.0).weapon_jammed, "the release started the count over: 2 s is not enough");
        s.tick_jam(1.0);
        assert!(!s.news(1.0).weapon_jammed, "3 s of hold clears it");
    }

    #[test]
    fn the_limit_ask_sets_the_group_limit_and_the_news_carry_the_kw() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::SetLimit(BreakerGroup::Engines, 0.5), &cfg(), &mut k);
        let n = s.news(1.0);
        assert_eq!(n.limits, [0.5, 1.0, 1.0, 1.0, 1.0, 1.0]);
        let demand = s.room.grid().group(Group::Engines).demand_kw;
        assert_eq!(n.demand_kw[0], demand, "the demand is the full demand, not the limited request");
        assert!((n.delivered_kw[0] - s.room.grid().supply(Group::Engines).delivered_kw).abs() < 1e-9);
        apply_any(&mut s, ReactorAsk::SetLimit(BreakerGroup::Lights, 7.0), &cfg(), &mut k);
        assert_eq!(s.news(1.0).limits[2], 1.0, "the host clamps it to 0..1");
    }

    #[test]
    fn power_off_and_the_power_switch_ask_the_room() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::SetPower(false), &cfg(), &mut k);
        // The output stops with the next step; injection follows with its lag (seconds).
        for _ in 0..10 {
            s.room.step(0.1, Input::auto());
        }
        let n = s.news(1.0);
        assert!(!n.power_on && !n.powered && n.output_kw == 0.0, "{n:?}");
        for _ in 0..200 {
            s.room.step(0.1, Input::auto());
        }
        assert!(s.news(1.0).heat < 40.0, "the heat falls");
        apply_any(&mut s, ReactorAsk::SetPower(true), &cfg(), &mut k);
        assert!(s.news(1.0).power_on);
    }

    #[test]
    fn breakers_and_the_pump_switch_are_on_the_snapshot() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        let before = s.news(1.0).load_kw;
        apply_any(&mut s, ReactorAsk::SetBreaker(BreakerGroup::Lights, false), &cfg(), &mut k);
        let n = s.news(1.0);
        assert_eq!(n.breakers, [true, true, false, true, true, true]);
        let lights = s.room.grid().group(Group::Lights).demand_kw;
        assert!((before - n.load_kw - lights).abs() < 1e-9, "the load drops by the lights' demand");
        apply_any(&mut s, ReactorAsk::SetPumpMode(PumpMode::Off), &cfg(), &mut k);
        settle(&mut s, |n| n.flow < 1e-3);
        let n = s.news(1.0);
        assert!(!n.pump_on && n.flow < 1e-3, "the pump spins down");
        apply_any(&mut s, ReactorAsk::SetPumpMode(PumpMode::High), &cfg(), &mut k);
        settle(&mut s, |n| n.flow >= 0.99);
        assert!(s.news(1.0).pump_on && s.news(1.0).flow >= 0.99);
    }

    /// #313: the room panel's LIGHTS ask reaches the sim and the news (so a client sees it); a dead JB-1 ignores it.
    #[test]
    fn the_engine_lamps_ask_flips_the_news_and_a_dead_jb1_ignores_it() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        assert!(!s.news(1.0).engine_lamps_off);
        apply_any(&mut s, ReactorAsk::SetEngineLamps(false), &cfg(), &mut k);
        assert!(s.news(1.0).engine_lamps_off);
        apply_any(&mut s, ReactorAsk::SetEngineLamps(true), &cfg(), &mut k);
        assert!(!s.news(1.0).engine_lamps_off);
        s.room.junction_mut(0).condition_mut().set_value(0.0);
        apply_any(&mut s, ReactorAsk::SetEngineLamps(false), &cfg(), &mut k);
        assert!(!s.news(1.0).engine_lamps_off, "JB-1 dead: the field is ignored");
    }

    #[test]
    fn a_drain_empties_the_tank_and_the_news_says_so() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        assert!(s.news(0.0).coolant > 99.0, "a new room has a full tank");
        apply_any(&mut s, ReactorAsk::Trigger(ReactorFault::DrainCoolant), &cfg(), &mut k);
        let n = s.news(0.0);
        assert_eq!(n.coolant, 0.0);
        assert!(n.coolant_low && n.flow == 0.0, "an empty tank is no flow");
    }

    #[test]
    fn the_aux_fault_flips_the_aux_reactor_and_the_news_follow() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        assert!(!s.room.aux_enabled());
        apply_any(&mut s, ReactorAsk::Trigger(ReactorFault::Aux), &cfg(), &mut k);
        assert!(s.room.aux_enabled(), "the first press turns the aux reactor on");
        assert!(s.news(0.2).aux_on);
        apply_any(&mut s, ReactorAsk::Trigger(ReactorFault::Aux), &cfg(), &mut k);
        assert!(!s.room.aux_enabled(), "the second press turns it off");
        assert!(!s.news(0.2).aux_on);
    }

    #[test]
    fn the_aux_switch_asks_reach_the_room_and_engines_is_not_aux_fed() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::Trigger(ReactorFault::Aux), &cfg(), &mut k);
        apply_any(&mut s, ReactorAsk::SetAuxSwitch(BreakerGroup::Lights, false), &cfg(), &mut k);
        assert!(!s.room.aux_switch(Group::Lights));
        assert!(!s.news(0.2).aux_lights);
        apply_any(&mut s, ReactorAsk::SetAuxSwitch(BreakerGroup::Engines, false), &cfg(), &mut k);
        assert!(s.room.aux_switch(Group::Engines), "Engines reads on whatever it is asked");
    }

    #[test]
    fn the_warm_start_fault_settles_the_room_and_costs_no_coolant_or_canister_life() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::Trigger(ReactorFault::DrainCoolant), &cfg(), &mut k);
        s.room.pump_mut().refill(100.0);
        let tank = s.room.pump().tank();
        let lives = s.room.reactor().canister_lives();
        apply_any(&mut s, ReactorAsk::Trigger(ReactorFault::WarmStart), &cfg(), &mut k);
        assert!(s.room.settled(), "the room is settled after the warm start");
        assert_eq!(s.room.pump().tank(), tank);
        assert_eq!(s.room.reactor().canister_lives(), lives);
        assert_eq!(k.fires, 0);
    }

    #[test]
    fn a_reset_refills_the_tank() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::Trigger(ReactorFault::DrainCoolant), &cfg(), &mut k);
        apply_any(&mut s, ReactorAsk::Reset, &cfg(), &mut k);
        assert!(s.news(0.0).coolant > 99.0);
    }

    #[test]
    fn a_imploded_core_is_on_the_snapshot_and_a_reset_clears_it() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.room.reactor_mut().set_targets(100.0, 30.0);
        for _ in 0..(200.0 / 0.1) as usize {
            s.room.step(0.1, Input::manual(100.0, 30.0));
            if s.room.reactor().imploded() {
                break;
            }
        }
        assert!(s.news(1.0).imploded);
        apply_any(&mut s, ReactorAsk::Reset, &cfg(), &mut k);
        assert!(!s.news(1.0).imploded);
    }

    #[test]
    fn targets_count_only_on_manual_and_manual_starts_from_where_the_reactor_is() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::SetInjection(70.0), &cfg(), &mut k);
        assert!(!s.manual && s.injection == 0.0, "ignored on Auto");
        for _ in 0..30 * 60 {
            s.room.step(1.0 / 30.0, Input::auto());
        }
        let (f, t) = s.room.reactor().targets();
        apply_any(&mut s, ReactorAsk::SetManual(true), &cfg(), &mut k);
        assert!(s.manual && (s.injection - f).abs() < 1e-9 && (s.converter - t).abs() < 1e-9);
        apply_any(&mut s, ReactorAsk::SetInjection(150.0), &cfg(), &mut k);
        assert_eq!(s.injection, 100.0, "clamped");
    }

    #[test]
    fn the_emergency_cooling_press_takes_the_tank_once_and_counts_once() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        let full = s.room.pump().tank();
        apply_any(&mut s, ReactorAsk::EmergencyCool, &cfg(), &mut k);
        let news = s.news(1.0);
        assert!(news.ecool_active && news.ecool_ready_in_s > 0.0 && !news.ecool_ok, "{news:?}");
        assert!(s.room.pump().tank() < full, "the press takes the tank cost");
        let tank = s.room.pump().tank();
        apply_any(&mut s, ReactorAsk::EmergencyCool, &cfg(), &mut k);
        assert_eq!(s.room.pump().tank(), tank, "a second press in the cooldown takes nothing");
        for e in s.room.step(1.0 / 30.0, Input::auto()) {
            route(e, &mut k);
        }
        assert_eq!(k.ecools, 1);
        s.room.step(1.0 / 30.0, Input::auto());
        assert_eq!(k.ecools, 1, "the event comes once");
    }

    #[test]
    fn reset_is_a_fresh_room_and_keeps_the_alarm_it_had() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.alarmed = true;
        apply_any(&mut s, ReactorAsk::Trigger(ReactorFault::PumpBreak), &cfg(), &mut k);
        apply_any(&mut s, ReactorAsk::Reset, &cfg(), &mut k);
        assert!(s.alarmed, "the ship's alarm is cleared by the next step, not by the reset");
        assert!(s.room.pump().condition().functioning() && s.room.reactor().power_on() && !s.manual);
    }

    #[test]
    fn the_eject_notice_shows_once_per_new_count_and_the_first_news_is_the_baseline() {
        let mut seen = None;
        assert!(!counter_fresh(&mut seen, 1), "a late joiner's first news is the baseline");
        assert!(!counter_fresh(&mut seen, 1));
        assert!(counter_fresh(&mut seen, 2));
        assert!(!counter_fresh(&mut seen, 2));
    }

    /// r38: the HELM's throttle sets the engines' target, clamped to -100..100; the news carries it.
    #[test]
    fn the_throttle_ask_sets_the_engine_target_clamped() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        assert_eq!(s.news(1.0).throttle, 100.0, "full throttle at the start");
        apply_any(&mut s, ReactorAsk::SetThrottle(-35.0), &cfg(), &mut k);
        assert_eq!(s.news(1.0).throttle, -35.0);
        apply_any(&mut s, ReactorAsk::SetThrottle(400.0), &cfg(), &mut k);
        assert_eq!(s.news(1.0).throttle, 100.0, "clamped");
    }

    #[test]
    fn the_events_are_counted() {
        let mut k = ReactorCounts::default();
        route(RoomEvent::Sparks(0), &mut k);
        route(RoomEvent::Reactor(ReactorEvent::Fire { size: 0.1 }), &mut k);
        route(RoomEvent::Fire { size: 0.05, source: reactor_core::Source::Junction(0) }, &mut k);
        route(RoomEvent::Reactor(ReactorEvent::Implosion), &mut k);
        assert_eq!(k, ReactorCounts { fires: 2, sparks: 1, implosions: 1, blasts: 0, ejects: 0, core_implosions: 0, scrams: 0, ecools: 0, jumps: 0 });
        route(RoomEvent::Reactor(ReactorEvent::Blast(reactor_core::Blast::default())), &mut k);
        assert_eq!(k.blasts, 1, "the blast is counted");
    }

    #[test]
    fn the_ambush_ask_starts_an_encounter_and_a_jump_escapes_it() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        assert!(s.news(1.0).enemy.is_none(), "no encounter until the ask");
        apply_any(&mut s, ReactorAsk::Ambush(true), &cfg(), &mut k);
        let e = s.news(1.0).enemy.expect("the ambush is on");
        assert_eq!((e.outcome, e.hull), (EnemyOutcome::None, 1.0));
        apply_any(&mut s, ReactorAsk::Trigger(ReactorFault::JumpFull), &cfg(), &mut k);
        apply_any(&mut s, ReactorAsk::JumpGo, &cfg(), &mut k);
        assert_eq!(s.news(1.0).enemy.map(|e| e.outcome), Some(EnemyOutcome::Escaped));
        apply_any(&mut s, ReactorAsk::Ambush(false), &cfg(), &mut k);
        assert!(s.news(1.0).enemy.is_none());
    }

    #[test]
    fn an_enemy_shot_is_the_count_going_up_in_an_encounter() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::Ambush(true), &cfg(), &mut k);
        let before = s.news(1.0);
        // The first shot comes 10 s into the encounter (incidents/enemy.json enemy.first_shot_s).
        for _ in 0..20 {
            s.room.step(0.5, Input::auto());
        }
        let after = s.news(1.0);
        assert!(crate::dev_area::turrets::enemy_shot(before.enemy.map(|e| e.shots), &after));
        assert!(!crate::dev_area::turrets::enemy_shot(None, &after), "the first news is the baseline");
    }

    #[test]
    fn a_jump_needs_the_full_charge_and_counts_once() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        apply_any(&mut s, ReactorAsk::JumpGo, &cfg(), &mut k);
        assert_eq!(k.jumps, 0, "a cold drive does not jump");
        apply_any(&mut s, ReactorAsk::Trigger(ReactorFault::JumpFull), &cfg(), &mut k);
        assert_eq!(s.news(1.0).jump_stage, JumpStage::Ready);
        apply_any(&mut s, ReactorAsk::JumpGo, &cfg(), &mut k);
        assert_eq!(k.jumps, 1);
        let n = s.news(1.0);
        assert_eq!((n.jump_stage, n.jump_charge), (JumpStage::Active, 0.0), "the jump empties the charge");
    }
}

fn part(share: f64, working: bool) -> PartNews {
    PartNews { share, working }
}
