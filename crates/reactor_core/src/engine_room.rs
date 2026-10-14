//! The engine room: reactor, coolant pump, junction box and the grid of consumers, stepped together.
//!
//! - The pump's flow (`EngineRoom::flow`: 0 when off or the reactor does not run, else the pump's
//!   factor from `device_core`, 1 working, 0 broken) scales the reactor's cooling offset, so off or
//!   broken lets the temperature climb. Heat above the optimal band wears a running pump as overload.
//! - The junction box sits between the reactor and the consumers. Supply above `overvolt_ratio`
//!   times the demand is overvoltage: the box takes damage per second, and while overvolted each
//!   second may bring sparks, and sparks may start a fire. A broken box passes nothing. Its
//!   breakers (one per group) are the only switches the box has.
//! - `powered`: the machines have power when the box is working and the reactor delivers.
//! - `Fault`s are the dev button's: `PumpBreak`, `LoadDrop` (engines and lights breakers off at
//!   once, so overvoltage and the Auto overshoot follow), `Surge` (the box takes a hit).
//! - `AlarmLevel` is derived each step for the glue: `Warn` above the optimal band or with a part
//!   that needs repair, `Danger` above the allowed band.
//!
//! Meltdown leaves the reactor broken with spent rods (`reactor`); repair and restart are the
//! glue's and use `device_core` repair. No explosion logic here.
use device_core::{Condition, DeviceEvent, DeviceTuning};
use gameplay_core::rng::Rng;

use crate::grid::{Grid, Group};
use crate::reactor::{AlarmKind, FuelRod, Mode, Reactor, ReactorEvent, ReactorTuning};
use crate::tuning::Tuning;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// Values of the engine room beyond the reactor. TODO(initiator): every number is a starting
/// value; later from data.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomTuning {
    /// The coolant pump's device values.
    pub pump: DeviceTuning,
    /// Degrees above the optimal band's top that add one unit of load (overload wear) to the pump.
    pub heat_wear_degrees: f64,
    /// The junction box's device values.
    pub junction: DeviceTuning,
    /// Supply above this multiple of the demand is overvoltage.
    pub overvolt_ratio: f64,
    /// Condition the junction box loses per second while overvolted.
    pub overvolt_damage_s: f64,
    /// Chance per second of sparks while overvolted.
    pub sparks_per_s: f64,
    /// Chance that a spark starts a fire.
    pub spark_fire_chance: f64,
    /// Size of a fire started by sparks, metres.
    pub spark_fire_size: f64,
    /// Condition a `Fault::Surge` takes from the junction box.
    pub surge_damage: f64,
}

impl Default for RoomTuning {
    fn default() -> RoomTuning {
        RoomTuning {
            pump: DeviceTuning::default(),
            heat_wear_degrees: 10.0,
            junction: DeviceTuning::default(),
            overvolt_ratio: 2.0,
            overvolt_damage_s: 2.0,
            sparks_per_s: 0.1,
            spark_fire_chance: 0.2,
            spark_fire_size: 0.05,
            surge_damage: 30.0,
        }
    }
}

/// What the engineer sets each step.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Input {
    /// Auto sets the targets; Manual uses `fission` and `turbine`.
    pub mode: Mode,
    pub fission: f64,
    pub turbine: f64,
}

impl Input {
    pub fn auto() -> Input {
        Input { mode: Mode::Auto, fission: 0.0, turbine: 0.0 }
    }

    pub fn manual(fission: f64, turbine: f64) -> Input {
        Input { mode: Mode::Manual, fission, turbine }
    }
}

/// Where a device event or a fire comes from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Reactor,
    Pump,
    Junction,
}

/// What happened in one step. The glue routes them.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RoomEvent {
    /// Reactor events as the reactor raises them (fire, meltdown, alarms).
    Reactor(ReactorEvent),
    /// A device crossed a line (needs repair, broke, works again).
    Device { source: Source, event: DeviceEvent },
    /// The junction box threw sparks (overvoltage).
    Sparks,
    /// A fire started outside the reactor (from sparks). Reactor fires come as `Reactor(Fire)`.
    Fire { size: f64, source: Source },
}

/// The dev button's faults.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The coolant pump goes to just below its broken limit.
    PumpBreak,
    /// Engines and lights go off at once: overvoltage at the junction box, and Auto overshoots.
    LoadDrop,
    /// The junction box takes `surge_damage`.
    Surge,
}

/// The alarm the glue shows for the whole room.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AlarmLevel {
    None,
    /// Above the optimal band, or a part needs repair.
    Warn,
    /// Above the allowed band: the meltdown timer runs.
    Danger,
}

/// The coolant pump. Its flow scales the reactor's cooling.
#[derive(Clone, Debug)]
pub struct Pump {
    condition: Condition,
    on: bool,
}

impl Pump {
    /// A pump that is on.
    pub fn new(tuning: &RoomTuning, seed: u64) -> Pump {
        Pump { condition: Condition::new(tuning.pump, seed), on: true }
    }

    /// The switch on the pump's screen. Off, it takes no heat away.
    pub fn on(&self) -> bool {
        self.on
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
    }

    pub fn condition(&self) -> &Condition {
        &self.condition
    }

    /// For repair and damage.
    pub fn condition_mut(&mut self) -> &mut Condition {
        &mut self.condition
    }

    /// Output factor 0..1: 1 working, 0 broken.
    pub fn factor(&self) -> f64 {
        self.condition.factor()
    }

    /// A running pump wears as overload while the reactor is above its optimal band.
    fn wear(&mut self, dt: f64, running: bool, excess_degrees: f64, t: &RoomTuning) -> Option<DeviceEvent> {
        let load = if running { 1.0 + excess_degrees.max(0.0) / t.heat_wear_degrees.max(1e-9) } else { 0.0 };
        self.condition.deteriorate(dt, load)
    }
}

/// The junction box between reactor and consumers.
#[derive(Clone, Debug)]
pub struct JunctionBox {
    condition: Condition,
    rng: Rng,
    overvolt: bool,
}

impl JunctionBox {
    pub fn new(tuning: &RoomTuning, seed: u64) -> JunctionBox {
        JunctionBox { condition: Condition::new(tuning.junction, seed), rng: Rng::new(seed ^ 0x05ba_43e5), overvolt: false }
    }

    /// Whether the last step had overvoltage (the screen's OVERVOLT word).
    pub fn overvolted(&self) -> bool {
        self.overvolt
    }

    pub fn condition(&self) -> &Condition {
        &self.condition
    }

    /// For repair and damage.
    pub fn condition_mut(&mut self) -> &mut Condition {
        &mut self.condition
    }

    /// Passes `supply_kw` on to the grid (0 when broken). Overvoltage damages the box and may
    /// throw sparks and start a fire.
    fn pass(&mut self, dt: f64, supply_kw: f64, demand_kw: f64, t: &RoomTuning) -> (f64, Vec<RoomEvent>) {
        let mut ev = Vec::new();
        let supply = supply_kw.max(0.0);
        let through = if self.condition.functioning() { supply } else { 0.0 };
        self.overvolt = supply > 0.0 && supply > t.overvolt_ratio * demand_kw;
        if self.overvolt {
            if let Some(e) = self.condition.damage(t.overvolt_damage_s * dt) {
                ev.push(RoomEvent::Device { source: Source::Junction, event: e });
            }
            if self.rng.unit() < t.sparks_per_s * dt {
                ev.push(RoomEvent::Sparks);
                if self.rng.unit() < t.spark_fire_chance {
                    ev.push(RoomEvent::Fire { size: t.spark_fire_size, source: Source::Junction });
                }
            }
        }
        (through, ev)
    }
}

/// Reactor, pump, junction box and grid, stepped together.
#[derive(Clone, Debug)]
pub struct EngineRoom {
    tuning: RoomTuning,
    reactor: Reactor,
    pump: Pump,
    junction: JunctionBox,
    grid: Grid,
    level: AlarmLevel,
}

impl EngineRoom {
    /// A cold room with the default values (two standard rods). See `from_tuning`.
    pub fn new(tuning: RoomTuning, reactor_tuning: ReactorTuning, seed: u64) -> EngineRoom {
        EngineRoom::from_tuning(&Tuning { room: tuning, reactor: reactor_tuning, ..Tuning::default() }, seed)
    }

    /// A cold room from the loaded values: `start_rods` standard rods, the operator at skill 50,
    /// the grid's demands. Power is off: `set_power(true)` starts the reactor.
    pub fn from_tuning(t: &Tuning, seed: u64) -> EngineRoom {
        let mut reactor = Reactor::new(t.reactor, DeviceTuning::default(), seed.wrapping_add(2));
        for _ in 0..t.start_rods {
            let _ = reactor.insert_rod(FuelRod::standard());
        }
        reactor.set_operator_skill(50.0);
        EngineRoom {
            pump: Pump::new(&t.room, seed),
            junction: JunctionBox::new(&t.room, seed.wrapping_add(1)),
            tuning: t.room,
            reactor,
            grid: Grid::from_tuning(&t.grid),
            level: AlarmLevel::None,
        }
    }

    /// Starts or stops the reactor. Refused while it is broken.
    pub fn set_power(&mut self, on: bool) -> bool {
        self.reactor.set_power(on)
    }

    pub fn set_operator_skill(&mut self, skill: f64) {
        self.reactor.set_operator_skill(skill);
    }

    pub fn tuning(&self) -> &RoomTuning {
        &self.tuning
    }

    pub fn reactor(&self) -> &Reactor {
        &self.reactor
    }

    /// For rods and the like.
    pub fn reactor_mut(&mut self) -> &mut Reactor {
        &mut self.reactor
    }

    pub fn pump(&self) -> &Pump {
        &self.pump
    }

    pub fn pump_mut(&mut self) -> &mut Pump {
        &mut self.pump
    }

    pub fn junction(&self) -> &JunctionBox {
        &self.junction
    }

    pub fn junction_mut(&mut self) -> &mut JunctionBox {
        &mut self.junction
    }

    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    /// For priorities and the like.
    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid
    }

    /// The machines have power: the junction box works and the reactor delivers (output above 0).
    pub fn powered(&self) -> bool {
        self.junction.condition.functioning() && self.reactor.output_kw() > 0.0
    }

    /// The coolant flow, 0..1: the pump's factor while it is on and the reactor runs, else 0.
    pub fn flow(&self) -> f64 {
        if self.pump.on && self.reactor.power_on() { self.pump.factor() } else { 0.0 }
    }

    /// Switches a consumer group's breaker on the junction box.
    pub fn set_breaker(&mut self, g: Group, on: bool) {
        self.grid.set_breaker(g, on);
    }

    /// Switches the coolant pump on or off.
    pub fn set_pump(&mut self, on: bool) {
        self.pump.set_on(on);
    }

    /// The room's alarm, as of the last step.
    pub fn level(&self) -> AlarmLevel {
        self.level
    }

    /// Advances `dt` seconds. Returns the events of this step.
    pub fn step(&mut self, dt: f64, input: Input) -> Vec<RoomEvent> {
        let t = self.tuning;
        let mut ev = Vec::new();

        self.reactor.set_mode(input.mode);
        if input.mode == Mode::Manual {
            self.reactor.set_targets(input.fission, input.turbine);
        }
        self.reactor.set_coolant(self.flow());

        let load = self.grid.request_kw();
        ev.extend(self.reactor.step(dt, load).into_iter().map(RoomEvent::Reactor));

        let (through, junction_ev) = self.junction.pass(dt, self.reactor.output_kw(), load, &t);
        ev.extend(junction_ev);
        self.grid.distribute(through);

        let excess = self.reactor.temperature() - self.reactor.bands().temp_optimal.hi;
        if let Some(e) = self.pump.wear(dt, self.pump.on && self.reactor.power_on(), excess, &t) {
            ev.push(RoomEvent::Device { source: Source::Pump, event: e });
        }

        self.level = self.compute_level();
        ev
    }

    /// Runs a dev-button fault. Returns the device events it caused.
    pub fn trigger(&mut self, fault: Fault) -> Vec<RoomEvent> {
        match fault {
            Fault::PumpBreak => {
                let to = (self.pump.condition.tuning().broken_below - 1.0).max(0.0);
                let amount = (self.pump.condition.value() - to).max(0.0);
                self.pump.condition.damage(amount).map(|e| RoomEvent::Device { source: Source::Pump, event: e }).into_iter().collect()
            }
            Fault::LoadDrop => {
                self.grid.set_breaker(Group::Engines, false);
                self.grid.set_breaker(Group::Lights, false);
                Vec::new()
            }
            Fault::Surge => {
                let amount = self.tuning.surge_damage;
                self.junction.condition.damage(amount).map(|e| RoomEvent::Device { source: Source::Junction, event: e }).into_iter().collect()
            }
        }
    }

    fn compute_level(&self) -> AlarmLevel {
        if self.reactor.alarm(AlarmKind::TempCritical) {
            AlarmLevel::Danger
        } else if self.reactor.alarm(AlarmKind::TempHigh)
            || self.reactor.condition().needs_repair()
            || self.pump.condition.needs_repair()
            || self.junction.condition.needs_repair()
        {
            AlarmLevel::Warn
        } else {
            AlarmLevel::None
        }
    }
}
