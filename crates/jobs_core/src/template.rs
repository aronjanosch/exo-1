//! The `job_template` record (#123): what a job asks for, before the board picks concrete places
//! and goods (#126). One JSON file each in `job_template/`.
use std::collections::BTreeMap;

use content_core::{File, Loaded, Record, check_id, load_records};
use gameplay_core::content::Owner;
use gameplay_core::{CommodityId, Condition, Content, LocationId, Tag, TextKey, TrackId};
use serde::{Deserialize, Serialize};

use crate::id::TemplateId;

/// The folder this system reads.
pub const FOLDER: &str = "job_template";

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobTemplate {
    pub id: TemplateId,
    pub title: TextKey,
    pub brief: TextKey,
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
}

impl JobContent {
    /// Loads `job_template/` from `files` and checks fields and references. Returns every error,
    /// each naming the file and the field.
    pub fn load(files: &[File], kernel: &Content) -> Result<JobContent, Vec<String>> {
        let mut e = Vec::new();
        let c = JobContent { templates: load_records(files, FOLDER, &mut e) };
        for Loaded { path, record: t } in c.templates.values() {
            c.check(path, t, kernel, &mut e);
        }
        if e.is_empty() { Ok(c) } else { Err(e) }
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
            }
        }
        if t.reward <= 0 {
            e.push(format!("{path}: reward: must be positive"));
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
        if let Some(f) = &t.follow_up
            && !self.templates.contains_key(f)
        {
            e.push(format!("{path}: follow_up: unknown job template '{f}'"));
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
