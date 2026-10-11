//! Traffic of round 6 (#226): airspace, the tower's answers, clearance, landing grades, fines and
//! speeding, through the events. Every test loads the SHIPPED content; a few tweak one file.
use std::path::Path;

use gameplay_core::save::Envelope;
use gameplay_core::text::TextTable;
use gameplay_core::{ClientId, Content, Event, File, LocationId, TrackId, TownId, ViolationKind, WorldEvent};
use traffic_core::*;

const PILOT: ClientId = ClientId(7);
const DRIP: &str = "drip_rock";
const WALLET: &str = "wallet";

/// Every file of the shipped `content/gameplay`.
fn shipped() -> Vec<File> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/gameplay");
    let mut out = Vec::new();
    for d in std::fs::read_dir(&root).unwrap() {
        let d = d.unwrap().path();
        if !d.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&d).unwrap() {
            let f = f.unwrap().path();
            out.push(File::new(f.strip_prefix(&root).unwrap().to_str().unwrap(), std::fs::read_to_string(&f).unwrap()));
        }
    }
    out
}

/// The shipped files with one text replaced in one of them.
fn tweak(path: &str, from: &str, to: &str) -> Vec<File> {
    let mut files = shipped();
    let f = files.iter_mut().find(|f| f.path == path).unwrap();
    assert!(f.text.contains(from), "{from} in {path}");
    f.text = f.text.replace(from, to);
    files
}

/// The kernel from the shipped locations, tracks, commodities and unlocks (as the texts test does).
fn kernel(files: &[File]) -> Content {
    let k: Vec<File> = files.iter().filter(|f| ["commodity/", "location/", "progress_track/", "unlock/"].iter().any(|p| f.path.starts_with(p))).cloned().collect();
    Content::load(&k, &["small", "medium", "large"]).unwrap()
}

/// Drip Rock with the hold chance at zero: the tower always grants at once after the delay.
fn no_hold() -> Vec<File> {
    tweak("town/drip_rock.json", "\"hold_chance\": 0.3", "\"hold_chance\": 0.0")
}

struct World {
    k: Content,
    tc: TrafficContent,
    t: Traffic,
    seq: u64,
}

impl World {
    fn new(files: &[File], seed: u64) -> World {
        let k = kernel(files);
        let tc = TrafficContent::load(files, &k).unwrap();
        World { k, tc, t: Traffic::new(seed), seq: 0 }
    }

    fn send(&mut self, payload: WorldEvent) -> Vec<Outcome> {
        self.seq += 1;
        self.t.apply_world(&self.tc, &self.k, &Event::new(PILOT, self.seq, payload))
    }

    fn hail(&mut self, town: &str) -> Vec<Outcome> {
        self.send(WorldEvent::Hailed { town: TownId::new(town) })
    }

    fn land(&mut self, at: &str, speed: f64) -> Vec<Outcome> {
        self.send(WorldEvent::Landed { at: Some(LocationId::new(at)), speed })
    }

    /// The ship's airspace as seen at this step (`observe` is the glue's call, not an event).
    fn observe(&mut self, town: Option<&str>, speed: f64, dt: f64) -> Vec<Outcome> {
        self.t.observe(&self.tc, PILOT, town.map(TownId::new), speed, dt)
    }

    /// Time passes in 0.5 s steps.
    fn advance(&mut self, secs: f64) -> Vec<Outcome> {
        let mut out = Vec::new();
        for _ in 0..(secs / 0.5).round() as usize {
            out.extend(self.send(WorldEvent::TimePassed { dt: 0.5 }));
        }
        out
    }

    /// Enters Drip Rock's airspace, hails it, and waits for the grant.
    fn granted_at_drip(&mut self) {
        self.observe(Some(DRIP), 0.0, 0.0);
        self.hail(DRIP);
        self.advance(2.5);
        assert!(self.t.clearance(PILOT).is_some(), "granted after the delay");
    }
}

fn events(out: &[Outcome]) -> Vec<WorldEvent> {
    out.iter()
        .filter_map(|o| match o {
            Outcome::Emit(e) => Some(e.clone()),
            _ => None,
        })
        .collect()
}

/// The radio lines as `town:key`.
fn says(out: &[Outcome]) -> Vec<String> {
    out.iter()
        .filter_map(|o| match o {
            Outcome::Say { town, key } => Some(format!("{town}:{}", key.as_str())),
            _ => None,
        })
        .collect()
}

fn notices(out: &[Outcome]) -> Vec<String> {
    out.iter()
        .filter_map(|o| match o {
            Outcome::Notice(n) => Some(n.key.as_str().to_string()),
            _ => None,
        })
        .collect()
}

fn wallet(delta: i64) -> WorldEvent {
    WorldEvent::TrackChanged { track: TrackId::new(WALLET), delta, player: None }
}

fn drip() -> TownId {
    TownId::new(DRIP)
}

#[test]
fn hail_out_of_range_gets_a_notice() {
    let mut w = World::new(&shipped(), 1);
    let out = w.hail(DRIP);
    assert_eq!(notices(&out), vec!["notice.traffic.out_of_range"]);
    assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.out_of_range"]);
    assert!(events(&out).is_empty());
    assert!(w.t.pending(PILOT).is_none(), "no answer is coming");
    assert!(events(&w.advance(5.0)).is_empty(), "and none comes later");
}

#[test]
fn airspace_entry_and_exit_are_raised_on_change() {
    let mut w = World::new(&shipped(), 1);
    assert_eq!(events(&w.observe(Some(DRIP), 0.0, 0.0)), vec![WorldEvent::AirspaceEntered { town: drip() }]);
    assert!(events(&w.observe(Some(DRIP), 0.0, 1.0)).is_empty(), "no change, no event");
    assert_eq!(events(&w.observe(None, 0.0, 1.0)), vec![WorldEvent::AirspaceLeft { town: drip() }]);
    assert_eq!(w.t.inside(PILOT), None);
}

#[test]
fn hail_is_granted_after_the_delay() {
    let mut w = World::new(&no_hold(), 1);
    w.observe(Some(DRIP), 0.0, 0.0);
    let out = w.hail(DRIP);
    assert!(events(&out).is_empty() && notices(&out).is_empty(), "the answer comes after the delay");
    assert!(events(&w.advance(2.0)).is_empty(), "still no answer at 2 s");
    let out = w.advance(0.5);
    let ev = events(&out);
    assert_eq!(ev.len(), 1, "one answer: {ev:?}");
    let WorldEvent::ClearanceGranted { town, pad, window_s, inspection } = &ev[0] else { panic!("granted, got {ev:?}") };
    assert_eq!(town, &drip());
    assert!(["drip_rock", "drip_rock_north", "slosh_wholesale"].contains(&pad.as_str()), "a normal pad, not the inspection pad: {pad}");
    assert_eq!(*window_s, 90.0);
    assert!(!inspection);
    assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.granted"]);
    assert_eq!(w.t.clearance(PILOT).unwrap().pad, *pad);
}

#[test]
fn hold_then_grant() {
    let mut w = World::new(&tweak("town/drip_rock.json", "\"hold_chance\": 0.3", "\"hold_chance\": 1.0"), 1);
    w.observe(Some(DRIP), 0.0, 0.0);
    w.hail(DRIP);
    let out = w.advance(2.5);
    assert_eq!(events(&out), vec![WorldEvent::ClearanceHeld { town: drip(), wait_s: 20.0 }]);
    assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.held"]);
    assert!(w.t.clearance(PILOT).is_none(), "a held sender has no clearance yet");
    assert!(events(&w.advance(19.5)).is_empty(), "the hold lasts 20 s");
    let out = w.advance(0.5);
    assert!(events(&out).iter().any(|e| matches!(e, WorldEvent::ClearanceGranted { inspection: false, .. })), "the second answer grants: {:?}", events(&out));
    assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.granted"]);
}

#[test]
fn high_heat_sends_the_crew_to_the_inspection_pad() {
    let mut w = World::new(&no_hold(), 1);
    w.send(WorldEvent::HeatStars { stars: 3 });
    assert_eq!(w.t.heat_stars(), 3, "the last heat is remembered");
    w.granted_at_drip();
    let c = w.t.clearance(PILOT).unwrap();
    assert_eq!((c.pad.as_str(), c.inspection), ("drip_rock_inspect", true));
    assert_eq!(c.time_left_s, 90.0);
}

#[test]
fn inspection_is_said_in_the_inspection_line() {
    let mut w = World::new(&no_hold(), 1);
    w.send(WorldEvent::HeatStars { stars: 3 });
    w.observe(Some(DRIP), 0.0, 0.0);
    w.hail(DRIP);
    let out = w.advance(2.5);
    assert!(events(&out).contains(&WorldEvent::ClearanceGranted { town: drip(), pad: LocationId::new("drip_rock_inspect"), window_s: 90.0, inspection: true }));
    assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.inspection"]);
}

#[test]
fn low_heat_never_sends_the_crew_to_the_inspection_pad() {
    let files = no_hold();
    for seed in 0..20 {
        let mut w = World::new(&files, seed);
        w.send(WorldEvent::HeatStars { stars: 2 });
        w.observe(Some(DRIP), 0.0, 0.0);
        w.hail(DRIP);
        let out = w.advance(2.5);
        let granted: Vec<WorldEvent> = events(&out).into_iter().filter(|e| matches!(e, WorldEvent::ClearanceGranted { .. })).collect();
        assert_eq!(granted.len(), 1, "seed {seed}");
        assert!(matches!(&granted[0], WorldEvent::ClearanceGranted { pad, inspection: false, .. } if pad.as_str() != "drip_rock_inspect"), "seed {seed}: {granted:?}");
    }
}

#[test]
fn expiry_after_the_window() {
    let mut w = World::new(&no_hold(), 1);
    w.granted_at_drip();
    assert!(events(&w.advance(89.5)).is_empty(), "the window is 90 s");
    let out = w.advance(0.5);
    assert_eq!(events(&out), vec![WorldEvent::ClearanceExpired { town: drip() }]);
    assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.expired"]);
    assert!(w.t.clearance(PILOT).is_none());
}

#[test]
fn hailing_again_while_cleared_repeats_the_clearance() {
    let mut w = World::new(&no_hold(), 1);
    w.granted_at_drip();
    let pad = w.t.clearance(PILOT).unwrap().pad.clone();
    w.advance(10.0);
    let out = w.hail(DRIP);
    assert_eq!(events(&out), vec![WorldEvent::ClearanceGranted { town: drip(), pad, window_s: 80.0, inspection: false }]);
    assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.granted"]);
    assert!(w.t.pending(PILOT).is_none());
}

#[test]
fn landing_on_time_is_graded_by_touchdown_speed() {
    // Bands from the tuning: 1.5 / 3.0 / 6.0 m/s for grades 3 / 2 / 1, faster is 0.
    for (speed, grade) in [(0.5, 3u8), (1.5, 3), (1.6, 2), (3.0, 2), (3.1, 1), (6.0, 1), (6.1, 0), (20.0, 0)] {
        let mut w = World::new(&no_hold(), 1);
        w.granted_at_drip();
        let pad = w.t.clearance(PILOT).unwrap().pad.as_str().to_string();
        let out = w.land(&pad, speed);
        assert_eq!(events(&out), vec![WorldEvent::LandingGraded { town: drip(), at: LocationId::new(&pad), grade, on_time: true }], "speed {speed}");
        assert_eq!(says(&out), vec![format!("drip_rock:tower.grumpy.graded.{grade}")], "speed {speed}");
        assert!(w.t.clearance(PILOT).is_none(), "the clearance ends with the landing");
    }
}

#[test]
fn landing_on_the_wrong_pad_is_a_violation_with_a_fine() {
    let mut w = World::new(&no_hold(), 1);
    w.granted_at_drip();
    let assigned = w.t.clearance(PILOT).unwrap().pad.as_str().to_string();
    let other = ["drip_rock", "drip_rock_north", "slosh_wholesale", "drip_rock_inspect"].into_iter().find(|p| *p != assigned).unwrap();
    let out = w.land(other, 0.5);
    assert_eq!(
        events(&out),
        vec![
            WorldEvent::Violation { town: drip(), kind: ViolationKind::WrongPad },
            wallet(-25),
            WorldEvent::LandingGraded { town: drip(), at: LocationId::new(other), grade: 3, on_time: false },
        ]
    );
    assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.violation.wrong_pad"]);
}

#[test]
fn landing_without_clearance_is_a_violation_with_a_fine() {
    let mut w = World::new(&no_hold(), 1);
    w.observe(Some(DRIP), 0.0, 0.0);
    let out = w.land("drip_rock", 0.5);
    assert_eq!(
        events(&out),
        vec![
            WorldEvent::Violation { town: drip(), kind: ViolationKind::NoClearance },
            wallet(-40),
            WorldEvent::LandingGraded { town: drip(), at: LocationId::new("drip_rock"), grade: 3, on_time: false },
        ]
    );
    assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.violation.no_clearance"]);
}

#[test]
fn an_unofficial_spot_raises_nothing() {
    let mut w = World::new(&no_hold(), 1);
    w.observe(Some(DRIP), 0.0, 0.0);
    let out = w.send(WorldEvent::Landed { at: None, speed: 0.5 });
    assert!(events(&out).is_empty() && says(&out).is_empty());
}

#[test]
fn free_port_is_silent() {
    let mut w = World::new(&tweak("town/drip_rock.json", "\"free_port\": false", "\"free_port\": true"), 1);
    assert_eq!(events(&w.observe(Some(DRIP), 0.0, 0.0)), vec![WorldEvent::AirspaceEntered { town: drip() }]);
    let out = w.hail(DRIP);
    assert_eq!(notices(&out), vec!["notice.traffic.no_tower"]);
    assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.no_tower"]);
    assert!(events(&w.advance(5.0)).is_empty(), "no answer comes in a free port");
    assert!(w.t.pending(PILOT).is_none());
    assert!(events(&w.land("drip_rock", 0.5)).is_empty(), "a landing without clearance is no violation");
    let mut all = Vec::new();
    for _ in 0..60 {
        all.extend(w.observe(Some(DRIP), 100.0, 1.0));
    }
    assert!(events(&all).is_empty(), "no speeding in a free port");
}

#[test]
fn speeding_has_a_grace_time_and_a_cooldown() {
    // Limit 30 m/s, grace 3 s, cooldown 20 s: fines at 4 s, then again once the cooldown ran out.
    let mut w = World::new(&shipped(), 1);
    w.observe(Some(DRIP), 0.0, 0.0);
    let mut fired = Vec::new();
    for t in 1..=24 {
        let out = w.observe(Some(DRIP), 65.0, 1.0);
        if events(&out).contains(&WorldEvent::Violation { town: drip(), kind: ViolationKind::Speeding }) {
            assert!(events(&out).contains(&wallet(-15)), "the fine comes with it at {t} s");
            assert_eq!(says(&out), vec!["drip_rock:tower.grumpy.violation.speeding"]);
            fired.push(t);
        }
    }
    assert_eq!(fired, vec![4, 24]);
}

#[test]
fn a_pending_answer_and_a_clearance_survive_a_save() {
    // One save with a pending answer, one with a clearance.
    let mut pending = World::new(&shipped(), 9);
    pending.send(WorldEvent::HeatStars { stars: 2 });
    pending.observe(Some(DRIP), 0.0, 0.0);
    pending.hail(DRIP);
    pending.advance(1.0);
    assert!(pending.t.pending(PILOT).is_some());

    let mut cleared = World::new(&no_hold(), 9);
    cleared.granted_at_drip();
    cleared.advance(7.0);

    for w in [pending, cleared] {
        let mut env = Envelope::new();
        w.t.save(&mut env);
        let loaded = Traffic::load(&Envelope::from_json(&env.to_json()).unwrap()).unwrap().unwrap();
        assert_eq!(loaded, w.t);
    }
}

#[test]
fn shipped_content_loads_and_its_texts_exist() {
    let files = shipped();
    let k = kernel(&files);
    let tc = TrafficContent::load(&files, &k).unwrap();
    assert_eq!(tc.towns.len(), 4);
    assert_eq!(tc.tuning.record.window_s, 90.0);
    let en = files.iter().find(|f| f.path == "text/en.json").unwrap();
    let table = TextTable::from_json(&en.path, &en.text).unwrap();
    assert_eq!(tc.check_texts(&table), Vec::<String>::new());
    assert_eq!(town_of_pad(&tc, &LocationId::new("noodle_town_inspect")).map(|t| t.as_str()), Some("noodle_town"));
    assert_eq!(town_of_pad(&tc, &LocationId::new("bent_spoon_east")).map(|t| t.as_str()), Some("bent_spoon"));
    assert!(tc.towns[&gameplay_core::TownId::new("gulch_freeport")].record.free_port, "the free port has no tower");
    // Courier drops near Drip Rock are no town pads: landing there is no tower's business.
    assert_eq!(town_of_pad(&tc, &LocationId::new("lint_trap")), None);
    assert_eq!(town_of_pad(&tc, &LocationId::new("drip_rock")).map(|t| t.as_str()), Some("drip_rock"));
    assert_eq!(town_of_pad(&tc, &LocationId::new("bent_spoon_not_a_pad")), None);
}

#[test]
fn a_missing_pool_line_is_a_text_error() {
    let files = shipped();
    let k = kernel(&files);
    let tc = TrafficContent::load(&files, &k).unwrap();
    let en = tweak("text/en.json", "\"tower.chirpy.granted\": [", "\"tower.chirpy.granted_old\": [");
    let f = en.iter().find(|f| f.path == "text/en.json").unwrap();
    let table = TextTable::from_json(&f.path, &f.text).unwrap();
    let errs = tc.check_texts(&table);
    assert!(errs.iter().any(|e| e.contains("tower.chirpy.granted")), "{errs:?}");
    assert!(pool_keys("tower.chirpy").contains(&"tower.chirpy.granted".to_string()));
}

#[test]
fn loader_names_the_file_and_the_field() {
    let k = kernel(&shipped());
    let errs = TrafficContent::load(&tweak("town/drip_rock.json", "\"drip_rock_inspect\"],", "\"nowhere\"],"), &k).err().unwrap();
    assert!(errs.iter().any(|e| e.contains("town/drip_rock.json") && e.contains("pads") && e.contains("nowhere")), "{errs:?}");

    let errs = TrafficContent::load(&tweak("town/drip_rock.json", "\"inspection_pad\": \"drip_rock_inspect\"", "\"inspection_pad\": \"bent_spoon\""), &k).err().unwrap();
    assert!(errs.iter().any(|e| e.contains("town/drip_rock.json") && e.contains("inspection_pad")), "{errs:?}");

    let errs = TrafficContent::load(&tweak("town/bent_spoon.json", "\"pads\": [\"bent_spoon\", \"bent_spoon_east\"", "\"pads\": [\"bent_spoon\", \"drip_rock\""), &k).err().unwrap();
    assert!(errs.iter().any(|e| e.contains("town/bent_spoon.json") && e.contains("already")), "{errs:?}");
}
