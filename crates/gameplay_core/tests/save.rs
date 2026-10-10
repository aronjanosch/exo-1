//! Save model of #128: the envelope, the kernel section and the world section. Each system's own
//! section is tested in its crate.
use std::path::Path;

use gameplay_core::save::*;
use gameplay_core::*;

const HOST: ClientId = ClientId(1);
const ANA: ClientId = ClientId(7);

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

fn ev(sender: ClientId, seq: u64, payload: WorldEvent) -> Event<WorldEvent> {
    Event::new(sender, seq, payload)
}

/// A kernel with some history: a joined player, a payout, an XP gain, a flag, an unlock.
fn played(c: &Content) -> KernelState {
    let mut k = KernelState::new(c);
    let events = [
        ev(ANA, 0, WorldEvent::PlayerJoined),
        ev(HOST, 0, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 200, player: None }),
        ev(HOST, 1, WorldEvent::TrackChanged { track: TrackId::new("freight_xp"), delta: 50, player: Some(ANA) }),
        ev(HOST, 2, WorldEvent::FlagRaised { flag: Flag::new("job_completed:first_haul") }),
        ev(ANA, 1, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") }),
    ];
    for e in &events {
        k.apply(c, e).unwrap();
    }
    k
}

#[test]
fn kernel_section_round_trips_and_continues_the_same() {
    let c = content();
    let k = played(&c);
    let mut env = Envelope::new();
    env.put(KERNEL, KERNEL_VERSION, &k);
    let back: KernelState = Envelope::from_json(&env.to_json()).unwrap().get(KERNEL, KERNEL_VERSION).unwrap().unwrap();
    assert_eq!(back, k);
    // The same events applied afterwards give the same result.
    let (mut a, mut b) = (k, back);
    let more = [
        ev(HOST, 3, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 75, player: None }),
        ev(ANA, 2, WorldEvent::FlagRaised { flag: Flag::new("x") }),
    ];
    for e in &more {
        assert_eq!(a.apply(&c, e), b.apply(&c, e));
    }
    assert_eq!(a, b);
    let price = c.unlocks[&UnlockId::new("bent_spoon_permit")].record.price;
    let start = c.tracks[&TrackId::new("wallet")].record.start;
    assert_eq!(b.progress.wallet(), start + 200 - price + 75);
}

#[test]
fn a_loaded_kernel_still_ignores_events_it_has_seen() {
    let c = content();
    let k = played(&c);
    let json = serde_json::to_string(&k).unwrap();
    let mut back: KernelState = serde_json::from_str(&json).unwrap();
    // A retried payout from before the save is not paid twice.
    let retry = ev(HOST, 0, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 200, player: None });
    assert_eq!(back.apply(&c, &retry), Ok(false));
    assert_eq!(back.progress.wallet(), k.progress.wallet());
}

#[test]
fn a_restarted_client_continues_its_numbering_above_the_saved_ids() {
    let c = content();
    let k = played(&c);
    assert_eq!(k.dedup.next_seq(HOST), 3);
    assert_eq!(k.dedup.next_seq(ANA), 2);
    assert_eq!(k.dedup.next_seq(ClientId(99)), 0, "an unknown sender starts at 0");
}

#[test]
fn world_section_round_trips() {
    let w = WorldSave {
        next_crate: 12,
        crates: vec![
            CrateSave { id: CrateId(4), commodity: CommodityId::new("fizzy_mud"), job: Some(2), condition: 0.75, place: CratePlace::Planet { planet: 0, pos: [1.5, -2.0, 6_371_000.25] } },
            CrateSave { id: CrateId(5), commodity: CommodityId::new("fizzy_mud"), job: Some(2), condition: 1.0, place: CratePlace::Ship { pos: [0.5, 0.3, -1.0] } },
        ],
        ship: Some(ShipPose { planet: 0, pos: [10.0, 20.0, 6_371_100.0], rot: [0.0, 0.0, 0.0, 1.0], vel: [0.0; 3] }),
    };
    let mut env = Envelope::new();
    env.put(WORLD, WORLD_VERSION, &w);
    let back: WorldSave = Envelope::from_json(&env.to_json()).unwrap().get(WORLD, WORLD_VERSION).unwrap().unwrap();
    assert_eq!(back, w, "f64 positions survive the text exactly");
}

#[test]
fn the_envelope_keeps_sections_apart_and_a_missing_one_is_none() {
    let mut env = Envelope::new();
    env.put("a", 1, &5_i64);
    env.put("b", 3, &"x".to_string());
    let back = Envelope::from_json(&env.to_json()).unwrap();
    assert_eq!(back.get::<i64>("a", 1).unwrap(), Some(5));
    assert_eq!(back.get::<String>("b", 3).unwrap(), Some("x".to_string()));
    assert_eq!(back.get::<i64>("c", 1).unwrap(), None);
    assert_eq!(back.names().collect::<Vec<_>>(), ["a", "b"]);
}

#[test]
fn a_wrong_version_is_refused_with_a_clear_error() {
    let mut env = Envelope::new();
    env.put("jobs", 2, &5_i64);
    let back = Envelope::from_json(&env.to_json()).unwrap();
    let e = back.get::<i64>("jobs", 1).unwrap_err();
    assert_eq!(e, SaveError::SectionVersion { section: "jobs".into(), found: 2, supported: 1 });
    assert!(e.to_string().contains("jobs") && e.to_string().contains('2') && e.to_string().contains('1'), "{e}");

    let text = env.to_json().replace("\"version\":1", "\"version\":9").replace("\"version\": 1", "\"version\": 9");
    let e = Envelope::from_json(&text).unwrap_err();
    assert!(matches!(e, SaveError::EnvelopeVersion { found: 9, supported: ENVELOPE_VERSION }), "{e:?}");
}

#[test]
fn damaged_input_is_an_error_not_a_panic() {
    assert!(matches!(Envelope::from_json("not json"), Err(SaveError::Parse(_))));
    let mut env = Envelope::new();
    env.put("a", 1, &"text".to_string());
    let e = env.get::<i64>("a", 1).unwrap_err();
    assert!(matches!(e, SaveError::Parse(_)), "{e:?}");
}
