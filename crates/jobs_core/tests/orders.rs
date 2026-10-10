//! Orders of #168 on the jobs side: an `OrderPlaced` event becomes an offer with the order's
//! places, goods, reward and deadline; the end of the job raises `OrderSettled`.
use std::path::Path;

use gameplay_core::{ClientId, CommodityId, Content, CrateId, Event, File, LocationId, OrderId, Progress, WorldEvent};
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
    outcomes: Vec<Outcome>,
}

impl Host {
    fn new() -> Host {
        let k = Content::load(&files("../gameplay_core/tests/fixtures/valid"), &["small", "medium", "large"]).unwrap();
        let jc = JobContent::load(&files("tests/fixtures"), &k).unwrap();
        let progress = Progress::new(&k);
        Host { k, jc, progress, jobs: Jobs::default(), seq: 0, next_crate: 1, outcomes: Vec::new() }
    }

    fn world(&mut self, e: WorldEvent) {
        self.seq += 1;
        let ev = Event::new(ANA, self.seq, e);
        self.progress.apply(&self.k, &ev).unwrap();
        let out = self.jobs.apply_world(&self.jc, &ev);
        self.route(out);
    }

    fn route(&mut self, out: Vec<Outcome>) {
        for o in out {
            self.outcomes.push(o.clone());
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
                Outcome::Emit(e) => {
                    self.seq += 1;
                    let ev = Event::new(HOST, self.seq, e);
                    self.progress.apply(&self.k, &ev).unwrap();
                    let more = self.jobs.apply_world(&self.jc, &ev);
                    self.route(more);
                }
                _ => {}
            }
        }
    }

    fn place(&mut self, order: u64, amount: u32, reward: i64, deadline_s: Option<f64>) {
        self.world(WorldEvent::OrderPlaced { order: OrderId(order), by: "old_nib".into(), from: LocationId::new("wholesale_yard"), to: LocationId::new("drip_rock"), commodity: CommodityId::new("sock_dust"), amount, reward, deadline_s });
    }

    fn accept(&mut self, job: JobId) {
        self.seq += 1;
        let out = self.jobs.apply_job(&self.jc, &self.k, &self.progress, &Event::new(ANA, self.seq, JobEvent::OfferAccepted { job })).unwrap();
        self.route(out);
    }

    fn offered(&self) -> Vec<&Job> {
        self.jobs.all().filter(|j| j.state == JobState::Offered).collect()
    }

    fn deliver(&mut self, job: JobId, cond: f64) {
        let crates: Vec<CrateId> = self.jobs.get(job).unwrap().legs.iter().flat_map(|l| l.crates.keys().copied()).collect();
        for c in crates {
            self.world(WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new("wholesale_yard") });
            self.world(WorldEvent::CrateDelivered { crate_id: c, at: LocationId::new("drip_rock"), condition: cond });
        }
    }

    fn settled(&self) -> Vec<(OrderId, u32, u32, f64, bool)> {
        self.outcomes.iter().filter_map(|o| if let Outcome::Emit(WorldEvent::OrderSettled { order, delivered, asked, condition, in_time }) = o { Some((*order, *delivered, *asked, *condition, *in_time)) } else { None }).collect()
    }
}

#[test]
fn an_order_becomes_an_offer_with_its_own_terms() {
    let mut h = Host::new();
    h.place(7, 3, 90, Some(300.0));
    let offers = h.offered();
    assert_eq!(offers.len(), 1);
    let j = offers[0];
    assert_eq!(j.template.as_str(), "customer_order");
    let leg = &j.legs[0];
    assert_eq!((leg.from.as_str(), leg.to.as_str(), leg.commodity.as_str(), leg.amount), ("wholesale_yard", "drip_rock", "sock_dust", 3));
    let o = j.order.as_ref().unwrap();
    assert_eq!((o.order, o.by.as_str(), o.reward, o.deadline_s), (OrderId(7), "old_nib", 90, Some(300.0)));
    // The same order again is not a second offer.
    h.place(7, 3, 90, Some(300.0));
    assert_eq!(h.offered().len(), 1);
}

#[test]
fn delivering_an_order_pays_its_reward_and_settles_it() {
    let mut h = Host::new();
    h.place(1, 2, 90, None);
    let id = h.offered()[0].id;
    h.accept(id);
    let before = h.progress.wallet();
    h.deliver(id, 1.0);
    assert_eq!(h.jobs.get(id).unwrap().state, JobState::Completed);
    assert_eq!(h.progress.wallet() - before, 90, "the order's reward, not the template's");
    assert_eq!(h.settled(), vec![(OrderId(1), 2, 2, 1.0, true)]);
}

#[test]
fn damaged_goods_settle_with_their_condition_and_pay_less() {
    let mut h = Host::new();
    h.place(1, 2, 100, None);
    let id = h.offered()[0].id;
    h.accept(id);
    let before = h.progress.wallet();
    h.deliver(id, 0.5);
    // 100 × band 1.0 × (1 - 0.5 × 0.5) = 75
    assert_eq!(h.progress.wallet() - before, 75);
    let s = h.settled();
    assert_eq!((s[0].1, s[0].2, s[0].3), (2, 2, 0.5));
}

#[test]
fn an_order_deadline_expires_the_job_and_settles_it_late() {
    let mut h = Host::new();
    h.place(2, 2, 100, Some(60.0));
    let id = h.offered()[0].id;
    h.accept(id);
    let c = *h.jobs.get(id).unwrap().legs[0].crates.keys().next().unwrap();
    h.world(WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new("wholesale_yard") });
    let left = h.jobs.get(id).unwrap().time_left_of(&h.jc);
    assert_eq!(left, Some(60.0), "the order's deadline counts, from the first pickup");
    h.world(WorldEvent::TimePassed { dt: 61.0 });
    assert_eq!(h.jobs.get(id).unwrap().state, JobState::Expired);
    let s = h.settled();
    assert_eq!((s[0].0, s[0].1, s[0].2, s[0].4), (OrderId(2), 0, 2, false));
}

#[test]
fn abandoning_an_order_settles_it_as_nothing_delivered() {
    let mut h = Host::new();
    h.place(3, 2, 100, None);
    let id = h.offered()[0].id;
    h.accept(id);
    h.seq += 1;
    let out = h.jobs.apply_job(&h.jc, &h.k, &h.progress, &Event::new(ANA, h.seq, JobEvent::JobAbandoned { job: id })).unwrap();
    h.route(out);
    let s = h.settled();
    assert_eq!(s.len(), 1);
    assert_eq!((s[0].0, s[0].1, s[0].2), (OrderId(3), 0, 2));
}

#[test]
fn without_the_order_template_the_event_is_ignored() {
    let mut h = Host::new();
    let mut files = files("tests/fixtures");
    files.retain(|f| f.path != "job_template/customer_order.json");
    h.jc = JobContent::load(&files, &h.k).unwrap();
    h.place(1, 1, 10, None);
    assert!(h.offered().is_empty());
}

#[test]
fn order_jobs_survive_a_save() {
    let mut h = Host::new();
    h.place(5, 2, 90, Some(300.0));
    let mut env = gameplay_core::save::Envelope::new();
    h.jobs.save(&mut env);
    let back = Jobs::load(&gameplay_core::save::Envelope::from_json(&env.to_json()).unwrap()).unwrap().unwrap();
    assert_eq!(back, h.jobs);
    assert_eq!(back.all().next().unwrap().order.as_ref().unwrap().order, OrderId(5));
}
