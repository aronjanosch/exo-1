//! The shipped content of #230 (`content/gameplay/`): it loads and checks, its texts exist, and
//! the crew can walk the whole shipped ladder and buy from the shipped shops.
mod common;

use common::*;
use gameplay_core::goals::{NOTICE_KEYS, PAY_TRIBUTE};
use gameplay_core::text::TextTable;
use gameplay_core::{CommodityId, LocationId, OrderId, ProductSpec, ScanChoice, TextKey, TownId, UnlockId, WorldEvent};
use std::collections::BTreeSet;

fn table() -> TextTable {
    let text = std::fs::read_to_string(shipped_root().join("text/en.json")).unwrap();
    TextTable::from_json("content/gameplay/text/en.json", &text).unwrap()
}

#[test]
fn shipped_ladder_shops_and_tallies_load() {
    let k = shipped_kernel();
    let ladders = shipped_ladders(&k);
    let tallies = shipped_tallies(&k);
    let shops = shipped_shops(&k);
    assert!(ladders.ladders.contains_key("gribbles"));
    assert_eq!(ladders.ladders["gribbles"].record.ranks.len(), 4);
    assert_eq!(tallies.tallies.len(), 5);
    assert_eq!(shops.shops.len(), 3);
}

#[test]
fn shipped_texts_exist() {
    let k = shipped_kernel();
    let t = table();
    assert_eq!(shipped_ladders(&k).check_texts(&t), Vec::<String>::new());
    assert_eq!(shipped_shops(&k).check_texts(&t), Vec::<String>::new());
    assert!(t.has(PAY_TRIBUTE));
    for key in NOTICE_KEYS {
        assert!(t.has(key), "{key}");
    }
    for u in ["lab_key", "far_buyers", "freeport_contact", "family_seat", "hidden_hold", "scan_jammer", "cargo_plates", "second_kettle"] {
        assert!(t.has(&format!("unlock.{u}.name")), "unlock.{u}.name");
    }
}

fn brewed() -> WorldEvent {
    WorldEvent::Brewed { station: "kettle".into(), product: ProductSpec { base: CommodityId::new("fizzy_mud"), effects: BTreeSet::new(), quality: 0.6 }, crates: 1, discovered: true }
}

fn deal() -> WorldEvent {
    WorldEvent::ProductOrderSettled { order: OrderId(1), paid: 100, matched: 1.0 }
}

fn clean_scan() -> WorldEvent {
    WorldEvent::ScanResolved { scan: 1, choice: ScanChoice::Comply, confiscate: false, fine: 0, heat: 0.0 }
}

fn perfect() -> WorldEvent {
    WorldEvent::LandingGraded { town: TownId::new("tiny_town"), at: LocationId::new("bent_spoon"), grade: 3, on_time: true }
}

fn repeat(w: &mut World, n: usize, e: impl Fn() -> WorldEvent) {
    for _ in 0..n {
        w.send(e());
    }
}

#[test]
fn the_crew_walks_the_shipped_ladder_to_the_top() {
    let k = shipped_kernel();
    let mut w = World::new(k.clone(), shipped_ladders(&k), shipped_tallies(&k));
    let next = |w: &World| w.empire.next_goal(&w.ladders, &w.progress, &w.k, "gribbles").unwrap();

    // Errand Kid: 2 jobs, no tribute.
    repeat(&mut w, 2, || job_done("first_haul"));
    assert!(next(&w).ready_to_pay);
    w.send(pay("gribbles", 0));
    assert_eq!(w.rank("gribbles"), 1);
    assert!(w.progress.owns(&UnlockId::new("lab_key")));

    // Runner: 1 recipe, 2 deals, 200 tribute. The wallet starts at 100.
    w.send(brewed());
    repeat(&mut w, 2, deal);
    w.send(track("wallet", 1000));
    w.send(pay("gribbles", 200));
    assert_eq!(w.rank("gribbles"), 2);
    assert!(w.progress.owns(&UnlockId::new("far_buyers")));
    assert_eq!(w.wallet(), 900);

    // Cook: 4 recipes, 6 deals, 2 perfect landings, 600 tribute.
    repeat(&mut w, 3, brewed);
    repeat(&mut w, 4, deal);
    repeat(&mut w, 2, perfect);
    w.send(track("wallet", 1000));
    assert_eq!(next(&w).goal_text, TextKey::new(PAY_TRIBUTE));
    w.send(pay("gribbles", 600));
    assert_eq!(w.rank("gribbles"), 3);
    assert!(w.progress.owns(&UnlockId::new("freeport_contact")));
    assert_eq!(w.wallet(), 1300);

    // Partner: 12 deals, 3 clean scans, 2000 tribute.
    repeat(&mut w, 6, deal);
    repeat(&mut w, 3, clean_scan);
    let g = next(&w);
    assert_eq!((g.goals_done, g.goals_total), (2, 2));
    w.send(track("wallet", 3000));
    w.send(pay("gribbles", 2000));
    assert_eq!(w.rank("gribbles"), 4);
    assert!(w.progress.owns(&UnlockId::new("family_seat")));
    assert_eq!(w.wallet(), 2300);
    assert!(w.empire.next_goal(&w.ladders, &w.progress, &w.k, "gribbles").is_none(), "the top rank");
}

#[test]
fn the_shipped_mechanic_sells_the_ship_upgrades() {
    let k = shipped_kernel();
    let shops = shipped_shops(&k);
    let mut w = World::new(k.clone(), shipped_ladders(&k), shipped_tallies(&k));
    let mech = &shops.shops["mechanic"].record;
    let bent = LocationId::new("bent_spoon");
    w.send(track("wallet", 1000));
    w.send_all(mech.buy(0, &w.progress, &w.k, bent.clone()).unwrap());
    assert_eq!(w.wallet(), 700, "the hidden hold, 400");
    assert!(w.progress.has_tag(&gameplay_core::Tag::new("ship_hidden_hold")));
    w.send_all(mech.buy(1, &w.progress, &w.k, bent.clone()).unwrap());
    assert_eq!(w.wallet(), 700 - 600, "the scan jammer, 600");
    assert!(w.progress.has_tag(&gameplay_core::Tag::new("ship_jammer")));
    assert!(w.progress.has_tag(&gameplay_core::Tag::new("ship_hidden_hold")));
}

#[test]
fn shipped_tally_ids_are_the_crew_tracks_of_the_ladder() {
    let k = shipped_kernel();
    for name in ["deals_done", "recipes_found", "perfect_landings", "clean_scans", "jobs_done"] {
        assert!(k.tracks.contains_key(&gameplay_core::TrackId::new(name)), "{name}");
    }
}
