//! #133, the first playable round, headless: take the fixed job on the Drip Rock pad, pick up a
//! crate, then put the crates down on the Bent Spoon pad and get paid; a second job with crates
//! dropped from 10 m pays less. The flight between the pads is the player's part (and the flight
//! scenarios'); here the walker and the crates are moved by test hooks.
use super::*;
use crate::cargo::Crate;
use crate::gameplay::{Gameplay, Goods};
use gameplay_core::{Flag, LocationId, TrackId};
use jobs_core::{JobState, TemplateId};

pub(super) fn gp(w: &World) -> &Gameplay {
    w.resource::<Gameplay>()
}

pub(super) fn pad(w: &World, l: &str) -> crate::gameplay::Pad {
    gp(w).pad_of(&LocationId::new(l)).unwrap_or_else(|| panic!("no pad for {l}")).clone()
}

pub(super) fn goods_of(w: &mut World) -> Vec<(Entity, Goods)> {
    let mut v: Vec<(Entity, Goods)> = w.query::<(Entity, &Goods)>().iter(w).map(|(e, g)| (e, g.clone())).collect();
    v.sort_by_key(|(_, g)| g.id);
    v
}

pub(super) fn prompt(w: &World) -> String {
    w.resource::<crate::interact::Interaction>().prompt.clone()
}

/// The walker on a pad, 2 m past its centre, facing the row the crates spawn in (4 m ahead).
pub(super) fn walker_on_pad(w: &mut World, l: &str) {
    let p = pad(w, l);
    let north = p.up.cross(p.up.any_orthonormal_vector());
    place_walker(w, p.centre + north * 2.0);
    face_towards(w, p.centre + north * 10.0);
}

/// Every goods crate of `job` to a spot above the pad of `l`, `height` m over the ground, in a row.
pub(super) fn crates_above(w: &mut World, l: &str, height: f64) {
    let p = pad(w, l);
    let east = p.up.any_orthonormal_vector();
    let pl = planet(w);
    for (i, (e, _)) in goods_of(w).into_iter().enumerate() {
        let at = p.centre + east * (i as f64 * 1.5 - 1.5);
        let dir = pl.up(at);
        let mut c = w.get_mut::<Crate>(e).unwrap();
        let half = c.body.half.y;
        c.body.pos = pl.centre + dir * (pl.surface(dir) + half + height);
        c.body.vel = DVec3::ZERO;
        c.body.grounded = false;
        c.body.wake();
    }
}

pub(super) fn job_state(w: &World, template: &str) -> Option<JobState> {
    let t = TemplateId::new(template);
    gp(w).jobs.all().filter(|j| j.template == t).last().map(|j| j.state)
}

pub fn deliver_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, c| {
        crate::scenario::cargo::clear_crates(w);
        begin(w, c, "deliver: take the job on the Drip Rock pad");
        walker_on_pad(w, "drip_rock");
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let p = prompt(w);
        check(c, p.contains("talk to") && p.contains("Dinglepost"), format!("deliver: the prompt at the counter names the giver ('{p}')"));
        c.v.insert("wallet0", gp(w).progress.wallet() as f64);
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    // The courier office has several offers: Tab until the panel shows the first haul.
    s.push(show_offer("first_haul"));
    s.push(Box::new(|w, c| {
        let p = prompt(w);
        let text = w.resource_mut::<Gameplay>().panel_text("[F] take");
        let open = gp(w).panel.is_some();
        check(c, open && text.contains("First haul") && text.contains("Dinglepost") && text.contains("fizzy mud") && text.contains("Bent Spoon"), format!("deliver: the counter opens the briefing of the giver ({})", text.replace('\n', " | ")));
        check(c, p.contains("take the job") && p.contains("First haul"), format!("deliver: the prompt offers the job ('{p}')"));
        // The same offer reads the same each time it is opened.
        let again = w.resource_mut::<Gameplay>().panel_text("[F] take");
        check(c, again == text, "deliver: the briefing reads the same when asked again".into());
        // Declining closes the panel and keeps the offer.
        tap(w, KeyCode::Backspace);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let closed = gp(w).panel.is_none();
        let offered = gp(w).jobs.all().any(|j| j.state == JobState::Offered);
        check(c, closed && offered, "deliver: declining closes the panel and the offer stays".into());
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.3));
    s.push(show_offer("first_haul"));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let g = goods_of(w);
        let on = g.iter().filter(|(_, g)| g.on_pad.as_ref().is_some_and(|l| l.as_str() == "drip_rock")).count();
        check(c, job_state(w, "first_haul") == Some(JobState::Active) && g.len() == 3 && on == 3, format!("deliver: the job is active and its 3 crates wait on the pad ({} crates, {on} on the pad)", g.len()));
        let target = crate::scenario::cargo::crate_world_pos(w, g[1].0);
        crate::scenario::cargo::look_at(w, target);
        c.v.insert("crate", g[1].0.to_bits() as f64);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.6));
    s.push(Box::new(|w, c| {
        let e = Entity::from_bits(c.v["crate"] as u64);
        let g = w.get::<Goods>(e).cloned();
        let held = w.resource::<crate::grab::Grab>().held.map(|h| h.crate_e) == Some(e);
        let job = gp(w).jobs.active().next().cloned();
        let clock = job.and_then(|j| j.clock_s);
        check(c, held && g.is_some_and(|g| g.on_pad.is_none()) && clock.is_some(), format!("deliver: picking a crate up is a pickup (held {held}, job clock {clock:?})"));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.5));
    s.push(Box::new(|w, c| {
        let delivered = gp(w).jobs.active().next().map_or(0, |j| j.delivered());
        check(c, delivered == 0, format!("deliver: setting it down on the pickup pad delivers nothing ({delivered} delivered)"));
        // The flight to Bent Spoon is the player's: the walker and the crates go there by test hook.
        walker_on_pad(w, "bent_spoon");
        true
    }));
    s.push(settle());
    s.push(Box::new(|w, _| {
        crates_above(w, "bent_spoon", 0.4);
        true
    }));
    s.push(wait(3.0));
    s.push(Box::new(|w, c| {
        let gp = gp(w);
        let paid = gp.progress.wallet() as f64 - c.v["wallet0"];
        let xp = gp.progress.value(&gp.kernel, &TrackId::new("freight_xp"), Some(crate::gameplay::HOST)).unwrap_or(0);
        let flag = gp.progress.has_flag(&Flag::new("job_completed:first_haul"));
        check(c, job_state(w, "first_haul") == Some(JobState::Completed) && paid == 300.0 && xp == 50 && flag,
            format!("deliver: 3 crates set down gently on the Bent Spoon pad complete the job (paid {paid}, XP {xp}, flag {flag})"));
        let standing = gp.progress.value(&gp.kernel, &TrackId::new("standing_courier_office"), None);
        check(c, standing == Some(10), format!("deliver: the giver's standing rose by a completed job (#167, {standing:?})"));
        let giver = jobs_core::GiverId::new("courier_office");
        check(c, gp.jobs.history(&giver).mood() == jobs_core::Mood::Regular, "deliver: the giver now treats the crew as a regular".into());
        end(w, c, format!("wallet {}", gp.progress.wallet()));
        true
    }));
    // #165: the ritual comes out staged, one beat after the other.
    s.push(wait(12.0));
    s.push(Box::new(|w, c| {
        begin(w, c, "deliver: the notices of the first job (#165)");
        let g = gp(w);
        let keys: Vec<&str> = g.shown.iter().map(|l| l.key.as_str()).collect();
        let order = ["notice.job.accepted", "notice.job.picked_up", "notice.job.delivered", "notice.job.completed", "notice.reward.base", "notice.reward.xp"];
        let mut at = 0;
        for k in &keys {
            if at < order.len() && *k == order[at] {
                at += 1;
            }
        }
        check(c, at == order.len(), format!("deliver: beats in order accepted, picked up, delivered, completed, base pay, XP ({keys:?})"));
        check(c, keys.iter().filter(|k| **k == "notice.job.delivered").count() == 3 && keys.iter().filter(|k| **k == "notice.job.completed").count() == 1, "deliver: three delivered beats and one completed".into());
        let done = g.shown.iter().find(|l| l.key == "notice.job.completed");
        check(c, done.is_some_and(|l| l.banner && l.money == Some(300) && l.text.contains("300")), format!("deliver: the completed banner carries the payout ({:?})", done.map(|l| &l.text)));
        // Never two banners at once: each banner starts after the previous one is over.
        let banners: Vec<&crate::gameplay::ShownLine> = g.shown.iter().filter(|l| l.banner).collect();
        let overlap = banners.windows(2).any(|p| p[1].at + 0.05 < p[0].at + p[0].seconds);
        check(c, !banners.is_empty() && !overlap, format!("deliver: never two banners at once ({} banners)", banners.len()));
        // Never the same line twice in a row for a key.
        let mut repeat = false;
        for key in ["notice.job.picked_up", "notice.job.delivered"] {
            let t: Vec<&str> = g.shown.iter().filter(|l| l.key == key).map(|l| l.text.as_str()).collect();
            repeat |= t.windows(2).any(|p| p[0] == p[1]);
        }
        check(c, !repeat, "deliver: no line twice in a row".into());
        check(c, g.notices.is_empty(), "deliver: the queue is empty after the ritual".into());
        end(w, c, format!("{} notices shown", g.shown.len()));
        true
    }));
    // A second round of the same job, the crates dropped from 10 m: they arrive damaged.
    s.push(Box::new(|w, c| {
        begin(w, c, "deliver: the same job with crates dropped from 10 m");
        let t = gp(w).jobs_content.templates[&TemplateId::new("first_haul")].record.clone();
        let id = w.resource_mut::<Gameplay>().jobs.offer_fixed(&t).unwrap();
        w.resource_mut::<Gameplay>().push_job(crate::gameplay::HOST, jobs_core::JobEvent::OfferAccepted { job: id });
        c.v.insert("wallet1", gp(w).progress.wallet() as f64);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, _| {
        // Only the new job's crates carry goods (the first job's stay as plain crates).
        crates_above(w, "bent_spoon", 10.0);
        true
    }));
    s.push(wait(4.0));
    s.push(Box::new(|w, c| {
        let paid = gp(w).progress.wallet() as f64 - c.v["wallet1"];
        check(c, job_state(w, "first_haul") == Some(JobState::Completed) && paid > 0.0 && paid < 300.0,
            format!("deliver: damaged crates still complete the job but pay less (paid {paid} of 300)"));
        end(w, c, format!("wallet {}", gp(w).progress.wallet()));
        true
    }));
    // #165: the player can skip the ritual; the same notices come out, fast.
    s.push(Box::new(|w, c| {
        begin(w, c, "deliver: skipping the ritual (#165)");
        c.v.insert("shown0", gp(w).shown.len() as f64);
        tap(w, KeyCode::Enter);
        true
    }));
    s.push(wait(4.0));
    s.push(Box::new(|w, c| {
        let g = gp(w);
        check(c, g.notices.is_empty() && g.shown.len() as f64 > c.v["shown0"], format!("deliver: after skipping the queue is empty within 4 s ({} notices, {} left)", g.shown.len(), g.notices.len()));
        end(w, c, String::new());
        true
    }));
}

/// With the counter open: Tab through the offers until the panel shows the template (the next
/// tap only after the last one was seen, a few ticks apart).
pub(super) fn show_offer(template: &'static str) -> Step {
    Box::new(move |w, c| {
        let shown = gp(w).panel_offer().and_then(|j| gp(w).jobs.get(j).map(|j| j.template.as_str() == template));
        if shown == Some(true) {
            return true;
        }
        assert!(c.t < 5.0, "the counter never showed {template}");
        if c.t - c.v.get("tab").copied().unwrap_or(-1.0) > 0.2 {
            c.v.insert("tab", c.t);
            tap(w, KeyCode::Tab);
        }
        false
    })
}
