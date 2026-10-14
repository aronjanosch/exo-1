//! The reactor. Fission makes heat (as much as the fuel allows), the turbine turns heat into power
//! and takes it away. Temperature follows `heat - turbine` at a limited rate. The output lies
//! between a minimum and a maximum set by turbine and temperature; inside that range it follows
//! the load.
//!
//! Overheat: above the optimal band a fire timer runs, and each time it is full the reactor starts
//! a fire (`ReactorEvent::Fire`). Above the allowed band a second timer runs; when it is full the
//! safety shuts the reactor down for a while. With the safety off it runs on to a meltdown. Low
//! condition makes both timers run up to twice as fast.
use device_core::{Condition, DeviceEvent, DeviceTuning};
use gameplay_core::notice::{Notice, NoticeKind};

#[cfg(test)]
mod tests;

/// Reactor values. TODO(initiator): starting values. Delays, burn rate, temperature rate and
/// controller speed follow the original; the output, the shutdown delay, the lockout, the alarm
/// lines and the fire size are guesses.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ReactorTuning {
    /// Output at full turbine and temperature, kW.
    pub max_output_kw: f64,
    /// Fuel rod slots.
    pub slots: usize,
    /// Rod life (0..100) used per second at fission 100.
    pub fuel_burn: f64,
    /// Temperature change per second at most.
    pub temp_rate: f64,
    /// Seconds above the optimal band per fire (at full condition).
    pub fire_delay_s: f64,
    /// Seconds above the allowed band until the safety shuts the reactor down.
    pub shutdown_delay_s: f64,
    /// Seconds above the allowed band until a meltdown, with the safety off.
    pub meltdown_delay_s: f64,
    /// Seconds after a safety shutdown before the reactor can be started again.
    pub restart_lockout_s: f64,
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
}

impl Default for ReactorTuning {
    fn default() -> ReactorTuning {
        ReactorTuning {
            max_output_kw: 1000.0,
            slots: 4,
            fuel_burn: 0.2,
            temp_rate: 10.0,
            fire_delay_s: 20.0,
            shutdown_delay_s: 30.0,
            meltdown_delay_s: 120.0,
            restart_lockout_s: 15.0,
            auto_turbine_speed: 4.0,
            auto_fission_speed: 0.5,
            fire_size: 0.1,
            alarm_hysteresis: 3.0,
            fuel_low: 20.0,
            cooling_max: 40.0,
        }
    }
}

/// Auto leaves fission alone while the temperature is this close to the middle of its band.
const AUTO_DEAD_BAND: f64 = 1.0;

/// A fuel rod: an item that goes into a slot and burns down.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FuelRod {
    /// How much fission it allows (the sum over all rods caps fission and scales heat).
    pub heat: f64,
    /// 100 when new, burnt out at 0.
    pub life: f64,
}

impl FuelRod {
    /// The common rod. TODO(initiator): stronger rods later (the original has 100 and 150).
    pub fn standard() -> FuelRod {
        FuelRod { heat: 80.0, life: 100.0 }
    }

    pub fn spent(&self) -> bool {
        self.life <= 0.0
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The controller sets the targets.
    Auto,
    /// The operator sets the targets.
    Manual,
}

/// A range `lo..hi` on a gauge.
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

/// The bands the panel shows. Optimal is the green zone, allowed the yellow one.
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
    /// Above the optimal band: the fire timer runs.
    TempHigh,
    /// Above the allowed band: the shutdown (or meltdown) timer runs.
    TempCritical,
    FuelLow,
    /// The safety shut the reactor down; off once it runs again.
    Shutdown,
}

impl AlarmKind {
    pub const ALL: [AlarmKind; 4] = [AlarmKind::TempHigh, AlarmKind::TempCritical, AlarmKind::FuelLow, AlarmKind::Shutdown];

    /// The notice for this alarm switching on. Text keys `reactor.<kind>`; the text pools are
    /// the glue's (TODO(initiator): wording).
    pub fn notice(self) -> Notice {
        let key = match self {
            AlarmKind::TempHigh => "reactor.temp_high",
            AlarmKind::TempCritical => "reactor.temp_critical",
            AlarmKind::FuelLow => "reactor.fuel_low",
            AlarmKind::Shutdown => "reactor.shutdown",
        };
        Notice::new(NoticeKind::Warning, key)
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// What happened in one step. The glue routes them.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ReactorEvent {
    /// Start a fire of `size` metres at the reactor (`fire_core`).
    Fire { size: f64 },
    /// The safety shut the reactor down after overheating.
    Shutdown,
    /// The core melted (safety off). The reactor is broken and its rods are spent; what it does
    /// to the ship is the glue's.
    Meltdown,
    Alarm { kind: AlarmKind, on: bool },
    /// The reactor's own condition crossed a line.
    Device(DeviceEvent),
}

/// The reactor.
#[derive(Clone, Debug)]
pub struct Reactor {
    tuning: ReactorTuning,
    mode: Mode,
    power_on: bool,
    safety: bool,
    target_fission: f64,
    target_turbine: f64,
    fission: f64,
    turbine: f64,
    temp: f64,
    /// Turbine set point that matches the load, smoothed.
    turbine_for_load: f64,
    skill: f64,
    rods: Vec<FuelRod>,
    /// Available fuel of the last step.
    fuel: f64,
    fire_timer: f64,
    overheat_timer: f64,
    lockout: f64,
    output: f64,
    alarms: [bool; 4],
    condition: Condition,
    /// Output factor of the coolant pump, 0..1 (set by the engine room each step).
    coolant: f64,
}

impl Reactor {
    /// A cold reactor, off, empty, in Auto, safety on, at full condition.
    pub fn new(tuning: ReactorTuning, device: DeviceTuning, seed: u64) -> Reactor {
        Reactor {
            tuning,
            mode: Mode::Auto,
            power_on: false,
            safety: true,
            target_fission: 0.0,
            target_turbine: 0.0,
            fission: 0.0,
            turbine: 0.0,
            temp: 0.0,
            turbine_for_load: 0.0,
            skill: 0.0,
            rods: Vec::new(),
            fuel: 0.0,
            fire_timer: 0.0,
            overheat_timer: 0.0,
            lockout: 0.0,
            output: 0.0,
            alarms: [false; 4],
            condition: Condition::new(device, seed),
            coolant: 1.0,
        }
    }

    // ---------- controls ----------

    /// Switches the reactor on or off. Refused (false) during the lockout after a safety shutdown
    /// or while the reactor is broken.
    pub fn set_power(&mut self, on: bool) -> bool {
        if on && (self.lockout > 0.0 || !self.condition.functioning()) {
            return false;
        }
        self.power_on = on;
        true
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
        self.coolant = factor.clamp(0.0, 1.0);
    }

    /// The safety shuts the reactor down when it overheats. Off, it runs on to a meltdown.
    pub fn set_safety(&mut self, on: bool) {
        self.safety = on;
    }

    /// Skill (0..100) of whoever runs the reactor; it widens the bands. 0 when nobody does.
    pub fn set_operator_skill(&mut self, skill: f64) {
        self.skill = skill.clamp(0.0, 100.0);
    }

    /// Puts a rod into a free slot; gives it back when all slots are full.
    pub fn insert_rod(&mut self, rod: FuelRod) -> Result<(), FuelRod> {
        if self.rods.len() >= self.tuning.slots {
            return Err(rod);
        }
        self.rods.push(rod);
        Ok(())
    }

    pub fn remove_rod(&mut self, slot: usize) -> Option<FuelRod> {
        (slot < self.rods.len()).then(|| self.rods.remove(slot))
    }

    // ---------- readouts ----------

    pub fn tuning(&self) -> &ReactorTuning {
        &self.tuning
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn power_on(&self) -> bool {
        self.power_on
    }
    pub fn safety(&self) -> bool {
        self.safety
    }
    pub fn temperature(&self) -> f64 {
        self.temp
    }
    pub fn fission(&self) -> f64 {
        self.fission
    }
    pub fn turbine(&self) -> f64 {
        self.turbine
    }
    /// (fission, turbine) targets.
    pub fn targets(&self) -> (f64, f64) {
        (self.target_fission, self.target_turbine)
    }
    /// Output of the last step, kW.
    pub fn output_kw(&self) -> f64 {
        self.output
    }
    pub fn rods(&self) -> &[FuelRod] {
        &self.rods
    }
    /// Sum of the heat of the rods that are not spent.
    pub fn available_fuel(&self) -> f64 {
        self.rods.iter().filter(|r| !r.spent()).map(|r| r.heat).sum()
    }
    /// Seconds left of the restart lockout.
    pub fn lockout_s(&self) -> f64 {
        self.lockout
    }
    /// 0..1 of the fire timer and the shutdown (or meltdown) timer, for the panel.
    pub fn timers(&self) -> (f64, f64) {
        let over = if self.safety { self.tuning.shutdown_delay_s } else { self.tuning.meltdown_delay_s };
        (self.fire_timer / self.tuning.fire_delay_s, self.overheat_timer / over)
    }
    pub fn bands(&self) -> Bands {
        Bands::new(self.skill, self.turbine_for_load, self.fuel)
    }
    pub fn alarm(&self, kind: AlarmKind) -> bool {
        self.alarms[kind.index()]
    }
    pub fn condition(&self) -> &Condition {
        &self.condition
    }
    /// For repair and damage.
    pub fn condition_mut(&mut self) -> &mut Condition {
        &mut self.condition
    }

    // ---------- simulation ----------

    /// Advances `dt` seconds with the grid asking for `load_kw`. Returns what happened; the
    /// output is `output_kw()`.
    pub fn step(&mut self, dt: f64, load_kw: f64) -> Vec<ReactorEvent> {
        let t = self.tuning;
        let mut ev = Vec::new();
        let k = dt.min(1.0);

        if !self.condition.functioning() {
            // Broken: off the grid, cools fast, targets run down. Fission and turbine stay where
            // they were (the original freezes them too).
            self.power_on = false;
            self.output = 0.0;
            self.temp = (self.temp - 100.0 * dt).max(0.0);
            self.target_fission = (self.target_fission - 10.0 * dt).max(0.0);
            self.target_turbine = (self.target_turbine - 10.0 * dt).max(0.0);
            self.fire_timer = 0.0;
            self.overheat_timer = 0.0;
            self.lockout = (self.lockout - dt).max(0.0);
            self.update_alarms(&mut ev);
            return ev;
        }

        let fuel = self.available_fuel();
        if t.max_output_kw > 0.0 {
            let want = load_kw / t.max_output_kw * 100.0;
            self.turbine_for_load += (want - self.turbine_for_load).clamp(-20.0, 20.0) * dt;
        }
        let bands = Bands::new(self.skill, self.turbine_for_load, fuel);

        let heat = self.fission * (self.fuel / 100.0) * 2.0;
        let cool = t.cooling_max * self.coolant;
        let diff = (heat - self.turbine - cool) - self.temp;
        self.temp = (self.temp + (diff.signum() * t.temp_rate * dt).clamp(-diff.abs(), diff.abs())).clamp(0.0, 100.0);
        self.fission += (self.target_fission.min(fuel) - self.fission) * k;
        self.turbine += (self.target_turbine - self.turbine) * k;

        if !self.power_on {
            self.target_fission = 0.0;
            self.target_turbine = 0.0;
        } else if self.mode == Mode::Auto {
            self.auto(dt, &bands, load_kw);
        }

        if self.fission > 0.0 {
            let burn = self.fission / 100.0 * t.fuel_burn * dt;
            for r in self.rods.iter_mut().filter(|r| !r.spent()) {
                r.life = (r.life - burn).max(0.0);
            }
        }
        self.fuel = fuel;
        self.lockout = (self.lockout - dt).max(0.0);

        self.overheat(dt, &bands, &mut ev);
        self.output = self.output_for(load_kw, &bands);

        let use_ = if self.temp > 0.1 { (self.fission + self.turbine) / 200.0 } else { 0.0 };
        if let Some(e) = self.condition.deteriorate(dt, use_) {
            ev.push(ReactorEvent::Device(e));
        }
        self.update_alarms(&mut ev);
        ev
    }

    /// The output range for the current turbine and temperature, kW.
    pub fn output_range(&self) -> (f64, f64) {
        self.range(&self.bands())
    }

    fn range(&self, b: &Bands) -> (f64, f64) {
        let p = self.tuning.max_output_kw;
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
        let st = self.tuning.auto_turbine_speed;
        let s = self.tuning.auto_fission_speed;
        self.target_turbine += (b.turbine_optimal.mid() - self.target_turbine).clamp(-st, st) * dt;
        self.target_turbine = self.target_turbine.clamp(0.0, 100.0);
        let mid = b.temp_optimal.mid();
        if self.temp > mid + AUTO_DEAD_BAND {
            self.target_fission = (self.target_fission - s * dt).min(b.fission_allowed.hi);
        } else if self.temp < mid - AUTO_DEAD_BAND || self.output < load_kw - 1.0 {
            self.target_fission = (self.target_fission + s * dt).min(100.0);
        }
        // Not far from the actual rate, or it overshoots while the temperature catches up.
        self.target_fission = self.target_fission.clamp(0.0, 100.0).clamp(self.fission - 5.0, self.fission + 5.0).max(0.0);
    }

    fn overheat(&mut self, dt: f64, b: &Bands, ev: &mut Vec<ReactorEvent>) {
        let t = self.tuning;
        // Twice as fast at condition 0, normal at full.
        let step = dt * (2.0 - self.condition.fraction());
        if self.temp > b.temp_allowed.hi {
            self.overheat_timer += step;
            if self.safety && self.overheat_timer >= t.shutdown_delay_s {
                self.power_on = false;
                self.target_fission = 0.0;
                self.target_turbine = 0.0;
                self.overheat_timer = 0.0;
                self.lockout = t.restart_lockout_s;
                self.set_alarm(AlarmKind::Shutdown, true, ev);
                ev.push(ReactorEvent::Shutdown);
            } else if !self.safety && self.overheat_timer >= t.meltdown_delay_s {
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
        let before = self.condition.value();
        self.condition.set_value(0.0);
        self.power_on = false;
        self.fire_timer = 0.0;
        self.overheat_timer = 0.0;
        for r in &mut self.rods {
            r.life = 0.0;
        }
        ev.push(ReactorEvent::Meltdown);
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
        if self.power_on {
            self.set_alarm(AlarmKind::Shutdown, false, ev);
        }
    }

    fn set_alarm(&mut self, kind: AlarmKind, on: bool, ev: &mut Vec<ReactorEvent>) {
        if self.alarms[kind.index()] != on {
            self.alarms[kind.index()] = on;
            ev.push(ReactorEvent::Alarm { kind, on });
        }
    }
}
