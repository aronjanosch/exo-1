//! The captain's mission display over `jobs_core`. Taken from PULSAR's missions tab: a list with
//! objective rows and the reward, filters for active, completed and abandoned missions, and an
//! abandon button only the captain has; showing the issuer only to the captain is ours
//! (`captain.md`, "Issuer is captain-only").
//!
//! This wraps `jobs_core` without changing it: rows are read from `Jobs`, and the captain-only
//! actions check the role and the job's state, then hand back the `JobEvent` for the glue to send
//! (or the job to track). `jobs_core` checks again when it applies the event.
use gameplay_core::{CommodityId, LocationId, TextKey};
use jobs_core::{CheckKind, CheckState, Job, JobContent, JobEvent, JobId, JobState, Jobs, MAX_ACTIVE};

use crate::status::MissionLine;
use crate::{Refusal, Role, captain_only};

/// Which missions a list shows.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Filter {
    Offered,
    Active,
    Completed,
    Abandoned,
    /// Expired or failed.
    Failed,
}

impl Filter {
    pub const ALL: [Filter; 5] = [Filter::Offered, Filter::Active, Filter::Completed, Filter::Abandoned, Filter::Failed];

    pub fn holds(self, s: JobState) -> bool {
        match self {
            Filter::Offered => s == JobState::Offered,
            Filter::Active => s == JobState::Active,
            Filter::Completed => s == JobState::Completed,
            Filter::Abandoned => s == JobState::Abandoned,
            Filter::Failed => matches!(s, JobState::Expired | JobState::Failed),
        }
    }
}

/// What one objective asks.
#[derive(Clone, Debug, PartialEq)]
pub enum Objective {
    Deliver { commodity: CommodityId, to: LocationId },
    Check(CheckKind),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveRow {
    pub what: Objective,
    pub done: u32,
    pub of: u32,
}

/// One mission card.
#[derive(Clone, Debug, PartialEq)]
pub struct MissionRow {
    pub job: JobId,
    pub title: TextKey,
    pub state: JobState,
    /// Money at full grade.
    pub reward: i64,
    /// None without a deadline or before the first pickup.
    pub time_left_s: Option<f64>,
    pub objectives: Vec<ObjectiveRow>,
    /// Sums over the objectives.
    pub done: u32,
    pub of: u32,
    /// The giver's name; only on the captain's rows.
    pub issuer: Option<TextKey>,
    /// The mission the captain set active (`set_active`).
    pub tracked: bool,
}

/// What the glue does after a captain's action.
#[derive(Clone, Debug, PartialEq)]
pub enum MissionCommand {
    /// Send this event to `Jobs::apply_job` as the captain.
    Apply(JobEvent),
    /// Make this the tracked mission (map pins, HUD line, status line).
    Track(JobId),
}

/// One job as a card for `viewer`; none if its template is unknown.
pub fn row(job: &Job, jc: &JobContent, viewer: Role, tracked: Option<JobId>) -> Option<MissionRow> {
    let t = &jc.templates.get(&job.template)?.record;
    let mut objectives: Vec<ObjectiveRow> = job.legs.iter().map(|l| ObjectiveRow { what: Objective::Deliver { commodity: l.commodity.clone(), to: l.to.clone() }, done: l.delivered(), of: l.amount }).collect();
    objectives.extend(job.checks.iter().map(|c| ObjectiveRow { what: Objective::Check(c.kind.clone()), done: matches!(c.state, CheckState::Done { .. }) as u32, of: 1 }));
    let issuer = (viewer == Role::Captain)
        .then(|| match &job.order {
            Some(o) => Some(TextKey::new(format!("customer.{}.name", o.by))),
            None => t.giver.as_ref().and_then(|g| jc.givers.get(g)).map(|g| g.record.name.clone()),
        })
        .flatten();
    Some(MissionRow {
        job: job.id,
        title: job.title_key(t),
        state: job.state,
        reward: job.order.as_ref().map_or(t.reward, |o| o.reward),
        time_left_s: job.time_left(t),
        done: objectives.iter().map(|o| o.done).sum(),
        of: objectives.iter().map(|o| o.of).sum(),
        objectives,
        issuer,
        tracked: tracked == Some(job.id),
    })
}

/// The cards of one filter, in the order the jobs were made.
pub fn rows(jobs: &Jobs, jc: &JobContent, viewer: Role, filter: Filter, tracked: Option<JobId>) -> Vec<MissionRow> {
    jobs.all().filter(|j| filter.holds(j.state)).filter_map(|j| row(j, jc, viewer, tracked)).collect()
}

/// The status display's mission line.
pub fn line(r: &MissionRow) -> MissionLine {
    MissionLine { title: r.title.clone(), done: r.done, of: r.of }
}

fn state(jobs: &Jobs, id: JobId) -> Result<JobState, Refusal> {
    jobs.get(id).map(|j| j.state).ok_or(Refusal::UnknownJob)
}

/// The captain takes an offer for the crew.
pub fn accept(by: Role, jobs: &Jobs, id: JobId) -> Result<MissionCommand, Refusal> {
    captain_only(by)?;
    if state(jobs, id)? != JobState::Offered {
        return Err(Refusal::NotOffered);
    }
    if jobs.active().count() >= MAX_ACTIVE {
        return Err(Refusal::TooManyActive);
    }
    Ok(MissionCommand::Apply(JobEvent::OfferAccepted { job: id }))
}

/// The captain makes an active mission the tracked one.
pub fn set_active(by: Role, jobs: &Jobs, id: JobId) -> Result<MissionCommand, Refusal> {
    captain_only(by)?;
    if state(jobs, id)? != JobState::Active {
        return Err(Refusal::NotActive);
    }
    Ok(MissionCommand::Track(id))
}

/// The captain drops an active mission.
pub fn abandon(by: Role, jobs: &Jobs, id: JobId) -> Result<MissionCommand, Refusal> {
    captain_only(by)?;
    if state(jobs, id)? != JobState::Active {
        return Err(Refusal::NotActive);
    }
    Ok(MissionCommand::Apply(JobEvent::JobAbandoned { job: id }))
}
