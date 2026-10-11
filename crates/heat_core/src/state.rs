//! The heat state (#227). Reads `Violation`, `ProductOrderSettled`, `TimePassed`,
//! `HeatPaidDown`, `ScanStarted` and `ScanAnswered`; raises `HeatStars`, `ScanResolved`, the
//! wallet `TrackChanged` and the star notices. Everything random comes from the seeded generator
//! in the state, so the same seed and the same events give the same scans.
use gameplay_core::content::WALLET;
use gameplay_core::notice::{Arg, Notice, NoticeKind};
use gameplay_core::rng::Rng;
use gameplay_core::save::{Envelope, SaveError};
use gameplay_core::{Event, ScanChoice, TrackId, WorldEvent};
use serde::{Deserialize, Serialize};

use crate::tuning::{CLEAN_AFTER_S, HeatContent, NOTICE_DOWN, NOTICE_UP, STARS_MAX, Tuning};

pub const SECTION: &str = "heat";
pub const SECTION_VERSION: u32 = 1;

/// What `apply_world` gives back: a domain event for every system (the host gives it an id and
/// applies it), or a notice for the player.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Emit(WorldEvent),
    Notice(Notice),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct OpenScan {
    scan: u64,
    visible: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Heat {
    rng: Rng,
    /// 0..=max_points.
    points: f64,
    /// Kept in step with `points` by `set_points`, so `stars()` needs no tuning.
    stars: u8,
    /// The last scan was clean, or nothing illegal has happened since. Starts clean.
    clean_scan: bool,
    /// Seconds since the last illegal thing (a violation, a deal, a scan that was not clean).
    since_illegal_s: f64,
    /// The scan waiting for an answer. Only one at a time: a second start is ignored.
    open: Option<OpenScan>,
}

impl Heat {
    pub fn new(seed: u64) -> Heat {
        Heat { rng: Rng::new(seed), points: 0.0, stars: 0, clean_scan: true, since_illegal_s: 0.0, open: None }
    }

    pub fn points(&self) -> f64 {
        self.points
    }

    pub fn stars(&self) -> u8 {
        self.stars
    }

    /// The scan waiting for an answer, if any.
    pub fn open_scan(&self) -> Option<u64> {
        self.open.as_ref().map(|o| o.scan)
    }

    /// Whether the clean decay rate applies now: the last scan was clean, or 5 minutes passed
    /// without anything illegal.
    pub fn is_clean(&self) -> bool {
        self.clean_scan || self.since_illegal_s >= CLEAN_AFTER_S
    }

    /// The chance that an official pad scans the crew. An inspection pad is certain (whatever the
    /// jammer does). The jammer multiplies a normal chance by `jammer_factor`.
    pub fn scan_chance(t: &Tuning, stars: u8, inspection: bool, jammer: bool) -> f64 {
        if inspection {
            return 1.0;
        }
        let base = t.scan_chance[usize::from(stars.min(STARS_MAX))];
        if jammer { base * t.jammer_factor } else { base }
    }

    /// Whether the official pad scans the ship now. Always draws once, so the rolls stay in step.
    pub fn roll_scan(&mut self, t: &Tuning, inspection: bool, jammer: bool) -> bool {
        let chance = Heat::scan_chance(t, self.stars, inspection, jammer);
        self.rng.unit() < chance
    }

    pub fn apply_world(&mut self, c: &HeatContent, ev: &Event<WorldEvent>) -> Vec<Outcome> {
        let t = &c.tuning.record;
        match &ev.payload {
            WorldEvent::Violation { kind, .. } => self.illegal(t, t.violation_points.of(*kind)),
            WorldEvent::ProductOrderSettled { .. } => self.illegal(t, t.deal_points),
            WorldEvent::TimePassed { dt } => self.time(t, *dt),
            WorldEvent::HeatPaidDown { amount } => self.pay(t, *amount),
            WorldEvent::ScanStarted { scan, visible, hidden, .. } => self.start_scan(t, *scan, *visible, *hidden),
            WorldEvent::ScanAnswered { scan, choice } => self.answer(t, *scan, *choice),
            _ => Vec::new(),
        }
    }

    /// Something illegal: points up, the quiet time starts again.
    fn illegal(&mut self, t: &Tuning, points: f64) -> Vec<Outcome> {
        self.clean_scan = false;
        self.since_illegal_s = 0.0;
        self.set_points(t, self.points + points)
    }

    /// Decay at the rate of the moment. The quiet time counts before this tick: the rate is the
    /// one from before `dt`, so five quiet minutes switch it at the next tick.
    fn time(&mut self, t: &Tuning, dt: f64) -> Vec<Outcome> {
        if !(dt > 0.0) {
            return Vec::new();
        }
        let rate = if self.is_clean() { t.decay_per_min_clean } else { t.decay_per_min };
        self.since_illegal_s += dt;
        self.set_points(t, self.points - rate * dt / 60.0)
    }

    /// Paying a fixer: `amount` money takes `amount / paydown_per_point` points off. The amount is
    /// charged as given (TODO(initiator): charge only what is owed?).
    fn pay(&mut self, t: &Tuning, amount: i64) -> Vec<Outcome> {
        if amount <= 0 {
            return Vec::new();
        }
        let mut out = vec![Outcome::Emit(WorldEvent::TrackChanged { track: TrackId::new(WALLET), delta: -amount, player: None })];
        out.extend(self.set_points(t, self.points - amount as f64 / t.paydown_per_point as f64));
        out
    }

    /// A scan on a pad. From `thorough_from_stars` a seeded roll may count the hidden hold as
    /// visible. Nothing visible then: a clean scan at once. Otherwise it waits for an answer.
    fn start_scan(&mut self, t: &Tuning, scan: u64, visible: u32, hidden: u32) -> Vec<Outcome> {
        if self.open.is_some() {
            return Vec::new();
        }
        let found = hidden > 0 && self.stars >= t.thorough_from_stars && self.rng.unit() < t.thorough_chance;
        let visible = if found { visible + hidden } else { visible };
        if visible > 0 {
            self.open = Some(OpenScan { scan, visible });
            return Vec::new();
        }
        self.clean_scan = true;
        let mut out = vec![Outcome::Emit(WorldEvent::ScanResolved { scan, choice: ScanChoice::Comply, confiscate: false, fine: 0, heat: t.comply_points })];
        out.extend(self.set_points(t, self.points + t.comply_points));
        out
    }

    /// The answer to the open scan. An answer to any other or a closed scan does nothing.
    fn answer(&mut self, t: &Tuning, scan: u64, choice: ScanChoice) -> Vec<Outcome> {
        let open = match &self.open {
            Some(o) if o.scan == scan => o.clone(),
            _ => return Vec::new(),
        };
        self.open = None;
        let base_fine = t.fine_base + t.fine_per_crate * i64::from(open.visible);
        let stars = self.stars;
        let (confiscate, fine, heat) = match choice {
            ScanChoice::Comply => (true, base_fine, t.comply_points),
            ScanChoice::Bribe => (false, t.bribe_base * (1 + i64::from(stars)), 0.0),
            ScanChoice::Talk => {
                let won = self.rng.unit() < t.talk_chance[usize::from(stars.min(STARS_MAX))];
                if won {
                    (false, 0, 0.0)
                } else {
                    (true, (base_fine as f64 * t.talk_fail_fine_factor).round() as i64, t.talk_fail_points)
                }
            }
            ScanChoice::Flee => (false, 0, t.flee_points),
        };
        // Clean: nothing taken, no fine, and no flight. Anything else is illegal.
        let clean = !confiscate && fine == 0 && choice != ScanChoice::Flee;
        self.clean_scan = clean;
        if !clean {
            self.since_illegal_s = 0.0;
        }
        let mut out = vec![Outcome::Emit(WorldEvent::ScanResolved { scan, choice, confiscate, fine, heat })];
        if fine > 0 {
            out.push(Outcome::Emit(WorldEvent::TrackChanged { track: TrackId::new(WALLET), delta: -fine, player: None }));
        }
        out.extend(self.set_points(t, self.points + heat));
        out
    }

    /// Sets the points, clamped to 0..=max_points, and raises the star change with its notice.
    fn set_points(&mut self, t: &Tuning, new: f64) -> Vec<Outcome> {
        self.points = new.clamp(0.0, t.max_points);
        let stars = ((self.points / t.points_per_star).floor() as u8).min(STARS_MAX);
        if stars == self.stars {
            return Vec::new();
        }
        let (kind, key) = if stars > self.stars { (NoticeKind::Warning, NOTICE_UP) } else { (NoticeKind::Updated, NOTICE_DOWN) };
        self.stars = stars;
        vec![
            Outcome::Emit(WorldEvent::HeatStars { stars }),
            Outcome::Notice(Notice::new(kind, key).arg("stars", Arg::Number(i64::from(stars)))),
        ]
    }

    pub fn save(&self, env: &mut Envelope) {
        env.put(SECTION, SECTION_VERSION, self);
    }

    /// None if the save has no heat section; an error for a version this build does not read.
    pub fn load(env: &Envelope) -> Result<Option<Heat>, SaveError> {
        env.get(SECTION, SECTION_VERSION)
    }
}
