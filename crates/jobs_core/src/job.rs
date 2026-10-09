//! Jobs as a state machine over domain events (#123): offered → active → completed, expired or
//! abandoned. Each deliver leg is a reducer over the crate events of its own crates.
use std::collections::{BTreeMap, BTreeSet};

use gameplay_core::notice::{Arg, Notice, NoticeKind};
use gameplay_core::save::{Envelope, SaveError};
use gameplay_core::{ClientId, CommodityId, Content, CrateId, Event, Flag, LocationId, Progress, TrackId, WorldEvent};
use serde::{Deserialize, Serialize};

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
}

impl Job {
    pub fn delivered(&self) -> u32 {
        self.legs.iter().map(Leg::delivered).sum()
    }

    pub fn asked(&self) -> u32 {
        self.legs.iter().map(|l| l.amount).sum()
    }

    /// Seconds left before the deadline; none without a deadline or before the first pickup.
    pub fn time_left(&self, t: &JobTemplate) -> Option<f64> {
        Some((t.deadline_s? - self.clock_s?).max(0.0))
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
}

/// The crew's offers and jobs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Jobs {
    next_id: u64,
    jobs: BTreeMap<JobId, Job>,
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
        let id = JobId(self.next_id);
        self.next_id += 1;
        self.jobs.insert(id, Job { id, template: template.clone(), state: JobState::Offered, legs, accepted_by: None, participants: BTreeSet::new(), clock_s: None });
        id
    }

    /// Makes an offer from a template that names everything: fixed places, one commodity, one
    /// amount (targeted and story jobs). None if it leaves a choice to the board.
    pub fn offer_fixed(&mut self, t: &JobTemplate) -> Option<JobId> {
        let legs = t
            .objectives
            .iter()
            .map(|o| match o {
                ObjectiveSpec::Deliver { from: PlaceSpec::Location(a), to: PlaceSpec::Location(b), commodity: CommoditySpec::OneOf(pool), amount } if pool.len() == 1 && amount[0] == amount[1] => {
                    Some(Leg::new(a.clone(), b.clone(), pool[0].clone(), amount[0]))
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        Some(self.offer(&t.id, legs))
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
                j.state = JobState::Active;
                j.accepted_by = Some(by);
                j.participants.insert(by);
                let mut out = vec![Outcome::Notice(Notice::new(NoticeKind::Accepted, "notice.job.accepted").arg("title", Arg::Key(t.title.clone())))];
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
                    out.push(Outcome::Notice(Notice::new(NoticeKind::Warning, "notice.job.abandoned").arg("title", Arg::Key(t.record.title.clone()))));
                }
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
        let title = |j: &Job| jc.templates.get(&j.template).map(|t| Arg::Key(t.record.title.clone()));
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
                        if j.legs.iter().all(Leg::resolved) {
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
                        if j.legs.iter().all(Leg::resolved) {
                            ended.push((j.id, JobState::Completed));
                        }
                    }
                }
            }
            WorldEvent::TimePassed { dt } => {
                for j in self.jobs.values_mut().filter(|j| j.state == JobState::Active) {
                    let Some(clock) = j.clock_s.as_mut() else { continue };
                    *clock += dt;
                    let deadline = jc.templates.get(&j.template).and_then(|t| t.record.deadline_s);
                    if deadline.is_some_and(|d| *clock >= d) {
                        ended.push((j.id, JobState::Expired));
                    }
                }
            }
            WorldEvent::PlayerJoined | WorldEvent::UnlockBought { .. } | WorldEvent::TrackChanged { .. } | WorldEvent::FlagRaised { .. } => {}
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
        j.state = state;
        let g = grade(t, j.delivered(), j.asked(), j.legs.iter().map(Leg::condition_sum).sum());
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
        let loose: Vec<CrateId> = j.legs.iter().flat_map(|l| l.crates.iter().filter(|(_, m)| !matches!(m, CrateMark::Delivered { .. })).map(|(c, _)| *c)).collect();
        if !loose.is_empty() {
            out.push(Outcome::ReleaseCrates { crates: loose });
        }
        out.push(Outcome::Ended { job: id, state, grade: Some(g) });
        out.extend(end_notices(t, state, &g, j.delivered(), j.asked()).into_iter().map(Outcome::Notice));
        out
    }
}

/// The notices of a finished job (#165): the banner, then the payout itemised (base, share,
/// condition, hazard; the lines add up to the pay), then XP.
fn end_notices(t: &JobTemplate, state: JobState, g: &Grade, delivered: u32, asked: u32) -> Vec<Notice> {
    let title = Arg::Key(t.title.clone());
    let (kind, key) = match state {
        JobState::Completed if g.money > 0 => (NoticeKind::Completed, "notice.job.completed"),
        JobState::Expired => (NoticeKind::Failed, "notice.job.expired"),
        _ => (NoticeKind::Failed, "notice.job.failed"),
    };
    let mut out = vec![Notice::new(kind, key).arg("title", title).arg("money", Arg::Number(g.money))];
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
    out
}
