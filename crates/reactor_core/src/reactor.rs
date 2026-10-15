//! The reactor. Fission makes heat (as much as the fuel allows), the turbine turns heat into power
//! TODO(initiator).
//! Barotrauma's NeedMoreFuel(1.0) and TooMuchFuel for the screen. Values: TODO(initiator).
//! back to functioning, the reactor has a new, empty core. Values and rules: TODO(initiator).
//! Values: TODO(initiator).
//! Values: TODO(initiator).
//! Values: TODO(initiator), Barotrauma's.
use std::collections::BTreeMap;

use device_core::{Condition, DeviceEvent, DeviceTuning};
use gameplay_core::notice::{Notice, NoticeKind};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// fuel. The same values travel in `ReactorEvent::Blast`. TODO(initiator): Barotrauma's values.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Blast {
    /// The blast only happens with more available fuel than this (Barotrauma: AvailableFuel > 1).
    pub min_fuel: f64,
    /// Range of the blast's burn and stun for the players, metres (Barotrauma 750 units, 100 per m).
    pub range_m: f64,
    pub burn: f64,
    pub stun_s: f64,
    pub device_damage: f64,
    /// Range of the radiation for the players, metres (Barotrauma 2000 units, 20 m), ignores cover.
    pub radiation_range_m: f64,
    /// Radiation sickness, scaled by the players' max vitality (Barotrauma: 75).
    pub radiation: f64,
}

impl Default for Blast {
    fn default() -> Blast {
        Blast { min_fuel: 1.0, range_m: 7.5, burn: 500.0, stun_s: 15.0, device_damage: 500.0, radiation_range_m: 20.0, radiation: 75.0 }
    }
}

/// Reactor values. TODO(initiator): starting values. Delays, burn rate, temperature rate and
/// controller speed follow the original; the output, the meltdown delay, the alarm
/// lines and the fire size are guesses.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReactorTuning {
    pub max_output_kw: f64,
    pub slots: usize,
    /// Rod life (0..100) used per second at fission 100.
    pub fuel_burn: f64,
    /// Temperature change per second at most.
    pub temp_rate: f64,
    /// Seconds above the optimal band per fire (at full condition).
    pub fire_delay_s: f64,
    pub meltdown_delay_s: f64,
    /// Turbine target change per second of the automatic controller (it follows the load).
    pub auto_turbine_speed: f64,
    /// Fission target change per second of the automatic controller. Slower than the turbine, so
    /// after a load drop the temperature climbs before fission catches up.
    pub auto_fission_speed: f64,
    /// Size of a fire the reactor starts, metres (`fire_core`).
    pub fire_size: f64,
    /// Temperature alarms switch off this far below their line.
    pub alarm_hysteresis: f64,
    /// Fuel alarm below this much rod life in total (one rod is 100); off again above `* 1.25`.
    pub fuel_low: f64,
    /// Heat the pump takes away at full output, as an offset of the temperature target (degrees).
    /// TODO(initiator): value. Two rods are needed to hold the optimal band with it at full load.
    pub cooling_max: f64,
    /// Residual (decay) heat in degrees of temperature target: it is there only without coolant
    /// (`(1 - flow) * decay * decay_heat`, see `decay_lag_s`). The turbine does not take it.
    /// TODO(initiator): value. Without coolant it keeps the core hot after power off and above the
    /// allowed band when fission is cut to nothing.
    pub decay_heat: f64,
    /// Seconds for the coolant flow to run down after the pump stops (and to come back): the
    /// water in the pipes coasts. TODO(initiator): value.
    pub coolant_coast_s: f64,
    /// Stiffness of the spring that pulls the temperature to its target, 1/s². With
    /// `temp_damping` it sets the swing (about 25 s with the starting values) and the overshoot
    /// of a step (about a quarter). TODO(initiator): value.
    pub temp_stiffness: f64,
    /// Damping of the temperature's speed, 1/s. TODO(initiator): value.
    pub temp_damping: f64,
    /// Heat the delivered output makes at full output, degrees of temperature target. Taken from
    /// the output of the last step. TODO(initiator): value.
    pub heat_output_factor: f64,
    /// Seconds for fission to follow its target (a time constant). Barotrauma: 1. TODO(initiator): value.
    pub fission_lag_s: f64,
    /// Seconds for the decay heat to follow fission (a time constant): the core's decay state
    /// rises with fission and fades after power off over minutes. Ours, not Barotrauma's (it has no
    /// decay heat). TODO(initiator): value.
    pub decay_lag_s: f64,
    /// Auto looks this many seconds ahead: it compares the temperature it predicts (temp + speed *
    /// this) with the band, so the spring's swing is not chased. TODO(initiator): value.
    pub auto_lookahead_s: f64,
    /// Size of a temperature boost, degrees of heat: the operator's plus and minus add this much
    /// (Barotrauma `TemperatureBoostAmount`, 25). TODO(initiator): value.
    pub boost_amount: f64,
    /// How fast a boost fades back to 0, per second (Barotrauma: 1). TODO(initiator): value.
    pub boost_decay: f64,
    /// Seconds the EJECT cover stays open after ARM EJECT (the second press must come inside it).
    /// The host keeps the timer (the glue's). TODO(initiator): value.
    pub eject_arm_s: f64,
    /// The meltdown blast (`Blast`). TODO(initiator): values.
    pub blast: Blast,
    /// Inertia on (the default): the temperature is the damped spring, the output makes heat and
    /// fission lags by `fission_lag_s`. Off: the first-order path of Barotrauma (the temperature moves
    /// at `temp_rate` towards its target, fission lags 1 s, no output heat), for the oracle tests.
    #[serde(default = "inertia_on")]
    pub inertia: bool,
    /// The reactor's own condition: repair threshold and wear floor (Barotrauma style, the floor
    /// is the lowest that normal wear reaches; the broken limit is the floor at least).
    pub condition: DeviceTuning,
}

fn inertia_on() -> bool {
    true
}

impl Default for ReactorTuning {
    fn default() -> ReactorTuning {
        ReactorTuning {
            max_output_kw: 2000.0,
            slots: 4,
            fuel_burn: 0.2,
            temp_rate: 10.0,
            fire_delay_s: 30.0,
            meltdown_delay_s: 120.0,
            auto_turbine_speed: 4.0,
            auto_fission_speed: 0.5,
            fire_size: 0.1,
            alarm_hysteresis: 3.0,
            fuel_low: 20.0,
            cooling_max: 40.0,
            decay_heat: 150.0,
            coolant_coast_s: 10.0,
            temp_stiffness: 0.06,
            temp_damping: 0.2,
            heat_output_factor: 10.0,
            fission_lag_s: 4.0,
            decay_lag_s: 90.0,
            auto_lookahead_s: 8.0,
            boost_amount: 25.0,
            boost_decay: 1.0,
            eject_arm_s: 5.0,
            blast: Blast::default(),
            inertia: true,
            condition: DeviceTuning { repair_threshold: 50.0, wear_floor: 10.0, ..DeviceTuning::default() },
        }
    }
}

/// Auto leaves fission alone while the temperature is this close to the middle of its band.
const AUTO_DEAD_BAND: f64 = 1.0;
/// Auto's fission speed is full this many degrees (predicted) from the middle of its optimal band;
/// nearer, it is proportional. TODO(initiator): 5 did not settle with the spring (the swing stayed
/// about 10 degrees); 20 settles.
const AUTO_FULL_SPEED_AT: f64 = 20.0;

#[derive(Clone, Debug, PartialEq)]
pub struct FuelRod {
    /// The kind's id in `reactor.json` (`rods`): "standard", "hot" or "spicy".
    pub kind: String,
    /// How much fission it allows (the sum over all rods caps fission and scales heat).
    pub heat: f64,
    pub life: f64,
}

impl FuelRod {
    /// The common rod, as `reactor.json` has it under "standard". TODO(initiator): values.
    pub fn standard() -> FuelRod {
        FuelRod { kind: "standard".into(), heat: 80.0, life: 100.0 }
    }

    pub fn spent(&self) -> bool {
        self.life <= 0.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RodKind {
    pub heat: f64,
    pub life: f64,
}

/// The rod kinds by id. The default is the shipped table (standard 80, hot 100, spicy 150, all
/// at life 100). TODO(initiator): hot and spicy values.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct RodKinds(pub BTreeMap<String, RodKind>);

impl Default for RodKinds {
    fn default() -> RodKinds {
        let kind = |heat| RodKind { heat, life: 100.0 };
        RodKinds(BTreeMap::from([
            ("hot".to_string(), kind(100.0)),
            ("spicy".to_string(), kind(150.0)),
            ("standard".to_string(), kind(80.0)),
        ]))
    }
}

impl RodKinds {
    /// A new rod of kind `id` (life when new), or None for an unknown id.
    pub fn rod(&self, id: &str) -> Option<FuelRod> {
        self.0.get(id).map(|k| FuelRod { kind: id.to_string(), heat: k.heat, life: k.life })
    }
}

/// What the fuel needs, for the screen (Barotrauma's advice to the operator): `More` when the rods
/// cannot carry the load, `Less` when they make more heat than the turbine can take away, `Ok`
/// otherwise.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FuelNeed {
    Ok,
    More,
    Less,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Manual,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Band {
    pub lo: f64,
    pub hi: f64,
}

impl Band {
    pub fn mid(&self) -> f64 {
        (self.lo + self.hi) * 0.5
    }

    pub fn contains(&self, v: f64) -> bool {
        v > self.lo && v < self.hi
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Bands {
    pub temp_optimal: Band,
    pub temp_allowed: Band,
    pub turbine_optimal: Band,
    pub turbine_allowed: Band,
    pub fission_optimal: Band,
    pub fission_allowed: Band,
}

impl Bands {
    /// Bands for an operator of `skill` (0..100), the turbine set point that matches the load,
    /// and the available fuel. Higher skill widens every band.
    pub fn new(skill: f64, turbine_for_load: f64, fuel: f64) -> Bands {
        let d = (skill / 100.0).clamp(0.0, 1.0);
        let lerp = |a: f64, b: f64| a + (b - a) * d;
        let half_opt = lerp(2.5, 10.0);
        let half_all = lerp(5.0, 20.0);
        let mut fis_opt = Band { lo: lerp(30.0, 20.0), hi: lerp(fuel - 20.0, fuel - 10.0) };
        fis_opt.lo = fis_opt.lo.min(fis_opt.hi - 10.0);
        let mut fis_all = Band { lo: lerp(20.0, 10.0), hi: fuel };
        fis_all.lo = fis_all.lo.min(fis_all.hi - 10.0);
        Bands {
            temp_optimal: Band { lo: lerp(40.0, 30.0), hi: lerp(60.0, 70.0) },
            temp_allowed: Band { lo: lerp(30.0, 10.0), hi: lerp(70.0, 90.0) },
            turbine_optimal: Band { lo: turbine_for_load - half_opt, hi: turbine_for_load + half_opt },
            turbine_allowed: Band { lo: turbine_for_load - half_all, hi: turbine_for_load + half_all },
            fission_optimal: fis_opt,
            fission_allowed: fis_all,
        }
    }
}

/// Alarm kinds the reactor raises. Each switches on and off with hysteresis.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AlarmKind {
    TempHigh,
    TempCritical,
    FuelLow,
}

impl AlarmKind {
    pub const ALL: [AlarmKind; 3] = [AlarmKind::TempHigh, AlarmKind::TempCritical, AlarmKind::FuelLow];

    /// The notice for this alarm switching on. Text keys `reactor.<kind>`; the text pools are
    /// the glue's (TODO(initiator): wording).
    pub fn notice(self) -> Notice {
        let key = match self {
            AlarmKind::TempHigh => "reactor.temp_high",
            AlarmKind::TempCritical => "reactor.temp_critical",
            AlarmKind::FuelLow => "reactor.fuel_low",
        };
        Notice::new(NoticeKind::Warning, key)
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ReactorEvent {
    Fire { size: f64 },
    /// The core melted. The reactor is broken and cold until a repair brings it back to
    /// functioning; what it does to the ship is the glue's.
    Meltdown,
    /// The meltdown blast: sent right after `Meltdown` when the core had more than `blast.min_fuel`
    /// available fuel. The values are the reactor's `blast` tuning.
    Blast(Blast),
    Alarm { kind: AlarmKind, on: bool },
    /// The core was thrown out (`Reactor::eject`). No melt and no blast follow it.
    CoreEjected,
    Device(DeviceEvent),
}

#[derive(Clone, Debug)]
pub struct Reactor {
    tuning: ReactorTuning,
    mode: Mode,
    power_on: bool,
    melted: bool,
    target_fission: f64,
    target_turbine: f64,
    fission: f64,
    turbine: f64,
    temp: f64,
    turbine_for_load: f64,
    skill: f64,
    rods: Vec<FuelRod>,
    /// Available fuel: the sum of the live rods' heat, kept in step with the rods (see `sync_fuel`).
    fuel: f64,
    /// The load the grid asked for in the last step, kW (for `fuel_need`).
    load: f64,
    fire_timer: f64,
    overheat_timer: f64,
    output: f64,
    alarms: [bool; 3],
    condition: Condition,
    /// Output factor of the coolant pump, 0..1 (set by the engine room each step).
    coolant_target: f64,
    /// The coolant flow that takes heat away now, 0..1: follows `coolant_target` with a coast.
    coolant: f64,
    heat_in: f64,
    /// The core was ejected: no core, no rods, no melt. Cleared by a repair (a new core).
    ejected: bool,
    /// Speed of the temperature, degrees per second.
    temp_vel: f64,
    /// Decay state, 0..1: follows fission / 100 with `decay_lag_s`; it drives the residual heat.
    /// Kept through power off, reset by a meltdown.
    decay: f64,
    /// Temperature boost (degrees of heat, -amount..amount), fades to 0 at `boost_decay`.
    boost: f64,
    /// kW added to the maximum output (the aux reactor's spare, set by the engine room). 0 by default.
    output_boost_kw: f64,
    /// Factor on Auto's two speeds, 1 in play. The engine room's warm start raises it (Barotrauma's
    /// `UpdateAutoTemp(100, …)`) and sets it back to 1 after.
    auto_boost: f64,
    /// Extra cooling in degrees of target, on top of the pump's (`cooling_max`). The engine room sets
    /// it while an emergency cooling runs (`EngineRoom::emergency_cool`), 0 otherwise.
    extra_cool: f64,
}

/// The lamps of the panel (Barotrauma's warning lights, client Reactor.cs 621-632). Each is on
/// or off, steady (the blink is the glue's); `short` gives the letters for a trace.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Warnings {
    pub overheat: bool,
    /// Temperature above the allowed band (client 621, critical heat). Power on only.
    pub critical_heat: bool,
    pub low_temp: bool,
    /// Temperature below the allowed band (client 622, cold). Power on only.
    pub cold: bool,
    pub high_output: bool,
    pub critical_output: bool,
    pub low_output: bool,
    pub fuel_out: bool,
    pub low_fuel: bool,
    pub meltdown: bool,
    /// Warm (above 0.1) with the power off (client 632).
    pub scram: bool,
}

impl Warnings {
    /// Short letters of the lamps that are on, in panel order, e.g. "OH HO LF"; "" when none.
    pub fn short(&self) -> String {
        let lamps = [
            (self.overheat, "OH"),
            (self.critical_heat, "CH"),
            (self.low_temp, "LT"),
            (self.cold, "CL"),
            (self.high_output, "HO"),
            (self.critical_output, "CO"),
            (self.low_output, "LO"),
            (self.fuel_out, "FO"),
            (self.low_fuel, "LF"),
            (self.meltdown, "MD"),
            (self.scram, "SC"),
        ];
        lamps.iter().filter(|(on, _)| *on).map(|(_, s)| *s).collect::<Vec<_>>().join(" ")
    }
}

impl Reactor {
    pub fn new(tuning: ReactorTuning, seed: u64) -> Reactor {
        Reactor {
            tuning,
            mode: Mode::Auto,
            power_on: false,
            melted: false,
            target_fission: 0.0,
            target_turbine: 0.0,
            fission: 0.0,
            turbine: 0.0,
            temp: 0.0,
            turbine_for_load: 0.0,
            skill: 0.0,
            rods: Vec::new(),
            fuel: 0.0,
            load: 0.0,
            fire_timer: 0.0,
            overheat_timer: 0.0,
            output: 0.0,
            alarms: [false; 3],
            condition: Condition::new(tuning.condition, seed),
            coolant_target: 1.0,
            coolant: 1.0,
            heat_in: 0.0,
            ejected: false,
            temp_vel: 0.0,
            decay: 0.0,
            boost: 0.0,
            output_boost_kw: 0.0,
            auto_boost: 1.0,
            extra_cool: 0.0,
        }
    }

    /// Switches the reactor on or off. Refused (false) while the reactor is broken.
    pub fn set_power(&mut self, on: bool) -> bool {
        if on && !self.condition.functioning() {
            return false;
        }
        self.power_on = on;
        true
    }

    /// Melts the core now, as if its meltdown timer had run out (the harness's forced melt, for a
    /// melt with no fuel or a script that does not wait for the timer). Nothing when the reactor is
    /// broken already. The events are those of a melt in `step`.
    pub fn force_meltdown(&mut self) -> Vec<ReactorEvent> {
        let mut ev = Vec::new();
        if self.condition.functioning() && !self.ejected {
            self.meltdown(&mut ev);
        }
        ev
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// Sets the targets (0..100) by hand; this switches to Manual.
    pub fn set_targets(&mut self, fission: f64, turbine: f64) {
        self.mode = Mode::Manual;
        self.target_fission = fission.clamp(0.0, 100.0);
        self.target_turbine = turbine.clamp(0.0, 100.0);
    }

    /// Output factor of the coolant pump, 0..1: scales the cooling offset.
    pub fn set_coolant(&mut self, factor: f64) {
        self.coolant_target = factor.clamp(0.0, 1.0);
    }

    /// The coolant flow that takes heat away now, 0..1 (lags the pump's factor).
    pub fn coolant_flow(&self) -> f64 {
        self.coolant
    }

    /// Extra cooling, degrees of temperature target, on top of the pump's cooling (0 with none). The
    /// engine room's emergency cooling sets it for its duration; nothing else reads it.
    pub fn set_extra_cooling(&mut self, degrees: f64) {
        self.extra_cool = degrees.max(0.0);
    }

    pub fn heat_input(&self) -> f64 {
        self.heat_in
    }

    /// Skill (0..100) of whoever runs the reactor; it widens the bands. 0 when nobody does.
    pub fn set_operator_skill(&mut self, skill: f64) {
        self.skill = skill.clamp(0.0, 100.0);
    }

    /// Multiplies Auto's turbine and fission speeds by `factor` (1 is the tuning's own). For the
    /// engine room's warm start only.
    /// Sets the kW added to the maximum output (`max_output_kw + boost` is the range's max); the
    /// engine room sets it from the aux reactor's spare each step. Negative values count as 0.
    pub fn set_output_boost_kw(&mut self, kw: f64) {
        self.output_boost_kw = kw.max(0.0);
    }

    pub fn set_auto_boost(&mut self, factor: f64) {
        self.auto_boost = factor.max(0.0);
    }

    /// The life of every rod, in slot order (for a warm start to put back).
    pub fn rod_lives(&self) -> Vec<f64> {
        self.rods.iter().map(|r| r.life).collect()
    }

    /// Puts the rods' life back, slot by slot (`lives` as `rod_lives` gave it, same rods). Caps
    /// fission to the fuel left, as `sync_fuel` does.
    pub fn set_rod_lives(&mut self, lives: &[f64]) {
        for (rod, life) in self.rods.iter_mut().zip(lives) {
            rod.life = life.clamp(0.0, 100.0);
        }
        self.sync_fuel();
    }

    /// Puts a rod into a free slot; gives it back when all slots are full.
    pub fn insert_rod(&mut self, rod: FuelRod) -> Result<(), FuelRod> {
        if self.ejected || self.rods.len() >= self.tuning.slots {
            return Err(rod);
        }
        self.rods.push(rod);
        self.sync_fuel();
        Ok(())
    }

    pub fn remove_rod(&mut self, slot: usize) -> Option<FuelRod> {
        let rod = (slot < self.rods.len()).then(|| self.rods.remove(slot))?;
        self.sync_fuel();
        Some(rod)
    }

    /// The emergency brake (Barotrauma's: pulling a rod). Takes the rod in
    /// `slot` out and returns it with the handling heat: the core's temperature at that moment
    /// (what the glue burns the hand with; TODO(initiator): the burn rule). The fuel goes at once:
    /// see `sync_fuel`. None for an empty slot.
    pub fn pull_rod(&mut self, slot: usize) -> Option<(FuelRod, f64)> {
        let rod = self.remove_rod(slot)?;
        Some((rod, self.temp))
    }

    /// Throws the core out (PULSAR's core eject). Everything the core did stops at once: fission,
    /// turbine, temperature, output, decay, boost, both timers and the targets go to 0, the power
    /// goes off, and every rod leaves with the core (not given back). The reactor is broken
    /// (condition 0) and `ejected` until a repair installs a new, empty core. Ejecting an ejected
    /// reactor does nothing; the events are `CoreEjected` alone, or none.
    pub fn eject(&mut self) -> Vec<ReactorEvent> {
        if self.ejected {
            return Vec::new();
        }
        self.ejected = true;
        self.melted = false;
        self.power_on = false;
        self.rods.clear();
        self.fuel = 0.0;
        self.fission = 0.0;
        self.turbine = 0.0;
        self.target_fission = 0.0;
        self.target_turbine = 0.0;
        self.temp = 0.0;
        self.temp_vel = 0.0;
        self.output = 0.0;
        self.heat_in = 0.0;
        self.decay = 0.0;
        self.boost = 0.0;
        self.fire_timer = 0.0;
        self.overheat_timer = 0.0;
        self.condition.set_value(0.0);
        vec![ReactorEvent::CoreEjected]
    }

    /// Brings the available fuel up to the rods and caps fission to it at once. Barotrauma caps
    /// the fission target and lets fission follow it, and its heat uses last step's fuel; a pulled
    /// rod must not leave a step of full fission on the rods that are left.
    fn sync_fuel(&mut self) {
        self.fuel = self.available_fuel();
        self.fission = self.fission.min(self.fuel);
    }

    /// The temperature boost (Barotrauma's plus and minus buttons): sets the boost to +`boost_amount`
    /// (`up`) or -`boost_amount`, replacing the old one. Refused (false, the boost unchanged) when
    /// `boost_allowed` is false, while the power is off, or while the reactor is broken.
    pub fn boost(&mut self, up: bool) -> bool {
        if !self.power_on || !self.condition.functioning() || !self.boost_allowed() {
            return false;
        }
        let amount = self.tuning.boost_amount;
        self.boost = if up { amount } else { -amount };
        true
    }

    /// Barotrauma's AllowTemperatureBoost: |boost| < 90 % of the amount (Reactor.cs:52).
    pub fn boost_allowed(&self) -> bool {
        self.boost.abs() < self.tuning.boost_amount * 0.9
    }

    // ---------- readouts ----------

    pub fn temperature_boost(&self) -> f64 {
        self.boost
    }

    pub fn tuning(&self) -> &ReactorTuning {
        &self.tuning
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn power_on(&self) -> bool {
        self.power_on
    }
    /// The core was ejected (`eject`): the screen shows NO CORE until a repair installs a new one.
    pub fn ejected(&self) -> bool {
        self.ejected
    }
    /// The core has melted: the screen shows MELTDOWN until the reactor is repaired back to
    /// functioning, and then it is a reactor again (the rods keep their life).
    pub fn melted(&self) -> bool {
        self.melted
    }
    pub fn temperature(&self) -> f64 {
        self.temp
    }
    /// The decay state, 0..1: the residual heat's share (follows fission / 100 with `decay_lag_s`).
    pub fn decay(&self) -> f64 {
        self.decay
    }
    pub fn temp_velocity(&self) -> f64 {
        self.temp_vel
    }
    pub fn fission(&self) -> f64 {
        self.fission
    }
    pub fn turbine(&self) -> f64 {
        self.turbine
    }
    pub fn targets(&self) -> (f64, f64) {
        (self.target_fission, self.target_turbine)
    }
    pub fn output_kw(&self) -> f64 {
        self.output
    }
    pub fn rods(&self) -> &[FuelRod] {
        &self.rods
    }
    pub fn available_fuel(&self) -> f64 {
        self.rods.iter().filter(|r| !r.spent()).map(|r| r.heat).sum()
    }
    /// Heat per fission at the current fuel: `fission * fuel / 100 * 2` (Reactor.cs:500-503,
    /// without its temperature boost).
    fn heat_at(&self, fission: f64) -> f64 {
        fission * (self.fuel / 100.0) * 2.0
    }
    /// What the fuel needs for the load now (`FuelNeed`). The first check wins: More, then Less.
    pub fn fuel_need(&self) -> FuelNeed {
        if self.need_more() {
            FuelNeed::More
        } else if self.too_much() {
            FuelNeed::Less
        } else {
            FuelNeed::Ok
        }
    }
    /// Barotrauma's NeedMoreFuel(minimumOutputRatio 1.0, minCondition 0), Reactor.cs:510-531: no
    /// fuel left while the grid asks, or the most the fuel can make (fission capped to it, turbine
    /// at 100, the temperature factor of the heat) is below the load.
    fn need_more(&self) -> bool {
        let remaining: f64 = self.rods.iter().map(|r| r.life).sum();
        if remaining <= 0.0 && self.load > 0.0 {
            return true;
        }
        let max_fission = self.fuel.min(100.0);
        if max_fission >= 100.0 {
            return false;
        }
        let temp_factor = (self.heat_at(max_fission) / 50.0).min(1.0);
        let max_output = temp_factor * self.tuning.max_output_kw;
        max_output < self.load
    }
    /// Barotrauma's TooMuchFuel, Reactor.cs:533-543: with more than one rod, the heat at the low
    /// end of the optimal fission band is above what the turbine can take (1.5 times the turbine
    /// for the load, at most 90).
    fn too_much(&self) -> bool {
        if self.rods.len() <= 1 {
            return false;
        }
        let b = self.bands();
        self.heat_at(b.fission_optimal.lo) > (self.turbine_for_load * 1.5).min(90.0)
    }
    /// Core stability, 0..1: 1 minus the meltdown timer's fraction (`timers().1`). 0 when the core
    /// has melted or was ejected; 1 when it is off and cold. No dynamics of its own.
    pub fn stability(&self) -> f64 {
        if self.melted || self.ejected {
            return 0.0;
        }
        (1.0 - self.timers().1).clamp(0.0, 1.0)
    }
    pub fn timers(&self) -> (f64, f64) {
        (self.fire_timer / self.tuning.fire_delay_s, self.overheat_timer / self.tuning.meltdown_delay_s)
    }
    pub fn bands(&self) -> Bands {
        Bands::new(self.skill, self.turbine_for_load, self.fuel)
    }
    /// The panel's lamps, for a grid asking `load_kw` now (the last step's output and fuel). The
    /// conditions are Barotrauma's (client Reactor.cs 621-632). Barotrauma blinks them at 2 Hz
    /// (client 619, `TotalTime % 0.5 < 0.25`), the glue's job here. With the power off every lamp is
    /// off except `meltdown`, whose timer part does not need the power (client 631, and on when the
    /// core melted), and `scram`, which is on when it is warm (client 632).
    pub fn warnings(&self, load_kw: f64) -> Warnings {
        let b = self.bands();
        let on = self.power_on;
        let out = self.output;
        let t = self.tuning;
        Warnings {
            // Client 625: temperature above the optimal band.
            overheat: on && self.temp > b.temp_optimal.hi,
            // Client 621: temperature above the allowed band.
            critical_heat: on && self.temp > b.temp_allowed.hi,
            // Client 627: temperature below the optimal band.
            low_temp: on && self.temp < b.temp_optimal.lo,
            // Client 622: temperature below the allowed band.
            cold: on && self.temp < b.temp_allowed.lo,
            // Client 626: output above 110 % of the load.
            high_output: on && out > load_kw * 1.1,
            // Client 623: output above 150 % of the load.
            critical_output: on && out > load_kw * 1.5,
            // Client 628: output below 90 % of the load.
            low_output: on && out < load_kw * 0.9,
            // Client 629: available fuel below 1 % of the fission.
            fuel_out: on && self.fuel < self.fission * 0.01,
            // Client 630: available fuel below the fission.
            low_fuel: on && self.fuel < self.fission,
            // Client 631: the meltdown timer over half its delay, or the condition at 0 with the
            // power on (Barotrauma), or the core melted (ours: the lamp stays on for the melt).
            meltdown: self.overheat_timer > t.meltdown_delay_s * 0.5 || (self.condition.value() <= 0.0 && on) || self.melted,
            // Client 632: temperature above 0.1 with the power off (not blinked there).
            scram: !on && self.temp > 0.1,
        }
    }

    pub fn alarm(&self, kind: AlarmKind) -> bool {
        self.alarms[kind.index()]
    }
    pub fn condition(&self) -> &Condition {
        &self.condition
    }
    pub fn condition_mut(&mut self) -> &mut Condition {
        &mut self.condition
    }

    /// Advances `dt` seconds with the grid asking for `load_kw`. Returns what happened; the
    /// output is `output_kw()`.
    pub fn step(&mut self, dt: f64, load_kw: f64) -> Vec<ReactorEvent> {
        let t = self.tuning;
        let mut ev = Vec::new();
        let k = dt.min(1.0);
        let coast = (dt / t.coolant_coast_s.max(1e-9)).min(1.0);
        self.coolant += (self.coolant_target - self.coolant) * coast;
        if self.melted && self.condition.functioning() {
            self.melted = false;
        }
        // Repaired back to functioning after an eject: a new, empty core (the rods are not back).
        if self.ejected && self.condition.functioning() {
            self.ejected = false;
        }

        self.heat_in = 0.0;
        if !self.condition.functioning() {
            // Broken: off the grid, cools fast, targets run down. Fission and turbine stay where
            // they were (the original freezes them too).
            self.power_on = false;
            self.output = 0.0;
            self.temp = (self.temp - 100.0 * dt).max(0.0);
            self.temp_vel = 0.0;
            self.target_fission = (self.target_fission - 10.0 * dt).max(0.0);
            self.target_turbine = (self.target_turbine - 10.0 * dt).max(0.0);
            self.fire_timer = 0.0;
            self.overheat_timer = 0.0;
            self.fade_boost(dt);
            self.update_alarms(&mut ev);
            return ev;
        }

        self.sync_fuel();
        let fuel = self.fuel;
        self.load = load_kw;
        if t.max_output_kw > 0.0 {
            let want = load_kw / t.max_output_kw * 100.0;
            self.turbine_for_load += (want - self.turbine_for_load).clamp(-20.0, 20.0) * dt;
        }
        let bands = Bands::new(self.skill, self.turbine_for_load, fuel);

        // Off: the targets are zero before the rods and the turbine follow them (a manual setting
        // the glue sends every step does not bring fission back).
        if !self.power_on {
            self.target_fission = 0.0;
            self.target_turbine = 0.0;
        }
        // The boost adds to the heat (Barotrauma GetGeneratedHeat, Reactor.cs:502).
        let heat = self.heat_at(self.fission) + self.boost;
        self.heat_in = heat;
        let cool = t.cooling_max * self.coolant + self.extra_cool;
        // Decay heat follows fission with a slow lag (`decay`): power off stops fission at once (the targets are 0) and the
        // decay heat fades over minutes. Inertia off: the old path, the heat with fission at once.
        let decay_level = if t.inertia {
            let lag = t.decay_lag_s.max(1e-9);
            self.decay += (self.fission / 100.0 - self.decay) * (dt / lag).min(1.0);
            self.decay
        } else {
            self.fission / 100.0
        };
        let residual = t.decay_heat * (1.0 - self.coolant) * decay_level;
        let output_heat = if t.inertia && t.max_output_kw > 0.0 { t.heat_output_factor * self.output / t.max_output_kw } else { 0.0 };
        self.step_temp(dt, heat + residual + output_heat - self.turbine - cool);
        self.fade_boost(dt);
        let lag = if t.inertia { t.fission_lag_s.max(1e-9) } else { 1.0 };
        self.fission += (self.target_fission.min(fuel) - self.fission) * (dt / lag).min(1.0);
        self.turbine += (self.target_turbine - self.turbine) * k;

        if !self.power_on {
        } else if self.mode == Mode::Auto {
            self.auto(dt, &bands, load_kw);
        }

        if self.fission > 0.0 {
            let burn = self.fission / 100.0 * t.fuel_burn * dt;
            for r in self.rods.iter_mut().filter(|r| !r.spent()) {
                r.life = (r.life - burn).max(0.0);
            }
        }
        self.overheat(dt, &bands, &mut ev);
        self.output = self.output_for(load_kw, &bands);

        let use_ = if self.temp > 0.1 { (self.fission + self.turbine) / 200.0 } else { 0.0 };
        if let Some(e) = self.condition.deteriorate(dt, use_) {
            ev.push(ReactorEvent::Device(e));
        }
        self.update_alarms(&mut ev);
        ev
    }

    /// The boost fades to 0 at `boost_decay` per second, never past 0 (Reactor.cs:327,
    /// adjustValueWithoutOverShooting).
    fn fade_boost(&mut self, dt: f64) {
        let step = self.tuning.boost_decay * dt;
        self.boost = if self.boost > 0.0 { (self.boost - step).max(0.0) } else { (self.boost + step).min(0.0) };
    }

    /// The temperature moves as a damped spring towards `target`, its speed limited to `temp_rate`.
    /// At 0 or 100 the speed stops if it pushes outward.
    fn step_temp(&mut self, dt: f64, target: f64) {
        let t = self.tuning;
        if !t.inertia {
            // Barotrauma's path: at most `temp_rate` towards the target, no speed of its own.
            let diff = target - self.temp;
            self.temp = (self.temp + (diff.signum() * t.temp_rate * dt).clamp(-diff.abs(), diff.abs())).clamp(0.0, 100.0);
            self.temp_vel = 0.0;
            return;
        }
        self.temp_vel += ((target - self.temp) * t.temp_stiffness - self.temp_vel * t.temp_damping) * dt;
        self.temp_vel = self.temp_vel.clamp(-t.temp_rate, t.temp_rate);
        let next = self.temp + self.temp_vel * dt;
        if next < 0.0 {
            self.temp = 0.0;
            self.temp_vel = self.temp_vel.max(0.0);
        } else if next > 100.0 {
            self.temp = 100.0;
            self.temp_vel = self.temp_vel.min(0.0);
        } else {
            self.temp = next;
        }
    }

    pub fn output_range(&self) -> (f64, f64) {
        self.range(&self.bands())
    }

    fn range(&self, b: &Bands) -> (f64, f64) {
        if !self.power_on {
            return (0.0, 0.0);
        }
        let p = self.tuning.max_output_kw + self.output_boost_kw;
        let tol = if b.turbine_optimal.contains(self.turbine) && b.temp_optimal.contains(self.temp) { 3.0 } else { 1.0 };
        let tf = (self.temp / 50.0).min(1.0);
        let lo = p * ((self.turbine - tol) / 100.0).min(tf).clamp(0.0, 1.0);
        let hi = p * ((self.turbine + tol) / 100.0).min(tf).clamp(0.0, 1.0);
        (lo, hi)
    }

    fn output_for(&self, load_kw: f64, b: &Bands) -> f64 {
        let (lo, hi) = self.range(b);
        load_kw.clamp(lo, hi.max(lo))
    }

    /// The automatic controller: turbine to the middle of its optimal band (it follows the
    /// load); fission down while the temperature is above the middle of its optimal band, up
    /// while it is below or the output is short, held inside a small dead band.
    ///
    /// Changed from the original, which also pulls fission towards the middle of its fission band
    /// and so settles at the lowest temperature that covers the load, often below the green zone.
    /// The turbine follows the load faster than fission is cut, so a load drop overshoots above
    /// the optimal band for some seconds (`auto_overshoots_after_a_load_drop`); a manual operator
    /// who sets both at once does not.
    fn auto(&mut self, dt: f64, b: &Bands, load_kw: f64) {
        let st = self.tuning.auto_turbine_speed * self.auto_boost;
        let s = self.tuning.auto_fission_speed * self.auto_boost;
        self.target_turbine += (b.turbine_optimal.mid() - self.target_turbine).clamp(-st, st) * dt;
        self.target_turbine = self.target_turbine.clamp(0.0, 100.0);
        let mid = b.temp_optimal.mid();
        let pred = self.temp + self.temp_vel * self.tuning.auto_lookahead_s;
        let gain = ((pred - mid).abs() / AUTO_FULL_SPEED_AT).min(1.0);
        if pred > mid + AUTO_DEAD_BAND {
            self.target_fission = (self.target_fission - s * gain * dt).min(b.fission_allowed.hi);
        } else if pred < mid - AUTO_DEAD_BAND || self.output < load_kw - 1.0 {
            let rate = if pred < mid - AUTO_DEAD_BAND { s * gain } else { s };
            self.target_fission = (self.target_fission + rate * dt).min(100.0);
        }
        // Not far from the actual rate, or it overshoots while the temperature catches up.
        self.target_fission = self.target_fission.clamp(0.0, 100.0).clamp(self.fission - 5.0, self.fission + 5.0).max(0.0);
    }

    fn overheat(&mut self, dt: f64, b: &Bands, ev: &mut Vec<ReactorEvent>) {
        let t = self.tuning;
        let step = dt * (2.0 - self.condition.fraction());
        if self.temp > b.temp_allowed.hi {
            self.overheat_timer += step;
            if self.overheat_timer >= t.meltdown_delay_s {
                self.meltdown(ev);
                return;
            }
        } else {
            self.overheat_timer = (self.overheat_timer - dt).max(0.0);
        }
        if self.temp > b.temp_optimal.hi {
            self.fire_timer += step;
            if self.fire_timer >= t.fire_delay_s {
                self.fire_timer = 0.0;
                ev.push(ReactorEvent::Fire { size: t.fire_size });
            }
        } else {
            self.fire_timer = (self.fire_timer - dt).max(0.0);
        }
    }

    fn meltdown(&mut self, ev: &mut Vec<ReactorEvent>) {
        // No core, no melt (an ejected core is out of the reactor, whatever the temperature was).
        if self.ejected {
            return;
        }
        // The fuel at the moment of the melt (Barotrauma's Conditional: AvailableFuel > 1).
        let fuel = self.available_fuel();
        let before = self.condition.value();
        self.condition.set_value(0.0);
        self.power_on = false;
        self.melted = true;
        self.fire_timer = 0.0;
        self.overheat_timer = 0.0;
        // Nothing runs in the melted core: fission and turbine stop, the rods keep their life.
        self.fission = 0.0;
        self.turbine = 0.0;
        self.target_fission = 0.0;
        self.target_turbine = 0.0;
        self.decay = 0.0;
        ev.push(ReactorEvent::Meltdown);
        if fuel > self.tuning.blast.min_fuel {
            ev.push(ReactorEvent::Blast(self.tuning.blast));
        }
        if before >= self.condition.tuning().broken_below {
            ev.push(ReactorEvent::Device(DeviceEvent::Broke));
        }
    }

    fn update_alarms(&mut self, ev: &mut Vec<ReactorEvent>) {
        let b = self.bands();
        let h = self.tuning.alarm_hysteresis;
        let line = |on: bool, v: f64, at: f64| if on { v > at - h } else { v > at };
        let high = line(self.alarm(AlarmKind::TempHigh), self.temp, b.temp_optimal.hi);
        let crit = line(self.alarm(AlarmKind::TempCritical), self.temp, b.temp_allowed.hi);
        let life: f64 = self.rods.iter().map(|r| r.life).sum();
        let low_at = self.tuning.fuel_low;
        let fuel_low = if self.alarm(AlarmKind::FuelLow) { life < low_at * 1.25 } else { life < low_at };
        self.set_alarm(AlarmKind::TempHigh, high, ev);
        self.set_alarm(AlarmKind::TempCritical, crit, ev);
        self.set_alarm(AlarmKind::FuelLow, fuel_low, ev);
    }

    fn set_alarm(&mut self, kind: AlarmKind, on: bool, ev: &mut Vec<ReactorEvent>) {
        if self.alarms[kind.index()] != on {
            self.alarms[kind.index()] = on;
            ev.push(ReactorEvent::Alarm { kind, on });
        }
    }
}
