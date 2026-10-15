//! The engine room: reactor, coolant pump, junction box and the grid of consumers, stepped together.
//!
//! - The coolant pump has PULSAR's three modes (manual, `pulsar-mech-conduits.md` 1.4): Off takes no
//!   coolant; Low is a steady small stream that drains the tank slowly; High a large stream that
//!   cools strongly and drains it quickly. Names and states are PULSAR's; every number is ours
//!   (`pump_low`, the time constants, `pump_use_low` and `pump_use_high`), TODO(initiator).
//! - The pump's speed (0..1) follows the mode's target with inertia: Off 0, Low `pump_low`, High 1;
//!   up with `pump_spin_up_s`, down with `pump_run_down_s` (an exponential approach, ours; Barotrauma
//!   has the setting follow at once).
//! - The flow is speed x condition factor x power factor (`EngineRoom::flow`, `stream`), 0 with an
//!   empty tank. Condition: `device_core`, 1 working, 0 broken (Barotrauma `Pump.cs:162` lerps
//!   the flow 0.5..1 with the condition; ours is the device's factor). Power: Barotrauma
//!   `Pump.cs:152` multiplies the flow by the voltage (`powerFactor`), and `Pump.cs:97` needs
//!   `Voltage >= MinVoltage` for `HasPower`: a Barotrauma pump needs grid power. Ours is a backup
//!   supply (our touch, coordinator decision for the initiator to confirm): with the reactor
//!   delivering nothing (off, cold start, SCRAM, broken) the pump runs on the backup at factor 1.
//!   While the reactor delivers, the factor is the junction box's voltage of the last step,
//!   clamped to 0..1, so a short grid (demand above the reactor's maximum) slows the pump, which
//!   cools less and runs hotter (the brownout death spiral, the point). The pump draws no kW from
//!   the grid here: TODO(initiator), pump as a grid consumer.
//! - The flow is the coolant: it sets the reactor's cooling offset, which follows it with a coast
//!   (`coolant_coast_s`, reactor.rs). Off or broken lets the temperature climb; with no flow the core
//!   climbs whatever Auto does. Heat above the optimal band wears a running pump as overload.
//! - The coolant tank: a mode in Low or High drains it at (`coolant_use_base` + `coolant_use_per_heat`
//!   x heat) x its use factor (`pump_use_low` 0.6, `pump_use_high` 1.5) x the stream (condition and
//!   power, not the speed). So High drains 2.5 times as fast as Low at the same heat, as PULSAR says
//!   "quickly" against "slowly". Off drains nothing; its speed runs down over `pump_run_down_s`.
//!   TODO(initiator): pump run-down and the reactor's coolant coast now stack after a switch-off.
//! - The junction box sits between the reactor and the consumers. Supply above `overvolt_ratio`
//!   times the demand is overvoltage (Barotrauma `PowerTransfer.cs:245-292`, `UpdateOvervoltage`:
//!   `Overload = Voltage > max(OverloadVoltage, 1)`). While overvolted, random hits take condition
//!   (Barotrauma: 1 % per 60 Hz frame, here `overvolt_hits_per_s`; each hit `rand(10..500) / 60 *
//!   lerp(5, 1, condition fraction)`, so a damaged box is more sensitive). A hit that breaks the box
//!   gives sparks and, with `break_fire_chance`, a fire (Barotrauma `FireProbability` 0.15, here
//!   fixed, not the intensity lerp; the 5 s `OverloadCooldown` after a break is kept). Sparks also
//!   come now and then while overvolted (`sparks_per_s`, our touch: the screen shows it coming).
//!   Undervoltage (supply delivered below the request) is seen (`voltage`, `undervolted`) and
//!   damages nothing; Barotrauma does not damage on undervoltage either. A broken box passes
//!   nothing. Its breakers (one per group) are the only switches the box has.
//! - The engines' load follows a throttle, 0..1, which scales the Engines group's demand (ours:
//!   Barotrauma `Engine.cs:190` has the consumption follow the force, with exponent 1.5; ours is
//!   linear). `set_engine_wander` is the dev stand-in for a helm: the throttle random-walks between
//!   `wander_min` and 1, a new target every `wander_every_s`, moving at `wander_rate` per second.
//! - `powered`: the machines have power when the box is working and the reactor delivers.
//! - `Fault`s are the dev button's: `PumpBreak`, `LoadDrop` (engines and lights breakers off at
//!   once, so overvoltage and the Auto overshoot follow), `Surge` (the box takes a hit).
//! - `AlarmLevel` is derived each step for the glue: `Warn` above the optimal band or with a part
//!   that needs repair, `Danger` above the allowed band.
//!
//! Meltdown blast (Barotrauma reactor.xml:45-58, `reactor` module doc): a meltdown with fuel left
//! raises `Reactor(Blast)`, and the pump and the junction box take `blast.device_damage` at once
//! (they break). The room has no positions, so every device in it is in range. The burn, stun and
//! radiation for the players are the glue's.
//!
//! Meltdown leaves the reactor broken with spent rods (`reactor`); repair and restart are the
//! glue's and use `device_core` repair. No explosion logic here.
use device_core::{Condition, DeviceEvent, DeviceTuning};
use gameplay_core::rng::Rng;

use crate::grid::{Grid, Group};
use crate::history::History;
use crate::reactor::{AlarmKind, FuelRod, Mode, Reactor, ReactorEvent, ReactorTuning, RodKinds};
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
    /// Hits per second while overvolted (Barotrauma rolls 1 % per 60 Hz frame: 0.6). TODO(initiator).
    pub overvolt_hits_per_s: f64,
    /// Chance that the hit which breaks the box starts a fire. Barotrauma `FireProbability` 0.15.
    pub break_fire_chance: f64,
    /// Chance per second of sparks while overvolted.
    pub sparks_per_s: f64,
    /// Size of a fire from the junction box (a break), metres.
    pub spark_fire_size: f64,
    /// Condition a `Fault::Surge` takes from the junction box.
    pub surge_damage: f64,
    /// Coolant tank use while the pump runs, % of the tank per second, before any heat.
    /// TODO(initiator): value. Full tank at normal running lasts about 6.5 min with the rest.
    pub coolant_use_base: f64,
    /// Extra coolant use, % of the tank per second per unit of heat the reactor makes, at full
    /// flow: the pump uses more the more heat it carries away. TODO(initiator): value.
    pub coolant_use_per_heat: f64,
    /// The tank is low below this level, %. The pump's screen warns LOW.
    pub coolant_low: f64,
    /// The speed the Low mode settles at, 0..1 (High settles at 1, Off at 0). TODO(initiator).
    pub pump_low: f64,
    /// Seconds for the pump's speed to close 63 % of the gap up to its target. TODO(initiator).
    pub pump_spin_up_s: f64,
    /// Seconds for the pump's speed to close 63 % of the gap down to its target. TODO(initiator).
    pub pump_run_down_s: f64,
    /// Coolant use factor of the Low mode (times the use per heat, at full stream). TODO(initiator).
    pub pump_use_low: f64,
    /// Coolant use factor of the High mode. TODO(initiator).
    pub pump_use_high: f64,
    /// What one coolant canister adds to the tank, %.
    pub coolant_canister: f64,
    /// Dev stand-in for a helm: the engines' throttle random-walks while on. Off in code, on in
    /// `reactor.json`.
    pub engine_wander: bool,
    /// Seconds between two new wander targets.
    pub wander_every_s: f64,
    /// The lowest wander target, 0..1.
    pub wander_min: f64,
    /// How fast the throttle moves to its target, per second.
    pub wander_rate: f64,
}

impl Default for RoomTuning {
    fn default() -> RoomTuning {
        RoomTuning {
            // Wear as in Barotrauma: normal wear stops at the floor (50), above the repair line (80)
            // for the pump and the junction box, so wear creates repair work but breaks nothing.
            pump: DeviceTuning { repair_threshold: 80.0, ..DeviceTuning::default() },
            heat_wear_degrees: 10.0,
            junction: DeviceTuning { repair_threshold: 80.0, ..DeviceTuning::default() },
            overvolt_ratio: 2.0,
            overvolt_hits_per_s: 0.6,
            break_fire_chance: 0.15,
            sparks_per_s: 0.1,
            spark_fire_size: 0.05,
            surge_damage: 30.0,
            coolant_use_base: 0.04,
            coolant_use_per_heat: 0.0013,
            coolant_low: 25.0,
            coolant_canister: 50.0,
            pump_low: 0.5,
            pump_spin_up_s: 3.0,
            pump_run_down_s: 12.0,
            pump_use_low: 0.6,
            pump_use_high: 1.5,
            engine_wander: false,
            wander_every_s: 6.0,
            wander_min: 0.3,
            wander_rate: 0.15,
        }
    }
}

/// Barotrauma's `OverloadCooldown` (`PowerTransfer.cs:22`): no hits for this long after a break.
const OVERLOAD_COOLDOWN_S: f64 = 5.0;

/// The warm start's step (1/60 s, a frame), Auto's speed factor during it (Barotrauma's speed 100),
/// the hold the reactor must keep settled (5 s), the cap (600 s) and the output tolerance (2 %).
/// TODO(initiator): values are ours, from the source's numbers.
const WARM_START_DT: f64 = 1.0 / 60.0;
const WARM_START_AUTO_BOOST: f64 = 100.0;
const WARM_START_HOLD_S: f64 = 5.0;
const WARM_START_CAP_S: f64 = 600.0;
const WARM_START_TOLERANCE: f64 = 0.02;

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

/// The coolant pump's switch, PULSAR's three states (manual, `pulsar-mech-conduits.md` 1.4): Off
/// takes no coolant; Low is a steady small stream that drains the tank slowly; High is a large
/// stream that cools strongly and drains it quickly.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PumpMode {
    Off,
    Low,
    High,
}

/// The coolant pump. Its flow scales the reactor's cooling.
#[derive(Clone, Debug)]
pub struct Pump {
    condition: Condition,
    mode: PumpMode,
    /// The pump's speed, 0..1: follows the mode's target with inertia (`spin`).
    speed: f64,
    /// Coolant left in the tank, 0..100 %. Empty: no flow.
    tank: f64,
}

impl Pump {
    /// A pump in High, running at full speed (as a pump that is on always was).
    pub fn new(tuning: &RoomTuning, seed: u64) -> Pump {
        Pump { condition: Condition::new(tuning.pump, seed), mode: PumpMode::High, speed: 1.0, tank: 100.0 }
    }

    /// The switch's mode.
    pub fn mode(&self) -> PumpMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: PumpMode) {
        self.mode = mode;
    }

    /// The speed the mode asks for, 0..1: Off 0, Low `pump_low`, High 1.
    fn target(&self, t: &RoomTuning) -> f64 {
        match self.mode {
            PumpMode::Off => 0.0,
            PumpMode::Low => t.pump_low.clamp(0.0, 1.0),
            PumpMode::High => 1.0,
        }
    }

    /// The pump's speed, 0..1 (inertia: it runs up and down toward the mode's target).
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// Moves the speed toward the target: up with `pump_spin_up_s`, down with `pump_run_down_s`,
    /// an exponential approach (`dt / tau`, at most the whole gap).
    fn spin(&mut self, dt: f64, t: &RoomTuning) {
        let target = self.target(t);
        let tau = if target > self.speed { t.pump_spin_up_s } else { t.pump_run_down_s };
        self.speed += (target - self.speed) * (dt / tau.max(1e-9)).min(1.0);
    }

    /// Coolant use factor of the mode: Off 0, Low `pump_use_low`, High `pump_use_high`.
    fn use_factor(&self, t: &RoomTuning) -> f64 {
        match self.mode {
            PumpMode::Off => 0.0,
            PumpMode::Low => t.pump_use_low,
            PumpMode::High => t.pump_use_high,
        }
    }

    /// Coolant left in the tank, 0..100 %.
    pub fn tank(&self) -> f64 {
        self.tank
    }

    /// Sets the tank level (0..100 %). For the dev drain and the tests.
    pub fn set_tank(&mut self, pct: f64) {
        self.tank = pct.clamp(0.0, 100.0);
    }

    /// Adds `pct` to the tank up to full. Returns what was added.
    pub fn refill(&mut self, pct: f64) -> f64 {
        let before = self.tank;
        self.tank = (self.tank + pct.max(0.0)).min(100.0);
        self.tank - before
    }

    /// The tank uses coolant while the pump is in Low or High, in proportion to the heat it carries
    /// away (`heat`, the reactor's heat input), the mode's use factor and `stream`, the flow the
    /// pump would have at full speed (condition and power). Speed is not in it, so High drains
    /// `pump_use_high / pump_use_low` times as fast as Low at the same heat.
    fn use_coolant(&mut self, dt: f64, stream: f64, heat: f64, t: &RoomTuning) {
        if stream > 0.0 {
            let rate = (t.coolant_use_base + t.coolant_use_per_heat * heat.max(0.0)) * self.use_factor(t) * stream;
            self.tank = (self.tank - rate * dt).max(0.0);
        }
    }

    /// The mode is Low or High. Off takes no coolant and runs its speed down.
    pub fn on(&self) -> bool {
        self.mode != PumpMode::Off
    }

    /// The screen's on/off switch: on is High, off is Off (`set_mode` for Low).
    pub fn set_on(&mut self, on: bool) {
        self.mode = if on { PumpMode::High } else { PumpMode::Off };
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
    /// Delivered supply over the request, 1 with no request (last step).
    voltage: f64,
    /// Barotrauma's `overloadCooldownTimer`, s.
    cooldown: f64,
}

impl JunctionBox {
    pub fn new(tuning: &RoomTuning, seed: u64) -> JunctionBox {
        JunctionBox {
            condition: Condition::new(tuning.junction, seed),
            rng: Rng::new(seed ^ 0x05ba_43e5),
            overvolt: false,
            voltage: 1.0,
            cooldown: 0.0,
        }
    }

    /// Whether the last step had overvoltage (the screen's OVERVOLT word).
    pub fn overvolted(&self) -> bool {
        self.overvolt
    }

    /// The last step's delivered supply over the request, 1 when nothing was asked for.
    pub fn voltage(&self) -> f64 {
        self.voltage
    }

    /// The grid got less than it asked for (the screen's LOW VOLTAGE word). Damages nothing.
    pub fn undervolted(&self) -> bool {
        self.voltage < 1.0
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
        let demand = demand_kw.max(0.0);
        let through = if self.condition.functioning() { supply } else { 0.0 };
        // Barotrauma `PowerTransfer.cs:250`: Overload = Voltage > max(OverloadVoltage, 1). Voltage is
        // supply / load, so a load of 0 with supply counts as overvoltage.
        self.overvolt = supply > 0.0 && supply > t.overvolt_ratio.max(1.0) * demand;
        if self.overvolt {
            ev.extend(self.overload(dt, t));
            if self.rng.unit() < t.sparks_per_s * dt {
                ev.push(RoomEvent::Sparks);
            }
        }
        // The voltage the grid sees: what the box passes over what the grid asked for.
        self.voltage = if demand > 0.0 { through / demand } else { 1.0 };
        (through, ev)
    }

    /// Barotrauma `UpdateOvervoltage` (`PowerTransfer.cs:257-291`): while overloaded, random hits.
    fn overload(&mut self, dt: f64, t: &RoomTuning) -> Vec<RoomEvent> {
        let mut ev = Vec::new();
        if self.cooldown > 0.0 {
            self.cooldown -= dt;
            return ev;
        }
        // Barotrauma rolls `Rand.Range(0, 1) < 0.01` per 60 Hz frame; here a rate per second.
        if self.rng.unit() < t.overvolt_hits_per_s * dt {
            // Damaged boxes are more sensitive to overvoltage (Barotrauma `:270-272`).
            let sensitivity = 5.0 + (1.0 - 5.0) * self.condition.fraction();
            let amount = (10.0 + self.rng.unit() * 490.0) / 60.0 * sensitivity;
            if let Some(e) = self.condition.damage(amount) {
                let broke = e == DeviceEvent::Broke;
                ev.push(RoomEvent::Device { source: Source::Junction, event: e });
                if broke {
                    // Barotrauma `:276-290`: zap and sparks; a fire with `FireProbability`.
                    self.cooldown = OVERLOAD_COOLDOWN_S;
                    ev.push(RoomEvent::Sparks);
                    if self.rng.unit() < t.break_fire_chance {
                        ev.push(RoomEvent::Fire { size: t.spark_fire_size, source: Source::Junction });
                    }
                }
            }
        }
        ev
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
    /// The Engines group's demand at full throttle, kW (from the tuning).
    engines_full_kw: f64,
    /// Engines throttle 0..1.
    throttle: f64,
    wander: bool,
    wander_rng: Rng,
    wander_target: f64,
    wander_left: f64,
    /// The rod kinds of the loaded values, for `rod_of`.
    rods: RodKinds,
    /// Output, load and temperature over the last points (the status board's graph).
    history: History,
}

impl EngineRoom {
    /// A cold room with the default values (two standard rods). See `from_tuning`.
    pub fn new(tuning: RoomTuning, reactor_tuning: ReactorTuning, seed: u64) -> EngineRoom {
        EngineRoom::from_tuning(&Tuning { room: tuning, reactor: reactor_tuning, ..Tuning::default() }, seed)
    }

    /// A cold room from the loaded values: `start_rods` standard rods, the operator at skill 50,
    /// the grid's demands. Power is off: `set_power(true)` starts the reactor.
    pub fn from_tuning(t: &Tuning, seed: u64) -> EngineRoom {
        let mut reactor = Reactor::new(t.reactor, seed.wrapping_add(2));
        for _ in 0..t.start_rods {
            let _ = reactor.insert_rod(FuelRod::standard());
        }
        let rods = t.rods.clone();
        reactor.set_operator_skill(50.0);
        EngineRoom {
            pump: Pump::new(&t.room, seed),
            junction: JunctionBox::new(&t.room, seed.wrapping_add(1)),
            tuning: t.room,
            rods,
            history: History::new(t.graph_points, t.graph_interval_s),
            reactor,
            grid: Grid::from_tuning(&t.grid),
            level: AlarmLevel::None,
            engines_full_kw: t.grid.engines.demand_kw,
            throttle: 1.0,
            wander: t.room.engine_wander,
            wander_rng: Rng::new(seed ^ 0x7a1d_e51a),
            wander_target: 1.0,
            wander_left: t.room.wander_every_s,
        }
    }

    /// Starts or stops the reactor. Refused while it is broken.
    pub fn set_power(&mut self, on: bool) -> bool {
        self.reactor.set_power(on)
    }

    /// The graph's history (newest first), fed by `step`.
    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn set_operator_skill(&mut self, skill: f64) {
        self.reactor.set_operator_skill(skill);
    }

    pub fn tuning(&self) -> &RoomTuning {
        &self.tuning
    }

    /// A new rod of the loaded kind `id` (a `reactor.json` `rods` key), or None for an unknown id.
    pub fn rod_of(&self, id: &str) -> Option<FuelRod> {
        self.rods.rod(id)
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

    /// The junction box's voltage: what the grid got over what it asked for, 0..1 (1 with no
    /// request). Below 1 the grid is short (`junction().undervolted()`).
    pub fn voltage(&self) -> f64 {
        self.junction.voltage()
    }

    /// The engines' throttle, 0..1 (scales the Engines group's demand).
    pub fn engine_throttle(&self) -> f64 {
        self.throttle
    }

    /// Sets the engines' throttle, 0..1, and the Engines demand with it. While wander is on, the
    /// next step moves it on.
    pub fn set_engine_throttle(&mut self, f: f64) {
        self.throttle = f.clamp(0.0, 1.0);
        self.grid.set_demand(Group::Engines, self.engines_full_kw * self.throttle);
    }

    pub fn engine_wander(&self) -> bool {
        self.wander
    }

    /// The dev stand-in for a helm: while on, `step` moves the throttle with a random walk.
    pub fn set_engine_wander(&mut self, on: bool) {
        self.wander = on;
    }

    /// The machines have power: the junction box works and the reactor delivers (output above 0).
    pub fn powered(&self) -> bool {
        self.junction.condition.functioning() && self.reactor.output_kw() > 0.0
    }

    /// The coolant flow, 0..1: the pump's speed times `stream`. Off, it runs down; an empty tank
    /// or no power is no flow.
    pub fn flow(&self) -> f64 {
        self.pump.speed * self.stream()
    }

    /// The flow the pump would have at full speed, 0..1: its condition factor times the power
    /// factor, 0 with an empty tank.
    pub fn stream(&self) -> f64 {
        if self.pump.tank > 0.0 { self.pump.factor() * self.power_factor() } else { 0.0 }
    }

    /// The pump's power factor, 0..1 (Barotrauma `Pump.cs:152`, `powerFactor`). With the reactor
    /// delivering nothing (off, cold start, SCRAM, broken) the pump runs on its backup supply at 1.
    /// Otherwise it is the junction box's voltage of the last step, clamped to 0..1: a short grid
    /// slows the pump, and a broken box (which passes nothing) stops it.
    pub fn power_factor(&self) -> f64 {
        if self.reactor.output_kw() <= 0.0 { 1.0 } else { self.junction.voltage().clamp(0.0, 1.0) }
    }

    /// Switches a consumer group's breaker on the junction box.
    pub fn set_breaker(&mut self, g: Group, on: bool) {
        self.grid.set_breaker(g, on);
    }

    /// Switches the coolant pump on (High) or off (Off).
    pub fn set_pump(&mut self, on: bool) {
        self.pump.set_on(on);
    }

    /// Sets the coolant pump's mode (Off, Low or High).
    pub fn set_pump_mode(&mut self, mode: PumpMode) {
        self.pump.set_mode(mode);
    }

    /// The reactor's lamps for the load the grid asks for now (`Reactor::warnings`).
    pub fn warnings(&self) -> crate::reactor::Warnings {
        self.reactor.warnings(self.grid.request_kw())
    }

    /// The room's alarm, as of the last step.
    pub fn level(&self) -> AlarmLevel {
        self.level
    }

    /// Barotrauma's warm start (`Reactor.cs:613-624`, `PowerUpImmediately`): the power on, Auto on,
    /// then steps with Auto's speeds boosted by 100 (Barotrauma's `UpdateAutoTemp(100, …)`) until
    /// the reactor is settled: the temperature inside its optimal band and the output within 2 %
    /// of the load, held for 5 s of sim time. Capped at 600 s. Barotrauma's first 5 s of a round
    /// (`Reactor.cs:253-258`) are the same rapid auto-temp, here made a start option.
    ///
    /// The coolant tank and the rods' life are put back as they were: a warm start costs no
    /// supplies. The events of the warm-up are dropped (no fire or alarm reaches the game from it).
    /// The pump keeps its mode; a pump that is off does not settle. Returns the sim seconds taken;
    /// 0 with the reactor broken (it cannot be started, nothing runs).
    pub fn warm_start(&mut self) -> f64 {
        self.warm_start_events().0
    }

    /// `warm_start`, with the warm-up's events (for the tests).
    fn warm_start_events(&mut self) -> (f64, Vec<RoomEvent>) {
        let mut ev = Vec::new();
        if !self.set_power(true) {
            return (0.0, ev);
        }
        let tank = self.pump.tank;
        let lives = self.reactor.rod_lives();
        self.reactor.set_auto_boost(WARM_START_AUTO_BOOST);
        let hold_steps = (WARM_START_HOLD_S / WARM_START_DT).round() as usize;
        let cap_steps = (WARM_START_CAP_S / WARM_START_DT).round() as usize;
        let mut steps = 0;
        let mut held = 0;
        while steps < cap_steps {
            ev.extend(self.step(WARM_START_DT, Input::auto()));
            steps += 1;
            if self.settled() {
                held += 1;
                if held >= hold_steps {
                    break;
                }
            } else {
                held = 0;
            }
        }
        self.reactor.set_auto_boost(1.0);
        self.pump.tank = tank;
        self.reactor.set_rod_lives(&lives);
        (steps as f64 * WARM_START_DT, ev)
    }

    /// The temperature inside its optimal band and the output within `WARM_START_TOLERANCE` of the
    /// load the grid asks for.
    pub fn settled(&self) -> bool {
        let load = self.grid.request_kw();
        self.reactor.bands().temp_optimal.contains(self.reactor.temperature())
            && (self.reactor.output_kw() - load).abs() <= WARM_START_TOLERANCE * load
    }

    /// Advances `dt` seconds. Returns the events of this step.
    pub fn step(&mut self, dt: f64, input: Input) -> Vec<RoomEvent> {
        let t = self.tuning;
        let mut ev = Vec::new();

        self.reactor.set_mode(input.mode);
        if input.mode == Mode::Manual {
            self.reactor.set_targets(input.fission, input.turbine);
        }
        self.pump.spin(dt, &t);
        let flow = self.flow();
        let stream = self.stream();
        self.reactor.set_coolant(flow);

        if self.wander {
            self.wander_step(dt, &t);
        }
        self.grid.set_demand(Group::Engines, self.engines_full_kw * self.throttle);
        let load = self.grid.request_kw();
        let reactor_ev = self.reactor.step(dt, load);
        ev.extend(reactor_ev.iter().map(|e| RoomEvent::Reactor(*e)));
        ev.extend(self.blast_damage(&reactor_ev));
        self.pump.use_coolant(dt, stream, self.reactor.heat_input(), &t);

        let (through, junction_ev) = self.junction.pass(dt, self.reactor.output_kw(), load, &t);
        ev.extend(junction_ev);
        self.grid.distribute(through);

        let excess = self.reactor.temperature() - self.reactor.bands().temp_optimal.hi;
        if let Some(e) = self.pump.wear(dt, self.pump.on() && self.reactor.power_on(), excess, &t) {
            ev.push(RoomEvent::Device { source: Source::Pump, event: e });
        }

        self.level = self.compute_level();
        self.history.push_sample(dt, self.reactor.output_kw(), load, self.reactor.temperature());
        ev
    }

    /// Melts the core now (`Reactor::force_meltdown`), with the room's blast damage. The harness's
    /// forced melt; the events are those of a melt in `step`.
    pub fn melt_now(&mut self) -> Vec<RoomEvent> {
        let reactor_ev = self.reactor.force_meltdown();
        let mut ev: Vec<RoomEvent> = reactor_ev.iter().map(|e| RoomEvent::Reactor(*e)).collect();
        ev.extend(self.blast_damage(&reactor_ev));
        ev
    }

    /// A blast in the reactor's events takes `device_damage` from the pump and the junction box:
    /// every device in the room is in range. Returns their device events.
    fn blast_damage(&mut self, reactor_ev: &[ReactorEvent]) -> Vec<RoomEvent> {
        let Some(blast) = reactor_ev.iter().find_map(|e| match e {
            ReactorEvent::Blast(b) => Some(*b),
            _ => None,
        }) else {
            return Vec::new();
        };
        let mut ev = Vec::new();
        if let Some(e) = self.pump.condition.damage(blast.device_damage) {
            ev.push(RoomEvent::Device { source: Source::Pump, event: e });
        }
        if let Some(e) = self.junction.condition.damage(blast.device_damage) {
            ev.push(RoomEvent::Device { source: Source::Junction, event: e });
        }
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

    /// One step of the wander: a new target every `wander_every_s` in [wander_min, 1], the throttle
    /// moves to it at `wander_rate` per second.
    fn wander_step(&mut self, dt: f64, t: &RoomTuning) {
        let lo = t.wander_min.clamp(0.0, 1.0);
        self.wander_left -= dt;
        if self.wander_left <= 0.0 {
            self.wander_target = lo + self.wander_rng.unit() * (1.0 - lo);
            self.wander_left = t.wander_every_s;
        }
        let step = t.wander_rate * dt;
        self.throttle = (self.throttle + (self.wander_target - self.throttle).clamp(-step, step)).clamp(lo, 1.0);
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
