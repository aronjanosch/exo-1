//! Jobs rules of #123 and #124. Kernel content: the fixture set of `gameplay_core`; job templates:
//! `tests/fixtures/job_template`. `Host` plays the glue: it deduplicates, routes every event to
//! the systems, and applies the events the jobs system emits.
use std::path::Path;

use gameplay_core::{ClientId, Content, CrateId, Dedup, Event, File, LocationId, Progress, TrackId, WorldEvent};
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

fn job_files() -> Vec<File> {
    files("tests/fixtures")
}

struct Host {
    k: Content,
    jc: JobContent,
    progress: Progress,
    jobs: Jobs,
    dedup: Dedup,
    seq: u64,
    next_crate: u64,
    /// Every outcome so far, for checks.
    log: Vec<Outcome>,
}

impl Host {
    fn new() -> Host {
        let k = kernel();
        let jc = JobContent::load(&job_files(), &k).unwrap();
        let progress = Progress::new(&k);
        Host { k, jc, progress, jobs: Jobs::default(), dedup: Dedup::default(), seq: 1000, next_crate: 1, log: Vec::new() }
    }

    fn offer(&mut self, template: &str) -> JobId {
        let t = &self.jc.templates[&TemplateId::new(template)].record;
        self.jobs.offer_fixed(t).unwrap()
    }

    /// A jobs event from `who`; spawns crates the way the glue would.
    fn job(&mut self, who: ClientId, seq: u64, e: JobEvent) -> Result<(), Refusal> {
        let ev = Event::new(who, seq, e);
        if !self.dedup.first_time(ev.id) {
            return Ok(());
        }
        let out = self.jobs.apply_job(&self.jc, &self.k, &self.progress, &ev)?;
        self.handle(out);
        Ok(())
    }

    fn world(&mut self, who: ClientId, seq: u64, e: WorldEvent) {
        let ev = Event::new(who, seq, e);
        if !self.dedup.first_time(ev.id) {
            return;
        }
        self.progress.apply(&self.k, &ev).unwrap();
        let out = self.jobs.apply_world(&self.jc, &ev);
        self.handle(out);
    }

    fn handle(&mut self, out: Vec<Outcome>) {
        for o in out {
            self.log.push(o.clone());
            match o {
                Outcome::SpawnCrates { job, leg, count, .. } => {
                    let crates = (0..count).map(|_| {
                        self.next_crate += 1;
                        CrateId(self.next_crate - 1)
                    });
                    let crates = crates.collect();
                    self.seq += 1;
                    self.job(HOST, self.seq, JobEvent::CratesSpawned { job, leg, crates }).unwrap();
                }
                Outcome::Emit(e) => {
                    self.seq += 1;
                    self.world(HOST, self.seq, e);
                }
                Outcome::ReleaseCrates { .. } | Outcome::Ended { .. } | Outcome::Notice(_) => {}
            }
        }
    }

    fn crates(&self, job: JobId) -> Vec<CrateId> {
        self.jobs.get(job).unwrap().legs.iter().flat_map(|l| l.crates.keys().copied()).collect()
    }

    fn state(&self, job: JobId) -> JobState {
        self.jobs.get(job).unwrap().state
    }

    fn xp(&self, who: ClientId) -> i64 {
        self.progress.value(&self.k, &TrackId::new("freight_xp"), Some(who)).unwrap()
    }

    fn ended(&self, job: JobId) -> Option<(JobState, Option<Grade>)> {
        self.log.iter().find_map(|o| match o {
            Outcome::Ended { job: j, state, grade } if *j == job => Some((*state, *grade)),
            _ => None,
        })
    }

    /// `who` carries crate `c` from the job's pickup to `to` and sets it down with `cond`.
    fn carry(&mut self, who: ClientId, seq: u64, c: CrateId, from: &str, to: &str, cond: f64) {
        self.world(who, seq, WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new(from) });
        self.world(who, seq + 1, WorldEvent::CrateDelivered { crate_id: c, at: LocationId::new(to), condition: cond });
    }
}

// ---------- loader ----------

#[test]
fn fixture_templates_load() {
    let h = Host::new();
    assert_eq!(h.jc.templates.len(), 3);
}

fn template_errors(path: &str, text: &str) -> Vec<String> {
    let mut f: Vec<File> = job_files().into_iter().filter(|f| f.path != path).collect();
    f.push(File::new(path, text));
    JobContent::load(&f, &kernel()).unwrap_err()
}

const SLOW: &str = r#"{ "id": "slow_mud", "title": "t", "brief": "b",
  "objectives": [ { "deliver": { "from": { "location": "bent_spoon" }, "to": { "location": "drip_rock" }, "commodity": { "one_of": ["fizzy_mud"] }, "amount": [2, 2] } } ],
  "reward": 100, "grading": { "bands": [ { "share": 1.0, "pays": 1.0 } ], "condition_weight": 0.5 }, "track": "freight_xp", "xp": 10 }"#;

#[test]
fn loader_errors_name_file_and_field() {
    let p = "job_template/slow_mud.json";
    let cases = [
        (SLOW.replace("\"bent_spoon\"", "\"moon_base\""), "objectives.deliver.from"),
        (SLOW.replace("\"bent_spoon\"", "\"drip_rock\""), "objectives.deliver.to"),
        (SLOW.replace("{ \"location\": \"bent_spoon\" }", "{ \"tagged\": \"frozen\" }"), "objectives.deliver.from"),
        (SLOW.replace("[\"fizzy_mud\"]", "[\"moon_dust\"]"), "objectives.deliver.commodity"),
        (SLOW.replace("[2, 2]", "[3, 2]"), "objectives.deliver.amount"),
        (SLOW.replace("\"reward\": 100", "\"reward\": 0"), "reward"),
        (SLOW.replace("{ \"share\": 1.0, \"pays\": 1.0 }", "{ \"share\": 1.0, \"pays\": 1.0 }, { \"share\": 0.5, \"pays\": 0.5 }"), "grading.bands"),
        (SLOW.replace("\"track\": \"freight_xp\"", "\"track\": \"wallet\""), "track"),
        (SLOW.replace("\"xp\": 10", "\"xp\": 10, \"follow_up\": \"nope\""), "follow_up"),
        (SLOW.replace("\"xp\": 10", "\"xp\": 10, \"deadline_s\": 0"), "deadline_s"),
        (SLOW.replace("\"xp\": 10", "\"xp\": 10, \"modifiers\": [ { \"tag\": \"a\", \"kind\": \"warning\" }, { \"tag\": \"b\", \"kind\": \"warning\" } ]"), "modifiers"),
        (SLOW.replace("\"xp\": 10", "\"xp\": 10, \"modifiers\": [ { \"tag\": \"a\", \"kind\": \"upside\", \"bonus\": 0.5 } ]"), "modifiers.bonus"),
        (SLOW.replace("\"xp\": 10", "\"xp\": 10, \"available\": { \"has_tag\": \"nobody_grants_this\" }"), "available"),
        (SLOW.replace("\"xp\": 10", "\"xp\": 10, \"colour\": 1"), "colour"),
    ];
    for (text, field) in cases {
        let e = template_errors(p, &text);
        assert_eq!(e.len(), 1, "{field}: {e:?}");
        assert!(e[0].starts_with(p) && e[0].contains(field), "{field}: {e:?}");
    }
}

#[test]
fn only_fully_named_templates_make_fixed_offers() {
    let mut h = Host::new();
    let t = h.jc.templates[&TemplateId::new("jelly_run")].record.clone();
    assert!(h.jobs.offer_fixed(&t).is_none(), "tag search and pools are the board's job");
    let j = h.offer("first_haul");
    let job = h.jobs.get(j).unwrap();
    assert_eq!(job.state, JobState::Offered);
    assert_eq!((job.legs[0].from.as_str(), job.legs[0].to.as_str(), job.legs[0].amount), ("drip_rock", "bent_spoon", 4));
}

// ---------- state machine (#123) ----------

#[test]
fn full_delivery_completes_and_pays() {
    let mut h = Host::new();
    let j = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    assert_eq!(h.state(j), JobState::Active);
    let crates = h.crates(j);
    assert_eq!(crates.len(), 4, "accepting spawns the crates at the pickup");
    assert!(h.log.iter().any(|o| matches!(o, Outcome::SpawnCrates { count: 4, at, .. } if at.as_str() == "drip_rock")));
    for (i, c) in crates.iter().enumerate() {
        h.carry(ANA, 10 + 2 * i as u64, *c, "drip_rock", "bent_spoon", 1.0);
    }
    assert_eq!(h.state(j), JobState::Completed);
    // 300 × 1.0 × 1.0 × 1.25 (bumpy warning)
    assert_eq!(h.progress.wallet(), 200 + 375);
    assert_eq!(h.xp(ANA), 50);
    assert!(h.progress.has_flag(&gameplay_core::Flag::new("job_completed:first_haul")));
}

#[test]
fn deadline_starts_at_first_pickup() {
    let mut h = Host::new();
    let j = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    h.world(HOST, 1, WorldEvent::TimePassed { dt: 10_000.0 });
    assert_eq!(h.state(j), JobState::Active, "no pickup yet: the clock has not started");
    let c = h.crates(j)[0];
    h.world(ANA, 1, WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new("drip_rock") });
    h.world(HOST, 2, WorldEvent::TimePassed { dt: 479.0 });
    let t = &h.jc.templates[&TemplateId::new("first_haul")].record;
    assert_eq!(h.jobs.get(j).unwrap().time_left(t), Some(1.0));
    assert_eq!(h.state(j), JobState::Active);
}

#[test]
fn partial_delivery_then_deadline_expires_with_the_share() {
    let mut h = Host::new();
    let j = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let crates = h.crates(j);
    for (i, c) in crates[..3].iter().enumerate() {
        h.carry(ANA, 10 + 2 * i as u64, *c, "drip_rock", "bent_spoon", 1.0);
    }
    h.world(HOST, 1, WorldEvent::TimePassed { dt: 480.0 });
    assert_eq!(h.state(j), JobState::Expired);
    let (_, g) = h.ended(j).unwrap();
    assert_eq!(g.unwrap().share, 0.75);
    // 300 × 0.7 (band at 0.75) × 1.25 = 262.5 → 263
    assert_eq!(h.progress.wallet(), 200 + 263);
    assert!(!h.progress.has_flag(&gameplay_core::Flag::new("job_completed:first_haul")), "expired is not completed");
    assert!(h.log.iter().any(|o| matches!(o, Outcome::ReleaseCrates { crates: cs } if cs == &vec![crates[3]])), "the crate left behind is released");
}

#[test]
fn a_job_without_deadline_never_expires() {
    let mut h = Host::new();
    let j = h.offer("slow_mud");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let c = h.crates(j)[0];
    h.world(ANA, 1, WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new("bent_spoon") });
    for s in 0..100 {
        h.world(HOST, 10 + s, WorldEvent::TimePassed { dt: 3600.0 });
    }
    assert_eq!(h.state(j), JobState::Active);
}

#[test]
fn a_duplicate_event_id_changes_nothing() {
    let mut h = Host::new();
    let j = h.offer("slow_mud");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let crates = h.crates(j);
    h.carry(ANA, 10, crates[0], "bent_spoon", "drip_rock", 1.0);
    // The same delivery again (a co-op retry): same ids, nothing happens.
    h.carry(ANA, 10, crates[0], "bent_spoon", "drip_rock", 1.0);
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    assert_eq!(h.jobs.get(j).unwrap().delivered(), 1);
    assert_eq!(h.state(j), JobState::Active);
    h.carry(ANA, 12, crates[1], "bent_spoon", "drip_rock", 1.0);
    h.carry(ANA, 12, crates[1], "bent_spoon", "drip_rock", 1.0);
    assert_eq!(h.progress.wallet(), 300, "paid exactly once");
}

#[test]
fn abandon_with_crates_on_board() {
    let mut h = Host::new();
    let j = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let crates = h.crates(j);
    h.world(ANA, 1, WorldEvent::CratePickedUp { crate_id: crates[0], at: LocationId::new("drip_rock") });
    h.job(BO, 0, JobEvent::JobAbandoned { job: j }).unwrap();
    assert_eq!(h.state(j), JobState::Abandoned);
    assert_eq!(h.progress.wallet(), 200, "no pay, no penalty");
    assert!(h.log.iter().any(|o| matches!(o, Outcome::ReleaseCrates { crates: cs } if cs.len() == 4)));
    // Delivering a released crate afterwards changes nothing.
    h.world(ANA, 2, WorldEvent::CrateDelivered { crate_id: crates[0], at: LocationId::new("bent_spoon"), condition: 1.0 });
    assert_eq!(h.progress.wallet(), 200);
    assert_eq!(h.job(ANA, 3, JobEvent::JobAbandoned { job: j }), Err(Refusal::NotActive));
}

#[test]
fn a_third_accept_is_refused() {
    let mut h = Host::new();
    let a = h.offer("slow_mud");
    let b = h.offer("slow_mud");
    let c = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: a }).unwrap();
    h.job(BO, 0, JobEvent::OfferAccepted { job: b }).unwrap();
    assert_eq!(h.job(ANA, 1, JobEvent::OfferAccepted { job: c }), Err(Refusal::TooManyActive));
    assert_eq!(h.state(c), JobState::Offered);
    assert_eq!(h.job(ANA, 2, JobEvent::OfferAccepted { job: a }), Err(Refusal::NotOffered));
}

#[test]
fn availability_is_read_for_the_accepting_player() {
    let mut h = Host::new();
    let t = h.jc.templates[&TemplateId::new("jelly_run")].record.clone();
    let legs = vec![Leg::new(LocationId::new("drip_rock"), LocationId::new("bent_spoon"), gameplay_core::CommodityId::new("jelly_bricks"), 2)];
    let j = h.jobs.offer(&t.id, legs);
    assert_eq!(h.job(ANA, 0, JobEvent::OfferAccepted { job: j }), Err(Refusal::NotAvailable));
}

#[test]
fn a_crate_set_down_elsewhere_waits_and_lost_crates_end_the_job() {
    let mut h = Host::new();
    let j = h.offer("slow_mud");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let crates = h.crates(j);
    h.carry(ANA, 10, crates[0], "bent_spoon", "bent_spoon", 1.0);
    assert_eq!(h.jobs.get(j).unwrap().legs[0].crates[&crates[0]], CrateMark::Waiting);
    h.carry(ANA, 12, crates[0], "bent_spoon", "drip_rock", 0.5);
    h.world(HOST, 1, WorldEvent::CrateLost { crate_id: crates[1] });
    assert_eq!(h.state(j), JobState::Completed, "nothing more can be delivered");
    // 100 × 0.5 (band at 0.5) × (1 - 0.5 × 0.5) = 37.5 → 38
    assert_eq!(h.progress.wallet(), 238);
}

#[test]
fn spawn_rules() {
    let mut h = Host::new();
    let j = h.offer("slow_mud");
    assert_eq!(h.job(HOST, 0, JobEvent::CratesSpawned { job: j, leg: 0, crates: vec![CrateId(90), CrateId(91)] }), Err(Refusal::NotActive));
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    assert_eq!(h.job(HOST, 1, JobEvent::CratesSpawned { job: j, leg: 0, crates: vec![CrateId(90), CrateId(91)] }), Err(Refusal::BadSpawn), "already spawned");
    assert_eq!(h.job(HOST, 2, JobEvent::CratesSpawned { job: j, leg: 5, crates: vec![] }), Err(Refusal::BadSpawn));
}

#[test]
fn jobs_round_trip_as_json() {
    let mut h = Host::new();
    let j = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let c = h.crates(j)[0];
    h.carry(ANA, 10, c, "drip_rock", "bent_spoon", 0.9);
    let s = serde_json::to_string(&h.jobs).unwrap();
    assert_eq!(serde_json::from_str::<Jobs>(&s).unwrap(), h.jobs);
}

// ---------- grading and payout (#124) ----------

#[test]
fn bands_at_their_edges() {
    let h = Host::new();
    let t = &h.jc.templates[&TemplateId::new("first_haul")].record;
    // (delivered of 4, band factor, money with the 1.25 hazard factor)
    for (d, band, money) in [(0, 0.0, 0), (1, 0.0, 0), (2, 0.4, 150), (3, 0.7, 263), (4, 1.0, 375)] {
        let g = grade(t, d, 4, d as f64);
        assert_eq!((g.band, g.money), (band, money), "{d} of 4");
        assert_eq!(g.xp, (50.0 * band).round() as i64);
    }
    // Just below an edge stays in the band below: 74 of 100 with the 0.75 edge.
    assert_eq!(grade(t, 74, 100, 74.0).band, 0.4);
    assert_eq!(grade(t, 75, 100, 75.0).band, 0.7);
}

#[test]
fn damaged_crates_pay_less() {
    let h = Host::new();
    let t = &h.jc.templates[&TemplateId::new("first_haul")].record;
    let g = grade(t, 4, 4, 2.0); // mean condition 0.5
    assert_eq!(g.condition, 0.75);
    assert_eq!(g.money, 281); // 300 × 0.75 × 1.25 = 281.25
    assert_eq!(grade(t, 4, 4, 0.0).money, 188); // wrecked: 300 × 0.5 × 1.25 = 187.5
}

#[test]
fn the_hazard_bonus_counts_warnings_only() {
    let h = Host::new();
    assert_eq!(grade(&h.jc.templates[&TemplateId::new("first_haul")].record, 4, 4, 4.0).hazard, 1.25);
    assert_eq!(grade(&h.jc.templates[&TemplateId::new("jelly_run")].record, 2, 2, 2.0).hazard, 1.0, "an upside pays no bonus");
}

#[test]
fn two_participants_get_xp_and_the_wallet_is_paid_once() {
    let mut h = Host::new();
    let j = h.offer("slow_mud");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let crates = h.crates(j);
    h.carry(ANA, 10, crates[0], "bent_spoon", "drip_rock", 1.0);
    h.carry(BO, 10, crates[1], "bent_spoon", "drip_rock", 1.0);
    assert_eq!(h.state(j), JobState::Completed);
    assert_eq!(h.progress.wallet(), 300);
    assert_eq!((h.xp(ANA), h.xp(BO), h.xp(ClientId(99))), (10, 10, 0));
}

// ---------- save section (#128) ----------

#[test]
fn jobs_section_round_trips_and_a_loaded_game_finishes_the_same() {
    use gameplay_core::save::{Envelope, KernelState, SaveError};
    let mut h = Host::new();
    let j = h.offer("first_haul");
    let _other = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let crates = h.crates(j);
    h.carry(ANA, 10, crates[0], "drip_rock", "bent_spoon", 0.8);
    h.world(ANA, 20, WorldEvent::CratePickedUp { crate_id: crates[1], at: LocationId::new("drip_rock") });
    h.world(HOST, 21, WorldEvent::TimePassed { dt: 12.5 });

    // Save the kernel and the jobs section, load them into a second host.
    let mut env = Envelope::new();
    KernelState { progress: h.progress.clone(), dedup: h.dedup.clone() }.save(&mut env);
    h.jobs.save(&mut env);
    let env = Envelope::from_json(&env.to_json()).unwrap();
    let k = KernelState::load(&env).unwrap().unwrap();
    let jobs = Jobs::load(&env).unwrap().unwrap();
    assert_eq!(jobs, h.jobs, "state → JSON → state is equal");

    let mut g = Host::new();
    g.progress = k.progress;
    g.dedup = k.dedup;
    g.jobs = jobs;
    g.next_crate = h.next_crate;
    g.seq = h.seq;

    // The rest of the delivery on both: the same result.
    for host in [&mut h, &mut g] {
        host.world(ANA, 22, WorldEvent::CrateDelivered { crate_id: crates[1], at: LocationId::new("bent_spoon"), condition: 1.0 });
        host.carry(BO, 30, crates[2], "drip_rock", "bent_spoon", 1.0);
        host.carry(BO, 32, crates[3], "drip_rock", "bent_spoon", 0.5);
    }
    assert_eq!(h.state(j), JobState::Completed);
    assert_eq!(g.jobs, h.jobs);
    assert_eq!(g.progress, h.progress);
    assert_eq!(g.jobs.get(j).unwrap().participants, h.jobs.get(j).unwrap().participants);

    // A save of an unknown jobs version is refused, a save without the section loads as none.
    let mut bad = Envelope::new();
    bad.put("jobs", 99, &h.jobs);
    assert!(matches!(Jobs::load(&bad), Err(SaveError::SectionVersion { found: 99, .. })));
    assert_eq!(Jobs::load(&Envelope::new()).unwrap(), None);
}

// ---------- notices (#165) ----------

fn notices(h: &Host) -> Vec<gameplay_core::notice::Notice> {
    h.log.iter().filter_map(|o| if let Outcome::Notice(n) = o { Some(n.clone()) } else { None }).collect()
}

fn num(n: &gameplay_core::notice::Notice, name: &str) -> Option<i64> {
    n.args.iter().find_map(|(k, a)| match a {
        gameplay_core::notice::Arg::Number(x) if k == name => Some(*x),
        _ => None,
    })
}

#[test]
fn a_delivery_reports_its_beats_in_order() {
    use gameplay_core::notice::NoticeKind as K;
    let mut h = Host::new();
    let j = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    for (i, c) in h.crates(j).into_iter().enumerate() {
        h.carry(ANA, 10 + 2 * i as u64, c, "drip_rock", "bent_spoon", 1.0);
    }
    let ns = notices(&h);
    let keys: Vec<&str> = ns.iter().map(|n| n.key.as_str()).collect();
    let mut want = vec!["notice.job.accepted"];
    for _ in 0..4 {
        want.extend(["notice.job.picked_up", "notice.job.delivered"]);
    }
    // 300 base, +75 for the warning, 50 XP; no share or condition line at full grade.
    want.extend(["notice.job.completed", "notice.reward.base", "notice.reward.hazard", "notice.reward.xp"]);
    assert_eq!(keys, want);
    assert_eq!(ns[0].kind, K::Accepted);
    assert_eq!(ns[1].kind, K::Updated);
    let done = ns.iter().find(|n| n.kind == K::Completed).unwrap();
    assert_eq!(num(done, "money"), Some(375));
    let last_delivered = ns.iter().rfind(|n| n.key.as_str() == "notice.job.delivered").unwrap();
    assert_eq!((num(last_delivered, "delivered"), num(last_delivered, "asked")), (Some(4), Some(4)));
    let hazard = ns.iter().find(|n| n.key.as_str() == "notice.reward.hazard").unwrap();
    assert_eq!(num(hazard, "n"), Some(75));
}

#[test]
fn the_itemised_payout_adds_up_to_the_pay() {
    let mut h = Host::new();
    let j = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let crates = h.crates(j);
    for (i, c) in crates[..2].iter().enumerate() {
        h.carry(ANA, 10 + 2 * i as u64, *c, "drip_rock", "bent_spoon", 0.6);
    }
    h.world(HOST, 1, WorldEvent::TimePassed { dt: 480.0 });
    let ns = notices(&h);
    let money: i64 = ns
        .iter()
        .filter(|n| matches!(n.key.as_str(), "notice.reward.base" | "notice.reward.share" | "notice.reward.condition" | "notice.reward.hazard"))
        .map(|n| num(n, "n").unwrap())
        .sum();
    let (_, g) = h.ended(j).unwrap();
    assert_eq!(money, g.unwrap().money, "base + share + condition + hazard lines");
    assert!(h.progress.wallet() == 200 + money);
    let keys: Vec<&str> = ns.iter().map(|n| n.key.as_str()).collect();
    assert!(keys.contains(&"notice.job.expired") && keys.contains(&"notice.reward.share") && keys.contains(&"notice.reward.condition"), "{keys:?}");
    assert!(!keys.contains(&"notice.job.completed"));
}

#[test]
fn abandoning_and_losing_a_crate_warn() {
    use gameplay_core::notice::NoticeKind as K;
    let mut h = Host::new();
    let j = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let c = h.crates(j)[0];
    h.world(ANA, 1, WorldEvent::CrateLost { crate_id: c });
    h.job(ANA, 2, JobEvent::JobAbandoned { job: j }).unwrap();
    let ns = notices(&h);
    assert!(ns.iter().any(|n| n.key.as_str() == "notice.job.crate_lost" && n.kind == K::Warning));
    assert!(ns.iter().any(|n| n.key.as_str() == "notice.job.abandoned" && n.kind == K::Warning));
}

#[test]
fn picking_a_carried_crate_up_again_says_nothing_new() {
    let mut h = Host::new();
    let j = h.offer("first_haul");
    h.job(ANA, 0, JobEvent::OfferAccepted { job: j }).unwrap();
    let c = h.crates(j)[0];
    h.world(ANA, 1, WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new("drip_rock") });
    let n = notices(&h).len();
    h.world(ANA, 2, WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new("drip_rock") });
    assert_eq!(notices(&h).len(), n);
}
