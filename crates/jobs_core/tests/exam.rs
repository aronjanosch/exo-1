//! The flight licence exam of #169: a job template with checks (take off, reach a pad, land) next to
//! the delivery, a fee, grades (pass, honours), retries at half price, a personal licence track.
//! The checks read only the events `TookOff`, `PadReached` and `Landed`; nothing of how the ship flies.
use std::path::Path;

use gameplay_core::notice::NoticeKind;
use gameplay_core::{ClientId, Content, CrateId, Event, File, Flag, LocationId, Progress, TrackId, WorldEvent};
use jobs_core::*;

const HOST: ClientId = ClientId(1);
const ANA: ClientId = ClientId(7);
const BO: ClientId = ClientId(8);

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

fn kernel() -> Content {
    Content::load(&files("../gameplay_core/tests/fixtures/valid"), &["small", "medium", "large"]).unwrap()
}

fn replace(path: &str, from: &str, to: &str) -> Vec<String> {
    let orig = files("tests/fixtures").into_iter().find(|f| f.path == path).unwrap();
    assert!(orig.text.contains(from), "{from} in {path}");
    let mut f: Vec<File> = files("tests/fixtures").into_iter().filter(|f| f.path != path).collect();
    f.push(File::new(path, orig.text.replace(from, to)));
    JobContent::load(&f, &kernel()).unwrap_err()
}

struct Host {
    k: Content,
    jc: JobContent,
    progress: Progress,
    jobs: Jobs,
    seq: u64,
    next_crate: u64,
    notices: Vec<gameplay_core::notice::Notice>,
}

impl Host {
    fn new() -> Host {
        let k = kernel();
        let jc = JobContent::load(&files("tests/fixtures"), &k).unwrap();
        let mut progress = Progress::new(&k);
        for who in [ANA, BO] {
            progress.apply(&k, &Event::new(who, 0, WorldEvent::PlayerJoined)).unwrap();
        }
        // Enough for a few tries.
        progress.apply(&k, &Event::new(HOST, 0, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 800, player: None })).unwrap();
        Host { k, jc, progress, jobs: Jobs::default(), seq: 100, next_crate: 1, notices: Vec::new() }
    }

    fn licence(&self, who: ClientId) -> i64 {
        self.progress.value(&self.k, &TrackId::new("licence_flight"), Some(who)).unwrap()
    }

    fn standing(&self) -> i64 {
        self.progress.value(&self.k, &TrackId::new("standing_courier"), None).unwrap()
    }

    fn world(&mut self, who: ClientId, e: WorldEvent) {
        self.seq += 1;
        let ev = Event::new(who, self.seq, e);
        self.progress.apply(&self.k, &ev).unwrap();
        let out = self.jobs.apply_world(&self.jc, &ev);
        self.route(out);
    }

    fn route(&mut self, out: Vec<Outcome>) {
        for o in out {
            match o {
                Outcome::SpawnCrates { job, leg, count, .. } => {
                    let crates = (0..count).map(|_| {
                        self.next_crate += 1;
                        CrateId(self.next_crate - 1)
                    }).collect();
                    self.seq += 1;
                    let ev = Event::new(HOST, self.seq, JobEvent::CratesSpawned { job, leg, crates });
                    self.jobs.apply_job(&self.jc, &self.k, &self.progress, &ev).unwrap();
                }
                Outcome::Emit(e) => self.world(HOST, e),
                Outcome::Notice(n) => self.notices.push(n),
                _ => {}
            }
        }
    }

    fn offer(&mut self) -> JobId {
        let t = self.jc.templates[&TemplateId::new("flight_exam")].record.clone();
        self.jobs.offer_fixed(&t).expect("an exam is fully named")
    }

    fn accept(&mut self, who: ClientId, job: JobId) -> Result<(), Refusal> {
        self.seq += 1;
        let out = self.jobs.apply_job(&self.jc, &self.k, &self.progress, &Event::new(who, self.seq, JobEvent::OfferAccepted { job }))?;
        self.route(out);
        Ok(())
    }

    fn fly(&mut self, who: ClientId, speed: f64) {
        self.world(who, WorldEvent::TookOff);
        self.world(who, WorldEvent::PadReached { at: LocationId::new("bent_spoon") });
        self.world(who, WorldEvent::Landed { at: Some(LocationId::new("bent_spoon")), speed });
    }

    fn deliver(&mut self, job: JobId, who: ClientId, cond: f64) {
        let c = *self.jobs.get(job).unwrap().legs[0].crates.keys().next().unwrap();
        self.world(who, WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new("drip_rock") });
        self.world(who, WorldEvent::CrateDelivered { crate_id: c, at: LocationId::new("bent_spoon"), condition: cond });
    }

    fn keys(&self) -> Vec<String> {
        self.notices.iter().map(|n| n.key.to_string()).collect()
    }

    fn state(&self, job: JobId) -> JobState {
        self.jobs.get(job).unwrap().state
    }
}

// ---------- loader ----------

#[test]
fn the_exam_and_the_licence_load() {
    let h = Host::new();
    let t = &h.jc.templates[&TemplateId::new("flight_exam")].record;
    assert_eq!(t.objectives.len(), 4);
    let e = t.exam.as_ref().unwrap();
    assert_eq!((e.fee, e.retry_fee, e.grants.as_str()), (150, 75, "licence_flight"));
    let l = &h.jc.licences[&LicenceId::new("flight")].record;
    assert_eq!((l.track.as_str(), l.exam.as_str()), ("licence_flight", "flight_exam"));
    assert_eq!(h.jc.licences_allowing("pilot_ship").len(), 1);
    assert!(h.jc.licences_allowing("fly_to_the_moon").is_empty());
}

#[test]
fn loader_errors_name_file_and_field() {
    let p = "job_template/flight_exam.json";
    for (from, to, field) in [
        ("\"grants\": \"licence_flight\"", "\"grants\": \"licence_nothing\"", "exam.grants"),
        ("\"giver\": \"courier\"", "\"giver\": \"nobody\"", "exam.honours.giver"),
        ("\"max_mps\": 6.0", "\"max_mps\": 0.0", "max_mps"),
        ("\"at\": \"bent_spoon\" } },\n    { \"land\"", "\"at\": \"moon_base\" } },\n    { \"land\"", "reach_pad"),
        ("\"min_condition\": 0.8", "\"min_condition\": 1.5", "min_condition"),
        ("\"fee\": 150", "\"fee\": -1", "fee"),
        ("\"retry_fee\": 75", "\"retry_fee\": 500", "retry_fee"),
        ("\"reward\": 0", "\"reward\": -5", "reward"),
    ] {
        let e = replace(p, from, to);
        assert!(e.iter().any(|m| m.starts_with(p) && m.contains(field)), "{field}: {e:?}");
    }
    // A job that is no exam still needs a positive reward.
    let orig = files("tests/fixtures").into_iter().find(|f| f.path == "job_template/slow_mud.json").unwrap();
    let mut f: Vec<File> = files("tests/fixtures").into_iter().filter(|f| f.path != orig.path).collect();
    f.push(File::new(&orig.path, orig.text.replace("\"reward\": 100", "\"reward\": 0")));
    let e = JobContent::load(&f, &kernel()).unwrap_err();
    assert!(e.iter().any(|m| m.contains("reward")), "{e:?}");

    let p = "licence/flight.json";
    for (from, to, field) in [("\"flight_exam\"", "\"no_exam\"", "exam"), ("\"licence_flight\"", "\"freight_xp\"", "track"), ("\"licence_flight\"", "\"wallet\"", "track")] {
        let e = replace(p, from, to);
        assert!(e.iter().any(|m| m.starts_with(p) && m.contains(field)), "{field}: {e:?}");
    }
}

// ---------- the exam ----------

#[test]
fn a_passed_exam_charges_the_fee_and_sets_the_personal_licence() {
    let mut h = Host::new();
    let wallet = h.progress.wallet();
    let j = h.offer();
    h.accept(ANA, j).unwrap();
    assert_eq!(h.progress.wallet(), wallet - 150, "the fee");
    assert_eq!(h.licence(ANA), 0);
    h.fly(ANA, 4.0);
    assert_eq!(h.state(j), JobState::Active, "the crate is still to deliver");
    h.deliver(j, ANA, 1.0);
    assert_eq!(h.state(j), JobState::Completed);
    assert_eq!(h.licence(ANA), 1, "the pilot has the licence");
    assert_eq!(h.licence(BO), 0, "per player: the other has not");
    assert!(h.keys().contains(&"notice.exam.passed".to_string()) && !h.keys().contains(&"notice.exam.honours".to_string()), "{:?}", h.keys());
    assert_eq!(h.standing(), 0, "no honours, no bonus");
    assert!(h.progress.has_flag(&Flag::new("job_completed:flight_exam")));
}

#[test]
fn honours_for_a_soft_landing_in_good_time_give_a_standing_bonus() {
    let mut h = Host::new();
    let j = h.offer();
    h.accept(ANA, j).unwrap();
    h.fly(ANA, 1.5);
    h.deliver(j, ANA, 1.0);
    assert_eq!(h.state(j), JobState::Completed);
    assert!(h.keys().contains(&"notice.exam.honours".to_string()), "{:?}", h.keys());
    assert!(h.progress.has_flag(&Flag::new("exam_honours:flight_exam")));
    assert_eq!(h.standing(), 10, "the honours bonus with the courier giver");
    assert_eq!(h.licence(ANA), 1);
}

#[test]
fn a_soft_landing_that_took_too_long_is_a_plain_pass() {
    let mut h = Host::new();
    let j = h.offer();
    h.accept(ANA, j).unwrap();
    h.world(ANA, WorldEvent::TookOff);
    h.world(HOST, WorldEvent::TimePassed { dt: 200.0 });
    h.world(ANA, WorldEvent::PadReached { at: LocationId::new("bent_spoon") });
    h.world(ANA, WorldEvent::Landed { at: Some(LocationId::new("bent_spoon")), speed: 1.0 });
    h.deliver(j, ANA, 1.0);
    assert_eq!(h.state(j), JobState::Completed);
    assert!(!h.keys().contains(&"notice.exam.honours".to_string()));
    assert_eq!(h.licence(ANA), 1);
}

#[test]
fn the_checks_come_in_order_and_only_from_the_examinee_on_the_right_pad() {
    let mut h = Host::new();
    let j = h.offer();
    h.accept(ANA, j).unwrap();
    // Out of order: reaching the pad or landing before taking off counts for nothing.
    h.world(ANA, WorldEvent::PadReached { at: LocationId::new("bent_spoon") });
    h.world(ANA, WorldEvent::Landed { at: Some(LocationId::new("bent_spoon")), speed: 1.0 });
    assert!(h.jobs.get(j).unwrap().checks.iter().all(|c| c.state == CheckState::Pending));
    // Someone else's flight is not the exam.
    h.world(BO, WorldEvent::TookOff);
    assert_eq!(h.jobs.get(j).unwrap().checks[0].state, CheckState::Pending);
    h.world(ANA, WorldEvent::TookOff);
    assert!(matches!(h.jobs.get(j).unwrap().checks[0].state, CheckState::Done { .. }));
    // The wrong pad, or no pad.
    h.world(ANA, WorldEvent::PadReached { at: LocationId::new("drip_rock") });
    assert_eq!(h.jobs.get(j).unwrap().checks[1].state, CheckState::Pending);
    h.world(ANA, WorldEvent::PadReached { at: LocationId::new("bent_spoon") });
    h.world(ANA, WorldEvent::Landed { at: None, speed: 1.0 });
    assert_eq!(h.jobs.get(j).unwrap().checks[2].state, CheckState::Pending, "a landing off the pad is no landing on it");
    h.world(ANA, WorldEvent::Landed { at: Some(LocationId::new("bent_spoon")), speed: 1.0 });
    assert!(matches!(h.jobs.get(j).unwrap().checks[2].state, CheckState::Done { value, .. } if value == 1.0));
}

#[test]
fn a_crash_landing_fails_the_exam_and_a_retry_costs_half() {
    let mut h = Host::new();
    let wallet = h.progress.wallet();
    let j = h.offer();
    h.accept(ANA, j).unwrap();
    h.fly(ANA, 9.0);
    assert_eq!(h.state(j), JobState::Failed);
    assert_eq!(h.licence(ANA), 0);
    assert!(h.keys().contains(&"notice.exam.failed".to_string()), "{:?}", h.keys());
    assert!(h.notices.iter().any(|n| n.kind == NoticeKind::Failed));
    assert!(!h.progress.has_flag(&Flag::new("job_completed:flight_exam")));
    // Try again: half the fee.
    let j2 = h.offer();
    h.accept(ANA, j2).unwrap();
    assert_eq!(h.progress.wallet(), wallet - 150 - 75);
    h.fly(ANA, 3.0);
    h.deliver(j2, ANA, 1.0);
    assert_eq!(h.state(j2), JobState::Completed);
    assert_eq!(h.licence(ANA), 1);
    // The fee counts per player: Bo's first try is the full price.
    let mut h = Host::new();
    let wallet = h.progress.wallet();
    let j = h.offer();
    h.accept(ANA, j).unwrap();
    h.fly(ANA, 9.0);
    let j = h.offer();
    h.accept(BO, j).unwrap();
    assert_eq!(h.progress.wallet(), wallet - 150 - 150);
}

#[test]
fn a_damaged_crate_or_the_clock_runs_out_fails_the_exam() {
    let mut h = Host::new();
    let j = h.offer();
    h.accept(ANA, j).unwrap();
    h.fly(ANA, 2.0);
    h.deliver(j, ANA, 0.5);
    assert_eq!(h.state(j), JobState::Failed, "0.5 is below 0.8");
    assert_eq!(h.licence(ANA), 0);

    let mut h = Host::new();
    let j = h.offer();
    h.accept(ANA, j).unwrap();
    h.world(ANA, WorldEvent::TookOff);
    h.world(HOST, WorldEvent::TimePassed { dt: 241.0 });
    assert_eq!(h.state(j), JobState::Expired);
    assert_eq!(h.licence(ANA), 0);
    assert!(h.keys().contains(&"notice.exam.failed".to_string()));
}

#[test]
fn the_exam_needs_the_fee_and_is_closed_to_those_with_the_licence() {
    let mut h = Host::new();
    h.world(HOST, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: -(h.progress.wallet() - 100), player: None });
    assert_eq!(h.progress.wallet(), 100);
    let j = h.offer();
    assert_eq!(h.accept(ANA, j).unwrap_err(), Refusal::CannotAfford { price: 150, wallet: 100 });
    assert_eq!(h.progress.wallet(), 100, "nothing is charged");
    assert_eq!(h.state(j), JobState::Offered);

    let mut h = Host::new();
    h.world(HOST, WorldEvent::TrackChanged { track: TrackId::new("licence_flight"), delta: 1, player: Some(ANA) });
    let j = h.offer();
    assert_eq!(h.accept(ANA, j).unwrap_err(), Refusal::NotAvailable, "passed is passed");
    assert!(h.accept(BO, j).is_ok(), "a crewmate without the licence may still take the exam");
}

#[test]
fn checks_and_attempts_survive_a_save() {
    let mut h = Host::new();
    let j = h.offer();
    h.accept(ANA, j).unwrap();
    h.world(ANA, WorldEvent::TookOff);
    let mut env = gameplay_core::save::Envelope::new();
    h.jobs.save(&mut env);
    let back = Jobs::load(&gameplay_core::save::Envelope::from_json(&env.to_json()).unwrap()).unwrap().unwrap();
    assert_eq!(back, h.jobs);
    let mut g = Host::new();
    g.jobs = back;
    g.progress = h.progress.clone();
    g.next_crate = h.next_crate;
    for host in [&mut h, &mut g] {
        host.world(ANA, WorldEvent::PadReached { at: LocationId::new("bent_spoon") });
        host.world(ANA, WorldEvent::Landed { at: Some(LocationId::new("bent_spoon")), speed: 9.0 });
    }
    assert_eq!(g.state(j), JobState::Failed);
    assert_eq!(g.jobs, h.jobs);
    // The attempt was counted before the save: a retry is half price after loading too.
    let wallet = g.progress.wallet();
    let j2 = g.offer();
    g.accept(ANA, j2).unwrap();
    assert_eq!(g.progress.wallet(), wallet - 75);
}
