//! Jobs as a state machine over domain events (#123): offered → active → completed, expired or
//! abandoned. Each deliver leg is a reducer over the crate events of its own crates.
use std::collections::{BTreeMap, BTreeSet};

use gameplay_core::notice::{Arg, Notice, NoticeKind};
use gameplay_core::save::{Envelope, SaveError};
use gameplay_core::{ClientId, CommodityId, Content, CrateId, Event, Flag, LocationId, OrderId, Progress, TextKey, TrackId, WorldEvent};
use serde::{Deserialize, Serialize};

use crate::giver::{GiverHistory, GiverId};
use crate::grading::{Grade, grade};
use crate::id::{JobId, TemplateId};
use crate::template::{CommoditySpec, JobContent, JobTemplate, ObjectiveSpec, PlaceSpec};

/// Active jobs per crew at most (initiator, 2026-10-09).
pub const MAX_ACTIVE: usize = 2;

/// Events of the jobs system.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobEvent {
    /// The sender takes the offer for the crew.
    OfferAccepted { job: JobId },
    /// Anyone in the crew drops the job; its crates lose their value.
    JobAbandoned { job: JobId },
    /// The host spawned the crates a `SpawnCrates` outcome asked for, with their stable ids.
    CratesSpawned { job: JobId, leg: usize, crates: Vec<CrateId> },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Offered,
    Active,
    Completed,
    Expired,
    Abandoned,
    /// An exam that went wrong: a crash landing, a crate too damaged (#169).
    Failed,
}

/// A check of an exam: something the player's ship has to do (not a delivery).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    TakeOff,
    ReachPad { at: LocationId },
    Land { at: LocationId, max_mps: f64 },
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Pending,
    /// Seconds on the job's clock when it was done, and a value (the touchdown speed for a landing).
    Done { at_s: f64, value: f64 },
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub kind: CheckKind,
    pub state: CheckState,
}

/// What a crate of a leg went through.
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrateMark {
    /// Spawned, not picked up yet (or set down somewhere else than the dropoff).
    Waiting,
    Carried,
    Delivered { condition: f64 },
    Lost,
}

/// One concrete deliver objective.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Leg {
    pub from: LocationId,
    pub to: LocationId,
    pub commodity: CommodityId,
    pub amount: u32,
    /// Filled by `CratesSpawned` once the job is accepted.
    pub crates: BTreeMap<CrateId, CrateMark>,
}

impl Leg {
    pub fn new(from: LocationId, to: LocationId, commodity: CommodityId, amount: u32) -> Leg {
        Leg { from, to, commodity, amount, crates: BTreeMap::new() }
    }

    pub fn delivered(&self) -> u32 {
        self.crates.values().filter(|m| matches!(m, CrateMark::Delivered { .. })).count() as u32
    }

    pub fn condition_sum(&self) -> f64 {
        self.crates.values().map(|m| if let CrateMark::Delivered { condition } = m { *condition } else { 0.0 }).sum()
    }

    /// Spawned, and every crate delivered or lost: nothing more can happen.
    pub fn resolved(&self) -> bool {
        self.crates.len() as u32 == self.amount && self.crates.values().all(|m| matches!(m, CrateMark::Delivered { .. } | CrateMark::Lost))
    }
}

/// The template orders become offers of (a `job_template` of this id must exist for orders to
/// show up; its places, commodity, amount, reward and deadline are replaced by the order's).
pub const ORDER_TEMPLATE: &str = "customer_order";

/// What a customer order asks of its job (#168).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderTerms {
    pub order: OrderId,
    /// The customer's id, for texts (`customer.<by>.name`).
    pub by: String,
    /// Money at full grade, instead of the template's.
    pub reward: i64,
    pub deadline_s: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub template: TemplateId,
    pub state: JobState,
    pub legs: Vec<Leg>,
    pub accepted_by: Option<ClientId>,
    /// Who accepted, picked up or delivered for it; they get the XP.
    pub participants: BTreeSet<ClientId>,
    /// Seconds since the first pickup; none before it.
    pub clock_s: Option<f64>,
    /// Set for a job that came from a customer's order.
    #[serde(default)]
    pub order: Option<OrderTerms>,
    /// What an exam asks of the ship besides the delivery, in order.
    #[serde(default)]
    pub checks: Vec<Check>,
}

impl Job {
    pub fn delivered(&self) -> u32 {
        self.legs.iter().map(Leg::delivered).sum()
    }

    /// Every delivery is resolved and every check done: nothing more can happen.
    fn finished(&self) -> bool {
        self.legs.iter().all(Leg::resolved) && self.checks.iter().all(|c| matches!(c.state, CheckState::Done { .. }))
    }

    pub fn asked(&self) -> u32 {
        self.legs.iter().map(|l| l.amount).sum()
    }

    /// The deadline: an order's own, else the template's.
    pub fn deadline_s(&self, t: &JobTemplate) -> Option<f64> {
        match &self.order {
            Some(o) => o.deadline_s,
            None => t.deadline_s,
        }
    }

    /// Seconds left before the deadline; none without a deadline or before the first pickup.
    pub fn time_left(&self, t: &JobTemplate) -> Option<f64> {
        Some((self.deadline_s(t)? - self.clock_s?).max(0.0))
    }

    /// `time_left` with the template looked up in `jc`.
    pub fn time_left_of(&self, jc: &JobContent) -> Option<f64> {
        self.time_left(&jc.templates.get(&self.template)?.record)
    }

    /// The text key of the title: a customer's order has its own (`customer.<by>.order_title`).
    pub fn title_key(&self, t: &JobTemplate) -> TextKey {
        match &self.order {
            Some(o) => TextKey::new(format!("customer.{}.order_title", o.by)),
            None => t.title.clone(),
        }
    }

    fn leg_of(&mut self, c: CrateId) -> Option<&mut Leg> {
        self.legs.iter_mut().find(|l| l.crates.contains_key(&c))
    }
}

/// What the host does after an event: spawn or release crates, route new domain events, tell the
/// players.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Spawn `count` crates of `commodity` at the pad of `at`, then send `CratesSpawned`.
    SpawnCrates { job: JobId, leg: usize, commodity: CommodityId, count: u32, at: LocationId },
    /// These crates belong to no job any more and are worth nothing.
    ReleaseCrates { crates: Vec<CrateId> },
    /// A job ended; `grade` is none for an abandoned job.
    Ended { job: JobId, state: JobState, grade: Option<Grade> },
    /// A domain event for every system (payout, XP, flag); the host gives it an id and applies it.
    Emit(WorldEvent),
    /// Something the players should hear about (#165); the host queues it.
    Notice(Notice),
}

/// Why a jobs event was not applied. The state is unchanged.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    UnknownJob,
    NotOffered,
    NotActive,
    TooManyActive,
    /// The template's `available` condition does not hold for the accepting player.
    NotAvailable,
    /// `CratesSpawned` for an unknown leg, twice, or with the wrong count.
    BadSpawn,
    /// An exam costs more than the crew has (#169).
    CannotAfford { price: i64, wallet: i64 },
}

/// The crew's offers and jobs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Jobs {
    next_id: u64,
    jobs: BTreeMap<JobId, Job>,
    /// How the crew stands with each giver (#167): drives the mood of the briefings.
    #[serde(default)]
    history: BTreeMap<GiverId, GiverHistory>,
    /// Paid tries of an exam per player and template (`"<client>:<template>"`): the retry fee.
    #[serde(default)]
    attempts: BTreeMap<String, u32>,
    /// Board state per location (#126).
    #[serde(default)]
    pub board: crate::board::Board,
}

/// The jobs section of a save (#128).
pub const SECTION: &str = "jobs";
pub const SECTION_VERSION: u32 = 1;

impl Jobs {
    pub fn save(&self, env: &mut Envelope) {
        env.put(SECTION, SECTION_VERSION, self);
    }

    /// None if the save has no jobs section; an error for a version this build does not read.
    pub fn load(env: &Envelope) -> Result<Option<Jobs>, SaveError> {
        env.get(SECTION, SECTION_VERSION)
    }

    /// Makes an offer from a template and concrete legs (the board's job, #126).
    pub fn offer(&mut self, template: &TemplateId, legs: Vec<Leg>) -> JobId {
        self.offer_with(template, legs, Vec::new())
    }

    /// `offer` with the checks of an exam.
    pub fn offer_with(&mut self, template: &TemplateId, legs: Vec<Leg>, checks: Vec<Check>) -> JobId {
        let id = JobId(self.next_id);
        self.next_id += 1;
        self.jobs.insert(id, Job { id, template: template.clone(), state: JobState::Offered, legs, accepted_by: None, participants: BTreeSet::new(), clock_s: None, order: None, checks });
        id
    }

    /// How many paid tries of an exam the player has made.
    pub fn attempts(&self, who: ClientId, template: &TemplateId) -> u32 {
        self.attempts.get(&format!("{}:{template}", who.0)).copied().unwrap_or(0)
    }

    /// Makes an offer from a template that names everything: fixed places, one commodity, one
    /// amount (targeted and story jobs). None if it leaves a choice to the board.
    pub fn offer_fixed(&mut self, t: &JobTemplate) -> Option<JobId> {
        let (mut legs, mut checks) = (Vec::new(), Vec::new());
        for o in &t.objectives {
            match o {
                ObjectiveSpec::Deliver { from: PlaceSpec::Location(a), to: PlaceSpec::Location(b), commodity: CommoditySpec::OneOf(pool), amount } if pool.len() == 1 && amount[0] == amount[1] => {
                    legs.push(Leg::new(a.clone(), b.clone(), pool[0].clone(), amount[0]));
                }
                ObjectiveSpec::Deliver { .. } => return None,
                ObjectiveSpec::TakeOff {} => checks.push(Check { kind: CheckKind::TakeOff, state: CheckState::Pending }),
                ObjectiveSpec::ReachPad { at } => checks.push(Check { kind: CheckKind::ReachPad { at: at.clone() }, state: CheckState::Pending }),
                ObjectiveSpec::Land { at, max_mps } => checks.push(Check { kind: CheckKind::Land { at: at.clone(), max_mps: *max_mps }, state: CheckState::Pending }),
            }
        }
        Some(self.offer_with(&t.id, legs, checks))
    }

    /// Like `offer_fixed`, but only if all locations in the legs are available.
    pub fn offer_fixed_if_available(&mut self, t: &JobTemplate, kernel: &Content, progress: &Progress) -> Option<JobId> {
        let (mut legs, mut checks) = (Vec::new(), Vec::new());
        for o in &t.objectives {
            match o {
                ObjectiveSpec::Deliver { from: PlaceSpec::Location(a), to: PlaceSpec::Location(b), commodity: CommoditySpec::OneOf(pool), amount } if pool.len() == 1 && amount[0] == amount[1] => {
                    // Check if both locations are available
                    if !progress.location_available(kernel, a) || !progress.location_available(kernel, b) {
                        return None;
                    }
                    legs.push(Leg::new(a.clone(), b.clone(), pool[0].clone(), amount[0]));
                }
                ObjectiveSpec::Deliver { .. } => return None,
                ObjectiveSpec::TakeOff {} => checks.push(Check { kind: CheckKind::TakeOff, state: CheckState::Pending }),
                ObjectiveSpec::ReachPad { at } => {
                    if !progress.location_available(kernel, at) {
                        return None;
                    }
                    checks.push(Check { kind: CheckKind::ReachPad { at: at.clone() }, state: CheckState::Pending })
                }
                ObjectiveSpec::Land { at, max_mps } => {
                    if !progress.location_available(kernel, at) {
                        return None;
                    }
                    checks.push(Check { kind: CheckKind::Land { at: at.clone(), max_mps: *max_mps }, state: CheckState::Pending })
                }
            }
        }
        Some(self.offer_with(&t.id, legs, checks))
    }

    /// An offer of `t`: its fixed places, else places by tag search; None when no place fits.
    fn offer_from_template(&mut self, t: &JobTemplate, kernel: &Content, progress: &Progress, rng: &mut gameplay_core::rng::Rng) -> Option<JobId> {
        if let Some(id) = self.offer_fixed_if_available(t, kernel, progress) {
            return Some(id);
        }
        let legs = crate::board::generate_legs(t, kernel, progress, rng).ok()?;
        Some(self.offer_with(&t.id, legs, Vec::new()))
    }

    /// Generate board offers for a location with a given seed.
    /// Returns a list of job ids that are now offered.
    pub fn generate_board_at(&mut self, jc: &JobContent, kernel: &Content, progress: &Progress, location: &str, seed: u64) -> Vec<JobId> {
        use crate::board::{BoardLocation, templates_for_location};
        use gameplay_core::rng::Rng;

        let mut rng = Rng::new(seed);
        let mut job_ids = Vec::new();
        let mut used_templates = BTreeSet::new();

        // Collect templates for this location (filtered by giver location, excludes exams/orders)
        let available_templates: Vec<&JobTemplate> =
            templates_for_location(jc, kernel, progress, location)
                .into_iter()
                .filter(|t| {
                    // Don't offer once_only templates that already have completed jobs
                    if t.once_only {
                        let flag = Flag::new(format!("job_completed:{}", t.id));
                        !progress.has_flag(&flag)
                    } else {
                        true
                    }
                })
                // A follow-up waits for the job it follows (#126).
                .filter(|t| prerequisites(jc, &t.id).all(|p| completed(progress, p)))
                .collect();

        // Follow-ups whose job is done come first, each once while none of it is open.
        for t in available_templates.iter().filter(|t| prerequisites(jc, &t.id).next().is_some()) {
            let open = self.jobs.values().any(|j| j.template == t.id && matches!(j.state, JobState::Offered | JobState::Active));
            if !open && let Some(id) = self.offer_from_template(t, kernel, progress, &mut rng) {
                job_ids.push(id);
                used_templates.insert(t.id.clone());
            }
        }

        if available_templates.is_empty() {
            self.board.offers_per_location.insert(location.to_string(), BoardLocation {
                offer_ids: Vec::new(),
                age_s: 0.0,
                seed,
            });
            return job_ids;
        }

        // Decide how many offers to generate (3-5 if we have at least 3 qualifying templates, else all)
        let target_count = if available_templates.len() >= 3 {
            3 + rng.below(3)
        } else {
            available_templates.len()
        };

        // Try to generate offers, allowing at most 1 per template (no duplicates)
        let mut attempts = 0;
        let max_attempts = available_templates.len() * 3;
        while job_ids.len() < target_count && attempts < max_attempts && !available_templates.is_empty() {
            let template_idx = rng.below(available_templates.len());
            let template = available_templates[template_idx];

            // Skip if we already used this template on this board
            if used_templates.contains(&template.id) {
                attempts += 1;
                continue;
            }

            // Try fixed offer first (for templates with all fixed locations and single commodity)
            let created = if let Some(id) = self.offer_from_template(template, kernel, progress, &mut rng) {
                job_ids.push(id);
                true
            } else {
                false
            };

            if created {
                used_templates.insert(template.id.clone());
            }
            attempts += 1;
        }

        // Track board state
        self.board.offers_per_location.insert(location.to_string(), BoardLocation {
            offer_ids: job_ids.clone(),
            age_s: 0.0,
            seed,
        });

        job_ids
    }

    /// Tick the board forward by dt seconds; rotate expired unaccepted offers and generate new ones.
    /// Accepted offers are never rotated.
    pub fn tick_board(&mut self, jc: &JobContent, kernel: &Content, progress: &Progress, location: &str, dt: f64) -> Vec<JobId> {
        use crate::board::{OFFER_LIFETIME_S, next_seed};

        let should_rotate = {
            if let Some(board_loc) = self.board.offers_per_location.get_mut(location) {
                board_loc.age_s += dt;
                board_loc.age_s >= OFFER_LIFETIME_S
            } else {
                false
            }
        };

        if !should_rotate {
            return Vec::new();
        }

        // Find which offers to remove (those that are still offered, not accepted)
        let to_remove: Vec<JobId> = if let Some(board_loc) = self.board.offers_per_location.get(location) {
            board_loc
                .offer_ids
                .iter()
                .filter(|id| {
                    if let Some(job) = self.jobs.get(id) {
                        // Remove only if still offered (not accepted)
                        job.state == JobState::Offered
                    } else {
                        true // Remove if job not found
                    }
                })
                .copied()
                .collect()
        } else {
            Vec::new()
        };

        // Remove the expired offers from Jobs
        for id in to_remove {
            self.jobs.remove(&id);
        }

        // Generate new offers with a deterministic next seed
        let (new_seed, location_str) = {
            let board_loc = self.board.offers_per_location.get_mut(location).unwrap();
            let old_seed = board_loc.seed;
            let new_seed = next_seed(old_seed);
            board_loc.age_s = 0.0;
            board_loc.seed = new_seed;
            (new_seed, location.to_string())
        };

        self.generate_board_at(jc, kernel, progress, &location_str, new_seed)
    }

    /// What happened between the crew and a giver; nothing yet for a new one.
    pub fn history(&self, g: &GiverId) -> GiverHistory {
        self.history.get(g).cloned().unwrap_or_default()
    }

    pub fn get(&self, id: JobId) -> Option<&Job> {
        self.jobs.get(&id)
    }

    pub fn all(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values()
    }

    pub fn active(&self) -> impl Iterator<Item = &Job> {
        self.jobs.values().filter(|j| j.state == JobState::Active)
    }

    /// Applies an event of the jobs system. `progress` is read for the template's condition.
    pub fn apply_job(&mut self, jc: &JobContent, kernel: &Content, progress: &Progress, ev: &Event<JobEvent>) -> Result<Vec<Outcome>, Refusal> {
        let by = ev.sender();
        match &ev.payload {
            JobEvent::OfferAccepted { job } => {
                let active = self.active().count();
                let j = self.jobs.get_mut(job).ok_or(Refusal::UnknownJob)?;
                if j.state != JobState::Offered {
                    return Err(Refusal::NotOffered);
                }
                if active >= MAX_ACTIVE {
                    return Err(Refusal::TooManyActive);
                }
                let t = &jc.templates.get(&j.template).ok_or(Refusal::UnknownJob)?.record;
                if let Some(c) = &t.available
                    && !c.holds(kernel, progress, Some(by))
                {
                    return Err(Refusal::NotAvailable);
                }
                // A giver that is not open offers nothing (the family in D).
                if let Some(c) = t.giver.as_ref().and_then(|g| jc.givers.get(g)).and_then(|g| g.record.available.as_ref())
                    && !c.holds(kernel, progress, Some(by))
                {
                    return Err(Refusal::NotAvailable);
                }
                // An exam costs a fee, half of it for every try after the first (per player).
                let mut fee_out = Vec::new();
                if let Some(x) = &t.exam {
                    let key = format!("{}:{}", by.0, j.template);
                    let tries = self.attempts.get(&key).copied().unwrap_or(0);
                    let fee = if tries == 0 { x.fee } else { x.retry_fee };
                    let wallet = progress.wallet();
                    if wallet < fee {
                        return Err(Refusal::CannotAfford { price: fee, wallet });
                    }
                    self.attempts.insert(key, tries + 1);
                    if fee > 0 {
                        fee_out.push(Outcome::Emit(WorldEvent::TrackChanged { track: TrackId::new(gameplay_core::content::WALLET), delta: -fee, player: None }));
                    }
                }
                j.state = JobState::Active;
                j.accepted_by = Some(by);
                j.participants.insert(by);
                let mut out = fee_out;
                out.push(Outcome::Notice(Notice::new(NoticeKind::Accepted, "notice.job.accepted").arg("title", Arg::Key(j.title_key(t)))));
                out.extend(j.legs.iter().enumerate().map(|(i, l)| Outcome::SpawnCrates { job: *job, leg: i, commodity: l.commodity.clone(), count: l.amount, at: l.from.clone() }));
                Ok(out)
            }
            JobEvent::JobAbandoned { job } => {
                let j = self.jobs.get_mut(job).ok_or(Refusal::UnknownJob)?;
                if j.state != JobState::Active {
                    return Err(Refusal::NotActive);
                }
                j.state = JobState::Abandoned;
                let crates = j.legs.iter().flat_map(|l| l.crates.keys().copied()).collect();
                let mut out = vec![Outcome::ReleaseCrates { crates }, Outcome::Ended { job: *job, state: JobState::Abandoned, grade: None }];
                if let Some(t) = jc.templates.get(&j.template) {
                    out.push(Outcome::Notice(Notice::new(NoticeKind::Warning, "notice.job.abandoned").arg("title", Arg::Key(j.title_key(&t.record)))));
                }
                out.extend(settle_order(j, true));
                Ok(out)
            }
            JobEvent::CratesSpawned { job, leg, crates } => {
                let j = self.jobs.get_mut(job).ok_or(Refusal::UnknownJob)?;
                if j.state != JobState::Active {
                    return Err(Refusal::NotActive);
                }
                let l = j.legs.get_mut(*leg).ok_or(Refusal::BadSpawn)?;
                let unique: BTreeSet<_> = crates.iter().collect();
                if !l.crates.is_empty() || crates.len() as u32 != l.amount || unique.len() != crates.len() {
                    return Err(Refusal::BadSpawn);
                }
                l.crates = crates.iter().map(|c| (*c, CrateMark::Waiting)).collect();
                Ok(Vec::new())
            }
        }
    }

    /// Applies a world event: crate moves and time. Never refused; events about other crates
    /// change nothing.
    pub fn apply_world(&mut self, jc: &JobContent, ev: &Event<WorldEvent>) -> Vec<Outcome> {
        let by = ev.sender();
        let mut ended = Vec::new();
        let mut notes = Vec::new();
        let title = |j: &Job| jc.templates.get(&j.template).map(|t| Arg::Key(j.title_key(&t.record)));
        match &ev.payload {
            WorldEvent::CratePickedUp { crate_id, .. } => {
                if let Some(j) = self.job_with(*crate_id)
                    && let Some(l) = j.leg_of(*crate_id)
                {
                    let m = l.crates.get_mut(crate_id).expect("leg_of found it");
                    if matches!(m, CrateMark::Waiting | CrateMark::Carried) {
                        let first = *m == CrateMark::Waiting;
                        *m = CrateMark::Carried;
                        j.participants.insert(by);
                        j.clock_s.get_or_insert(0.0);
                        if first && let Some(t) = title(j) {
                            notes.push(Outcome::Notice(Notice::new(NoticeKind::Updated, "notice.job.picked_up").arg("title", t).arg("delivered", Arg::Number(j.delivered() as i64)).arg("asked", Arg::Number(j.asked() as i64))));
                        }
                    }
                }
            }
            WorldEvent::CrateDelivered { crate_id, at, condition } => {
                if let Some(j) = self.job_with(*crate_id)
                    && let Some(l) = j.leg_of(*crate_id)
                {
                    let to = l.to.clone();
                    let m = l.crates.get_mut(crate_id).expect("leg_of found it");
                    if matches!(m, CrateMark::Waiting | CrateMark::Carried) {
                        // Set down elsewhere: it waits there to be picked up again.
                        let there = *at == to;
                        *m = if there { CrateMark::Delivered { condition: condition.clamp(0.0, 1.0) } } else { CrateMark::Waiting };
                        j.participants.insert(by);
                        if there && let Some(t) = title(j) {
                            notes.push(Outcome::Notice(Notice::new(NoticeKind::Updated, "notice.job.delivered").arg("title", t).arg("delivered", Arg::Number(j.delivered() as i64)).arg("asked", Arg::Number(j.asked() as i64))));
                        }
                        if j.finished() {
                            ended.push((j.id, JobState::Completed));
                        }
                    }
                }
            }
            WorldEvent::CrateLost { crate_id } => {
                if let Some(j) = self.job_with(*crate_id)
                    && let Some(l) = j.leg_of(*crate_id)
                {
                    let m = l.crates.get_mut(crate_id).expect("leg_of found it");
                    if !matches!(m, CrateMark::Delivered { .. }) {
                        *m = CrateMark::Lost;
                        if let Some(t) = title(j) {
                            notes.push(Outcome::Notice(Notice::new(NoticeKind::Warning, "notice.job.crate_lost").arg("title", t)));
                        }
                        if j.finished() {
                            ended.push((j.id, JobState::Completed));
                        }
                    }
                }
            }
            WorldEvent::TimePassed { dt } => {
                for j in self.jobs.values_mut().filter(|j| j.state == JobState::Active) {
                    let Some(clock) = j.clock_s.as_mut() else { continue };
                    *clock += dt;
                    let clock = *clock;
                    let deadline = jc.templates.get(&j.template).and_then(|t| j.deadline_s(&t.record));
                    if deadline.is_some_and(|d| clock >= d) {
                        ended.push((j.id, JobState::Expired));
                    }
                }
            }
            WorldEvent::TookOff => {
                for j in self.jobs.values_mut().filter(|j| j.state == JobState::Active && j.accepted_by == Some(by)) {
                    let clock = *j.clock_s.get_or_insert(0.0);
                    if let Some(c) = j.checks.iter_mut().find(|c| c.state == CheckState::Pending)
                        && c.kind == CheckKind::TakeOff
                    {
                        c.state = CheckState::Done { at_s: clock, value: 0.0 };
                    }
                }
            }
            WorldEvent::PadReached { at } => {
                for j in self.jobs.values_mut().filter(|j| j.state == JobState::Active && j.accepted_by == Some(by)) {
                    let clock = j.clock_s.unwrap_or(0.0);
                    if let Some(c) = j.checks.iter_mut().find(|c| c.state == CheckState::Pending)
                        && matches!(&c.kind, CheckKind::ReachPad { at: want } if want == at)
                    {
                        c.state = CheckState::Done { at_s: clock, value: 0.0 };
                    }
                }
            }
            WorldEvent::Landed { at: Some(at), speed } => {
                for j in self.jobs.values_mut().filter(|j| j.state == JobState::Active && j.accepted_by == Some(by)) {
                    let clock = j.clock_s.unwrap_or(0.0);
                    let id = j.id;
                    let finished_before = j.finished();
                    let Some(c) = j.checks.iter_mut().find(|c| c.state == CheckState::Pending) else { continue };
                    let CheckKind::Land { at: want, max_mps } = c.kind.clone() else { continue };
                    if want != *at {
                        continue;
                    }
                    if *speed > max_mps {
                        c.state = CheckState::Failed;
                        ended.push((id, JobState::Failed));
                    } else {
                        c.state = CheckState::Done { at_s: clock, value: *speed };
                        if !finished_before && j.finished() {
                            ended.push((id, JobState::Completed));
                        }
                    }
                }
            }
            WorldEvent::Landed { at: None, .. } => {}
            WorldEvent::OrderPlaced { order, by, from, to, commodity, amount, reward, deadline_s } => {
                // A customer's order becomes an offer of the order template, with the order's own
                // terms. Without the template, or for an order seen before, nothing happens.
                let template = TemplateId::new(ORDER_TEMPLATE);
                let known = self.jobs.values().any(|j| j.order.as_ref().is_some_and(|o| o.order == *order));
                if jc.templates.contains_key(&template) && !known {
                    let id = self.offer(&template, vec![Leg::new(from.clone(), to.clone(), commodity.clone(), *amount)]);
                    self.jobs.get_mut(&id).expect("just made").order = Some(OrderTerms { order: *order, by: by.clone(), reward: *reward, deadline_s: *deadline_s });
                }
            }
            WorldEvent::PlayerJoined | WorldEvent::UnlockBought { .. } | WorldEvent::TrackChanged { .. } | WorldEvent::FlagRaised { .. } | WorldEvent::OrderSettled { .. } => {}
        }
        notes.extend(ended.into_iter().flat_map(|(id, state)| self.end(jc, id, state)));
        notes
    }

    /// The active job a crate belongs to.
    fn job_with(&mut self, c: CrateId) -> Option<&mut Job> {
        self.jobs.values_mut().find(|j| j.state == JobState::Active && j.legs.iter().any(|l| l.crates.contains_key(&c)))
    }

    /// Ends an active job: grade it, pay the wallet once, XP to each participant, raise the
    /// completion flag, release the crates not delivered.
    fn end(&mut self, jc: &JobContent, id: JobId, state: JobState) -> Vec<Outcome> {
        let Some(j) = self.jobs.get_mut(&id) else { return Vec::new() };
        let Some(t) = jc.templates.get(&j.template).map(|t| &t.record) else { return Vec::new() };
        // A customer's order pays its own reward.
        let mut t_order;
        let t = match &j.order {
            Some(o) => {
                t_order = t.clone();
                t_order.reward = o.reward;
                t_order.deadline_s = o.deadline_s;
                &t_order
            }
            None => t,
        };
        // An exam is failed by a crate too damaged, and pays no XP unless it is passed.
        let exam = t.exam.clone();
        let mut state = state;
        let mean_condition = if j.delivered() == 0 { 1.0 } else { j.legs.iter().map(Leg::condition_sum).sum::<f64>() / j.delivered() as f64 };
        if let Some(x) = &exam
            && state == JobState::Completed
            && mean_condition < x.min_condition
        {
            state = JobState::Failed;
        }
        j.state = state;
        let mut g = grade(t, j.delivered(), j.asked(), j.legs.iter().map(Leg::condition_sum).sum());
        if exam.is_some() && state != JobState::Completed {
            g.money = 0;
            g.xp = 0;
        }
        let mut out = Vec::new();
        if g.money != 0 {
            out.push(Outcome::Emit(WorldEvent::TrackChanged { track: TrackId::new(gameplay_core::content::WALLET), delta: g.money, player: None }));
        }
        if g.xp != 0 {
            for p in &j.participants {
                out.push(Outcome::Emit(WorldEvent::TrackChanged { track: t.track.clone(), delta: g.xp, player: Some(*p) }));
            }
        }
        if state == JobState::Completed {
            out.push(Outcome::Emit(WorldEvent::FlagRaised { flag: Flag::new(format!("job_completed:{}", t.id)) }));
        }
        // Standing with the giver: work raises it, a failure lowers it (to the floor, not for good).
        let mut standing = None;
        if let Some(giver) = t.giver.as_ref().and_then(|g| jc.givers.get(g)).map(|g| &g.record) {
            let h = self.history.entry(giver.id.clone()).or_default();
            let delta = if state == JobState::Completed && (g.money > 0 || exam.is_some()) {
                h.completed += 1;
                h.failure_streak = 0;
                giver.gain
            } else {
                h.failure_streak += 1;
                -giver.loss
            };
            if delta != 0 {
                out.push(Outcome::Emit(WorldEvent::TrackChanged { track: giver.standing.clone(), delta, player: None }));
            }
            standing = Some((giver.name.clone(), delta));
        }
        // Passing the exam: the examinee's personal licence track, and honours' standing bonus.
        let mut exam_result = None;
        if let Some(x) = &exam {
            if state == JobState::Completed {
                let landing = j.checks.iter().find_map(|c| match (&c.kind, c.state) {
                    (CheckKind::Land { .. }, CheckState::Done { value, .. }) => Some(value),
                    _ => None,
                });
                let honours = x.honours.as_ref().filter(|h| landing.is_some_and(|v| v <= h.max_touchdown_mps) && j.clock_s.is_some_and(|c| c <= h.within_s));
                if let Some(who) = j.accepted_by {
                    out.push(Outcome::Emit(WorldEvent::TrackChanged { track: x.grants.clone(), delta: 1, player: Some(who) }));
                }
                if let Some(h) = honours {
                    if let Some(giver) = jc.givers.get(&h.giver) {
                        out.push(Outcome::Emit(WorldEvent::TrackChanged { track: giver.record.standing.clone(), delta: h.standing, player: None }));
                    }
                    out.push(Outcome::Emit(WorldEvent::FlagRaised { flag: Flag::new(format!("exam_honours:{}", t.id)) }));
                }
                exam_result = Some(if honours.is_some() { ExamResult::Honours } else { ExamResult::Passed });
            } else {
                exam_result = Some(ExamResult::Failed { retry_fee: x.retry_fee });
            }
        }
        let loose: Vec<CrateId> = j.legs.iter().flat_map(|l| l.crates.iter().filter(|(_, m)| !matches!(m, CrateMark::Delivered { .. })).map(|(c, _)| *c)).collect();
        if !loose.is_empty() {
            out.push(Outcome::ReleaseCrates { crates: loose });
        }
        out.push(Outcome::Ended { job: id, state, grade: Some(g) });
        out.extend(end_notices(t, &j.title_key(t), state, &g, j.delivered(), j.asked(), standing, exam_result).into_iter().map(Outcome::Notice));
        out.extend(settle_order(j, state != JobState::Expired));
        out
    }
}

/// The notices of a finished job (#165): the banner, then the payout itemised (base, share,
/// condition, hazard; the lines add up to the pay), then XP.
fn end_notices(t: &JobTemplate, title: &TextKey, state: JobState, g: &Grade, delivered: u32, asked: u32, standing: Option<(TextKey, i64)>, exam: Option<ExamResult>) -> Vec<Notice> {
    let title = Arg::Key(title.clone());
    let (kind, key) = match (exam, state) {
        (Some(ExamResult::Passed), _) => (NoticeKind::Completed, "notice.exam.passed"),
        (Some(ExamResult::Honours), _) => (NoticeKind::Completed, "notice.exam.honours"),
        (Some(ExamResult::Failed { .. }), _) => (NoticeKind::Failed, "notice.exam.failed"),
        (None, _) => match state {
        JobState::Completed if g.money > 0 => (NoticeKind::Completed, "notice.job.completed"),
        JobState::Expired => (NoticeKind::Failed, "notice.job.expired"),
        _ => (NoticeKind::Failed, "notice.job.failed"),
        },
    };
    let mut out = vec![Notice::new(kind, key).arg("title", title).arg("money", Arg::Number(g.money))];
    if let Some(ExamResult::Failed { retry_fee }) = exam {
        out[0] = out[0].clone().arg("retry", Arg::Number(retry_fee));
    }
    // Running totals, each rounded, so the lines add up exactly to the rounded pay.
    let base = t.reward as f64;
    let steps = [
        ("notice.reward.base", base),
        ("notice.reward.share", base * g.band),
        ("notice.reward.condition", base * g.band * g.condition),
        ("notice.reward.hazard", base * g.band * g.condition * g.hazard),
    ];
    let mut prev = 0i64;
    for (key, total) in steps {
        let total = total.round() as i64;
        let n = total - prev;
        prev = total;
        if n == 0 && key != "notice.reward.base" || g.money == 0 {
            continue;
        }
        let mut line = Notice::new(NoticeKind::Reward, key).arg("n", Arg::Number(n));
        if key == "notice.reward.share" {
            line = line.arg("delivered", Arg::Number(delivered as i64)).arg("asked", Arg::Number(asked as i64));
        }
        out.push(line);
    }
    if g.xp != 0 {
        out.push(Notice::new(NoticeKind::Reward, "notice.reward.xp").arg("n", Arg::Number(g.xp)).arg("track", Arg::Key(gameplay_core::TextKey::new(format!("track.{}.name", t.track)))));
    }
    if let Some((giver, delta)) = standing
        && delta != 0
    {
        let (kind, key) = if delta > 0 { (NoticeKind::Reward, "notice.reward.standing") } else { (NoticeKind::Warning, "notice.reward.standing_lost") };
        out.push(Notice::new(kind, key).arg("n", Arg::Number(delta.abs())).arg("giver", Arg::Key(giver)));
    }
    out
}

/// How an exam ended.
#[derive(Copy, Clone, Debug, PartialEq)]
enum ExamResult {
    Passed,
    Honours,
    Failed { retry_fee: i64 },
}

/// What an order job tells the customers system when it ends: how much arrived, in what
/// condition, whether in time.
fn settle_order(j: &Job, in_time: bool) -> Option<Outcome> {
    let o = j.order.as_ref()?;
    let delivered = j.delivered();
    let condition = if delivered == 0 { 1.0 } else { (j.legs.iter().map(Leg::condition_sum).sum::<f64>() / delivered as f64).clamp(0.0, 1.0) };
    Some(Outcome::Emit(WorldEvent::OrderSettled { order: o.order, delivered, asked: j.asked(), condition, in_time }))
}

/// Templates whose `follow_up` is `id`.
fn prerequisites<'a>(jc: &'a JobContent, id: &'a TemplateId) -> impl Iterator<Item = &'a TemplateId> + 'a {
    jc.templates.values().filter(move |t| t.record.follow_up.as_ref() == Some(id)).map(|t| &t.record.id)
}

fn completed(progress: &Progress, t: &TemplateId) -> bool {
    progress.has_flag(&Flag::new(format!("job_completed:{t}")))
}
