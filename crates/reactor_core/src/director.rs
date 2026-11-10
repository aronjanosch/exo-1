//! The incident director (#328): things on the ship break by themselves, one at a time, while the crew is busy.
//! Two kinds only: a junction box faults (it breaks at once, sparks, maybe a small fire), or a machine catches fire
//! (the reactor or the coolant pump). Everything the older director could do (pump break, surge, hull breach,
//! hits) stays parked; the room's `Fault`s are still there for the dev menu.
//!
//! Pacing: the first incident after `first_s`, then a random wait between `min_interval_s` and `max_interval_s`.
//! The wait only runs down while the ship is calm: never a new incident while a box is blown, a fire burns, or the
//! reactor's implosion countdown runs. Seeded, so the same seed gives the same sequence.
//!
//! The older intensity model (Barotrauma's `EventManager` threshold, port of r38) is dropped: "calm" is those three
//! plain rules, so there are fewer knobs. TODO(initiator): say if the intensity should come back.
use gameplay_core::rng::Rng;
use serde::{Deserialize, Serialize};

use crate::engine_room::{Source, JUNCTIONS};

/// `director` in incidents/director.json. Start values, TODO(initiator).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorTuning {
    /// On at the start (the dev menu's "Director: on/off" sets it in play).
    pub enabled: bool,
    /// Calm seconds before the first incident.
    pub first_s: f64,
    /// The random wait between incidents, calm seconds.
    pub min_interval_s: f64,
    pub max_interval_s: f64,
    /// A faulting junction box starts a small fire with this chance, 0..1.
    pub fire_chance: f64,
}

impl Default for DirectorTuning {
    fn default() -> DirectorTuning {
        DirectorTuning { enabled: false, first_s: 180.0, min_interval_s: 150.0, max_interval_s: 330.0, fire_chance: 0.25 }
    }
}

/// What the director does to the room; the host turns it into the room's calls and events.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Incident {
    /// Box `index` breaks at once (`EngineRoom::blow_junction`); with `fire`, a small fire starts at its spark point.
    JunctionFault { index: usize, fire: bool },
    /// A small fire at the machine's ignition spot: `Source::Reactor` or `Source::Pump`.
    MachineFire(Source),
}

/// What the ship reports to the director.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Situation {
    /// Each box is blown (broke, no new fuse yet).
    pub blown: [bool; JUNCTIONS],
    /// Fires burning anywhere the crew goes.
    pub fires: u32,
    /// The reactor's implosion timer runs (aboard or outside).
    pub countdown: bool,
}

impl Situation {
    /// An incident is open or the ship is in a worse trouble: nothing new starts.
    pub fn busy(&self) -> bool {
        self.fires > 0 || self.countdown || self.blown.iter().any(|b| *b)
    }
}

/// The director's state as a save holds it (the tuning comes back from the values).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirectorSave {
    pub enabled: bool,
    /// Calm seconds until the next incident.
    pub timer: f64,
    pub rng: Rng,
}

#[derive(Debug)]
pub struct Director {
    cfg: DirectorTuning,
    rng: Rng,
    enabled: bool,
    /// Calm seconds until the next incident.
    timer: f64,
}

impl Director {
    pub fn new(cfg: &DirectorTuning, seed: u64) -> Director {
        Director { cfg: cfg.clone(), rng: Rng::new(seed), enabled: cfg.enabled, timer: cfg.first_s }
    }

    pub fn to_save(&self) -> DirectorSave {
        DirectorSave { enabled: self.enabled, timer: self.timer, rng: self.rng.clone() }
    }

    pub fn from_save(cfg: &DirectorTuning, s: &DirectorSave) -> Director {
        Director { cfg: cfg.clone(), rng: s.rng.clone(), enabled: s.enabled, timer: s.timer }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
    }

    /// Calm seconds until the next incident.
    pub fn timer(&self) -> f64 {
        self.timer
    }

    /// Steps `dt` seconds. Off: nothing moves. The wait runs down only while the ship is calm; the incident that
    /// starts now, if any.
    pub fn step(&mut self, dt: f64, s: &Situation) -> Option<Incident> {
        if !self.enabled || s.busy() {
            return None;
        }
        self.timer -= dt;
        if self.timer > 0.0 {
            return None;
        }
        self.timer = self.next_interval();
        Some(self.pick(s))
    }

    /// The dev menu's "force an incident": one now, whatever the pacing and whatever is open, and the wait starts over.
    pub fn force(&mut self, s: &Situation) -> Incident {
        self.timer = self.next_interval();
        self.pick(s)
    }

    /// A box that is not blown or a machine fire, equal chance; with every box blown, a machine fire.
    fn pick(&mut self, s: &Situation) -> Incident {
        let sound: Vec<usize> = (0..JUNCTIONS).filter(|i| !s.blown[*i]).collect();
        if !sound.is_empty() && self.rng.below(2) == 0 {
            let index = sound[self.rng.below(sound.len())];
            let fire = self.rng.unit() < self.cfg.fire_chance;
            Incident::JunctionFault { index, fire }
        } else {
            Incident::MachineFire(if self.rng.below(2) == 0 { Source::Reactor } else { Source::Pump })
        }
    }

    fn next_interval(&mut self) -> f64 {
        let u = self.rng.unit();
        self.cfg.min_interval_s + (self.cfg.max_interval_s - self.cfg.min_interval_s) * u
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> DirectorTuning {
        DirectorTuning { enabled: true, first_s: 180.0, min_interval_s: 150.0, max_interval_s: 330.0, fire_chance: 0.25 }
    }

    /// A little ship for the rules: an incident opens a box or a fire that the "crew" closes after `fix_s`.
    struct Ship {
        s: Situation,
        open_for: f64,
    }

    impl Ship {
        fn apply(&mut self, i: Incident, fix_s: f64) {
            match i {
                Incident::JunctionFault { index, .. } => self.s.blown[index] = true,
                Incident::MachineFire(_) => self.s.fires += 1,
            }
            self.open_for = fix_s;
        }

        fn tick(&mut self, dt: f64) {
            if self.open_for > 0.0 {
                self.open_for -= dt;
                if self.open_for <= 0.0 {
                    self.s = Situation::default();
                }
            }
        }
    }

    /// Runs `secs` in 0.5 s steps; each incident stays open `fix_s`. Returns the incidents with their times and the largest
    /// number open at once.
    fn run(d: &mut Director, secs: f64, fix_s: f64) -> (Vec<(f64, Incident)>, usize) {
        let mut ship = Ship { s: Situation::default(), open_for: 0.0 };
        let (mut out, mut most, mut t) = (Vec::new(), 0, 0.0);
        while t < secs {
            t += 0.5;
            ship.tick(0.5);
            if let Some(i) = d.step(0.5, &ship.s) {
                let open = ship.s.fires as usize + ship.s.blown.iter().filter(|b| **b).count();
                most = most.max(open + 1);
                out.push((t, i));
                ship.apply(i, fix_s);
            }
        }
        (out, most)
    }

    #[test]
    fn off_does_nothing() {
        let mut d = Director::new(&DirectorTuning { enabled: false, ..cfg() }, 1);
        assert!(run(&mut d, 5000.0, 10.0).0.is_empty());
    }

    #[test]
    fn no_incident_before_the_first_time_and_one_right_after() {
        let mut d = Director::new(&cfg(), 1);
        let (got, _) = run(&mut d, 179.0, 10.0);
        assert!(got.is_empty(), "{got:?}");
        let (got, _) = run(&mut d, 2.0, 10.0);
        assert_eq!(got.len(), 1, "{got:?}");
    }

    #[test]
    fn never_two_open_at_once_and_the_wait_is_inside_the_interval() {
        let mut d = Director::new(&cfg(), 5);
        let (got, most) = run(&mut d, 6000.0, 60.0);
        assert!(got.len() >= 10, "{got:?}");
        assert_eq!(most, 1, "one incident open at a time");
        for w in got.windows(2) {
            let gap = w[1].0 - w[0].0;
            // The incident stays open 60 s; the wait counts only after it is closed.
            assert!(gap >= 150.0 + 60.0 - 1.0 && gap <= 330.0 + 60.0 + 1.0, "{gap}");
        }
    }

    #[test]
    fn an_open_incident_holds_the_next_one_however_long() {
        let mut d = Director::new(&cfg(), 2);
        let open = Situation { blown: [false, true, false], ..Default::default() };
        for _ in 0..10_000 {
            assert!(d.step(1.0, &open).is_none());
        }
        let fire = Situation { fires: 1, ..Default::default() };
        assert!(d.step(1000.0, &fire).is_none());
    }

    #[test]
    fn no_incident_while_the_countdown_runs() {
        let mut d = Director::new(&cfg(), 3);
        let counting = Situation { countdown: true, ..Default::default() };
        for _ in 0..2000 {
            assert!(d.step(1.0, &counting).is_none());
        }
        // The wait did not run down meanwhile: the full first wait is still ahead.
        assert!(d.step(100.0, &Situation::default()).is_none());
        assert!(d.step(100.0, &Situation::default()).is_some());
    }

    #[test]
    fn the_same_seed_gives_the_same_sequence() {
        let a = run(&mut Director::new(&cfg(), 7), 8000.0, 30.0).0;
        let b = run(&mut Director::new(&cfg(), 7), 8000.0, 30.0).0;
        let c = run(&mut Director::new(&cfg(), 8), 8000.0, 30.0).0;
        assert_eq!(a, b);
        assert!(a.len() >= 15 && a != c, "{a:?}");
        assert!(a.iter().any(|(_, i)| matches!(i, Incident::JunctionFault { .. })) && a.iter().any(|(_, i)| matches!(i, Incident::MachineFire(_))));
    }

    #[test]
    fn a_fault_picks_only_a_box_that_is_not_blown_and_all_blown_means_fire() {
        let mut d = Director::new(&cfg(), 9);
        let some = Situation { blown: [true, false, true], ..Default::default() };
        for _ in 0..200 {
            if let Incident::JunctionFault { index, .. } = d.force(&some) {
                assert_eq!(index, 1);
            }
        }
        let all = Situation { blown: [true; JUNCTIONS], ..Default::default() };
        for _ in 0..50 {
            assert!(matches!(d.force(&all), Incident::MachineFire(_)));
        }
    }

    #[test]
    fn the_fire_chance_is_the_configured_one() {
        let mut d = Director::new(&DirectorTuning { fire_chance: 0.0, ..cfg() }, 4);
        assert!((0..300).all(|_| !matches!(d.force(&Situation::default()), Incident::JunctionFault { fire: true, .. })));
        let mut d = Director::new(&DirectorTuning { fire_chance: 1.0, ..cfg() }, 4);
        let faults: Vec<_> = (0..300).filter_map(|_| match d.force(&Situation::default()) { Incident::JunctionFault { fire, .. } => Some(fire), _ => None }).collect();
        assert!(!faults.is_empty() && faults.iter().all(|f| *f));
    }

    #[test]
    fn a_save_goes_on_where_it_left_off() {
        let mut a = Director::new(&cfg(), 6);
        let _ = run(&mut a, 400.0, 20.0);
        let mut b = Director::from_save(&cfg(), &a.to_save());
        assert_eq!(run(&mut a, 3000.0, 20.0).0, run(&mut b, 3000.0, 20.0).0);
    }
}
