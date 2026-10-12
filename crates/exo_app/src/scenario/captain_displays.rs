//! #288: the captain's displays in the dev area, through `Controls` and the pointer only (plus
//! `DevRole` flipped by the script to play a non-captain). Overview: alert to yellow, a crew order
//! (the HUD line shows it), an order for the tactical slot (the demo bot obeys), a special order,
//! a budget release, orders cleared. Missions: accept an offer, set it active, an engineer's
//! abandon is refused. Ship state: fire and breach forced, the bars change. Starmap: a
//! destination. Bot priorities: fire to max and the turret to low turn the demo bot to the fire.
//! Run with `--start=dev_area`.
use super::dev_area::{active, hold_walk_until, stand_until_still, tap_until};
use super::*;
use crate::dev_area::captain::{self, bots, missions, overview, ship_state, starmap, CrewState, DevRole, CONSOLE_Y};
use crate::dev_area::{DevArea, CAPTAIN};
use crate::displays::{overlay_point_of, ActiveDisplay, DisplayPanel};
use crate::gameplay::{Gameplay, HOST};
use crate::interact::Interaction;
use crew_core::orders::OrderKind;
use crew_core::status::ShipSystem;
use crew_core::{AlertLevel, Role, Target};
use display_core::PanelId;
use jobs_core::{JobId, JobState};

fn display_of(w: &mut World, id: &PanelId) -> Option<Entity> {
    w.query::<(Entity, &DisplayPanel)>().iter(w).find(|(_, d)| &d.id == id).map(|(e, _)| e)
}

fn crew(w: &World) -> &CrewState {
    w.resource::<CrewState>()
}

fn gp(w: &World) -> &Gameplay {
    w.resource::<Gameplay>()
}

fn logged(w: &World, line: &str) -> bool {
    gp(w).log.iter().any(|l| l == line)
}

/// Walk to a console (from 3 m south of it), F, and its display is in use.
fn go_to(name: &'static str, id: PanelId, title: &'static str, x: f64) -> Vec<Step> {
    let id2 = id.clone();
    let id3 = id.clone();
    vec![
        Box::new(move |w, c| {
            if active(w).using().is_some() {
                // Leave the display in use first.
                if c.t == 0.0 {
                    tap(w, KeyCode::KeyF);
                }
                return c.t > 0.3;
            }
            begin(w, c, name);
            let pl = planet(w);
            let (at, _) = w.resource::<DevArea>().ground(&pl, CAPTAIN.point([x, CONSOLE_Y - 3.0]), 1.0).expect("dev area");
            place_walker(w, at);
            let e = display_of(w, &id).expect("the console is spawned");
            let target = w.get::<DisplayPanel>(e).unwrap().centre;
            face_towards(w, target);
            true
        }),
        hold_walk_until(&[KeyCode::KeyW], 10.0, move |w| w.resource::<Interaction>().prompt.contains(title)),
        stand_until_still(),
        tap_until(KeyCode::KeyF, name, 1.0, move |w| {
            let e = display_of(w, &id2);
            e.is_some() && active(w).using() == e && w.resource::<Controls>().free_cursor
        }),
        Box::new(move |w, c| {
            let e = display_of(w, &id3);
            end(w, c, format!("using {:?}", e.map(|e| e.index())));
            true
        }),
    ]
}

/// Click a widget of a panel (overlay) and wait until `done` (at most `limit` s).
fn click(panel: PanelId, action: String, what: String, limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            let Some(at) = overlay_point_of(w, &panel, &action, None) else {
                check(c, false, format!("{what}: no widget '{action}' on {panel}"));
                return true;
            };
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
            return false;
        }
        if done(w) {
            check(c, true, format!("{what} ({:.2} s)", c.t));
            return true;
        }
        if c.t >= limit {
            check(c, false, format!("{what}: not within {limit} s"));
            return true;
        }
        false
    })
}

/// A click whose outcome is a UI state (checked silently).
fn press(panel: PanelId, action: &'static str, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            let at = overlay_point_of(w, &panel, action, None).unwrap_or_else(|| panic!("no widget '{action}' on {panel}"));
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
            return false;
        }
        if done(w) || c.t >= 1.0 {
            check(c, done(w), format!("click '{action}' on {panel}"));
            return true;
        }
        false
    })
}

/// An offer the host may accept: no exam, its condition holds.
fn acceptable_offer(w: &World) -> Option<JobId> {
    let g = gp(w);
    g.jobs.all().filter(|j| j.state == JobState::Offered).find(|j| {
        g.jobs_content.templates.get(&j.template).is_some_and(|t| t.record.exam.is_none() && t.record.available.as_ref().is_none_or(|c| c.holds(&g.kernel, &g.progress, Some(HOST))))
    }).map(|j| j.id)
}

pub(super) fn captain_displays_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    let ov = overview::ID;
    s.push(Box::new(|w, c| {
        begin(w, c, "captain: the consoles stand");
        let ids = [overview::ID, missions::ID, ship_state::ID, starmap::ID, captain::comms::ID, bots::ID];
        let n = ids.iter().filter(|id| display_of(w, id).is_some()).count();
        check(c, n == ids.len(), format!("captain: {n} of {} captain displays in the captain zone", ids.len()));
        check(c, w.resource::<DevRole>().0 == Role::Captain, "captain: the dev area plays the captain".into());
        end(w, c, format!("{n} displays"));
        true
    }));

    // Overview.
    s.extend(go_to("captain: walk to the overview", ov.clone(), overview::NAME, -21.0));
    s.push(click(ov.clone(), "alert:yellow".into(), "captain: the alert goes to yellow".into(), 1.0, |w| crew(w).alert.level() == AlertLevel::Yellow && logged(w, "captain: notice.alert.yellow")));
    s.push(press(ov.clone(), "to:0", |w| w.resource::<overview::OverviewUi>().to == crew_core::Addressee::Crew));
    s.push(press(ov.clone(), "kind:stay", |w| w.resource::<overview::OverviewUi>().kind == OrderKind::Stay));
    s.push(click(ov.clone(), "give".into(), "captain: a crew order 'stay' is given and told".into(), 1.0, |w| {
        crew(w).orders.crew().is_some_and(|o| o.kind == OrderKind::Stay) && logged(w, "captain: notice.captain.order")
    }));
    s.push(Box::new(|w, c| {
        let line = captain::hud_order_line(gp(w), crew(w), w.resource::<DevRole>().0);
        check(c, line == "ORDER  Stay where you are", format!("captain: the HUD order line reads {line:?}"));
        true
    }));
    s.push(press(ov.clone(), "to:3", |w| w.resource::<overview::OverviewUi>().to == crew_core::Addressee::Slot(Role::Tactical)));
    s.push(press(ov.clone(), "kind:operate", |w| w.resource::<overview::OverviewUi>().kind == OrderKind::Operate));
    s.push(press(ov.clone(), "tgt_next", |w| w.resource::<overview::OverviewUi>().target == 1));
    s.push(click(ov.clone(), "give".into(), "captain: the tactical slot is ordered to the port turret".into(), 1.0, |w| {
        crew(w).orders.for_role(Role::Tactical).is_some_and(|o| o.kind == OrderKind::Operate && o.target == Some(Target::Station(crew_core::StationId::new(bots::TURRET_STATION))))
    }));
    s.push(Box::new(|w, c| {
        // The demo bot (tactical) takes the ordered turret over the table.
        let d = w.resource::<bots::BotDemo>().clone();
        let by_order = d.current.as_ref().is_some_and(|c| c.level.is_none() && c.score >= crew_core::priorities::ORDER_SCORE);
        if (d.doing() == "turret" && by_order) || c.t > 1.0 {
            check(c, d.doing() == "turret" && by_order, format!("captain: the demo bot obeys the order: {} ({:?})", d.doing(), d.current.map(|c| c.score)));
            return true;
        }
        false
    }));
    s.push(click(ov.clone(), "phrase:0".into(), "captain: a special order slot takes a canned phrase".into(), 1.0, |w| crew(w).orders.custom()[0].as_deref() == Some(overview::PHRASES[0][0])));
    s.push(click(ov.clone(), "rel:engineer".into(), "captain: 100 credits released to the engineer".into(), 1.0, |w| crew(w).budget.remaining(Role::Engineer) == 100));
    s.push(Box::new(|w, c| {
        let pool = crew(w).budget.pool(gp(w).progress.wallet());
        check(c, pool.total - pool.available == 100, format!("captain: the pool shows total {} and available {}", pool.total, pool.available));
        true
    }));
    s.push(Box::new(|w, _| {
        let at = overlay_point_of(w, &overview::ID, "give", None);
        w.resource_mut::<Controls>().pointer = at;
        true
    }));
    s.push(wait(0.2));
    s.push(shot_step("overview"));
    s.push(click(ov.clone(), "clear".into(), "captain: the tactical order is cleared".into(), 1.0, |w| crew(w).orders.for_role(Role::Tactical).is_some_and(|o| o.kind == OrderKind::Stay)));
    s.push(press(ov.clone(), "to:0", |w| w.resource::<overview::OverviewUi>().to == crew_core::Addressee::Crew));
    s.push(click(ov.clone(), "clear".into(), "captain: the crew order is cleared".into(), 1.0, |w| crew(w).orders.crew().is_none()));

    // Missions.
    let mi = missions::ID;
    s.extend(go_to("captain: walk to the missions", mi.clone(), missions::NAME, -14.0));
    s.push(Box::new(|w, c| {
        let offers = gp(w).jobs.all().filter(|j| j.state == JobState::Offered).count();
        let rows = missions::rows(gp(w), w.resource::<missions::MissionsUi>(), Role::Captain).len();
        let job = acceptable_offer(w);
        check(c, offers >= 2 && rows == offers && job.is_some(), format!("captain: {offers} offers on the board, {rows} rows, one to accept: {job:?}"));
        c.v.insert("job", job.map_or(-1.0, |j| j.0 as f64));
        true
    }));
    s.push(Box::new(|w, c| {
        let job = JobId(c.v["job"] as u64);
        if c.t == 0.0 {
            let at = overlay_point_of(w, &missions::ID, &format!("open:{}", job.0), None).expect("the offer's row");
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
            return false;
        }
        let ok = w.resource::<missions::MissionsUi>().open == Some(job);
        if ok || c.t > 1.0 {
            let issuer = missions::rows(gp(w), w.resource::<missions::MissionsUi>(), Role::Captain).into_iter().find(|r| r.job == job).and_then(|r| r.issuer);
            check(c, ok && issuer.is_some(), format!("captain: the card opens, the captain sees the issuer {issuer:?}"));
            return true;
        }
        false
    }));
    s.push(Box::new(|w, c| {
        let job = JobId(c.v["job"] as u64);
        if c.t == 0.0 {
            let at = overlay_point_of(w, &missions::ID, "accept", None).expect("accept");
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
            return false;
        }
        let state = gp(w).jobs.get(job).map(|j| j.state);
        if state == Some(JobState::Active) || c.t > 2.0 {
            check(c, state == Some(JobState::Active), format!("captain: accept takes the mission ({state:?}, {:.2} s)", c.t));
            return true;
        }
        false
    }));
    s.push(Box::new(|w, c| {
        let job = JobId(c.v["job"] as u64);
        if c.t == 0.0 {
            let at = overlay_point_of(w, &missions::ID, "track", None).expect("track");
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
            return false;
        }
        if gp(w).tracked == Some(job) || c.t > 1.0 {
            check(c, gp(w).tracked == Some(job), format!("captain: set active tracks the mission ({:?})", gp(w).tracked));
            return true;
        }
        false
    }));
    s.push(press(mi.clone(), "filter:ACTIVE", |w| w.resource::<missions::MissionsUi>().filter == crew_core::Filter::Active));
    s.push(wait(0.2));
    s.push(shot_step("missions"));
    // An engineer presses abandon: refused, the mission stays.
    s.push(Box::new(|w, _| {
        w.resource_mut::<DevRole>().0 = Role::Engineer;
        true
    }));
    s.push(Box::new(|w, c| {
        let job = JobId(c.v["job"] as u64);
        if c.t == 0.0 {
            let at = overlay_point_of(w, &missions::ID, "abandon", None).expect("abandon");
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
            return false;
        }
        let refused = logged(w, "captain: refused refusal.not_captain");
        if refused || c.t > 1.0 {
            let state = gp(w).jobs.get(job).map(|j| j.state);
            let issuer = missions::rows(gp(w), w.resource::<missions::MissionsUi>(), Role::Engineer).into_iter().find(|r| r.job == job).and_then(|r| r.issuer);
            check(c, refused && state == Some(JobState::Active) && issuer.is_none(), format!("captain: an engineer's abandon is refused (refusal noted {refused}, mission {state:?}, issuer hidden {})", issuer.is_none()));
            return true;
        }
        false
    }));
    s.push(Box::new(|w, _| {
        w.resource_mut::<DevRole>().0 = Role::Captain;
        true
    }));

    // Ship state.
    let st = ship_state::ID;
    s.extend(go_to("captain: walk to the ship state", st.clone(), ship_state::NAME, -7.0));
    s.push(press(st.clone(), "room_next", |w| w.resource::<ship_state::ShipStateUi>().room == 1));
    s.push(click(st.clone(), "fire".into(), "captain: fire forced in the engine room".into(), 1.0, |w| crew(w).ship.status().system(ShipSystem::Engineering).fire));
    s.push(click(st.clone(), "breach".into(), "captain: breach forced in the engine room".into(), 1.0, |w| crew(w).ship.status().rooms.iter().any(|r| r.breach)));
    s.push(Box::new(|w, c| {
        let ship = crew(w).ship.clone();
        let mut calm = ship.clone();
        calm.clear();
        let (now, before) = (ship.status(), calm.status());
        let eng = (now.system(ShipSystem::Engineering).condition, before.system(ShipSystem::Engineering).condition);
        let hull = before.hull.value - now.hull.value;
        let panel_fire = w.resource::<crate::displays::Panels>().get(&ship_state::ID).is_some_and(|p| {
            p.layout().iter().any(|x| matches!(x.widget, display_core::Widget::Bar { label, .. } if label.starts_with("ENGINEERING") && label.ends_with("FIRE")))
        });
        check(c, eng.0.zip(eng.1).is_some_and(|(a, b)| b - a == 6 || a == 0) && (hull - 15.0).abs() < 1e-9 && panel_fire,
            format!("captain: the bars change: engineering {:?} -> {:?}, hull -{hull:.1}, the panel marks the fire {panel_fire}", eng.1, eng.0));
        true
    }));
    s.push(wait(0.2));
    s.push(shot_step("ship-state"));
    s.push(click(st.clone(), "clear".into(), "captain: the forced states are cleared".into(), 1.0, |w| crew(w).ship.forced().is_empty()));

    // Starmap.
    let sm = starmap::ID;
    s.extend(go_to("captain: walk to the starmap", sm.clone(), starmap::NAME, 0.0));
    s.push(click(sm.clone(), "dest:dev_area".into(), "captain: the destination is set".into(), 1.0, |w| w.resource::<starmap::Destination>().0.as_deref() == Some("dev_area") && logged(w, "captain: notice.captain.destination")));

    // Bot priorities.
    let bp = bots::ID;
    s.extend(go_to("captain: walk to the bot priorities", bp.clone(), bots::NAME, 14.0));
    s.push(Box::new(|w, c| {
        let d = w.resource::<bots::BotDemo>().clone();
        if d.doing() == "turret" || c.t > 1.0 {
            check(c, d.doing() == "turret", format!("captain: with the default table the demo bot takes the turret ({}, {:?})", d.doing(), d.current.map(|c| c.score)));
            return true;
        }
        false
    }));
    s.push(click(bp.clone(), "lvl:extinguish_fire:MAX".into(), "captain: fire set to max".into(), 1.0, |w| crew(w).table.page(Role::Tactical).is_some_and(|p| p.base(crew_core::TaskKind::ExtinguishFire) == crew_core::Level::Max)));
    s.push(click(bp.clone(), "lvl:operate_turret:LOW".into(), "captain: turret set to low".into(), 1.0, |w| crew(w).table.page(Role::Tactical).is_some_and(|p| p.base(crew_core::TaskKind::OperateTurret) == crew_core::Level::Low)));
    s.push(Box::new(|w, c| {
        let d = w.resource::<bots::BotDemo>().clone();
        let near = bots::distance_to(&d, bots::FIRE_AT);
        if (d.doing() == "fire" && near < 2.0) || c.t > 15.0 {
            check(c, d.doing() == "fire" && near < 2.0, format!("captain: the demo bot turns to the fire and gets there ({}, {near:.1} m, {:.1} s)", d.doing(), c.t));
            return true;
        }
        false
    }));
    s.push(wait(0.2));
    s.push(shot_step("bots"));
    s.push(tap_until(KeyCode::KeyF, "captain: F leaves the console", 1.0, |w| active(w) == ActiveDisplay::Idle));
}
