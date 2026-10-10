//! The map's pin list of #166, without Bevy: places the crew may use, the tracked job's next stop,
//! the players.
use std::path::Path;

use gameplay_core::{ClientId, Content, CrateId, Event, File, LocationId, Progress, WorldEvent};
use jobs_core::map::*;
use jobs_core::*;

const HOST: ClientId = ClientId(1);
const ANA: ClientId = ClientId(7);

fn files(dir: &str) -> Vec<File> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut out = Vec::new();
    for d in std::fs::read_dir(&root).unwrap() {
        let d = d.unwrap().path();
        for f in std::fs::read_dir(&d).unwrap() {
            let f = f.unwrap().path();
            out.push(File::new(f.strip_prefix(&root).unwrap().to_str().unwrap(), std::fs::read_to_string(&f).unwrap()));
        }
    }
    out
}

struct Host {
    k: Content,
    jc: JobContent,
    progress: Progress,
    jobs: Jobs,
    seq: u64,
    next_crate: u64,
}

impl Host {
    fn new() -> Host {
        let k = Content::load(&files("../gameplay_core/tests/fixtures/valid"), &["small", "medium", "large"]).unwrap();
        let jc = JobContent::load(&files("tests/fixtures"), &k).unwrap();
        let progress = Progress::new(&k);
        Host { k, jc, progress, jobs: Jobs::default(), seq: 0, next_crate: 1 }
    }

    fn world(&mut self, e: WorldEvent) {
        self.seq += 1;
        let out = self.jobs.apply_world(&self.jc, &Event::new(ANA, self.seq, e));
        self.route(out);
    }

    fn route(&mut self, out: Vec<Outcome>) {
        for o in out {
            if let Outcome::SpawnCrates { job, leg, count, .. } = o {
                let crates = (0..count).map(|_| {
                    self.next_crate += 1;
                    CrateId(self.next_crate - 1)
                }).collect();
                self.seq += 1;
                self.jobs.apply_job(&self.jc, &self.k, &self.progress, &Event::new(HOST, self.seq, JobEvent::CratesSpawned { job, leg, crates })).unwrap();
            }
        }
    }

    /// First haul: drip_rock to bent_spoon, 4 crates. Accepted.
    fn active_job(&mut self) -> JobId {
        let t = self.jc.templates[&TemplateId::new("first_haul")].record.clone();
        let id = self.jobs.offer_fixed(&t).unwrap();
        self.seq += 1;
        let out = self.jobs.apply_job(&self.jc, &self.k, &self.progress, &Event::new(ANA, self.seq, JobEvent::OfferAccepted { job: id })).unwrap();
        self.route(out);
        id
    }

    fn crates(&self, j: JobId) -> Vec<CrateId> {
        self.jobs.get(j).unwrap().legs[0].crates.keys().copied().collect()
    }

    fn pins(&self, tracked: Option<JobId>, places: &[PlacePos], players: &[PlayerPos], planet: u32) -> Vec<Pin> {
        pins(&self.k, &self.progress, &self.jobs, tracked, places, players, ANA, planet)
    }
}

fn place(l: &str, planet: u32, x: f64) -> PlacePos {
    PlacePos { location: LocationId::new(l), planet, dir: [x, 1.0, 0.0] }
}

fn places() -> Vec<PlacePos> {
    vec![place("drip_rock", 0, 0.0), place("bent_spoon", 0, 0.1), place("wholesale_yard", 1, 0.2), place("moon_base", 0, 0.3)]
}

fn kinds(p: &[Pin]) -> Vec<(PinKind, String)> {
    p.iter().map(|p| (p.kind.clone(), p.label.to_string())).collect()
}

#[test]
fn places_the_crew_may_use_on_this_planet_are_pinned_and_unknown_or_foreign_ones_are_not() {
    let mut h = Host::new();
    let p = h.pins(None, &places(), &[], 0);
    // drip_rock is open; bent_spoon needs the permit; wholesale_yard is on another planet; moon_base is unknown.
    assert_eq!(kinds(&p), vec![(PinKind::Place, "location.drip_rock.name".to_string())]);
    h.progress.apply(&h.k, &Event::new(ANA, 1, WorldEvent::UnlockBought { unlock: gameplay_core::UnlockId::new("bent_spoon_permit") })).unwrap();
    let p = h.pins(None, &places(), &[], 0);
    assert_eq!(p.len(), 2, "the permit opens Bent Spoon");
    let on_other = h.pins(None, &places(), &[], 1);
    assert_eq!(kinds(&on_other), vec![(PinKind::Place, "location.wholesale_yard.name".to_string())]);
}

#[test]
fn the_tracked_jobs_pin_moves_from_the_pickup_to_the_dropoff_when_the_crates_are_carried_off() {
    let mut h = Host::new();
    let j = h.active_job();
    let target = |h: &Host| h.pins(Some(j), &places(), &[], 0).into_iter().find(|p| matches!(p.kind, PinKind::Target(_))).map(|p| (p.kind, p.label.to_string(), p.dir));
    // Crates wait at the pickup.
    assert_eq!(target(&h).map(|t| (t.0, t.1)), Some((PinKind::Target(Stop::Pickup), "location.drip_rock.name".to_string())));
    let crates = h.crates(j);
    for c in &crates[..3] {
        h.world(WorldEvent::CratePickedUp { crate_id: *c, at: LocationId::new("drip_rock") });
    }
    assert_eq!(target(&h).map(|t| t.0), Some(PinKind::Target(Stop::Pickup)), "one more crate is still to pick up");
    h.world(WorldEvent::CratePickedUp { crate_id: crates[3], at: LocationId::new("drip_rock") });
    let t = target(&h).unwrap();
    assert_eq!((t.0, t.1.as_str()), (PinKind::Target(Stop::Dropoff), "location.bent_spoon.name"), "all carried: the next stop is the dropoff");
    assert_eq!(t.2, [0.1, 1.0, 0.0], "at the dropoff's position, even before the permit opens its pin");
    // Delivered: the job is over, no target.
    for c in &crates {
        h.world(WorldEvent::CrateDelivered { crate_id: *c, at: LocationId::new("bent_spoon"), condition: 1.0 });
    }
    assert_eq!(target(&h), None);
}

#[test]
fn a_crate_set_down_elsewhere_sends_the_pin_back_to_the_pickup() {
    let mut h = Host::new();
    let j = h.active_job();
    let crates = h.crates(j);
    for c in &crates {
        h.world(WorldEvent::CratePickedUp { crate_id: *c, at: LocationId::new("drip_rock") });
    }
    h.world(WorldEvent::CrateDelivered { crate_id: crates[0], at: LocationId::new("wholesale_yard"), condition: 1.0 });
    let t = h.pins(Some(j), &places(), &[], 0).into_iter().find(|p| matches!(p.kind, PinKind::Target(_))).unwrap();
    assert_eq!(t.kind, PinKind::Target(Stop::Pickup), "one crate waits again");
    assert_eq!(next_stop(h.jobs.get(j).unwrap()).map(|s| s.1), Some(Stop::Pickup));
}

#[test]
fn an_untracked_offered_or_foreign_target_is_not_pinned() {
    let mut h = Host::new();
    let j = h.active_job();
    assert!(h.pins(None, &places(), &[], 0).iter().all(|p| !matches!(p.kind, PinKind::Target(_))));
    // The target's place on another planet is not pinned on this one.
    let foreign = vec![place("drip_rock", 1, 0.0), place("bent_spoon", 1, 0.1)];
    assert!(h.pins(Some(j), &foreign, &[], 0).iter().all(|p| !matches!(p.kind, PinKind::Target(_))));
    assert!(h.pins(Some(j), &foreign, &[], 1).iter().any(|p| matches!(p.kind, PinKind::Target(_))));
    // An offer that nobody has taken is not tracked.
    let t = h.jc.templates[&TemplateId::new("first_haul")].record.clone();
    let offered = h.jobs.offer_fixed(&t).unwrap();
    assert!(h.pins(Some(offered), &places(), &[], 0).iter().all(|p| !matches!(p.kind, PinKind::Target(_))));
    assert!(h.pins(Some(JobId(999)), &places(), &[], 0).iter().all(|p| !matches!(p.kind, PinKind::Target(_))));
}

#[test]
fn players_are_pinned_on_their_planet() {
    let h = Host::new();
    let players = vec![
        PlayerPos { who: ANA, planet: 0, dir: [0.0, 1.0, 0.0] },
        PlayerPos { who: HOST, planet: 0, dir: [0.2, 1.0, 0.0] },
        PlayerPos { who: ClientId(9), planet: 1, dir: [0.0, 1.0, 0.0] },
    ];
    let p = h.pins(None, &[], &players, 0);
    let kinds: Vec<PinKind> = p.iter().map(|p| p.kind.clone()).collect();
    assert_eq!(kinds, vec![PinKind::Player { own: true }, PinKind::Player { own: false }], "own first, a crewmate on another planet not shown");
}

#[test]
fn pins_come_in_a_fixed_order_places_then_target_then_players() {
    let mut h = Host::new();
    let j = h.active_job();
    h.progress.apply(&h.k, &Event::new(ANA, 1, WorldEvent::UnlockBought { unlock: gameplay_core::UnlockId::new("bent_spoon_permit") })).unwrap();
    let players = vec![PlayerPos { who: ANA, planet: 0, dir: [0.0, 1.0, 0.0] }];
    let p = h.pins(Some(j), &places(), &players, 0);
    let order: Vec<u8> = p.iter().map(|p| match p.kind {
        PinKind::Place => 0,
        PinKind::Target(_) => 1,
        PinKind::Player { .. } => 2,
    }).collect();
    assert_eq!(order, vec![0, 0, 1, 2]);
    assert_eq!(p, h.pins(Some(j), &places(), &players, 0), "stable");
}
