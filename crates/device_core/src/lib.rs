//! device_core: the condition of a ship device, without engine types, in f64. Every machine the engineer keeps alive (reactor, engines, shield generator, oxygen
//! generator) carries one `Condition`.
//!
//! Taken from Barotrauma's repairable devices: a condition bar with a repair threshold, repair time
//! between a slow and a fast duration by skill, a skill roll that can fail, slow wear after a random
//! delay, and skill gain divided by skill^1.5. Our own code; the broken limit, the output factor and
//! the overload wear are ours (`research/barotrauma-mech-damage-repair.md` in the concept repo).
//!
//! - Condition runs from 0 to `max`. Below `repair_threshold` the device asks for repair (HUD,
//!   orders), below `broken_below` it stops working.
//! - `factor()` is what the device puts out: 1 above the repair threshold, falling to
//!   `factor_at_broken` at the broken limit, 0 when broken.
//! - Hold-to-repair: `repair_step` while the button is held, `stop_repair` on release. Each attempt
//!   starts with one roll; a failure costs condition, locks repair for a moment, and the glue
//!   shocks the repairer.
//! - Wear: `deteriorate(dt, load)`. Normal wear starts after a random delay and stops at a floor;
//!   overload (`load > 1`) wears at once and down to 0.
//!
//! Skill is the player's value 0..100 (`skills-and-talents.md`: mechanical for repair). All values
//! are starting points for the playtest (`TODO(initiator)`).
use gameplay_core::rng::Rng;

pub mod minigame;

#[cfg(test)]
mod tests;

/// Values of one device kind. TODO(initiator): every number is a starting value; later from data.
#[derive(Copy, Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceTuning {
    /// Full condition.
    pub max: f64,
    /// Below this the device asks for repair. Only full repairs from below it give skill.
    pub repair_threshold: f64,
    /// Below this the device does not work at all.
    pub broken_below: f64,
    /// Output factor just above the broken limit; it rises linearly to 1 at the repair threshold.
    pub factor_at_broken: f64,
    /// Seconds from 0 to full at skill 0 with a standard tool.
    pub fix_slow_s: f64,
    /// Seconds from 0 to full at skill 100.
    pub fix_fast_s: f64,
    /// Skill at and above which a repair never fails.
    pub required_skill: f64,
    /// Chance that an attempt fails at skill 0; falls linearly to 0 at `required_skill`.
    pub fail_chance: f64,
    /// Condition lost by a failed attempt.
    pub fail_cost: f64,
    /// Seconds repair is locked after a failure.
    pub fail_lockout_s: f64,
    /// Condition per second of normal wear.
    pub wear_rate: f64,
    /// Normal wear stops here.
    pub wear_floor: f64,
    /// Seconds of use before normal wear starts, drawn from this range after each full repair.
    pub wear_delay_s: (f64, f64),
    /// Wear while idle, as a share of `wear_rate` (0: no idle wear, and the delay does not run).
    pub idle_wear: f64,
    /// Extra wear per unit of load above 1, as a multiple of `wear_rate`. Ignores delay and floor.
    pub overload_wear: f64,
}

impl Default for DeviceTuning {
    fn default() -> DeviceTuning {
        // TODO(initiator): starting values. Fix times, wear rate, floor and delay follow the
        // original's reactor; the broken limit, the factor curve, the failure chance and the
        // overload wear are guesses.
        DeviceTuning {
            max: 100.0,
            repair_threshold: 50.0,
            broken_below: 20.0,
            factor_at_broken: 0.5,
            fix_slow_s: 25.0,
            fix_fast_s: 5.0,
            required_skill: 50.0,
            fail_chance: 0.3,
            fail_cost: 5.0,
            fail_lockout_s: 2.0,
            wear_rate: 0.125,
            wear_floor: 50.0,
            wear_delay_s: (120.0, 720.0),
            idle_wear: 0.0,
            overload_wear: 4.0,
        }
    }
}

/// What the repairer holds. Speed multiplies the repair rate.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Tool {
    pub speed: f64,
}

impl Tool {
    /// The standard repair tool.
    pub const WRENCH: Tool = Tool { speed: 1.0 };
    /// The screwdriver (junction box). TODO(initiator): speed.
    pub const SCREWDRIVER: Tool = Tool { speed: 0.8 };
    /// No tool. TODO(initiator): allowed at all, and how slow?
    pub const HANDS: Tool = Tool { speed: 0.5 };
}

/// What one `repair_step` did.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RepairOutcome {
    /// Condition rose; `progress` (0..1) is the bar to show.
    Working { progress: f64 },
    /// The attempt failed at its start: `lost` condition; the glue shocks or burns the repairer.
    Failed { lost: f64 },
    /// Repair is locked after a failure.
    Locked { remaining_s: f64 },
    /// The device is full. `skill_gain` goes to the repairer's skill (0 if the device did not
    /// need repair when the attempt started).
    Done { skill_gain: f64 },
    /// Nothing to repair.
    Full,
}

/// A change of state the glue may show or sound.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DeviceEvent {
    /// Fell below the repair threshold.
    NeedsRepair,
    /// Fell below the broken limit: the device stops.
    Broke,
    /// Rose above the broken limit again: it works (at `factor()`).
    Works,
}

/// The condition of one device.
#[derive(Clone, Debug)]
pub struct Condition {
    tuning: DeviceTuning,
    value: f64,
    rng: Rng,
    wear_delay: f64,
    /// An attempt is running (the button is held and the start roll passed).
    repairing: bool,
    /// The running attempt started below the repair threshold.
    gains_skill: bool,
    lockout: f64,
}

impl Condition {
    /// A device at full condition. `seed` makes rolls and wear delays repeatable.
    pub fn new(tuning: DeviceTuning, seed: u64) -> Condition {
        let mut rng = Rng::new(seed);
        let wear_delay = draw(&mut rng, tuning.wear_delay_s);
        Condition { tuning, value: tuning.max, rng, wear_delay, repairing: false, gains_skill: false, lockout: 0.0 }
    }

    pub fn tuning(&self) -> &DeviceTuning {
        &self.tuning
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// 0..1.
    pub fn fraction(&self) -> f64 {
        self.value / self.tuning.max
    }

    /// Sets the value directly (debug menu, scenarios, save).
    pub fn set_value(&mut self, v: f64) {
        self.value = v.clamp(0.0, self.tuning.max);
    }

    pub fn needs_repair(&self) -> bool {
        self.value < self.tuning.repair_threshold
    }

    pub fn functioning(&self) -> bool {
        self.value >= self.tuning.broken_below
    }

    /// Output factor 0..1 of the device at this condition.
    pub fn factor(&self) -> f64 {
        let t = &self.tuning;
        if !self.functioning() {
            return 0.0;
        }
        if self.value >= t.repair_threshold {
            return 1.0;
        }
        let span = (t.repair_threshold - t.broken_below).max(1e-9);
        let x = (self.value - t.broken_below) / span;
        t.factor_at_broken + (1.0 - t.factor_at_broken) * x
    }

    /// Seconds of use before normal wear starts.
    pub fn wear_delay_left(&self) -> f64 {
        self.wear_delay
    }

    pub fn is_repairing(&self) -> bool {
        self.repairing
    }

    /// Damage from outside (fire, a hit). Returns the state change, if any.
    pub fn damage(&mut self, amount: f64) -> Option<DeviceEvent> {
        let before = self.value;
        self.value = (self.value - amount.max(0.0)).max(0.0);
        self.change(before)
    }

    /// Wear for `dt` seconds at `load`: 0 idle, 1 full use, above 1 overloaded.
    pub fn deteriorate(&mut self, dt: f64, load: f64) -> Option<DeviceEvent> {
        let t = self.tuning;
        let before = self.value;
        if load > 1.0 {
            self.value = (self.value - t.wear_rate * t.overload_wear * (load - 1.0) * dt).max(0.0);
        }
        let mult = if load > 0.0 { 1.0 } else { t.idle_wear };
        if mult > 0.0 {
            // Normal wear never goes below the floor, and never below the broken limit either: wear
            // alone can ask for repair, it cannot break the device.
            let floor = t.wear_floor.max(t.broken_below);
            if self.wear_delay > 0.0 {
                self.wear_delay = (self.wear_delay - dt).max(0.0);
            } else if self.value > floor {
                self.value = (self.value - t.wear_rate * mult * dt).max(floor);
            }
        }
        self.change(before)
    }

    /// Raises the condition by `amount` (a hit of the repair minigame, or a repair step). The
    /// delay of the next wear is drawn again when it reaches full. Returns `Works` when it crosses
    /// back over the broken limit.
    pub fn restore(&mut self, amount: f64) -> Option<DeviceEvent> {
        let before = self.value;
        self.value = (self.value + amount.max(0.0)).min(self.tuning.max);
        if before < self.tuning.max && self.value >= self.tuning.max {
            self.wear_delay = draw(&mut self.rng, self.tuning.wear_delay_s);
        }
        self.change(before)
    }

    /// One repair step of `dt` seconds at `skill` with `tool`: `restore` by the rate of a full
    /// repair. Returns the same as `restore`.
    pub fn repair_rise(&mut self, dt: f64, skill: f64, tool: Tool) -> Option<DeviceEvent> {
        self.restore(dt * self.tuning.max / self.repair_seconds(skill, tool))
    }

    /// One step of hold-to-repair, `skill` 0..100. Call it every frame while the button is held,
    /// and `stop_repair` when it is released.
    pub fn repair_step(&mut self, dt: f64, skill: f64, tool: Tool) -> RepairOutcome {
        let t = self.tuning;
        if self.lockout > 0.0 {
            self.lockout = (self.lockout - dt).max(0.0);
            return RepairOutcome::Locked { remaining_s: self.lockout };
        }
        if self.value >= t.max {
            self.repairing = false;
            return RepairOutcome::Full;
        }
        if !self.repairing {
            if self.rng.unit() < self.fail_chance(skill) {
                let lost = self.value.min(t.fail_cost);
                self.value -= lost;
                self.lockout = t.fail_lockout_s;
                return RepairOutcome::Failed { lost };
            }
            self.repairing = true;
            self.gains_skill = self.needs_repair();
        }
        self.repair_rise(dt, skill, tool);
        if self.value >= t.max {
            self.repairing = false;
            let skill_gain = if self.gains_skill { skill_gain(REPAIR_SKILL_GAIN, skill) } else { 0.0 };
            return RepairOutcome::Done { skill_gain };
        }
        RepairOutcome::Working { progress: self.fraction() }
    }

    /// The button was released: the next `repair_step` starts a new attempt (with a new roll).
    pub fn stop_repair(&mut self) {
        self.repairing = false;
    }

    /// Seconds a repair from 0 to full takes at this skill with this tool.
    pub fn repair_seconds(&self, skill: f64, tool: Tool) -> f64 {
        let t = &self.tuning;
        let d = (skill / 100.0).clamp(0.0, 1.0);
        (t.fix_slow_s + (t.fix_fast_s - t.fix_slow_s) * d) / tool.speed.max(1e-6)
    }

    /// Chance that an attempt fails at this skill.
    pub fn fail_chance(&self, skill: f64) -> f64 {
        let t = &self.tuning;
        if t.required_skill <= 0.0 {
            return 0.0;
        }
        t.fail_chance * (1.0 - skill / t.required_skill).clamp(0.0, 1.0)
    }

    fn change(&self, before: f64) -> Option<DeviceEvent> {
        let t = &self.tuning;
        let (b, a) = (before, self.value);
        if b >= t.broken_below && a < t.broken_below {
            Some(DeviceEvent::Broke)
        } else if b < t.broken_below && a >= t.broken_below {
            Some(DeviceEvent::Works)
        } else if b >= t.repair_threshold && a < t.repair_threshold {
            Some(DeviceEvent::NeedsRepair)
        } else {
            None
        }
    }
}

/// Skill points for one full repair from below the threshold. TODO(initiator): the original's
/// value; a session multiplier for short sessions is open (`skills-and-talents.md`).
pub const REPAIR_SKILL_GAIN: f64 = 40.0;

/// Skill gain for an action worth `base` at the current `skill`: `base / max(skill, 15)^1.5`,
/// at most 2. Early gains are fast, late ones slow.
pub fn skill_gain(base: f64, skill: f64) -> f64 {
    (base / skill.max(15.0).powf(1.5)).min(2.0)
}

fn draw(rng: &mut Rng, (lo, hi): (f64, f64)) -> f64 {
    lo + (hi - lo) * rng.unit()
}
