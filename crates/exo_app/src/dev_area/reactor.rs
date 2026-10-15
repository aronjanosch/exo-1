//! The reactor (#297, slice S3b): the host steps the engine room's `EngineRoom` each fixed tick,
//! on Auto by default with the power on. Clients send their requests (`ReactorAsk`: manual or
//! Auto, fission and turbine targets, a dev fault, a reset) and take the host's snapshot
//! (`ReactorNews`, about 4 times a second while it changes) into `Items::reactor`.
//!
//! - Values: `content/gameplay/reactor/reactor.json` (`reactor_core::Tuning`, strict loader).
//! - No automatic safety: SCRAM (`ReactorAsk::Scram`) cuts fission and the power; the power switch
//!   starts it again. Breakers and the pump's switch are asks too.
//! - The room's alarm drives `ShipAlarm` while the reactor is not calm (Warn, Danger); while it is
//!   calm the test console's alarm button may set it.
//! - Fires, sparks and meltdowns are logged (`info!`/`warn!`) and counted in `ReactorCounts` for
//!   the scenarios. A fire starts in the engine room's air (`fire`, #298); the meltdown's effects
//!   come with their slice. The dev faults screen's Fire asks for a fire at the reactor.
//! - The readout and the dev fault screen are `reactor_screen`.
use super::*;
use crate::dev_area::engine_room::{EngineAir, ShipAlarm};
use crate::dev_area::fire::{self, FireAlarm, FireConfig, FireGrid, FireModel};
use crate::items::{Items, NetOut, Role};
use crate::dev_area::engine_room::ROOM;
use crate::dev_area::rods;
use item_core::wire::{Alarm, BreakerGroup, FuelTube, Lamp, PartNews, PumpMode, ReactorAsk, ReactorFault, ReactorNews, GRAPH_POINTS};
use reactor_core::{AlarmLevel, EngineRoom, Fault, Group, Input, ReactorEvent, RoomEvent, Source, Tuning, Warnings};

/// Seconds between two sends of a changed snapshot (4 Hz).
pub const SEND_EVERY_S: f64 = 0.25;
/// The room's seed: the same room every start (its rolls are seeded).
pub const SEED: u64 = 0x297;

/// The loaded values.
#[derive(Resource, Clone, Debug)]
pub struct ReactorConfig(pub Tuning);

/// What the reactor's events added up to on this host (scenarios and logs read it).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReactorCounts {
    pub fires: u32,
    pub sparks: u32,
    pub meltdowns: u32,
    /// The meltdown blasts (`dev_area::blast`).
    pub blasts: u32,
}

/// The host's reactor: the room, the manual targets, and what was last sent.
#[derive(Resource, Debug)]
pub struct ReactorSim {
    pub room: EngineRoom,
    /// Manual when true: the room gets `fission` and `turbine` each step; Auto otherwise.
    pub manual: bool,
    pub fission: f64,
    pub turbine: f64,
    /// Whether the room's alarm had been on (Warn or Danger) at the last step.
    alarmed: bool,
    sent: Option<ReactorNews>,
    since_send: f64,
}

/// `--warm-reactor`: the room is warm-started when it is created (`ReactorSim::created`), not run 150 s
/// on Auto.
#[derive(Resource)]
pub struct WarmAtCreation;

impl ReactorSim {
    /// A room on Auto with the power on, already warmed up (a cold start takes 60 to 90 s).
    pub fn new(cfg: &Tuning) -> ReactorSim {
        ReactorSim::created(cfg, false)
    }

    /// A new room, warmed up by `EngineRoom::warm_start` when `warm`, else by 150 s on Auto.
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
        ReactorSim { room, manual: false, fission: 0.0, turbine: 0.0, alarmed: false, sent: None, since_send: f64::MAX }
    }

    /// One request, applied on this host. Set fission and turbine only count on Manual.
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
                *self = ReactorSim { sent: None, since_send: f64::MAX, alarmed, ..ReactorSim::new(cfg) };
                info!("reactor: reset");
            }
            ReactorAsk::Scram => {
                self.room.reactor_mut().scram();
                info!("reactor: SCRAM");
            }
            ReactorAsk::SetPower(on) => {
                if !self.room.set_power(on) {
                    info!("reactor: power on refused (broken)");
                }
            }
            ReactorAsk::SetBreaker(g, on) => self.room.set_breaker(group_of(g), on),
            ReactorAsk::SetPumpMode(m) => self.room.set_pump_mode(pump_of(m)),
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
            temp: r.temperature(),
            set_fission,
            set_turbine,
            output_kw: r.output_kw(),
            load_kw: grid.request_kw(),
            optimal: [b.temp_optimal.lo, b.temp_optimal.hi],
            allowed: [b.temp_allowed.lo, b.temp_allowed.hi],
            powered: self.room.powered(),
            breakers: Group::ALL.map(|g| grid.breaker(g)),
            pump_on: self.room.pump().on(),
            pump_mode: wire_pump(self.room.pump().mode()),
            pump_speed: self.room.pump().speed(),
            flow: self.room.flow(),
            coolant: self.room.pump().tank(),
            coolant_low: self.room.pump().tank() < self.room.tuning().coolant_low,
            overvolt: self.room.junction().overvolted(),
            // The host's blast count is set by `reactor_step` (the sim does not count).
            blasts: 0,
            oxygen,
            fuel: std::array::from_fn(|i| r.rods().get(i).map_or_else(FuelTube::default, |x| FuelTube { rod: Some(rods::fuel_kind(&x.kind)), life: x.life })),
            reactor: part(r.condition().fraction(), r.condition().functioning()),
            pump: part(self.room.pump().condition().fraction(), self.room.pump().condition().functioning()),
            junction: part(self.room.junction().condition().fraction(), self.room.junction().condition().functioning()),
            lamps: lamps_of(&self.room.warnings()),
            boost: r.temperature_boost(),
            boost_ok: r.power_on() && r.condition().functioning() && r.boost_allowed(),
            meltdown_eta_s: meltdown_eta(r.timers().1, r.tuning().meltdown_delay_s, r.melted()),
            graph_points: self.room.history().size().min(GRAPH_POINTS) as u8,
            graph_output: graph_row(&self.room.history().output),
            graph_load: graph_row(&self.room.history().load),
        }
    }
}

/// The first `GRAPH_POINTS` of a graph row (newest first), in whole kW.
fn graph_row(row: &[f64]) -> [u16; GRAPH_POINTS] {
    std::array::from_fn(|i| row.get(i).map_or(0, |v| v.round().clamp(0.0, f64::from(u16::MAX)) as u16))
}

/// The lamps of the core's warnings, by `Lamp::index`.
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

/// Whether the blinking lamps are lit at a time: 2 Hz, lit for the first quarter of each half
/// second (Barotrauma's `TotalTime % 0.5 < 0.25`).
pub fn lamp_blink_on(t: f64) -> bool {
    t.rem_euclid(0.5) < 0.25
}

/// The grid's group of a wire breaker.
pub fn group_of(g: BreakerGroup) -> Group {
    match g {
        BreakerGroup::Engines => Group::Engines,
        BreakerGroup::LifeSupport => Group::LifeSupport,
        BreakerGroup::Lights => Group::Lights,
    }
}

/// The wire's pump mode as the room's.
pub fn pump_of(m: PumpMode) -> reactor_core::engine_room::PumpMode {
    use reactor_core::engine_room::PumpMode as Core;
    match m {
        PumpMode::Off => Core::Off,
        PumpMode::Low => Core::Low,
        PumpMode::High => Core::High,
    }
}

/// The room's pump mode on the wire.
pub fn wire_pump(m: reactor_core::engine_room::PumpMode) -> PumpMode {
    use reactor_core::engine_room::PumpMode as Core;
    match m {
        Core::Off => PumpMode::Off,
        Core::Low => PumpMode::Low,
        Core::High => PumpMode::High,
    }
}

/// The room's fault for a dev request; None for the fire, which the air takes.
fn fault(f: ReactorFault) -> Option<Fault> {
    match f {
        ReactorFault::PumpBreak => Some(Fault::PumpBreak),
        ReactorFault::LoadDrop => Some(Fault::LoadDrop),
        ReactorFault::Surge => Some(Fault::Surge),
        ReactorFault::Fire | ReactorFault::DrainCoolant | ReactorFault::WarmStart => None,
    }
}

/// The room's alarm as the ship's alarm.
pub fn alarm_of(l: AlarmLevel) -> Alarm {
    match l {
        AlarmLevel::None => Alarm::None,
        AlarmLevel::Warn => Alarm::Warn,
        AlarmLevel::Danger => Alarm::Danger,
    }
}

/// Logs the events and counts them; the fires start in the air (`fire::start_of`), not here.
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
        RoomEvent::Device { source, event } => info!("engine room: {source:?} {event:?} (the machine's condition crossed a line)"),
        _ => {}
    }
}

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let tuning = Tuning::load(&files).unwrap_or_else(|e| panic!("reactor: {e:?}"));
    let warm = app.world().contains_resource::<WarmAtCreation>();
    app.insert_resource(ReactorSim::created(&tuning, warm)).insert_resource(ReactorConfig(tuning)).init_resource::<ReactorCounts>();
    app.init_resource::<super::blast::PendingBlasts>().init_resource::<super::blast::BlastFlash>();
    app.add_systems(FixedUpdate, (reactor_step, super::blast::reactor_blast, super::blast::client_blast_news).chain().before(super::engine_room::room_state).in_set(crate::phases::Fx::Input));
}

/// Host and one process: the requests, one step, the events (fires start in the air), the ship's
/// alarm (the room's or the fire's, the higher), the snapshot out. A client only forwards its
/// requests.
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
    // Canisters the host applied (`dev_area::coolant`): each refills the tank by a canister's worth.
    for _ in 0..items.take_refills() {
        let added = sim.room.pump_mut().refill(cfg.0.room.coolant_canister);
        info!("reactor: coolant canister, the tank gains {added:.0} %");
    }
    // The brake (`Items::pull_asks`, by the sender): the rod with the most life comes out into the puller's hands,
    // and the core's heat above its optimal band burns the hand once (the reactor's miss burn, as
    // repair's miss does it, `hurt_of`). The rod keeps its kind and life.
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
    // with no free slot gives it back to its owner (the client checks first, so this is a race).
    for load in items.take_rod_loads() {
        let Some(mut rod) = rods::kind_of_item(&load.item).and_then(|k| cfg.0.rods.rod(k)) else { continue };
        rod.life = load.life;
        let kind = rod.kind.clone();
        if let Err(rod) = sim.room.reactor_mut().insert_rod(rod) {
            info!("reactor: no free slot for the {} rod, it goes back", rod.kind);
            let Some(def) = items.defs.get(&load.item).cloned() else { continue };
            if let Err(stack) = items.give(time.elapsed_secs_f64(), load.player, rods::rod_stack(&def, rod.life)) {
                // The hands are full too: the rod lies at the reactor's front.
                if let Some(at) = rods::reactor_front(&area, &planet) {
                    items.spawn(time.elapsed_secs_f64(), stack, at);
                }
            }
        } else {
            info!("reactor: the {kind} rod is in a slot ({:.0} life)", load.life);
        }
    }
    let dt = time.delta_secs_f64();
    let input = if sim.manual { Input::manual(sim.fission, sim.turbine) } else { Input::auto() };
    events.extend(sim.room.step(dt, input));
    for e in events {
        if let Some((source, size)) = fire::start_of(&e) {
            fire::ignite(&mut air, &mut grid, *model, source, size);
        }
        // The blast's rules run in `blast::reactor_blast` (the bodies, the fires, the news).
        if let RoomEvent::Reactor(ReactorEvent::Blast(b)) = e {
            blasts.0.push(b);
        }
        route(e, &mut counts);
    }
    // Not calm: the room's alarm (or the fire's, the higher) is the ship's. Calm after an alarm:
    // back to none.
    let room_level = alarm_of(sim.room.level());
    let level = room_level.max(fire_alarm.0);
    if level != Alarm::None || sim.alarmed {
        alarm.level = level;
    }
    sim.alarmed = level != Alarm::None;

    let oxygen = (air.oxygen_at(ROOM.centre) * 100.0).round() / 100.0;
    let mut now = sim.news(oxygen);
    now.blasts = counts.blasts;
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

    /// Steps the room until the snapshot says so (the pump's speed moves with inertia), at most 300 s.
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
        // Warm with the power off after SCRAM: the SCRAM lamp is on, the others off.
        s.apply(ReactorAsk::Scram, &cfg(), &mut k);
        let n = s.news(1.0);
        assert!(n.lamps[Lamp::Scram.index()], "warm with the power off");
        assert!(!n.lamps[Lamp::Hot.index()] && !n.lamps[Lamp::Meltdown.index()]);
        assert!(!n.boost_ok, "no boost with the power off");
        assert_eq!(n.meltdown_eta_s, None);
        // Power on: the boost passes; a boost that is up at its amount refuses the next press.
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
        s.apply(ReactorAsk::Scram, &cfg(), &mut k);
        s.apply(ReactorAsk::Boost(true), &cfg(), &mut k);
        assert_eq!(s.room.reactor().temperature_boost(), 0.0, "the power is needed");
        assert_eq!(s.news(1.0).boost, 0.0);
    }

    #[test]
    fn the_meltdown_countdown_is_the_seconds_left_while_the_timer_runs() {
        assert_eq!(meltdown_eta(0.0, 120.0, false), None, "no timer, no countdown");
        assert_eq!(meltdown_eta(0.25, 120.0, false), Some(90.0), "a quarter run: 90 s left");
        assert_eq!(meltdown_eta(0.25, 120.0, true), None, "melted: the view says so, no countdown");
    }

    #[test]
    fn the_lamps_blink_at_two_hertz_and_scram_does_not() {
        // On for a quarter of each half second (Barotrauma's TotalTime % 0.5 < 0.25).
        assert!(lamp_blink_on(0.0) && lamp_blink_on(0.2));
        assert!(!lamp_blink_on(0.25) && !lamp_blink_on(0.49));
        assert!(lamp_blink_on(0.5));
    }

    #[test]
    fn scram_and_the_power_switch_ask_the_room() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::Scram, &cfg(), &mut k);
        let n = s.news(1.0);
        assert!(!n.power_on && !n.powered && n.output_kw == 0.0, "{n:?}");
        for _ in 0..60 {
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
        assert_eq!(n.breakers, [true, true, false]);
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
    fn the_pump_mode_is_on_the_snapshot_and_low_runs_between_off_and_high() {
        let mut s = ReactorSim::new(&cfg());
        let mut k = ReactorCounts::default();
        s.apply(ReactorAsk::SetPumpMode(PumpMode::Low), &cfg(), &mut k);
        let n = s.news(1.0);
        assert!(n.pump_on && n.pump_mode == PumpMode::Low);
        for _ in 0..1200 {
            s.room.step(0.1, Input::auto());
        }
        let speed = s.news(1.0).pump_speed;
        assert!(speed > 0.2 && speed < 0.8, "Low settles between Off and High: {speed}");
        s.apply(ReactorAsk::SetPumpMode(PumpMode::Off), &cfg(), &mut k);
        assert!(!s.news(1.0).pump_on && s.news(1.0).pump_mode == PumpMode::Off);
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
    fn a_new_reactor_is_on_auto_with_the_power_on() {
        let s = ReactorSim::new(&cfg());
        assert!(!s.manual && s.room.reactor().power_on());
        assert!(!s.news(1.0).melted && s.news(1.0).powered);
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
    fn the_events_are_counted() {
        let mut k = ReactorCounts::default();
        route(RoomEvent::Sparks, &mut k);
        route(RoomEvent::Reactor(ReactorEvent::Fire { size: 0.1 }), &mut k);
        route(RoomEvent::Fire { size: 0.05, source: reactor_core::Source::Junction }, &mut k);
        route(RoomEvent::Reactor(ReactorEvent::Meltdown), &mut k);
        assert_eq!(k, ReactorCounts { fires: 2, sparks: 1, meltdowns: 1, blasts: 0 });
        route(RoomEvent::Reactor(ReactorEvent::Blast(reactor_core::Blast::default())), &mut k);
        assert_eq!(k.blasts, 1, "the blast is counted");
    }
}

fn part(share: f64, working: bool) -> PartNews {
    PartNews { share, working }
}
