//! Kernel rules of #122 and #125 against the fixture set in `tests/fixtures/valid`.
use std::path::Path;

use gameplay_core::*;

const SIZES: [&str; 3] = ["small", "medium", "large"];
const HOST: ClientId = ClientId(1);
const ANA: ClientId = ClientId(7);
const BO: ClientId = ClientId(8);

/// The valid set, every file with its path relative to the set's root.
fn valid_files() -> Vec<File> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/valid");
    let mut out = Vec::new();
    for dir in std::fs::read_dir(&root).unwrap() {
        let dir = dir.unwrap().path();
        for f in std::fs::read_dir(&dir).unwrap() {
            let f = f.unwrap().path();
            let rel = f.strip_prefix(&root).unwrap().to_str().unwrap().to_string();
            out.push(File::new(rel, std::fs::read_to_string(&f).unwrap()));
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn content() -> Content {
    Content::load(&valid_files(), &SIZES).unwrap()
}

/// The valid set with one file replaced or added.
fn errors_with(path: &str, text: &str) -> Vec<String> {
    let mut files: Vec<File> = valid_files().into_iter().filter(|f| f.path != path).collect();
    files.push(File::new(path, text));
    Content::load(&files, &SIZES).unwrap_err()
}

/// Exactly one error, naming the file and the field.
fn one_error(errors: &[String], path: &str, field: &str) {
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with(path) && errors[0].contains(field), "expected {path} and {field} in {errors:?}");
}

fn ev(sender: ClientId, seq: u64, payload: WorldEvent) -> Event<WorldEvent> {
    Event::new(sender, seq, payload)
}

// ---------- loader (#122) ----------

#[test]
fn valid_set_loads() {
    let c = content();
    assert_eq!(c.commodities.len(), 3);
    assert_eq!(c.locations.len(), 3);
    assert_eq!(c.tracks.len(), 4);
    assert_eq!(c.unlocks.len(), 2);
    assert_eq!(c.locations[&LocationId::new("bent_spoon")].path, "location/bent_spoon.json");
}

#[test]
fn other_folders_are_left_to_their_systems() {
    let mut files = valid_files();
    files.push(File::new("job_template/whatever.json", "{ not even json"));
    assert!(Content::load(&files, &SIZES).is_ok());
}

#[test]
fn unknown_field() {
    let e = errors_with("commodity/fizzy_mud.json", r#"{ "id": "fizzy_mud", "name": "n", "base_price": 40, "crate_size": "small", "colour": "brown" }"#);
    one_error(&e, "commodity/fizzy_mud.json", "colour");
}

#[test]
fn missing_field() {
    let e = errors_with("commodity/fizzy_mud.json", r#"{ "id": "fizzy_mud", "name": "n", "crate_size": "small" }"#);
    one_error(&e, "commodity/fizzy_mud.json", "base_price");
}

#[test]
fn duplicate_id() {
    let e = errors_with("commodity/fizzy_mud_2.json", r#"{ "id": "fizzy_mud", "name": "n", "base_price": 1, "crate_size": "small" }"#);
    one_error(&e, "commodity/fizzy_mud_2.json", "id");
}

#[test]
fn bad_id() {
    let e = errors_with("commodity/fizzy_mud.json", r#"{ "id": "Fizzy-Mud", "name": "n", "base_price": 40, "crate_size": "small" }"#);
    one_error(&e, "commodity/fizzy_mud.json", "id");
}

#[test]
fn unknown_crate_size() {
    let e = errors_with("commodity/fizzy_mud.json", r#"{ "id": "fizzy_mud", "name": "n", "base_price": 40, "crate_size": "huge" }"#);
    one_error(&e, "commodity/fizzy_mud.json", "crate_size");
}

#[test]
fn price_not_positive() {
    let e = errors_with("commodity/fizzy_mud.json", r#"{ "id": "fizzy_mud", "name": "n", "base_price": 0, "crate_size": "small" }"#);
    one_error(&e, "commodity/fizzy_mud.json", "base_price");
}

#[test]
fn unknown_track_in_condition() {
    let e = errors_with(
        "location/bent_spoon.json",
        r#"{ "id": "bent_spoon", "name": "n", "place": "bent_spoon", "pad": "main", "available": { "track_at_least": { "track": "fame", "value": 1 } } }"#,
    );
    one_error(&e, "location/bent_spoon.json", "available");
    assert!(e[0].contains("fame"), "{e:?}");
}

#[test]
fn tag_no_unlock_grants() {
    let e = errors_with(
        "location/bent_spoon.json",
        r#"{ "id": "bent_spoon", "name": "n", "place": "bent_spoon", "pad": "main", "available": { "not": { "has_tag": "moon_pass" } } }"#,
    );
    one_error(&e, "location/bent_spoon.json", "available");
    assert!(e[0].contains("moon_pass"), "{e:?}");
}

#[test]
fn unknown_condition_kind() {
    let e = errors_with(
        "location/bent_spoon.json",
        r#"{ "id": "bent_spoon", "name": "n", "place": "bent_spoon", "pad": "main", "available": { "moon_is_full": true } }"#,
    );
    one_error(&e, "location/bent_spoon.json", "moon_is_full");
}

#[test]
fn thresholds_not_ascending() {
    let e = errors_with("progress_track/freight_xp.json", r#"{ "id": "freight_xp", "name": "n", "owner": "player", "thresholds": [100, 100] }"#);
    one_error(&e, "progress_track/freight_xp.json", "thresholds");
}

#[test]
fn wallet_must_be_a_crew_track() {
    let e = errors_with("progress_track/wallet.json", r#"{ "id": "wallet", "name": "n", "owner": "player" }"#);
    one_error(&e, "progress_track/wallet.json", "owner");
}

#[test]
fn wallet_missing() {
    let files: Vec<File> = valid_files().into_iter().filter(|f| f.path != "progress_track/wallet.json").collect();
    let e = Content::load(&files, &SIZES).unwrap_err();
    // The bent spoon's `has_tag` still resolves; only the wallet is missing.
    one_error(&e, "progress_track", "wallet");
}

#[test]
fn unlock_grants_nothing() {
    let e = errors_with("unlock/bent_spoon_permit.json", r#"{ "id": "bent_spoon_permit", "name": "n", "price": 150, "grants": [] }"#);
    // The bent spoon now asks for a tag nobody grants, too.
    assert!(e.iter().any(|m| m.starts_with("unlock/bent_spoon_permit.json") && m.contains("grants")), "{e:?}");
}

#[test]
fn all_errors_are_reported_at_once() {
    let mut files = valid_files();
    files.push(File::new("commodity/a.json", r#"{ "id": "a", "name": "n", "base_price": -1, "crate_size": "huge" }"#));
    files.push(File::new("unlock/b.json", r#"{ "id": "b", "name": "n", "price": -5, "grants": ["x"] }"#));
    let e = Content::load(&files, &SIZES).unwrap_err();
    assert_eq!(e.len(), 3, "{e:?}");
}

// ---------- events (#122) ----------

#[test]
fn a_duplicate_event_id_changes_nothing() {
    let c = content();
    let mut d = Dedup::default();
    let mut p = Progress::new(&c);
    let pay = ev(HOST, 0, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 50, player: None });
    for _ in 0..3 {
        if d.first_time(pay.id) {
            p.apply(&c, &pay).unwrap();
        }
    }
    assert_eq!(p.wallet(), 250);
}

#[test]
fn dedup_handles_out_of_order_and_many_senders() {
    let mut d = Dedup::default();
    let id = |s, n| EventId { sender: s, seq: n };
    assert!(d.first_time(id(ANA, 2)));
    assert!(d.first_time(id(ANA, 0)));
    assert!(d.first_time(id(BO, 0)), "another sender's 0 is a different event");
    assert!(!d.first_time(id(ANA, 2)));
    assert_eq!(d.pending(), 1, "seq 2 waits above the mark until 1 arrives");
    assert!(d.first_time(id(ANA, 1)));
    assert_eq!(d.pending(), 0);
    assert!(!d.first_time(id(ANA, 0)) && !d.first_time(id(ANA, 1)) && !d.first_time(id(ANA, 2)));
    assert!(d.first_time(id(ANA, 3)));
}

#[test]
fn events_round_trip_as_json() {
    let e = ev(ANA, 4, WorldEvent::CrateDelivered { crate_id: CrateId(9), at: LocationId::new("drip_rock"), condition: 0.75 });
    let s = serde_json::to_string(&e).unwrap();
    assert_eq!(serde_json::from_str::<Event<WorldEvent>>(&s).unwrap(), e);
}

// ---------- conditions (#125) ----------

fn holds(c: &Content, p: &Progress, json: &str, player: Option<ClientId>) -> bool {
    let cond: Condition = serde_json::from_str(json).unwrap();
    cond.holds(c, p, player)
}

#[test]
fn track_at_least() {
    let c = content();
    let mut p = Progress::new(&c);
    assert!(holds(&c, &p, r#"{ "track_at_least": { "track": "wallet", "value": 200 } }"#, None));
    assert!(!holds(&c, &p, r#"{ "track_at_least": { "track": "wallet", "value": 201 } }"#, None));
    // Personal track: read for the asking player; without a player it does not hold.
    p.apply(&c, &ev(HOST, 0, WorldEvent::TrackChanged { track: TrackId::new("freight_xp"), delta: 120, player: Some(ANA) })).unwrap();
    let xp = r#"{ "track_at_least": { "track": "freight_xp", "value": 100 } }"#;
    assert!(holds(&c, &p, xp, Some(ANA)));
    assert!(!holds(&c, &p, xp, Some(BO)));
    assert!(!holds(&c, &p, xp, None));
    assert!(!holds(&c, &p, r#"{ "track_at_least": { "track": "fame", "value": 0 } }"#, None));
}

#[test]
fn has_tag_and_flag_set() {
    let c = content();
    let mut p = Progress::new(&c);
    assert!(!holds(&c, &p, r#"{ "has_tag": "route_bent_spoon" }"#, None));
    assert!(!holds(&c, &p, r#"{ "flag_set": "job_completed:first_haul" }"#, None));
    p.apply(&c, &ev(ANA, 0, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") })).unwrap();
    p.apply(&c, &ev(HOST, 0, WorldEvent::FlagRaised { flag: Flag::new("job_completed:first_haul") })).unwrap();
    assert!(holds(&c, &p, r#"{ "has_tag": "route_bent_spoon" }"#, None));
    assert!(holds(&c, &p, r#"{ "flag_set": "job_completed:first_haul" }"#, None));
}

#[test]
fn all_any_not() {
    let c = content();
    let p = Progress::new(&c);
    let yes = r#"{ "track_at_least": { "track": "wallet", "value": 0 } }"#;
    let no = r#"{ "has_tag": "route_bent_spoon" }"#;
    assert!(holds(&c, &p, &format!(r#"{{ "all": [{yes}, {yes}] }}"#), None));
    assert!(!holds(&c, &p, &format!(r#"{{ "all": [{yes}, {no}] }}"#), None));
    assert!(holds(&c, &p, &format!(r#"{{ "any": [{no}, {yes}] }}"#), None));
    assert!(!holds(&c, &p, &format!(r#"{{ "any": [{no}, {no}] }}"#), None));
    assert!(holds(&c, &p, &format!(r#"{{ "not": {no} }}"#), None));
    assert!(!holds(&c, &p, &format!(r#"{{ "not": {yes} }}"#), None));
    assert!(holds(&c, &p, r#"{ "all": [] }"#, None));
    assert!(!holds(&c, &p, r#"{ "any": [] }"#, None));
}

// ---------- tracks and unlocks (#125) ----------

#[test]
fn crew_tracks_start_at_their_start_value() {
    let c = content();
    let p = Progress::new(&c);
    assert_eq!(p.wallet(), 200);
    assert_eq!(p.value(&c, &TrackId::new("freight_xp"), Some(ANA)), Some(0));
    assert_eq!(p.value(&c, &TrackId::new("freight_xp"), None), None);
}

#[test]
fn personal_tracks_are_per_player_and_levels_follow_thresholds() {
    let c = content();
    let mut p = Progress::new(&c);
    let xp = TrackId::new("freight_xp");
    for (seq, who, d) in [(0, ANA, 99), (1, BO, 300), (2, ANA, 1)] {
        p.apply(&c, &ev(HOST, seq, WorldEvent::TrackChanged { track: xp.clone(), delta: d, player: Some(who) })).unwrap();
    }
    assert_eq!(p.value(&c, &xp, Some(ANA)), Some(100));
    assert_eq!(p.level(&c, &xp, Some(ANA)), 1);
    assert_eq!(p.level(&c, &xp, Some(BO)), 2);
    assert_eq!(p.level(&c, &xp, Some(ClientId(99))), 0);
}

#[test]
fn track_change_refusals() {
    let c = content();
    let mut p = Progress::new(&c);
    let before = p.clone();
    let r = p.apply(&c, &ev(HOST, 0, WorldEvent::TrackChanged { track: TrackId::new("freight_xp"), delta: 5, player: None }));
    assert_eq!(r, Err(Refusal::NoPlayer));
    let r = p.apply(&c, &ev(HOST, 1, WorldEvent::TrackChanged { track: TrackId::new("fame"), delta: 5, player: None }));
    assert_eq!(r, Err(Refusal::UnknownTrack));
    assert_eq!(p, before);
}

#[test]
fn player_joined_sets_up_personal_tracks() {
    let c = content();
    let mut p = Progress::new(&c);
    p.apply(&c, &ev(ANA, 0, WorldEvent::PlayerJoined)).unwrap();
    assert_eq!(p.value(&c, &TrackId::new("freight_xp"), Some(ANA)), Some(0));
}

#[test]
fn buying_an_unlock_takes_money_and_grants_tags() {
    let c = content();
    let mut p = Progress::new(&c);
    p.apply(&c, &ev(ANA, 0, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") })).unwrap();
    assert_eq!(p.wallet(), 50);
    assert!(p.owns(&UnlockId::new("bent_spoon_permit")));
    assert!(p.has_tag(&Tag::new("route_bent_spoon")));
}

#[test]
fn buying_with_too_little_money_is_refused() {
    let c = content();
    let mut p = Progress::new(&c);
    p.apply(&c, &ev(HOST, 0, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: -100, player: None })).unwrap();
    let before = p.clone();
    let r = p.apply(&c, &ev(ANA, 0, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") }));
    assert_eq!(r, Err(Refusal::NotEnoughMoney { price: 150, wallet: 100 }));
    assert_eq!(p, before);
}

#[test]
fn buying_twice_is_a_no_op() {
    let c = content();
    let mut p = Progress::new(&c);
    p.apply(&c, &ev(HOST, 0, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 1000, player: None })).unwrap();
    p.apply(&c, &ev(ANA, 0, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") })).unwrap();
    let before = p.clone();
    let r = p.apply(&c, &ev(BO, 0, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") }));
    assert_eq!(r, Err(Refusal::AlreadyOwned));
    assert_eq!(p, before);
}

#[test]
fn an_unlock_condition_is_read_for_the_buyer() {
    let c = content();
    let mut p = Progress::new(&c);
    let license = UnlockId::new("jelly_license");
    let buy = |who| ev(who, 0, WorldEvent::UnlockBought { unlock: license.clone() });
    assert_eq!(p.apply(&c, &buy(ANA)), Err(Refusal::ConditionNotMet));
    p.apply(&c, &ev(HOST, 0, WorldEvent::FlagRaised { flag: Flag::new("job_completed:first_haul") })).unwrap();
    p.apply(&c, &ev(HOST, 1, WorldEvent::TrackChanged { track: TrackId::new("freight_xp"), delta: 150, player: Some(ANA) })).unwrap();
    assert_eq!(p.apply(&c, &buy(BO)), Err(Refusal::ConditionNotMet), "Bo has no freight XP");
    assert_eq!(p.apply(&c, &buy(ANA)), Ok(()));
    assert!(p.has_tag(&Tag::new("jelly_jobs")));
}

#[test]
fn unknown_unlock_is_refused() {
    let c = content();
    let mut p = Progress::new(&c);
    assert_eq!(p.apply(&c, &ev(ANA, 0, WorldEvent::UnlockBought { unlock: UnlockId::new("moon") })), Err(Refusal::UnknownUnlock));
}

#[test]
fn an_unlock_tag_makes_a_location_available() {
    let c = content();
    let mut p = Progress::new(&c);
    let spoon = LocationId::new("bent_spoon");
    assert!(p.location_available(&c, &LocationId::new("drip_rock")), "no condition: open from the start");
    assert!(!p.location_available(&c, &spoon));
    p.apply(&c, &ev(ANA, 0, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") })).unwrap();
    assert!(p.location_available(&c, &spoon));
}

#[test]
fn progress_round_trips_as_json() {
    let c = content();
    let mut p = Progress::new(&c);
    p.apply(&c, &ev(ANA, 0, WorldEvent::UnlockBought { unlock: UnlockId::new("bent_spoon_permit") })).unwrap();
    p.apply(&c, &ev(HOST, 0, WorldEvent::TrackChanged { track: TrackId::new("freight_xp"), delta: 42, player: Some(BO) })).unwrap();
    let s = serde_json::to_string(&p).unwrap();
    assert_eq!(serde_json::from_str::<Progress>(&s).unwrap(), p);
}

// ---------- track floor (#167) ----------

#[test]
fn a_track_with_a_floor_never_drops_below_it_and_recovers() {
    let c = content();
    let mut p = Progress::new(&c);
    let t = TrackId::new("standing_courier");
    let change = |p: &mut Progress, seq, d| p.apply(&c, &ev(HOST, seq, WorldEvent::TrackChanged { track: TrackId::new("standing_courier"), delta: d, player: None })).unwrap();
    change(&mut p, 0, 20);
    change(&mut p, 1, -50);
    assert_eq!(p.value(&c, &t, None), Some(0), "a failure lowers standing, not below the floor");
    change(&mut p, 2, 15);
    assert_eq!(p.value(&c, &t, None), Some(15), "work brings it back from the floor");
    // The wallet has no floor.
    p.apply(&c, &ev(HOST, 3, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: -500, player: None })).unwrap();
    assert_eq!(p.wallet(), -300);
}

#[test]
fn a_floor_above_the_start_is_a_content_error() {
    let e = errors_with("progress_track/standing_courier.json", r#"{ "id": "standing_courier", "name": "x", "owner": "crew", "min": 5, "start": 0 }"#);
    one_error(&e, "progress_track/standing_courier.json", "min");
}

// ---------- order events (#168) ----------

#[test]
fn order_events_are_plain_domain_events_that_progress_ignores_and_that_round_trip() {
    let c = content();
    let mut p = Progress::new(&c);
    let placed = ev(HOST, 0, WorldEvent::OrderPlaced { order: OrderId(3), by: "old_nib".into(), from: LocationId::new("wholesale_yard"), to: LocationId::new("drip_rock"), commodity: CommodityId::new("fizzy_mud"), amount: 2, reward: 90, deadline_s: Some(300.0) });
    let settled = ev(HOST, 1, WorldEvent::OrderSettled { order: OrderId(3), delivered: 2, asked: 2, condition: 0.9, in_time: true });
    let before = p.clone();
    p.apply(&c, &placed).unwrap();
    p.apply(&c, &settled).unwrap();
    assert_eq!(p, before, "progress does not change by orders, systems read them");
    for e in [placed, settled] {
        let back: Event<WorldEvent> = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }
}

#[test]
fn flight_events_are_plain_domain_events() {
    let c = content();
    let mut p = Progress::new(&c);
    let before = p.clone();
    let evs = [
        ev(ANA, 0, WorldEvent::TookOff),
        ev(ANA, 1, WorldEvent::PadReached { at: LocationId::new("bent_spoon") }),
        ev(ANA, 2, WorldEvent::Landed { at: Some(LocationId::new("bent_spoon")), speed: 2.5 }),
        ev(ANA, 3, WorldEvent::Landed { at: None, speed: 11.0 }),
    ];
    for e in &evs {
        p.apply(&c, e).unwrap();
        let back: Event<WorldEvent> = serde_json::from_str(&serde_json::to_string(e).unwrap()).unwrap();
        assert_eq!(&back, e);
    }
    assert_eq!(p, before);
}
