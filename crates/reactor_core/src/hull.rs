//! The engine room's hull (r40, replaces the r37 single breach). Hits and sections follow
//! Barotrauma: a hit is data (`Hit`: where, how much on the walls, on the machines, the fire
//! chance), a wall is split into sections with their own damage, and a section leaks by its
//! damage. Sources: `Map/Structure.cs:53` LeakThreshold 0.1, `:54` BigGapThreshold 0.7, `:59`
//! SmallGapOpenness 0.35, `:64` LargeGapOpenness 0.75, and `:1340-1352` the gap's openness from the
//! damage ratio (0 up to 0.1; lerp 0 to 0.35 up to 0.7; lerp 0.35 to 0.75 up to 1). The walls take
//! a hit as `Map/Explosion.cs:655` RangedStructureDamage does: structure * (1 - distance / range)
//! inside the range (`distanceFalloff`). The openness feeds `fire_core` (one leak per section).
use serde::{Deserialize, Serialize};

use crate::engine_room::Source;

/// The sections of the engine room's walls (each wall halved).
pub const SECTIONS: usize = 8;
/// Barotrauma `LeakThreshold`: a section at or below this damage does not leak.
pub const LEAK_BELOW: f64 = 0.1;
/// Barotrauma `BigGapThreshold`: above it the gap lerps from `SMALL_GAP_OPEN` up to `LARGE_GAP_OPEN`.
pub const BIG_GAP_ABOVE: f64 = 0.7;
pub const SMALL_GAP_OPEN: f64 = 0.35;
pub const LARGE_GAP_OPEN: f64 = 0.75;

/// The engine room's inner walls in area metres (mirrors `dev_area::engine_room::ROOM`, centre
/// (-14, -15), half (5, 4); the glue checks it). South-west corner and north-east corner.
pub const WALLS_MIN: [f64; 2] = [-19.0, -19.0];
pub const WALLS_MAX: [f64; 2] = [-9.0, -11.0];
/// The reactor (mirrors `dev_area::engine_room::REACTOR_AT`): the blast's centre.
pub const REACTOR_AT: [f64; 2] = [-14.0, -17.8];

/// The section centres on the wall line, counter-clockwise from the south-west corner: the south
/// wall in two halves, then east, north and west. Section 0 is the old r37 breach spot.
pub const SECTION_CENTRES: [[f64; 2]; SECTIONS] = [
    [-16.5, -19.0],
    [-11.5, -19.0],
    [-9.0, -16.0],
    [-9.0, -12.0],
    [-11.5, -11.0],
    [-16.5, -11.0],
    [-19.0, -12.0],
    [-19.0, -16.0],
];

/// The machines of the room with their centres (mirrors the glue's `*_AT`). A hit in range
/// damages them, as a blast does.
pub const DEVICES: [(Source, [f64; 2]); 3] = [(Source::Pump, [-10.5, -17.5]), (Source::Junction, [-17.0, -18.85]), (Source::OxygenGenerator, [-9.4, -13.0])];

/// Barotrauma's gap openness of a section at `damage` (0..1), `Structure.cs:1340-1352`.
pub fn openness(damage: f64) -> f64 {
    let d = damage.clamp(0.0, 1.0);
    if d > BIG_GAP_ABOVE {
        lerp(SMALL_GAP_OPEN, LARGE_GAP_OPEN, (d - BIG_GAP_ABOVE) / (1.0 - BIG_GAP_ABOVE))
    } else if d > LEAK_BELOW {
        lerp(0.0, SMALL_GAP_OPEN, (d - LEAK_BELOW) / (BIG_GAP_ABOVE - LEAK_BELOW))
    } else {
        0.0
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// One wall section: its centre (area metres) and its damage, 0 (sound) .. 1 (wall gone).
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub centre: [f64; 2],
    damage: f64,
}

impl Section {
    pub fn damage(&self) -> f64 {
        self.damage
    }

    /// The leak's openness, 0 (closed) .. 1 (fully open).
    pub fn open(&self) -> f64 {
        openness(self.damage)
    }
}

/// The engine room's walls: `SECTIONS` sections, all sound at first.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hull {
    pub sections: [Section; SECTIONS],
}

impl Default for Hull {
    fn default() -> Hull {
        Hull { sections: SECTION_CENTRES.map(|centre| Section { centre, damage: 0.0 }) }
    }
}

impl Hull {
    pub fn damage(&self, i: usize) -> f64 {
        self.sections[i].damage
    }

    pub fn open(&self, i: usize) -> f64 {
        self.sections[i].open()
    }

    /// True while no section has any damage.
    pub fn intact(&self) -> bool {
        self.sections.iter().all(|s| s.damage <= 0.0)
    }

    /// True while some section leaks (its openness above 0).
    pub fn leaking(&self) -> bool {
        self.sections.iter().any(|s| s.open() > 0.0)
    }

    /// Sets a section's damage (the dev fault and the blast's centre): 0..1, clamped.
    pub fn set(&mut self, i: usize, damage: f64) {
        self.sections[i].damage = damage.clamp(0.0, 1.0);
    }

    /// A repair takes `amount` of a section's damage off, never below 0.
    pub fn mend(&mut self, i: usize, amount: f64) {
        let s = &mut self.sections[i];
        s.damage = (s.damage - amount.max(0.0)).max(0.0);
    }

    /// The section whose centre is nearest `point` (ties to the lower index).
    pub fn nearest(&self, point: [f64; 2]) -> usize {
        let mut best = 0;
        for i in 1..SECTIONS {
            if dist(self.sections[i].centre, point) < dist(self.sections[best].centre, point) {
                best = i;
            }
        }
        best
    }

    /// The damaged section nearest `point`, or None while the walls are sound.
    pub fn nearest_damaged(&self, point: [f64; 2]) -> Option<usize> {
        (0..SECTIONS).filter(|&i| self.sections[i].damage > 0.0).min_by(|&a, &b| dist(self.sections[a].centre, point).total_cmp(&dist(self.sections[b].centre, point)))
    }

    /// The walls take a hit at `at`: every section within `radius` gets `structure * (1 - d / radius)`.
    /// Returns whether any section is in range.
    pub fn hit(&mut self, at: [f64; 2], structure: f64, radius: f64) -> bool {
        let mut reached = false;
        for s in self.sections.iter_mut() {
            let d = dist(s.centre, at);
            if d < radius {
                reached = true;
                s.damage = (s.damage + structure * (1.0 - d / radius)).clamp(0.0, 1.0);
            }
        }
        reached
    }

    /// A point on the inner wall line at `u` (0..1 around the perimeter, counter-clockwise from the
    /// south-west corner): what a random hit lands on.
    pub fn wall_point(u: f64) -> [f64; 2] {
        let [x0, y0] = WALLS_MIN;
        let [x1, y1] = WALLS_MAX;
        let (w, h) = (x1 - x0, y1 - y0);
        // Four walls as (start, direction, length), walked in order.
        let walls = [([x0, y0], [1.0, 0.0], w), ([x1, y0], [0.0, 1.0], h), ([x1, y1], [-1.0, 0.0], w), ([x0, y1], [0.0, -1.0], h)];
        let mut left = u.clamp(0.0, 1.0) * 2.0 * (w + h);
        for (start, dir, len) in walls {
            if left <= len {
                return [start[0] + dir[0] * left, start[1] + dir[1] * left];
            }
            left -= len;
        }
        WALLS_MIN
    }
}

/// One hit, as data: where it lands, its damage to the walls (`structure`, at the centre of
/// `radius`), to the machines in range (`device_damage`, the same falloff) and the chance of a fire
/// at the impact (when it reaches the room).
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hit {
    pub at: [f64; 2],
    pub structure: f64,
    pub radius: f64,
    pub device_damage: f64,
    pub fire_chance: f64,
}

/// A hit preset of reactor.json `hits`: the same numbers without a place.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitTuning {
    pub structure: f64,
    pub radius: f64,
    pub device_damage: f64,
    pub fire_chance: f64,
}

impl HitTuning {
    pub fn at(&self, at: [f64; 2]) -> Hit {
        Hit { at, structure: self.structure, radius: self.radius, device_damage: self.device_damage, fire_chance: self.fire_chance }
    }
}

/// The presets of reactor.json `hits`: the dev faults HIT LIGHT and HIT HEAVY.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitsTuning {
    pub light: HitTuning,
    pub heavy: HitTuning,
}

impl Default for HitsTuning {
    fn default() -> HitsTuning {
        // TODO(initiator): all values. Derivation: the blast's wall damage is 0.6 (reactor.json blast's
        // range 7.5 m, device 500, Barotrauma's numbers). A junction box overvoltage hit is 10..500 / 60
        // at up to 5x sensitivity (`JunctionBox::overload`), about 15 to 40 a hit: light 15, heavy 40.
        // Light radius 2.5 m: the farthest wall point from its nearest section centre is 2.5 m (sections
        // are 4 to 5 m long), so a light hit always reaches a section. Fire chance: Barotrauma's
        // FireProbability 0.15 for a box break, halved for light (0.05) and raised for heavy (0.25).
        HitsTuning { light: HitTuning { structure: 0.4, radius: 2.5, device_damage: 15.0, fire_chance: 0.05 }, heavy: HitTuning { structure: 0.9, radius: 4.0, device_damage: 40.0, fire_chance: 0.25 } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_openness_curve_is_barotrauma_s() {
        assert_eq!(openness(0.05), 0.0, "a small dent does not leak");
        assert_eq!(openness(LEAK_BELOW), 0.0, "at the threshold the wall still holds");
        assert!((openness(0.4) - 0.175).abs() < 1e-12, "half way to the big gap: half of 0.35");
        assert!((openness(0.7) - 0.35).abs() < 1e-12, "the small gap's top");
        assert!((openness(1.0) - 0.75).abs() < 1e-12, "the large gap");
    }

    #[test]
    fn a_hit_falls_off_with_distance_and_a_far_one_does_nothing() {
        let mut h = Hull::default();
        let c = h.sections[0].centre;
        assert!(h.hit([c[0] + 1.0, c[1]], 0.8, 2.0));
        assert!((h.damage(0) - 0.8 * 0.5).abs() < 1e-12, "1 m of 2 m: half the structure");
        let before = h;
        assert!(!h.hit([0.0, 0.0], 0.8, 2.0), "far from every section");
        assert_eq!(h, before);
    }

    #[test]
    fn a_repaired_section_closes() {
        let mut h = Hull::default();
        h.set(3, 0.55);
        assert!((h.open(3) - openness(0.55)).abs() < 1e-12, "0.55 is 0.2625 open");
        h.mend(3, 1.0);
        assert_eq!(h.damage(3), 0.0);
        assert_eq!(h.open(3), 0.0, "a repaired section closes its leak");
        assert!(h.intact());
    }

    #[test]
    fn nearest_points_at_the_section_and_the_damaged_one() {
        let mut h = Hull::default();
        assert_eq!(h.nearest([-16.4, -18.9]), 0);
        assert_eq!(h.nearest_damaged([-16.4, -18.9]), None, "intact walls have no damaged section");
        h.set(5, 0.2);
        assert_eq!(h.nearest_damaged([-16.4, -18.9]), Some(5));
    }

    #[test]
    fn a_wall_point_stays_on_the_inner_wall() {
        for k in 0..=40 {
            let p = Hull::wall_point(k as f64 / 40.0);
            let on_wall = (p[0] - WALLS_MIN[0]).abs() < 1e-9 || (p[0] - WALLS_MAX[0]).abs() < 1e-9 || (p[1] - WALLS_MIN[1]).abs() < 1e-9 || (p[1] - WALLS_MAX[1]).abs() < 1e-9;
            assert!(on_wall && p[0] >= WALLS_MIN[0] - 1e-9 && p[0] <= WALLS_MAX[0] + 1e-9 && p[1] >= WALLS_MIN[1] - 1e-9 && p[1] <= WALLS_MAX[1] + 1e-9, "{p:?}");
        }
        assert_eq!(Hull::wall_point(0.0), [WALLS_MIN[0], WALLS_MIN[1]]);
    }

}
