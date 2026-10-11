//! The family ladder of #230: the next goal walks the ranks, a tribute is refused with the right
//! notice, a rank-up grants its unlocks and charges the wallet once, saves round-trip.
mod common;

use common::*;
use gameplay_core::goals::{Empire, GoalView, Ladders, PAY_TRIBUTE, SECTION};
use gameplay_core::notice::{Arg, NoticeKind};
use gameplay_core::save::Envelope;
use gameplay_core::{File, Flag, ProductSpec, TextKey, WorldEvent};

use std::collections::BTreeSet;

fn brewed() -> WorldEvent {
    WorldEvent::Brewed { station: "kettle".into(), product: ProductSpec { base: gameplay_core::CommodityId::new("fizzy_mud"), effects: BTreeSet::new(), quality: 0.5 }, crates: 1, discovered: true }
}

fn deal() -> WorldEvent {
    WorldEvent::ProductOrderSettled { order: gameplay_core::OrderId(1), paid: 100, matched: 1.0 }
}

fn goal_view(w: &World) -> GoalView {
    w.empire.next_goal(&w.ladders, &w.progress, &w.k, "gribbles").expect("the ladder has a next rank")
}

#[test]
fn next_goal_walks_the_ranks() {
    let mut w = World::fixture();
    let g = goal_view(&w);
    assert_eq!(g.rank_name, TextKey::new("rank.one"));
    assert_eq!(g.goal_text, TextKey::new("goal.jobs"));
    assert_eq!((g.goals_done, g.goals_total, g.tribute, g.ready_to_pay), (0, 1, 0, false));

    w.send(job_done("first_haul"));
    let g = goal_view(&w);
    assert_eq!((g.goals_done, g.goal_text.clone()), (0, TextKey::new("goal.jobs")), "one job of two");

    w.send(job_done("first_haul"));
    let g = goal_view(&w);
    assert!(g.ready_to_pay);
    assert_eq!(g.goal_text, TextKey::new(PAY_TRIBUTE), "every goal holds: the next line is the tribute");

    // Tribute 0: the rank comes, its unlock is granted for free, and the flag is raised.
    w.send(pay("gribbles", 0));
    assert_eq!(w.rank("gribbles"), 1);
    assert!(w.progress.owns(&gameplay_core::UnlockId::new("lab_key")));
    assert!(w.progress.has_tag(&gameplay_core::Tag::new("lab_access")));
    assert!(w.progress.has_flag(&Flag::new("rank:gribbles:0")));
    let g = goal_view(&w);
    assert_eq!((g.rank_name.clone(), g.goals_done, g.goals_total, g.tribute), (TextKey::new("rank.two"), 0, 2, 200));
    assert_eq!(g.goal_text, TextKey::new("goal.recipe"));

    w.send(brewed());
    let g = goal_view(&w);
    assert_eq!((g.goals_done, g.goal_text.clone()), (1, TextKey::new("goal.deals")), "the recipe holds, the deals do not");

    w.send(deal());
    w.send(deal());
    assert!(goal_view(&w).ready_to_pay);
}

#[test]
fn a_missing_goal_is_refused_and_named() {
    let mut w = World::fixture();
    w.send(pay("gribbles", 0));
    assert_eq!(w.rank("gribbles"), 0, "the first goal is missing");
    let n = notice(&w, "notice.empire.goal_missing").expect("a refusal notice");
    assert_eq!(n.kind, NoticeKind::Warning);
    assert_eq!(n.args, vec![("goal".to_string(), Arg::Key(TextKey::new("goal.jobs")))]);
    assert!(w.applied.iter().all(|e| !matches!(e, WorldEvent::RankReached { .. })), "no rank without the goal");
}

#[test]
fn a_short_tribute_is_refused_and_names_the_price() {
    let mut w = World::fixture();
    reach_rank_two(&mut w);
    w.send(track("wallet", 1000));
    w.send(pay("gribbles", 150));
    assert_eq!(w.rank("gribbles"), 1, "150 is less than the 200 tribute");
    let n = notice(&w, "notice.empire.tribute_short").expect("a refusal notice");
    assert_eq!(n.args, vec![("amount".to_string(), Arg::Number(200))]);
    assert_eq!(w.wallet(), 1100, "nothing charged");
}

#[test]
fn a_tribute_the_wallet_cannot_pay_is_refused() {
    let mut w = World::fixture();
    reach_rank_two(&mut w);
    w.send(pay("gribbles", 200));
    assert_eq!(w.rank("gribbles"), 1, "the wallet holds 100, the tribute is 200");
    assert!(notice(&w, "notice.empire.cannot_afford").is_some());
    assert_eq!(w.wallet(), 100);
}

#[test]
fn a_rank_up_grants_its_unlocks_and_charges_the_wallet_once() {
    let mut w = World::fixture();
    reach_rank_two(&mut w);
    w.send(track("wallet", 200));
    let before = w.applied.len();
    w.send(pay("gribbles", 200));

    assert_eq!(w.wallet(), 100, "300 - 200, once");
    assert!(w.progress.owns(&gameplay_core::UnlockId::new("far_buyers")));
    assert!(w.progress.has_tag(&gameplay_core::Tag::new("buyers_far")));
    assert_eq!(w.rank("gribbles"), 2);
    assert!(w.progress.has_flag(&Flag::new("rank:gribbles:1")));
    assert_eq!(
        w.applied[before..].to_vec(),
        vec![
            pay("gribbles", 200),
            track("wallet", -200),
            WorldEvent::UnlockBought { unlock: gameplay_core::UnlockId::new("far_buyers") },
            WorldEvent::RankReached { ladder: "gribbles".into(), rank: 1 },
            WorldEvent::FlagRaised { flag: Flag::new("rank:gribbles:1") },
        ],
        "the events of the rank-up, in order"
    );
    let n = notice(&w, "notice.empire.rank_up").expect("the rank-up notice");
    assert_eq!(n.kind, NoticeKind::Rank);
    assert_eq!(n.args, vec![("ladder".to_string(), Arg::Key(TextKey::new("ladder.gribbles.name"))), ("rank".to_string(), Arg::Key(TextKey::new("rank.two")))]);
}

#[test]
fn the_top_rank_has_no_next_goal_and_says_so() {
    let mut w = World::fixture();
    reach_rank_two(&mut w);
    w.send(track("wallet", 200));
    w.send(pay("gribbles", 200));
    assert!(w.empire.next_goal(&w.ladders, &w.progress, &w.k, "gribbles").is_none());
    let sent = w.applied.len();
    w.send(pay("gribbles", 200));
    assert_eq!(w.applied.len(), sent + 1, "only the request, nothing emitted");
    assert!(notice(&w, "notice.empire.top").is_some());
}

#[test]
fn an_unknown_ladder_changes_nothing() {
    let mut w = World::fixture();
    w.send(pay("no_such_family", 0));
    assert_eq!(w.applied.len(), 1);
    assert!(w.notices.is_empty());
    assert!(w.empire.next_goal(&w.ladders, &w.progress, &w.k, "no_such_family").is_none());
}

#[test]
fn rank_reached_is_idempotent_and_never_goes_back() {
    let mut w = World::fixture();
    reach_rank_two(&mut w);
    w.send(track("wallet", 200));
    w.send(pay("gribbles", 200));
    w.send(WorldEvent::RankReached { ladder: "gribbles".into(), rank: 0 });
    w.send(WorldEvent::RankReached { ladder: "gribbles".into(), rank: 1 });
    assert_eq!(w.rank("gribbles"), 2, "an old rank does not undo a newer one");
}

#[test]
fn the_empire_saves_and_loads() {
    let mut w = World::fixture();
    reach_rank_two(&mut w);
    let mut env = Envelope::new();
    w.empire.save(&mut env);
    assert_eq!(Empire::load(&env).unwrap(), Some(w.empire.clone()));
    assert_eq!(Empire::load(&Envelope::new()).unwrap(), None, "a save without the section");

    let mut newer = Envelope::new();
    newer.put(SECTION, 2, &w.empire);
    assert!(Empire::load(&newer).is_err(), "a version this build does not read is an error");
}

#[test]
fn bad_ladders_are_refused_with_file_and_field() {
    let k = kernel();
    let load = |text: &str| -> Vec<String> {
        let mut f: Vec<File> = fixture_files(&[]).into_iter().filter(|f| f.path != "ladder/gribbles.json").collect();
        f.push(File::new("ladder/gribbles.json", text));
        Ladders::load(&f, &k).unwrap_err()
    };
    let rank = |extra: &str| format!(r#"{{"id":"gribbles","name":"ladder.gribbles.name","ranks":[{{"name":"rank.one","tribute":0{extra}}}]}}"#);
    let has = |e: &[String], field: &str, what: &str| e.iter().any(|x| x.contains("ladder/gribbles.json") && x.contains(field) && x.contains(what));

    let e = load(&rank(r#","unlocks":["nope"]"#));
    assert!(has(&e, "unlocks", "nope"), "{e:?}");
    let e = load(&rank(r#","unlocks":["hidden_hold"]"#));
    assert!(has(&e, "unlocks", "costs 400"), "a rank unlock is free: {e:?}");
    let e = load(&rank(r#","requires":[{"text":"goal.x","condition":{"track_at_least":{"track":"no_such","value":1}}}]"#));
    assert!(has(&e, "requires", "no_such"), "{e:?}");
    let e = load(r#"{"id":"gribbles","name":"ladder.gribbles.name","ranks":[]}"#);
    assert!(has(&e, "ranks", "empty"), "{e:?}");
    let e = load(r#"{"id":"gribbles","name":"ladder.gribbles.name","ranks":[{"name":"rank.one","tribute":-5}]}"#);
    assert!(has(&e, "tribute", "negative"), "{e:?}");
}

#[test]
fn texts_name_every_key_the_ladder_shows() {
    let k = kernel();
    let ladders = Ladders::load(&fixture_files(&[]), &k).unwrap();
    let table = gameplay_core::text::TextTable::from_json("en", r#"{"ladder.gribbles.name":"x"}"#).unwrap();
    let e = ladders.check_texts(&table);
    assert!(e.iter().any(|x| x.contains("rank.one")), "{e:?}");
    assert!(e.iter().any(|x| x.contains(PAY_TRIBUTE)), "{e:?}");
}

/// Two jobs, the tribute 0, one recipe and two deals: the crew stands at rank two, owing 200.
fn reach_rank_two(w: &mut World) {
    w.send(job_done("first_haul"));
    w.send(job_done("first_haul"));
    w.send(pay("gribbles", 0));
    w.send(brewed());
    w.send(deal());
    w.send(deal());
    assert_eq!(w.rank("gribbles"), 1);
}
