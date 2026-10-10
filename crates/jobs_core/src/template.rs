//! The `job_template` record (#123): what a job asks for, before the board picks concrete places
//! and goods (#126). One JSON file each in `job_template/`.
use std::collections::BTreeMap;

use content_core::{File, Loaded, Record, check_id, load_records};
use gameplay_core::content::Owner;
use gameplay_core::text::TextTable;
use gameplay_core::{CommodityId, Condition, Content, LocationId, Tag, TextKey, TrackId};
use serde::{Deserialize, Serialize};

use crate::giver::{self, Giver, GiverId};
use crate::licence::{self, Licence, LicenceId};
use crate::id::TemplateId;

/// The folder this system reads.
pub const FOLDER: &str = "job_template";

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobTemplate {
    pub id: TemplateId,
    pub title: TextKey,
    pub brief: TextKey,
    /// Who offers it (a `giver` record); none for a job nobody in particular gives.
    #[serde(default)]
    pub giver: Option<GiverId>,
    /// At least one. Milestone D has only `deliver`.
    pub objectives: Vec<ObjectiveSpec>,
    /// Money at full grade, before modifiers.
    pub reward: i64,
    pub grading: Grading,
    /// Seconds from the first pickup; none means no deadline.
    #[serde(default)]
    pub deadline_s: Option<f64>,
    /// At most one warning and one upside.
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
    /// The personal track the participants' XP goes to.
    pub track: TrackId,
    /// XP at full grade for each participant.
    pub xp: i64,
    /// Who may accept it, read for the accepting player; none means anyone.
    #[serde(default)]
    pub available: Option<Condition>,
    /// Offered until it is completed once (story and targeted jobs).
    #[serde(default)]
    pub once_only: bool,
    /// Offered once this one is completed (#126).
    #[serde(default)]
    pub follow_up: Option<TemplateId>,
    /// Set for a licence exam.
    #[serde(default)]
    pub exam: Option<Exam>,
}

impl Record for JobTemplate {
    type Id = TemplateId;
    fn id(&self) -> &TemplateId {
        &self.id
    }
}

/// One objective kind; each is a reducer over domain events. New kinds come with their systems.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ObjectiveSpec {
    /// Carry `amount` crates of a commodity from one place to another.
    Deliver { from: PlaceSpec, to: PlaceSpec, commodity: CommoditySpec, amount: [u32; 2] },
    /// The ship leaves the ground under the player's hands (the event `TookOff`). Checks come in
    /// the order of the objectives, and only from the player who took the job (#169).
    TakeOff {},
    /// The ship comes over the pad of a location (`PadReached`).
    ReachPad { at: LocationId },
    /// The ship touches down on the pad of a location (`Landed`); faster than `max_mps` towards the
    /// ground is a crash and fails the job. Only these events are read, never the flight model.
    Land { at: LocationId, max_mps: f64 },
}

/// A job that is a licence exam (#169): it costs a fee to take, grants a personal track when
/// passed, and may give honours.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exam {
    /// Paid on the first try. TODO(initiator): a starting value.
    pub fee: i64,
    /// Paid for every later try of the same player.
    pub retry_fee: i64,
    /// The personal track set to 1 when passed.
    pub grants: TrackId,
    /// A delivered crate must be at least this good (0..1) or the exam is failed.
    pub min_condition: f64,
    #[serde(default)]
    pub honours: Option<Honours>,
}

/// Passed with honours: a soft touchdown and a good time give a standing bonus with a giver.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Honours {
    pub max_touchdown_mps: f64,
    /// Seconds from take-off to the end.
    pub within_s: f64,
    pub giver: GiverId,
    pub standing: i64,
}

/// A fixed location or any location with a tag (the board picks one, never the same for both ends).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PlaceSpec {
    Location(LocationId),
    Tagged(Tag),
}

/// The commodity: one of a pool (a pool of one is a fixed commodity).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CommoditySpec {
    OneOf(Vec<CommodityId>),
}

/// How the payout follows the delivered share and the crates' condition.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grading {
    /// Ascending by share. The highest band whose share is reached pays its factor; below the
    /// lowest band nothing is paid.
    pub bands: Vec<Band>,
    /// 0..1: how much a wrecked crate costs. Factor = 1 - weight × (1 - mean condition).
    pub condition_weight: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Band {
    /// Delivered share 0..1 (delivered crates / asked crates).
    pub share: f64,
    /// Factor on the reward and the XP.
    pub pays: f64,
}

/// A warning or upside shown on the offer. Encounters with the same tag come more often on its
/// flights (#127); a warning's bonus raises the pay.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifier {
    pub tag: Tag,
    pub kind: ModifierKind,
    /// Added to the hazard factor (0.25 = +25 %).
    #[serde(default)]
    pub bonus: f64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierKind {
    Warning,
    Upside,
}

/// All job templates, checked against the kernel content.
#[derive(Clone, Debug, PartialEq)]
pub struct JobContent {
    pub templates: BTreeMap<TemplateId, Loaded<JobTemplate>>,
    pub givers: BTreeMap<GiverId, Loaded<Giver>>,
    pub licences: BTreeMap<LicenceId, Loaded<Licence>>,
}

impl JobContent {
    /// Loads `job_template/` from `files` and checks fields and references. Returns every error,
    /// each naming the file and the field.
    pub fn load(files: &[File], kernel: &Content) -> Result<JobContent, Vec<String>> {
        let mut e = Vec::new();
        let c = JobContent { templates: load_records(files, FOLDER, &mut e), givers: load_records(files, giver::FOLDER, &mut e), licences: load_records(files, licence::FOLDER, &mut e) };
        for Loaded { path, record: t } in c.templates.values() {
            c.check(path, t, kernel, &mut e);
        }
        for Loaded { path, record: g } in c.givers.values() {
            c.check_giver(path, g, kernel, &mut e);
        }
        for Loaded { path, record: l } in c.licences.values() {
            c.check_licence(path, l, kernel, &mut e);
        }
        if e.is_empty() { Ok(c) } else { Err(e) }
    }

    fn check_giver(&self, path: &str, g: &Giver, k: &Content, e: &mut Vec<String>) {
        check_id(path, "id", g.id.as_str(), e);
        k.check_location(path, "location", &g.location, e);
        match k.tracks.get(&g.standing) {
            None => e.push(format!("{path}: standing: unknown track '{}'", g.standing)),
            Some(t) if t.record.owner != Owner::Crew => e.push(format!("{path}: standing: '{}' must be a crew track", g.standing)),
            Some(_) => {}
        }
        if g.gain < 0 {
            e.push(format!("{path}: gain: must not be negative"));
        }
        if g.loss < 0 {
            e.push(format!("{path}: loss: must not be negative"));
        }
        if let Some(c) = &g.available {
            k.check_condition(path, "available", c, e);
        }
    }

    fn check_licence(&self, path: &str, l: &Licence, k: &Content, e: &mut Vec<String>) {
        check_id(path, "id", l.id.as_str(), e);
        match k.tracks.get(&l.track) {
            None => e.push(format!("{path}: track: unknown track '{}'", l.track)),
            Some(t) if t.record.owner != Owner::Player => e.push(format!("{path}: track: '{}' must be a personal track", l.track)),
            Some(_) => {}
        }
        match self.templates.get(&l.exam).map(|t| &t.record) {
            None => e.push(format!("{path}: exam: unknown job template '{}'", l.exam)),
            Some(t) => match &t.exam {
                None => e.push(format!("{path}: exam: '{}' is no exam (it has no exam block)", l.exam)),
                Some(x) if x.grants != l.track => e.push(format!("{path}: track: the exam grants '{}', not '{}'", x.grants, l.track)),
                Some(_) => {}
            },
        }
        if l.allows.is_empty() {
            e.push(format!("{path}: allows: empty"));
        }
    }

    /// The licences that allow something (`pilot_ship`).
    pub fn licences_allowing(&self, tag: &str) -> Vec<&Licence> {
        self.licences.values().map(|l| &l.record).filter(|l| l.allows.iter().any(|t| t.as_str() == tag)).collect()
    }

    /// Checks the texts the givers and their templates need against the text table: every voice
    /// key has a pool of enough lines, every template of a giver has its title and brief.
    pub fn check_texts(&self, table: &TextTable) -> Vec<String> {
        let mut e = Vec::new();
        for g in self.givers.values() {
            giver::check_texts(g, table, &mut e);
        }
        for Loaded { path, record: l } in self.licences.values() {
            if !table.has(l.name.as_str()) {
                e.push(format!("{path}: name: no text '{}'", l.name));
            }
        }
        for Loaded { path, record: t } in self.templates.values().filter(|t| t.record.giver.is_some()) {
            for (field, k) in [("title", &t.title), ("brief", &t.brief)] {
                if !table.has(k.as_str()) {
                    e.push(format!("{path}: {field}: no text '{k}'"));
                }
            }
        }
        e
    }

    /// Checks that all text pools have at least 3 lines (TODO(initiator): pool text placeholders).
    pub fn check_pool_sizes(&self, table: &TextTable) -> Vec<String> {
        let mut e = Vec::new();
        for g in self.givers.values() {
            giver::check_pool_sizes(g, table, &mut e);
        }
        e
    }

    fn check(&self, path: &str, t: &JobTemplate, k: &Content, e: &mut Vec<String>) {
        check_id(path, "id", t.id.as_str(), e);
        if t.objectives.is_empty() {
            e.push(format!("{path}: objectives: empty"));
        }
        for o in &t.objectives {
            match o {
                ObjectiveSpec::Deliver { from, to, commodity, amount } => {
                    check_place(path, "objectives.deliver.from", from, k, e);
                    check_place(path, "objectives.deliver.to", to, k, e);
                    if let (PlaceSpec::Location(a), PlaceSpec::Location(b)) = (from, to)
                        && a == b
                    {
                        e.push(format!("{path}: objectives.deliver.to: same location as from"));
                    }
                    let CommoditySpec::OneOf(pool) = commodity;
                    if pool.is_empty() {
                        e.push(format!("{path}: objectives.deliver.commodity: empty pool"));
                    }
                    for c in pool {
                        k.check_commodity(path, "objectives.deliver.commodity", c, e);
                    }
                    if amount[0] == 0 || amount[0] > amount[1] {
                        e.push(format!("{path}: objectives.deliver.amount: needs 1 <= min <= max"));
                    }
                }
                ObjectiveSpec::TakeOff {} => {}
                ObjectiveSpec::ReachPad { at } => k.check_location(path, "objectives.reach_pad.at", at, e),
                ObjectiveSpec::Land { at, max_mps } => {
                    k.check_location(path, "objectives.land.at", at, e);
                    if !(*max_mps > 0.0) {
                        e.push(format!("{path}: objectives.land.max_mps: must be positive"));
                    }
                }
            }
        }
        // An exam costs a fee instead of paying a reward.
        if t.reward < 0 || t.reward == 0 && t.exam.is_none() {
            e.push(format!("{path}: reward: must be positive (zero only for an exam)"));
        }
        if let Some(x) = &t.exam {
            self.check_exam(path, x, k, e);
        }
        let b = &t.grading.bands;
        if b.is_empty() {
            e.push(format!("{path}: grading.bands: empty"));
        }
        if b.windows(2).any(|w| w[0].share >= w[1].share || w[0].pays > w[1].pays) {
            e.push(format!("{path}: grading.bands: shares must rise and pays must not fall"));
        }
        if b.iter().any(|b| !(b.share > 0.0 && b.share <= 1.0) || b.pays < 0.0) {
            e.push(format!("{path}: grading.bands: share must be in (0, 1], pays not negative"));
        }
        if !(0.0..=1.0).contains(&t.grading.condition_weight) {
            e.push(format!("{path}: grading.condition_weight: must be in 0..1"));
        }
        if t.deadline_s.is_some_and(|d| !(d > 0.0)) {
            e.push(format!("{path}: deadline_s: must be positive"));
        }
        for kind in [ModifierKind::Warning, ModifierKind::Upside] {
            if t.modifiers.iter().filter(|m| m.kind == kind).count() > 1 {
                e.push(format!("{path}: modifiers: at most one {kind:?} (lower case in the data)"));
            }
        }
        for m in &t.modifiers {
            check_id(path, "modifiers.tag", m.tag.as_str(), e);
            if m.bonus < 0.0 {
                e.push(format!("{path}: modifiers.bonus: must not be negative"));
            }
            if m.kind == ModifierKind::Upside && m.bonus != 0.0 {
                e.push(format!("{path}: modifiers.bonus: only a warning pays a bonus"));
            }
        }
        match k.tracks.get(&t.track) {
            None => e.push(format!("{path}: track: unknown track '{}'", t.track)),
            Some(tr) if tr.record.owner != Owner::Player => e.push(format!("{path}: track: '{}' must be a personal track", t.track)),
            Some(_) => {}
        }
        if t.xp < 0 {
            e.push(format!("{path}: xp: must not be negative"));
        }
        if let Some(c) = &t.available {
            k.check_condition(path, "available", c, e);
        }
        if let Some(g) = &t.giver
            && !self.givers.contains_key(g)
        {
            e.push(format!("{path}: giver: unknown giver '{g}'"));
        }
        if let Some(f) = &t.follow_up
            && !self.templates.contains_key(f)
        {
            e.push(format!("{path}: follow_up: unknown job template '{f}'"));
        }
    }
}

impl JobContent {
    fn check_exam(&self, path: &str, x: &Exam, k: &Content, e: &mut Vec<String>) {
        if x.fee < 0 {
            e.push(format!("{path}: exam.fee: must not be negative"));
        }
        if x.retry_fee < 0 || x.retry_fee > x.fee {
            e.push(format!("{path}: exam.retry_fee: must be between 0 and the fee"));
        }
        match k.tracks.get(&x.grants) {
            None => e.push(format!("{path}: exam.grants: unknown track '{}'", x.grants)),
            Some(t) if t.record.owner != Owner::Player => e.push(format!("{path}: exam.grants: '{}' must be a personal track", x.grants)),
            Some(_) => {}
        }
        if !(0.0..=1.0).contains(&x.min_condition) {
            e.push(format!("{path}: exam.min_condition: must be in 0..1"));
        }
        if let Some(h) = &x.honours {
            if !self.givers.contains_key(&h.giver) {
                e.push(format!("{path}: exam.honours.giver: unknown giver '{}'", h.giver));
            }
            if !(h.max_touchdown_mps > 0.0 && h.within_s > 0.0) || h.standing < 0 {
                e.push(format!("{path}: exam.honours: needs positive max_touchdown_mps and within_s, and a standing that is not negative"));
            }
        }
    }
}

fn check_place(path: &str, field: &str, p: &PlaceSpec, k: &Content, e: &mut Vec<String>) {
    match p {
        PlaceSpec::Location(l) => k.check_location(path, field, l, e),
        PlaceSpec::Tagged(t) => {
            if !k.locations.values().any(|l| l.record.tags.contains(t)) {
                e.push(format!("{path}: {field}: no location has the tag '{t}'"));
            }
        }
    }
}
