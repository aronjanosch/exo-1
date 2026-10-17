//! The reactor. Injection makes heat (as much as the fill allows), the converter turns heat into power
//! TODO(initiator).
//! Barotrauma's NeedMoreFill(1.0) and TooMuchFill for the screen. Values: TODO(initiator).
//! back to functioning, the reactor has a new, empty core. Values and rules: TODO(initiator).
//! Values: TODO(initiator).
//! Values: TODO(initiator).
//! Values: TODO(initiator), Barotrauma's.
use std::collections::BTreeMap;

use device_core::{Condition, ConditionSave, DeviceEvent, DeviceTuning};
use gameplay_core::notice::{Notice, NoticeKind};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// fill. The same values travel in `ReactorEvent::Blast`. TODO(initiator): Barotrauma's values.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Blast {
    /// The blast only happens with more available fill than this (Barotrauma: AvailableFuel > 1).
    pub min_fill: f64,
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
        Blast { min_fill: 1.0, range_m: 7.5, burn: 500.0, stun_s: 15.0, device_damage: 500.0, radiation_range_m: 20.0, radiation: 75.0 }
    }
}

/// Reactor values. TODO(initiator): starting values. Delays, burn rate, heat rate and
/// controller speed follow the original; the output, the implosion delay, the alarm
/// lines and the fire size are guesses.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReactorTuning {
    pub max_output_kw: f64,
    pub slots: usize,
    /// Canister life (0..100) used per second at injection 100.
    pub fill_burn: f64,
    /// Heat change per second at most.
    pub heat_rate: f64,
    /// Seconds above the optimal band per fire (at full condition).
    pub fire_delay_s: f64,
    pub implosion_delay_s: f64,
    /// Converter target change per second of the automatic controller (it follows the load).
    pub auto_converter_speed: f64,
    /// Injection target change per second of the automatic controller. Slower than the converter, so
    /// after a load drop the heat climbs before injection catches up.
    pub auto_injection_speed: f64,
    /// Size of a fire the reactor starts, metres (`fire_core`).
    pub fire_size: f64,
    /// Heat alarms switch off this far below their line.
    pub alarm_hysteresis: f64,
    /// Fill alarm below this much canister life in total (one canister is 100); off again above `* 1.25`.
    pub fill_low: f64,
    /// Heat the pump takes away at full output, as an offset of the heat target (degrees).
    /// TODO(initiator): value. Two canisters are needed to hold the optimal band with it at full load.
    pub cooling_max: f64,
    /// Residual (decay) heat in degrees of heat target: it is there only without coolant
    /// (`(1 - flow) * decay * decay_heat`, see `decay_lag_s`). The converter does not take it.
    /// TODO(initiator): value. Without coolant it keeps the core hot after power off and above the
    /// allowed band when injection is cut to nothing.
    pub decay_heat: f64,
    /// Seconds for the coolant flow to run down after the pump stops (and to come back): the
    /// water in the pipes coasts. TODO(initiator): value.
    pub coolant_coast_s: f64,
    /// Stiffness of the spring that pulls the heat to its target, 1/s². With
    /// `heat_damping` it sets the swing (about 25 s with the starting values) and the overshoot
    /// of a step (about a quarter). TODO(initiator): value.
    pub heat_stiffness: f64,
    /// Damping of the heat's speed, 1/s. TODO(initiator): value.
    pub heat_damping: f64,
    /// Heat the delivered output makes at full output, degrees of heat target. Taken from
    /// the output of the last step. TODO(initiator): value.
    pub heat_output_factor: f64,
    /// Seconds for injection to follow its target (a time constant). Barotrauma: 1. TODO(initiator): value.
    pub injection_lag_s: f64,
    /// Seconds for the decay heat to follow injection (a time constant): the core's decay state
    /// rises with injection and fades after power off over minutes. Ours, not Barotrauma's (it has no
    /// decay heat). TODO(initiator): value.
    pub decay_lag_s: f64,
    /// Auto looks this many seconds ahead: it compares the heat it predicts (heat + speed *
    /// this) with the band, so the spring's swing is not chased. TODO(initiator): value.
    pub auto_lookahead_s: f64,
    /// Size of a heat boost, degrees of heat: the operator's plus and minus add this much
    /// (Barotrauma `TemperatureBoostAmount`, 25). TODO(initiator): value.
    pub boost_amount: f64,
    /// How fast a boost fades back to 0, per second (Barotrauma: 1). TODO(initiator): value.
    pub boost_decay: f64,
    /// Seconds the EJECT cover stays open after ARM EJECT (the second press must come inside it).
    /// The host keeps the timer (the glue's). TODO(initiator): value.
    pub eject_arm_s: f64,
    /// What a SCRAM costs (#316, the physical emergency stop): the share (0..1) of every canister's
    /// remaining fill the vented chamber loses. An orderly shutdown costs nothing. TODO(initiator): value.
    #[serde(default = "default_scram_fill_loss")]
    pub scram_fill_loss: f64,
    /// The implosion blast (`Blast`). TODO(initiator): values.
    pub blast: Blast,
    /// Inertia on (the default): the heat is the damped spring, the output makes heat and
    /// injection lags by `injection_lag_s`. Off: the first-order path of Barotrauma (the heat moves
    /// at `heat_rate` towards its target, injection lags 1 s, no output heat), for the oracle tests.
    #[serde(default = "inertia_on")]
    pub inertia: bool,
    /// The reactor's own condition: repair threshold and wear floor (Barotrauma style, the floor
    /// is the lowest that normal wear reaches; the broken limit is the floor at least).
    pub condition: DeviceTuning,
}

fn default_scram_fill_loss() -> f64 {
    0.25
}

fn inertia_on() -> bool {
    true
}

impl Default for ReactorTuning {
    fn default() -> ReactorTuning {
        ReactorTuning {
            max_output_kw: 2000.0,
            slots: 4,
            fill_burn: 0.2,
            heat_rate: 10.0,
            fire_delay_s: 30.0,
            implosion_delay_s: 120.0,
            auto_converter_speed: 4.0,
            auto_injection_speed: 0.5,
            fire_size: 0.1,
            alarm_hysteresis: 3.0,
            fill_low: 20.0,
            cooling_max: 40.0,
            decay_heat: 150.0,
            coolant_coast_s: 10.0,
            heat_stiffness: 0.06,
            heat_damping: 0.2,
            heat_output_factor: 10.0,
            injection_lag_s: 4.0,
            decay_lag_s: 90.0,
            auto_lookahead_s: 8.0,
            boost_amount: 25.0,
            boost_decay: 1.0,
            eject_arm_s: 5.0,
            scram_fill_loss: default_scram_fill_loss(),
            blast: Blast::default(),
            inertia: true,
            condition: DeviceTuning { repair_threshold: 50.0, wear_floor: 10.0, ..DeviceTuning::default() },
        }
    }
}

/// Auto leaves injection alone while the heat is this close to the middle of its band.
const AUTO_DEAD_BAND: f64 = 1.0;
/// Auto's injection speed is full this many degrees (predicted) from the middle of its optimal band;
/// nearer, it is proportional. TODO(initiator): 5 did not settle with the spring (the swing stayed
/// about 10 degrees); 20 settles.
const AUTO_FULL_SPEED_AT: f64 = 20.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Canister {
    /// The kind's id in `engineering/engineering.json` (`canisters`): "standard", "hot" or "spicy".
    pub kind: String,
    /// How much injection it allows (the sum over all canisters caps injection and scales heat).
    pub heat: f64,
    pub life: f64,
}

impl Canister {
    /// The common canister, as `engineering/engineering.json` has it under "standard". TODO(initiator): values.
    pub fn standard() -> Canister {
        Canister { kind: "standard".into(), heat: 80.0, life: 100.0 }
    }

    pub fn spent(&self) -> bool {
        self.life <= 0.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanisterKind {
    pub heat: f64,
    pub life: f64,
}

/// The canister kinds by id. The default is the shipped table (standard 80, hot 100, spicy 150, all
/// at life 100). TODO(initiator): hot and spicy values.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct CanisterKinds(pub BTreeMap<String, CanisterKind>);

impl Default for CanisterKinds {
    fn default() -> CanisterKinds {
        let kind = |heat| CanisterKind { heat, life: 100.0 };
        CanisterKinds(BTreeMap::from([
            ("hot".to_string(), kind(100.0)),
            ("spicy".to_string(), kind(150.0)),
            ("standard".to_string(), kind(80.0)),
        ]))
    }
}

impl CanisterKinds {
    /// A new canister of kind `id` (life when new), or None for an unknown id.
    pub fn canister(&self, id: &str) -> Option<Canister> {
        self.0.get(id).map(|k| Canister { kind: id.to_string(), heat: k.heat, life: k.life })
    }
}

/// What the fill needs, for the screen (Barotrauma's advice to the operator): `More` when the canisters
/// cannot carry the load, `Less` when they make more heat than the converter can take away, `Ok`
/// otherwise.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FillNeed {
    Ok,
    More,
    Less,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    pub heat_optimal: Band,
    pub heat_allowed: Band,
    pub converter_optimal: Band,
    pub converter_allowed: Band,
    pub injection_optimal: Band,
    pub injection_allowed: Band,
}

impl Bands {
    /// Bands for an operator of `skill` (0..100), the converter set point that matches the load,
    /// and the available fill. Higher skill widens every band.
    pub fn new(skill: f64, converter_for_load: f64, fill: f64) -> Bands {
        let d = (skill / 100.0).clamp(0.0, 1.0);
        let lerp = |a: f64, b: f64| a + (b - a) * d;
        let half_opt = lerp(2.5, 10.0);
        let half_all = lerp(5.0, 20.0);
        let mut inj_opt = Band { lo: lerp(30.0, 20.0), hi: lerp(fill - 20.0, fill - 10.0) };
        inj_opt.lo = inj_opt.lo.min(inj_opt.hi - 10.0);
        let mut inj_all = Band { lo: lerp(20.0, 10.0), hi: fill };
        inj_all.lo = inj_all.lo.min(inj_all.hi - 10.0);
        Bands {
            heat_optimal: Band { lo: lerp(40.0, 30.0), hi: lerp(60.0, 70.0) },
            heat_allowed: Band { lo: lerp(30.0, 10.0), hi: lerp(70.0, 90.0) },
            converter_optimal: Band { lo: converter_for_load - half_opt, hi: converter_for_load + half_opt },
            converter_allowed: Band { lo: converter_for_load - half_all, hi: converter_for_load + half_all },
            injection_optimal: inj_opt,
            injection_allowed: inj_all,
        }
    }
}

/// Alarm kinds the reactor raises. Each switches on and off with hysteresis.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AlarmKind {
    HeatHigh,
    HeatCritical,
    FillLow,
}

impl AlarmKind {
    pub const ALL: [AlarmKind; 3] = [AlarmKind::HeatHigh, AlarmKind::HeatCritical, AlarmKind::FillLow];

    /// The notice for this alarm switching on. Text keys `reactor.<kind>`; the text pools are
    /// the glue's (TODO(initiator): wording).
    pub fn notice(self) -> Notice {
        let key = match self {
            AlarmKind::HeatHigh => "reactor.heat_high",
            AlarmKind::HeatCritical => "reactor.heat_critical",
            AlarmKind::FillLow => "reactor.fill_low",
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
    /// The core imploded. The reactor is broken and cold until a repair brings it back to
    /// functioning; what it does to the ship is the glue's.
    Implosion,
    /// The implosion blast: sent right after `Implosion` when the core had more than `blast.min_fill`
    /// available fill. The values are the reactor's `blast` tuning.
    Blast(Blast),
    Alarm { kind: AlarmKind, on: bool },
    /// The core was thrown out (`Reactor::eject`). No implode and no blast follow it.
    CoreEjected,
    Device(DeviceEvent),
}

/// The reactor's state as a save holds it: the plain values, not the tuning (the loader gives it back).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReactorSave {
    pub mode: Mode,
    pub power_on: bool,
    pub imploded: bool,
    pub ejected: bool,
    pub target_injection: f64,
    pub target_converter: f64,
    pub injection: f64,
    pub converter: f64,
    pub heat: f64,
    pub heat_vel: f64,
    pub converter_for_load: f64,
    pub skill: f64,
    pub canisters: Vec<Canister>,
    pub fill: f64,
    pub load: f64,
    pub fire_timer: f64,
    pub overheat_timer: f64,
    pub output: f64,
    pub alarms: [bool; 3],
    pub condition: ConditionSave,
    pub coolant_target: f64,
    pub coolant: f64,
    pub heat_in: f64,
    pub decay: f64,
    pub boost: f64,
    pub output_boost_kw: f64,
    pub auto_boost: f64,
    pub extra_cool: f64,
}

#[derive(Clone, Debug)]
pub struct Reactor {
    tuning: ReactorTuning,
    mode: Mode,
    power_on: bool,
    imploded: bool,
    target_injection: f64,
    target_converter: f64,
    injection: f64,
    converter: f64,
    heat: f64,
    converter_for_load: f64,
    skill: f64,
    canisters: Vec<Canister>,
    /// Available fill: the sum of the live canisters' heat, kept in step with the canisters (see `sync_fill`).
    fill: f64,
    /// The load the grid asked for in the last step, kW (for `fill_need`).
    load: f64,
    fire_timer: f64,
    overheat_timer: f64,
    output: f64,
    alarms: [bool; 3],
    condition: Condition,
    /// Normal wear on (`EngineRoom::set_wear`); damage from heat and blasts takes condition either way.
    wears: bool,
    /// Output factor of the coolant pump, 0..1 (set by the engine room each step).
    coolant_target: f64,
    /// The coolant flow that takes heat away now, 0..1: follows `coolant_target` with a coast.
    coolant: f64,
    heat_in: f64,
    /// The core was ejected: no core, no canisters, no implode. Cleared by a repair (a new core).
    ejected: bool,
    /// Speed of the heat, degrees per second.
    heat_vel: f64,
    /// Decay state, 0..1: follows injection / 100 with `decay_lag_s`; it drives the residual heat.
    /// Kept through power off, reset by an implosion.
    decay: f64,
    /// Heat boost (degrees of heat, -amount..amount), fades to 0 at `boost_decay`.
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

impl Reactor {
    pub fn to_save(&self) -> ReactorSave {
        ReactorSave {
            mode: self.mode,
            power_on: self.power_on,
            imploded: self.imploded,
            ejected: self.ejected,
            target_injection: self.target_injection,
            target_converter: self.target_converter,
            injection: self.injection,
            converter: self.converter,
            heat: self.heat,
            heat_vel: self.heat_vel,
            converter_for_load: self.converter_for_load,
            skill: self.skill,
            canisters: self.canisters.clone(),
            fill: self.fill,
            load: self.load,
            fire_timer: self.fire_timer,
            overheat_timer: self.overheat_timer,
            output: self.output,
            alarms: self.alarms,
            condition: self.condition.to_save(),
            coolant_target: self.coolant_target,
            coolant: self.coolant,
            heat_in: self.heat_in,
            decay: self.decay,
            boost: self.boost,
            output_boost_kw: self.output_boost_kw,
            auto_boost: self.auto_boost,
            extra_cool: self.extra_cool,
        }
    }

    pub fn from_save(tuning: ReactorTuning, s: &ReactorSave) -> Reactor {
        Reactor {
            tuning,
            mode: s.mode,
            power_on: s.power_on,
            imploded: s.imploded,
            ejected: s.ejected,
            target_injection: s.target_injection,
            target_converter: s.target_converter,
            injection: s.injection,
            converter: s.converter,
            heat: s.heat,
            heat_vel: s.heat_vel,
            converter_for_load: s.converter_for_load,
            skill: s.skill,
            canisters: s.canisters.clone(),
            fill: s.fill,
            load: s.load,
            fire_timer: s.fire_timer,
            overheat_timer: s.overheat_timer,
            output: s.output,
            alarms: s.alarms,
            condition: Condition::from_save(tuning.condition, &s.condition),
            wears: true,
            coolant_target: s.coolant_target,
            coolant: s.coolant,
            heat_in: s.heat_in,
            decay: s.decay,
            boost: s.boost,
            output_boost_kw: s.output_boost_kw,
            auto_boost: s.auto_boost,
            extra_cool: s.extra_cool,
        }
    }
}

/// The lamps of the panel (Barotrauma's warning lights, client Reactor.cs 621-632). Each is on
/// or off, steady (the blink is the glue's); `short` gives the letters for a trace.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Warnings {
    pub overheat: bool,
    /// Heat above the allowed band (client 621, critical heat). Power on only.
    pub critical_heat: bool,
    pub low_heat: bool,
    /// Heat below the allowed band (client 622, cold). Power on only.
    pub cold: bool,
    pub high_output: bool,
    pub critical_output: bool,
    pub low_output: bool,
    pub fill_out: bool,
    pub low_fill: bool,
    pub implosion: bool,
    /// Warm (above 0.1) with the power off (client 632).
    pub scram: bool,
}

impl Warnings {
    /// Short letters of the lamps that are on, in panel order, e.g. "OH HO LF"; "" when none.
    pub fn short(&self) -> String {
        let lamps = [
            (self.overheat, "OH"),
            (self.critical_heat, "CH"),
            (self.low_heat, "LT"),
            (self.cold, "CL"),
            (self.high_output, "HO"),
            (self.critical_output, "CO"),
            (self.low_output, "LO"),
            (self.fill_out, "FO"),
            (self.low_fill, "LF"),
            (self.implosion, "MD"),
            (self.scram, "SC"),
        ];
        lamps.iter().filter(|(on, _)| *on).map(|(_, s)| *s).collect::<Vec<_>>().join(" ")
    }
}

impl Reactor {
    pub fn set_wear(&mut self, on: bool) {
        self.wears = on;
    }

    pub fn new(tuning: ReactorTuning, seed: u64) -> Reactor {
        Reactor {
            tuning,
            mode: Mode::Auto,
            power_on: false,
            imploded: false,
            target_injection: 0.0,
            target_converter: 0.0,
            injection: 0.0,
            converter: 0.0,
            heat: 0.0,
            converter_for_load: 0.0,
            skill: 0.0,
            canisters: Vec::new(),
            fill: 0.0,
            load: 0.0,
            fire_timer: 0.0,
            overheat_timer: 0.0,
            output: 0.0,
            alarms: [false; 3],
            condition: Condition::new(tuning.condition, seed),
            wears: true,
            coolant_target: 1.0,
            coolant: 1.0,
            heat_in: 0.0,
            ejected: false,
            heat_vel: 0.0,
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

    /// Implodes the core now, as if its implosion timer had run out (the harness's forced implosion, for a
    /// implode with no fill or a script that does not wait for the timer). Nothing when the reactor is
    /// broken already. The events are those of an implosion in `step`.
    pub fn force_implosion(&mut self) -> Vec<ReactorEvent> {
        let mut ev = Vec::new();
        if self.condition.functioning() && !self.ejected {
            self.implosion(&mut ev);
        }
        ev
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// Sets the targets (0..100) by hand; this switches to Manual.
    pub fn set_targets(&mut self, injection: f64, converter: f64) {
        self.mode = Mode::Manual;
        self.target_injection = injection.clamp(0.0, 100.0);
        self.target_converter = converter.clamp(0.0, 100.0);
    }

    /// Output factor of the coolant pump, 0..1: scales the cooling offset.
    pub fn set_coolant(&mut self, factor: f64) {
        self.coolant_target = factor.clamp(0.0, 1.0);
    }

    /// The coolant flow that takes heat away now, 0..1 (lags the pump's factor).
    pub fn coolant_flow(&self) -> f64 {
        self.coolant
    }

    /// Extra cooling, degrees of heat target, on top of the pump's cooling (0 with none). The
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

    /// Multiplies Auto's converter and injection speeds by `factor` (1 is the tuning's own). For the
    /// engine room's warm start only.
    /// Sets the kW added to the maximum output (`max_output_kw + boost` is the range's max); the
    /// engine room sets it from the aux reactor's spare each step. Negative values count as 0.
    pub fn set_output_boost_kw(&mut self, kw: f64) {
        self.output_boost_kw = kw.max(0.0);
    }

    pub fn set_auto_boost(&mut self, factor: f64) {
        self.auto_boost = factor.max(0.0);
    }

    /// The life of every canister, in slot order (for a warm start to put back).
    pub fn canister_lives(&self) -> Vec<f64> {
        self.canisters.iter().map(|r| r.life).collect()
    }

    /// Puts the canisters' life back, slot by slot (`lives` as `canister_lives` gave it, same canisters). Caps
    /// injection to the fill left, as `sync_fill` does.
    pub fn set_canister_lives(&mut self, lives: &[f64]) {
        for (canister, life) in self.canisters.iter_mut().zip(lives) {
            canister.life = life.clamp(0.0, 100.0);
        }
        self.sync_fill();
    }

    /// Puts a canister into a free slot; gives it back when all slots are full.
    pub fn insert_canister(&mut self, canister: Canister) -> Result<(), Canister> {
        if self.ejected || self.canisters.len() >= self.tuning.slots {
            return Err(canister);
        }
        self.canisters.push(canister);
        self.sync_fill();
        Ok(())
    }

    pub fn remove_canister(&mut self, slot: usize) -> Option<Canister> {
        let canister = (slot < self.canisters.len()).then(|| self.canisters.remove(slot))?;
        self.sync_fill();
        Some(canister)
    }

    /// The emergency brake (Barotrauma's: pulling a canister). Takes the canister in
    /// `slot` out and returns it with the handling heat: the core's heat at that moment
    /// (what the glue burns the hand with; TODO(initiator): the burn rule). The fill goes at once:
    /// see `sync_fill`. None for an empty slot.
    pub fn pull_canister(&mut self, slot: usize) -> Option<(Canister, f64)> {
        let canister = self.remove_canister(slot)?;
        Some((canister, self.heat))
    }

    /// The emergency stop (#316): the chamber is vented. Power goes off, injection and both targets fall to 0 at once
    /// (no lag), and every canister loses `scram_fill_loss` of its remaining fill (never below 0). The heat is left to
    /// the coolant. Returns the fill lost, in life points summed over the canisters (0 with none). Refused (None) on
    /// an ejected core. An orderly shutdown is `set_power(false)` and costs nothing.
    pub fn scram(&mut self) -> Option<f64> {
        if self.ejected {
            return None;
        }
        let share = if self.tuning.scram_fill_loss.is_finite() { self.tuning.scram_fill_loss.clamp(0.0, 1.0) } else { 0.0 };
        let mut lost = 0.0;
        for c in &mut self.canisters {
            let before = c.life.max(0.0);
            c.life = before * (1.0 - share);
            lost += before - c.life;
        }
        self.power_on = false;
        self.injection = 0.0;
        self.target_injection = 0.0;
        self.target_converter = 0.0;
        self.sync_fill();
        Some(lost)
    }

    /// Throws the core out (PULSAR's core eject). Everything the core did stops at once: injection,
    /// converter, heat, output, decay, boost, both timers and the targets go to 0, the power
    /// goes off, and every canister leaves with the core (not given back). The reactor is broken
    /// (condition 0) and `ejected` until a repair installs a new, empty core. Ejecting an ejected
    /// reactor does nothing; the events are `CoreEjected` alone, or none.
    pub fn eject(&mut self) -> Vec<ReactorEvent> {
        if self.ejected {
            return Vec::new();
        }
        self.ejected = true;
        self.imploded = false;
        self.power_on = false;
        self.canisters.clear();
        self.fill = 0.0;
        self.injection = 0.0;
        self.converter = 0.0;
        self.target_injection = 0.0;
        self.target_converter = 0.0;
        self.heat = 0.0;
        self.heat_vel = 0.0;
        self.output = 0.0;
        self.heat_in = 0.0;
        self.decay = 0.0;
        self.boost = 0.0;
        self.fire_timer = 0.0;
        self.overheat_timer = 0.0;
        self.condition.set_value(0.0);
        vec![ReactorEvent::CoreEjected]
    }

    /// Brings the available fill up to the canisters and caps injection to it at once. Barotrauma caps
    /// the injection target and lets injection follow it, and its heat uses last step's fill; a pulled
    /// canister must not leave a step of full injection on the canisters that are left.
    fn sync_fill(&mut self) {
        self.fill = self.available_fill();
        self.injection = self.injection.min(self.fill);
    }

    /// The heat boost (Barotrauma's plus and minus buttons): sets the boost to +`boost_amount`
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

    pub fn heat_boost(&self) -> f64 {
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
    /// The core has imploded: the screen shows IMPLOSION until the reactor is repaired back to
    /// functioning, and then it is a reactor again (the canisters keep their life).
    pub fn imploded(&self) -> bool {
        self.imploded
    }
    pub fn heat(&self) -> f64 {
        self.heat
    }
    /// The decay state, 0..1: the residual heat's share (follows injection / 100 with `decay_lag_s`).
    pub fn decay(&self) -> f64 {
        self.decay
    }
    pub fn heat_velocity(&self) -> f64 {
        self.heat_vel
    }
    pub fn injection(&self) -> f64 {
        self.injection
    }
    pub fn converter(&self) -> f64 {
        self.converter
    }
    pub fn targets(&self) -> (f64, f64) {
        (self.target_injection, self.target_converter)
    }
    pub fn output_kw(&self) -> f64 {
        self.output
    }
    pub fn canisters(&self) -> &[Canister] {
        &self.canisters
    }
    pub fn available_fill(&self) -> f64 {
        self.canisters.iter().filter(|r| !r.spent()).map(|r| r.heat).sum()
    }
    /// Heat per injection at the current fill: `injection * fill / 100 * 2` (Reactor.cs:500-503,
    /// without its heat boost).
    fn heat_at(&self, injection: f64) -> f64 {
        injection * (self.fill / 100.0) * 2.0
    }
    /// What the fill needs for the load now (`FillNeed`). The first check wins: More, then Less.
    pub fn fill_need(&self) -> FillNeed {
        if self.need_more() {
            FillNeed::More
        } else if self.too_much() {
            FillNeed::Less
        } else {
            FillNeed::Ok
        }
    }
    /// Barotrauma's NeedMoreFill(minimumOutputRatio 1.0, minCondition 0), Reactor.cs:510-531: no
    /// fill left while the grid asks, or the most the fill can make (injection capped to it, converter
    /// at 100, the heat factor of the heat) is below the load.
    fn need_more(&self) -> bool {
        let remaining: f64 = self.canisters.iter().map(|r| r.life).sum();
        if remaining <= 0.0 && self.load > 0.0 {
            return true;
        }
        let max_injection = self.fill.min(100.0);
        if max_injection >= 100.0 {
            return false;
        }
        let heat_factor = (self.heat_at(max_injection) / 50.0).min(1.0);
        let max_output = heat_factor * self.tuning.max_output_kw;
        max_output < self.load
    }
    /// Barotrauma's TooMuchFill, Reactor.cs:533-543: with more than one canister, the heat at the low
    /// end of the optimal injection band is above what the converter can take (1.5 times the converter
    /// for the load, at most 90).
    fn too_much(&self) -> bool {
        if self.canisters.len() <= 1 {
            return false;
        }
        let b = self.bands();
        self.heat_at(b.injection_optimal.lo) > (self.converter_for_load * 1.5).min(90.0)
    }
    /// Core stability, 0..1: 1 minus the implosion timer's fraction (`timers().1`). 0 when the core
    /// has imploded or was ejected; 1 when it is off and cold. No dynamics of its own.
    pub fn stability(&self) -> f64 {
        if self.imploded || self.ejected {
            return 0.0;
        }
        (1.0 - self.timers().1).clamp(0.0, 1.0)
    }
    pub fn timers(&self) -> (f64, f64) {
        (self.fire_timer / self.tuning.fire_delay_s, self.overheat_timer / self.tuning.implosion_delay_s)
    }
    pub fn bands(&self) -> Bands {
        Bands::new(self.skill, self.converter_for_load, self.fill)
    }
    /// The panel's lamps, for a grid asking `load_kw` now (the last step's output and fill). The
    /// conditions are Barotrauma's (client Reactor.cs 621-632). Barotrauma blinks them at 2 Hz
    /// (client 619, `TotalTime % 0.5 < 0.25`), the glue's job here. With the power off every lamp is
    /// off except `implosion`, whose timer part does not need the power (client 631, and on when the
    /// core imploded), and `scram`, which is on when it is warm (client 632).
    pub fn warnings(&self, load_kw: f64) -> Warnings {
        let b = self.bands();
        let on = self.power_on;
        let out = self.output;
        let t = self.tuning;
        Warnings {
            // Client 625: heat above the optimal band.
            overheat: on && self.heat > b.heat_optimal.hi,
            // Client 621: heat above the allowed band.
            critical_heat: on && self.heat > b.heat_allowed.hi,
            // Client 627: heat below the optimal band.
            low_heat: on && self.heat < b.heat_optimal.lo,
            // Client 622: heat below the allowed band.
            cold: on && self.heat < b.heat_allowed.lo,
            // Client 626: output above 110 % of the load.
            high_output: on && out > load_kw * 1.1,
            // Client 623: output above 150 % of the load.
            critical_output: on && out > load_kw * 1.5,
            // Client 628: output below 90 % of the load.
            low_output: on && out < load_kw * 0.9,
            // Client 629: available fill below 1 % of the injection.
            fill_out: on && self.fill < self.injection * 0.01,
            // Client 630: available fill below the injection.
            low_fill: on && self.fill < self.injection,
            // Client 631: the implosion timer over half its delay, or the condition at 0 with the
            // power on (Barotrauma), or the core imploded (ours: the lamp stays on for the implode).
            implosion: self.overheat_timer > t.implosion_delay_s * 0.5 || (self.condition.value() <= 0.0 && on) || self.imploded,
            // Client 632: heat above 0.1 with the power off (not blinked there).
            scram: !on && self.heat > 0.1,
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
        if self.imploded && self.condition.functioning() {
            self.imploded = false;
        }
        // Repaired back to functioning after an eject: a new, empty core (the canisters are not back).
        if self.ejected && self.condition.functioning() {
            self.ejected = false;
        }

        self.heat_in = 0.0;
        if !self.condition.functioning() {
            // Broken: off the grid, cools fast, targets run down. Injection and converter stay where
            // they were (the original freezes them too).
            self.power_on = false;
            self.output = 0.0;
            self.heat = (self.heat - 100.0 * dt).max(0.0);
            self.heat_vel = 0.0;
            self.target_injection = (self.target_injection - 10.0 * dt).max(0.0);
            self.target_converter = (self.target_converter - 10.0 * dt).max(0.0);
            self.fire_timer = 0.0;
            self.overheat_timer = 0.0;
            self.fade_boost(dt);
            self.update_alarms(&mut ev);
            return ev;
        }

        self.sync_fill();
        let fill = self.fill;
        self.load = load_kw;
        if t.max_output_kw > 0.0 {
            let want = load_kw / t.max_output_kw * 100.0;
            self.converter_for_load += (want - self.converter_for_load).clamp(-20.0, 20.0) * dt;
        }
        let bands = Bands::new(self.skill, self.converter_for_load, fill);

        // Off: the targets are zero before the canisters and the converter follow them (a manual setting
        // the glue sends every step does not bring injection back).
        if !self.power_on {
            self.target_injection = 0.0;
            self.target_converter = 0.0;
        }
        // The boost adds to the heat (Barotrauma GetGeneratedHeat, Reactor.cs:502).
        let heat = self.heat_at(self.injection) + self.boost;
        self.heat_in = heat;
        let cool = t.cooling_max * self.coolant + self.extra_cool;
        // Decay heat follows injection with a slow lag (`decay`): power off stops injection at once (the targets are 0) and the
        // decay heat fades over minutes. Inertia off: the old path, the heat with injection at once.
        let decay_level = if t.inertia {
            let lag = t.decay_lag_s.max(1e-9);
            self.decay += (self.injection / 100.0 - self.decay) * (dt / lag).min(1.0);
            self.decay
        } else {
            self.injection / 100.0
        };
        let residual = t.decay_heat * (1.0 - self.coolant) * decay_level;
        let output_heat = if t.inertia && t.max_output_kw > 0.0 { t.heat_output_factor * self.output / t.max_output_kw } else { 0.0 };
        self.step_heat(dt, heat + residual + output_heat - self.converter - cool);
        self.fade_boost(dt);
        let lag = if t.inertia { t.injection_lag_s.max(1e-9) } else { 1.0 };
        self.injection += (self.target_injection.min(fill) - self.injection) * (dt / lag).min(1.0);
        self.converter += (self.target_converter - self.converter) * k;

        if !self.power_on {
        } else if self.mode == Mode::Auto {
            self.auto(dt, &bands, load_kw);
        }

        if self.injection > 0.0 {
            let burn = self.injection / 100.0 * t.fill_burn * dt;
            for r in self.canisters.iter_mut().filter(|r| !r.spent()) {
                r.life = (r.life - burn).max(0.0);
            }
        }
        self.overheat(dt, &bands, &mut ev);
        self.output = self.output_for(load_kw, &bands);

        let use_ = if self.wears && self.heat > 0.1 { (self.injection + self.converter) / 200.0 } else { 0.0 };
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

    /// The heat moves as a damped spring towards `target`, its speed limited to `heat_rate`.
    /// At 0 or 100 the speed stops if it pushes outward.
    fn step_heat(&mut self, dt: f64, target: f64) {
        let t = self.tuning;
        if !t.inertia {
            // Barotrauma's path: at most `heat_rate` towards the target, no speed of its own.
            let diff = target - self.heat;
            self.heat = (self.heat + (diff.signum() * t.heat_rate * dt).clamp(-diff.abs(), diff.abs())).clamp(0.0, 100.0);
            self.heat_vel = 0.0;
            return;
        }
        self.heat_vel += ((target - self.heat) * t.heat_stiffness - self.heat_vel * t.heat_damping) * dt;
        self.heat_vel = self.heat_vel.clamp(-t.heat_rate, t.heat_rate);
        let next = self.heat + self.heat_vel * dt;
        if next < 0.0 {
            self.heat = 0.0;
            self.heat_vel = self.heat_vel.max(0.0);
        } else if next > 100.0 {
            self.heat = 100.0;
            self.heat_vel = self.heat_vel.min(0.0);
        } else {
            self.heat = next;
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
        let tol = if b.converter_optimal.contains(self.converter) && b.heat_optimal.contains(self.heat) { 3.0 } else { 1.0 };
        let tf = (self.heat / 50.0).min(1.0);
        let lo = p * ((self.converter - tol) / 100.0).min(tf).clamp(0.0, 1.0);
        let hi = p * ((self.converter + tol) / 100.0).min(tf).clamp(0.0, 1.0);
        (lo, hi)
    }

    fn output_for(&self, load_kw: f64, b: &Bands) -> f64 {
        let (lo, hi) = self.range(b);
        load_kw.clamp(lo, hi.max(lo))
    }

    /// The automatic controller: converter to the middle of its optimal band (it follows the
    /// load); injection down while the heat is above the middle of its optimal band, up
    /// while it is below or the output is short, held inside a small dead band.
    ///
    /// Changed from the original, which also pulls injection towards the middle of its injection band
    /// and so settles at the lowest heat that covers the load, often below the green zone.
    /// The converter follows the load faster than injection is cut, so a load drop overshoots above
    /// the optimal band for some seconds (`auto_overshoots_after_a_load_drop`); a manual operator
    /// who sets both at once does not.
    fn auto(&mut self, dt: f64, b: &Bands, load_kw: f64) {
        let st = self.tuning.auto_converter_speed * self.auto_boost;
        let s = self.tuning.auto_injection_speed * self.auto_boost;
        self.target_converter += (b.converter_optimal.mid() - self.target_converter).clamp(-st, st) * dt;
        self.target_converter = self.target_converter.clamp(0.0, 100.0);
        let mid = b.heat_optimal.mid();
        let pred = self.heat + self.heat_vel * self.tuning.auto_lookahead_s;
        let gain = ((pred - mid).abs() / AUTO_FULL_SPEED_AT).min(1.0);
        if pred > mid + AUTO_DEAD_BAND {
            self.target_injection = (self.target_injection - s * gain * dt).min(b.injection_allowed.hi);
        } else if pred < mid - AUTO_DEAD_BAND || self.output < load_kw - 1.0 {
            let rate = if pred < mid - AUTO_DEAD_BAND { s * gain } else { s };
            self.target_injection = (self.target_injection + rate * dt).min(100.0);
        }
        // Not far from the actual rate, or it overshoots while the heat catches up.
        self.target_injection = self.target_injection.clamp(0.0, 100.0).clamp(self.injection - 5.0, self.injection + 5.0).max(0.0);
    }

    fn overheat(&mut self, dt: f64, b: &Bands, ev: &mut Vec<ReactorEvent>) {
        let t = self.tuning;
        let step = dt * (2.0 - self.condition.fraction());
        if self.heat > b.heat_allowed.hi {
            self.overheat_timer += step;
            if self.overheat_timer >= t.implosion_delay_s {
                self.implosion(ev);
                return;
            }
        } else {
            self.overheat_timer = (self.overheat_timer - dt).max(0.0);
        }
        if self.heat > b.heat_optimal.hi {
            self.fire_timer += step;
            if self.fire_timer >= t.fire_delay_s {
                self.fire_timer = 0.0;
                ev.push(ReactorEvent::Fire { size: t.fire_size });
            }
        } else {
            self.fire_timer = (self.fire_timer - dt).max(0.0);
        }
    }

    fn implosion(&mut self, ev: &mut Vec<ReactorEvent>) {
        // No core, no implode (an ejected core is out of the reactor, whatever the heat was).
        if self.ejected {
            return;
        }
        // The fill at the moment of the implode (Barotrauma's Conditional: AvailableFuel > 1).
        let fill = self.available_fill();
        let before = self.condition.value();
        self.condition.set_value(0.0);
        self.power_on = false;
        self.imploded = true;
        self.fire_timer = 0.0;
        self.overheat_timer = 0.0;
        // Nothing runs in the imploded core: injection and converter stop, the canisters keep their life.
        self.injection = 0.0;
        self.converter = 0.0;
        self.target_injection = 0.0;
        self.target_converter = 0.0;
        self.decay = 0.0;
        ev.push(ReactorEvent::Implosion);
        if fill > self.tuning.blast.min_fill {
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
        let high = line(self.alarm(AlarmKind::HeatHigh), self.heat, b.heat_optimal.hi);
        let crit = line(self.alarm(AlarmKind::HeatCritical), self.heat, b.heat_allowed.hi);
        let life: f64 = self.canisters.iter().map(|r| r.life).sum();
        let low_at = self.tuning.fill_low;
        let fill_low = if self.alarm(AlarmKind::FillLow) { life < low_at * 1.25 } else { life < low_at };
        self.set_alarm(AlarmKind::HeatHigh, high, ev);
        self.set_alarm(AlarmKind::HeatCritical, crit, ev);
        self.set_alarm(AlarmKind::FillLow, fill_low, ev);
    }

    fn set_alarm(&mut self, kind: AlarmKind, on: bool, ev: &mut Vec<ReactorEvent>) {
        if self.alarms[kind.index()] != on {
            self.alarms[kind.index()] = on;
            ev.push(ReactorEvent::Alarm { kind, on });
        }
    }
}
