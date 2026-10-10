//! Feedback beats of #165: notices, the pacing queue, text pools without immediate repeats, and
//! the notices the kernel itself raises (unlock, rank).
use std::path::Path;

use gameplay_core::notice::*;
use gameplay_core::text::*;
use gameplay_core::*;

const HOST: ClientId = ClientId(1);
const ANA: ClientId = ClientId(7);

fn n(kind: NoticeKind, key: &str) -> Notice {
    Notice::new(kind, key)
}

/// Runs the queue for `secs` in 50 ms steps; everything released, in order.
fn run(q: &mut NoticeQueue, secs: f64) -> Vec<Shown> {
    let mut out = Vec::new();
    for _ in 0..(secs / 0.05).round() as usize {
        out.extend(q.tick(0.05));
    }
    out
}

fn keys(v: &[Shown]) -> Vec<&str> {
    v.iter().map(|s| s.notice.key.as_str()).collect()
}

// ---------- queue ----------

#[test]
fn notices_come_out_in_order_with_a_gap() {
    let mut q = NoticeQueue::default();
    q.push(n(NoticeKind::Updated, "a"));
    q.push(n(NoticeKind::Updated, "b"));
    q.push(n(NoticeKind::Reward, "c"));
    // The first is out at once, the next ones one gap apart.
    let first = q.tick(0.05);
    assert_eq!(keys(&first), ["a"]);
    assert!(q.tick(0.05).is_empty(), "a gap between notices");
    let gap = q.pacing().gap_s;
    let rest = run(&mut q, gap + 1.0);
    assert_eq!(keys(&rest), ["b", "c"]);
    assert!(q.is_empty());
}

#[test]
fn never_two_banners_at_once() {
    let mut q = NoticeQueue::default();
    q.push(n(NoticeKind::Completed, "done"));
    q.push(n(NoticeKind::Failed, "other"));
    q.push(n(NoticeKind::Updated, "toast_after"));
    let mut shown = q.tick(0.05);
    assert_eq!(keys(&shown), ["done"]);
    let banner_s = q.pacing().banner_s;
    // Until the first banner is over, the second banner waits, and so does the toast behind it (order).
    shown = run(&mut q, banner_s - 0.5);
    assert!(shown.is_empty(), "{:?}", keys(&shown));
    shown = run(&mut q, 2.0);
    assert_eq!(keys(&shown)[0], "other");
    // At no moment did two banners overlap: banner starts are at least banner_s apart.
    let mut q = NoticeQueue::default();
    for i in 0..4 {
        q.push(n(NoticeKind::Accepted, &format!("b{i}")));
    }
    let mut starts = Vec::new();
    let mut t = 0.0;
    while !q.is_empty() && t < 60.0 {
        for s in q.tick(0.05) {
            assert!(s.banner);
            starts.push(t);
        }
        t += 0.05;
    }
    assert_eq!(starts.len(), 4);
    assert!(starts.windows(2).all(|w| w[1] - w[0] >= banner_s - 0.06), "{starts:?}");
}

#[test]
fn a_rank_is_held_while_objectives_change_and_released_when_calm() {
    let mut q = NoticeQueue::default();
    let calm = q.pacing().calm_s;
    q.push(n(NoticeKind::Rank, "rank_up"));
    q.push(n(NoticeKind::Updated, "step"));
    // An objective changes every second: the rank never comes, the toast does.
    let mut all = Vec::new();
    for _ in 0..6 {
        q.objective_changed();
        all.extend(run(&mut q, 1.0));
    }
    assert_eq!(keys(&all), ["step"], "the rank waits while things happen");
    // Quiet for the calm time: now it is released.
    let after = run(&mut q, calm + 1.0);
    assert_eq!(keys(&after), ["rank_up"]);
    assert!(after[0].banner);
}

#[test]
fn pushing_an_objective_notice_counts_as_a_change() {
    let mut q = NoticeQueue::default();
    q.push(n(NoticeKind::Rank, "rank_up"));
    let calm = q.pacing().calm_s;
    run(&mut q, calm - 0.5);
    // A delivery notice resets the calm timer.
    q.push(n(NoticeKind::Updated, "delivered"));
    let shown = run(&mut q, 1.0);
    assert_eq!(keys(&shown), ["delivered"]);
    assert!(run(&mut q, calm - 1.5).is_empty(), "calm time starts over");
    assert_eq!(keys(&run(&mut q, 3.0)), ["rank_up"]);
}

#[test]
fn skipping_the_ritual_shows_everything_in_order_but_fast() {
    let mut slow = NoticeQueue::default();
    let mut fast = NoticeQueue::default();
    for q in [&mut slow, &mut fast] {
        q.push(n(NoticeKind::Completed, "done"));
        for i in 0..4 {
            q.push(n(NoticeKind::Reward, &format!("r{i}")));
        }
    }
    fast.skip();
    let time_to_empty = |q: &mut NoticeQueue| {
        let mut t = 0.0;
        let mut seen = Vec::new();
        while !q.is_empty() {
            seen.extend(q.tick(0.05).into_iter().map(|s| s.notice.key));
            t += 0.05;
            assert!(t < 120.0);
        }
        (t, seen)
    };
    let (ts, seen_slow) = time_to_empty(&mut slow);
    let (tf, seen_fast) = time_to_empty(&mut fast);
    assert_eq!(seen_slow, seen_fast, "same notices, same order");
    assert!(tf < ts / 3.0, "skipped {tf:.1} s vs {ts:.1} s");
}

#[test]
fn every_kind_has_a_sound_and_a_weight() {
    for k in NoticeKind::ALL {
        assert!(!k.sound().is_empty());
        let _ = Notice::new(k, "x").weight;
    }
    assert_eq!(Notice::new(NoticeKind::Rank, "x").weight, Weight::Calm);
    assert_eq!(Notice::new(NoticeKind::Unlock, "x").weight, Weight::Calm);
    assert_eq!(Notice::new(NoticeKind::Completed, "x").weight, Weight::Banner);
    assert_eq!(Notice::new(NoticeKind::Reward, "x").weight, Weight::Toast);
}

// ---------- text pools ----------

const TABLE: &str = r#"{
  "_comment": "x",
  "plain": "just one line",
  "greet": ["hello", "howdy", "ahoy"],
  "reward": "{what} +{n}",
  "job.t": "First haul"
}"#;

#[test]
fn a_table_holds_lines_and_pools() {
    let t = TextTable::from_json("text/en.json", TABLE).unwrap();
    assert_eq!(t.lines("plain").unwrap(), ["just one line"]);
    assert_eq!(t.lines("greet").unwrap().len(), 3);
    assert!(t.lines("missing").is_none());
    assert!(TextTable::from_json("text/en.json", r#"{ "a": [] }"#).is_err(), "an empty pool is an error");
    assert!(TextTable::from_json("text/en.json", r#"{ "a": 3 }"#).is_err());
}

#[test]
fn a_pool_never_repeats_a_line_twice_in_a_row() {
    let t = TextTable::from_json("t", TABLE).unwrap();
    for seed in 0..50 {
        let mut p = Picker::new(seed);
        let mut last = String::new();
        for _ in 0..200 {
            let s = p.pick(&t, "greet");
            assert_ne!(s, last, "seed {seed}");
            last = s;
        }
    }
    // A pool of one has no choice.
    let mut p = Picker::new(1);
    assert_eq!(p.pick(&t, "plain"), p.pick(&t, "plain"));
}

#[test]
fn picking_is_deterministic_for_a_seed_and_uses_the_whole_pool() {
    let t = TextTable::from_json("t", TABLE).unwrap();
    let run = |seed| {
        let mut p = Picker::new(seed);
        (0..30).map(|_| p.pick(&t, "greet")).collect::<Vec<_>>()
    };
    assert_eq!(run(5), run(5));
    assert_ne!(run(5), run(6));
    let seen: std::collections::BTreeSet<_> = run(5).into_iter().collect();
    assert_eq!(seen.len(), 3);
}

#[test]
fn arguments_are_filled_in_and_a_missing_key_shows_the_key() {
    let t = TextTable::from_json("t", TABLE).unwrap();
    let mut p = Picker::new(1);
    let args = [("what".to_string(), Arg::Text("Base".into())), ("n".to_string(), Arg::Number(300))];
    assert_eq!(p.render(&t, &Notice::new(NoticeKind::Reward, "reward").args(args.to_vec())), "Base +300");
    let with_key = [("what".to_string(), Arg::Key(TextKey::new("job.t"))), ("n".to_string(), Arg::Number(-5))];
    assert_eq!(p.render(&t, &Notice::new(NoticeKind::Reward, "reward").args(with_key.to_vec())), "First haul +-5");
    assert_eq!(p.render(&t, &Notice::new(NoticeKind::Updated, "no.such.key")), "no.such.key");
}

// ---------- kernel notices ----------

fn content() -> Content {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/valid");
    let mut files = Vec::new();
    for dir in std::fs::read_dir(&root).unwrap() {
        for f in std::fs::read_dir(dir.unwrap().path()).unwrap() {
            let f = f.unwrap().path();
            files.push(File::new(f.strip_prefix(&root).unwrap().to_str().unwrap(), std::fs::read_to_string(&f).unwrap()));
        }
    }
    Content::load(&files, &["small", "medium", "large"]).unwrap()
}

#[test]
fn buying_an_unlock_and_reaching_a_rank_raise_notices() {
    let c = content();
    let mut p = Progress::new(&c);
    let ev = |s, e| Event::new(HOST, s, e);
    p.apply(&c, &ev(0, WorldEvent::PlayerJoined)).unwrap();
    let bought = p.apply_with_notices(&c, &Event::new(ANA, 0, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") })).unwrap();
    assert_eq!(bought.len(), 1);
    assert_eq!(bought[0].kind, NoticeKind::Unlock);
    assert!(bought[0].args.iter().any(|(k, a)| k == "name" && *a == Arg::Key(TextKey::new("unlock.bent_spoon_permit.name"))));

    // freight_xp has thresholds [100, 300, 700] in the fixture: 50 is no rank, 60 more is rank 1, then 20 is none.
    let xp = |s, d| ev(s, WorldEvent::TrackChanged { track: TrackId::new("freight_xp"), delta: d, player: Some(HOST) });
    assert!(p.apply_with_notices(&c, &xp(1, 50)).unwrap().is_empty());
    let up = p.apply_with_notices(&c, &xp(2, 60)).unwrap();
    assert_eq!(up.len(), 1);
    assert_eq!(up[0].kind, NoticeKind::Rank);
    assert!(up[0].args.iter().any(|(k, a)| k == "level" && *a == Arg::Number(1)));
    assert!(p.apply_with_notices(&c, &xp(3, 20)).unwrap().is_empty());
    // Money moves are not ranks (the wallet has no thresholds).
    assert!(p.apply_with_notices(&c, &ev(4, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 500, player: None })).unwrap().is_empty());
    // A refused event raises nothing.
    assert!(p.apply_with_notices(&c, &Event::new(ANA, 1, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") })).is_err());
}
