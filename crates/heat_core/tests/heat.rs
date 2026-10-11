//! Heat of #227: the wanted level, scans and the four answers, paying it down, save round trip.
//! Every rule of the spec has a test; the tuning is the SHIPPED `content/gameplay/heat/` record.
use std::path::Path;

use content_core::File;
use gameplay_core::content::WALLET;
use gameplay_core::notice::NoticeKind;
use gameplay_core::save::Envelope;
use gameplay_core::text::TextTable;
use gameplay_core::{ClientId, Event, OrderId, ScanChoice, ViolationKind, WorldEvent, TownId, LocationId, TrackId};
use heat_core::*;

const HOST: ClientId = ClientId(1);

fn shipped_files(sub: &str) -> Vec<File> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/gameplay").join(sub);
    let mut out = Vec::new();
    for f in std::fs::read_dir(&root).unwrap() {
        let f = f.unwrap().path();
        out.push(File::new(format!("{sub}/{}", f.file_name().unwrap().to_str().unwrap()), std::fs::read_to_string(&f).unwrap()));
    }
    out
}

fn shipped() -> HeatContent {
    HeatContent::load(&shipped_files("heat")).unwrap()
}

fn tuning() -> Tuning {
    shipped().tuning.record
}

/// The shipped tuning with one replacement in its text; the loader's errors come back.
fn load_with(from: &str, to: &str) -> Vec<String> {
    let f = shipped_files("heat").into_iter().next().unwrap();
    assert!(f.text.contains(from), "{from} in {}", f.path);
    HeatContent::load(&[File::new(f.path, f.text.replace(from, to))]).unwrap_err()
}

fn ev(seq: u64, e: WorldEvent) -> Event<WorldEvent> {
    Event::new(HOST, seq, e)
}

fn town() -> TownId {
    TownId::new("slosh_town")
}

/// Drives a heat state with events, numbering them itself.
struct World {
    c: HeatContent,
    h: Heat,
    seq: u64,
    out: Vec<Outcome>,
}

impl World {
    fn new(seed: u64) -> World {
        World { c: shipped(), h: Heat::new(seed), seq: 0, out: Vec::new() }
    }

    fn send(&mut self, e: WorldEvent) -> Vec<Outcome> {
        self.seq += 1;
        let out = self.h.apply_world(&self.c, &ev(self.seq, e));
        self.out.extend(out.iter().cloned());
        out
    }

    fn violate(&mut self, kind: ViolationKind) -> Vec<Outcome> {
        self.send(WorldEvent::Violation { town: town(), kind })
    }

    fn pass(&mut self, dt: f64) -> Vec<Outcome> {
        self.send(WorldEvent::TimePassed { dt })
    }

    /// Points up to `pts` by violations of no_clearance (15 each) and wrong_pad (5 each).
    fn heat_to(&mut self, pts: u32) {
        let fifteen = pts / 15;
        for _ in 0..fifteen {
            self.violate(ViolationKind::NoClearance);
        }
        for _ in 0..(pts % 15) / 5 {
            self.violate(ViolationKind::WrongPad);
        }
        assert_eq!(self.h.points(), pts as f64);
    }

    fn scan(&mut self, scan: u64, visible: u32, hidden: u32) -> Vec<Outcome> {
        self.send(WorldEvent::ScanStarted { scan, at: LocationId::new("bent_spoon"), visible, hidden })
    }

    fn answer(&mut self, scan: u64, choice: ScanChoice) -> Vec<Outcome> {
        self.send(WorldEvent::ScanAnswered { scan, choice })
    }

    fn pay(&mut self, amount: i64) -> Vec<Outcome> {
        self.send(WorldEvent::HeatPaidDown { amount })
    }
}

/// The emitted events of an outcome list.
fn emits(out: &[Outcome]) -> Vec<WorldEvent> {
    out.iter()
        .filter_map(|o| match o {
            Outcome::Emit(e) => Some(e.clone()),
            Outcome::Notice(_) => None,
        })
        .collect()
}

fn notices(out: &[Outcome]) -> Vec<&gameplay_core::notice::Notice> {
    out.iter()
        .filter_map(|o| match o {
            Outcome::Notice(n) => Some(n),
            Outcome::Emit(_) => None,
        })
        .collect()
}

fn resolved(out: &[Outcome]) -> (ScanChoice, bool, i64, f64) {
    let found: Vec<_> = emits(out)
        .into_iter()
        .filter_map(|e| match e {
            WorldEvent::ScanResolved { choice, confiscate, fine, heat, .. } => Some((choice, confiscate, fine, heat)),
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "exactly one ScanResolved, got {found:?}");
    found[0]
}

fn wallet_changes(out: &[Outcome]) -> Vec<i64> {
    emits(out)
        .into_iter()
        .filter_map(|e| match e {
            WorldEvent::TrackChanged { track, delta, player: None } if track == TrackId::new(WALLET) => Some(delta),
            _ => None,
        })
        .collect()
}

fn star_events(out: &[Outcome]) -> Vec<u8> {
    emits(out)
        .into_iter()
        .filter_map(|e| match e {
            WorldEvent::HeatStars { stars } => Some(stars),
            _ => None,
        })
        .collect()
}

// ---- Tuning record and loader

#[test]
fn shipped_tuning_loads_with_the_spec_values() {
    let t = tuning();
    assert_eq!(t.points_per_star, 20.0);
    assert_eq!(t.max_points, 100.0);
    assert_eq!(t.decay_per_min, 4.0);
    assert_eq!(t.decay_per_min_clean, 8.0);
    assert_eq!(t.violation_points.no_clearance, 15.0);
    assert_eq!(t.violation_points.wrong_pad, 5.0);
    assert_eq!(t.violation_points.speeding, 6.0);
    assert_eq!(t.violation_points.fled_scan, 25.0);
    assert_eq!(t.deal_points, 4.0);
    assert_eq!(t.scan_chance, vec![0.1, 0.25, 0.45, 0.65, 0.85, 1.0]);
    assert_eq!(t.jammer_factor, 0.5);
    assert_eq!(t.fine_per_crate, 40);
    assert_eq!(t.fine_base, 25);
    assert_eq!(t.bribe_base, 60);
    assert_eq!(t.talk_chance, vec![0.7, 0.55, 0.4, 0.25, 0.15, 0.1]);
    assert_eq!(t.talk_fail_fine_factor, 1.5);
    assert_eq!(t.talk_fail_points, 10.0);
    assert_eq!(t.flee_points, 25.0);
    assert_eq!(t.comply_points, -5.0);
    assert_eq!(t.thorough_from_stars, 4);
    assert_eq!(t.thorough_chance, 0.5);
    assert_eq!(t.paydown_per_point, 10);
}

#[test]
fn loader_errors_name_the_file_and_the_field() {
    let missing = {
        let f = shipped_files("heat").into_iter().next().unwrap();
        let text = f.text.replace("\"fine_base\": 25,", "");
        HeatContent::load(&[File::new(f.path, text)]).unwrap_err()
    };
    assert!(missing.iter().any(|e| e.starts_with("heat/tuning.json") && e.contains("fine_base")), "{missing:?}");

    let unknown = load_with("\"deal_points\": 4.0,", "\"deal_points\": 4.0,\n  \"deal_pointz\": 1,");
    assert!(unknown.iter().any(|e| e.starts_with("heat/tuning.json") && e.contains("deal_pointz")), "{unknown:?}");

    let short = load_with("[0.1, 0.25, 0.45, 0.65, 0.85, 1.0]", "[0.1, 0.25]");
    assert!(short.iter().any(|e| e.starts_with("heat/tuning.json") && e.contains("scan_chance")), "{short:?}");

    let big = load_with("\"jammer_factor\": 0.5", "\"jammer_factor\": 1.5");
    assert!(big.iter().any(|e| e.contains("jammer_factor")), "{big:?}");

    let paydown = load_with("\"paydown_per_point\": 10", "\"paydown_per_point\": 0");
    assert!(paydown.iter().any(|e| e.contains("paydown_per_point")), "{paydown:?}");

    let stars = load_with("\"max_points\": 100.0", "\"max_points\": 60.0");
    assert!(stars.iter().any(|e| e.contains("max_points")), "{stars:?}");

    let thorough = load_with("\"thorough_from_stars\": 4", "\"thorough_from_stars\": 6");
    assert!(thorough.iter().any(|e| e.contains("thorough_from_stars")), "{thorough:?}");
}

#[test]
fn shipped_texts_for_the_star_notices_exist() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/gameplay/text/en.json");
    let table = TextTable::from_json("text/en.json", &std::fs::read_to_string(path).unwrap()).unwrap();
    for key in [NOTICE_UP, NOTICE_DOWN] {
        let lines = table.lines(key).unwrap_or_else(|| panic!("no text '{key}'"));
        assert!(lines.len() >= 3, "{key} needs 3 lines");
        assert!(lines.iter().all(|l| l.contains("{stars}")), "{key} shows the stars");
    }
}

// ---- Stars and chances

#[test]
fn stars_follow_points_and_cap_at_five() {
    let mut w = World::new(1);
    assert_eq!(w.h.stars(), 0);
    w.heat_to(60);
    assert_eq!(w.h.stars(), 3);
    let mut top = World::new(1);
    top.heat_to(100);
    assert_eq!(top.h.stars(), 5);
    assert_eq!(top.h.points(), 100.0);
}

#[test]
fn points_never_pass_the_maximum() {
    let mut w = World::new(1);
    w.heat_to(90);
    w.violate(ViolationKind::NoClearance);
    assert_eq!(w.h.points(), 100.0);
    w.violate(ViolationKind::FledScan);
    assert_eq!(w.h.points(), 100.0);
}

#[test]
fn scan_chance_per_star_matches_the_table() {
    let t = tuning();
    for s in 0..=5u8 {
        assert_eq!(Heat::scan_chance(&t, s, false, false), t.scan_chance[s as usize], "stars {s}");
    }
}

#[test]
fn the_jammer_halves_the_chance() {
    let t = tuning();
    assert_eq!(Heat::scan_chance(&t, 2, false, true), 0.45 * 0.5);
    assert_eq!(Heat::scan_chance(&t, 5, false, true), 1.0 * 0.5);
}

#[test]
fn an_inspection_pad_is_certain_with_or_without_a_jammer() {
    let t = tuning();
    for s in 0..=5u8 {
        assert_eq!(Heat::scan_chance(&t, s, true, false), 1.0);
        assert_eq!(Heat::scan_chance(&t, s, true, true), 1.0);
    }
}

#[test]
fn roll_scan_inspection_always_scans_and_a_normal_roll_follows_the_chance() {
    let t = tuning();
    let mut sure = Heat::new(9);
    for _ in 0..200 {
        assert!(sure.roll_scan(&t, true, false));
    }
    let mut hits = 0;
    let mut h = Heat::new(9);
    for _ in 0..1000 {
        if h.roll_scan(&t, false, false) {
            hits += 1;
        }
    }
    // Stars 0: 10 % per roll. The seed is fixed, so this only guards the rule, not a number.
    assert!((60..=140).contains(&hits), "hits {hits}");
}

#[test]
fn a_jammed_roll_hits_less_often_than_a_plain_one() {
    let t = tuning();
    let (mut plain, mut jammed) = (World::new(4), World::new(4));
    plain.heat_to(60);
    jammed.heat_to(60);
    assert_eq!(plain.h.stars(), 3);
    let p = (0..2000).filter(|_| plain.h.roll_scan(&t, false, false)).count();
    let j = (0..2000).filter(|_| jammed.h.roll_scan(&t, false, true)).count();
    assert!(j < p, "jammed {j} plain {p}");
}

// ---- Points: violations, deals, decay, paydown

#[test]
fn each_violation_kind_adds_its_points() {
    let t = tuning();
    for (kind, want) in [
        (ViolationKind::NoClearance, t.violation_points.no_clearance),
        (ViolationKind::WrongPad, t.violation_points.wrong_pad),
        (ViolationKind::Speeding, t.violation_points.speeding),
        (ViolationKind::FledScan, t.violation_points.fled_scan),
    ] {
        let mut w = World::new(1);
        w.violate(kind);
        assert_eq!(w.h.points(), want, "{kind:?}");
    }
}

#[test]
fn a_settled_deal_adds_deal_points() {
    let mut w = World::new(1);
    w.send(WorldEvent::ProductOrderSettled { order: OrderId(1), paid: 300, matched: 0.8 });
    assert_eq!(w.h.points(), tuning().deal_points);
}

#[test]
fn decay_takes_the_dirty_rate_until_five_quiet_minutes_then_the_clean_rate() {
    let mut w = World::new(1);
    w.heat_to(60);
    // Quiet time is counted before the tick: the first tick is still a dirty rate (4 per min).
    w.pass(300.0);
    assert_eq!(w.h.points(), 60.0 - 20.0);
    // Five minutes passed without anything illegal: the clean rate (8 per min) now.
    w.pass(60.0);
    assert_eq!(w.h.points(), 40.0 - 8.0);
}

#[test]
fn a_violation_puts_decay_back_to_the_dirty_rate() {
    let mut w = World::new(1);
    w.heat_to(60);
    w.pass(300.0);
    w.violate(ViolationKind::WrongPad);
    let before = w.h.points();
    w.pass(60.0);
    assert_eq!(w.h.points(), before - 4.0);
}

#[test]
fn decay_never_goes_below_zero() {
    let mut w = World::new(1);
    w.heat_to(30);
    w.pass(60.0 * 1000.0);
    assert_eq!(w.h.points(), 0.0);
    assert_eq!(w.h.stars(), 0);
}

#[test]
fn a_clean_scan_brings_the_clean_rate_at_once() {
    let mut w = World::new(1);
    w.heat_to(60);
    w.scan(1, 0, 0);
    assert_eq!(w.h.points(), 55.0);
    w.pass(60.0);
    assert_eq!(w.h.points(), 55.0 - 8.0);
}

#[test]
fn paying_down_takes_money_over_the_rate_and_charges_the_wallet() {
    let mut w = World::new(1);
    w.heat_to(60);
    let out = w.pay(200);
    assert_eq!(w.h.points(), 60.0 - 20.0);
    assert_eq!(wallet_changes(&out), vec![-200]);
}

#[test]
fn paying_down_can_always_bring_heat_to_zero() {
    let mut w = World::new(1);
    w.heat_to(60);
    w.pass(7.0); // a fraction of a point is left over
    let owed = (w.h.points() * 10.0).ceil() as i64;
    w.pay(owed);
    assert_eq!(w.h.points(), 0.0);
    assert_eq!(w.h.stars(), 0);
}

#[test]
fn paying_more_than_owed_stops_at_zero() {
    let mut w = World::new(1);
    w.heat_to(30);
    let out = w.pay(10_000);
    assert_eq!(w.h.points(), 0.0);
    assert_eq!(wallet_changes(&out), vec![-10_000], "the amount is charged as paid");
}

#[test]
fn a_zero_or_negative_payment_changes_nothing() {
    let mut w = World::new(1);
    w.heat_to(30);
    assert!(w.pay(0).is_empty());
    assert!(w.pay(-50).is_empty());
    assert_eq!(w.h.points(), 30.0);
}

#[test]
fn no_mistake_locks_you_out_for_good() {
    let mut w = World::new(1);
    w.heat_to(45);
    w.pay(10_000);
    w.violate(ViolationKind::NoClearance);
    assert_eq!(w.h.points(), 15.0);
    assert_eq!(w.h.stars(), 0);
}

// ---- Star crossings

#[test]
fn a_star_crossing_up_raises_heat_stars_and_a_notice() {
    let mut w = World::new(1);
    let out = w.violate(ViolationKind::NoClearance); // 15 points: no star yet
    assert!(star_events(&out).is_empty());
    let out = w.violate(ViolationKind::WrongPad); // 20 points: one star
    assert_eq!(star_events(&out), vec![1]);
    let n = notices(&out);
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].key.as_str(), NOTICE_UP);
    assert_eq!(n[0].kind, NoticeKind::Warning);
}

#[test]
fn a_star_crossing_down_raises_heat_stars_and_a_notice() {
    let mut w = World::new(1);
    w.heat_to(40);
    assert_eq!(w.h.stars(), 2);
    let out = w.pay(150); // 40 - 15 = 25 points: one star
    assert_eq!(star_events(&out), vec![1]);
    assert_eq!(notices(&out).len(), 1);
    assert_eq!(notices(&out)[0].key.as_str(), NOTICE_DOWN);
    let out = w.pay(1000); // 0 points
    assert_eq!(star_events(&out), vec![0]);
}

#[test]
fn no_star_change_means_no_star_event_and_no_notice() {
    let mut w = World::new(1);
    w.heat_to(25);
    let out = w.violate(ViolationKind::Speeding); // 31 points, still one star
    assert_eq!(w.h.stars(), 1);
    assert!(star_events(&out).is_empty());
    assert!(notices(&out).is_empty());
}

#[test]
fn decay_that_crosses_a_star_also_raises_it() {
    let mut w = World::new(1);
    w.heat_to(20);
    let out = w.pass(60.0); // 20 - 4 = 16 points: no star
    assert_eq!(w.h.stars(), 0);
    assert_eq!(star_events(&out), vec![0]);
}

// ---- Scans: the start

#[test]
fn a_scan_with_nothing_visible_is_clean_at_once() {
    let mut w = World::new(1);
    w.heat_to(30);
    let out = w.scan(7, 0, 0);
    assert_eq!(resolved(&out), (ScanChoice::Comply, false, 0, -5.0));
    assert_eq!(w.h.open_scan(), None);
    assert!(wallet_changes(&out).is_empty());
    assert_eq!(w.h.points(), 25.0);
}

#[test]
fn hidden_crates_stay_hidden_below_the_thorough_star_level() {
    let mut w = World::new(1);
    w.heat_to(60); // three stars, below thorough_from_stars (4)
    let out = w.scan(7, 0, 3);
    assert_eq!(resolved(&out).2, 0);
    assert_eq!(w.h.open_scan(), None);
}

#[test]
fn a_thorough_roll_can_find_the_hidden_hold_from_four_stars_on() {
    let mut found = 0;
    let mut clean = 0;
    for seed in 0..40 {
        let mut w = World::new(seed);
        w.heat_to(80); // four stars
        let out = w.scan(7, 0, 3);
        if w.h.open_scan() == Some(7) {
            found += 1;
            assert!(resolved_absent(&out));
        } else {
            clean += 1;
            assert_eq!(resolved(&out).3, -5.0);
        }
    }
    assert!(found > 0 && clean > 0, "found {found}, clean {clean}");
}

fn resolved_absent(out: &[Outcome]) -> bool {
    emits(out).iter().all(|e| !matches!(e, WorldEvent::ScanResolved { .. }))
}

#[test]
fn a_second_scan_while_one_is_open_is_ignored() {
    let mut w = World::new(1);
    w.scan(1, 2, 0);
    let out = w.scan(2, 5, 0);
    assert!(out.is_empty());
    assert_eq!(w.h.open_scan(), Some(1));
}

// ---- Scans: the answers

#[test]
fn comply_confiscates_and_fines_by_the_visible_crates() {
    let mut w = World::new(1);
    w.heat_to(30);
    w.scan(1, 2, 0);
    let out = w.answer(1, ScanChoice::Comply);
    assert_eq!(resolved(&out), (ScanChoice::Comply, true, 25 + 2 * 40, -5.0));
    assert_eq!(wallet_changes(&out), vec![-105]);
    assert_eq!(w.h.points(), 25.0);
    assert_eq!(w.h.open_scan(), None);
}

#[test]
fn bribe_costs_more_the_hotter_you_are_and_takes_nothing() {
    let mut w = World::new(1);
    w.heat_to(45); // two stars
    w.scan(1, 3, 0);
    let out = w.answer(1, ScanChoice::Bribe);
    assert_eq!(resolved(&out), (ScanChoice::Bribe, false, 60 * 3, 0.0));
    assert_eq!(wallet_changes(&out), vec![-180]);
    assert_eq!(w.h.points(), 45.0);
}

#[test]
fn flee_takes_no_loss_now_and_adds_heat() {
    let mut w = World::new(1);
    w.heat_to(30);
    w.scan(1, 2, 0);
    let out = w.answer(1, ScanChoice::Flee);
    assert_eq!(resolved(&out), (ScanChoice::Flee, false, 0, 25.0));
    assert!(wallet_changes(&out).is_empty());
    assert_eq!(w.h.points(), 55.0);
}

#[test]
fn talk_has_both_branches_with_fixed_seeds() {
    let t = tuning();
    let mut won = false;
    let mut lost = false;
    for seed in 0..60 {
        let mut w = World::new(seed);
        w.heat_to(30); // one star: talk chance 0.55
        w.scan(1, 2, 0);
        let out = w.answer(1, ScanChoice::Talk);
        let (choice, confiscate, fine, heat) = resolved(&out);
        assert_eq!(choice, ScanChoice::Talk);
        if confiscate {
            lost = true;
            let fine_want = ((t.fine_base + t.fine_per_crate * 2) as f64 * t.talk_fail_fine_factor).round() as i64;
            assert_eq!(fine, fine_want);
            assert_eq!(heat, t.talk_fail_points);
            assert_eq!(wallet_changes(&out), vec![-fine_want]);
        } else {
            won = true;
            assert_eq!(fine, 0);
            assert_eq!(heat, 0.0);
            assert!(wallet_changes(&out).is_empty());
        }
    }
    assert!(won && lost, "won {won}, lost {lost}");
}

#[test]
fn an_answer_to_an_unknown_or_closed_scan_raises_nothing() {
    let mut w = World::new(1);
    assert!(w.answer(9, ScanChoice::Comply).is_empty());
    w.heat_to(30);
    w.scan(1, 1, 0);
    w.answer(1, ScanChoice::Flee);
    assert!(w.answer(1, ScanChoice::Comply).is_empty());
    assert_eq!(w.h.points(), 55.0);
}

// ---- Save

#[test]
fn save_round_trip_keeps_the_state_and_the_open_scan() {
    let mut w = World::new(5);
    w.heat_to(45);
    w.scan(3, 2, 0);
    let mut env = Envelope::new();
    w.h.save(&mut env);
    let back = Heat::load(&env).unwrap().expect("heat section");
    assert_eq!(back, w.h);
    assert_eq!(back.open_scan(), Some(3));
    assert_eq!(back.stars(), 2);
}

#[test]
fn a_loaded_heat_answers_the_same_as_the_one_it_came_from() {
    let mut w = World::new(12);
    w.heat_to(30);
    w.scan(1, 2, 0);
    let mut env = Envelope::new();
    w.h.save(&mut env);
    let mut loaded = Heat::load(&env).unwrap().unwrap();
    let a = w.answer(1, ScanChoice::Talk);
    let b = loaded.apply_world(&w.c, &ev(w.seq, WorldEvent::ScanAnswered { scan: 1, choice: ScanChoice::Talk }));
    assert_eq!(a.len(), b.len());
    assert_eq!(emits(&a), emits(&b));
}

#[test]
fn a_save_without_heat_loads_as_none() {
    assert!(Heat::load(&Envelope::new()).unwrap().is_none());
}

#[test]
fn the_save_section_is_named_and_versioned() {
    assert_eq!(SECTION, "heat");
    assert_eq!(SECTION_VERSION, 1);
}
