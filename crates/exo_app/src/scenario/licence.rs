//! #169, the flight licence, headless: without the licence the seat refuses (with a pointer to the
//! exam), the exam is taken at the flight school's counter for a fee, the ship is lent for it, the
//! exam's events (take-off, pad reached, landing) are set by test hooks (the exam reads only
//! these events, never the flight model), the crate is set down on the Drip Rock pad, and the
//! pilot has the licence: the seat works, the exam is closed to them, the courier office gives a
//! standing bonus for honours.
use super::deliver::{crates_above, goods_of, gp, prompt, show_offer, walker_on_pad};
use super::*;
use crate::gameplay::{Gameplay, HOST};
use gameplay_core::{LocationId, TrackId, WorldEvent};
use jobs_core::JobState;

fn licence(w: &World) -> i64 {
    let g = gp(w);
    g.progress.value(&g.kernel, &TrackId::new("licence_flight"), Some(HOST)).unwrap_or(0)
}

fn seated(w: &mut World) -> bool {
    with_player(w, |p| p.seated)
}

fn exam_state(w: &World) -> Option<JobState> {
    gp(w).jobs.all().filter(|j| j.template.as_str() == "flight_exam" && j.accepted_by.is_some()).last().map(|j| j.state)
}

pub fn licence_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, c| {
        crate::scenario::cargo::clear_crates(w);
        begin(w, c, "licence: the seat refuses without the licence");
        check(c, licence(w) == 0 && !gp(w).may_pilot(HOST), "licence: a new player has no licence".into());
        put_at_seat(w);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let p = prompt(w);
        check(c, p.contains("sit") && p.contains("licence") && p.contains("Skyhook"), format!("licence: the prompt at the seat points at the exam ('{p}')"));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let sat = seated(w);
        check(c, !sat, "licence: the seat refuses".into());
        check(c, gp(w).notices.len() > 0 || gp(w).shown.iter().any(|l| l.key == "notice.licence.needed"), "licence: and says why".into());
        // Riding along works: the walker is in the cabin and may walk about (no licence needed).
        let inside = with_player(w, |p| p.ship.is_some());
        check(c, inside, "licence: riding along in the cabin is allowed".into());
        end(w, c, String::new());
        true
    }));
    // Money for the fee: a few courier jobs in play, a test hook here.
    s.push(Box::new(|w, c| {
        begin(w, c, "licence: take the exam at the school's counter");
        w.resource_mut::<Gameplay>().push_world(HOST, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 200, player: None });
        walker_on_pad(w, "skyhook_school");
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        c.v.insert("wallet0", gp(w).progress.wallet() as f64);
        let p = prompt(w);
        check(c, p.contains("talk to") && p.contains("Skyhook"), format!("licence: the prompt names the school ('{p}')"));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(show_offer("flight_exam"));
    s.push(Box::new(|w, c| {
        let text = w.resource_mut::<Gameplay>().panel_text("[F] take");
        check(c, text.contains("Flight licence exam") && text.contains("Skyhook"), format!("licence: the counter shows the exam ({})", text.replace('\n', " | ")));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let paid = c.v["wallet0"] - gp(w).progress.wallet() as f64;
        check(c, exam_state(w) == Some(JobState::Active) && paid == 150.0, format!("licence: the exam is taken and the fee of 150 is paid ({paid})"));
        let crates = goods_of(w);
        let g = gp(w);
        check(c, crates.len() == 1 && crates[0].1.on_pad.as_ref().is_some_and(|l| l.as_str() == "skyhook_school"), format!("licence: the school's crate waits on its pad ({} crates)", crates.len()));
        let checks: Vec<_> = g.jobs.active().next().map(|j| j.checks.len()).into_iter().collect();
        check(c, checks == vec![3], format!("licence: three checks (take off, reach the pad, land) besides the crate ({checks:?})"));
        check(c, g.may_pilot(HOST), "licence: the exam lends the ship, the seat is open for it".into());
        check(c, licence(w) == 0, "licence: not passed yet".into());
        put_at_seat(w);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let p = prompt(w);
        check(c, p.contains("sit") && !p.contains("licence"), format!("licence: the seat prompt is plain now ('{p}')"));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let sat = seated(w);
        check(c, sat, "licence: during the exam the player may sit and fly".into());
        end(w, c, String::new());
        true
    }));
    // The flight: the three events by test hook, the player's client as sender.
    s.push(Box::new(|w, c| {
        begin(w, c, "licence: the exam's events (take-off, pad reached, landing)");
        w.resource_mut::<Gameplay>().push_world(HOST, WorldEvent::TookOff);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, _| {
        w.resource_mut::<Gameplay>().push_world(HOST, WorldEvent::PadReached { at: LocationId::new("drip_rock") });
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let done = gp(w).jobs.active().next().map(|j| j.checks.iter().filter(|c| matches!(c.state, jobs_core::CheckState::Done { .. })).count());
        check(c, done == Some(2), format!("licence: take-off and pad reached are checked off ({done:?} of 3)"));
        w.resource_mut::<Gameplay>().push_world(HOST, WorldEvent::Landed { at: Some(LocationId::new("drip_rock")), speed: 1.5 });
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let g = gp(w);
        let done = g.jobs.active().next().map(|j| j.checks.iter().filter(|c| matches!(c.state, jobs_core::CheckState::Done { .. })).count());
        check(c, done == Some(3) && exam_state(w) == Some(JobState::Active), format!("licence: the landing is checked off, the crate is still to set down ({done:?})"));
        c.v.insert("standing0", g.progress.value(&g.kernel, &TrackId::new("standing_courier_office"), None).unwrap_or(0) as f64);
        // The pilot gets up from the seat; then goes to the Drip Rock pad where the crate comes.
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, _| {
        walker_on_pad(w, "drip_rock");
        true
    }));
    s.push(settle());
    s.push(Box::new(|w, _| {
        crates_above(w, "drip_rock", 0.4);
        true
    }));
    s.push(wait(14.0));
    s.push(Box::new(|w, c| {
        let g = gp(w);
        check(c, exam_state(w) == Some(JobState::Completed), format!("licence: the crate set down completes the exam ({:?})", exam_state(w)));
        check(c, licence(w) == 1, "licence: the pilot has the flight licence".into());
        let keys: Vec<&str> = g.shown.iter().map(|l| l.key.as_str()).collect();
        check(c, keys.contains(&"notice.exam.honours"), format!("licence: passed with honours, a soft landing in good time ({keys:?})"));
        let standing = g.progress.value(&g.kernel, &TrackId::new("standing_courier_office"), None).unwrap_or(0) as f64;
        check(c, standing - c.v["standing0"] == 10.0, format!("licence: the honours give the courier office's standing a bonus ({} to {standing})", c.v["standing0"]));
        check(c, g.may_pilot(HOST) && g.offers_of(&jobs_core::GiverId::new("flight_school")).is_empty(), "licence: the licence stays, the exam is closed to its holder".into());
        let flag = g.progress.has_flag(&gameplay_core::Flag::new("exam_honours:flight_exam"));
        check(c, flag, "licence: the honours are on record".into());
        // Back at the seat (the exam's ship is no longer lent, the licence opens it).
        put_at_seat(w);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let p = prompt(w);
        check(c, p.contains("sit") && !p.contains("licence"), format!("licence: the licence holder's seat prompt is plain ('{p}')"));
        end(w, c, format!("licence {}", licence(w)));
        true
    }));
}
