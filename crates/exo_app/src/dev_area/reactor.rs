//! The reactor (#297, slice S3b): the host steps the engine room's `EngineRoom` each fixed tick,
//! on Auto by default with the power on. Clients send their requests (`ReactorAsk`: manual or
//! Auto, fission and turbine targets, a dev fault, a reset) and take the host's snapshot
//! (`ReactorNews`, about 4 times a second while it changes) into `Items::reactor`.
//!
//! - Values: `content/gameplay/reactor/reactor.json` (`reactor_core::Tuning`, strict loader).
//! - The room's alarm drives `ShipAlarm` while the reactor is not calm (Warn, Danger); while it is
//!   calm the test console's alarm button may set it.
//! - Fires, sparks and meltdowns are logged (`info!`/`warn!`) and counted in `ReactorCounts` for
//!   the scenarios. Their effects come with the fire and meltdown slices.
//! - The readout and the dev fault screen are `reactor_screen`.
use super::*;
use crate::dev_area::engine_room::ShipAlarm;
use crate::items::{Items, NetOut, Role};
use item_core::wire::{Alarm, PartNews, ReactorAsk, ReactorFault, ReactorNews};
use reactor_core::{AlarmLevel, EngineRoom, Fault, Input, ReactorEvent, RoomEvent, Tuning};

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
    pub shutdowns: u32,
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
    /// A cold room on Auto with the power on: it warms up on its own.
    pub fn new(cfg: &Tuning) -> ReactorSim {
        let mut room = EngineRoom::from_tuning(cfg, SEED);
        room.set_power(true);
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
            ReactorAsk::Trigger(f) => {
                let events = self.room.trigger(fault(f));
                info!("reactor: dev fault {f:?}");
                for e in events {
                    route(e, counts);
                }
            }
            ReactorAsk::Reset => {
                // The alarm the old room set stays until the new room says otherwise.
                let alarmed = self.alarmed;
                *self = ReactorSim { sent: None, since_send: f64::MAX, alarmed, ..ReactorSim::new(cfg) };
                info!("reactor: reset");
            }
        }
    }

    /// The snapshot as the wire carries it.
    pub fn news(&self) -> ReactorNews {
        let r = self.room.reactor();
        let b = r.bands();
        let (fire, melt) = r.timers();
        let (set_fission, set_turbine) = r.targets();
        ReactorNews {
            power_on: r.power_on(),
            manual: self.manual,
            temp: r.temperature(),
            fission: r.fission(),
            turbine: r.turbine(),
            set_fission,
            set_turbine,
            output_kw: r.output_kw(),
            load_kw: self.room.grid().request_kw(),
            optimal: [b.temp_optimal.lo, b.temp_optimal.hi],
            allowed: [b.temp_allowed.lo, b.temp_allowed.hi],
            pump: part(self.room.pump().condition().fraction(), self.room.pump().condition().functioning()),
            junction: part(self.room.junction().condition().fraction(), self.room.junction().condition().functioning()),
            alarm: alarm_of(self.room.level()),
            fire_timer: fire.clamp(0.0, 1.0),
            meltdown_timer: melt.clamp(0.0, 1.0),
            rods: r.rods().iter().filter(|x| !x.spent()).count() as u8,
        }
    }
}

fn fault(f: ReactorFault) -> Fault {
    match f {
        ReactorFault::PumpBreak => Fault::PumpBreak,
        ReactorFault::LoadDrop => Fault::LoadDrop,
        ReactorFault::Surge => Fault::Surge,
    }
}

/// A part's condition for the readout: its share of full condition and whether it works.
fn part(share: f64, working: bool) -> PartNews {
    PartNews { share, working }
}

/// The room's alarm as the ship's alarm.
pub fn alarm_of(l: AlarmLevel) -> Alarm {
    match l {
        AlarmLevel::None => Alarm::None,
        AlarmLevel::Warn => Alarm::Warn,
        AlarmLevel::Danger => Alarm::Danger,
    }
}

/// Logs the events the effects of later slices will use, and counts them.
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
        RoomEvent::Reactor(ReactorEvent::Shutdown) => {
            counts.shutdowns += 1;
            info!("reactor: safety shutdown");
        }
        _ => {}
    }
}

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let tuning = Tuning::load(&files).unwrap_or_else(|e| panic!("reactor: {e:?}"));
    app.insert_resource(ReactorSim::new(&tuning)).insert_resource(ReactorConfig(tuning)).init_resource::<ReactorCounts>();
    app.add_systems(FixedUpdate, reactor_step.before(super::engine_room::room_state).in_set(crate::phases::Fx::Input));
}

/// Host and one process: the requests, one step, the events, the ship's alarm, the snapshot out.
/// A client only forwards its requests.
fn reactor_step(time: Res<Time>, mut items: ResMut<Items>, cfg: Res<ReactorConfig>, mut sim: ResMut<ReactorSim>, mut counts: ResMut<ReactorCounts>, mut alarm: ResMut<ShipAlarm>) {
    let asks = std::mem::take(&mut items.reactor_asks);
    if items.role == Role::Client {
        for a in asks {
            items.net_out.push(NetOut::ReactorAsk(a));
        }
        return;
    }
    for a in asks {
        sim.apply(a, &cfg.0, &mut counts);
    }
    let dt = time.delta_secs_f64();
    let input = if sim.manual { Input::manual(sim.fission, sim.turbine) } else { Input::auto() };
    for e in sim.room.step(dt, input) {
        route(e, &mut counts);
    }
    // Not calm: the room's alarm is the ship's. Calm after an alarm: back to none.
    let level = alarm_of(sim.room.level());
    if level != Alarm::None || sim.alarmed {
        alarm.level = level;
    }
    sim.alarmed = level != Alarm::None;

    let now = sim.news();
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
    fn a_new_reactor_is_on_auto_with_the_power_on() {
        let s = ReactorSim::new(&cfg());
        assert!(!s.manual && s.room.reactor().power_on());
        assert_eq!(s.news().rods, 2);
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
        assert_eq!(k, ReactorCounts { fires: 2, sparks: 1, meltdowns: 1, shutdowns: 0 });
    }
}
