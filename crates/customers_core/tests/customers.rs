//! Customers of #168: records, seeded orders and their rhythm, satisfaction and relationship,
//! neglect without loss for good, the chain order → offer → delivery → satisfaction through events.
use std::path::Path;

use customers_core::*;
use gameplay_core::notice::NoticeKind;
use gameplay_core::save::Envelope;
use gameplay_core::text::TextTable;
use gameplay_core::{ClientId, Content, CrateId, Event, File, LocationId, OrderId, Progress, WorldEvent};

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

fn table() -> TextTable {
    let f = files("tests/fixtures").into_iter().find(|f| f.path == "text/en.json").unwrap();
    TextTable::from_json(&f.path, &f.text).unwrap()
}

fn content() -> CustomerContent {
    CustomerContent::load(&files("tests/fixtures"), &kernel()).unwrap()
}

fn load_with(path: &str, text: &str) -> Vec<String> {
    let mut f: Vec<File> = files("tests/fixtures").into_iter().filter(|f| f.path != path).collect();
    f.push(File::new(path, text));
    CustomerContent::load(&f, &kernel()).unwrap_err()
}

fn replace(path: &str, from: &str, to: &str) -> Vec<String> {
    let orig = files("tests/fixtures").into_iter().find(|f| f.path == path).unwrap();
    assert!(orig.text.contains(from), "{from} in {path}");
    load_with(path, &orig.text.replace(from, to))
}

fn ev(seq: u64, e: WorldEvent) -> Event<WorldEvent> {
    Event::new(HOST, seq, e)
}

/// The OrderPlaced events among the outcomes.
fn placed(out: &[Outcome]) -> Vec<(OrderId, String, String, String, u32, i64, Option<f64>)> {
    out.iter()
        .filter_map(|o| match o {
            Outcome::Emit(WorldEvent::OrderPlaced { order, by, to, commodity, amount, reward, deadline_s, from }) => Some((*order, by.clone(), format!("{from}>{to}"), commodity.to_string(), *amount, *reward, *deadline_s)),
            _ => None,
        })
        .collect()
}

struct World {
    k: Content,
    cc: CustomerContent,
    c: Customers,
    seq: u64,
    t: f64,
    /// Every order placed, with the time.
    orders: Vec<(f64, OrderId, String, i64)>,
    notices: Vec<gameplay_core::notice::Notice>,
}

impl World {
    fn new(seed: u64) -> World {
        let k = kernel();
        let cc = content();
        let c = Customers::new(&cc, seed);
        World { k, cc, c, seq: 0, t: 0.0, orders: Vec::new(), notices: Vec::new() }
    }

    fn take(&mut self, out: Vec<Outcome>) {
        for o in out {
            match o {
                Outcome::Emit(WorldEvent::OrderPlaced { order, by, reward, .. }) => self.orders.push((self.t, order, by, reward)),
                Outcome::Notice(n) => self.notices.push(n),
                _ => {}
            }
        }
    }

    fn step(&mut self, dt: f64) {
        self.t += dt;
        self.seq += 1;
        let out = self.c.apply_world(&self.cc, &self.k, &ev(self.seq, WorldEvent::TimePassed { dt }));
        self.take(out);
    }

    /// Runs `secs` in 1 s steps; `settle` answers each new order at once with that result.
    fn run(&mut self, secs: f64, settle: Option<(u32, f64, bool)>) {
        for _ in 0..secs as usize {
            let before = self.orders.len();
            self.step(1.0);
            if let Some((share_of, cond, in_time)) = settle {
                for i in before..self.orders.len() {
                    let id = self.orders[i].1;
                    self.settle(id, share_of, 2, cond, in_time);
                }
            }
        }
    }

    fn settle(&mut self, order: OrderId, delivered: u32, asked: u32, condition: f64, in_time: bool) {
        self.seq += 1;
        let out = self.c.apply_world(&self.cc, &self.k, &ev(self.seq, WorldEvent::OrderSettled { order, delivered, asked, condition, in_time }));
        self.take(out);
    }

    fn rel(&self, id: &str) -> f64 {
        self.c.relationship(&CustomerId::new(id))
    }
}

// ---------- loader ----------

#[test]
fn customers_and_the_price_table_load() {
    let cc = content();
    assert_eq!(cc.customers.len(), 2);
    assert_eq!(cc.customers[&CustomerId::new("old_nib")].record.location.as_str(), "drip_rock");
    assert_eq!(cc.prices.record.customer[&gameplay_core::CommodityId::new("sock_dust")], 30);
    assert!(cc.check_texts(&table()).is_empty(), "{:?}", cc.check_texts(&table()));
}

#[test]
fn loader_errors_name_file_and_field() {
    let p = "customer/old_nib.json";
    for (from, to, field) in [
        ("\"drip_rock\"", "\"moon_base\"", "location"),
        ("[\"dry\"]", "[\"frozen\"]", "taste"),
        ("[\"dry\"]", "[]", "taste"),
        ("\"standard\": 0.7", "\"standard\": 1.5", "standard"),
        ("[100.0, 200.0]", "[300.0, 200.0]", "rhythm_s"),
        ("[1, 2]", "[0, 2]", "amount"),
        ("\"start\": 2.0", "\"start\": 9.0", "start"),
        ("\"patience_s\": 600.0", "\"patience_s\": 0.0", "patience_s"),
        ("\"standard\": 0.7,", "\"standard\": 0.7, \"colour\": 1,", "colour"),
    ] {
        let e = replace(p, from, to);
        assert_eq!(e.len(), 1, "{field}: {e:?}");
        assert!(e[0].starts_with(p) && e[0].contains(field), "{field}: {e:?}");
    }
    // A customer must be able to order something from the wholesaler: taste "wet" matches only
    // fizzy_mud, which is fine; a price table without it is not.
    let e = replace("price_table/start.json", "\"fizzy_mud\": 40, ", "");
    assert!(e.iter().any(|m| m.starts_with("price_table/start.json") && m.contains("customer") && m.contains("fizzy_mud")), "{e:?}");
    let e = replace("price_table/start.json", "\"sock_dust\": 30", "\"sock_dust\": 10");
    assert!(e.iter().any(|m| m.starts_with("price_table/start.json") && m.contains("sock_dust") && m.contains("wholesale")), "{e:?}");
    let e = replace("price_table/start.json", "\"fizzy_mud\": 20", "\"moon_dust\": 20");
    assert!(e.iter().any(|m| m.contains("moon_dust")), "{e:?}");
}

#[test]
fn missing_pools_are_reported() {
    let cc = content();
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text/en.json")).unwrap();
    for (key, field) in [("customer.old_nib.thanks", "voice.thanks"), ("customer.old_nib.order_title", "order_title"), ("customer.old_nib.name", "name")] {
        let t = TextTable::from_json("t", &text.replace(&format!("\"{key}\""), "\"x.y\"")).unwrap();
        let e = cc.check_texts(&t);
        assert!(e.iter().any(|m| m.starts_with("customer/old_nib.json") && m.contains(field) && m.contains(key)), "{field}: {e:?}");
    }
    let thin = TextTable::from_json("t", &text.replace("[\"Old Nib nods. High praise.\", \"Old Nib: not bad, not bad.\", \"Old Nib mutters something nice.\"]", "\"Old Nib nods.\"")).unwrap();
    assert!(cc.check_texts(&thin).iter().any(|m| m.contains("voice.thanks") && m.contains("at least")));
}

// ---------- orders ----------

#[test]
fn orders_are_deterministic_for_a_seed_and_differ_between_seeds() {
    let run = |seed| {
        let mut w = World::new(seed);
        w.run(3000.0, Some((2, 1.0, true)));
        w.orders.iter().map(|(t, o, by, r)| (*t, *o, by.clone(), *r)).collect::<Vec<_>>()
    };
    let (a, b, c) = (run(11), run(11), run(12));
    assert!(a.len() > 10, "{} orders in 3000 s", a.len());
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn the_first_order_comes_within_the_rhythm_scaled_by_the_relationship() {
    for seed in 0..20 {
        let mut w = World::new(seed);
        w.run(1500.0, None);
        let first = |by: &str| w.orders.iter().find(|o| o.2 == by).map(|o| o.0).unwrap();
        // old_nib: rhythm 100..200 at relationship 2.0; moss_hat: 300 at relationship 1.0.
        let (fa, fb) = (rhythm_factor(2.0), rhythm_factor(1.0));
        let t = first("old_nib");
        assert!(t >= 100.0 * fa - 1.0 && t <= 200.0 * fa + 1.0, "seed {seed}: old_nib first order at {t:.0} s, want {:.0} to {:.0}", 100.0 * fa, 200.0 * fa);
        let t = first("moss_hat");
        assert!((t - 300.0 * fb).abs() <= 1.5, "seed {seed}: moss_hat first order at {t:.0} s, want {:.0}", 300.0 * fb);
    }
}

#[test]
fn an_order_is_from_the_wholesaler_to_the_customer_for_goods_of_its_taste() {
    let mut w = World::new(5);
    let mut out = Vec::new();
    for _ in 0..600 {
        w.t += 1.0;
        w.seq += 1;
        out.extend(w.c.apply_world(&w.cc, &w.k, &ev(w.seq, WorldEvent::TimePassed { dt: 1.0 })));
    }
    let all = placed(&out);
    assert_eq!(all.len(), 2, "one order each, no second one while the first is open");
    for (_, by, route, commodity, amount, reward, deadline) in all {
        match by.as_str() {
            "old_nib" => {
                assert_eq!((route.as_str(), commodity.as_str()), ("wholesale_yard>drip_rock", "sock_dust"));
                assert!((1..=2).contains(&amount));
                // 30 per crate at relationship 2.0 (+4 % each point): 30 × 1.08 = 32.4 per crate
                assert_eq!(reward, (amount as f64 * 30.0 * pay_factor(2.0)).round() as i64);
                assert_eq!(deadline, Some(600.0));
            }
            "moss_hat" => {
                assert_eq!((route.as_str(), commodity.as_str(), amount), ("wholesale_yard>bent_spoon", "fizzy_mud", 2));
                assert_eq!(reward, (2.0 * 40.0 * pay_factor(1.0)).round() as i64);
                assert_eq!(deadline, None);
            }
            other => panic!("{other}"),
        }
    }
}

#[test]
fn placing_an_order_raises_a_notice_with_the_customer() {
    let mut w = World::new(5);
    w.run(600.0, None);
    let n: Vec<_> = w.notices.iter().filter(|n| n.key.as_str() == "notice.order.placed").collect();
    assert_eq!(n.len(), 2);
    assert!(n.iter().all(|n| n.kind == NoticeKind::Available && n.args.iter().any(|(k, _)| k == "customer")));
}

// ---------- satisfaction and relationship ----------

#[test]
fn a_good_delivery_raises_the_relationship_and_a_bad_one_lowers_it() {
    let mut w = World::new(1);
    w.run(600.0, None);
    let (a, b) = (w.orders.iter().find(|o| o.2 == "old_nib").unwrap().1, w.orders.iter().find(|o| o.2 == "moss_hat").unwrap().1);
    let (rel_a, rel_b) = (w.rel("old_nib"), w.rel("moss_hat"));
    w.settle(a, 2, 2, 1.0, true);
    assert!(w.rel("old_nib") > rel_a + 0.5, "{} from {rel_a}", w.rel("old_nib"));
    // Half the goods, wrecked, late.
    w.settle(b, 1, 2, 0.1, false);
    assert!(w.rel("moss_hat") < rel_b, "{} from {rel_b}", w.rel("moss_hat"));
    // The customers answer in their own voice.
    let keys: Vec<&str> = w.notices.iter().map(|n| n.key.as_str()).collect();
    assert!(keys.contains(&"customer.old_nib.thanks") && keys.contains(&"customer.moss_hat.grumble"), "{keys:?}");
}

#[test]
fn satisfaction_follows_share_condition_and_time() {
    let c = &content().customers[&CustomerId::new("old_nib")].record.clone();
    let perfect = satisfaction(c, 2, 2, 1.0, true);
    assert!(perfect > 0.5);
    assert!(satisfaction(c, 2, 2, 0.8, true) < perfect + 1e-9, "above the standard is as good");
    assert!(satisfaction(c, 2, 2, 0.4, true) < 0.0 + perfect - 0.5, "below the standard hurts");
    assert!(satisfaction(c, 2, 2, 1.0, false) < perfect, "late hurts");
    assert!(satisfaction(c, 1, 2, 1.0, true) < perfect, "half the goods hurt");
    assert!(satisfaction(c, 0, 2, 1.0, true) < -0.5, "nothing at all is the worst");
}

#[test]
fn the_relationship_stays_between_zero_and_five() {
    let mut w = World::new(2);
    w.run(20000.0, Some((2, 1.0, true)));
    assert!((w.rel("old_nib") - 5.0).abs() < 1e-9, "{}", w.rel("old_nib"));
    let mut w = World::new(2);
    w.run(20000.0, Some((0, 1.0, false)));
    assert_eq!(w.rel("old_nib"), 0.0);
    assert_eq!(w.rel("moss_hat"), 0.0);
}

#[test]
fn a_regular_orders_more_and_pays_a_little_more() {
    let (mut happy, mut unhappy) = (World::new(3), World::new(3));
    happy.run(12000.0, Some((2, 1.0, true)));
    unhappy.run(12000.0, Some((0, 1.0, false)));
    let n = |w: &World| w.orders.iter().filter(|o| o.2 == "old_nib").count();
    assert!(n(&happy) > 2 * n(&unhappy), "{} orders when happy, {} when unhappy", n(&happy), n(&unhappy));
    assert!(n(&unhappy) >= 5, "an unhappy customer still orders now and then: {}", n(&unhappy));
    assert!(pay_factor(5.0) > pay_factor(0.0) && pay_factor(5.0) < 1.5, "a little more, not a lot");
    assert!(rhythm_factor(5.0) < rhythm_factor(0.0));
    assert!(rhythm_factor(0.0) <= 3.0 && rhythm_factor(5.0) >= 0.2);
}

#[test]
fn a_neglected_customer_orders_less_but_is_never_lost_and_can_be_won_back() {
    let mut w = World::new(4);
    w.run(600.0, None);
    // Serve old_nib perfectly a few times so the relationship is high.
    for _ in 0..6 {
        let id = w.orders.iter().rev().find(|o| o.2 == "old_nib").map(|o| o.1).unwrap();
        w.settle(id, 2, 2, 1.0, true);
        w.run(300.0, None);
    }
    let high = w.rel("old_nib");
    assert!(high > 3.5, "{high}");
    // Ten hours without a delivery: the relationship sinks to its floor, not below.
    let before = w.orders.len();
    w.run(36000.0, None);
    let floor = w.rel("old_nib");
    assert!(floor < high && floor >= NEGLECT_FLOOR - 1e-9, "sank to {floor:.2} from {high:.2}");
    assert!(floor <= NEGLECT_FLOOR + 1e-9, "all the way to the floor: {floor}");
    // Nobody settled the open order, so no new one came: that is the customer waiting, not lost.
    assert_eq!(w.orders.len() - before, 0);
    // Serve it once: orders come again and the relationship recovers with work.
    let id = w.orders.iter().rev().find(|o| o.2 == "old_nib").map(|o| o.1).unwrap();
    w.settle(id, 2, 2, 1.0, true);
    assert!(w.rel("old_nib") > floor + 0.5);
    w.run(2000.0, None);
    assert!(w.orders.len() > before, "it orders again");
}

#[test]
fn settling_an_unknown_or_twice_settled_order_changes_nothing() {
    let mut w = World::new(1);
    w.run(400.0, None);
    let a = w.orders.iter().find(|o| o.2 == "old_nib").unwrap().1;
    w.settle(OrderId(999), 2, 2, 1.0, true);
    let r = w.rel("old_nib");
    w.settle(a, 2, 2, 1.0, true);
    let r2 = w.rel("old_nib");
    assert!(r2 > r);
    w.settle(a, 2, 2, 1.0, true);
    assert_eq!(w.rel("old_nib"), r2, "the second settlement is a repeat");
}

#[test]
fn customer_lines_never_repeat_twice_in_a_row() {
    // The notices carry the pool key; the glue's picker takes the line.
    let t = table();
    let mut p = gameplay_core::text::Picker::new(3);
    for key in ["customer.old_nib.thanks", "customer.old_nib.grumble", "customer.moss_hat.order"] {
        let mut last = String::new();
        for _ in 0..30 {
            let line = p.pick(&t, key);
            assert_ne!(line, last, "{key}");
            last = line;
        }
    }
}

// ---------- the chain through events, with the jobs system ----------

struct Chain {
    k: Content,
    jc: jobs_core::JobContent,
    cc: CustomerContent,
    c: Customers,
    progress: Progress,
    jobs: jobs_core::Jobs,
    seq: u64,
    next_crate: u64,
    notices: Vec<gameplay_core::notice::Notice>,
}

impl Chain {
    fn new(seed: u64) -> Chain {
        let k = kernel();
        let job_files = files("../jobs_core/tests/fixtures");
        let jc = jobs_core::JobContent::load(&job_files, &k).unwrap();
        let cc = content();
        let c = Customers::new(&cc, seed);
        let progress = Progress::new(&k);
        Chain { k, jc, cc, c, progress, jobs: jobs_core::Jobs::default(), seq: 0, next_crate: 1, notices: Vec::new() }
    }

    /// One world event to every system; the outcomes go back in as events.
    fn world(&mut self, from: ClientId, e: WorldEvent) {
        self.seq += 1;
        let ev = Event::new(from, self.seq, e);
        self.progress.apply(&self.k, &ev).unwrap();
        let outs = self.jobs.apply_world(&self.jc, &ev);
        self.jobs_out(outs);
        let cust = self.c.apply_world(&self.cc, &self.k, &ev);
        for o in cust {
            match o {
                Outcome::Emit(e) => self.world(HOST, e),
                Outcome::Notice(n) => self.notices.push(n),
            }
        }
    }

    fn jobs_out(&mut self, outs: Vec<jobs_core::Outcome>) {
        for o in outs {
            match o {
                jobs_core::Outcome::SpawnCrates { job, leg, count, .. } => {
                    let crates = (0..count).map(|_| {
                        self.next_crate += 1;
                        CrateId(self.next_crate - 1)
                    }).collect();
                    self.seq += 1;
                    let ev = Event::new(HOST, self.seq, jobs_core::JobEvent::CratesSpawned { job, leg, crates });
                    self.jobs.apply_job(&self.jc, &self.k, &self.progress, &ev).unwrap();
                }
                jobs_core::Outcome::Emit(e) => self.world(HOST, e),
                _ => {}
            }
        }
    }

    fn wait(&mut self, secs: u32) {
        for _ in 0..secs {
            self.world(HOST, WorldEvent::TimePassed { dt: 1.0 });
        }
    }

    fn offer_of(&self, by: &str) -> Option<jobs_core::JobId> {
        self.jobs.all().find(|j| j.state == jobs_core::JobState::Offered && j.order.as_ref().is_some_and(|o| o.by == by)).map(|j| j.id)
    }

    fn accept(&mut self, job: jobs_core::JobId) {
        self.seq += 1;
        let out = self.jobs.apply_job(&self.jc, &self.k, &self.progress, &Event::new(ANA, self.seq, jobs_core::JobEvent::OfferAccepted { job })).unwrap();
        self.jobs_out(out);
    }

    fn deliver(&mut self, job: jobs_core::JobId, to: &str, cond: f64) {
        let leg = self.jobs.get(job).unwrap().legs[0].clone();
        for c in leg.crates.keys() {
            self.world(ANA, WorldEvent::CratePickedUp { crate_id: *c, at: leg.from.clone() });
            self.world(ANA, WorldEvent::CrateDelivered { crate_id: *c, at: LocationId::new(to), condition: cond });
        }
    }
}

#[test]
fn an_order_becomes_an_offer_and_its_delivery_changes_how_the_customer_feels() {
    let mut ch = Chain::new(8);
    ch.wait(400);
    let job = ch.offer_of("old_nib").expect("the order is on offer");
    let j = ch.jobs.get(job).unwrap();
    assert_eq!(j.legs[0].from.as_str(), "wholesale_yard");
    assert_eq!(j.legs[0].to.as_str(), "drip_rock");
    assert_eq!(j.legs[0].commodity.as_str(), "sock_dust");
    let reward = j.order.as_ref().unwrap().reward;
    ch.accept(job);
    let wallet = ch.progress.wallet();
    let rel = ch.c.relationship(&CustomerId::new("old_nib"));
    ch.deliver(job, "drip_rock", 1.0);
    assert_eq!(ch.progress.wallet() - wallet, reward, "the order's reward is paid");
    let after = ch.c.relationship(&CustomerId::new("old_nib"));
    assert!(after > rel, "{after} from {rel}");
    assert!(ch.notices.iter().any(|n| n.key.as_str() == "customer.old_nib.thanks"));
    // Orders come again.
    ch.wait(2000);
    assert!(ch.offer_of("old_nib").is_some());
}

#[test]
fn damaged_late_or_missing_goods_lower_the_relationship_through_the_same_chain() {
    let mut ch = Chain::new(9);
    ch.wait(400);
    let job = ch.offer_of("old_nib").unwrap();
    ch.accept(job);
    let rel = ch.c.relationship(&CustomerId::new("old_nib"));
    ch.deliver(job, "drip_rock", 0.2);
    let after = ch.c.relationship(&CustomerId::new("old_nib"));
    assert!(after < rel + 0.45, "wrecked goods earn little: {after} from {rel}");
    assert!(ch.notices.iter().any(|n| n.key.as_str() == "customer.old_nib.grumble"), "{:?}", ch.notices.iter().map(|n| n.key.to_string()).collect::<Vec<_>>());

    // An order that is never delivered runs out of its patience (600 s): the job expires and the
    // customer is let down.
    ch.wait(2000);
    let job = ch.offer_of("old_nib").unwrap();
    ch.accept(job);
    let c = *ch.jobs.get(job).unwrap().legs[0].crates.keys().next().unwrap();
    ch.world(ANA, WorldEvent::CratePickedUp { crate_id: c, at: LocationId::new("wholesale_yard") });
    let rel = ch.c.relationship(&CustomerId::new("old_nib"));
    ch.wait(700);
    assert_eq!(ch.jobs.get(job).unwrap().state, jobs_core::JobState::Expired);
    assert!(ch.c.relationship(&CustomerId::new("old_nib")) < rel, "late and empty-handed");
}

// ---------- save ----------

#[test]
fn the_customers_section_round_trips_and_continues_the_same() {
    let mut w = World::new(7);
    w.run(1500.0, Some((2, 0.9, true)));
    let mut env = Envelope::new();
    w.c.save(&mut env);
    let back = Customers::load(&Envelope::from_json(&env.to_json()).unwrap()).unwrap().unwrap();
    assert_eq!(back, w.c, "state → JSON → state is equal");
    let mut w2 = World { k: kernel(), cc: content(), c: back, seq: w.seq, t: w.t, orders: Vec::new(), notices: Vec::new() };
    let n = w.orders.len();
    w.run(1500.0, Some((2, 0.9, true)));
    w2.run(1500.0, Some((2, 0.9, true)));
    assert_eq!(w.orders[n..], w2.orders[..], "the same orders follow after a load");
    let mut bad = Envelope::new();
    bad.put("customers", 99, &w.c);
    assert!(Customers::load(&bad).is_err());
    assert_eq!(Customers::load(&Envelope::new()).unwrap(), None);
}
