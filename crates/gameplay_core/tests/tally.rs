//! Tallies of #230: each kind of match raises `TrackChanged{track, +1}` on its crew track; the
//! loader rejects a bad kind, a player track and an unknown track, naming the file and the field.
mod common;

use std::collections::BTreeSet;

use common::*;
use gameplay_core::tally::Tallies;
use gameplay_core::{CommodityId, Event, Flag, LocationId, OrderId, ProductSpec, ScanChoice, TownId, TrackId, WorldEvent};

fn ev(p: WorldEvent) -> Event<WorldEvent> {
    Event::new(HOST, 1, p)
}

fn tallies() -> Tallies {
    let k = kernel();
    Tallies::load(&fixture_files(&[]), &k).unwrap()
}

fn one(t: &str) -> WorldEvent {
    track(t, 1)
}

fn product() -> ProductSpec {
    ProductSpec { base: CommodityId::new("fizzy_mud"), effects: BTreeSet::new(), quality: 0.5 }
}

#[test]
fn product_order_settled_counts_a_deal() {
    let t = tallies();
    let out = t.apply_world(&ev(WorldEvent::ProductOrderSettled { order: OrderId(1), paid: 120, matched: 0.8 }));
    assert_eq!(out, vec![one("deals_done")]);
}

#[test]
fn recipe_counts_only_when_it_is_new() {
    let t = tallies();
    let new = WorldEvent::Brewed { station: "kettle".into(), product: product(), crates: 2, discovered: true };
    let old = WorldEvent::Brewed { station: "kettle".into(), product: product(), crates: 2, discovered: false };
    assert_eq!(t.apply_world(&ev(new)), vec![one("recipes_found")]);
    assert!(t.apply_world(&ev(old)).is_empty());
}

#[test]
fn perfect_landing_is_grade_three_on_time() {
    let t = tallies();
    let land = |grade, on_time| WorldEvent::LandingGraded { town: TownId::new("tiny_town"), at: LocationId::new("bent_spoon"), grade, on_time };
    assert_eq!(t.apply_world(&ev(land(3, true))), vec![one("perfect_landings")]);
    assert!(t.apply_world(&ev(land(3, false))).is_empty(), "late is not perfect");
    assert!(t.apply_world(&ev(land(2, true))).is_empty(), "grade 2 is not perfect");
}

#[test]
fn clean_scan_has_no_fine_and_no_confiscation() {
    let t = tallies();
    let scan = |fine, confiscate| WorldEvent::ScanResolved { scan: 1, choice: ScanChoice::Comply, confiscate, fine, heat: 0.0 };
    assert_eq!(t.apply_world(&ev(scan(0, false))), vec![one("clean_scans")]);
    assert!(t.apply_world(&ev(scan(0, true))).is_empty(), "confiscated is not clean");
    assert!(t.apply_world(&ev(scan(50, false))).is_empty(), "a fine is not clean");
}

#[test]
fn flag_prefix_counts_a_finished_job_of_any_template() {
    let t = tallies();
    let job = |s: &str| WorldEvent::FlagRaised { flag: Flag::new(s) };
    assert_eq!(t.apply_world(&ev(job("job_completed:first_haul"))), vec![one("jobs_done")]);
    assert_eq!(t.apply_world(&ev(job("job_completed:courier_lint_trap"))), vec![one("jobs_done")]);
    assert!(t.apply_world(&ev(job("rank:gribbles:0"))).is_empty(), "another flag does not match");
}

#[test]
fn other_events_raise_nothing() {
    let t = tallies();
    assert!(t.apply_world(&ev(WorldEvent::TimePassed { dt: 1.0 })).is_empty());
    assert_eq!(t.tallies.len(), 5, "one per kind in the fixture");
}

#[test]
fn bad_tallies_are_refused_with_file_and_field() {
    let k = kernel();
    let load = |extra: &[(&str, &str)]| -> Vec<String> {
        let mut f = fixture_files(&[]);
        f.extend(fixture(extra));
        Tallies::load(&f, &k).unwrap_err()
    };
    let e = load(&[("tally/bad.json", r#"{"id":"bad","on":"sold_a_thing","track":"deals_done"}"#)]);
    assert!(e.iter().any(|x| x.contains("tally/bad.json") && x.contains("on") && x.contains("sold_a_thing")), "{e:?}");
    let e = load(&[("tally/empty.json", r#"{"id":"empty","on":"flag:","track":"jobs_done"}"#)]);
    assert!(e.iter().any(|x| x.contains("tally/empty.json") && x.contains("on")), "{e:?}");
    let e = load(&[("tally/nope.json", r#"{"id":"nope","on":"recipe_discovered","track":"no_such"}"#)]);
    assert!(e.iter().any(|x| x.contains("tally/nope.json") && x.contains("track") && x.contains("no_such")), "{e:?}");
    let e = load(&[("tally/xp.json", r#"{"id":"xp","on":"recipe_discovered","track":"freight_xp"}"#)]);
    assert!(e.iter().any(|x| x.contains("tally/xp.json") && x.contains("track") && x.contains("crew")), "{e:?}");
}

#[test]
fn shipped_tallies_count_the_crew_tracks() {
    let k = shipped_kernel();
    let t = shipped_tallies(&k);
    let track_of = |id: &str| t.tallies.get(id).unwrap_or_else(|| panic!("no tally {id}")).record.track.clone();
    assert_eq!(track_of("deal"), TrackId::new("deals_done"));
    assert_eq!(track_of("recipe"), TrackId::new("recipes_found"));
    assert_eq!(track_of("landing"), TrackId::new("perfect_landings"));
    assert_eq!(track_of("scan"), TrackId::new("clean_scans"));
    assert_eq!(track_of("job"), TrackId::new("jobs_done"));
    let deal = t.apply_world(&ev(WorldEvent::ProductOrderSettled { order: OrderId(9), paid: 1, matched: 1.0 }));
    assert_eq!(deal, vec![one("deals_done")]);
}
