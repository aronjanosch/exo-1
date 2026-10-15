use std::path::Path;

use gameplay_core::{ClientId, Content, CrateId, Dedup, Event, File, Flag, LocationId, Progress, TrackId, WorldEvent};
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

fn kernel() -> Content {
    Content::load(&files("../gameplay_core/tests/fixtures/valid"), &["small", "medium", "large"]).unwrap()
}

fn tmpl(id: &str, extra: &str) -> File {
    File::new(
        format!("job_template/{id}.json"),
        format!(
            r#"{{ "id": "{id}", "title": "job.first_haul.title", "brief": "job.first_haul.title", "giver": "courier",
  "objectives": [ {{ "deliver": {{ "from": {{ "location": "drip_rock" }}, "to": {{ "location": "bent_spoon" }}, "commodity": {{ "one_of": ["fizzy_mud"] }}, "amount": [2, 2] }} }} ],
  "reward": 100, "grading": {{ "bands": [ {{ "share": 1.0, "pays": 1.0 }} ], "condition_weight": 0.5 }}, "track": "freight_xp", "xp": 10 {extra} }}"#
        ),
    )
}

fn load_with(extra: Vec<File>) -> Result<JobContent, Vec<String>> {
    let mut f = files("tests/fixtures");
    f.extend(extra);
    JobContent::load(&f, &kernel())
}

fn replace(path: &str, from: &str, to: &str) -> Vec<String> {
    let mut f: Vec<File> = files("tests/fixtures").into_iter().filter(|f| f.path != path).collect();
    let orig = files("tests/fixtures").into_iter().find(|f| f.path == path).unwrap();
    assert!(orig.text.contains(from), "{from} in {path}");
    f.push(File::new(path, orig.text.replace(from, to)));
    JobContent::load(&f, &kernel()).unwrap_err()
}

struct Host {
    k: Content,
    jc: JobContent,
    progress: Progress,
    jobs: Jobs,
    dedup: Dedup,
    seq: u64,
    next_crate: u64,
}

impl Host {
    fn new(templates: Vec<File>) -> Host {
        let k = kernel();
        let jc = load_with(templates).unwrap();
        let progress = Progress::new(&k);
        Host { k, jc, progress, jobs: Jobs::default(), dedup: Dedup::default(), seq: 1000, next_crate: 1 }
    }

    fn standing(&self) -> i64 {
        self.progress.value(&self.k, &TrackId::new("standing_courier"), None).unwrap()
    }

    fn route(&mut self, out: Vec<Outcome>) {
        for o in out {
            match o {
                Outcome::SpawnCrates { job, leg, count, .. } => {
                    let crates: Vec<CrateId> = (0..count).map(|_| {
                        self.next_crate += 1;
                        CrateId(self.next_crate - 1)
                    }).collect();
                    self.seq += 1;
                    let ev = Event::new(HOST, self.seq, JobEvent::CratesSpawned { job, leg, crates });
                    self.dedup.first_time(ev.id);
                    self.jobs.apply_job(&self.jc, &self.k, &self.progress, &ev).unwrap();
                }
                Outcome::Emit(e) => {
                    self.seq += 1;
                    let ev = Event::new(HOST, self.seq, e);
                    self.dedup.first_time(ev.id);
                    self.progress.apply(&self.k, &ev).unwrap();
                    let more = self.jobs.apply_world(&self.jc, &ev);
                    self.route(more);
                }
                _ => {}
            }
        }
    }

    fn accept(&mut self, template: &str) -> Result<JobId, Refusal> {
        let t = self.jc.templates[&TemplateId::new(template)].record.clone();
        let id = self.jobs.offer_fixed(&t).unwrap();
        self.seq += 1;
        let out = self.jobs.apply_job(&self.jc, &self.k, &self.progress, &Event::new(ANA, self.seq, JobEvent::OfferAccepted { job: id }))?;
        self.route(out);
        Ok(id)
    }

    fn world(&mut self, e: WorldEvent) {
        self.seq += 1;
        let ev = Event::new(ANA, self.seq, e);
        self.progress.apply(&self.k, &ev).unwrap();
        let out = self.jobs.apply_world(&self.jc, &ev);
        self.route(out);
    }

    fn deliver_all(&mut self, job: JobId) {
        let crates: Vec<CrateId> = self.jobs.get(job).unwrap().legs.iter().flat_map(|l| l.crates.keys().copied()).collect();
        for c in crates {
            self.world(WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new("drip_rock") });
            self.world(WorldEvent::CrateDelivered { crate_id: c, at: LocationId::new("bent_spoon"), condition: 1.0 });
        }
    }

    fn fail(&mut self, job: JobId) {
        let c = *self.jobs.get(job).unwrap().legs[0].crates.keys().next().unwrap();
        self.world(WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new("drip_rock") });
        self.world(WorldEvent::TimePassed { dt: 1000.0 });
    }
}

const TIMED: &str = r#", "deadline_s": 60"#;

// ---------- loader ----------

#[test]
fn loader_errors_name_file_and_field() {
    let e = load_with(vec![tmpl("run_a", r#", "giver_typo": 1"#)]).unwrap_err();
    assert!(e[0].starts_with("job_template/run_a.json"), "{e:?}");
    let bad = tmpl("run_b", "").text.replace("\"giver\": \"courier\"", "\"giver\": \"nobody\"");
    let e = load_with(vec![File::new("job_template/run_b.json", bad)]).unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].starts_with("job_template/run_b.json") && e[0].contains("giver") && e[0].contains("nobody"), "{e:?}");

    let p = "giver/courier.json";
    for (from, to, field) in [
        ("\"standing_courier\"", "\"standing_nothing\"", "standing"),
        ("\"drip_rock\"", "\"moon_base\"", "location"),
        ("\"gain\": 10", "\"gain\": -1", "gain"),
        ("\"loss\": 15", "\"loss\": -1", "loss"),
        ("\"legal\"", "\"mafia\"", "mafia"),
        ("\"intro\": \"g.courier.intro\",", "", "intro"),
    ] {
        // (Breaking the courier also breaks what refers to it, like the exam's honours: only the
        // errors of this file count.)
        let e: Vec<String> = replace(p, from, to).into_iter().filter(|m| m.starts_with(p)).collect();
        assert_eq!(e.len(), 1, "{field}: {e:?}");
        assert!(e[0].contains(field), "{field}: {e:?}");
    }
    // The standing must be a crew track: freight_xp is a personal one.
    let e: Vec<String> = replace(p, "\"standing_courier\"", "\"freight_xp\"").into_iter().filter(|m| m.starts_with(p)).collect();
    assert!(e[0].contains("standing") && e[0].contains("crew"), "{e:?}");
}

// ---------- briefings ----------

// ---------- standing and rank ----------

#[test]
fn completed_jobs_raise_standing_failures_lower_it_and_it_recovers() {
    let mut h = Host::new(vec![tmpl("run_a", ""), tmpl("run_t", TIMED)]);
    let g = GiverId::new("courier");
    assert_eq!(h.standing(), 0);
    let j = h.accept("run_a").unwrap();
    h.deliver_all(j);
    assert_eq!(h.standing(), 10, "gain of the giver");
    assert_eq!(h.jobs.history(&g), GiverHistory { completed: 1, failure_streak: 0 });

    let j = h.accept("run_t").unwrap();
    h.fail(j);
    assert_eq!(h.standing(), 0, "10 - 15, never below the floor");
    assert_eq!(h.jobs.history(&g).mood(), Mood::AfterFailure);

    // Not locked out for good: the same giver still gives jobs, and work brings standing back.
    let j = h.accept("run_a").unwrap();
    h.deliver_all(j);
    assert_eq!(h.standing(), 10);
    assert_eq!(h.jobs.history(&g), GiverHistory { completed: 2, failure_streak: 0 });
    assert_eq!(h.jobs.history(&g).mood(), Mood::Regular);
}

#[test]
fn abandoning_costs_no_standing() {
    let mut h = Host::new(vec![tmpl("run_a", "")]);
    let j = h.accept("run_a").unwrap();
    h.seq += 1;
    let out = h.jobs.apply_job(&h.jc, &h.k, &h.progress, &Event::new(ANA, h.seq, JobEvent::JobAbandoned { job: j })).unwrap();
    h.route(out);
    assert_eq!(h.standing(), 0);
    assert_eq!(h.jobs.history(&GiverId::new("courier")), GiverHistory::default());
}

#[test]
fn standing_ranks_unlock_a_template() {
    let gated = r#", "available": { "track_at_least": { "track": "standing_courier", "value": 20 } }"#;
    let mut h = Host::new(vec![tmpl("run_a", ""), tmpl("run_gold", gated)]);
    assert_eq!(h.accept("run_gold").unwrap_err(), Refusal::NotAvailable);
    for _ in 0..2 {
        let j = h.accept("run_a").unwrap();
        h.deliver_all(j);
    }
    assert_eq!(h.standing(), 20);
    assert!(h.accept("run_gold").is_ok(), "two completed jobs reach the rank");
}

#[test]
fn a_giver_that_is_not_open_offers_nothing_yet() {
    let t = tmpl("run_fam", "").text.replace("\"giver\": \"courier\"", "\"giver\": \"family\"");
    let mut h = Host::new(vec![File::new("job_template/run_fam.json", t)]);
    assert_eq!(h.accept("run_fam").unwrap_err(), Refusal::NotAvailable);
    h.progress.apply(&h.k, &Event::new(HOST, 5000, WorldEvent::FlagRaised { flag: Flag::new("family_open") })).unwrap();
    assert!(h.accept("run_fam").is_ok());
}

#[test]
fn history_survives_a_save() {
    let mut h = Host::new(vec![tmpl("run_a", "")]);
    let j = h.accept("run_a").unwrap();
    h.deliver_all(j);
    let mut env = gameplay_core::save::Envelope::new();
    h.jobs.save(&mut env);
    let back = Jobs::load(&gameplay_core::save::Envelope::from_json(&env.to_json()).unwrap()).unwrap().unwrap();
    assert_eq!(back.history(&GiverId::new("courier")).completed, 1);
}
