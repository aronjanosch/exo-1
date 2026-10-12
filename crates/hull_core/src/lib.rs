//! hull_core: what a hit does to the ship, without engine types, in f64 (#286, part of #174 and
//! #255). A hit is data (`Hit`): its damage type and strength say how much it hurts the hull
//! section it lands on, the ship systems in that room, and how likely it starts a fire. A damaged
//! section opens a breach (a `fire_core` leak to space) whose size grows with the damage; air
//! leaves through it and pulls walkers and loose things towards the hole. Holding a repair tool at
//! the section patches it (time by skill, `device_core`). The oxygen generator puts air back.
//!
//! Taken from Barotrauma: wall sections with a damage value, a leak that opens past a tenth of the
//! section's health and grows in two steps (a small opening up to seven tenths, a large one beyond),
//! a flow force from a gap that falls with distance, repair that closes the gap again, and an oxygen
//! generator whose output scales with its supply and with its condition squared. Taken from PULSAR:
//! damage types (energy, physical, collision) as data. Our own code; the damage table, the hit
//! strengths, the volley and the air model (one gas, leak flow with the room's pressure) are ours
//! (`research/barotrauma-mech-damage-repair.md`, `barotrauma-mech-water.md` in the concept repo).
//!
//! All values are starting points for the playtest (`TODO(initiator)`).
use device_core::{Condition, DeviceTuning, RepairOutcome, Tool};
use fire_core::{Deck, FireId, LeakId, RoomId};
use gameplay_core::rng::Rng;
use glam::DVec3;

#[cfg(test)]
mod tests;

// ---------- hits as data ----------

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DamageType {
    /// Beams and plasma: hard on systems, starts fires, light on the hull.
    Energy,
    /// Shells and debris: holes in the hull.
    Physical,
    /// Ramming and crashes: the hull takes it, systems shake, few fires.
    Collision,
}

impl DamageType {
    pub const ALL: [DamageType; 3] = [DamageType::Energy, DamageType::Physical, DamageType::Collision];

    pub fn name(self) -> &'static str {
        match self {
            DamageType::Energy => "energy",
            DamageType::Physical => "physical",
            DamageType::Collision => "collision",
        }
    }

    /// How this type scales a hit's hull damage, system damage and fire chance.
    pub fn scale(self) -> TypeScale {
        // TODO(initiator): the damage table.
        match self {
            DamageType::Energy => TypeScale { hull: 0.6, system: 1.3, fire: 1.5 },
            DamageType::Physical => TypeScale { hull: 1.0, system: 0.8, fire: 0.6 },
            DamageType::Collision => TypeScale { hull: 1.3, system: 0.5, fire: 0.3 },
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TypeScale {
    pub hull: f64,
    pub system: f64,
    pub fire: f64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Strength {
    Light,
    Medium,
    Heavy,
}

impl Strength {
    pub const ALL: [Strength; 3] = [Strength::Light, Strength::Medium, Strength::Heavy];

    pub fn name(self) -> &'static str {
        match self {
            Strength::Light => "light",
            Strength::Medium => "medium",
            Strength::Heavy => "heavy",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SectionId(pub usize);

/// One hit on the hull, as data: what it is, how hard, where, and what it can cause. The damage
/// type's `scale` applies on top.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Hit {
    pub kind: DamageType,
    /// Hull damage before the type's scale (a section has 100).
    pub amount: f64,
    pub section: SectionId,
    /// Chance to start a fire, before the type's scale.
    pub fire_chance: f64,
    /// Area of the hole it can tear (m²); the opening grows with the section's damage.
    pub breach_size: f64,
    /// Condition every ship system in the room loses, before the type's scale.
    pub system_damage: f64,
}

impl Hit {
    /// The standard hit of a type and strength. TODO(initiator): the numbers.
    pub fn standard(kind: DamageType, strength: Strength, section: SectionId) -> Hit {
        let (amount, fire_chance, breach_size, system_damage) = match strength {
            Strength::Light => (15.0, 0.15, 0.5, 5.0),
            Strength::Medium => (35.0, 0.35, 1.0, 12.0),
            Strength::Heavy => (70.0, 0.6, 2.0, 25.0),
        };
        Hit { kind, amount, section, fire_chance, breach_size, system_damage }
    }
}

/// What a hit did. The glue shows each as a notice and applies `SystemDamage` to the ship systems
/// standing in the room.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum HitEffect {
    HullDamaged { section: SectionId, amount: f64 },
    /// The section's breach is open (`opened`: it was closed before this hit).
    Breach { section: SectionId, open: f64, opened: bool },
    Fire { room: RoomId, fire: FireId },
    SystemDamage { room: RoomId, amount: f64 },
}

// ---------- hull sections ----------

/// Hull values. TODO(initiator): starting values; the leak thresholds and openings follow the
/// original, the rest are guesses.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct HullTuning {
    /// Below this share of damage a section holds tight.
    pub leak_threshold: f64,
    /// From this share on the hole is large.
    pub big_gap_threshold: f64,
    /// Opening at the big-gap threshold.
    pub small_open: f64,
    /// Opening when the section is gone.
    pub large_open: f64,
    /// Size of a fire a hit starts (m).
    pub fire_size: f64,
    /// Pull towards a breach: acceleration (m/s²) per m³/s of outflow, at most `pull_max`, falling
    /// to nothing at `pull_reach` metres; below `pull_min_flow` (m³/s) there is no pull.
    pub pull_per_flow: f64,
    pub pull_max: f64,
    pub pull_reach: f64,
    pub pull_min_flow: f64,
    /// Seconds to patch a section from nothing to whole at skill 0 and 100 (welding never fails).
    pub patch_slow_s: f64,
    pub patch_fast_s: f64,
    /// Air one person breathes (m³/s).
    pub breath: f64,
}

impl Default for HullTuning {
    fn default() -> HullTuning {
        HullTuning {
            leak_threshold: 0.1,
            big_gap_threshold: 0.7,
            small_open: 0.35,
            large_open: 0.75,
            fire_size: 0.3,
            pull_per_flow: 1.5,
            pull_max: 3.0,
            pull_reach: 6.0,
            pull_min_flow: 1.0,
            patch_slow_s: 30.0,
            patch_fast_s: 10.0,
            breath: 0.01,
        }
    }
}

impl HullTuning {
    /// How far a section's hole is open at a damage share 0..1: shut below the leak threshold,
    /// up to `small_open` at the big-gap threshold, up to `large_open` when it is gone.
    pub fn openness(&self, ratio: f64) -> f64 {
        let inv = |a: f64, b: f64, x: f64| ((x - a) / (b - a)).clamp(0.0, 1.0);
        if ratio > self.big_gap_threshold {
            self.small_open + (self.large_open - self.small_open) * inv(self.big_gap_threshold, 1.0, ratio)
        } else if ratio > self.leak_threshold {
            self.small_open * inv(self.leak_threshold, self.big_gap_threshold, ratio)
        } else {
            0.0
        }
    }

    /// The condition values of a section: 100 is whole, it asks for repair while it leaks.
    pub fn section_device(&self) -> DeviceTuning {
        DeviceTuning {
            max: 100.0,
            repair_threshold: 100.0 * (1.0 - self.leak_threshold),
            broken_below: 0.0,
            factor_at_broken: 1.0,
            fix_slow_s: self.patch_slow_s,
            fix_fast_s: self.patch_fast_s,
            required_skill: 0.0,
            fail_chance: 0.0,
            fail_cost: 0.0,
            fail_lockout_s: 0.0,
            wear_rate: 0.0,
            wear_floor: 0.0,
            wear_delay_s: (0.0, 0.0),
            idle_wear: 0.0,
            overload_wear: 0.0,
        }
    }
}

/// A piece of hull: the wall of a room where hits land.
#[derive(Clone, Debug)]
pub struct Section {
    pub room: RoomId,
    /// Where the hole would be (ship space); fires start here too.
    pub pos: DVec3,
    /// 100 whole .. 0 gone.
    pub condition: Condition,
    /// The largest hole a hit tore here (m²); the leak's area.
    pub hole: f64,
    pub leak: LeakId,
}

impl Section {
    /// Damage share 0..1.
    pub fn damage(&self) -> f64 {
        1.0 - self.condition.fraction()
    }
}

/// The ship's hull: its sections and the hit rolls.
#[derive(Clone, Debug)]
pub struct Hull {
    tuning: HullTuning,
    sections: Vec<Section>,
    rng: Rng,
}

impl Hull {
    /// `seed` makes the fire rolls repeatable.
    pub fn new(tuning: HullTuning, seed: u64) -> Hull {
        Hull { tuning, sections: Vec::new(), rng: Rng::new(seed) }
    }

    pub fn tuning(&self) -> &HullTuning {
        &self.tuning
    }

    /// A whole section of `room` at `pos`; it owns a closed leak in the deck.
    pub fn add_section(&mut self, deck: &mut Deck, room: RoomId, pos: DVec3) -> SectionId {
        let leak = deck.add_leak(room, pos, 0.0, 0.0);
        self.sections.push(Section { room, pos, condition: Condition::new(self.tuning.section_device(), self.sections.len() as u64), hole: 0.0, leak });
        SectionId(self.sections.len() - 1)
    }

    pub fn section(&self, s: SectionId) -> &Section {
        &self.sections[s.0]
    }

    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// How far the section's hole is open, 0..1.
    pub fn openness(&self, s: SectionId) -> f64 {
        self.tuning.openness(self.sections[s.0].damage())
    }

    /// Whether air leaves through the section.
    pub fn breached(&self, s: SectionId) -> bool {
        self.openness(s) > 0.0 && self.sections[s.0].hole > 0.0
    }

    /// Lands a hit: hull damage, a breach if the section is past the leak threshold, a fire on a
    /// seeded roll, and system damage for the room.
    pub fn apply(&mut self, hit: &Hit, deck: &mut Deck) -> Vec<HitEffect> {
        let sc = hit.kind.scale();
        let mut out = Vec::new();
        let was_open = self.breached(hit.section);
        let amount = hit.amount * sc.hull;
        let s = &mut self.sections[hit.section.0];
        s.condition.damage(amount);
        s.hole = s.hole.max(hit.breach_size);
        let (room, pos) = (s.room, s.pos);
        out.push(HitEffect::HullDamaged { section: hit.section, amount });
        self.sync(hit.section, deck);
        if self.breached(hit.section) {
            out.push(HitEffect::Breach { section: hit.section, open: self.openness(hit.section), opened: !was_open });
        }
        if self.rng.unit() < (hit.fire_chance * sc.fire).min(1.0) {
            let fire = deck.ignite(room, pos, self.tuning.fire_size);
            out.push(HitEffect::Fire { room, fire });
        }
        if hit.system_damage > 0.0 {
            out.push(HitEffect::SystemDamage { room, amount: hit.system_damage * sc.system });
        }
        out
    }

    /// One step of patching the section from inside, `skill` 0..100 (mechanical). The hole closes
    /// once the section is back above the leak threshold; going on makes it whole.
    pub fn patch_step(&mut self, s: SectionId, dt: f64, skill: f64, tool: Tool, deck: &mut Deck) -> RepairOutcome {
        let out = self.sections[s.0].condition.repair_step(dt, skill, tool);
        if self.openness(s) <= 0.0 {
            self.sections[s.0].hole = 0.0;
        }
        self.sync(s, deck);
        out
    }

    pub fn stop_patch(&mut self, s: SectionId) {
        self.sections[s.0].condition.stop_repair();
    }

    /// Sets the section's leak from its damage and hole.
    fn sync(&self, s: SectionId, deck: &mut Deck) {
        let sec = &self.sections[s.0];
        deck.set_leak(sec.leak, sec.hole, self.openness(s));
    }

    /// The pull towards the open breaches of `room` at `at` (m/s², ship space), for walkers and
    /// loose things. Only a breach with a strong outflow pulls; it fades with distance.
    pub fn pull(&self, deck: &Deck, room: RoomId, at: DVec3) -> DVec3 {
        let t = &self.tuning;
        let mut sum = DVec3::ZERO;
        for s in self.sections.iter().filter(|s| s.room == room) {
            let flow = deck.leak(s.leak).flow;
            if flow < t.pull_min_flow {
                continue;
            }
            let d = s.pos - at;
            let dist = d.length();
            if dist < 1e-6 || dist >= t.pull_reach {
                continue;
            }
            sum += d / dist * (t.pull_per_flow * flow).min(t.pull_max) * (1.0 - dist / t.pull_reach);
        }
        sum.clamp_length_max(t.pull_max)
    }

    /// People breathing in a room for `dt` seconds.
    pub fn breathe(&self, deck: &mut Deck, room: RoomId, people: f64, dt: f64) {
        let r = deck.room_mut(room);
        r.oxygen = (r.oxygen - self.tuning.breath * people * dt).max(0.0);
    }
}

// ---------- oxygen generator ----------

/// The oxygen generator: makes air for the rooms its vents reach, with its supply and condition.
#[derive(Clone, Debug)]
pub struct OxygenGenerator {
    pub condition: Condition,
    /// m³/s at full supply and condition. TODO(initiator): a guess for our room sizes.
    pub rate: f64,
    /// m³/s of the last step.
    pub output: f64,
}

impl OxygenGenerator {
    pub fn new(rate: f64, tuning: DeviceTuning, seed: u64) -> OxygenGenerator {
        OxygenGenerator { condition: Condition::new(tuning, seed), rate, output: 0.0 }
    }

    /// One step: `supply` 0..1 is its group's factor from the grid. Output scales with supply and
    /// with condition squared (half condition gives a quarter), nothing when broken; it is shared
    /// by volume between `rooms`, and no room is filled past full. Returns m³ added.
    pub fn step(&mut self, dt: f64, supply: f64, deck: &mut Deck, rooms: &[RoomId]) -> f64 {
        let c = if self.condition.functioning() { self.condition.fraction() } else { 0.0 };
        let made = self.rate * supply.clamp(0.0, 1.0) * c * c * dt;
        let total: f64 = rooms.iter().map(|&r| deck.room(r).volume).sum();
        let mut added = 0.0;
        if total > 0.0 {
            for &r in rooms {
                let room = deck.room_mut(r);
                let share = made * room.volume / total;
                let put = share.min(room.volume - room.oxygen).max(0.0);
                room.oxygen += put;
                added += put;
            }
        }
        self.output = if dt > 0.0 { added / dt } else { 0.0 };
        added
    }
}

// ---------- a volley ----------

/// A seeded string of hits over a few seconds: the dev area's stand-in for an enemy's volley.
#[derive(Clone, Debug)]
pub struct Volley {
    rng: Rng,
    sections: Vec<SectionId>,
    left: u32,
    wait: f64,
}

impl Volley {
    /// `count` hits on random `sections`, 0.6 to 1.5 s apart.
    pub fn new(seed: u64, sections: Vec<SectionId>, count: u32) -> Volley {
        Volley { rng: Rng::new(seed), sections, left: count, wait: 0.0 }
    }

    pub fn left(&self) -> u32 {
        self.left
    }

    /// The hit that lands in this step, if any.
    pub fn step(&mut self, dt: f64) -> Option<Hit> {
        if self.left == 0 || self.sections.is_empty() {
            return None;
        }
        self.wait -= dt;
        if self.wait > 0.0 {
            return None;
        }
        self.wait = 0.6 + 0.9 * self.rng.unit();
        self.left -= 1;
        let kind = DamageType::ALL[self.rng.below(3)];
        let roll = self.rng.unit();
        let strength = if roll < 0.5 { Strength::Light } else if roll < 0.85 { Strength::Medium } else { Strength::Heavy };
        let section = self.sections[self.rng.below(self.sections.len())];
        Some(Hit::standard(kind, strength, section))
    }
}
