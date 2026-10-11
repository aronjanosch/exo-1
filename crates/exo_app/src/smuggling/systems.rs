//! The round-6 systems in one place: contents and states of traffic, heat, the lab and the family
//! ladder with its tallies and shops, routed from `gameplay::gameplay_step` like the customers. The
//! glue's facts about the world (contraband aboard, the ship's tags) come in through `cargo` and
//! the progress; the requests for the world (spawn crates, confiscate) go out through fields the
//! Bevy systems read.
use gameplay_core::goals::{Empire, Ladders};
use gameplay_core::notice::Notice;
use gameplay_core::save::{Envelope, SaveError};
use gameplay_core::shop::Shops;
use gameplay_core::tally::Tallies;
use gameplay_core::text::TextTable;
use gameplay_core::{CommodityId, Content, Event, File, LocationId, ProductSpec, Progress, ScanChoice, Tag, TownId, WorldEvent};
use heat_core::{Heat, HeatContent, Patrol};
use production_core::{Lab, LabContent};
use traffic_core::{Traffic, TrafficContent};

/// The family ladder the HUD follows. TODO(initiator): more ladders later.
pub const LADDER: &str = "gribbles";
/// Tags the shop's upgrades grant (`content/gameplay/unlock/`).
pub const TAG_HIDDEN_HOLD: &str = "ship_hidden_hold";
pub const TAG_JAMMER: &str = "ship_jammer";
pub const TAG_LAB: &str = "lab_access";
pub const TAG_KETTLE_2: &str = "lab_kettle_2";
/// Crates the hidden hold hides. TODO(initiator).
pub const HIDDEN_HOLD_CRATES: u32 = 2;
/// Seeds of the new systems (fixed, so scenarios repeat; the save keeps the states).
const TRAFFIC_SEED: u64 = 0x7a1f;
const HEAT_SEED: u64 = 0x4ea7;
const PATROL_SEED: u64 = 0x9a71;

/// What a round-6 system gives back to the host.
pub enum Out {
    Emit(WorldEvent),
    Notice(Notice),
    /// A radio line: who speaks (a text key: the town's name) and the line's pool.
    Say { who: gameplay_core::TextKey, key: gameplay_core::TextKey },
}

/// Contraband the ship carries, counted by the glue each step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CargoView {
    /// Contraband crates in the cabin that a scan sees.
    pub visible: u32,
    /// Contraband crates the hidden hold hides.
    pub hidden: u32,
}

/// Crates the glue should spawn at a location's pad.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnStock {
    pub commodity: CommodityId,
    pub product: Option<ProductSpec>,
    pub count: u32,
    pub at: LocationId,
}

pub struct Systems {
    pub traffic_c: TrafficContent,
    pub traffic: Traffic,
    pub heat_c: HeatContent,
    pub heat: Heat,
    /// Air patrols that order a hot ship to halt (`heat_core::patrol`).
    pub patrol: Patrol,
    /// The open scan came from a patrol in the air (fleeing is speeding off, not taking off).
    pub patrol_scan: bool,
    pub lab_c: LabContent,
    pub lab: Lab,
    pub ladders: Ladders,
    pub empire: Empire,
    pub tallies: Tallies,
    pub shops: Shops,
    pub cargo: CargoView,
    next_scan: u64,
    /// The scan waiting for an answer and its pad.
    pub open_scan: Option<(u64, LocationId)>,
    /// A scan took the visible contraband: the glue removes it.
    pub confiscate: u32,
    pub spawn: Vec<SpawnStock>,
    /// Round-6 events seen, oldest first (scenario checks), short.
    pub log: Vec<String>,
    /// Radio lines as said (speaker, text, game clock), oldest first; the HUD shows the recent ones.
    pub radio: Vec<super::RadioLine>,
    /// The buyer's order the arrow points to (F on an order in the phone).
    pub tracked_order: Option<gameplay_core::OrderId>,
    /// The clearance whose 20 s warning was given (town, pad), so it is said once.
    pub reminded: Option<(TownId, LocationId)>,
}

impl Systems {
    pub fn load(files: &[File], kernel: &Content, text: &TextTable) -> Result<Systems, Vec<String>> {
        let mut e = Vec::new();
        let traffic_c = TrafficContent::load(files, kernel).map_err(|x| e.extend(x)).ok();
        let heat_c = HeatContent::load(files).map_err(|x| e.extend(x)).ok();
        let lab_c = LabContent::load(files, kernel).map_err(|x| e.extend(x)).ok();
        let ladders = Ladders::load(files, kernel).map_err(|x| e.extend(x)).ok();
        let tallies = Tallies::load(files, kernel).map_err(|x| e.extend(x)).ok();
        let shops = Shops::load(files, kernel).map_err(|x| e.extend(x)).ok();
        if let Some(t) = &traffic_c {
            e.extend(t.check_texts(text));
        }
        if let Some(l) = &ladders {
            e.extend(l.check_texts(text));
        }
        if let Some(s) = &shops {
            e.extend(s.check_texts(text));
        }
        if !e.is_empty() {
            return Err(e);
        }
        Ok(Systems {
            traffic_c: traffic_c.unwrap(),
            traffic: Traffic::new(TRAFFIC_SEED),
            heat_c: heat_c.unwrap(),
            heat: Heat::new(HEAT_SEED),
            patrol: Patrol::new(PATROL_SEED),
            patrol_scan: false,
            lab_c: lab_c.unwrap(),
            lab: Lab::new(),
            ladders: ladders.unwrap(),
            empire: Empire::new(),
            tallies: tallies.unwrap(),
            shops: shops.unwrap(),
            cargo: CargoView::default(),
            next_scan: 1,
            open_scan: None,
            confiscate: 0,
            spawn: Vec::new(),
            log: Vec::new(),
            radio: Vec::new(),
            tracked_order: None,
            reminded: None,
        })
    }

    /// The town whose official pad this is (not the free port).
    pub fn official_town(&self, at: &LocationId) -> Option<&traffic_core::Town> {
        let town = traffic_core::town_of_pad(&self.traffic_c, at)?;
        self.traffic_c.towns.get(town).map(|t| &t.record).filter(|t| !t.free_port)
    }

    pub fn town(&self, id: &TownId) -> Option<&traffic_core::Town> {
        self.traffic_c.towns.get(id).map(|t| &t.record)
    }

    /// One applied world event through every round-6 system.
    pub fn apply_world(&mut self, kernel: &Content, progress: &Progress, ev: &Event<WorldEvent>) -> Vec<Out> {
        let mut out = Vec::new();
        for o in self.traffic.apply_world(&self.traffic_c, kernel, ev) {
            out.push(match o {
                traffic_core::Outcome::Emit(e) => Out::Emit(e),
                traffic_core::Outcome::Notice(n) => Out::Notice(n),
                traffic_core::Outcome::Say { town, key } => Out::Say { who: self.town(&town).map(|t| t.name.clone()).unwrap_or_else(|| gameplay_core::TextKey::new(town.as_str())), key },
            });
        }
        for o in self.heat.apply_world(&self.heat_c, ev) {
            out.push(match o {
                heat_core::Outcome::Emit(e) => Out::Emit(e),
                heat_core::Outcome::Notice(n) => Out::Notice(n),
            });
        }
        for o in self.lab.apply_world(ev) {
            out.push(match o {
                production_core::LabOutcome::Emit(e) => Out::Emit(e),
                production_core::LabOutcome::Notice(n) => Out::Notice(n),
            });
        }
        for o in self.empire.apply_world(&self.ladders, kernel, progress, ev) {
            out.push(match o {
                gameplay_core::goals::Outcome::Emit(e) => Out::Emit(e),
                gameplay_core::goals::Outcome::Notice(n) => Out::Notice(n),
            });
        }
        out.extend(self.tallies.apply_world(ev).into_iter().map(Out::Emit));
        match &ev.payload {
            // An official pad may scan whoever lands on it (the free port and unofficial spots never do).
            WorldEvent::Landed { at: Some(at), .. } if self.open_scan.is_none() => {
                if let Some(town) = self.official_town(at) {
                    let inspection = town.inspection_pad.as_ref() == Some(at);
                    let jammer = progress.has_tag(&Tag::new(TAG_JAMMER));
                    let roll = self.heat.roll_scan(&self.heat_c.tuning.record, inspection, jammer);
                    self.log.push(format!("landed at {at}: scan {roll} (inspection {inspection}, jammer {jammer})"));
                    if roll {
                        let scan = self.next_scan;
                        self.next_scan += 1;
                        out.push(Out::Emit(WorldEvent::ScanStarted { scan, at: at.clone(), visible: self.cargo.visible, hidden: self.cargo.hidden }));
                    }
                }
            }
            WorldEvent::ScanStarted { scan, at, .. } => {
                self.open_scan = Some((*scan, at.clone()));
            }
            WorldEvent::ScanResolved { scan, choice, confiscate, fine, .. } => {
                if self.open_scan.as_ref().is_some_and(|(s, _)| s == scan) {
                    self.open_scan = None;
                }
                self.patrol.scan_over(&self.heat_c.tuning.record);
                self.patrol_scan = false;
                if *confiscate {
                    self.confiscate += 1;
                }
                self.log.push(format!("scan {scan}: {choice:?}, confiscate {confiscate}, fine {fine}"));
            }
            // Taking off during a scan is fleeing it.
            WorldEvent::TookOff => {
                if let Some((scan, _)) = &self.open_scan {
                    out.push(Out::Emit(WorldEvent::ScanAnswered { scan: *scan, choice: ScanChoice::Flee }));
                }
            }
            WorldEvent::GoodsBought { commodity, amount, at } => {
                self.spawn.push(SpawnStock { commodity: commodity.clone(), product: None, count: *amount, at: at.clone() });
            }
            WorldEvent::ClearanceGranted { pad, .. } => self.log.push(format!("cleared for {pad}")),
            WorldEvent::LandingGraded { at, grade, on_time, .. } => self.log.push(format!("graded {grade} at {at}, on time {on_time}")),
            WorldEvent::Violation { kind, .. } => self.log.push(format!("violation {kind:?}")),
            WorldEvent::RankReached { ladder, rank } => self.log.push(format!("rank {rank} on {ladder}")),
            // The rival gloats on the radio when it takes a buyer.
            WorldEvent::CustomerPoached { by } => {
                self.log.push(format!("poached {by}"));
                out.push(Out::Say { who: gameplay_core::TextKey::new("rival.name"), key: gameplay_core::TextKey::new("rival.poach") });
            }
            WorldEvent::ProductOrderSettled { order, .. } if self.tracked_order == Some(*order) => self.tracked_order = None,
            _ => {}
        }
        out
    }

    /// A scan the glue starts (a patrol's halt in the air): the next scan id and the cargo seen.
    pub fn scan_at(&mut self, at: LocationId) -> WorldEvent {
        let scan = self.next_scan;
        self.next_scan += 1;
        WorldEvent::ScanStarted { scan, at, visible: self.cargo.visible, hidden: self.cargo.hidden }
    }

    pub fn save(&self, env: &mut Envelope) {
        self.traffic.save(env);
        self.heat.save(env);
        self.patrol.save(env);
        self.lab.save(env);
        self.empire.save(env);
    }

    pub fn restore(&mut self, env: &Envelope) -> Result<(), SaveError> {
        self.traffic = Traffic::load(env)?.unwrap_or_else(|| Traffic::new(TRAFFIC_SEED));
        self.heat = Heat::load(env)?.unwrap_or_else(|| Heat::new(HEAT_SEED));
        self.patrol = Patrol::load(env)?.unwrap_or_else(|| Patrol::new(PATROL_SEED));
        self.patrol_scan = false;
        self.lab = Lab::load(env)?.unwrap_or_else(Lab::new);
        self.empire = Empire::load(env)?.unwrap_or_default();
        self.open_scan = None;
        self.confiscate = 0;
        self.spawn.clear();
        Ok(())
    }
}
