//! The repair minigame's rules (#299): a pointer circles a ring around the reticle; zones on the
//! ring appear, shrink and vanish. LMB inside a zone hits it (the zone goes) and every LMB turns
//! the pointer round; RMB speeds it up. Pure f64, no engine types: the glue draws the ring, runs it
//! on the client and sends the hits and misses to the host, which applies them to the condition.
//!
//! Angles are degrees, clockwise from the top of the ring. All values are starting points for the
//! playtest (`TODO(initiator)`); they come from `content/gameplay/repair/repair.json`.
use gameplay_core::rng::Rng;
use serde::{Deserialize, Serialize};

/// The values of the repair rules. Every field is in the JSON file, and an unknown one is an error.
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    /// The repairer's skill, 0..100, for the session's duration (all players 50 for now).
    pub skill: f64,
    /// A hit adds this share of the device's max condition.
    pub hit_share: f64,
    /// A miss costs this share of the device's max condition.
    pub miss_share: f64,
    /// The pointer's speed, degrees per second.
    pub pointer_deg_s: f64,
    /// The pointer's speed factor while RMB is held.
    pub boost: f64,
    /// Zones on the ring at once.
    pub zones_max: usize,
    /// A zone's width when it appears, degrees.
    pub zone_deg: f64,
    /// Seconds from appearing to gone; the zone narrows evenly to 0 at this time.
    pub zone_life_s: f64,
    /// Seconds after a zone is gone (or the count is below the maximum) before the next appears.
    pub respawn_s: f64,
    /// A new zone's centre is at least this far from the pointer, degrees.
    pub spawn_min_deg: f64,
    /// The reach of the look at the device, metres.
    pub reach_m: f64,
    /// Walking further than this from where the session began ends it, metres.
    pub walk_away_m: f64,
    /// Seconds the hit or miss flash shows on the HUD.
    pub flash_s: f64,
    /// The minigame runs only while the condition is below this share of max (more than half
    /// damaged). Above it the repair is the slow one, with no ring.
    pub minigame_below: f64,
    /// A broken device (not functioning) makes no progress until this many hits in the session.
    pub broken_hits: u32,
}

/// Whether the minigame runs at this condition (a share of max, 0..1).
pub fn minigame_on(fraction: f64, t: &Tuning) -> bool {
    fraction < t.minigame_below
}

/// Whether a session may raise the condition now. A session that began on a broken device (`gated`)
/// waits for its hits, and stays open once it has them.
pub fn may_rise(gated: bool, hits: u32, t: &Tuning) -> bool {
    !gated || hits >= t.broken_hits
}

/// One repairer's session on one machine: the ring and the hits so far. The ring turns, and LMB
/// counts, only while the minigame is on; the condition's rise is the glue's (`may_rise`).
#[derive(Clone, Debug)]
pub struct Session {
    pub ring: Ring,
    /// Hits this session has made.
    pub hits: u32,
}

impl Session {
    pub fn new(tuning: Tuning, seed: u64) -> Session {
        Session { ring: Ring::new(tuning, seed), hits: 0 }
    }

    /// One step at this condition: the ring turns only while the minigame is on, and a ring that
    /// is off shows no zones.
    pub fn step(&mut self, dt: f64, boosted: bool, fraction: f64) {
        if minigame_on(fraction, &self.ring.tuning) {
            self.ring.step(dt, boosted);
        } else {
            self.ring.zones.clear();
        }
    }

    /// LMB at this condition: None when the minigame is off (nothing happens), else the press.
    pub fn press(&mut self, fraction: f64) -> Option<Press> {
        if !minigame_on(fraction, &self.ring.tuning) {
            return None;
        }
        let p = self.ring.press();
        if p == Press::Hit {
            self.hits += 1;
        }
        Some(p)
    }
}

/// A zone on the ring: its centre and its age. Its width shrinks with the age.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Zone {
    pub centre_deg: f64,
    pub age_s: f64,
}

impl Zone {
    /// Width now, degrees: `zone_deg` at the start, 0 at `zone_life_s`.
    pub fn width(&self, t: &Tuning) -> f64 {
        t.zone_deg * (1.0 - self.age_s / t.zone_life_s).clamp(0.0, 1.0)
    }

    /// Start and end angle of the zone now, degrees (the start may be past the end when it wraps).
    pub fn arc(&self, t: &Tuning) -> (f64, f64) {
        let half = self.width(t) * 0.5;
        (wrap(self.centre_deg - half), wrap(self.centre_deg + half))
    }
}

/// What one LMB press did.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Press {
    /// The pointer was in a zone: the zone is gone.
    Hit,
    /// The pointer was in no zone.
    Miss,
}

/// The ring while a session runs.
#[derive(Clone, Debug)]
pub struct Ring {
    pub tuning: Tuning,
    /// The pointer's angle, degrees 0..360.
    pub pointer_deg: f64,
    /// +1 clockwise, -1 anticlockwise.
    pub dir: f64,
    /// The zones now, oldest first.
    pub zones: Vec<Zone>,
    /// Seconds until the next zone may appear (counts only while below the maximum).
    respawn_left: f64,
    rng: Rng,
}

impl Ring {
    /// A new ring: the pointer at the top, going clockwise, no zones yet. `seed` makes the zones
    /// repeatable.
    pub fn new(tuning: Tuning, seed: u64) -> Ring {
        Ring { tuning, pointer_deg: 0.0, dir: 1.0, zones: Vec::new(), respawn_left: tuning.respawn_s, rng: Rng::new(seed) }
    }

    /// One step of `dt` seconds: the pointer moves (faster while `boosted`), the zones age and
    /// vanish, and a new zone appears when the count is below the maximum and the wait is over.
    pub fn step(&mut self, dt: f64, boosted: bool) {
        let t = self.tuning;
        let speed = t.pointer_deg_s * if boosted { t.boost } else { 1.0 };
        self.pointer_deg = wrap(self.pointer_deg + self.dir * speed * dt);
        for z in &mut self.zones {
            z.age_s += dt;
        }
        let before = self.zones.len();
        self.zones.retain(|z| z.age_s < t.zone_life_s);
        if self.zones.len() < before {
            self.respawn_left = t.respawn_s;
        }
        if self.zones.len() < t.zones_max {
            self.respawn_left -= dt;
            if self.respawn_left <= 0.0 {
                self.spawn();
                self.respawn_left = t.respawn_s;
            }
        }
    }

    /// LMB: a zone the pointer is in is hit and gone, else it is a miss. Either way the pointer
    /// turns round.
    pub fn press(&mut self) -> Press {
        self.dir = -self.dir;
        let t = self.tuning;
        let hit = self.zones.iter().position(|z| {
            let w = z.width(&t);
            w > 0.0 && angle_dist(self.pointer_deg, z.centre_deg) <= w * 0.5
        });
        match hit {
            Some(i) => {
                self.zones.remove(i);
                self.respawn_left = t.respawn_s;
                Press::Hit
            }
            None => Press::Miss,
        }
    }

    /// Whether the pointer is inside a zone now: an LMB now would hit.
    pub fn in_zone(&self) -> bool {
        let t = self.tuning;
        self.zones.iter().any(|z| {
            let w = z.width(&t);
            w > 0.0 && angle_dist(self.pointer_deg, z.centre_deg) <= w * 0.5
        })
    }

    /// A zone at a random centre at least `spawn_min_deg` away from the pointer.
    fn spawn(&mut self) {
        let t = self.tuning;
        let span = (360.0 - 2.0 * t.spawn_min_deg).max(0.0);
        let off = t.spawn_min_deg + span * self.rng.unit();
        self.zones.push(Zone { centre_deg: wrap(self.pointer_deg + off), age_s: 0.0 });
    }
}

/// An angle in 0..360.
pub fn wrap(a: f64) -> f64 {
    a.rem_euclid(360.0)
}

/// The smaller angle between two angles, 0..180.
pub fn angle_dist(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Whether the ray from `origin` along `dir` (both in the box's frame, metres) hits the axis-aligned
/// box with half sizes `half` centred on the origin of that frame, within `reach` metres.
pub fn ray_hits_box(origin: [f64; 3], dir: [f64; 3], half: [f64; 3], reach: f64) -> bool {
    let (mut lo, mut hi) = (0.0_f64, reach);
    for i in 0..3 {
        // The box is centred on zero, so its faces are at +-half.
        let (o, d, h) = (origin[i], dir[i], half[i]);
        if d.abs() < 1e-12 {
            if o.abs() > h {
                return false;
            }
        } else {
            let (a, b) = ((-h - o) / d, (h - o) / d);
            lo = lo.max(a.min(b));
            hi = hi.min(a.max(b));
            if lo > hi {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f64 = 1.0 / 60.0;

    fn tuning() -> Tuning {
        Tuning {
            skill: 50.0,
            hit_share: 0.08,
            miss_share: 0.02,
            pointer_deg_s: 90.0,
            boost: 2.0,
            zones_max: 2,
            zone_deg: 40.0,
            zone_life_s: 3.0,
            respawn_s: 0.8,
            spawn_min_deg: 60.0,
            reach_m: 1.8,
            walk_away_m: 2.5,
            flash_s: 0.3,
            minigame_below: 0.5,
            broken_hits: 3,
        }
    }

    /// A ring with one zone at `centre` and the pointer at `pointer`.
    fn with_zone(pointer: f64, centre: f64) -> Ring {
        let mut r = Ring::new(tuning(), 1);
        r.pointer_deg = pointer;
        r.zones.push(Zone { centre_deg: centre, age_s: 0.0 });
        r
    }

    #[test]
    fn lmb_inside_a_zone_hits_it_and_the_zone_goes() {
        let mut r = with_zone(100.0, 105.0);
        assert_eq!(r.press(), Press::Hit);
        assert!(r.zones.is_empty());
    }

    #[test]
    fn in_zone_says_whether_an_lmb_now_would_hit() {
        assert!(with_zone(100.0, 105.0).in_zone());
        assert!(!with_zone(0.0, 120.0).in_zone());
        let mut r = with_zone(100.0, 105.0);
        r.press();
        assert!(!r.in_zone(), "the hit zone is gone");
    }

    #[test]
    fn lmb_outside_every_zone_misses_and_keeps_the_zone() {
        let mut r = with_zone(0.0, 120.0);
        assert_eq!(r.press(), Press::Miss);
        assert_eq!(r.zones.len(), 1);
    }

    #[test]
    fn the_zone_edge_counts_and_the_wrap_at_zero_counts() {
        // 20 degrees is the half width: exactly on the edge hits, beyond it misses.
        assert_eq!(with_zone(20.0, 0.0).press(), Press::Hit);
        assert_eq!(with_zone(20.5, 0.0).press(), Press::Miss);
        // A zone centred on 0 reaches 350 degrees.
        assert_eq!(with_zone(355.0, 0.0).press(), Press::Hit);
    }

    #[test]
    fn every_lmb_reverses_the_pointer() {
        let mut r = with_zone(0.0, 0.0);
        assert_eq!(r.dir, 1.0);
        r.press();
        assert_eq!(r.dir, -1.0);
        r.press();
        assert_eq!(r.dir, 1.0);
        let mut m = with_zone(0.0, 180.0);
        m.press();
        assert_eq!(m.dir, -1.0, "a miss turns the pointer round too");
    }

    #[test]
    fn the_pointer_moves_at_the_set_speed_and_rmb_doubles_it() {
        let mut plain = Ring::new(tuning(), 1);
        plain.step(0.5, false);
        assert!((plain.pointer_deg - 45.0).abs() < 1e-9);
        let mut boosted = Ring::new(tuning(), 1);
        boosted.step(0.5, true);
        assert!((boosted.pointer_deg - 90.0).abs() < 1e-9);
    }

    #[test]
    fn a_reversed_pointer_goes_back() {
        let mut r = Ring::new(tuning(), 1);
        r.pointer_deg = 10.0;
        r.press();
        r.step(0.1, false);
        // 9 degrees back in 0.1 s at 90 degrees a second: 1.
        assert!((r.pointer_deg - 1.0).abs() < 1e-9);
    }

    #[test]
    fn zones_shrink_to_nothing_and_then_vanish() {
        let t = tuning();
        let mut r = with_zone(0.0, 180.0);
        r.step(1.5, false);
        assert!((r.zones[0].width(&t) - 20.0).abs() < 1e-9, "half way: half the width");
        r.step(1.5 - 1e-9, false);
        assert!(r.zones.is_empty() || r.zones[0].width(&t) < 1e-6, "at the end: nothing");
        let mut gone = with_zone(0.0, 180.0);
        gone.step(3.0, false);
        // The old zone (age 3 s) is gone; a fresh one may have come in its place (the wait is over).
        assert!(gone.zones.iter().all(|z| z.age_s == 0.0), "vanished at the life's end");
    }

    #[test]
    fn at_most_two_zones_at_once_and_one_appears_after_the_wait() {
        let mut r = Ring::new(tuning(), 9);
        let mut t = 0.0;
        let mut first = None;
        let mut most = 0;
        while t < 60.0 {
            r.step(DT, false);
            t += DT;
            most = most.max(r.zones.len());
            if first.is_none() && !r.zones.is_empty() {
                first = Some(t);
            }
        }
        assert!(most <= 2, "never more than two zones, saw {most}");
        assert_eq!(most, 2, "two zones do appear");
        let first = first.expect("a zone appears");
        assert!((first - 0.8).abs() < DT * 1.5, "the first zone appears about 0.8 s in, at {first}");
    }

    #[test]
    fn a_new_zone_appears_about_a_wait_after_one_is_gone() {
        let mut r = with_zone(0.0, 180.0);
        r.zones[0].age_s = 2.99;
        r.zones.push(Zone { centre_deg: 270.0, age_s: 0.0 });
        r.step(0.02, false); // the first zone vanishes: the count is 1, the wait starts
        assert_eq!(r.zones.len(), 1);
        let mut t = 0.0;
        while r.zones.len() < 2 && t < 5.0 {
            r.step(DT, false);
            t += DT;
        }
        // The wait runs from the vanish: 0.8 s less the 0.02 s step that took the zone away.
        assert!((t - 0.78).abs() < DT * 1.5, "the next zone after about 0.8 s, at {t}");
    }

    #[test]
    fn a_new_zone_is_at_least_sixty_degrees_from_the_pointer() {
        for seed in 0..300 {
            let mut r = Ring::new(tuning(), seed);
            r.pointer_deg = (seed as f64 * 37.0) % 360.0;
            r.spawn();
            let z = r.zones[0];
            assert!(angle_dist(r.pointer_deg, z.centre_deg) >= 60.0 - 1e-9, "seed {seed}: {} from {}", z.centre_deg, r.pointer_deg);
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_zones() {
        let run = |seed| {
            let mut r = Ring::new(tuning(), seed);
            for _ in 0..600 {
                r.step(DT, false);
            }
            r.zones.iter().map(|z| z.centre_deg).collect::<Vec<_>>()
        };
        assert_eq!(run(4), run(4));
        assert_ne!(run(4), run(5));
    }

    #[test]
    fn below_half_damage_there_are_no_zones_and_no_presses() {
        let t = tuning();
        let mut s = Session::new(t, 3);
        for _ in 0..600 {
            s.step(DT, false, 0.6);
        }
        assert!(s.ring.zones.is_empty(), "at 60 % condition the minigame is off");
        assert_eq!(s.press(0.6), None, "and LMB does nothing");
        assert_eq!(s.ring.dir, 1.0, "the pointer did not turn");
        assert_eq!(s.hits, 0);
    }

    #[test]
    fn below_half_condition_the_minigame_runs_with_zones() {
        let t = tuning();
        let mut s = Session::new(t, 3);
        for _ in 0..240 {
            s.step(DT, false, 0.4);
        }
        assert!(!s.ring.zones.is_empty(), "at 40 % condition zones come");
    }

    #[test]
    fn a_broken_device_waits_for_three_hits() {
        let t = tuning();
        assert!(!may_rise(true, 0, &t) && !may_rise(true, 2, &t), "no progress before the third hit");
        assert!(may_rise(true, 3, &t), "after the third hit it rises");
        assert!(may_rise(false, 0, &t), "a session that did not start on a broken device always rises");
        let mut s = Session::new(t, 5);
        let mut hits = 0;
        // Hits land whenever the pointer is in a zone; the session counts them.
        for _ in 0..(60 * 60) {
            s.step(DT, false, 0.1);
            if s.ring.in_zone() && hits < 3 {
                s.press(0.1);
                hits = s.hits;
            }
            if hits >= 3 {
                break;
            }
        }
        assert_eq!(s.hits, 3, "three hits are made");
        assert!(may_rise(true, s.hits, &t));
    }

    #[test]
    fn the_minigame_threshold_is_half() {
        let t = tuning();
        assert!(minigame_on(0.49, &t));
        assert!(!minigame_on(0.5, &t), "at 50 % the repair is the slow one");
        assert!(!minigame_on(1.0, &t));
    }

    #[test]
    fn the_box_test_hits_inside_and_misses_beside_and_beyond_reach() {
        let half = [0.5, 0.5, 0.5];
        assert!(ray_hits_box([0.0, 0.0, -2.0], [0.0, 0.0, 1.0], half, 3.0));
        assert!(!ray_hits_box([0.0, 0.0, -2.0], [0.0, 0.0, 1.0], half, 1.2), "the box's near face is 1.5 m away: out of a 1.2 m reach");
        assert!(ray_hits_box([0.0, 0.0, -2.0], [0.0, 0.0, 1.0], half, 1.8), "in reach at 1.8 m");
        assert!(!ray_hits_box([0.7, 0.0, -2.0], [0.0, 0.0, 1.0], half, 3.0), "beside the box");
        assert!(!ray_hits_box([0.0, 0.0, -2.0], [0.0, 0.0, -1.0], half, 3.0), "the box is behind");
    }
}
