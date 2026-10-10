//! #170, courier jobs on foot, headless: take a courier job at the counter of the courier office,
//! carry the wobbly parcel by hand from the Drip Rock pad to the Lint Trap pad (the walker walks
//! the whole way, nothing is moved by test hooks except the start), set it down and get paid,
//! with the beats of #165. Then the same for the longer, timed job to the Pebble Kiosk.
use super::deliver::{gp, pad, prompt, show_offer, walker_on_pad};
use super::*;
use crate::gameplay::{Gameplay, Goods};
use jobs_core::JobState;

/// Walk the held parcel to the pad of `to` and set it down; `limit` seconds at most. Records the
/// walked time in `c.v["walk_s"]` and the distance in `c.v["walk_m"]`.
fn carry_to(to: &'static str, limit: f64) -> Step {
    Box::new(move |w, c| {
        let target = pad(w, to).centre;
        if c.t == 0.0 {
            begin(w, c, &format!("courier: walk the parcel to {to}"));
            c.p.insert("from", player_world(w));
            keys(w, &[KeyCode::KeyW], true);
        }
        face_towards(w, target);
        let p = player_world(w);
        let up = planet(w).up(p);
        let d = p - target;
        let flat = (d - up * d.dot(up)).length();
        if flat < 3.0 || c.t > limit {
            keys(w, &[KeyCode::KeyW], false);
            let walked = p.distance(c.p["from"]);
            c.v.insert("walk_s", c.t);
            c.v.insert("walk_m", walked);
            check(c, flat < 3.0, format!("courier: the walker reached the {to} pad on foot ({:.0} m walked in {:.0} s, {flat:.1} m short)", walked, c.t));
            end(w, c, format!("{walked:.0} m in {:.0} s", c.t));
            tap(w, KeyCode::KeyF);
            return true;
        }
        false
    })
}

/// Opens the counter, shows `template`, takes it and picks up its first crate by hand.
fn take_and_pick_up(s: &mut Vec<Step>, template: &'static str, pay: i64) {
    s.push(Box::new(move |w, c| {
        begin(w, c, &format!("courier: take {template} at the counter"));
        walker_on_pad(w, "drip_rock");
        c.v.insert("wallet0", gp(w).progress.wallet() as f64);
        c.v.insert("standing0", standing(w) as f64);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(show_offer(template));
    s.push(Box::new(move |w, c| {
        let text = w.resource_mut::<Gameplay>().panel_text("[F] take");
        check(c, text.contains("Dinglepost") && text.contains("wobbly parcel") && text.contains(&pay.to_string()), format!("courier: the briefing names the giver, the parcel and the pay ({})", text.replace('\n', " | ")));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(move |w, c| {
        let n = super::deliver::goods_of(w).len();
        let active = gp(w).jobs.active().any(|j| j.template.as_str() == template);
        check(c, active && n > 0, format!("courier: {template} is active and its parcels wait on the pad ({n})"));
        // #136: while parcels wait, the arrow points at the pickup.
        let arrow = gp(w).arrow.clone();
        check(c, arrow.as_ref().is_some_and(|a| a.target.as_str() == "drip_rock" && a.stop == jobs_core::map::Stop::Pickup), format!("courier: the arrow points at the pickup while parcels wait ({arrow:?})"));
        let e = super::deliver::goods_of(w)[0].0;
        let target = crate::scenario::cargo::crate_world_pos(w, e);
        crate::scenario::cargo::look_at(w, target);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.6));
    s.push(Box::new(|w, c| {
        let held = w.resource::<crate::grab::Grab>().held.is_some();
        check(c, held, "courier: the parcel is in the hands".into());
        true
    }));
}

fn standing(w: &World) -> i64 {
    let g = gp(w);
    g.progress.value(&g.kernel, &gameplay_core::TrackId::new("standing_courier_office"), None).unwrap_or(0)
}

/// #132: take the Lint Trap job, press abandon once (it asks, the job stays), then twice (it is
/// dropped, its parcel is a plain crate again); the job is on offer again afterwards.
fn abandon_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, c| {
        begin(w, c, "courier: abandoning a job (#132)");
        let id = gp(w).jobs.all().find(|j| j.template.as_str() == "courier_lint_trap" && j.state == JobState::Offered).map(|j| j.id).expect("on offer");
        c.v.insert("abandon_job", id.0 as f64);
        w.resource_mut::<crate::gameplay::Gameplay>().push_job(crate::gameplay::HOST, jobs_core::JobEvent::OfferAccepted { job: id });
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let n = super::deliver::goods_of(w).len();
        check(c, gp(w).jobs.active().count() == 1 && n == 1, format!("courier: the job is taken, its parcel waits ({n})"));
        c.v.insert("said0", (gp(w).notices.len() + gp(w).shown.len()) as f64);
        tap(w, KeyCode::KeyY);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        // The question is queued or already shown (the queue paces notices).
        let asked = (gp(w).notices.len() + gp(w).shown.len()) as f64 > c.v["said0"];
        check(c, asked && gp(w).jobs.active().count() == 1, "courier: one press asks and the job stays".into());
        tap(w, KeyCode::KeyY);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let id = jobs_core::JobId(c.v["abandon_job"] as u64);
        let state = gp(w).jobs.get(id).map(|j| j.state);
        let goods = super::deliver::goods_of(w).len();
        let again = gp(w).jobs.all().any(|j| j.template.as_str() == "courier_lint_trap" && j.state == JobState::Offered);
        check(c, state == Some(JobState::Abandoned) && goods == 0 && gp(w).jobs.active().count() == 0 && again, format!("courier: the second press drops the job ({state:?}), its parcel is plain ({goods} goods), the job is on offer again ({again})"));
        crate::scenario::cargo::clear_crates(w);
        end(w, c, String::new());
        true
    }));
}

pub fn courier_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, _| {
        crate::scenario::cargo::clear_crates(w);
        true
    }));
    abandon_steps(s);
    take_and_pick_up(s, "courier_lint_trap", 30);
    s.push(Box::new(|w, c| {
        // #136: the parcel in the hands, the arrow turns to the dropoff, about as far as the walk.
        let arrow = gp(w).arrow.clone();
        let to = pad(w, "lint_trap").centre;
        let from = w.query::<&crate::walker::Player>().single(w).unwrap().w.pos;
        let ok = arrow.as_ref().is_some_and(|a| a.target.as_str() == "lint_trap" && a.stop == jobs_core::map::Stop::Dropoff && (a.dist_m - from.distance(to)).abs() < 5.0);
        check(c, ok, format!("courier: after the pickup the arrow points at the dropoff ({arrow:?})"));
        true
    }));
    s.push(carry_to("lint_trap", 240.0));
    // The parcel is let go at 5 m/s and slides before it rests (it counts as delivered at rest):
    // as an Avian body that takes about 1.3 s longer than the sweep crate did, and each beat
    // follows 0.6 s after the one before.
    s.push(wait(5.5));
    s.push(Box::new(|w, c| {
        begin(w, c, "courier: paid at the Lint Trap");
        let g = gp(w);
        let paid = g.progress.wallet() as f64 - c.v["wallet0"];
        // Set down by hand from about a metre: a hair of damage, so a little under the full 30.
        // (A repeatable job is offered again at once, so look for any completed one.)
        let done = gp(w).jobs.all().any(|j| j.template.as_str() == "courier_lint_trap" && j.state == JobState::Completed);
        check(c, done && (27.0..=30.0).contains(&paid), format!("courier: the parcel set down on the Lint Trap pad completes the job (paid {paid} of 30)"));
        // #136: the money on the job line follows the payout; no job, no arrow.
        let money = format!("{} {}", g.progress.wallet(), g.text(&gameplay_core::TextKey::new("track.wallet.name")));
        check(c, paid > 0.0 && g.readout.starts_with(&money) && g.arrow.is_none(), format!("courier: the HUD shows the new money and no arrow ('{}', {:?})", g.readout.lines().next().unwrap_or(""), g.arrow));
        check(c, standing(w) as f64 - c.v["standing0"] == 10.0, format!("courier: the courier office's standing rose ({} to {})", c.v["standing0"], standing(w)));
        let walk_m = c.v["walk_m"];
        check(c, (150.0..=400.0).contains(&walk_m), format!("courier: the drop is a walk of 150 to 400 m ({walk_m:.0} m)"));
        let secs = c.v["walk_s"];
        check(c, (15.0..=180.0).contains(&secs), format!("courier: on foot about {secs:.0} s"));
        let keys: Vec<&str> = g.shown.iter().map(|l| l.key.as_str()).collect();
        let order = ["notice.job.accepted", "notice.job.picked_up", "notice.job.delivered", "notice.job.completed", "notice.reward.base", "notice.reward.xp", "notice.reward.standing"];
        let mut at = 0;
        for k in &keys {
            if at < order.len() && *k == order[at] {
                at += 1;
            }
        }
        check(c, at == order.len(), format!("courier: the beats of #165 in order ({keys:?})"));
        end(w, c, format!("wallet {}", g.progress.wallet()));
        true
    }));
    // The timed, longer job.
    take_and_pick_up(s, "courier_pebble_kiosk", 60);
    s.push(Box::new(|w, c| {
        // Three parcels: the other two ride along in a second trip; here the hand carries one.
        let left = super::deliver::goods_of(w).len();
        check(c, left == 3, format!("courier: the timed job asks for 3 parcels ({left})"));
        let clock = gp(w).jobs.active().next().and_then(|j| j.clock_s);
        check(c, clock.is_some(), "courier: its clock runs from the first pickup".into());
        true
    }));
    s.push(carry_to("pebble_kiosk", 240.0));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let m = c.v["walk_m"];
        check(c, (150.0..=400.0).contains(&m), format!("courier: the Pebble Kiosk is a walk of 150 to 400 m ({m:.0} m)"));
        let n = w.query::<&Goods>().iter(w).count();
        check(c, n == 3, format!("courier: one of three parcels delivered, the job stays open ({n} parcels in the job)"));
        let _ = prompt(w);
        true
    }));
}
