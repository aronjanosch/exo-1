//! The fizz pockets (#333): hidden spots of gas on the ground around the dig site, found by ear with the sniffer. A
//! hidden pocket opens when someone taps it (holds the tap on it for `tap_s`), puffs for `open_s`, gives
//! `jars` jars of gas and is then spent; a new hidden one forms at another free point. No Bevy types; the host steps
//! it, clients only read the places and compute the signal themselves (`signal_at`): this is a co-op game, so the
//! hidden places are not secret. Seeded with the project's rng, so the same seed gives the same places.
//! Ground is in area metres (x east, y north), the same frame as `Zone`s in the app.
use gameplay_core::rng::Rng;
use serde::Deserialize;

/// How many tries a new place gets before the pocket waits (the field may be full).
const TRIES: usize = 400;

/// `pocket` in dig.json. Every number is a starting value, TODO(initiator).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PocketConfig {
    /// Hidden pockets that exist at a time.
    pub pockets: u32,
    /// Two pockets lie at least this far apart (m); a new pocket also keeps this far from the spent one.
    pub min_gap_m: f64,
    /// The sniffer hears a pocket up to this far (m).
    pub range_m: f64,
    /// The signal's near scale (m): at this distance the pocket's own pull is half of full. Smaller means the
    /// last metres matter more.
    pub core_m: f64,
    /// Holding the tap this close to a hidden pocket opens it (m).
    pub tap_radius_m: f64,
    /// Seconds the tap is held until the pocket opens.
    pub tap_s: f64,
    /// Jars of gas one pocket gives.
    pub jars: u32,
    /// Seconds an open pocket puffs; then it closes again and keeps its jars.
    pub open_s: f64,
    /// How far an open vent reaches for a jar (m, from the eye).
    pub reach_m: f64,
    /// The search field's half size around the dig site's centre (m): 20 makes a square of 40 x 40 m.
    pub field_half_m: f64,
    /// Pockets keep this far from the blocked ground and the field's edge (m).
    pub margin_m: f64,
}

impl PocketConfig {
    pub fn problems(&self) -> Vec<String> {
        let mut e = Vec::new();
        let mut need = |ok: bool, field: &str, rule: &str| {
            if !ok {
                e.push(format!("pocket.{field}: {rule}"));
            }
        };
        let pos = |x: f64| x.is_finite() && x > 0.0;
        need(self.pockets > 0, "pockets", "must be > 0");
        need(pos(self.min_gap_m), "min_gap_m", "must be > 0");
        need(pos(self.range_m), "range_m", "must be > 0");
        need(pos(self.core_m), "core_m", "must be > 0");
        need(pos(self.tap_radius_m), "tap_radius_m", "must be > 0");
        need(pos(self.tap_s), "tap_s", "must be > 0");
        need(self.jars > 0, "jars", "must be > 0");
        need(pos(self.open_s), "open_s", "must be > 0");
        need(pos(self.reach_m), "reach_m", "must be > 0");
        need(pos(self.field_half_m), "field_half_m", "must be > 0");
        need(self.margin_m.is_finite() && self.margin_m >= 0.0, "margin_m", "must be >= 0");
        e
    }
}

/// An axis-aligned rectangle of ground: its centre and half sizes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub centre: [f64; 2],
    pub half: [f64; 2],
}

impl Rect {
    fn near(&self, p: [f64; 2], margin: f64) -> bool {
        (p[0] - self.centre[0]).abs() <= self.half[0] + margin && (p[1] - self.centre[1]).abs() <= self.half[1] + margin
    }
}

/// Where pockets may lie: inside `lo..hi`, outside every blocked rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct Ground {
    pub lo: [f64; 2],
    pub hi: [f64; 2],
    pub blocked: Vec<Rect>,
}

impl Ground {
    pub fn is_free(&self, p: [f64; 2], margin: f64) -> bool {
        p.iter().all(|x| x.is_finite())
            && (0..2).all(|i| p[i] >= self.lo[i] + margin && p[i] <= self.hi[i] - margin)
            && !self.blocked.iter().any(|r| r.near(p, margin))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PocketState {
    Hidden,
    /// Puffing: seconds until it closes again.
    Open { left_s: f64 },
    /// All jars given; waiting for a place for its successor (normally replaced at once).
    Spent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pocket {
    pub at: [f64; 2],
    pub jars_left: u32,
    pub state: PocketState,
    /// Seconds the tap has been held on it (hidden only), 0..tap_s.
    pub tap: f64,
}

/// What a fill gave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    /// The pocket is not open (or no such pocket): nothing.
    Nothing,
    /// One jar's worth; `spent`: that was the last, a new pocket formed.
    Jar { spent: bool },
}

#[derive(Clone, Debug)]
pub struct PocketField {
    cfg: PocketConfig,
    ground: Ground,
    rng: Rng,
    pockets: Vec<Pocket>,
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn sane(dt: f64) -> f64 {
    if dt.is_finite() && dt > 0.0 { dt } else { 0.0 }
}

impl PocketField {
    pub fn new(cfg: &PocketConfig, ground: Ground, seed: u64) -> PocketField {
        let mut f = PocketField { cfg: cfg.clone(), ground, rng: Rng::new(seed), pockets: Vec::new() };
        for _ in 0..cfg.pockets {
            let avoid: Vec<[f64; 2]> = f.pockets.iter().map(|p| p.at).collect();
            match f.pick(&avoid) {
                Some(at) => f.pockets.push(Pocket { at, jars_left: cfg.jars, state: PocketState::Hidden, tap: 0.0 }),
                None => break,
            }
        }
        f
    }

    /// A random free point at least `min_gap_m` from every point in `avoid`.
    fn pick(&mut self, avoid: &[[f64; 2]]) -> Option<[f64; 2]> {
        for _ in 0..TRIES {
            let p = [
                self.ground.lo[0] + self.rng.unit() * (self.ground.hi[0] - self.ground.lo[0]),
                self.ground.lo[1] + self.rng.unit() * (self.ground.hi[1] - self.ground.lo[1]),
            ];
            if self.ground.is_free(p, self.cfg.margin_m) && avoid.iter().all(|a| dist(*a, p) >= self.cfg.min_gap_m) {
                return Some(p);
            }
        }
        None
    }

    pub fn pockets(&self) -> &[Pocket] {
        &self.pockets
    }

    pub fn config(&self) -> &PocketConfig {
        &self.cfg
    }

    pub fn ground(&self) -> &Ground {
        &self.ground
    }

    /// The sniffer's signal at a point, 0..1, from all pockets that are there (a spent one is not).
    pub fn signal(&self, at: [f64; 2]) -> f64 {
        signal_at(&self.cfg, self.pockets.iter().filter(|p| p.state != PocketState::Spent).map(|p| p.at), at)
    }

    /// The open pocket nearest to `at` within `reach` metres (on the ground plane).
    pub fn open_near(&self, at: [f64; 2], reach: f64) -> Option<usize> {
        (0..self.pockets.len())
            .filter(|i| matches!(self.pockets[*i].state, PocketState::Open { .. }) && dist(self.pockets[*i].at, at) <= reach)
            .min_by(|a, b| dist(self.pockets[*a].at, at).total_cmp(&dist(self.pockets[*b].at, at)))
    }

    /// Steps `dt` seconds. `tappers` are the places where a tap is held now. An open pocket runs down and closes
    /// again with its jars; a hidden one a tap is held on opens after `tap_s`; its progress runs back when the tap is
    /// let go. A spent pocket looks for a place again. A `dt` that is negative or not finite is no time.
    pub fn step(&mut self, dt: f64, tappers: &[[f64; 2]]) {
        let dt = sane(dt);
        let (radius, tap_s, open_s) = (self.cfg.tap_radius_m, self.cfg.tap_s, self.cfg.open_s);
        for p in &mut self.pockets {
            match &mut p.state {
                PocketState::Open { left_s } => {
                    *left_s -= dt;
                    if *left_s <= 0.0 {
                        p.state = PocketState::Hidden;
                        p.tap = 0.0;
                    }
                }
                PocketState::Hidden => {
                    if tappers.iter().any(|t| dist(*t, p.at) <= radius) {
                        p.tap = (p.tap + dt).min(tap_s);
                        if p.tap >= tap_s {
                            p.state = PocketState::Open { left_s: open_s };
                            p.tap = 0.0;
                        }
                    } else {
                        p.tap = (p.tap - dt).max(0.0);
                    }
                }
                PocketState::Spent => {}
            }
        }
        for i in 0..self.pockets.len() {
            if self.pockets[i].state == PocketState::Spent {
                self.replace(i);
            }
        }
    }

    /// Takes one jar's worth from open pocket `i`. The last one spends it and a new hidden pocket forms.
    pub fn fill(&mut self, i: usize) -> Fill {
        let Some(p) = self.pockets.get_mut(i) else { return Fill::Nothing };
        if !matches!(p.state, PocketState::Open { .. }) || p.jars_left == 0 {
            return Fill::Nothing;
        }
        p.jars_left -= 1;
        if p.jars_left > 0 {
            return Fill::Jar { spent: false };
        }
        p.state = PocketState::Spent;
        self.replace(i);
        Fill::Jar { spent: true }
    }

    /// A spent pocket's successor: a fresh hidden one somewhere else that keeps the gap from every pocket,
    /// the spent one included. Without a free place it stays spent and tries again at the next step.
    fn replace(&mut self, i: usize) {
        let avoid: Vec<[f64; 2]> = self.pockets.iter().map(|p| p.at).collect();
        let others: Vec<[f64; 2]> = avoid;
        if let Some(at) = self.pick(&others) {
            self.pockets[i] = Pocket { at, jars_left: self.cfg.jars, state: PocketState::Hidden, tap: 0.0 };
        }
    }
}

/// The sniffer's signal at `at`: for each pocket `1 / (1 + (d / core_m)^2)` (steep near it: the last metres matter),
/// faded to exactly 0 at `range_m` by `1 - (d / range_m)^2`; the strongest pocket counts. Always finite, 0..1,
/// and falling with the distance.
pub fn signal_at(cfg: &PocketConfig, places: impl IntoIterator<Item = [f64; 2]>, at: [f64; 2]) -> f64 {
    if !at.iter().all(|x| x.is_finite()) {
        return 0.0;
    }
    places
        .into_iter()
        .map(|p| {
            let d = dist(p, at);
            if !d.is_finite() || d >= cfg.range_m {
                return 0.0;
            }
            (1.0 / (1.0 + (d / cfg.core_m).powi(2))) * (1.0 - (d / cfg.range_m).powi(2))
        })
        .fold(0.0, f64::max)
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PocketConfig {
        PocketConfig { pockets: 2, min_gap_m: 8.0, range_m: 18.0, core_m: 3.0, tap_radius_m: 1.5, tap_s: 2.0, jars: 3, open_s: 60.0, reach_m: 2.5, field_half_m: 20.0, margin_m: 1.0 }
    }

    fn ground() -> Ground {
        Ground { lo: [-20.0, -20.0], hi: [20.0, 20.0], blocked: vec![Rect { centre: [0.0, 0.0], half: [6.0, 4.0] }, Rect { centre: [12.0, -10.0], half: [3.0, 3.0] }] }
    }

    fn one_pocket(at: [f64; 2]) -> PocketField {
        let mut f = PocketField::new(&cfg(), ground(), 1);
        f.pockets.truncate(1);
        f.pockets[0].at = at;
        f
    }

    #[test]
    fn the_signal_is_finite_in_range_and_falls_with_the_distance() {
        let f = one_pocket([10.0, 10.0]);
        let mut last = f64::INFINITY;
        for i in 0..=400 {
            let d = i as f64 * 0.05;
            let s = f.signal([10.0 + d, 10.0]);
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "{d}: {s}");
            assert!(s <= last + 1e-12, "not monotonic at {d}: {s} after {last}");
            last = s;
        }
        assert_eq!(f.signal([10.0 + 18.0, 10.0]), 0.0, "nothing at the range");
        assert_eq!(f.signal([10.0 + 40.0, 10.0]), 0.0);
        assert_eq!(f.signal([10.0, 10.0]), 1.0, "full on the pocket");
        for bad in [[f64::NAN, 0.0], [f64::INFINITY, 0.0], [0.0, f64::NEG_INFINITY]] {
            assert_eq!(f.signal(bad), 0.0);
        }
    }

    #[test]
    fn the_last_metres_matter_more_than_the_first() {
        let f = one_pocket([10.0, 10.0]);
        let s = |d: f64| f.signal([10.0 + d, 10.0]);
        assert!(s(15.0) < s(6.0) && s(6.0) < s(0.5), "15 m {} 6 m {} 0.5 m {}", s(15.0), s(6.0), s(0.5));
        assert!(s(0.5) - s(6.0) > s(6.0) - s(15.0), "steep near the pocket");
        assert!(s(0.5) > 0.95, "the buzz starts on the pocket ({})", s(0.5));
    }

    #[test]
    fn the_strongest_pocket_counts() {
        let mut f = PocketField::new(&cfg(), ground(), 1);
        f.pockets[0].at = [10.0, 10.0];
        f.pockets[1].at = [-10.0, 10.0];
        assert_eq!(f.signal([10.0, 10.0]), 1.0);
        assert_eq!(f.signal([-10.0, 10.0]), 1.0);
        assert!(f.signal([-8.0, 10.0]) > f.signal([-4.0, 10.0]) - 0.5);
    }

    #[test]
    fn pockets_never_lie_outside_the_free_ground_or_closer_than_the_gap() {
        let (c, g) = (cfg(), ground());
        for seed in 0..200 {
            let mut f = PocketField::new(&c, g.clone(), seed);
            assert_eq!(f.pockets().len(), c.pockets as usize);
            // Open, fill and spend a lot of pockets: every place that ever forms keeps the rules.
            for _ in 0..40 {
                for p in f.pockets() {
                    assert!(g.is_free(p.at, c.margin_m), "seed {seed}: {:?} is not free ground", p.at);
                }
                for (i, a) in f.pockets().iter().enumerate() {
                    for b in &f.pockets()[i + 1..] {
                        assert!(dist(a.at, b.at) >= c.min_gap_m, "seed {seed}: {:?} and {:?} are too close", a.at, b.at);
                    }
                }
                let at = f.pockets()[0].at;
                f.step(c.tap_s + 0.1, &[at]);
                for _ in 0..c.jars {
                    f.fill(0);
                    f.step(0.1, &[]);
                }
            }
        }
    }

    #[test]
    fn a_pocket_gives_exactly_its_jars_never_more() {
        let c = cfg();
        let mut f = PocketField::new(&c, ground(), 7);
        assert_eq!(f.fill(0), Fill::Nothing, "a hidden pocket gives nothing");
        let at = f.pockets()[0].at;
        f.step(c.tap_s + 0.01, &[at]);
        assert!(matches!(f.pockets()[0].state, PocketState::Open { .. }));
        let mut got = 0;
        let mut spent = false;
        for _ in 0..10 {
            if let Fill::Jar { spent: s } = f.fill(0) {
                got += 1;
                spent = s;
                if s {
                    break;
                }
            }
        }
        assert_eq!((got, spent), (c.jars, true));
        assert_eq!(f.fill(0), Fill::Nothing, "the successor is hidden: no more from the old place");
        assert_eq!(f.fill(99), Fill::Nothing, "no such pocket");
    }

    #[test]
    fn the_same_seed_gives_the_same_places_and_another_seed_others() {
        let places = |seed| PocketField::new(&cfg(), ground(), seed).pockets().iter().map(|p| p.at).collect::<Vec<_>>();
        assert_eq!(places(5), places(5));
        assert_ne!(places(5), places(6));
        // And the same dealings give the same successors.
        let run = |seed| {
            let mut f = PocketField::new(&cfg(), ground(), seed);
            let at = f.pockets()[0].at;
            f.step(2.5, &[at]);
            for _ in 0..3 {
                f.fill(0);
            }
            f.pockets().iter().map(|p| p.at).collect::<Vec<_>>()
        };
        assert_eq!(run(9), run(9));
    }

    #[test]
    fn a_spent_pocket_is_replaced_by_a_fresh_one_elsewhere() {
        let c = cfg();
        let mut f = PocketField::new(&c, ground(), 3);
        let old = f.pockets()[0].at;
        f.step(c.tap_s + 0.01, &[old]);
        for _ in 0..c.jars {
            f.fill(0);
        }
        let new = &f.pockets()[0];
        assert!(dist(new.at, old) >= c.min_gap_m, "not under the player's feet");
        assert_eq!((new.state, new.jars_left), (PocketState::Hidden, c.jars));
        assert_eq!(f.pockets().len(), c.pockets as usize);
    }

    #[test]
    fn a_full_field_leaves_the_pocket_spent_until_a_place_is_free() {
        let c = PocketConfig { min_gap_m: 100.0, pockets: 1, ..cfg() };
        let mut f = PocketField::new(&c, ground(), 3);
        let at = f.pockets()[0].at;
        f.step(c.tap_s + 0.01, &[at]);
        for _ in 0..c.jars {
            f.fill(0);
        }
        assert_eq!(f.pockets()[0].state, PocketState::Spent, "no place 100 m away");
        assert_eq!(f.signal(at), 0.0, "a spent pocket is silent");
        f.step(1.0, &[at]);
        assert_eq!(f.pockets()[0].state, PocketState::Spent, "and a tap does nothing to it");
    }

    #[test]
    fn a_tap_opens_a_pocket_after_tap_s_only_while_it_is_held_close() {
        let c = cfg();
        let mut f = PocketField::new(&c, ground(), 11);
        let at = f.pockets()[0].at;
        let far = [at[0] + c.tap_radius_m + 0.5, at[1]];
        for _ in 0..100 {
            f.step(0.1, &[far]);
        }
        assert_eq!(f.pockets()[0].state, PocketState::Hidden, "a hold away from it opens nothing");
        f.step(1.5, &[at]);
        assert_eq!(f.pockets()[0].state, PocketState::Hidden, "not yet");
        f.step(2.0, &[]);
        f.step(1.5, &[at]);
        assert_eq!(f.pockets()[0].state, PocketState::Hidden, "the progress ran back when the tap was let go");
        f.step(0.6, &[at]);
        assert!(matches!(f.pockets()[0].state, PocketState::Open { .. }));
    }

    #[test]
    fn an_open_pocket_that_times_out_closes_again_with_its_jars() {
        let c = cfg();
        let mut f = PocketField::new(&c, ground(), 11);
        let at = f.pockets()[0].at;
        f.step(c.tap_s + 0.01, &[at]);
        assert_eq!(f.open_near(at, c.reach_m), Some(0));
        assert_eq!(f.open_near([at[0] + 5.0, at[1]], c.reach_m), None);
        assert_eq!(f.fill(0), Fill::Jar { spent: false });
        f.step(c.open_s + 1.0, &[]);
        let p = &f.pockets()[0];
        assert_eq!((p.state, p.jars_left, p.at), (PocketState::Hidden, c.jars - 1, at));
        assert_eq!(f.open_near(at, c.reach_m), None);
    }

    #[test]
    fn huge_and_bad_steps_are_safe() {
        let c = cfg();
        let mut f = PocketField::new(&c, ground(), 2);
        let at = f.pockets()[0].at;
        for dt in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -5.0, 0.0, 1e300, 1e-300] {
            f.step(dt, &[at, [f64::NAN, 0.0]]);
            for p in f.pockets() {
                assert!(p.tap.is_finite() && (0.0..=c.tap_s).contains(&p.tap));
                if let PocketState::Open { left_s } = p.state {
                    assert!(left_s.is_finite() && left_s > 0.0);
                }
            }
        }
    }

    #[test]
    fn the_config_checks_its_numbers() {
        assert!(cfg().problems().is_empty());
        assert!(PocketConfig { range_m: 0.0, ..cfg() }.problems()[0].contains("range_m"));
        assert!(PocketConfig { jars: 0, ..cfg() }.problems()[0].contains("jars"));
        assert!(PocketConfig { tap_s: f64::NAN, ..cfg() }.problems()[0].contains("tap_s"));
    }
}
