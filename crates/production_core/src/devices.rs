//! The lab device chain (#287): a `Batch` leaves the kettle (the mixer) and goes through the
//! heater and the centrifuge into syringes. Each device is hands-on: the player sets a value and
//! watches it; doing a step well raises the batch's quality, doing it badly costs quality or volume.
//! Skill (0..100) scales every device time and gives bonus tiers when the batch is drawn up.
//!
//! Quality: the kettle's needle gives up to `mixer_share`, a held heat adds `heater.quality_bonus`,
//! a full separation adds `centrifuge.quality_bonus`, each skill tier adds `skill.quality_per_tier`.
//! The shipped values add up to 1.
//!
//! - `DeviceTuning`: every number, from `lab/devices.json`.
//! - `Batch`: what moves between the devices.
//! - `Heater`, `Centrifuge`: `load`, the player's value, `step(dt, skill, tuning)`, `unload`.
//! - `fill_syringe` and `Syringe::inject` with a `Target`.
use std::collections::BTreeSet;

use content_core::{File, parse_strict};
use gameplay_core::{CommodityId, Content, EffectId, ProductSpec};
use serde::{Deserialize, Serialize};

use crate::ingredient::Transform;
use crate::skill::{self, Dice};

pub const DEVICES_PATH: &str = "lab/devices.json";

/// How skill shapes lab work (`skill` module). Every value is a starting value, TODO(initiator).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SkillTuning {
    /// The skill at which the devices run at their base times.
    pub required: f64,
    /// The speed factor far below the requirement (a time divides by it).
    pub slowest: f64,
    /// The speed factor far above the requirement.
    pub fastest: f64,
    /// Skill from which the first bonus tier can roll.
    pub plus_one_from: f64,
    /// Skill from which the second bonus tier can roll.
    pub plus_two_from: f64,
    /// Quality added per bonus tier.
    pub quality_per_tier: f64,
}

/// The heater: a dial (setpoint), an element that follows it and a batch that follows the
/// element, each with its own lag. TODO(initiator): every value.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeaterTuning {
    pub max_setpoint_c: f64,
    /// Time constant of the element towards the setpoint.
    pub element_lag_s: f64,
    /// Time constant of the batch towards the element.
    pub batch_lag_s: f64,
    pub band_low_c: f64,
    pub band_high_c: f64,
    /// Above this the batch burns, once.
    pub burn_c: f64,
    /// Seconds in the band (at speed 1) for a full heat.
    pub hold_s: f64,
    pub quality_bonus: f64,
    /// A burn multiplies the quality by this.
    pub burn_quality_factor: f64,
    /// Effects a full heat turns (same rule as an ingredient's transforms).
    #[serde(default)]
    pub transforms: Vec<Transform>,
}

/// The centrifuge. TODO(initiator): every value.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CentrifugeTuning {
    pub max_rpm: f64,
    /// Below this nothing separates.
    pub sweet_low_rpm: f64,
    /// Above this the spin wastes volume.
    pub sweet_high_rpm: f64,
    /// Seconds in the sweet range (at speed 1) for a full separation.
    pub separate_s: f64,
    pub quality_bonus: f64,
    /// Share of the volume lost per second above the sweet range.
    pub fast_loss_per_s: f64,
    /// Share of the volume lost per second of spinning after full separation.
    pub overspin_loss_per_s: f64,
    /// Above this the drum wobbles.
    pub wobble_rpm: f64,
    /// Wobble gained per second per 1000 rpm above `wobble_rpm`; at 1 the centrifuge trips.
    pub wobble_per_s_per_krpm: f64,
    /// Wobble lost per second at or below `wobble_rpm`.
    pub wobble_decay_per_s: f64,
    /// Share of the volume spilled by a trip.
    pub trip_loss: f64,
}

/// The syringe and its use. TODO(initiator): every value.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SyringeTuning {
    /// Doses per syringe.
    pub capacity: u32,
    /// The skill at which an injection always succeeds; each point below loses 1 % chance.
    pub required_skill: f64,
    /// Strength of a dose of quality 0 (quality 1 gives 1).
    pub min_strength: f64,
    /// A failed injection multiplies the strength by this.
    pub fail_strength: f64,
}

/// Every number of the device chain, from `lab/devices.json`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceTuning {
    /// Room temperature: a fresh batch and a cold heater.
    pub ambient_c: f64,
    /// Doses per product crate of the kettle.
    pub doses_per_crate: u32,
    /// The kettle's needle quality is scaled by this into the batch.
    pub mixer_share: f64,
    pub skill: SkillTuning,
    pub heater: HeaterTuning,
    pub centrifuge: CentrifugeTuning,
    pub syringe: SyringeTuning,
}

impl DeviceTuning {
    /// Loads `lab/devices.json` from `files` and checks it (heat effects against the kernel).
    /// Returns every error, each naming the file and the field.
    pub fn load(files: &[File], kernel: &Content) -> Result<DeviceTuning, Vec<String>> {
        let p = DEVICES_PATH;
        let Some(f) = files.iter().find(|f| f.path == p) else { return Err(vec![format!("{p}: missing")]) };
        let t = parse_strict::<DeviceTuning>(p, &f.text).map_err(|m| vec![m])?;
        let mut e = Vec::new();
        t.check(kernel, &mut e);
        if e.is_empty() { Ok(t) } else { Err(e) }
    }

    fn check(&self, k: &Content, e: &mut Vec<String>) {
        let p = DEVICES_PATH;
        let mut need = |ok: bool, field: &str, rule: &str| {
            if !ok {
                e.push(format!("{p}: {field}: {rule}"));
            }
        };
        let unit = |x: f64| (0.0..=1.0).contains(&x);
        need(self.doses_per_crate >= 1, "doses_per_crate", "must be at least 1");
        need(unit(self.mixer_share), "mixer_share", "must be in 0..1");
        let s = &self.skill;
        need((0.0..=100.0).contains(&s.required), "skill.required", "must be in 0..100");
        need(s.slowest > 0.0 && s.slowest <= 1.0, "skill.slowest", "must be in (0, 1]");
        need(s.fastest >= 1.0, "skill.fastest", "must be at least 1");
        need(s.plus_one_from <= s.plus_two_from, "skill.plus_two_from", "must be at least plus_one_from");
        need(unit(s.quality_per_tier), "skill.quality_per_tier", "must be in 0..1");
        let h = &self.heater;
        need(h.element_lag_s > 0.0, "heater.element_lag_s", "must be > 0");
        need(h.batch_lag_s > 0.0, "heater.batch_lag_s", "must be > 0");
        need(self.ambient_c < h.band_low_c, "heater.band_low_c", "must be above ambient_c");
        need(h.band_low_c < h.band_high_c, "heater.band_high_c", "must be above band_low_c");
        need(h.band_high_c < h.burn_c, "heater.burn_c", "must be above band_high_c");
        need(h.burn_c <= h.max_setpoint_c, "heater.max_setpoint_c", "must be at least burn_c");
        need(h.hold_s > 0.0, "heater.hold_s", "must be > 0");
        need(unit(h.quality_bonus), "heater.quality_bonus", "must be in 0..1");
        need(unit(h.burn_quality_factor), "heater.burn_quality_factor", "must be in 0..1");
        let c = &self.centrifuge;
        need(c.sweet_low_rpm > 0.0, "centrifuge.sweet_low_rpm", "must be > 0");
        need(c.sweet_low_rpm < c.sweet_high_rpm, "centrifuge.sweet_high_rpm", "must be above sweet_low_rpm");
        need(c.sweet_high_rpm <= c.wobble_rpm, "centrifuge.wobble_rpm", "must be at least sweet_high_rpm");
        need(c.wobble_rpm <= c.max_rpm, "centrifuge.max_rpm", "must be at least wobble_rpm");
        need(c.separate_s > 0.0, "centrifuge.separate_s", "must be > 0");
        need(unit(c.quality_bonus), "centrifuge.quality_bonus", "must be in 0..1");
        need(c.fast_loss_per_s >= 0.0, "centrifuge.fast_loss_per_s", "must be >= 0");
        need(c.overspin_loss_per_s >= 0.0, "centrifuge.overspin_loss_per_s", "must be >= 0");
        need(c.wobble_per_s_per_krpm >= 0.0, "centrifuge.wobble_per_s_per_krpm", "must be >= 0");
        need(c.wobble_decay_per_s >= 0.0, "centrifuge.wobble_decay_per_s", "must be >= 0");
        need(unit(c.trip_loss), "centrifuge.trip_loss", "must be in 0..1");
        let y = &self.syringe;
        need(y.capacity >= 1, "syringe.capacity", "must be at least 1");
        need(unit(y.min_strength), "syringe.min_strength", "must be in 0..1");
        need(unit(y.fail_strength), "syringe.fail_strength", "must be in 0..1");
        for (i, t) in h.transforms.iter().enumerate() {
            for (side, id) in [("from", &t.from), ("to", &t.to)] {
                if !k.effects.contains_key(id) {
                    e.push(format!("{p}: heater.transforms[{i}].{side}: unknown effect '{id}'"));
                }
            }
        }
    }
}

/// What a device refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceRefusal {
    /// It holds a batch already, or is running.
    Busy,
    /// Nothing in it (or not enough for one dose).
    Empty,
}

/// A product on its way through the lab.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    pub base: CommodityId,
    pub ingredients: Vec<CommodityId>,
    pub effects: BTreeSet<EffectId>,
    /// 0..1.
    pub quality: f64,
    /// In doses.
    pub volume: f64,
    pub temp_c: f64,
    /// Heat progress 0..1; at 1 the heat bonus was given.
    pub heat: f64,
    pub burnt: bool,
    /// Separation 0..1 from the centrifuge.
    pub separation: f64,
    /// The bonus tiers, rolled once when the batch is first drawn up.
    pub bonus_tiers: Option<u8>,
}

impl Batch {
    /// A fresh batch from a released kettle: the needle quality scaled by `mixer_share`, the
    /// crates turned into doses, at room temperature.
    pub fn from_brew(product: &ProductSpec, ingredients: Vec<CommodityId>, crates: u32, t: &DeviceTuning) -> Batch {
        Batch {
            base: product.base.clone(),
            ingredients,
            effects: product.effects.clone(),
            quality: product.quality * t.mixer_share,
            volume: (crates * t.doses_per_crate) as f64,
            temp_c: t.ambient_c,
            heat: 0.0,
            burnt: false,
            separation: 0.0,
            bonus_tiers: None,
        }
    }
}

/// Heats one batch. The player turns the setpoint; the element and the batch follow with lag, so
/// cranking it up and turning down late overshoots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Heater {
    batch: Option<Batch>,
    setpoint_c: f64,
    element_c: f64,
    /// The hottest the current batch has been, for the readout.
    peak_c: f64,
}

impl Heater {
    pub fn new(t: &DeviceTuning) -> Heater {
        Heater { batch: None, setpoint_c: t.ambient_c, element_c: t.ambient_c, peak_c: t.ambient_c }
    }

    pub fn load(&mut self, b: Batch) -> Result<(), DeviceRefusal> {
        if self.batch.is_some() {
            return Err(DeviceRefusal::Busy);
        }
        self.peak_c = b.temp_c;
        self.batch = Some(b);
        Ok(())
    }

    pub fn unload(&mut self) -> Option<Batch> {
        self.batch.take()
    }

    pub fn batch(&self) -> Option<&Batch> {
        self.batch.as_ref()
    }

    /// Turns the dial (clamped to ambient..max).
    pub fn set_setpoint(&mut self, c: f64, t: &DeviceTuning) {
        self.setpoint_c = c.clamp(t.ambient_c, t.heater.max_setpoint_c);
    }

    pub fn setpoint(&self) -> f64 {
        self.setpoint_c
    }

    pub fn element(&self) -> f64 {
        self.element_c
    }

    /// The batch's temperature; None when empty.
    pub fn temperature(&self) -> Option<f64> {
        self.batch.as_ref().map(|b| b.temp_c)
    }

    pub fn peak_c(&self) -> f64 {
        self.peak_c
    }

    /// Advances `dt` seconds. A batch in the band gains heat progress (faster with skill); a full
    /// heat adds the bonus and turns effects, once. Above the burn line it burns, once.
    pub fn step(&mut self, dt: f64, skill: f64, t: &DeviceTuning) {
        let h = &t.heater;
        self.element_c += (self.setpoint_c - self.element_c) * (1.0 - (-dt / h.element_lag_s).exp());
        let Some(b) = &mut self.batch else { return };
        b.temp_c += (self.element_c - b.temp_c) * (1.0 - (-dt / h.batch_lag_s).exp());
        self.peak_c = self.peak_c.max(b.temp_c);
        if !b.burnt && b.temp_c > h.burn_c {
            b.burnt = true;
            b.quality *= h.burn_quality_factor;
        }
        if !b.burnt && b.heat < 1.0 && (h.band_low_c..=h.band_high_c).contains(&b.temp_c) {
            b.heat = (b.heat + dt * skill::speed(skill, t.skill.required, &t.skill) / h.hold_s).min(1.0);
            if b.heat >= 1.0 {
                b.quality = (b.quality + h.quality_bonus).min(1.0);
                apply_transforms(&mut b.effects, &h.transforms);
            }
        }
    }
}

fn apply_transforms(effects: &mut BTreeSet<EffectId>, ts: &[Transform]) {
    for t in ts {
        if effects.contains(&t.from) && !effects.contains(&t.to) {
            effects.remove(&t.from);
            effects.insert(t.to.clone());
        }
    }
}

/// What the centrifuge is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Spin {
    Idle,
    Spinning,
    /// Ran its time or was stopped.
    Done,
    /// Wobbled too much and stopped itself.
    Tripped,
}

/// Spins one batch. The player sets the speed and the time; the speed can change while it runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Centrifuge {
    batch: Option<Batch>,
    rpm: f64,
    left_s: f64,
    /// 0..1; at 1 it trips.
    wobble: f64,
    state: Spin,
}

impl Centrifuge {
    pub fn new() -> Centrifuge {
        Centrifuge { batch: None, rpm: 0.0, left_s: 0.0, wobble: 0.0, state: Spin::Idle }
    }

    pub fn load(&mut self, b: Batch) -> Result<(), DeviceRefusal> {
        if self.batch.is_some() {
            return Err(DeviceRefusal::Busy);
        }
        self.batch = Some(b);
        self.state = Spin::Idle;
        Ok(())
    }

    /// Takes the batch out; not while it spins.
    pub fn unload(&mut self) -> Option<Batch> {
        if self.state == Spin::Spinning {
            return None;
        }
        self.state = Spin::Idle;
        self.batch.take()
    }

    pub fn batch(&self) -> Option<&Batch> {
        self.batch.as_ref()
    }

    /// Starts a run at `rpm` for `secs`.
    pub fn start(&mut self, rpm: f64, secs: f64, t: &DeviceTuning) -> Result<(), DeviceRefusal> {
        if self.batch.is_none() {
            return Err(DeviceRefusal::Empty);
        }
        if self.state == Spin::Spinning {
            return Err(DeviceRefusal::Busy);
        }
        self.set_rpm(rpm, t);
        self.left_s = secs.max(0.0);
        self.wobble = 0.0;
        self.state = Spin::Spinning;
        Ok(())
    }

    /// Changes the speed (clamped to 0..max).
    pub fn set_rpm(&mut self, rpm: f64, t: &DeviceTuning) {
        self.rpm = rpm.clamp(0.0, t.centrifuge.max_rpm);
    }

    /// Stops a run early.
    pub fn stop(&mut self) {
        if self.state == Spin::Spinning {
            self.state = Spin::Done;
        }
    }

    pub fn state(&self) -> Spin {
        self.state
    }

    pub fn rpm(&self) -> f64 {
        self.rpm
    }

    pub fn left_s(&self) -> f64 {
        self.left_s
    }

    pub fn wobble(&self) -> f64 {
        self.wobble
    }

    /// Advances `dt` seconds of a run. In the sweet range the batch separates (faster with skill)
    /// and gains quality as it does; past full separation, or above the range, it loses volume.
    /// Above the wobble speed the drum wobbles and at 1 it trips, spilling a share.
    pub fn step(&mut self, dt: f64, skill: f64, t: &DeviceTuning) {
        if self.state != Spin::Spinning {
            return;
        }
        let c = &t.centrifuge;
        let Some(b) = &mut self.batch else { return };
        let used = dt.min(self.left_s);
        self.left_s -= used;
        if self.rpm >= c.sweet_low_rpm {
            let speed = skill::speed(skill, t.skill.required, &t.skill);
            let gain = (used * speed / c.separate_s).min(1.0 - b.separation);
            b.separation += gain;
            b.quality = (b.quality + c.quality_bonus * gain).min(1.0);
            let after_full_s = used - gain * c.separate_s / speed;
            if after_full_s > 0.0 {
                b.volume *= (-c.overspin_loss_per_s * after_full_s).exp();
            }
            if self.rpm > c.sweet_high_rpm {
                b.volume *= (-c.fast_loss_per_s * used).exp();
            }
        }
        if self.rpm > c.wobble_rpm {
            self.wobble += used * c.wobble_per_s_per_krpm * (self.rpm - c.wobble_rpm) / 1000.0;
        } else {
            self.wobble = (self.wobble - used * c.wobble_decay_per_s).max(0.0);
        }
        if self.wobble >= 1.0 {
            b.volume *= 1.0 - c.trip_loss;
            self.wobble = 0.0;
            self.state = Spin::Tripped;
        } else if self.left_s <= 1e-9 {
            self.state = Spin::Done;
        }
    }
}

impl Default for Centrifuge {
    fn default() -> Self {
        Self::new()
    }
}

/// A filled syringe: the product (its quality included) and the doses left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Syringe {
    pub product: ProductSpec,
    pub doses: u32,
}

/// One injected dose.
#[derive(Clone, Debug, PartialEq)]
pub struct Dose {
    pub effects: BTreeSet<EffectId>,
    /// 0..1, from the quality, lowered by a failed injection.
    pub strength: f64,
    pub success: bool,
}

/// Whoever receives a dose (a crew member, later the affliction model).
pub trait Target {
    fn receive(&mut self, dose: &Dose);
}

/// Draws up to `doses` whole doses (at most the syringe's capacity) from the batch. The first
/// draw rolls the batch's bonus tiers with the filler's skill; every syringe of the batch shares it.
pub fn fill_syringe(batch: &mut Batch, doses: u32, skill: f64, t: &DeviceTuning, dice: &mut Dice) -> Result<Syringe, DeviceRefusal> {
    let whole = batch.volume.max(0.0).floor() as u32;
    let n = doses.min(t.syringe.capacity).min(whole);
    if n == 0 {
        return Err(DeviceRefusal::Empty);
    }
    let tiers = *batch.bonus_tiers.get_or_insert_with(|| skill::bonus_tiers(skill, t.skill.required, &t.skill, dice));
    batch.volume -= n as f64;
    let quality = (batch.quality + tiers as f64 * t.skill.quality_per_tier).min(1.0);
    Ok(Syringe { product: ProductSpec { base: batch.base.clone(), effects: batch.effects.clone(), quality }, doses: n })
}

impl Syringe {
    /// Injects one dose into `target`. The injection succeeds with a chance that falls 1 % per
    /// skill point below the requirement; a failure gives a weaker dose.
    pub fn inject<T: Target + ?Sized>(&mut self, target: &mut T, skill: f64, t: &DeviceTuning, dice: &mut Dice) -> Result<Dose, DeviceRefusal> {
        if self.doses == 0 {
            return Err(DeviceRefusal::Empty);
        }
        let s = &t.syringe;
        let success = dice.chance((1.0 + (skill - s.required_skill) / 100.0).clamp(0.0, 1.0));
        let full = s.min_strength + (1.0 - s.min_strength) * self.product.quality;
        let dose = Dose { effects: self.product.effects.clone(), strength: if success { full } else { full * s.fail_strength }, success };
        self.doses -= 1;
        target.receive(&dose);
        Ok(dose)
    }
}
