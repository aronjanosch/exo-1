//! Helpers of the empire lane (#230): the shipped content, an inline kernel and fixtures, and a
//! `World` that routes events the way the host does (each id once, every system sees every
//! event, the events a system emits go round again).
#![allow(dead_code)]
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use gameplay_core::goals::{Empire, Ladders, Outcome};
use gameplay_core::notice::Notice;
use gameplay_core::shop::Shops;
use gameplay_core::tally::Tallies;
use gameplay_core::{ClientId, Content, Dedup, Event, File, Flag, Progress, Refusal, TrackId, WorldEvent};

pub const HOST: ClientId = ClientId(1);
pub const SIZES: [&str; 3] = ["small", "medium", "large"];

/// The inline kernel of the tests: money, the crew tracks, a personal track, four unlocks, four
/// places (one closed until a job is done), two goods.
pub const KERNEL: &[(&str, &str)] = &[
    ("progress_track/wallet.json", r#"{"id":"wallet","name":"track.wallet.name","owner":"crew","start":100}"#),
    ("progress_track/deals_done.json", r#"{"id":"deals_done","name":"track.deals_done.name","owner":"crew","min":0}"#),
    ("progress_track/recipes_found.json", r#"{"id":"recipes_found","name":"track.recipes_found.name","owner":"crew","min":0}"#),
    ("progress_track/perfect_landings.json", r#"{"id":"perfect_landings","name":"track.perfect_landings.name","owner":"crew","min":0}"#),
    ("progress_track/clean_scans.json", r#"{"id":"clean_scans","name":"track.clean_scans.name","owner":"crew","min":0}"#),
    ("progress_track/jobs_done.json", r#"{"id":"jobs_done","name":"track.jobs_done.name","owner":"crew","min":0}"#),
    ("progress_track/freight_xp.json", r#"{"id":"freight_xp","name":"track.freight_xp.name","owner":"player"}"#),
    ("unlock/lab_key.json", r#"{"id":"lab_key","name":"unlock.lab_key.name","price":0,"grants":["lab_access"]}"#),
    ("unlock/far_buyers.json", r#"{"id":"far_buyers","name":"unlock.far_buyers.name","price":0,"grants":["buyers_far"]}"#),
    ("unlock/hidden_hold.json", r#"{"id":"hidden_hold","name":"unlock.hidden_hold.name","price":400,"grants":["ship_hidden_hold"]}"#),
    ("unlock/second_kettle.json", r#"{"id":"second_kettle","name":"unlock.second_kettle.name","price":500,"grants":["lab_kettle_2"]}"#),
    ("location/bent_spoon.json", r#"{"id":"bent_spoon","name":"location.bent_spoon.name","place":"bent_spoon","pad":"main"}"#),
    ("location/drip_rock.json", r#"{"id":"drip_rock","name":"location.drip_rock.name","place":"drip_rock","pad":"main"}"#),
    ("location/back_alley.json", r#"{"id":"back_alley","name":"location.back_alley.name","place":"back_alley","pad":"main","available":{"track_at_least":{"track":"jobs_done","value":1}}}"#),
    ("location/slosh_wholesale.json", r#"{"id":"slosh_wholesale","name":"location.slosh_wholesale.name","place":"slosh_wholesale","pad":"main","tags":["wholesale"]}"#),
    ("commodity/fizzy_mud.json", r#"{"id":"fizzy_mud","name":"commodity.fizzy_mud.name","base_price":40,"crate_size":"small"}"#),
    ("commodity/sock_dust.json", r#"{"id":"sock_dust","name":"commodity.sock_dust.name","base_price":30,"crate_size":"small"}"#),
];

/// The tally fixtures: one per kind, each on a crew track.
pub const TALLIES: &[(&str, &str)] = &[
    ("tally/deal.json", r#"{"id":"deal","on":"product_order_settled","track":"deals_done"}"#),
    ("tally/recipe.json", r#"{"id":"recipe","on":"recipe_discovered","track":"recipes_found"}"#),
    ("tally/landing.json", r#"{"id":"landing","on":"landing_perfect","track":"perfect_landings"}"#),
    ("tally/scan.json", r#"{"id":"scan","on":"scan_clean","track":"clean_scans"}"#),
    ("tally/job.json", r#"{"id":"job","on":"flag:job_completed:","track":"jobs_done"}"#),
];

/// A two-rank ladder: rank one asks for 2 jobs and gives lab access; rank two asks for a recipe
/// and 2 deals, costs 200 and gives far buyers.
pub const LADDER: &[(&str, &str)] = &[(
    "ladder/gribbles.json",
    r#"{"id":"gribbles","name":"ladder.gribbles.name","ranks":[
        {"name":"rank.one","tribute":0,"requires":[{"text":"goal.jobs","condition":{"track_at_least":{"track":"jobs_done","value":2}}}],"unlocks":["lab_key"]},
        {"name":"rank.two","tribute":200,"requires":[
            {"text":"goal.recipe","condition":{"track_at_least":{"track":"recipes_found","value":1}}},
            {"text":"goal.deals","condition":{"track_at_least":{"track":"deals_done","value":2}}}],"unlocks":["far_buyers"]}
    ]}"#,
)];

pub fn fixture(list: &[(&str, &str)]) -> Vec<File> {
    list.iter().map(|(p, t)| File::new(*p, *t)).collect()
}

pub fn kernel_files() -> Vec<File> {
    fixture(KERNEL)
}

pub fn kernel() -> Content {
    Content::load(&kernel_files(), &SIZES).unwrap_or_else(|e| panic!("{e:#?}"))
}

/// The inline ladder, the tallies, and the given extra files, as one list.
pub fn fixture_files(extra: &[File]) -> Vec<File> {
    let mut f = kernel_files();
    f.extend(fixture(TALLIES));
    f.extend(fixture(LADDER));
    f.extend(extra.iter().cloned());
    f
}

fn read_folders(root: &Path) -> Vec<File> {
    let mut out = Vec::new();
    for d in std::fs::read_dir(root).unwrap() {
        let d = d.unwrap().path();
        if !d.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&d).unwrap() {
            let f = f.unwrap().path();
            if f.is_file() {
                out.push(File::new(f.strip_prefix(root).unwrap().to_str().unwrap(), std::fs::read_to_string(&f).unwrap()));
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The shipped `content/gameplay/` of the repo, every file.
pub fn shipped_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/gameplay")
}

pub fn shipped_files() -> Vec<File> {
    read_folders(&shipped_root())
}

pub fn shipped_kernel() -> Content {
    Content::load(&shipped_files(), &SIZES).unwrap_or_else(|e| panic!("{e:#?}"))
}

pub fn shipped_ladders(k: &Content) -> Ladders {
    Ladders::load(&shipped_files(), k).unwrap_or_else(|e| panic!("{e:#?}"))
}

pub fn shipped_tallies(k: &Content) -> Tallies {
    Tallies::load(&shipped_files(), k).unwrap_or_else(|e| panic!("{e:#?}"))
}

pub fn shipped_shops(k: &Content) -> Shops {
    Shops::load(&shipped_files(), k).unwrap_or_else(|e| panic!("{e:#?}"))
}

/// The state of a crew in a test: the systems, the progress, and the host's routing.
pub struct World {
    pub k: Content,
    pub ladders: Ladders,
    pub tallies: Tallies,
    pub empire: Empire,
    pub progress: Progress,
    pub dedup: Dedup,
    pub seq: u64,
    /// Every event the host applied, in order (requests and what the systems emitted).
    pub applied: Vec<WorldEvent>,
    /// Notices the systems raised, in order.
    pub notices: Vec<Notice>,
    /// Events the progress refused (the state did not change).
    pub refused: Vec<Refusal>,
}

impl World {
    pub fn new(k: Content, ladders: Ladders, tallies: Tallies) -> World {
        let progress = Progress::new(&k);
        World { k, ladders, tallies, empire: Empire::new(), progress, dedup: Dedup::default(), seq: 0, applied: Vec::new(), notices: Vec::new(), refused: Vec::new() }
    }

    /// The inline world: the fixture kernel, the ladder and the tallies.
    pub fn fixture() -> World {
        let k = kernel();
        let files = fixture_files(&[]);
        let ladders = Ladders::load(&files, &k).unwrap_or_else(|e| panic!("{e:#?}"));
        let tallies = Tallies::load(&files, &k).unwrap_or_else(|e| panic!("{e:#?}"));
        World::new(k, ladders, tallies)
    }

    /// Sends one event from the host and routes it, and everything the systems emit, to all systems.
    pub fn send(&mut self, payload: WorldEvent) {
        let mut queue = VecDeque::from([payload]);
        while let Some(p) = queue.pop_front() {
            self.seq += 1;
            let ev = Event::new(HOST, self.seq, p);
            if !self.dedup.first_time(ev.id) {
                continue;
            }
            self.applied.push(ev.payload.clone());
            match self.progress.apply_with_notices(&self.k, &ev) {
                Ok(ns) => self.notices.extend(ns),
                Err(r) => self.refused.push(r),
            }
            for o in self.empire.apply_world(&self.ladders, &self.k, &self.progress, &ev) {
                match o {
                    Outcome::Emit(e) => queue.push_back(e),
                    Outcome::Notice(n) => self.notices.push(n),
                }
            }
            queue.extend(self.tallies.apply_world(&ev));
        }
    }

    pub fn send_all(&mut self, payloads: Vec<WorldEvent>) {
        for p in payloads {
            self.send(p);
        }
    }

    pub fn wallet(&self) -> i64 {
        self.progress.wallet()
    }

    pub fn rank(&self, ladder: &str) -> u32 {
        self.empire.rank(ladder)
    }
}

pub fn track(t: &str, delta: i64) -> WorldEvent {
    WorldEvent::TrackChanged { track: TrackId::new(t), delta, player: None }
}

pub fn job_done(template: &str) -> WorldEvent {
    WorldEvent::FlagRaised { flag: Flag::new(format!("job_completed:{template}")) }
}

pub fn pay(ladder: &str, amount: i64) -> WorldEvent {
    WorldEvent::TributePaid { ladder: ladder.to_string(), amount }
}

/// The last notice with this key the world raised.
pub fn notice<'a>(w: &'a World, key: &str) -> Option<&'a Notice> {
    w.notices.iter().rev().find(|n| n.key.as_str() == key)
}
