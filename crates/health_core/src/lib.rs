//! health_core: a character's health (#295, from the dev-area round #254, #287), without engine
//! types, in f64.
//!
//! Afflictions (burn, oxygen low, radiation) each have a strength; its strength band sets how much
//! vitality it takes and how fast it heals by itself. Vitality is the maximum minus what the
//! afflictions take. At 0 the character is unconscious, at `dead_at` x the maximum dead. A
//! treatment (a bandage, a deglow) reduces one affliction per use, scaled by the treater's skill,
//! with a success roll. Respawn clears the body.
//!
//! Taken from Barotrauma: vitality as the maximum minus the afflictions' banded vitality loss,
//! unconscious at zero and dead below zero, the burn and oxygen-low bands, oxygen falling in bad
//! air and while unconscious, and the skill-based success roll.
//!
//! - `HealthTuning`: everything from `content/gameplay/health/health.json` (strict loader).
//! - `Health`: `add`, `step`, `treat`, `respawn`; `HealthEvent`s out of `step` and `treat`.
//! - `Rng`: a seeded xorshift64 for the treatment rolls (no crate dependency).
use std::collections::BTreeMap;

use content_core::{parse_strict, File};
use serde::{Deserialize, Serialize};

pub const HEALTH_PATH: &str = "health/health.json";

/// One strength band of an affliction: from the previous band's `to` (0 for the first) to `to`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Band {
    pub to: f64,
    /// Vitality lost at the band's start and end, as a share of the maximum.
    pub vitality: [f64; 2],
    /// The strength's own change per second (negative heals).
    pub change_per_s: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AfflictionKind {
    pub id: String,
    pub max: f64,
    pub bands: Vec<Band>,
}

impl AfflictionKind {
    /// The band a strength falls in, with the strength where that band starts.
    pub fn band(&self, s: f64) -> (f64, &Band) {
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

    /// The band index a strength falls in (0 is the mildest).
    pub fn band_index(&self, s: f64) -> usize {
        self.bands.iter().position(|b| s <= b.to).unwrap_or(self.bands.len().saturating_sub(1))
    }
}

/// What a treatment reduces per use.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Treatment {
    /// An item kind (`bandage`, `deglow`).
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
}

/// Every health number, from `health/health.json`. TODO(initiator): all values are starting points.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HealthTuning {
    pub max_vitality: f64,
    /// Dead at this share of the maximum (negative).
    pub dead_at: f64,
    /// Seconds a dead player waits before the respawn.
    pub respawn_s: f64,
    pub oxygen: OxygenTuning,
    pub treat: TreatTuning,
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
        need(self.respawn_s >= 0.0, "respawn_s".into(), "must be >= 0");
        need((0.0..=1.0).contains(&self.oxygen.low_below), "oxygen.low_below".into(), "must be in 0..1");
        need(self.treat.skill_low > 0.0 && self.treat.skill_high >= self.treat.skill_low, "treat.skill_high".into(), "must be at least skill_low > 0");
        let mut seen = std::collections::HashSet::new();
        for (i, a) in self.afflictions.iter().enumerate() {
            need(seen.insert(a.id.clone()), format!("afflictions[{i}].id"), "used twice");
            need(a.max > 0.0, format!("afflictions[{i}].max"), "must be > 0");
            need(!a.bands.is_empty(), format!("afflictions[{i}].bands"), "needs a band");
            let mut from = 0.0;
            for (j, b) in a.bands.iter().enumerate() {
                need(b.to > from, format!("afflictions[{i}].bands[{j}].to"), "must be above the band before");
                need(b.vitality[0] <= b.vitality[1], format!("afflictions[{i}].bands[{j}].vitality"), "start must not be above end");
                from = b.to;
            }
            need((from - a.max).abs() < 1e-9, format!("afflictions[{i}].bands"), "the last band must end at max");
        }
        for (i, t) in self.treatments.iter().enumerate() {
            need((0.0..=1.0).contains(&t.fail_factor), format!("treatments[{i}].fail_factor"), "must be in 0..1");
            for (k, v) in &t.reduce {
                need(self.affliction(k).is_some(), format!("treatments[{i}].reduce"), &format!("unknown affliction '{k}'"));
                need(*v > 0.0, format!("treatments[{i}].reduce.{k}"), "must be > 0");
            }
        }
        need(self.affliction("oxygen_low").is_some(), "afflictions".into(), "needs 'oxygen_low'");
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

/// A seeded xorshift64: enough for the treatment rolls, no crate needed.
#[derive(Clone, Debug, PartialEq)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }

    /// The next number in 0..1.
    pub fn next_f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }

    /// True with probability `p` (0..1).
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }
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
}

/// The surroundings for one step: the air's oxygen fraction (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Env {
    pub oxygen: f64,
}

/// What a treatment did.
#[derive(Clone, Debug, PartialEq)]
pub struct TreatResult {
    pub success: bool,
    /// Strength removed in total.
    pub reduced: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Health {
    /// Affliction id to strength; only those above zero are kept.
    afflictions: BTreeMap<String, f64>,
    vitality: f64,
    max: f64,
    life: Life,
    rng: Rng,
}

impl Health {
    /// A healthy character; `seed` for its rolls.
    pub fn new(t: &HealthTuning, seed: u64) -> Health {
        Health { afflictions: BTreeMap::new(), vitality: t.max_vitality, max: t.max_vitality, life: Life::Conscious, rng: Rng::new(seed) }
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

    /// Strength of an affliction (0 if none).
    pub fn strength(&self, id: &str) -> f64 {
        self.afflictions.get(id).copied().unwrap_or(0.0)
    }

    /// Adds strength to an affliction, capped at its max. False for an unknown id. The dead take
    /// nothing.
    pub fn add(&mut self, t: &HealthTuning, id: &str, amount: f64) -> bool {
        let Some(k) = t.affliction(id) else { return false };
        if self.life == Life::Dead || amount <= 0.0 {
            return true;
        }
        let s = self.afflictions.entry(id.to_string()).or_insert(0.0);
        *s = (*s + amount).min(k.max);
        self.recalc(t);
        true
    }

    fn recalc(&mut self, t: &HealthTuning) {
        self.afflictions.retain(|_, s| *s > 0.0);
        let lost: f64 = self.afflictions.iter().filter_map(|(id, s)| t.affliction(id).map(|k| k.vitality_share(*s))).sum();
        self.vitality = self.max - lost * self.max;
    }

    /// The affliction that takes the most vitality.
    pub fn worst(&self, t: &HealthTuning) -> Option<&str> {
        self.afflictions
            .iter()
            .max_by(|a, b| {
                let v = |id: &str, s: f64| t.affliction(id).map(|k| k.vitality_share(s)).unwrap_or(0.0);
                v(a.0, *a.1).total_cmp(&v(b.0, *b.1))
            })
            .map(|(id, _)| id.as_str())
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
            self.add(t, "oxygen_low", air * dt);
        } else if let Some(s) = self.afflictions.get_mut("oxygen_low") {
            *s = (*s + air * dt).max(0.0);
        }
        for (id, s) in self.afflictions.iter_mut() {
            if let Some(k) = t.affliction(id) {
                let (_, b) = k.band(*s);
                *s = (*s + b.change_per_s * dt).clamp(0.0, k.max);
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
            let cause = self.worst(t).unwrap_or_default().to_string();
            ev.push(HealthEvent::Died { cause });
        } else if self.vitality > 0.0 && self.life == Life::Unconscious {
            self.life = Life::Conscious;
            ev.push(HealthEvent::Woke);
        }
        ev
    }

    /// Applies a treatment by its id: each listed affliction loses its amount x skill factor (x
    /// the failure factor on a failed roll). None for an unknown treatment. The dead take nothing.
    pub fn treat(&mut self, t: &HealthTuning, id: &str, skill: f64) -> Option<TreatResult> {
        let tr = t.treatment(id)?.clone();
        if self.life == Life::Dead {
            return Some(TreatResult { success: false, reduced: 0.0 });
        }
        let p = (1.0 + (skill - tr.required_skill) / 100.0).clamp(0.0, 1.0);
        let success = self.rng.chance(p);
        let factor = t.skill_factor(skill) * if success { 1.0 } else { tr.fail_factor };
        let mut reduced = 0.0;
        for (aff, amount) in &tr.reduce {
            if let Some(s) = self.afflictions.get_mut(aff) {
                let r = (amount * factor).min(*s);
                *s -= r;
                reduced += r;
            }
        }
        self.recalc(t);
        self.update_life(t);
        Some(TreatResult { success, reduced })
    }

    /// Back to full health, conscious.
    pub fn respawn(&mut self, t: &HealthTuning) {
        self.afflictions.clear();
        self.life = Life::Conscious;
        self.recalc(t);
    }
}

/// Where a ray from `origin` along `dir` (unit) first meets a capsule that stands on `feet` and
/// reaches `height` up `up` (unit), with `radius`: the distance along the ray, within `reach`.
/// Sampled every 5 cm (a rough hit test for the aim; the glue passes plain numbers).
pub fn capsule_hit(origin: [f64; 3], dir: [f64; 3], reach: f64, feet: [f64; 3], up: [f64; 3], height: f64, radius: f64) -> Option<f64> {
    const STEP: f64 = 0.05;
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let steps = (reach / STEP).ceil().max(0.0) as usize;
    for i in 0..=steps {
        let t = (i as f64 * STEP).min(reach);
        let p = [origin[0] + dir[0] * t, origin[1] + dir[1] * t, origin[2] + dir[2] * t];
        let rel = [p[0] - feet[0], p[1] - feet[1], p[2] - feet[2]];
        let s = dot(rel, up).clamp(0.0, height);
        let c = [feet[0] + up[0] * s, feet[1] + up[1] * s, feet[2] + up[2] * s];
        let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        if dot(d, d) <= radius * radius {
            return Some(t);
        }
    }
    None
}
