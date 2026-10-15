//! - Values: `content/gameplay/reactor/reactor.json` (`reactor_core::Tuning`, strict loader).
use super::*;
use crate::dev_area::engine_room::{EngineAir, ShipAlarm};
use crate::dev_area::fire::{self, FireAlarm, FireConfig, FireGrid, FireModel};
use crate::items::{Items, NetOut, Role};
use crate::dev_area::engine_room::ROOM;
use crate::dev_area::rods;
use crate::gameplay::Gameplay;
use gameplay_core::notice::{Notice, NoticeKind};
use item_core::wire::{Alarm, BreakerGroup, FuelTube, Lamp, PartNews, PumpMode, ReactorAsk, ReactorFault, ReactorNews, RoomAir, GRAPH_POINTS};
use reactor_core::director::Situation;
use reactor_core::{AlarmLevel, Director, EngineRoom, Fault, Group, Input, ReactorEvent, RoomEvent, Source, Tuning, Warnings};

pub const SEND_EVERY_S: f64 = 0.25;
/// The room's seed: the same room every start (its rolls are seeded).
pub const SEED: u64 = 0x297;

#[derive(Resource, Clone, Debug)]
pub struct ReactorConfig(pub Tuning);

#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReactorCounts {
    pub fires: u32,
    pub sparks: u32,
    pub meltdowns: u32,
    pub blasts: u32,
    pub ejects: u32,
    pub ecools: u32,
}

#[derive(Resource, Debug)]
pub struct ReactorSim {
    pub room: EngineRoom,
    pub manual: bool,
    pub fission: f64,
    pub turbine: f64,
    alarmed: bool,
    /// The host keeps it: a second Eject inside the window ejects.
    eject_left: Option<f64>,
    sent: Option<ReactorNews>,
    since_send: f64,
    /// The two vents' valves (r38; index 0 the engine room's, 1 the corridor's). The host keeps them;
    /// a new room starts with both open.
    pub vents: [bool; 2],
    /// The incident director (r38): the host steps it in `reactor_step`; the toggle sets it.
    pub director: Director,
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
        ReactorSim { room, manual: false, fission: 0.0, turbine: 0.0, alarmed: false, eject_left: None, sent: None, since_send: f64::MAX, vents: [true; 2], director: Director::new(&cfg.director, SEED) }
    }

    pub fn tick_eject(&mut self, dt: f64) {
        if let Some(left) = self.eject_left {
            self.eject_left = (left - dt > 0.0).then_some(left - dt);
        }
    }

    pub fn apply(&mut self, a: ReactorAsk, cfg: &Tuning, counts: &mut ReactorCounts) {
        match a {
            ReactorAsk::SetManual(on) => {
                if on && !self.manual {
                    // Start from where the reactor is, not from zero.
                    let (f, t) = self.room.reactor().targets();
                    self.fission = f;
                    self.turbine = t;
                }
                self.manual = on;
            }
            ReactorAsk::SetFission(v) if self.manual => self.fission = v.clamp(0.0, 100.0),
            ReactorAsk::SetTurbine(v) if self.manual => self.turbine = v.clamp(0.0, 100.0),
            ReactorAsk::SetFission(_) | ReactorAsk::SetTurbine(_) => {}
            ReactorAsk::Trigger(ReactorFault::DrainCoolant) => {
                self.room.pump_mut().set_tank(0.0);
                info!("reactor: dev drain of the coolant tank");
            }
            ReactorAsk::Trigger(ReactorFault::WarmStart) => {
                let secs = self.room.warm_start();
                info!("reactor: warm start, settled after {secs:.1} s of sim time");
            }
            ReactorAsk::Trigger(ReactorFault::Aux) => {
                let on = !self.room.aux_enabled();
                self.room.set_aux(on);
                info!("reactor: dev aux reactor {}", if on { "on" } else { "off" });
            }
            ReactorAsk::Trigger(f) => {
                // The fire is started by `reactor_step`, which owns the air (`fault` is None for it).
                if let Some(f) = fault(f) {
                    let events = self.room.trigger(f);
                    info!("reactor: dev fault {f:?}");
                    for e in events {
                        route(e, counts);
                    }
                }
            }
            ReactorAsk::Reset => {
                // The alarm the old room set stays until the new room says otherwise.
                let alarmed = self.alarmed;
                let director = self.director.enabled();
                *self = ReactorSim { sent: None, since_send: f64::MAX, alarmed, ..ReactorSim::new(cfg) };
                // The dev toggle is the operator's: a reset keeps it.
                self.director.set_enabled(director);
                info!("reactor: reset");
            }
            ReactorAsk::ArmEject => {
                if !self.room.reactor().ejected() && self.eject_left.is_none() {
                    self.eject_left = Some(cfg.reactor.eject_arm_s);
                    info!("reactor: EJECT cover open for {} s", cfg.reactor.eject_arm_s);
                }
            }
            ReactorAsk::Eject => {
                if self.eject_left.take().is_some() {
                    for e in self.room.eject_core() {
                        route(e, counts);
                    }
                } else {
                    info!("reactor: eject refused (the cover is shut)");
                }
            }
            ReactorAsk::EmergencyCool => {
                if !self.room.emergency_cool() {
                    info!("reactor: emergency cooling refused (cooldown, tank or pump)");
                }
            }
            ReactorAsk::SetPower(on) => {
                if !self.room.set_power(on) {
                    info!("reactor: power on refused (broken)");
                }
            }
            ReactorAsk::SetBreaker(g, on) => self.room.set_breaker(group_of(g), on),
            ReactorAsk::SetAuxSwitch(g, on) => {
                // Engines is never aux-fed: the room ignores its switch.
                self.room.set_aux_switch(group_of(g), on);
            }
            ReactorAsk::SetLimit(g, v) => self.room.grid_mut().set_limit(group_of(g), v),
            ReactorAsk::SetBatteryRecharge(v) => self.room.battery_mut().set_recharge(v),
            ReactorAsk::SetVent(i, open) => match self.vents.get_mut(i as usize) {
                Some(v) => *v = open,
                None => info!("reactor: no vent {i}"),
            },
            ReactorAsk::SetPumpMode(m) => self.room.set_pump_mode(pump_of(m)),
            // The HELM's slider (r38): the engines' target, the room's clamp (-100..100) and the host's.
            ReactorAsk::SetThrottle(pct) => self.room.set_engine_throttle(pct.clamp(-100.0, 100.0)),
            ReactorAsk::SetDirector(on) => {
                self.director.set_enabled(on);
                info!("reactor: dev director {}", if on { "on" } else { "off" });
            }
            ReactorAsk::Boost(up) => {
                if !self.room.reactor_mut().boost(up) {
                    info!("reactor: boost refused (off, broken, or not allowed now)");
                }
            }
        }
    }

    /// The snapshot as the wire carries it; `oxygen` is the engine room's air (0..1).
    pub fn news(&self, oxygen: f64) -> ReactorNews {
        let r = self.room.reactor();
        let b = r.bands();
        let (set_fission, set_turbine) = r.targets();
        let grid = self.room.grid();
        ReactorNews {
            power_on: r.power_on(),
            manual: self.manual,
            melted: r.melted(),
            stability: (r.stability() * 100.0).round() as u8,
            temp: r.temperature(),
            set_fission,
            set_turbine,
            fission: r.fission(),
            turbine: r.turbine(),
            output_kw: r.output_kw(),
            load_kw: self.room.main_load_kw(),
            optimal: [b.temp_optimal.lo, b.temp_optimal.hi],
            allowed: [b.temp_allowed.lo, b.temp_allowed.hi],
            fission_optimal: [b.fission_optimal.lo, b.fission_optimal.hi],
            fission_allowed: [b.fission_allowed.lo, b.fission_allowed.hi],
            turbine_optimal: [b.turbine_optimal.lo, b.turbine_optimal.hi],
            turbine_allowed: [b.turbine_allowed.lo, b.turbine_allowed.hi],
            powered: self.room.powered(),
            breakers: Group::ALL.map(|g| grid.breaker(g)),
            limits: Group::ALL.map(|g| grid.limit(g)),
            delivered_kw: Group::ALL.map(|g| grid.supply(g).delivered_kw),
            demand_kw: Group::ALL.map(|g| grid.group(g).demand_kw),
            lights_factor: grid.factor(Group::Lights),
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
            overvolt: self.room.junction().overvolted(),
            // The host's blast count is set by `reactor_step` (the sim does not count).
            blasts: 0,
            oxygen,
            fuel: std::array::from_fn(|i| r.rods().get(i).map_or_else(FuelTube::default, |x| FuelTube { rod: Some(rods::fuel_kind(&x.kind)), life: x.life })),
            reactor: part(r.condition().fraction(), r.condition().functioning()),
            pump: part(self.room.pump().condition().fraction(), self.room.pump().condition().functioning()),
            junction: part(self.room.junction().condition().fraction(), self.room.junction().condition().functioning()),
            oxygen_generator: part(self.room.oxygen_generator().condition().fraction(), self.room.oxygen_generator().condition().functioning()),
            breach: self.room.breach().damage(),
            lamps: lamps_of(&self.room.warnings()),
            boost: r.temperature_boost(),
            boost_ok: r.power_on() && r.condition().functioning() && r.boost_allowed(),
            meltdown_eta_s: meltdown_eta(r.timers().1, r.tuning().meltdown_delay_s, r.melted()),
            ejected: r.ejected(),
            eject_armed: self.eject_left.is_some(),
            // The host's count is set by `reactor_step`, as the blasts are.
            ejects: 0,
            ecool_active: self.room.ecool_active(),
            ecool_ready_in_s: self.room.ecool_ready_in_s(),
            ecool_ok: self.room.ecool_allowed(),
            ecools: 0,
            // The host sets the rooms in `reactor_step` (they come from the air and the fire).
            rooms: Default::default(),
            vents: self.vents,
            throttle: self.room.engine_throttle(),
            engine_thrust: self.room.engine_thrust(),
            graph_points: self.room.history().size().min(GRAPH_POINTS) as u8,
            graph_output: graph_row(&self.room.history().output),
            graph_load: graph_row(&self.room.history().load),
            battery_charge: self.room.battery().fraction(),
            battery_recharge: self.room.battery().recharge(),
            battery_kw: self.room.battery().output_kw(),
            director: self.director.enabled(),
        }
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
        (Lamp::LowTemp, w.low_temp),
        (Lamp::HighOut, w.high_output),
        (Lamp::CritOut, w.critical_output),
        (Lamp::LowOut, w.low_output),
        (Lamp::FuelLow, w.low_fuel),
        (Lamp::NoFuel, w.fuel_out),
        (Lamp::Meltdown, w.meltdown),
        (Lamp::Scram, w.scram),
    ] {
        lamps[lamp.index()] = on;
    }
    lamps
}

/// Seconds to the implosion at the current rate: the meltdown timer (0..1 of its delay) has
/// `(1 - timer) * delay` left. None while the timer is at 0 and once the core has melted.
pub fn meltdown_eta(timer: f64, delay_s: f64, melted: bool) -> Option<f64> {
    (timer > 0.0 && !melted).then(|| (1.0 - timer.clamp(0.0, 1.0)) * delay_s)
}

/// Every process: a news counter (`ejects`, `ecools`) against the last one this process saw. True when
/// it went up (the notice shows); the first news only sets the baseline, so a late joiner does not
/// replay old events.
pub fn counter_fresh(seen: &mut Option<u32>, ejects: u32) -> bool {
    let fresh = matches!(*seen, Some(s) if ejects > s);
    *seen = Some(ejects);
    fresh
}

pub fn eject_notice(items: Res<Items>, mut seen: Local<Option<u32>>, mut gp: ResMut<Gameplay>) {
    let Some(news) = items.reactor else { return };
    if counter_fresh(&mut seen, news.ejects) {
        gp.notify(Notice::new(NoticeKind::Warning, "notice.reactor.eject"));
    }
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

fn fault(f: ReactorFault) -> Option<Fault> {
    match f {
        ReactorFault::PumpBreak => Some(Fault::PumpBreak),
        ReactorFault::OxygenGeneratorBreak => Some(Fault::OxygenGeneratorBreak),
        ReactorFault::LoadDrop => Some(Fault::LoadDrop),
        ReactorFault::Surge => Some(Fault::Surge),
        ReactorFault::HullBreach => Some(Fault::HullBreach),
        ReactorFault::Fire | ReactorFault::DrainCoolant | ReactorFault::WarmStart | ReactorFault::Aux => None,
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
        RoomEvent::Sparks => {
            counts.sparks += 1;
            info!("engine room: sparks at the junction box");
        }
        RoomEvent::Reactor(ReactorEvent::Meltdown) => {
            counts.meltdowns += 1;
            warn!("reactor: MELTDOWN");
        }
        RoomEvent::Reactor(ReactorEvent::Blast(_)) => {
            counts.blasts += 1;
        }
        RoomEvent::Reactor(ReactorEvent::CoreEjected) => {
            counts.ejects += 1;
            info!("reactor: CORE EJECTED");
        }
        RoomEvent::EmergencyCool => {
            counts.ecools += 1;
            info!("reactor: EMERGENCY COOLING");
        }
        RoomEvent::Device { source, event } => info!("engine room: {source:?} {event:?} (the machine's condition crossed a line)"),
        _ => {}
    }
}

pub fn ecool_notice(items: Res<Items>, mut seen: Local<Option<u32>>, mut gp: ResMut<Gameplay>) {
    let Some(news) = items.reactor else { return };
    if counter_fresh(&mut seen, news.ecools) {
        gp.notify(Notice::new(NoticeKind::Warning, "notice.reactor.ecool"));
    }
}

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let tuning = Tuning::load(&files).unwrap_or_else(|e| panic!("reactor: {e:?}"));
    let warm = app.world().contains_resource::<WarmAtCreation>();
    app.insert_resource(ReactorSim::created(&tuning, warm)).insert_resource(ReactorConfig(tuning)).init_resource::<ReactorCounts>();
    app.init_resource::<super::blast::PendingBlasts>().init_resource::<super::blast::BlastFlash>();
    app.add_systems(FixedUpdate, (reactor_step, super::blast::reactor_blast, super::blast::client_blast_news, eject_notice, ecool_notice).chain().before(super::engine_room::room_state).in_set(crate::phases::Fx::Input));
}

#[allow(clippy::too_many_arguments)]
fn reactor_step(time: Res<Time>, mut items: ResMut<Items>, cfg: Res<ReactorConfig>, mut sim: ResMut<ReactorSim>, mut counts: ResMut<ReactorCounts>, mut alarm: ResMut<ShipAlarm>, mut air: ResMut<EngineAir>, fires: Res<FireConfig>, fire_alarm: Res<FireAlarm>, mut grid: ResMut<FireGrid>, model: Res<FireModel>, area: Res<DevArea>, planet: Res<PlanetRes>, repair: Res<crate::repair::RepairConfig>, mut health: Option<ResMut<crate::health::HealthState>>, mut blasts: ResMut<super::blast::PendingBlasts>) {
    let asks = std::mem::take(&mut items.reactor_asks);
    if items.role == Role::Client {
        for a in asks {
            items.net_out.push(NetOut::ReactorAsk(a));
        }
        return;
    }
    let mut events = Vec::new();
    for a in asks {
        if a == ReactorAsk::Trigger(ReactorFault::Fire) {
            info!("reactor: dev fire");
            events.push(RoomEvent::Fire { size: fires.0.start_size, source: Source::Reactor });
        } else {
            if a == ReactorAsk::Reset {
                // The fires are the air's: a fresh room starts with a fresh air (no fire survives).
                *air = EngineAir::new();
            }
            sim.apply(a, &cfg.0, &mut counts);
        }
    }
    for _ in 0..items.take_refills() {
        let added = sim.room.pump_mut().refill(cfg.0.room.coolant_canister);
        info!("reactor: coolant canister, the tank gains {added:.0} %");
    }
    for player in std::mem::take(&mut items.pull_asks) {
        let Some(i) = rods::pull_index(sim.room.reactor().rods()) else {
            info!("reactor: the brake, no rod in a slot");
            continue;
        };
        let Some((rod, temp)) = sim.room.reactor_mut().pull_rod(i) else { continue };
        let band = sim.room.reactor().bands().temp_optimal;
        if temp > band.hi {
            let burn = device_core::minigame::hurt_of(device_core::minigame::Machine::Hot, device_core::minigame::Press::Miss, &repair.0).burn;
            if let Some(st) = health.as_deref_mut()
                && let Some(b) = st.bodies.get_mut(&player)
            {
                b.health.add(&st.tuning, "burn", burn);
            }
        }
        info!("reactor: the brake pulled the {} rod ({:.0} life) at {temp:.0} degrees", rod.kind, rod.life);
        let Some(def) = rods::item_of_kind(&rod.kind).and_then(|id| items.defs.get(id)).cloned() else { continue };
        let now = time.elapsed_secs_f64();
        if let Err(stack) = items.give(now, player, rods::rod_stack(&def, rod.life))
            && let Some(at) = rods::reactor_front(&area, &planet)
        {
            items.spawn(now, stack, at);
        }
    }
    // Fuel rods the host took out of a hand (`dev_area::rods`): each goes into a free slot. A reactor
    for load in items.take_rod_loads() {
        let Some(mut rod) = rods::kind_of_item(&load.item).and_then(|k| cfg.0.rods.rod(k)) else { continue };
        rod.life = load.life;
        let kind = rod.kind.clone();
        if let Err(rod) = sim.room.reactor_mut().insert_rod(rod) {
            info!("reactor: no free slot for the {} rod, it goes back", rod.kind);
            let Some(def) = items.defs.get(&load.item).cloned() else { continue };
            if let Err(stack) = items.give(time.elapsed_secs_f64(), load.player, rods::rod_stack(&def, rod.life)) {
                if let Some(at) = rods::reactor_front(&area, &planet) {
                    items.spawn(time.elapsed_secs_f64(), stack, at);
                }
            }
        } else {
            info!("reactor: the {kind} rod is in a slot ({:.0} life)", load.life);
        }
    }
    let dt = time.delta_secs_f64();
    sim.tick_eject(dt);
    let input = if sim.manual { Input::manual(sim.fission, sim.turbine) } else { Input::auto() };
    events.extend(sim.room.step(dt, input));
    // The incident director (r38): the host's only. Its report is the room's devices and temperature,
    // the fires in the two rooms and the engine room's oxygen; it breaks at most one device here.
    let fires = match *model {
        FireModel::Point => [air.engine, air.corridor].into_iter().map(|room| air.deck.fires_in(room).count()).sum::<usize>(),
        FireModel::Grid => grid.0.burning().count(),
    } as u32;
    let situation = Situation::of_room(&sim.room, fires, air.deck.room(air.engine).oxygen_fraction());
    if let Some(f) = sim.director.step(dt, &situation) {
        info!("reactor: director {f:?}");
        events.extend(sim.room.trigger(f));
    }
    // Life support's air: the oxygen generator feeds the air through its vents (r38; the engine room's
    // and the corridor's, `EngineAir::add_oxygen_through_vents`). `EngineRoom::oxygen_flow` is the flow:
    // the life support's power and the generator's condition squared (`fire_core::oxygen`).
    let o2 = sim.room.oxygen_flow(cfg.0.room.oxygen_generator_rate);
    air.add_oxygen_through_vents(o2 * dt, sim.vents);
    // The hull breach (r37): its hole leaks the room's air to space at the breach's openness.
    let breach = sim.room.breach().open();
    let hole = air.breach;
    air.deck.set_leak(hole, cfg.0.room.breach_area_m2, breach);
    for e in events {
        if let Some((source, size)) = fire::start_of(&e) {
            fire::ignite(&mut air, &mut grid, *model, source, size);
        }
        if let RoomEvent::Reactor(ReactorEvent::Blast(b)) = e {
            blasts.0.push(b);
        }
        route(e, &mut counts);
    }
    let room_level = alarm_of(sim.room.level());
    let level = room_level.max(fire_alarm.0);
    if level != Alarm::None || sim.alarmed {
        alarm.level = level;
    }
    sim.alarmed = level != Alarm::None;

    let oxygen = (air.oxygen_at(ROOM.centre) * 100.0).round() / 100.0;
    let mut now = sim.news(oxygen);
    now.blasts = counts.blasts;
    now.ejects = counts.ejects;
    now.ecools = counts.ecools;
    // The status monitor's rooms (r38): the engine room first, then the corridor.
    now.rooms = [air.engine, air.corridor].map(|room| RoomAir {
        o2: (air.deck.room(room).oxygen_fraction() * 100.0).round() / 100.0,
        fire: match *model {
            FireModel::Point => air.deck.fires_in(room).next().is_some(),
            FireModel::Grid => grid.0.burning().any(|c| c.room == room),
        },
    });
    items.reactor = Some(now);
    sim.since_send += dt;
    if items.role == Role::Host && sim.sent != Some(now) && sim.since_send >= SEND_EVERY_S {
        items.net_out.push(NetOut::Reactor { to: None, r: now });
        sim.sent = Some(now);
        sim.since_send = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Tuning {
        Tuning::default()
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
        s.apply(ReactorAsk::SetPower(false), &cfg(), &mut k);
        let n = s.news(1.0);
        assert!(n.lamps[Lamp::Scram.index()], "warm with the power off");
        assert!(!n.lamps[Lamp::Hot.index()] && !n.lamps[Lamp::Meltdown.index()]);
        assert!(!n.boost_ok, "no boost with the power off");
        assert_eq!(n.meltdown_eta_s, None);
        s.apply(ReactorAsk::SetPower(true), &cfg(), &mut k);
        s.apply(ReactorAsk::Boost(true), &cfg(), &mut k);
        let amount = s.room.reactor().tuning().boost_amount;
        let n = s.news(1.0);
        assert_eq!(n.boost, amount, "the boost is up");
        assert!(!n.boost_ok, "a boost at its amount is not allowed again");
        s.apply(ReactorAsk::Boost(false), &cfg(), &mut k);
        assert_eq!(s.news(1.0).boost, amount, "refused: the boost stays up");
    }

    #[test]
    fn the_boost_ask_is_refused_when_the_core_refuses_it() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::SetPower(false), &cfg(), &mut k);
        s.apply(ReactorAsk::Boost(true), &cfg(), &mut k);
        assert_eq!(s.room.reactor().temperature_boost(), 0.0, "the power is needed");
        assert_eq!(s.news(1.0).boost, 0.0);
    }

    #[test]
    fn the_limit_ask_sets_the_group_limit_and_the_news_carry_the_kw() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::SetLimit(BreakerGroup::Engines, 0.5), &cfg(), &mut k);
        let n = s.news(1.0);
        assert_eq!(n.limits, [0.5, 1.0, 1.0, 1.0, 1.0, 1.0]);
        let demand = s.room.grid().group(Group::Engines).demand_kw;
        assert_eq!(n.demand_kw[0], demand, "the demand is the full demand, not the limited request");
        assert!((n.delivered_kw[0] - s.room.grid().supply(Group::Engines).delivered_kw).abs() < 1e-9);
        s.apply(ReactorAsk::SetLimit(BreakerGroup::Lights, 7.0), &cfg(), &mut k);
        assert_eq!(s.news(1.0).limits[2], 1.0, "the host clamps it to 0..1");
    }

    #[test]
    fn power_off_and_the_power_switch_ask_the_room() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::SetPower(false), &cfg(), &mut k);
        // The output stops with the next step; fission follows with its lag (seconds).
        for _ in 0..10 {
            s.room.step(0.1, Input::auto());
        }
        let n = s.news(1.0);
        assert!(!n.power_on && !n.powered && n.output_kw == 0.0, "{n:?}");
        for _ in 0..200 {
            s.room.step(0.1, Input::auto());
        }
        assert!(s.news(1.0).temp < 40.0, "the temperature falls");
        s.apply(ReactorAsk::SetPower(true), &cfg(), &mut k);
        assert!(s.news(1.0).power_on);
    }

    #[test]
    fn breakers_and_the_pump_switch_are_on_the_snapshot() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        let before = s.news(1.0).load_kw;
        s.apply(ReactorAsk::SetBreaker(BreakerGroup::Lights, false), &cfg(), &mut k);
        let n = s.news(1.0);
        assert_eq!(n.breakers, [true, true, false, true, true, true]);
        let lights = s.room.grid().group(Group::Lights).demand_kw;
        assert!((before - n.load_kw - lights).abs() < 1e-9, "the load drops by the lights' demand");
        s.apply(ReactorAsk::SetPumpMode(PumpMode::Off), &cfg(), &mut k);
        settle(&mut s, |n| n.flow < 1e-3);
        let n = s.news(1.0);
        assert!(!n.pump_on && n.flow < 1e-3, "the pump spins down");
        s.apply(ReactorAsk::SetPumpMode(PumpMode::High), &cfg(), &mut k);
        settle(&mut s, |n| n.flow >= 0.99);
        assert!(s.news(1.0).pump_on && s.news(1.0).flow >= 0.99);
    }

    #[test]
    fn a_drain_empties_the_tank_and_the_news_says_so() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        assert!(s.news(0.0).coolant > 99.0, "a new room has a full tank");
        s.apply(ReactorAsk::Trigger(ReactorFault::DrainCoolant), &cfg(), &mut k);
        let n = s.news(0.0);
        assert_eq!(n.coolant, 0.0);
        assert!(n.coolant_low && n.flow == 0.0, "an empty tank is no flow");
    }

    #[test]
    fn the_aux_fault_flips_the_aux_reactor_and_the_news_follow() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        assert!(!s.room.aux_enabled());
        s.apply(ReactorAsk::Trigger(ReactorFault::Aux), &cfg(), &mut k);
        assert!(s.room.aux_enabled(), "the first press turns the aux reactor on");
        assert!(s.news(0.2).aux_on);
        s.apply(ReactorAsk::Trigger(ReactorFault::Aux), &cfg(), &mut k);
        assert!(!s.room.aux_enabled(), "the second press turns it off");
        assert!(!s.news(0.2).aux_on);
    }

    #[test]
    fn the_aux_switch_asks_reach_the_room_and_engines_is_not_aux_fed() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::Trigger(ReactorFault::Aux), &cfg(), &mut k);
        s.apply(ReactorAsk::SetAuxSwitch(BreakerGroup::Lights, false), &cfg(), &mut k);
        assert!(!s.room.aux_switch(Group::Lights));
        assert!(!s.news(0.2).aux_lights);
        s.apply(ReactorAsk::SetAuxSwitch(BreakerGroup::Engines, false), &cfg(), &mut k);
        assert!(s.room.aux_switch(Group::Engines), "Engines reads on whatever it is asked");
    }

    #[test]
    fn the_warm_start_fault_settles_the_room_and_costs_no_coolant_or_rod_life() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::Trigger(ReactorFault::DrainCoolant), &cfg(), &mut k);
        s.room.pump_mut().refill(100.0);
        let tank = s.room.pump().tank();
        let lives = s.room.reactor().rod_lives();
        s.apply(ReactorAsk::Trigger(ReactorFault::WarmStart), &cfg(), &mut k);
        assert!(s.room.settled(), "the room is settled after the warm start");
        assert_eq!(s.room.pump().tank(), tank);
        assert_eq!(s.room.reactor().rod_lives(), lives);
        assert_eq!(k.fires, 0);
    }

    #[test]
    fn a_reset_refills_the_tank() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::Trigger(ReactorFault::DrainCoolant), &cfg(), &mut k);
        s.apply(ReactorAsk::Reset, &cfg(), &mut k);
        assert!(s.news(0.0).coolant > 99.0);
    }

    #[test]
    fn a_melted_core_is_on_the_snapshot_and_a_reset_clears_it() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.room.reactor_mut().set_targets(100.0, 30.0);
        for _ in 0..(200.0 / 0.1) as usize {
            s.room.step(0.1, Input::manual(100.0, 30.0));
            if s.room.reactor().melted() {
                break;
            }
        }
        assert!(s.news(1.0).melted);
        s.apply(ReactorAsk::Reset, &cfg(), &mut k);
        assert!(!s.news(1.0).melted);
    }

    #[test]
    fn targets_count_only_on_manual_and_manual_starts_from_where_the_reactor_is() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::SetFission(70.0), &cfg(), &mut k);
        assert!(!s.manual && s.fission == 0.0, "ignored on Auto");
        for _ in 0..30 * 60 {
            s.room.step(1.0 / 30.0, Input::auto());
        }
        let (f, t) = s.room.reactor().targets();
        s.apply(ReactorAsk::SetManual(true), &cfg(), &mut k);
        assert!(s.manual && (s.fission - f).abs() < 1e-9 && (s.turbine - t).abs() < 1e-9);
        s.apply(ReactorAsk::SetFission(150.0), &cfg(), &mut k);
        assert_eq!(s.fission, 100.0, "clamped");
    }

    #[test]
    fn the_emergency_cooling_press_takes_the_tank_once_and_counts_once() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        let full = s.room.pump().tank();
        s.apply(ReactorAsk::EmergencyCool, &cfg(), &mut k);
        let news = s.news(1.0);
        assert!(news.ecool_active && news.ecool_ready_in_s > 0.0 && !news.ecool_ok, "{news:?}");
        assert!(s.room.pump().tank() < full, "the press takes the tank cost");
        let tank = s.room.pump().tank();
        s.apply(ReactorAsk::EmergencyCool, &cfg(), &mut k);
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
        s.apply(ReactorAsk::Trigger(ReactorFault::PumpBreak), &cfg(), &mut k);
        s.apply(ReactorAsk::Reset, &cfg(), &mut k);
        assert!(s.alarmed, "the ship's alarm is cleared by the next step, not by the reset");
        assert!(s.room.pump().condition().functioning() && s.room.reactor().power_on() && !s.manual);
    }

    #[test]
    fn the_eject_needs_the_cover_open_and_a_second_press_inside_its_window() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::Eject, &cfg(), &mut k);
        assert!(!s.room.reactor().ejected(), "a press with the cover shut does nothing");
        s.apply(ReactorAsk::ArmEject, &cfg(), &mut k);
        assert!(s.news(1.0).eject_armed && !s.news(1.0).ejected);
        s.tick_eject(4.0);
        assert!(s.news(1.0).eject_armed, "still inside the 5 s window");
        s.apply(ReactorAsk::Eject, &cfg(), &mut k);
        assert!(s.room.reactor().ejected() && s.news(1.0).ejected && !s.news(1.0).eject_armed);
        assert_eq!(k.ejects, 1);
        assert!(s.room.reactor().rods().is_empty());
    }

    #[test]
    fn the_cover_shuts_again_after_its_window_and_the_eject_is_refused() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::ArmEject, &cfg(), &mut k);
        s.tick_eject(5.1);
        assert!(!s.news(1.0).eject_armed, "the cover shut after 5 s");
        s.apply(ReactorAsk::Eject, &cfg(), &mut k);
        assert!(!s.room.reactor().ejected() && k.ejects == 0);
    }

    #[test]
    fn a_second_arm_does_not_restart_the_window() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::ArmEject, &cfg(), &mut k);
        s.tick_eject(3.0);
        s.apply(ReactorAsk::ArmEject, &cfg(), &mut k);
        s.tick_eject(2.5);
        assert!(!s.news(1.0).eject_armed, "the first window ran out; the second press did not extend it");
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
        s.apply(ReactorAsk::SetThrottle(-35.0), &cfg(), &mut k);
        assert_eq!(s.news(1.0).throttle, -35.0);
        s.apply(ReactorAsk::SetThrottle(400.0), &cfg(), &mut k);
        assert_eq!(s.news(1.0).throttle, 100.0, "clamped");
    }

    /// r38: each vent is a valve the host keeps; a bad index is ignored and the news carries both.
    #[test]
    fn the_vent_asks_set_the_valves_and_the_news_carry_them() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        assert_eq!(s.news(1.0).vents, [true, true], "both open at the start");
        s.apply(ReactorAsk::SetVent(0, false), &cfg(), &mut k);
        assert_eq!(s.news(1.0).vents, [false, true]);
        s.apply(ReactorAsk::SetVent(1, false), &cfg(), &mut k);
        assert_eq!(s.news(1.0).vents, [false, false], "all shut is a state too");
        s.apply(ReactorAsk::SetVent(2, true), &cfg(), &mut k);
        assert_eq!(s.news(1.0).vents, [false, false], "no third vent");
        s.apply(ReactorAsk::SetVent(1, true), &cfg(), &mut k);
        assert_eq!(s.news(1.0).vents, [false, true]);
    }

    #[test]
    fn the_events_are_counted() {
        let mut k = ReactorCounts::default();
        route(RoomEvent::Sparks, &mut k);
        route(RoomEvent::Reactor(ReactorEvent::Fire { size: 0.1 }), &mut k);
        route(RoomEvent::Fire { size: 0.05, source: reactor_core::Source::Junction }, &mut k);
        route(RoomEvent::Reactor(ReactorEvent::Meltdown), &mut k);
        assert_eq!(k, ReactorCounts { fires: 2, sparks: 1, meltdowns: 1, blasts: 0, ejects: 0, ecools: 0 });
        route(RoomEvent::Reactor(ReactorEvent::Blast(reactor_core::Blast::default())), &mut k);
        assert_eq!(k.blasts, 1, "the blast is counted");
    }
}

fn part(share: f64, working: bool) -> PartNews {
    PartNews { share, working }
}
