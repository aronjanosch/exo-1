//! health_core: a character's health (#254, #287). Afflictions (burn, bleeding, fracture, oxygen
//! low, poison, and harmless statuses from the lab such as giggly) each have a strength; their
//! strength bands set how much vitality they take, how fast they heal by themselves and how they
//! slow the walk. Vitality is the maximum minus what the afflictions take: at 0 the character is
//! unconscious, at `dead_at` x the maximum dead. Treatments (a bandage, a medkit, a lab effect in a
//! syringe) reduce afflictions by a table, scaled by the treater's skill, with a success roll and
//! a chance of poisoning when there is little to treat. Respawn clears the body and names the
//! share of the skills lost.
//!
//! Taken from Barotrauma: vitality as maximum minus the afflictions' banded vitality loss,
//! unconscious at zero and dead at minus the maximum, the burn, bleeding and oxygen-low bands and
//! rates, oxygen falling in bad air and while unconscious, treatments as a per-item table that
//! spreads a reduction over the matching afflictions, and the skill-based success roll.
//!
//! - `HealthTuning`: everything from `content/gameplay/health/health.json` (strict loader).
//! - `Health`: `add`, `step(dt, Env)`, `treat`, `mobility`, `respawn`; `HealthEvent`s.
//! - `Patient`: the `production_core::Target` for a syringe's dose.
use std::collections::BTreeMap;

use content_core::{parse_strict, File};
use production_core::devices::{Dose, Target};
use production_core::skill::Dice;
use serde::{Deserialize, Serialize};

pub const HEALTH_PATH: &str = "health/health.json";

/// A body part for limb afflictions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Part {
    Head,
    Torso,
    Arms,
    Legs,
}

impl Part {
    pub const ALL: [Part; 4] = [Part::Head, Part::Torso, Part::Arms, Part::Legs];
    pub fn name(self) -> &'static str {
        match self {
            Part::Head => "head",
            Part::Torso => "torso",
            Part::Arms => "arms",
            Part::Legs => "legs",
        }
    }
}

fn yes() -> bool {
    true
}
fn one() -> f64 {
    1.0
}

/// One strength band of an affliction: from the previous band's `to` (0 for the first) to `to`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Band {
    pub to: f64,
    /// Vitality lost at the band's start and end, as a share of the maximum.
    pub vitality: [f64; 2],
    /// The strength's own change per second (negative heals).
    #[serde(default)]
    pub change_per_s: f64,
    /// Walking speed factor.
    #[serde(default = "one")]
    pub walk: f64,
    #[serde(default = "yes")]
    pub sprint: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AfflictionKind {
    pub id: String,
    pub max: f64,
    /// Sits on a body part (else on the whole body).
    pub limb: bool,
    pub bands: Vec<Band>,
}

impl AfflictionKind {
    /// The band a strength falls in, with its start.
    fn band(&self, s: f64) -> (f64, &Band) {
        let mut from = 0.0;
        for b in &self.bands {
            if s <= b.to {
                return (from, b);
            }
            from = b.to;
        }
        let last = self.bands.last().expect("checked: at least one band");
        (self.bands.len().checked_sub(2).map(|i| self.bands[i].to).unwrap_or(0.0), last)
    }

    /// Vitality lost at strength `s` (share of the maximum).
    pub fn vitality_share(&self, s: f64) -> f64 {
        let (from, b) = self.band(s);
        let t = if b.to > from { ((s - from) / (b.to - from)).clamp(0.0, 1.0) } else { 1.0 };
        b.vitality[0] + (b.vitality[1] - b.vitality[0]) * t
    }
}

/// What a treatment reduces per use.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Treatment {
    /// An item kind (`bandage`) or a lab effect (`cooling`).
    pub id: String,
    /// At this skill it always works; each point below loses 1 % chance.
    pub required_skill: f64,
    /// A failure multiplies the reduction by this.
    pub fail_factor: f64,
    /// Affliction id to strength removed per use.
    pub reduce: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OxygenTuning {
    /// Air below this fraction is bad.
    pub low_below: f64,
    /// Oxygen low grows by this per second in bad air (conscious).
    pub fall_per_s: f64,
    /// And falls by this in good air.
    pub rise_per_s: f64,
    /// Unconscious, it grows by this whatever the air.
    pub unconscious_per_s: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TreatTuning {
    /// Reduction factor at skill 0 and at skill 100.
    pub skill_low: f64,
    pub skill_high: f64,
    /// Chance of poisoning when everything is left over (scaled by the share left over).
    pub overdose_chance: f64,
    /// Poison per unit of reduction left over.
    pub overdose_poison: f64,
    /// Strength of a status affliction from a full-strength dose.
    pub status_dose: f64,
}

/// Every health number, from `health/health.json`. TODO(initiator): all values.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HealthTuning {
    pub max_vitality: f64,
    /// Dead at this share of the maximum (negative).
    pub dead_at: f64,
    pub oxygen: OxygenTuning,
    pub treat: TreatTuning,
    /// Share of every skill lost by a respawn.
    pub respawn_skill_loss: f64,
    pub afflictions: Vec<AfflictionKind>,
    pub treatments: Vec<Treatment>,
}

impl HealthTuning {
    /// Loads and checks `health/health.json` from `files`. Errors name the file and the field.
    pub fn load(files: &[File]) -> Result<HealthTuning, Vec<String>> {
        let p = HEALTH_PATH;
        let Some(f) = files.iter().find(|f| f.path == p) else { return Err(vec![format!("{p}: missing")]) };
        let t = parse_strict::<HealthTuning>(p, &f.text).map_err(|m| vec![m])?;
        let mut e = Vec::new();
        t.check(&mut e);
        if e.is_empty() { Ok(t) } else { Err(e) }
    }

    fn check(&self, e: &mut Vec<String>) {
        let p = HEALTH_PATH;
        let mut need = |ok: bool, field: String, rule: &str| {
            if !ok {
                e.push(format!("{p}: {field}: {rule}"));
            }
        };
        need(self.max_vitality > 0.0, "max_vitality".into(), "must be > 0");
        need(self.dead_at < 0.0, "dead_at".into(), "must be below 0");
        need((0.0..=1.0).contains(&self.oxygen.low_below), "oxygen.low_below".into(), "must be in 0..1");
        need(self.treat.skill_low > 0.0 && self.treat.skill_high >= self.treat.skill_low, "treat.skill_high".into(), "must be at least skill_low > 0");
        need((0.0..=1.0).contains(&self.treat.overdose_chance), "treat.overdose_chance".into(), "must be in 0..1");
        need((0.0..=1.0).contains(&self.respawn_skill_loss), "respawn_skill_loss".into(), "must be in 0..1");
        let mut seen = std::collections::HashSet::new();
        for (i, a) in self.afflictions.iter().enumerate() {
            need(seen.insert(a.id.clone()), format!("afflictions[{i}].id"), "used twice");
            need(a.max > 0.0, format!("afflictions[{i}].max"), "must be > 0");
            need(!a.bands.is_empty(), format!("afflictions[{i}].bands"), "needs a band");
            let mut from = 0.0;
            for (j, b) in a.bands.iter().enumerate() {
                need(b.to > from, format!("afflictions[{i}].bands[{j}].to"), "must be above the band before");
                need(b.walk > 0.0 && b.walk <= 1.0, format!("afflictions[{i}].bands[{j}].walk"), "must be in (0, 1]");
                from = b.to;
            }
            need((from - a.max).abs() < 1e-9, format!("afflictions[{i}].bands"), "the last band must end at max");
        }
        for (i, t) in self.treatments.iter().enumerate() {
            need((0.0..=1.0).contains(&t.fail_factor), format!("treatments[{i}].fail_factor"), "must be in 0..1");
            for (k, v) in &t.reduce {
                need(self.afflictions.iter().any(|a| &a.id == k), format!("treatments[{i}].reduce"), &format!("unknown affliction '{k}'"));
                need(*v > 0.0, format!("treatments[{i}].reduce.{k}"), "must be > 0");
            }
        }
        for id in ["oxygen_low", "poison"] {
            need(self.afflictions.iter().any(|a| a.id == id), "afflictions".into(), &format!("needs '{id}'"));
        }
    }

    pub fn affliction(&self, id: &str) -> Option<&AfflictionKind> {
        self.afflictions.iter().find(|a| a.id == id)
    }

    pub fn treatment(&self, id: &str) -> Option<&Treatment> {
        self.treatments.iter().find(|t| t.id == id)
    }

    /// The reduction factor of a treater's skill (0..100).
    pub fn skill_factor(&self, skill: f64) -> f64 {
        let k = (skill / 100.0).clamp(0.0, 1.0);
        self.treat.skill_low + (self.treat.skill_high - self.treat.skill_low) * k
    }
}

/// One affliction on a character.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affliction {
    pub id: String,
    pub part: Option<Part>,
    pub strength: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Life {
    Conscious,
    Unconscious,
    Dead,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HealthEvent {
    Unconscious,
    Woke,
    /// The affliction that took the most vitality.
    Died { cause: String },
    /// After a respawn: the share of every skill lost.
    Respawn { skills_lost: f64 },
}

/// The surroundings for one step: the air's oxygen fraction (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Env {
    pub oxygen: f64,
}

/// How the body may move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mobility {
    pub walk: f64,
    pub sprint: bool,
    pub conscious: bool,
}

/// Who treats and how strong the medicine is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Treater {
    pub skill: f64,
    /// 0..1, from the medicine's quality (1 for first aid).
    pub potency: f64,
    /// Already rolled (a syringe rolls its own), or None to roll here.
    pub success: Option<bool>,
}

/// What a treatment did.
#[derive(Clone, Debug, PartialEq)]
pub struct TreatResult {
    pub success: bool,
    /// Strength removed in total.
    pub reduced: f64,
    /// Poison added by an overdose.
    pub overdose: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Health {
    afflictions: Vec<Affliction>,
    vitality: f64,
    max: f64,
    life: Life,
    dice: Dice,
}

impl Health {
    /// A healthy character; `seed` for its rolls.
    pub fn new(t: &HealthTuning, seed: u64) -> Health {
        Health { afflictions: Vec::new(), vitality: t.max_vitality, max: t.max_vitality, life: Life::Conscious, dice: Dice::new(seed) }
    }

    pub fn afflictions(&self) -> &[Affliction] {
        &self.afflictions
    }
    pub fn vitality(&self) -> f64 {
        self.vitality
    }
    pub fn max_vitality(&self) -> f64 {
        self.max
    }
    pub fn state(&self) -> Life {
        self.life
    }

    /// Total strength of an affliction over all parts.
    pub fn strength(&self, id: &str) -> f64 {
        self.afflictions.iter().filter(|a| a.id == id).fold(0.0, |s, a| s + a.strength)
    }

    pub fn strength_on(&self, id: &str, part: Part) -> f64 {
        self.afflictions.iter().filter(|a| a.id == id && a.part == Some(part)).fold(0.0, |s, a| s + a.strength)
    }

    /// Adds strength to an affliction (a limb one without a part goes to the torso). False for an
    /// unknown id. The dead take nothing.
    pub fn add(&mut self, t: &HealthTuning, id: &str, part: Option<Part>, amount: f64) -> bool {
        let Some(k) = t.affliction(id) else { return false };
        if self.life == Life::Dead || amount <= 0.0 {
            return true;
        }
        let part = if k.limb { Some(part.unwrap_or(Part::Torso)) } else { None };
        match self.afflictions.iter_mut().find(|a| a.id == id && a.part == part) {
            Some(a) => a.strength = (a.strength + amount).min(k.max),
            None => self.afflictions.push(Affliction { id: id.into(), part, strength: amount.min(k.max) }),
        }
        self.recalc(t);
        true
    }

    fn recalc(&mut self, t: &HealthTuning) {
        self.afflictions.retain(|a| a.strength > 0.0);
        let lost: f64 = self.afflictions.iter().filter_map(|a| t.affliction(&a.id).map(|k| k.vitality_share(a.strength))).sum();
        self.vitality = self.max - lost * self.max;
    }

    /// The affliction that takes the most vitality.
    pub fn worst(&self, t: &HealthTuning) -> Option<&Affliction> {
        self.afflictions.iter().max_by(|a, b| {
            let v = |x: &Affliction| t.affliction(&x.id).map(|k| k.vitality_share(x.strength)).unwrap_or(0.0);
            v(a).total_cmp(&v(b))
        })
    }

    /// One step: oxygen from the air, each band's own change, then vitality and the state.
    pub fn step(&mut self, t: &HealthTuning, dt: f64, env: Env) -> Vec<HealthEvent> {
        if self.life == Life::Dead {
            return Vec::new();
        }
        let o = &t.oxygen;
        let air = match self.life {
            Life::Unconscious => o.unconscious_per_s,
            _ if env.oxygen < o.low_below => o.fall_per_s,
            _ => -o.rise_per_s,
        };
        if air > 0.0 {
            self.add(t, "oxygen_low", None, air * dt);
        } else if let Some(a) = self.afflictions.iter_mut().find(|a| a.id == "oxygen_low") {
            a.strength = (a.strength + air * dt).max(0.0);
        }
        for a in &mut self.afflictions {
            if let Some(k) = t.affliction(&a.id) {
                let (_, b) = k.band(a.strength);
                a.strength = (a.strength + b.change_per_s * dt).clamp(0.0, k.max);
            }
        }
        self.recalc(t);
        self.update_life(t)
    }

    fn update_life(&mut self, t: &HealthTuning) -> Vec<HealthEvent> {
        let mut ev = Vec::new();
        let dead_line = t.dead_at * self.max;
        if self.vitality <= 0.0 && self.life == Life::Conscious {
            self.life = Life::Unconscious;
            ev.push(HealthEvent::Unconscious);
        }
        if self.vitality <= dead_line + 1e-9 && self.life != Life::Dead {
            self.life = Life::Dead;
            let cause = self.worst(t).map(|a| a.id.clone()).unwrap_or_default();
            ev.push(HealthEvent::Died { cause });
        } else if self.vitality > 0.0 && self.life == Life::Unconscious {
            self.life = Life::Conscious;
            ev.push(HealthEvent::Woke);
        }
        ev
    }

    /// How the body may move: the slowest band's walk factor, no sprint if any band forbids it.
    pub fn mobility(&self, t: &HealthTuning) -> Mobility {
        let mut m = Mobility { walk: 1.0, sprint: true, conscious: self.life == Life::Conscious && self.vitality > 0.0 };
        for a in &self.afflictions {
            if let Some(k) = t.affliction(&a.id) {
                let (_, b) = k.band(a.strength);
                m.walk = m.walk.min(b.walk);
                m.sprint &= b.sprint;
            }
        }
        m
    }

    /// Applies a treatment: each listed affliction loses its amount x skill factor x potency (x
    /// the failure factor on a failed roll), spread evenly over the parts that have it, smallest
    /// first, the surplus passed on. What is left over may poison. None for an unknown treatment.
    pub fn treat(&mut self, t: &HealthTuning, id: &str, who: Treater) -> Option<TreatResult> {
        let tr = t.treatment(id)?.clone();
        if self.life == Life::Dead {
            return Some(TreatResult { success: false, reduced: 0.0, overdose: None });
        }
        let success = who.success.unwrap_or_else(|| {
            let p = (1.0 + (who.skill - tr.required_skill) / 100.0).clamp(0.0, 1.0);
            self.dice.chance(p)
        });
        let factor = t.skill_factor(who.skill) * who.potency.clamp(0.0, 1.0) * if success { 1.0 } else { tr.fail_factor };
        let (mut total, mut reduced) = (0.0, 0.0);
        for (aff, amount) in &tr.reduce {
            let mut left = amount * factor;
            total += left;
            let mut idx: Vec<usize> = (0..self.afflictions.len()).filter(|&i| &self.afflictions[i].id == aff).collect();
            idx.sort_by(|&a, &b| self.afflictions[a].strength.total_cmp(&self.afflictions[b].strength));
            let n = idx.len();
            for (k, i) in idx.into_iter().enumerate() {
                let share = left / (n - k) as f64;
                let a = &mut self.afflictions[i];
                let r = share.min(a.strength);
                a.strength -= r;
                left -= r;
                reduced += r;
            }
        }
        let excess = total - reduced;
        let mut overdose = None;
        if excess > 1e-9 && total > 0.0 && self.dice.chance(t.treat.overdose_chance * excess / total) {
            let p = excess * t.treat.overdose_poison;
            self.add(t, "poison", None, p);
            overdose = Some(p);
        }
        self.recalc(t);
        self.update_life(t);
        Some(TreatResult { success, reduced, overdose })
    }

    /// Back to full health, conscious; names the share of the skills lost.
    pub fn respawn(&mut self, t: &HealthTuning) -> HealthEvent {
        self.afflictions.clear();
        self.life = Life::Conscious;
        self.recalc(t);
        HealthEvent::Respawn { skills_lost: t.respawn_skill_loss }
    }
}

/// A character receiving a syringe's dose: each effect that is a treatment treats (the dose's
/// strength as potency, the injector's success already in it), each effect that is an
/// affliction adds it as a status. Other effects do nothing.
pub struct Patient<'a> {
    pub health: &'a mut Health,
    pub tuning: &'a HealthTuning,
    /// The skill of whoever injects.
    pub treater_skill: f64,
    /// The last treatment a dose gave, for the readout.
    pub last: Option<TreatResult>,
    /// The statuses the last dose added.
    pub statuses: Vec<String>,
}

impl<'a> Patient<'a> {
    pub fn new(health: &'a mut Health, tuning: &'a HealthTuning, treater_skill: f64) -> Patient<'a> {
        Patient { health, tuning, treater_skill, last: None, statuses: Vec::new() }
    }
}

impl Target for Patient<'_> {
    fn receive(&mut self, dose: &Dose) {
        for e in &dose.effects {
            let id = e.as_str();
            if self.tuning.treatment(id).is_some() {
                let r = self.health.treat(self.tuning, id, Treater { skill: self.treater_skill, potency: dose.strength, success: Some(true) });
                self.last = r.map(|r| TreatResult { success: dose.success, ..r });
            } else if self.tuning.affliction(id).is_some() {
                self.health.add(self.tuning, id, None, self.tuning.treat.status_dose * dose.strength);
                self.statuses.push(id.to_string());
            }
        }
    }
}
