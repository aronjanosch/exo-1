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
use item_core::wire::{Alarm, BreakerGroup, PartNews, ReactorAsk, ReactorFault, ReactorNews};
use reactor_core::{AlarmLevel, EngineRoom, Fault, Group, Input, ReactorEvent, RoomEvent, Source, Tuning};

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

impl ReactorSim {
    /// A room on Auto with the power on, already warmed up (a cold start takes 60 to 90 s).
    pub fn new(cfg: &Tuning) -> ReactorSim {
        let mut room = EngineRoom::from_tuning(cfg, SEED);
        room.set_power(true);
        for _ in 0..1500 {
            let _ = room.step(0.1, Input::auto());
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
            ReactorAsk::SetPump(on) => self.room.set_pump(on),
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
            flow: self.room.flow(),
            coolant: self.room.pump().tank(),
            coolant_low: self.room.pump().tank() < self.room.tuning().coolant_low,
            overvolt: self.room.junction().overvolted(),
            oxygen,
            fuel: [0, 1].map(|i| r.rods().get(i).map_or(0.0, |x| x.life)),
            reactor: part(r.condition().fraction(), r.condition().functioning()),
            pump: part(self.room.pump().condition().fraction(), self.room.pump().condition().functioning()),
            junction: part(self.room.junction().condition().fraction(), self.room.junction().condition().functioning()),
        }
    }
}

/// The grid's group of a wire breaker.
pub fn group_of(g: BreakerGroup) -> Group {
    match g {
        BreakerGroup::Engines => Group::Engines,
        BreakerGroup::LifeSupport => Group::LifeSupport,
        BreakerGroup::Lights => Group::Lights,
    }
}

/// The room's fault for a dev request; None for the fire, which the air takes.
fn fault(f: ReactorFault) -> Option<Fault> {
    match f {
        ReactorFault::PumpBreak => Some(Fault::PumpBreak),
        ReactorFault::LoadDrop => Some(Fault::LoadDrop),
        ReactorFault::Surge => Some(Fault::Surge),
        ReactorFault::Fire | ReactorFault::DrainCoolant => None,
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
        RoomEvent::Device { source, event } => info!("engine room: {source:?} {event:?} (the machine's condition crossed a line)"),
        _ => {}
    }
}

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let tuning = Tuning::load(&files).unwrap_or_else(|e| panic!("reactor: {e:?}"));
    app.insert_resource(ReactorSim::new(&tuning)).insert_resource(ReactorConfig(tuning)).init_resource::<ReactorCounts>();
    app.add_systems(FixedUpdate, reactor_step.before(super::engine_room::room_state).in_set(crate::phases::Fx::Input));
}

/// Host and one process: the requests, one step, the events (fires start in the air), the ship's
/// alarm (the room's or the fire's, the higher), the snapshot out. A client only forwards its
/// requests.
#[allow(clippy::too_many_arguments)]
fn reactor_step(time: Res<Time>, mut items: ResMut<Items>, cfg: Res<ReactorConfig>, mut sim: ResMut<ReactorSim>, mut counts: ResMut<ReactorCounts>, mut alarm: ResMut<ShipAlarm>, mut air: ResMut<EngineAir>, fires: Res<FireConfig>, fire_alarm: Res<FireAlarm>, mut grid: ResMut<FireGrid>, model: Res<FireModel>) {
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
    let dt = time.delta_secs_f64();
    let input = if sim.manual { Input::manual(sim.fission, sim.turbine) } else { Input::auto() };
    events.extend(sim.room.step(dt, input));
    for e in events {
        if let Some((source, size)) = fire::start_of(&e) {
            fire::ignite(&mut air, &mut grid, *model, source, size);
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
    let now = sim.news(oxygen);
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
        s.apply(ReactorAsk::SetPump(false), &cfg(), &mut k);
        settle(&mut s, |n| n.flow < 1e-3);
        let n = s.news(1.0);
        assert!(!n.pump_on && n.flow < 1e-3, "the pump spins down");
        s.apply(ReactorAsk::SetPump(true), &cfg(), &mut k);
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
        assert_eq!(k, ReactorCounts { fires: 2, sparks: 1, meltdowns: 1 });
    }
}

fn part(share: f64, working: bool) -> PartNews {
    PartNews { share, working }
}
