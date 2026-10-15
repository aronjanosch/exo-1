//! The engine room: reactor, coolant pump, junction box and the grid of consumers, stepped together.
//!
//! Engines (r38): the crew's throttle is the engine's target (`engine::Engine`, after Barotrauma's
//! `Engine.cs`); the engine's power draw is the Engines demand, and its thrust is shown on the HELM.
//! `engine_wander` is a dev driver that sets the target.
//!
//! Pump leak (r36): below 50 % condition the pump leaks coolant out of the tank, running or not,
//! at `pump_leak_max * (1 - c / 0.5)^2` tank fraction per second (c = condition, 0..1). The curve is
//! ours (Barotrauma's condition-squared curve, `OxygenGenerator.cs:55-59`, is the shape's source).
//! The hold repair of the pump raises its condition and so stops the leak.
//!   (`pump_low`, the time constants, `pump_use_low` and `pump_use_high`), TODO(initiator).
//!   the grid here: TODO(initiator), pump as a grid consumer.
//!   TODO(initiator): pump run-down and the reactor's coolant coast now stack after a switch-off.
//!   the cost, or with the pump broken. All values TODO(initiator).
use device_core::{Condition, DeviceEvent, DeviceTuning};
use gameplay_core::rng::Rng;

use crate::battery::Battery;
use crate::enemy::{EnemyTuning, Encounter, Outcome};
use crate::weapons::Weapons;
use crate::engine::Engine;
use crate::hull::{Hit, HitsTuning, Hull, DEVICES, REACTOR_AT};
use crate::grid::{Grid, Group};
use crate::history::History;
use crate::jump::JumpDrive;
use crate::reactor::{AlarmKind, FuelRod, Mode, Reactor, ReactorEvent, ReactorTuning, RodKinds};
use crate::shield::Shield;
use crate::tuning::Tuning;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// Values of the engine room beyond the reactor. TODO(initiator): every number is a starting
/// value; later from data.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomTuning {
    pub pump: DeviceTuning,
    /// Degrees above the optimal band's top that add one unit of load (overload wear) to the pump.
    pub heat_wear_degrees: f64,
    pub junction: DeviceTuning,
    /// The oxygen generator's device values (r37, same numbers as the pump; TODO(initiator)).
    pub oxygen_generator: DeviceTuning,
    /// Supply above this multiple of the demand is overvoltage.
    pub overvolt_ratio: f64,
    /// Hits per second while overvolted (Barotrauma rolls 1 % per 60 Hz frame: 0.6). TODO(initiator).
    pub overvolt_hits_per_s: f64,
    /// Chance that the hit which breaks the box starts a fire. Barotrauma `FireProbability` 0.15.
    pub break_fire_chance: f64,
    pub sparks_per_s: f64,
    /// Size of a fire from the junction box (a break), metres.
    pub spark_fire_size: f64,
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
    /// The coolant pump's leak at condition 0, tank fraction per second (`Pump::leak_rate`). Below
    /// half condition the leak runs at this times (1 - c / 0.5)^2. TODO(initiator): value (0.01: a
    /// broken pump drains a full tank in about 100 s).
    pub pump_leak_max: f64,
    /// Emergency cooling (PULSAR's manual button, `EngineRoom::emergency_cool`): the tank it takes
    /// at once, %. TODO(initiator): value.
    pub ecool_tank_cost: f64,
    /// Seconds the emergency cooling runs. TODO(initiator): value (PULSAR's is not in the manual).
    pub ecool_duration_s: f64,
    /// Extra cooling while it runs, degrees of the reactor's temperature target, on top of the
    /// normal cooling. TODO(initiator): value.
    pub ecool_strength: f64,
    /// Seconds from the press until it can be pressed again (PULSAR's EmergencyCooldownTime). TODO(initiator): value.
    pub ecool_cooldown_s: f64,
    /// Dev stand-in for a helm: the engines' target random-walks while on (the force follows it). Off
    /// in code and in `reactor.json`. TODO(initiator): on or off by default.
    pub engine_wander: bool,
    pub wander_every_s: f64,
    /// The lowest wander target, 0..1 of full throttle.
    pub wander_min: f64,
    /// The aux reactor (PULSAR manual: auxiliary systems are not connected to the main reactor).
    /// On: the main grid carries the Engines group only; Life Support and Lights are fed by the aux
    /// reactor, and what they leave of its output boosts the main reactor. TODO(initiator): default.
    pub aux_enabled: bool,
    /// The aux reactor's output, kW. TODO(initiator): value.
    pub aux_output_kw: f64,
    /// The oxygen generator's air, m³ per second at full life support power (`fire_core::oxygen`).
    /// TODO(initiator): value; 2.13 refills the 256 m³ engine room from half in about 60 s.
    pub oxygen_generator_rate: f64,
    /// The air one breathing player takes out of the engine room, m³ per second. Barotrauma
    /// `Hull.OxygenConsumptionSpeed` 700 px²/s over a 1000 × 320 px hull is 0.0022 of the room a
    /// second, which is 0.56 m³/s in our 256 m³ room. TODO(initiator): value; at 0.56 the full
    /// generator (2.13) outruns about 3.8 players.
    pub breath_m3_per_s: f64,
    /// The hull breach's hole, m² when fully open (`fire_core` leak). TODO(initiator): value; 0.5
    /// empties the 256 m³ room from full to the bad-air line (0.3) in about 2.6 min, shut door.
    pub breach_area_m2: f64,
    /// The wall damage the meltdown blast and the HULL BREACH dev fault set (0..1). The blast
    /// damages the sections near the reactor (`hull::Hull::hit`); the dev fault sets the one nearest
    /// the reactor. TODO(initiator): value.
    pub blast_breach_damage: f64,
    /// The size of a fire a hit starts at its impact, m (`RoomEvent::HitFire`). TODO(initiator): value.
    pub hit_fire_size: f64,
}

impl Default for RoomTuning {
    fn default() -> RoomTuning {
        RoomTuning {
            // Wear as in Barotrauma: normal wear stops at the floor (50), above the repair line (80)
            // for the pump and the junction box, so wear creates repair work but breaks nothing.
            pump: DeviceTuning { repair_threshold: 80.0, ..DeviceTuning::default() },
            heat_wear_degrees: 10.0,
            junction: DeviceTuning { repair_threshold: 80.0, ..DeviceTuning::default() },
            oxygen_generator: DeviceTuning { repair_threshold: 80.0, ..DeviceTuning::default() },
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
            pump_leak_max: 0.01,
            ecool_tank_cost: 25.0,
            ecool_duration_s: 10.0,
            ecool_strength: 60.0,
            ecool_cooldown_s: 90.0,
            pump_low: 0.5,
            pump_spin_up_s: 3.0,
            pump_run_down_s: 12.0,
            pump_use_low: 0.6,
            pump_use_high: 1.5,
            engine_wander: false,
            wander_every_s: 6.0,
            wander_min: 0.3,
            aux_enabled: false,
            aux_output_kw: 500.0,
            oxygen_generator_rate: 2.13,
            breath_m3_per_s: 0.56,
            breach_area_m2: 0.5,
            blast_breach_damage: 0.6,
            hit_fire_size: 0.1,
        }
    }
}

/// The pump's condition fraction below which it leaks (r36). TODO(initiator): value.
const PUMP_LEAK_BELOW: f64 = 0.5;

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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Reactor,
    Pump,
    Junction,
    OxygenGenerator,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RoomEvent {
    Reactor(ReactorEvent),
    /// The operator pressed the emergency cooling (`EngineRoom::emergency_cool`) in the last step.
    EmergencyCool,
    Device { source: Source, event: DeviceEvent },
    Sparks,
    /// A fire started outside the reactor (from sparks). Reactor fires come as `Reactor(Fire)`.
    Fire { size: f64, source: Source },
    /// A fire at a hit's impact (`EngineRoom::apply_hit`, r40): the same fire, placed where it hit.
    HitFire { size: f64, at: [f64; 2] },
    /// The enemy's turret fired (r41, `enemy.rs`): its shot hit the wall at `section` (`SECTION_CENTRES`).
    EnemyShot { section: usize },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The coolant pump goes to just below its broken limit.
    PumpBreak,
    /// The oxygen generator goes to just below its broken limit (the dev faults screen's O2 gen break).
    OxygenGeneratorBreak,
    /// Engines and lights go off at once: overvoltage at the junction box, and Auto overshoots.
    LoadDrop,
    Surge,
    /// The engine room's hull breach at the blast's damage (`blast_breach_damage`): the section
    /// nearest the reactor.
    HullBreach,
    /// A light hit on a random point of the wall (`reactor.json` hits.light, r40).
    HitLight,
    /// A heavy hit on a random point of the wall (`reactor.json` hits.heavy, r40).
    HitHeavy,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AlarmLevel {
    None,
    Warn,
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
    pub fn new(tuning: &RoomTuning, seed: u64) -> Pump {
        Pump { condition: Condition::new(tuning.pump, seed), mode: PumpMode::High, speed: 1.0, tank: 100.0 }
    }

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

    fn use_factor(&self, t: &RoomTuning) -> f64 {
        match self.mode {
            PumpMode::Off => 0.0,
            PumpMode::Low => t.pump_use_low,
            PumpMode::High => t.pump_use_high,
        }
    }

    pub fn tank(&self) -> f64 {
        self.tank
    }

    pub fn set_tank(&mut self, pct: f64) {
        self.tank = pct.clamp(0.0, 100.0);
    }

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

    /// The leak out of the tank, tank fraction per second: none at half condition or better, then
    /// `pump_leak_max * (1 - c / 0.5)^2` (c = condition fraction). Running or not: the seals are worn.
    pub fn leak_rate(&self, t: &RoomTuning) -> f64 {
        let c = self.condition.fraction().clamp(0.0, 1.0);
        if c >= PUMP_LEAK_BELOW {
            0.0
        } else {
            t.pump_leak_max * (1.0 - c / PUMP_LEAK_BELOW).powi(2)
        }
    }

    /// Drains the tank by the leak over `dt` (the tank is in %, the rate in fractions).
    fn leak(&mut self, dt: f64, t: &RoomTuning) {
        let rate = self.leak_rate(t);
        if rate > 0.0 {
            self.tank = (self.tank - rate * 100.0 * dt).max(0.0);
        }
    }

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

    pub fn condition_mut(&mut self) -> &mut Condition {
        &mut self.condition
    }

    pub fn factor(&self) -> f64 {
        self.condition.factor()
    }

    /// A running pump wears as overload while the reactor is above its optimal band.
    fn wear(&mut self, dt: f64, running: bool, excess_degrees: f64, t: &RoomTuning) -> Option<DeviceEvent> {
        let load = if running { 1.0 + excess_degrees.max(0.0) / t.heat_wear_degrees.max(1e-9) } else { 0.0 };
        self.condition.deteriorate(dt, load)
    }
}

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

/// The oxygen generator (r37): makes the engine room's air from the life support's power
/// (`EngineRoom::oxygen_flow`, `fire_core::oxygen`). It wears only while it runs, i.e. while life
/// support gets at least `fire_core::oxygen::MIN_VOLTAGE`, as the pump wears only while it runs.
#[derive(Clone, Debug)]
pub struct OxygenGenerator {
    condition: Condition,
}

impl OxygenGenerator {
    pub fn new(tuning: &RoomTuning, seed: u64) -> OxygenGenerator {
        OxygenGenerator { condition: Condition::new(tuning.oxygen_generator, seed) }
    }

    pub fn condition(&self) -> &Condition {
        &self.condition
    }

    pub fn condition_mut(&mut self) -> &mut Condition {
        &mut self.condition
    }

    /// Normal wear while running; none while it has no power.
    fn wear(&mut self, dt: f64, running: bool) -> Option<DeviceEvent> {
        self.condition.deteriorate(dt, if running { 1.0 } else { 0.0 })
    }
}

/// Shield points one whole unit of a hit's structure damage costs (r40): a heavy hit (0.9) takes
/// 450 of the shield's 1000. Ours, joining PULSAR's shield points to Barotrauma's structure
/// damage. TODO(initiator).
const SHIELD_POINTS_PER_STRUCTURE: f64 = 500.0;

#[derive(Clone, Debug)]
pub struct EngineRoom {
    tuning: RoomTuning,
    reactor: Reactor,
    pump: Pump,
    junction: JunctionBox,
    oxygen: OxygenGenerator,
    grid: Grid,
    level: AlarmLevel,
    engines_full_kw: f64,
    engine: Engine,
    wander: bool,
    wander_rng: Rng,
    wander_target: f64,
    wander_left: f64,
    rods: RodKinds,
    history: History,
    /// Seconds of emergency cooling left (0 when none runs).
    ecool_left: f64,
    /// Seconds until the emergency cooling can be pressed again (0 when ready).
    ecool_cd: f64,
    /// A press since the last step: the next step reports `RoomEvent::EmergencyCool`.
    ecool_pressed: bool,
    aux: bool,
    /// The aux switches of Life Support and Lights (indexed by `Group`; Engines is never aux-fed).
    aux_switch: [bool; Group::ALL.len()],
    /// The engine room's walls (`hull`): each section's damage is one leak in the glue.
    hull: Hull,
    /// The hit presets (`reactor.json` hits) and the rng of the random hits' points.
    hits: HitsTuning,
    hit_rng: Rng,
    /// The battery behind the reactor (`battery`): gives the grid's shortfall, takes its surplus.
    battery: Battery,
    /// The shield (`shield`, r40): fed by the Shields group, its demand is `shield_full_kw` times its share.
    shield: Shield,
    shield_full_kw: f64,
    /// The weapons' capacitor (`weapons`): the WEAPONS group charges it, a shot takes its energy.
    weapons: Weapons,
    /// The jump drive's charge (`jump`, r40 #252): it draws the JUMP group's demand while charging.
    jump: JumpDrive,
    /// The JUMP group's full demand, kW (the tuned value; the grid's own starts at 0).
    jump_full_kw: f64,
    /// The enemy ship's values (`enemy` in reactor.json, r41) and the encounter that runs, if any.
    enemy: EnemyTuning,
    encounter: Option<Encounter>,
    /// The rng of the enemy's shots (the hits' points and kinds): seeded with the room.
    enemy_rng: Rng,
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
            oxygen: OxygenGenerator::new(&t.room, seed.wrapping_add(3)),
            tuning: t.room,
            rods,
            history: History::new(t.graph_points, t.graph_interval_s),
            reactor,
            grid: Grid::from_tuning(&t.grid),
            level: AlarmLevel::None,
            engines_full_kw: t.grid.engines.demand_kw,
            engine: Engine::new(),
            wander: t.room.engine_wander,
            wander_rng: Rng::new(seed ^ 0x7a1d_e51a),
            wander_target: 1.0,
            wander_left: t.room.wander_every_s,
            ecool_left: 0.0,
            ecool_cd: 0.0,
            ecool_pressed: false,
            aux: t.room.aux_enabled,
            aux_switch: [true; Group::ALL.len()],
            hull: Hull::default(),
            hits: t.hits,
            hit_rng: Rng::new(seed ^ 0x6a1f_0b17),
            battery: Battery::new(t.battery),
            shield: Shield::new(t.shield),
            shield_full_kw: t.grid.shields.demand_kw,
            weapons: Weapons::new(t.weapons),
            jump: JumpDrive::new(t.jump),
            jump_full_kw: t.grid.jump_drive.demand_kw,
            enemy: t.enemy.clone(),
            encounter: None,
            enemy_rng: Rng::new(seed ^ 0x3e11_0a5b),
        }
    }

    /// PULSAR's emergency cooling, pressed by the operator: takes `ecool_tank_cost` of the tank at
    /// once and runs `ecool_duration_s` with `ecool_strength` extra cooling, then the cooldown of
    /// `ecool_cooldown_s` starts at the press. Refused (false, nothing taken) during the cooldown, with
    /// the tank below the cost, or with the pump broken.
    pub fn emergency_cool(&mut self) -> bool {
        if !self.ecool_allowed() {
            return false;
        }
        let t = self.tuning;
        self.pump.tank -= t.ecool_tank_cost;
        self.ecool_left = t.ecool_duration_s;
        self.ecool_cd = t.ecool_cooldown_s;
        self.ecool_pressed = true;
        true
    }

    /// A press would be taken now (`emergency_cool` returns true): off the cooldown, the tank holds the
    /// cost, the pump works.
    pub fn ecool_allowed(&self) -> bool {
        self.ecool_cd <= 0.0 && self.pump.condition.functioning() && self.pump.tank >= self.tuning.ecool_tank_cost
    }

    pub fn ecool_active(&self) -> bool {
        self.ecool_left > 0.0
    }

    /// Seconds until the emergency cooling can be pressed again (0 when it can).
    pub fn ecool_ready_in_s(&self) -> f64 {
        self.ecool_cd
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

    pub fn reactor_mut(&mut self) -> &mut Reactor {
        &mut self.reactor
    }

    /// The coolant pump's leak, tank fraction per second (`Pump::leak_rate`): the screen's LEAK and
    /// the steam at the pump both read it. 0 with a sound pump.
    /// The engine room's walls (the glue's leaks and visuals read the sections).
    pub fn hull(&self) -> &Hull {
        &self.hull
    }

    /// The wrench's hold on section `i`: `dt` seconds take its damage down at the pump's fix speed
    /// (`pump.fix_slow_s` for a whole section, 25 s), so a 0.6 section is mended in 15 s of hold.
    pub fn mend_section(&mut self, i: usize, dt: f64) {
        let whole = self.tuning.pump.fix_slow_s.max(1e-6);
        self.hull.mend(i, dt.max(0.0) / whole);
    }

    /// A hit (r40, Barotrauma's damage as data): the walls take `structure` by the falloff, the
    /// machines in range lose `device_damage` by the same falloff, and the impact starts a fire with
    /// `fire_chance` when the hit reaches the room (a section or a machine in range; the roll is
    /// `rng`'s, so a seed gives the same fires). A hit out of range does nothing and draws nothing.
    pub fn apply_hit(&mut self, hit: Hit, rng: &mut Rng) -> Vec<RoomEvent> {
        let mut ev = Vec::new();
        let structure = hit.structure;
        let mut reached = self.hull.hit(hit.at, structure, hit.radius);
        for (source, at) in DEVICES {
            let d = ((at[0] - hit.at[0]).powi(2) + (at[1] - hit.at[1]).powi(2)).sqrt();
            if d >= hit.radius {
                continue;
            }
            reached = true;
            let amount = hit.device_damage * (1.0 - d / hit.radius);
            let condition = match source {
                Source::Pump => &mut self.pump.condition,
                Source::Junction => &mut self.junction.condition,
                _ => &mut self.oxygen.condition,
            };
            if let Some(e) = condition.damage(amount) {
                ev.push(RoomEvent::Device { source, event: e });
            }
        }
        if reached && rng.unit() < hit.fire_chance {
            ev.push(RoomEvent::HitFire { size: self.tuning.hit_fire_size, at: hit.at });
        }
        ev
    }

    /// A hit from outside the ship (r40): the shield takes it first (`Shield::absorb`, PULSAR's
    /// shield before the hull), and what passes reaches the room as a weaker hit
    /// (`apply_hit`, the same share of its structure and device damage). A fully absorbed hit
    /// does nothing to the room. A meltdown blast is inside the ship and does not come here.
    pub fn outside_hit(&mut self, hit: Hit, rng: &mut Rng) -> Vec<RoomEvent> {
        let points = hit.structure * SHIELD_POINTS_PER_STRUCTURE;
        let passed = self.shield.absorb(points);
        let share = if points > 0.0 { passed / points } else { 1.0 };
        if share <= 0.0 {
            return Vec::new();
        }
        self.apply_hit(Hit { structure: hit.structure * share, device_damage: hit.device_damage * share, ..hit }, rng)
    }

    pub fn pump_leak_rate(&self) -> f64 {
        self.pump.leak_rate(&self.tuning)
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

    pub fn oxygen_generator(&self) -> &OxygenGenerator {
        &self.oxygen
    }

    pub fn oxygen_generator_mut(&mut self) -> &mut OxygenGenerator {
        &mut self.oxygen
    }

    /// The generator's air, m³ per second: `rate` at full life support power and condition, with
    /// the condition squared (`fire_core::generator_flow`). The power is the life support group's
    /// factor of the last step; nothing below `MIN_VOLTAGE`.
    pub fn oxygen_flow(&self, rate: f64) -> f64 {
        fire_core::oxygen::generator_flow(self.grid.factor(Group::LifeSupport), rate, self.oxygen.condition.fraction())
    }

    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid
    }

    pub fn battery(&self) -> &Battery {
        &self.battery
    }

    pub fn battery_mut(&mut self) -> &mut Battery {
        &mut self.battery
    }

    pub fn shield(&self) -> &Shield {
        &self.shield
    }

    /// A hit on the ship's shield (`Shield::absorb`): returns what passes to the hull.
    pub fn absorb_hit(&mut self, damage: f64) -> f64 {
        self.shield.absorb(damage)
    }

    pub fn weapons(&self) -> &Weapons {
        &self.weapons
    }

    /// A shot from the GN-01 screen (the FIRE button): true when the gun fired, false when refused
    /// (reloading, or not one shot's energy in the capacitor).
    pub fn fire_weapon(&mut self) -> bool {
        let fired = self.weapons.fire();
        if fired {
            if let Some(e) = self.encounter.as_mut() {
                let dmg = e.shot_damage();
                e.take_shot(dmg);
            }
        }
        fired
    }

    pub fn jump(&self) -> &JumpDrive {
        &self.jump
    }

    pub fn jump_mut(&mut self) -> &mut JumpDrive {
        &mut self.jump
    }

    /// The pilot's jump (`JumpDrive::jump`, r41): when it goes ACTIVE the running encounter ends
    /// Escaped. True when it jumped.
    pub fn jump_go(&mut self) -> bool {
        let jumped = self.jump.jump();
        if jumped {
            if let Some(e) = self.encounter.as_mut() {
                e.escape();
            }
        }
        jumped
    }

    /// The dev AMBUSH on (r41): a new encounter, unless one runs. True when it started.
    pub fn start_ambush(&mut self) -> bool {
        if self.encounter.as_ref().is_some_and(|e| e.outcome() == Outcome::None) {
            return false;
        }
        self.encounter = Some(Encounter::new(&self.enemy, &self.hits));
        true
    }

    /// The dev AMBUSH off (r41): the encounter is gone, whatever its state.
    pub fn stop_ambush(&mut self) {
        self.encounter = None;
    }

    /// The encounter, running or ended (None when there is none).
    pub fn encounter(&self) -> Option<&Encounter> {
        self.encounter.as_ref()
    }

    /// The junction box's voltage: what the grid got over what it asked for, 0..1 (1 with no
    /// request). Below 1 the grid is short (`junction().undervolted()`).
    pub fn voltage(&self) -> f64 {
        self.junction.voltage()
    }

    /// The crew's throttle (the engine's target), -100..100.
    pub fn engine_throttle(&self) -> f64 {
        self.engine.target()
    }

    /// Sets the engines' throttle, -100..100 (clamped). The force follows it over the next steps,
    /// and the Engines demand with the force. While wander is on, the next step moves the target on.
    pub fn set_engine_throttle(&mut self, pct: f64) {
        self.engine.set_target(pct);
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The thrust the engines give now, % (`Engine::thrust` at the junction box's voltage).
    pub fn engine_thrust(&self) -> f64 {
        self.engine.thrust(self.voltage())
    }

    pub fn engine_wander(&self) -> bool {
        self.wander
    }

    /// The dev stand-in for a helm: while on, `step` moves the engines' target with a random walk.
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
    /// delivering nothing (off, cold start, broken) the pump runs on its backup supply at 1.
    /// Otherwise it is the junction box's voltage of the last step, clamped to 0..1: a short grid
    /// slows the pump, and a broken box (which passes nothing) stops it.
    pub fn power_factor(&self) -> f64 {
        if self.reactor.output_kw() <= 0.0 { 1.0 } else { self.junction.voltage().clamp(0.0, 1.0) }
    }

    /// Turns the aux reactor on or off. Off: the main grid carries every group and the boost is 0.
    pub fn set_aux(&mut self, on: bool) {
        self.aux = on;
        self.sync_aux();
    }

    pub fn aux_enabled(&self) -> bool {
        self.aux
    }

    /// Switches a group's aux switch. Life Support and Lights only; the Engines switch is ignored
    /// (Engines is always on the main reactor). Default on.
    pub fn set_aux_switch(&mut self, g: Group, on: bool) {
        if g != Group::Engines {
            self.aux_switch[g.index()] = on;
            self.sync_aux();
        }
    }

    /// The aux switch of a group (Life Support and Lights; Engines always reads on).
    pub fn aux_switch(&self, g: Group) -> bool {
        g == Group::Engines || self.aux_switch[g.index()]
    }

    fn aux_fed(&self, g: Group) -> bool {
        self.aux && g != Group::Engines && self.aux_switch[g.index()]
    }

    /// kW the main reactor gets from what the aux groups leave of the aux output (0 with aux off):
    /// `aux_output_kw` minus the demand of the aux groups that are switched on, at least 0.
    pub fn aux_boost_kw(&self) -> f64 {
        if !self.aux {
            return 0.0;
        }
        let used: f64 = Group::ALL.iter().filter(|&&g| self.aux_fed(g)).map(|&g| self.grid.group(g).demand_kw).sum();
        (self.tuning.aux_output_kw - used).max(0.0)
    }

    /// The load the main grid asks for, kW: with aux on, the Engines group only.
    pub fn main_load_kw(&self) -> f64 {
        if self.aux { self.grid.request_of(Group::Engines) } else { self.grid.request_kw() }
    }

    fn sync_aux(&mut self) {
        self.reactor.set_output_boost_kw(self.aux_boost_kw());
    }

    pub fn set_breaker(&mut self, g: Group, on: bool) {
        self.grid.set_breaker(g, on);
    }

    pub fn set_pump(&mut self, on: bool) {
        self.pump.set_on(on);
    }

    pub fn set_pump_mode(&mut self, mode: PumpMode) {
        self.pump.set_mode(mode);
    }

    /// The reactor's lamps for the load the grid asks for now (`Reactor::warnings`).
    pub fn warnings(&self) -> crate::reactor::Warnings {
        self.reactor.warnings(self.main_load_kw())
    }

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
        let load = self.main_load_kw();
        self.reactor.bands().temp_optimal.contains(self.reactor.temperature())
            && (self.reactor.output_kw() - load).abs() <= WARM_START_TOLERANCE * load
    }

    /// Advances `dt` seconds. Returns the events of this step.
    pub fn step(&mut self, dt: f64, input: Input) -> Vec<RoomEvent> {
        let t = self.tuning;
        let mut ev = Vec::new();
        if std::mem::take(&mut self.ecool_pressed) {
            ev.push(RoomEvent::EmergencyCool);
        }
        // The emergency cooling runs for the whole step it is active at its start, then its timers run down.
        let cooling = self.ecool_left > 0.0;
        self.ecool_left = (self.ecool_left - dt).max(0.0);
        self.ecool_cd = (self.ecool_cd - dt).max(0.0);
        self.reactor.set_extra_cooling(if cooling { t.ecool_strength } else { 0.0 });

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
        // The voltage is the junction box's as the last step left it (Barotrauma's `prevVoltage`).
        self.engine.step(dt, self.voltage());
        self.grid.set_demand(Group::Engines, self.engine.demand_kw(self.engines_full_kw));
        // The shield asks its full demand while it is below max, the upkeep at max. Its regen runs on
        // the Shields factor of the last step (the same one-step lag as the engines' voltage).
        self.grid.set_demand(Group::Shields, self.shield_full_kw * self.shield.demand_share());
        self.shield.step(dt, self.grid.factor(Group::Shields));
        // The WEAPONS group asks for its recharge while the capacitor is not full: a full one asks for
        // nothing, so the grid's power goes to the other groups.
        self.grid.set_demand(Group::Weapons, self.weapons.tuning().recharge_kw * self.weapons.demand_share());
        // The jump drive steps at last step's power (the grid's factor, as the voltage above): a
        // drive armed this step has no power yet, so it is PAUSED for one step at most.
        self.jump.step(dt, self.grid.factor(Group::JumpDrive));
        self.grid.set_demand(Group::JumpDrive, self.jump.demand_kw(self.jump_full_kw));
        let load = self.main_load_kw();
        self.sync_aux();
        let reactor_ev = self.reactor.step(dt, load);
        ev.extend(reactor_ev.iter().map(|e| RoomEvent::Reactor(*e)));
        ev.extend(self.blast_damage(&reactor_ev));
        self.pump.use_coolant(dt, stream, self.reactor.heat_input(), &t);
        self.pump.leak(dt, &t);

        // The enemy's turret (r41): each shot is an outside hit on the wall, the shield takes it first.
        let shot = match self.encounter.as_mut() {
            Some(e) => e.step(dt, &mut self.enemy_rng),
            None => None,
        };
        if let Some(hit) = shot {
            let section = self.encounter.as_ref().map_or(0, |e| e.last_section());
            ev.push(RoomEvent::EnemyShot { section });
            let mut rng = self.hit_rng.clone();
            ev.extend(self.outside_hit(hit, &mut rng));
            self.hit_rng = rng;
        }
        let (through, junction_ev) = self.junction.pass(dt, self.reactor.output_kw(), load, &t);
        ev.extend(junction_ev);
        // The battery sits behind the reactor: it gives what the main grid lacks, and takes the
        // surplus only when nothing was short (`load` is the main grid's request in both modes).
        let shortfall = (load - through).max(0.0);
        let supply = through + self.battery.supply(shortfall, dt);
        if self.aux {
            // The main grid: the Engines group only. The aux reactor: the aux groups that are switched on.
            // Only Life Support and Lights can go to the aux side; every other group stays on the main grid.
            let aux_side = |g: Group| matches!(g, Group::LifeSupport | Group::Lights);
            let main: [f64; Group::ALL.len()] = std::array::from_fn(|i| if aux_side(Group::ALL[i]) { 0.0 } else { self.grid.request_of(Group::ALL[i]) });
            let main_write: [bool; Group::ALL.len()] = std::array::from_fn(|i| !aux_side(Group::ALL[i]));
            let left = self.grid.distribute_with(supply, main, main_write);
            if shortfall <= 0.0 {
                self.battery.absorb(left, dt);
            }
            let mut aux = [0.0; Group::ALL.len()];
            for g in [Group::LifeSupport, Group::Lights] {
                if self.aux_fed(g) {
                    aux[g.index()] = self.grid.group(g).demand_kw;
                }
            }
            let aux_write: [bool; Group::ALL.len()] = std::array::from_fn(|i| aux_side(Group::ALL[i]));
            self.grid.distribute_with(self.tuning.aux_output_kw, aux, aux_write);
        } else {
            let left = self.grid.distribute(supply);
            if shortfall <= 0.0 {
                self.battery.absorb(left, dt);
            }
        }

        // The capacitor takes the share of its demand the grid delivered this step (`factor`).
        self.weapons.step(dt, self.grid.factor(Group::Weapons));

        let excess = self.reactor.temperature() - self.reactor.bands().temp_optimal.hi;
        if let Some(e) = self.pump.wear(dt, self.pump.on() && self.reactor.power_on(), excess, &t) {
            ev.push(RoomEvent::Device { source: Source::Pump, event: e });
        }
        let running = self.grid.factor(Group::LifeSupport) >= fire_core::oxygen::MIN_VOLTAGE;
        if let Some(e) = self.oxygen.wear(dt, running) {
            ev.push(RoomEvent::Device { source: Source::OxygenGenerator, event: e });
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

    /// Throws the core out (`Reactor::eject`, PULSAR's core eject). The events are the reactor's:
    /// `CoreEjected` alone, or none when the core is out already. No blast: an ejected core cannot melt.
    pub fn eject_core(&mut self) -> Vec<RoomEvent> {
        self.reactor.eject().into_iter().map(RoomEvent::Reactor).collect()
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
        // The blast tears the walls near the reactor open (Barotrauma's explosions damage the walls):
        // the same falloff as a hit, over the blast's range.
        self.hull.hit(REACTOR_AT, self.tuning.blast_breach_damage, blast.range_m);
        let mut ev = Vec::new();
        if let Some(e) = self.pump.condition.damage(blast.device_damage) {
            ev.push(RoomEvent::Device { source: Source::Pump, event: e });
        }
        if let Some(e) = self.junction.condition.damage(blast.device_damage) {
            ev.push(RoomEvent::Device { source: Source::Junction, event: e });
        }
        if let Some(e) = self.oxygen.condition.damage(blast.device_damage) {
            ev.push(RoomEvent::Device { source: Source::OxygenGenerator, event: e });
        }
        ev
    }

    pub fn trigger(&mut self, fault: Fault) -> Vec<RoomEvent> {
        match fault {
            Fault::PumpBreak => {
                let to = (self.pump.condition.tuning().broken_below - 1.0).max(0.0);
                let amount = (self.pump.condition.value() - to).max(0.0);
                self.pump.condition.damage(amount).map(|e| RoomEvent::Device { source: Source::Pump, event: e }).into_iter().collect()
            }
            Fault::OxygenGeneratorBreak => {
                let to = (self.oxygen.condition.tuning().broken_below - 1.0).max(0.0);
                let amount = (self.oxygen.condition.value() - to).max(0.0);
                self.oxygen.condition.damage(amount).map(|e| RoomEvent::Device { source: Source::OxygenGenerator, event: e }).into_iter().collect()
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
            Fault::HullBreach => {
                let i = self.hull.nearest(REACTOR_AT);
                self.hull.set(i, self.tuning.blast_breach_damage);
                Vec::new()
            }
            Fault::HitLight | Fault::HitHeavy => {
                let preset = if fault == Fault::HitLight { self.hits.light } else { self.hits.heavy };
                // The point comes from a clone of the rng, so the room's own rng is kept as it is.
                let mut rng = self.hit_rng.clone();
                let at = Hull::wall_point(rng.unit());
                let ev = self.outside_hit(preset.at(at), &mut rng);
                self.hit_rng = rng;
                ev
            }
        }
    }

    /// One step of the wander: a new target every `wander_every_s`, in [wander_min, 1] of full
    /// throttle. The engine's force follows the target on its own.
    fn wander_step(&mut self, dt: f64, t: &RoomTuning) {
        let lo = t.wander_min.clamp(0.0, 1.0);
        self.wander_left -= dt;
        if self.wander_left <= 0.0 {
            self.wander_target = lo + self.wander_rng.unit() * (1.0 - lo);
            self.wander_left = t.wander_every_s;
            self.engine.set_target(100.0 * self.wander_target);
        }
    }

    fn compute_level(&self) -> AlarmLevel {
        if self.reactor.alarm(AlarmKind::TempCritical) {
            AlarmLevel::Danger
        } else if self.reactor.alarm(AlarmKind::TempHigh)
            || self.reactor.condition().needs_repair()
            || self.pump.condition.needs_repair()
            || self.junction.condition.needs_repair()
            || self.oxygen.condition.needs_repair()
            || !self.hull.intact()
        {
            AlarmLevel::Warn
        } else {
            AlarmLevel::None
        }
    }
}
