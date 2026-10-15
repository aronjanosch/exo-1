//! Mission display rows and captain-only mission actions over `jobs_core` (#288, #122).
//! Content: the kernel and job fixtures of `gameplay_core` and `jobs_core`, plus one job with a
//! giver (`courier_run`), written here.
use std::path::Path;

use crew_core::mission_view::{self, Objective};
use crew_core::*;
use gameplay_core::{ClientId, Content, CrateId, Event, File, LocationId, Progress, WorldEvent};
use jobs_core::{JobContent, JobEvent, JobId, JobState, Jobs, TemplateId};

const CAPTAIN: ClientId = ClientId(1);

const COURIER_RUN: &str = r#"{
  "id": "courier_run",
  "title": "job.courier_run.title",
  "brief": "job.courier_run.brief",
  "giver": "courier",
  "objectives": [
    { "deliver": { "from": { "location": "drip_rock" }, "to": { "location": "bent_spoon" }, "commodity": { "one_of": ["fizzy_mud"] }, "amount": [2, 2] } }
  ],
  "reward": 180,
  "grading": { "bands": [ { "share": 1.0, "pays": 1.0 } ], "condition_weight": 0.5 },
  "deadline_s": 300,
  "track": "freight_xp",
  "xp": 10
}"#;

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

struct World {
    kernel: Content,
    jc: JobContent,
    progress: Progress,
    jobs: Jobs,
    seq: u64,
}

impl World {
    fn new() -> World {
        let kernel = Content::load(&files("../gameplay_core/tests/fixtures/valid"), &["small", "medium", "large"]).unwrap();
        let mut job_files = files("../jobs_core/tests/fixtures");
        job_files.push(File::new("job_template/courier_run.json", COURIER_RUN));
        let jc = JobContent::load(&job_files, &kernel).unwrap();
        let progress = Progress::new(&kernel);
        World { kernel, jc, progress, jobs: Jobs::default(), seq: 0 }
    }

    fn offer(&mut self, template: &str) -> JobId {
        self.jobs.offer_fixed(&self.jc.templates[&TemplateId::new(template)].record).unwrap()
    }

    /// Applies what a captain action asked for, the way the glue would.
    fn run(&mut self, c: MissionCommand) {
        let MissionCommand::Apply(e) = c else { panic!("{c:?}") };
        self.seq += 1;
        self.jobs.apply_job(&self.jc, &self.kernel, &self.progress, &Event::new(CAPTAIN, self.seq, e)).unwrap();
    }

    fn world(&mut self, e: WorldEvent) {
        self.seq += 1;
        self.jobs.apply_world(&self.jc, &Event::new(CAPTAIN, self.seq, e));
    }
}

#[test]
fn rows_show_the_issuer_only_to_the_captain() {
    let mut w = World::new();
    let id = w.offer("courier_run");
    let captain = mission_view::rows(&w.jobs, &w.jc, Role::Captain, Filter::Offered, None);
    let pilot = mission_view::rows(&w.jobs, &w.jc, Role::Pilot, Filter::Offered, None);
    assert_eq!(captain.len(), 1);
    assert_eq!(captain[0].issuer.as_ref().map(|k| k.as_str()), Some("giver.courier.name"));
    assert_eq!(pilot[0].issuer, None);
    let r = &pilot[0];
    assert_eq!((r.job, r.state, r.reward, r.time_left_s), (id, JobState::Offered, 180, None));
    assert_eq!(r.title.as_str(), "job.courier_run.title");
    assert_eq!((r.done, r.of), (0, 2));
    assert_eq!(r.objectives, vec![ObjectiveRow { what: Objective::Deliver { commodity: gameplay_core::CommodityId::new("fizzy_mud"), to: LocationId::new("bent_spoon") }, done: 0, of: 2 }]);
    assert!(mission_view::rows(&w.jobs, &w.jc, Role::Captain, Filter::Active, None).is_empty());
}

#[test]
fn only_the_captain_accepts_tracks_and_abandons() {
    let mut w = World::new();
    let a = w.offer("courier_run");
    let b = w.offer("first_haul");
    let c = w.offer("courier_run");

    assert_eq!(mission_view::accept(Role::Engineer, &w.jobs, a), Err(Refusal::NotCaptain));
    assert_eq!(mission_view::accept(Role::Captain, &w.jobs, JobId(99)), Err(Refusal::UnknownJob));
    assert_eq!(mission_view::set_active(Role::Captain, &w.jobs, a), Err(Refusal::NotActive));
    let cmd = mission_view::accept(Role::Captain, &w.jobs, a).unwrap();
    assert_eq!(cmd, MissionCommand::Apply(JobEvent::OfferAccepted { job: a }));
    w.run(cmd);
    assert_eq!(mission_view::accept(Role::Captain, &w.jobs, a), Err(Refusal::NotOffered));
    w.run(mission_view::accept(Role::Captain, &w.jobs, b).unwrap());
    // Two active is the most (jobs_core::MAX_ACTIVE).
    assert_eq!(mission_view::accept(Role::Captain, &w.jobs, c), Err(Refusal::TooManyActive));

    assert_eq!(mission_view::set_active(Role::Pilot, &w.jobs, a), Err(Refusal::NotCaptain));
    assert_eq!(mission_view::set_active(Role::Captain, &w.jobs, a), Ok(MissionCommand::Track(a)));
    let active = mission_view::rows(&w.jobs, &w.jc, Role::Scientist, Filter::Active, Some(a));
    assert_eq!(active.len(), 2);
    assert!(active.iter().find(|r| r.job == a).unwrap().tracked);
    assert!(!active.iter().find(|r| r.job == b).unwrap().tracked);

    assert_eq!(mission_view::abandon(Role::Tactical, &w.jobs, b), Err(Refusal::NotCaptain));
    w.run(mission_view::abandon(Role::Captain, &w.jobs, b).unwrap());
    assert_eq!(mission_view::abandon(Role::Captain, &w.jobs, b), Err(Refusal::NotActive));
    let gone = mission_view::rows(&w.jobs, &w.jc, Role::Captain, Filter::Abandoned, None);
    assert_eq!(gone.iter().map(|r| r.job).collect::<Vec<_>>(), vec![b]);
}

#[test]
fn rows_follow_progress_time_and_completion() {
    let mut w = World::new();
    let a = w.offer("courier_run");
    w.run(mission_view::accept(Role::Captain, &w.jobs, a).unwrap());
    w.run(MissionCommand::Apply(JobEvent::CratesSpawned { job: a, leg: 0, crates: vec![CrateId(1), CrateId(2)] }));
    let drip = LocationId::new("drip_rock");
    let spoon = LocationId::new("bent_spoon");
    w.world(WorldEvent::CratePickedUp { crate_id: CrateId(1), at: drip.clone() });
    w.world(WorldEvent::TimePassed { dt: 40.0 });
    w.world(WorldEvent::CrateDelivered { crate_id: CrateId(1), at: spoon.clone(), condition: 1.0 });

    let r = &mission_view::rows(&w.jobs, &w.jc, Role::Captain, Filter::Active, Some(a))[0];
    assert_eq!((r.done, r.of), (1, 2));
    assert_eq!(r.time_left_s, Some(260.0));
    let line = mission_view::line(r);
    assert_eq!((line.title.as_str(), line.done, line.of), ("job.courier_run.title", 1, 2));

    w.world(WorldEvent::CratePickedUp { crate_id: CrateId(2), at: drip });
    w.world(WorldEvent::CrateDelivered { crate_id: CrateId(2), at: spoon, condition: 1.0 });
    assert!(mission_view::rows(&w.jobs, &w.jc, Role::Captain, Filter::Active, None).is_empty());
    let done = mission_view::rows(&w.jobs, &w.jc, Role::Captain, Filter::Completed, None);
    assert_eq!((done.len(), done[0].state, done[0].done), (1, JobState::Completed, 2));
}
