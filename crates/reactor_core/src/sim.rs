//! Simulation harness for the engine room: scripted runs with a time table, for looking at what the
use std::fmt::Write;

use crate::engine_room::{EngineRoom, Fault, Input, PumpMode, RoomEvent, Source};
use crate::grid::Group;
use crate::reactor::{AlarmKind, FuelRod, ReactorEvent};
use crate::tuning::Tuning;

#[cfg(test)]
mod barotrauma;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Auto,
    Manual {
        fission: f64,
        turbine: f64,
    },
    Power(bool),
    /// The coolant pump's switch: on is High, off is Off.
    Pump(bool),
    PumpMode(PumpMode),
    Breaker(Group, bool),
    Fault(Fault),
    SetTank(f64),
    /// The engines' wander on or off (the dev stand-in for a helm).
    Wander(bool),
    RepairReactor,
    RepairPump,
    /// Puts a standard rod into a free slot (a full slot list refuses it silently).
    InsertRod,
    /// Waits, then powers the reactor off once the meltdown timer (0..1) is above this.
    PowerOffWhenMeltdownAbove(f64),
    /// Waits for the core to melt, then runs the inner action `delay` seconds later (once).
    AfterMeltdown(f64, Box<Action>),
    /// Waits for the alarm to be on, then runs the inner action (once per armed action).
    OnAlarm(AlarmKind, Box<Action>),
    /// Pulls the rod in this slot (the emergency brake); the run logs the handling heat.
    PullRod(usize),
    /// Puts a new rod of this `engineering/engineering.json` rods kind into a free slot (unknown kinds do nothing).
    InsertRodKind(String),
    /// The temperature boost: plus (true) or minus the amount. A refused one logs "Boost refused".
    Boost(bool),
    /// Melts the core now, as if the meltdown timer had run out (`EngineRoom::melt_now`).
    MeltNow,
    /// Throws the core out (`EngineRoom::eject_core`, PULSAR's core eject). The events are logged.
    Eject,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    pub name: &'static str,
    pub duration_s: f64,
    pub steps: Vec<(f64, Action)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub t: f64,
    pub fission: f64,
    pub turbine: f64,
    pub temp: f64,
    pub output_kw: f64,
    pub load_kw: f64,
    /// Coolant flow, 0..1.
    pub flow: f64,
    /// Coolant tank, %.
    pub tank: f64,
    pub pump_mode: PumpMode,
    /// The coolant pump's speed, 0..1 (inertia; the flow is this times the stream).
    pub pump_speed: f64,
    /// Fire timer, 0..1 of its delay.
    pub fire: f64,
    /// Meltdown timer, 0..1 of its delay.
    pub melt: f64,
    /// Junction box voltage: delivered over requested, 1 with no request.
    pub volt: f64,
    /// The reactor's lamps that are on, short letters (`Warnings::short`), "" when none.
    pub warn: String,
    pub junction: f64,
    pub reactor: f64,
    pub pump: f64,
    pub power: bool,
    /// Events since the previous row, in order.
    pub events: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Trace {
    pub name: &'static str,
    pub rows: Vec<Row>,
    pub melted: bool,
}

impl Trace {
    pub fn table(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "{:>6} {:>6} {:>6} {:>6} {:>8} {:>7} {:>5} {:>6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6} {:>6} {:>6} {:>5}  {:<14}  {}",
            "t", "fis", "turb", "temp", "output", "load", "flow", "tank", "pmode", "pspd", "fire", "melt", "volt", "junc", "react", "pump", "on", "warn", "events"
        );
        for r in &self.rows {
            let _ = writeln!(
                s,
                "{:>6.1} {:>6.1} {:>6.1} {:>6.1} {:>8.1} {:>7.1} {:>5.2} {:>6.1} {:>5} {:>5.2} {:>5.2} {:>5.2} {:>5.2} {:>6.1} {:>6.1} {:>6.1} {:>5}  {:<14}  {}",
                r.t,
                r.fission,
                r.turbine,
                r.temp,
                r.output_kw,
                r.load_kw,
                r.flow,
                r.tank,
                format!("{:?}", r.pump_mode),
                r.pump_speed,
                r.fire,
                r.melt,
                r.volt,
                r.junction,
                r.reactor,
                r.pump,
                if r.power { "on" } else { "off" },
                r.warn,
                r.events.join(", ")
            );
        }
        s
    }

    pub fn peak_temp(&self) -> f64 {
        self.rows.iter().map(|r| r.temp).fold(f64::MIN, f64::max)
    }

    /// How many events of exactly this label happened in the run (`Fire`, `Meltdown`, ...).
    pub fn count(&self, label: &str) -> usize {
        self.rows
            .iter()
            .flat_map(|r| r.events.iter())
            .filter(|e| e.as_str() == label)
            .count()
    }

    pub fn first(&self, label: &str) -> Option<f64> {
        self.rows
            .iter()
            .find(|r| r.events.iter().any(|e| e == label))
            .map(|r| r.t)
    }

    pub fn last(&self) -> Option<&Row> {
        self.rows.last()
    }
}

/// A reactor warmed up for 120 s on Auto with two standard rods, power on, at the grid's full
/// load (the wander is off for the warm-up, so every script starts from the same 500 kW).
pub fn warm_room(tuning: &Tuning, seed: u64) -> EngineRoom {
    let mut room = EngineRoom::from_tuning(tuning, seed);
    room.set_engine_wander(false);
    assert!(room.set_power(true));
    for _ in 0..1200 {
        room.step(0.1, Input::auto());
    }
    room
}

/// The warm room's settled targets for `boost_kick` (Auto's values after `warm_room`, on the grid's
/// 1280 kW; the temperature sits at about 37): fission 44.2, turbine 64. Held by hand from 0 s.
const BOOST_KICK_FISSION: f64 = 44.2;
const BOOST_KICK_TURBINE: f64 = 64.0;

pub fn scripts() -> Vec<Script> {
    vec![
        Script {
            name: "load_drop",
            duration_s: 180.0,
            steps: vec![(10.0, Action::Fault(Fault::LoadDrop))],
        },
        // All breakers are on after the warm-up already; no operator action follows the pump break.
        Script {
            name: "pump_fail_full_load",
            duration_s: 300.0,
            steps: vec![(10.0, Action::Fault(Fault::PumpBreak))],
        },
        Script {
            name: "turbine_max_fission_max",
            duration_s: 300.0,
            steps: vec![(
                10.0,
                Action::Manual {
                    fission: 100.0,
                    turbine: 100.0,
                },
            )],
        },
        Script {
            name: "coolant_empty",
            duration_s: 300.0,
            steps: vec![(10.0, Action::SetTank(0.0))],
        },
        Script {
            name: "off_last_second",
            duration_s: 300.0,
            steps: vec![
                (0.0, Action::PowerOffWhenMeltdownAbove(0.9)),
                (10.0, Action::Fault(Fault::PumpBreak)),
            ],
        },
        // The core eject: the operator holds fission and turbine at 100 (Manual), the pump
        // breaks at 10 s, and at TempCritical the core is thrown out. No meltdown follows.
        Script {
            name: "eject_in_time",
            duration_s: 300.0,
            steps: vec![
                (0.0, Action::Manual { fission: 100.0, turbine: 100.0 }),
                (10.0, Action::Fault(Fault::PumpBreak)),
                (0.0, Action::OnAlarm(AlarmKind::TempCritical, Box::new(Action::Eject))),
            ],
        },
        Script {
            name: "meltdown_repair_restart",
            duration_s: 600.0,
            steps: vec![
                (10.0, Action::Fault(Fault::PumpBreak)),
                (
                    10.0,
                    Action::Manual {
                        fission: 100.0,
                        turbine: 100.0,
                    },
                ),
                // Armed, they run at melt + 5 s, in this order.
                (
                    0.0,
                    Action::AfterMeltdown(5.0, Box::new(Action::RepairReactor)),
                ),
                (
                    0.0,
                    Action::AfterMeltdown(5.0, Box::new(Action::RepairPump)),
                ),
                (0.0, Action::AfterMeltdown(5.0, Box::new(Action::InsertRod))),
                (0.0, Action::AfterMeltdown(5.0, Box::new(Action::InsertRod))),
                (
                    0.0,
                    Action::AfterMeltdown(5.0, Box::new(Action::Power(true))),
                ),
                (0.0, Action::AfterMeltdown(5.0, Box::new(Action::Auto))),
            ],
        },
        Script {
            name: "pump_low_full_load",
            duration_s: 300.0,
            steps: vec![
                (0.0, Action::Breaker(Group::Engines, true)),
                (0.0, Action::Breaker(Group::LifeSupport, true)),
                (0.0, Action::Breaker(Group::Lights, true)),
                (10.0, Action::PumpMode(PumpMode::Low)),
            ],
        },
        Script {
            name: "wandering_load",
            duration_s: 300.0,
            steps: vec![(0.0, Action::Wander(true))],
        },
        // The warm room, held by hand at its settled values; the operator boosts at 10 s, and again at
        // 13 s, when the boost has faded below 90 % of its amount (22.5). The warn column shows the lamps.
        Script {
            name: "boost_kick",
            duration_s: 120.0,
            steps: vec![
                (0.0, Action::Manual { fission: BOOST_KICK_FISSION, turbine: BOOST_KICK_TURBINE }),
                (10.0, Action::Boost(true)),
                (13.0, Action::Boost(true)),
            ],
        },
        // A melt with no fuel left: both rods come out at 60 s (the fuel is 0 at once, fission with it),
        // and the core melts at 61 s. No Blast: Barotrauma's blast needs more than one unit of fuel.
        // PullRod shifts the slots, so the second rod is slot 0 again.
        Script {
            name: "meltdown_dry",
            duration_s: 200.0,
            steps: vec![
                (10.0, Action::Manual { fission: 100.0, turbine: 100.0 }),
                (60.0, Action::PullRod(0)),
                (60.0, Action::PullRod(0)),
                (61.0, Action::MeltNow),
            ],
        },
        // The brake (rods): the operator holds fission and turbine at 100 (Manual), the pump breaks
        // at 10 s, and at TempCritical both rods come out.
        Script {
            name: "pull_rods_brake",
            duration_s: 300.0,
            steps: vec![
                (10.0, Action::Manual { fission: 100.0, turbine: 100.0 }),
                (10.0, Action::Fault(Fault::PumpBreak)),
                (
                    0.0,
                    Action::OnAlarm(AlarmKind::TempCritical, Box::new(Action::PullRod(0))),
                ),
                (
                    0.0,
                    Action::OnAlarm(AlarmKind::TempCritical, Box::new(Action::PullRod(0))),
                ),
            ],
        },
        // Full turbine and fission with only life support on: the grid asks for 400 kW of 2000 and the
        // junction box sees the surplus (more than twice the load).
        Script {
            name: "overvolt_turbine_max",
            duration_s: 120.0,
            steps: vec![
                (0.0, Action::Breaker(Group::Engines, false)),
                (0.0, Action::Breaker(Group::Lights, false)),
                (10.0, Action::Manual { fission: 100.0, turbine: 100.0 }),
            ],
        },
        Script {
            name: "off_at_temp_high",
            duration_s: 300.0,
            steps: vec![
                (10.0, Action::Manual { fission: 100.0, turbine: 100.0 }),
                (10.0, Action::Fault(Fault::PumpBreak)),
                (0.0, Action::OnAlarm(AlarmKind::TempHigh, Box::new(Action::Power(false)))),
            ],
        },
        Script {
            name: "spicy_rod",
            duration_s: 300.0,
            steps: vec![
                (10.0, Action::PullRod(0)),
                (10.0, Action::InsertRodKind("spicy".into())),
            ],
        },
    ]
}

/// Runs `script` on `room` with steps of `dt`, a row every `sample_every_s`. The room keeps its
/// state after the run, so a test can read it.
pub fn run(room: &mut EngineRoom, script: &Script, dt: f64, sample_every_s: f64) -> Trace {
    let mut steps = script.steps.clone();
    steps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut runner = Runner {
        input: Input::auto(),
        armed_alarm: Vec::new(),
        armed_off: Vec::new(),
        armed_melt: Vec::new(),
        melt_seen: false,
        due: Vec::new(),
        pending: Vec::new(),
    };
    let n = (script.duration_s / dt).round() as usize;
    let every = ((sample_every_s / dt).round() as usize).max(1);
    let mut rows = Vec::new();
    let mut next = 0;
    for k in 0..n {
        let t = k as f64 * dt;
        while next < steps.len() && steps[next].0 <= t + 1e-9 {
            runner.apply(room, &steps[next].1);
            next += 1;
        }
        let mut i = 0;
        while i < runner.due.len() {
            if runner.due[i].0 <= t + 1e-9 {
                let (_, action) = runner.due.remove(i);
                runner.apply(room, &action);
            } else {
                i += 1;
            }
        }

        for e in room.step(dt, runner.input) {
            runner.pending.push(label(&e));
        }
        let t_end = (k + 1) as f64 * dt;
        if room.reactor().melted() && !runner.melt_seen {
            runner.melt_seen = true;
            for (delay, action) in runner.armed_melt.drain(..) {
                runner.due.push((t_end + delay, action));
            }
        }
        let mut i = 0;
        while i < runner.armed_alarm.len() {
            if room.reactor().alarm(runner.armed_alarm[i].0) {
                let (_, action) = runner.armed_alarm.remove(i);
                runner.due.push((t_end, action));
            } else {
                i += 1;
            }
        }
        let mut i = 0;
        while i < runner.armed_off.len() {
            if room.reactor().timers().1 > runner.armed_off[i] {
                runner.armed_off.remove(i);
                runner.apply(room, &Action::Power(false));
            } else {
                i += 1;
            }
        }

        if (k + 1) % every == 0 || k + 1 == n {
            rows.push(sample(room, t_end, std::mem::take(&mut runner.pending)));
        }
    }
    Trace {
        name: script.name,
        rows,
        melted: room.reactor().melted(),
    }
}

struct Runner {
    input: Input,
    armed_alarm: Vec<(AlarmKind, Action)>,
    armed_off: Vec<f64>,
    armed_melt: Vec<(f64, Action)>,
    melt_seen: bool,
    due: Vec<(f64, Action)>,
    pending: Vec<String>,
}

impl Runner {
    fn apply(&mut self, room: &mut EngineRoom, action: &Action) {
        match action {
            Action::PowerOffWhenMeltdownAbove(x) => {
                self.armed_off.push(*x);
                return;
            }
            Action::AfterMeltdown(delay, inner) => {
                self.armed_melt.push((*delay, (**inner).clone()));
                return;
            }
            Action::OnAlarm(kind, inner) => {
                self.armed_alarm.push((*kind, (**inner).clone()));
                return;
            }
            _ => {}
        }
        self.pending.push(format!("do {action:?}"));
        match action {
            Action::Auto => self.input = Input::auto(),
            Action::Manual { fission, turbine } => self.input = Input::manual(*fission, *turbine),
            Action::Power(on) => {
                room.set_power(*on);
            }
            Action::Pump(on) => room.set_pump(*on),
            Action::PumpMode(mode) => room.set_pump_mode(*mode),
            Action::Breaker(g, on) => room.set_breaker(*g, *on),
            Action::Fault(f) => {
                for e in room.trigger(*f) {
                    self.pending.push(label(&e));
                }
            }
            Action::SetTank(pct) => room.pump_mut().set_tank(*pct),
            Action::Wander(on) => room.set_engine_wander(*on),
            Action::RepairReactor => {
                let max = room.reactor().condition().tuning().max;
                room.reactor_mut().condition_mut().set_value(max);
            }
            Action::RepairPump => {
                let max = room.pump().condition().tuning().max;
                room.pump_mut().condition_mut().set_value(max);
            }
            Action::InsertRod => {
                let _ = room.reactor_mut().insert_rod(FuelRod::standard());
            }
            Action::PullRod(slot) => {
                if let Some((rod, heat)) = room.reactor_mut().pull_rod(*slot) {
                    self.pending.push(format!("Pulled {} at {heat:.1}", rod.kind));
                }
            }
            Action::InsertRodKind(id) => {
                if let Some(rod) = room.rod_of(id) {
                    let _ = room.reactor_mut().insert_rod(rod);
                }
            }
            Action::Boost(up) => {
                if !room.reactor_mut().boost(*up) {
                    self.pending.push("Boost refused".into());
                }
            }
            Action::MeltNow => {
                for e in room.melt_now() {
                    self.pending.push(label(&e));
                }
            }
            Action::Eject => {
                for e in room.eject_core() {
                    self.pending.push(label(&e));
                }
            }
            Action::PowerOffWhenMeltdownAbove(_) | Action::AfterMeltdown(..) | Action::OnAlarm(..) => {}
        }
    }
}

fn sample(room: &EngineRoom, t: f64, events: Vec<String>) -> Row {
    let r = room.reactor();
    let (fire, melt) = r.timers();
    Row {
        t,
        fission: r.fission(),
        turbine: r.turbine(),
        temp: r.temperature(),
        output_kw: r.output_kw(),
        load_kw: room.grid().request_kw(),
        flow: room.flow(),
        tank: room.pump().tank(),
        pump_mode: room.pump().mode(),
        pump_speed: room.pump().speed(),
        fire,
        melt,
        volt: room.voltage(),
        warn: room.warnings().short(),
        junction: room.junction().condition().value(),
        reactor: r.condition().value(),
        pump: room.pump().condition().value(),
        power: r.power_on(),
        events,
    }
}

/// The label an event has in a trace. Reactor fires are `Fire`, junction fires `Fire Junction`.
fn label(e: &RoomEvent) -> String {
    match e {
        RoomEvent::Reactor(ReactorEvent::Fire { .. }) => "Fire".into(),
        RoomEvent::Reactor(ReactorEvent::Meltdown) => "Meltdown".into(),
        RoomEvent::Reactor(ReactorEvent::Blast(_)) => "Blast".into(),
        RoomEvent::Reactor(ReactorEvent::CoreEjected) => "CoreEjected".into(),
        RoomEvent::Reactor(ReactorEvent::Alarm { kind, on }) => {
            format!("Alarm {kind:?} {}", if *on { "on" } else { "off" })
        }
        RoomEvent::Reactor(ReactorEvent::Device(d)) => format!("Reactor {d:?}"),
        RoomEvent::Device { source, event } => format!("{source:?} {event:?}"),
        RoomEvent::Sparks => "Sparks".into(),
        RoomEvent::EmergencyCool => "EmergencyCool".into(),
        RoomEvent::Fire { source, .. } => match source {
            Source::Reactor => "Fire Reactor".into(),
            s => format!("Fire {s:?}"),
        },
        RoomEvent::HitFire { .. } => "Fire Hit".into(),
        RoomEvent::EnemyShot { .. } => "Enemy Shot".into(),
    }
}
