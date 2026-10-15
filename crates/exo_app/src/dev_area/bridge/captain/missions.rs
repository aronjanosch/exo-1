//! The captain's missions display: the crew's real jobs from `Gameplay` as cards through
//! `crew_core::mission_view` (filters, the issuer only for the captain), and the captain-only
//! actions accept, set active (the tracked job) and abandon. The offers are the givers' fixed
//! jobs `Gameplay::refresh_offers` keeps on offer, so the dev area needs no counter of its own.
use super::*;
use crate::displays::{DisplayAction, Panels};
use crate::gameplay::HOST;
use crew_core::mission_view::{self, Objective};
use crew_core::{Filter, MissionCommand, MissionRow};
use display_core::{button, column, label, row, space, title, Panel, Tone};
use jobs_core::{CheckKind, JobId, JobState};

pub const ID: PanelId = PanelId::new("captain_missions");
pub const NAME: &str = "Missions";
/// Cards in the list at most.
pub const ROWS: usize = 7;

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct MissionsUi {
    pub filter: Filter,
    pub open: Option<JobId>,
}

impl Default for MissionsUi {
    fn default() -> Self {
        MissionsUi { filter: Filter::Offered, open: None }
    }
}

pub fn plugin(app: &mut App) {
    app.world_mut().resource_mut::<DevAreaDevices>().add(console(ID, NAME, -3.45, [0.3, 0.3, 0.5]));
    app.init_resource::<MissionsUi>();
    app.add_systems(FixedUpdate, (read_clicks, write_panel).chain().in_set(crate::phases::Fx::Effects).after(super::overview::read_clicks));
}

pub fn filter_name(f: Filter) -> &'static str {
    match f {
        Filter::Offered => "OFFERED",
        Filter::Active => "ACTIVE",
        Filter::Completed => "DONE",
        Filter::Abandoned => "DROPPED",
        Filter::Failed => "FAILED",
    }
}

fn objective_text(gp: &Gameplay, o: &Objective) -> String {
    match o {
        Objective::Deliver { commodity, to } => {
            let c = gp.kernel.commodities.get(commodity).map(|c| gp.text(&c.record.name)).unwrap_or_else(|| commodity.to_string());
            let l = gp.kernel.locations.get(to).map(|l| gp.text(&l.record.name)).unwrap_or_else(|| to.to_string());
            format!("Deliver {c} to {l}")
        }
        Objective::Check(CheckKind::TakeOff) => "Take off".into(),
        Objective::Check(CheckKind::ReachPad { at }) => format!("Reach the pad of {at}"),
        Objective::Check(CheckKind::Land { at, .. }) => format!("Land at {at}"),
    }
}

/// The rows of the current filter.
pub fn rows(gp: &Gameplay, ui: &MissionsUi, role: Role) -> Vec<MissionRow> {
    mission_view::rows(&gp.jobs, &gp.jobs_content, role, ui.filter, gp.tracked)
}

pub fn panel(gp: &Gameplay, ui: &MissionsUi, role: Role) -> Panel {
    let rs = rows(gp, ui, role);
    let mut left = vec![title("MISSIONS").fixed(40.0)];
    left.push(row(6.0, Filter::ALL.iter().map(|f| button(pick(filter_name(*f), *f == ui.filter), format!("filter:{}", filter_name(*f)))).collect()).fixed(40.0));
    if rs.is_empty() {
        left.push(label("(none)").tone(Tone::Dim).fixed(26.0));
    }
    for r in rs.iter().take(ROWS) {
        let mark = if r.tracked { "* " } else { "" };
        let text = format!("{mark}{}   {}/{}   {} cr", gp.text(&r.title), r.done, r.of, r.reward);
        left.push(button(pick(&text, ui.open == Some(r.job)), format!("open:{}", r.job.0)).fixed(40.0));
    }
    left.push(space());
    left.push(label(format!("{} active of {} at most", gp.jobs.active().count(), jobs_core::MAX_ACTIVE)).tone(Tone::Dim).fixed(24.0));

    // The open card.
    let card = ui.open.and_then(|id| gp.jobs.get(id)).and_then(|j| mission_view::row(j, &gp.jobs_content, role, gp.tracked));
    let mut right = vec![];
    match &card {
        None => right.push(label("Pick a mission on the left.").tone(Tone::Dim).fixed(30.0)),
        Some(r) => {
            right.push(title(gp.text(&r.title)).fixed(40.0));
            right.push(label(format!("{:?}{}", r.state, if r.tracked { "  (tracked)" } else { "" })).tone(if r.state == JobState::Active { Tone::Good } else { Tone::Normal }).fixed(26.0));
            right.push(label(format!("Reward  {} credits", r.reward)).fixed(26.0));
            right.push(label(match r.time_left_s {
                Some(t) => format!("Time left  {:.0} s", t),
                None => "Time left  - (starts at the first pickup)".into(),
            }).fixed(26.0));
            // The issuer only on the captain's card.
            right.push(match &r.issuer {
                Some(k) => label(format!("Issued by  {}", gp.text(k))).fixed(26.0),
                None => label("Issued by  (captain only)").tone(Tone::Dim).fixed(26.0),
            });
            right.push(label("OBJECTIVES").tone(Tone::Dim).fixed(22.0));
            for o in &r.objectives {
                right.push(label(format!("{}  {}/{}", objective_text(gp, &o.what), o.done, o.of)).tone(if o.done >= o.of { Tone::Good } else { Tone::Normal }).fixed(26.0));
            }
            right.push(space());
            right.push(row(8.0, vec![button("ACCEPT", "accept"), button("SET ACTIVE", "track"), button("ABANDON", "abandon")]).fixed(48.0));
        }
    }
    if card.is_none() {
        right.push(space());
    }
    Panel::new(PANEL_PX[0], PANEL_PX[1], row(28.0, vec![column(6.0, left).flex(1.1), column(6.0, right).flex(1.0)]))
}

pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut ui: ResMut<MissionsUi>, mut gp: ResMut<Gameplay>, role: Res<DevRole>) {
    for c in clicks.read().filter(|c| c.panel == ID) {
        let (head, arg) = c.action.split_once(':').unwrap_or((c.action.as_str(), ""));
        let job = ui.open;
        let res = match (head, job) {
            ("filter", _) => {
                ui.filter = Filter::ALL.into_iter().find(|f| filter_name(*f) == arg).unwrap_or(ui.filter);
                continue;
            }
            ("open", _) => {
                ui.open = arg.parse::<u64>().ok().map(JobId);
                continue;
            }
            ("accept", Some(j)) => mission_view::accept(role.0, &gp.jobs, j),
            ("track", Some(j)) => mission_view::set_active(role.0, &gp.jobs, j),
            ("abandon", Some(j)) => mission_view::abandon(role.0, &gp.jobs, j),
            _ => {
                warn!("captain missions: unknown action {c:?}");
                continue;
            }
        };
        match res {
            Ok(MissionCommand::Apply(e)) => gp.push_job(HOST, e),
            Ok(MissionCommand::Track(j)) => gp.tracked = Some(j),
            Err(r) => refuse(&mut gp, &r),
        }
    }
}

pub fn write_panel(gp: Res<Gameplay>, ui: Res<MissionsUi>, role: Res<DevRole>, mut panels: ResMut<Panels>) {
    panels.set(ID, panel(&gp, &ui, role.0));
}
