//! The enemy ship of the ambush (r41, #255 basics, the loop of #256). An abstract ship: no model and
//! no flight, because the dev area has no space flight. It has a shield and a hull, and a turret that
//! fires on a charge timer. Each shot is a hit from outside on the engine room's wall
//! (`EngineRoom::outside_hit`), the light or the heavy preset of `reactor.json` hits, at a seeded
//! random point of the wall. Our GN-01 shot (`EngineRoom::fire_weapon`) takes the enemy's shield
//! first, then its hull. The encounter ends Destroyed at hull 0, or Escaped when our jump goes ACTIVE.
//!
//! Sources:
//! - Shield and hull: PULSAR's enemy shields are the same shield as ours (the manual's shield
//!   generator, `reactor_core::Shield`, r40, sources in shield.rs), so the enemy takes `Shield` with its
//!   own tuning. Its hull is one number: a shot's damage past the shield takes it down.
//! - The fire cadence: Barotrauma's turret counts its `reload` down (`Items/Components/Turret.cs:31`
//!   field, `:457` `reload -= deltaTime`, `:185` the Reload property) and fires when it is done. Ours
//!   is a timer: the first shot after `first_shot_s`, then one every `fire_every_s`.
//! - The shot's kind: the light or the heavy hit by `light_share` (our number, the turret's two kinds of
//!   shot in the hits block).
//! - Our shot's damage (`shot_damage`) is our number: the GN-01's coilgun, 120 points.
//! Units: points, s. TODO(initiator): every value in reactor.json `enemy`.
#[cfg(test)]
mod tests;

use gameplay_core::rng::Rng;
use serde::{Deserialize, Serialize};

use crate::hull::{Hit, HitTuning, HitsTuning, Hull, SECTION_CENTRES};
use crate::shield::{Shield, ShieldTuning};

/// `enemy` in reactor.json. Start values, all TODO(initiator): see the module doc for their sources.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnemyTuning {
    /// The enemy's shield (the same model as ours, its own numbers).
    pub shield: ShieldTuning,
    /// The enemy's hull, points: a shot past the shield takes it down.
    pub hull: f64,
    /// The damage of our GN-01 shot, points.
    pub shot_damage: f64,
    /// Seconds between the enemy's shots.
    pub fire_every_s: f64,
    /// The share of its shots that are light (the rest are heavy), 0..1.
    pub light_share: f64,
    /// Seconds from the start of an encounter to the enemy's first shot.
    pub first_shot_s: f64,
}

impl Default for EnemyTuning {
    fn default() -> EnemyTuning {
        EnemyTuning { shield: ShieldTuning { max: 600.0, charge_rate_max: 20.0, ..ShieldTuning::default() }, hull: 1000.0, shot_damage: 120.0, fire_every_s: 8.0, light_share: 0.7, first_shot_s: 10.0 }
    }
}

/// How an encounter ends: None while it runs.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Outcome {
    #[default]
    None,
    /// The enemy's hull reached 0.
    Destroyed,
    /// Our jump went ACTIVE.
    Escaped,
}

/// The enemy ship: its shield, its hull and the time to its next shot.
#[derive(Clone, Debug)]
pub struct Enemy {
    shield: Shield,
    hull: f64,
    hull_max: f64,
    /// Seconds until the next shot.
    fire_in: f64,
    /// False once the encounter has ended (no more shots, no more shield regen).
    active: bool,
}

/// One encounter: the enemy, how it ends, and the shots it has made.
#[derive(Clone, Debug)]
pub struct Encounter {
    enemy: Enemy,
    outcome: Outcome,
    every_s: f64,
    light_share: f64,
    light: HitTuning,
    heavy: HitTuning,
    shot_damage: f64,
    shots: u32,
    /// The wall section (index into `SECTION_CENTRES`) of the last shot.
    last_section: usize,
}

impl Encounter {
    /// A new encounter: the enemy at full shield and hull, its first shot `first_shot_s` away.
    pub fn new(t: &EnemyTuning, hits: &HitsTuning) -> Encounter {
        let enemy = Enemy { shield: Shield::new(t.shield), hull: t.hull, hull_max: t.hull, fire_in: t.first_shot_s, active: true };
        Encounter {
            enemy,
            outcome: Outcome::None,
            every_s: t.fire_every_s,
            light_share: t.light_share.clamp(0.0, 1.0),
            light: hits.light,
            heavy: hits.heavy,
            shot_damage: t.shot_damage,
            shots: 0,
            last_section: 0,
        }
    }

    pub fn outcome(&self) -> Outcome {
        self.outcome
    }

    /// The enemy's shield, 0..1.
    pub fn shield_fraction(&self) -> f64 {
        self.enemy.shield.fraction()
    }

    /// The enemy's hull, 0..1.
    pub fn hull_fraction(&self) -> f64 {
        self.enemy.hull / self.enemy.hull_max.max(1e-9)
    }

    /// The enemy's shots so far (the glue counts the booms by it).
    pub fn shots(&self) -> u32 {
        self.shots
    }

    /// The wall section the last shot hit (0 before the first shot).
    pub fn last_section(&self) -> usize {
        self.last_section
    }

    /// Advances `dt` seconds. The shield regens; when the shot timer runs out the enemy fires one shot:
    /// the light or the heavy preset at a point of the wall, drawn from `rng` (a seed gives the same
    /// shots). At most one shot a step. None while the encounter has ended.
    pub fn step(&mut self, dt: f64, rng: &mut Rng) -> Option<Hit> {
        if !self.enemy.active {
            return None;
        }
        self.enemy.shield.step(dt, 1.0);
        self.enemy.fire_in -= dt;
        if self.enemy.fire_in > 0.0 {
            return None;
        }
        self.enemy.fire_in += self.every_s;
        let preset = if rng.unit() < self.light_share { self.light } else { self.heavy };
        let at = Hull::wall_point(rng.unit());
        self.shots += 1;
        self.last_section = nearest_section(at);
        Some(preset.at(at))
    }

    /// Our shot: the shield takes it first, what passes takes the hull. Hull 0 is Destroyed. Nothing
    /// happens once the encounter has ended.
    pub fn take_shot(&mut self, dmg: f64) {
        if self.outcome != Outcome::None {
            return;
        }
        let through = self.enemy.shield.absorb(dmg);
        self.enemy.hull = (self.enemy.hull - through).max(0.0);
        if self.enemy.hull <= 0.0 {
            self.end(Outcome::Destroyed);
        }
    }

    /// Our jump went ACTIVE: the enemy is left behind. Nothing happens once the encounter has ended.
    pub fn escape(&mut self) {
        if self.outcome == Outcome::None {
            self.end(Outcome::Escaped);
        }
    }

    fn end(&mut self, how: Outcome) {
        self.outcome = how;
        self.enemy.active = false;
    }

    /// Our GN-01 shot's damage (`EnemyTuning::shot_damage`), as the room passes it.
    pub fn shot_damage(&self) -> f64 {
        self.shot_damage
    }
}

/// The section whose centre is nearest `at` (ties to the lower index).
fn nearest_section(at: [f64; 2]) -> usize {
    let d = |c: [f64; 2]| (c[0] - at[0]).powi(2) + (c[1] - at[1]).powi(2);
    (0..SECTION_CENTRES.len()).min_by(|&a, &b| d(SECTION_CENTRES[a]).total_cmp(&d(SECTION_CENTRES[b]))).unwrap_or(0)
}
