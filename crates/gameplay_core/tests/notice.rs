use std::path::Path;

use gameplay_core::notice::*;
use gameplay_core::text::*;
use gameplay_core::*;

const HOST: ClientId = ClientId(1);
const ANA: ClientId = ClientId(7);

// ---------- queue ----------

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
    assert!(p.apply_with_notices(&c, &ev(4, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 500, player: None })).unwrap().is_empty());
    assert!(p.apply_with_notices(&c, &Event::new(ANA, 1, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") })).is_err());
}
