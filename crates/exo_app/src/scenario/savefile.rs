//! #135, the save file, headless: take the first haul, deliver one of its three crates, knock
//! another about, save to the file, restart the gameplay state and load it; wallet, tracks, flags,
//! the active job and the crates (with their condition) come back, and the job finishes. The
//! crates move by test hooks, as in `deliver`.
use super::deliver::{goods_of, gp, job_state, pad, walker_on_pad};
use super::*;
use crate::cargo::Crate;
use crate::gameplay::{Gameplay, Goods, HOST};
use crate::savefile;
use gameplay_core::TrackId;
use jobs_core::{JobEvent, JobState, TemplateId};

/// One goods crate, set down `height` m over the pad of `l`, `side` m along it.
fn crate_above(w: &mut World, e: Entity, l: &str, side: f64, height: f64) {
    let p = pad(w, l);
    let east = p.up.any_orthonormal_vector();
    let pl = planet(w);
    let at = p.centre + east * side;
    let dir = pl.up(at);
    let mut c = w.get_mut::<Crate>(e).unwrap();
    let half = c.body.half.y;
    c.body.pos = pl.centre + dir * (pl.surface(dir) + half + height);
    c.body.vel = DVec3::ZERO;
    c.body.grounded = false;
    c.body.wake();
}

/// The state before the restart, to compare after it.
#[derive(Resource)]
struct Before(String);

/// What must survive the restart, as text to compare.
fn state(w: &mut World) -> String {
    let crates: Vec<String> = {
        let mut q = w.query::<(&Crate, &Goods)>();
        let mut v: Vec<(u64, String)> = q.iter(w).map(|(c, g)| (g.id.0, format!("{}:{}:{:.3}:{:?}", g.id.0, g.commodity, c.condition, g.on_pad))).collect();
        v.sort();
        v.into_iter().map(|(_, s)| s).collect()
    };
    let g = gp(w);
    let xp = g.progress.value(&g.kernel, &TrackId::new("freight_xp"), Some(HOST));
    let job = g.jobs.active().map(|j| format!("{}:{:?}:{}", j.template, j.state, j.delivered())).collect::<Vec<_>>();
    format!("wallet {} xp {xp:?} jobs {job:?} crates {crates:?} progress {}", g.progress.wallet(), serde_json::to_string(&g.progress).unwrap())
}

pub(super) fn savefile_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, c| {
        crate::scenario::cargo::clear_crates(w);
        begin(w, c, "savefile: a job half done survives a restart (#135)");
        walker_on_pad(w, "drip_rock");
        // The first haul is on offer at the courier office; take it.
        let t = TemplateId::new("first_haul");
        let id = gp(w).jobs.all().find(|j| j.template == t && j.state == JobState::Offered).map(|j| j.id).expect("first haul on offer");
        c.v.insert("wallet0", gp(w).progress.wallet() as f64);
        w.resource_mut::<Gameplay>().push_job(HOST, JobEvent::OfferAccepted { job: id });
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let g = goods_of(w);
        check(c, job_state(w, "first_haul") == Some(JobState::Active) && g.len() == 3, format!("savefile: the job is active with its 3 crates ({} crates)", g.len()));
        // One crate to Bent Spoon, set down gently: one of three delivered.
        crate_above(w, g[0].0, "bent_spoon", 0.0, 0.4);
        // Another knocked about on the pickup pad: a little worse for wear.
        w.get_mut::<Crate>(g[1].0).unwrap().condition = 0.8;
        true
    }));
    s.push(wait(3.0));
    s.push(Box::new(|w, c| {
        let delivered = gp(w).jobs.active().next().map_or(0, |j| j.delivered());
        let writes = w.resource::<savefile::Autosave>().writes;
        check(c, delivered == 1, format!("savefile: one crate delivered before the save ({delivered})"));
        check(c, writes >= 1, format!("savefile: the job events were autosaved ({writes} writes)"));
        let before = state(w);
        let r = savefile::save_now(w);
        check(c, r.is_ok(), format!("savefile: saved to the file ({r:?})"));
        w.insert_resource(Before(before));
        // The restart: a fresh gameplay state as at start, no goods crates, then the file read back.
        let fresh = Gameplay::load(w.resource::<crate::cargo::Crates>());
        w.insert_resource(fresh);
        let goods: Vec<Entity> = goods_of(w).into_iter().map(|(e, _)| e).collect();
        for e in goods {
            w.despawn(e);
        }
        check(c, gp(w).progress.wallet() as f64 == c.v["wallet0"] && gp(w).jobs.active().next().is_none(), "savefile: the restart starts from nothing".into());
        let dir = w.resource::<savefile::SaveDir>().0.clone();
        let text = savefile::read_slot(&dir, savefile::SLOT).ok().flatten().unwrap_or_default();
        match gameplay_core::save::Envelope::from_json(&text) {
            Ok(env) => {
                w.insert_resource(savefile::PendingLoad(env));
            }
            Err(e) => check(c, false, format!("savefile: the file reads back ({e})")),
        }
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let after = state(w);
        let loaded = gp(w).log.iter().any(|l| l.contains("loaded the save"));
        let before = w.resource::<Before>().0.clone();
        check(c, loaded && after == before, format!("savefile: wallet, tracks, the job and the crates are as saved\n  before {before}\n  after  {after}"));
        let worn = { let mut q = w.query::<(&Crate, &Goods)>(); q.iter(w).filter(|(c, _)| (c.condition - 0.8).abs() < 1e-6).count() };
        check(c, worn == 1, format!("savefile: the worn crate kept its condition ({worn} at 0.8)"));
        // Finish the job: the other two to Bent Spoon.
        let g = goods_of(w);
        let rest: Vec<Entity> = g.iter().filter(|(_, g)| g.on_pad.as_ref().is_none_or(|l| l.as_str() != "bent_spoon")).map(|(e, _)| *e).collect();
        check(c, rest.len() == 2, format!("savefile: two crates still to carry ({})", rest.len()));
        for (i, e) in rest.into_iter().enumerate() {
            crate_above(w, e, "bent_spoon", 1.5 + i as f64 * 1.5, 0.4);
        }
        true
    }));
    s.push(wait(3.0));
    s.push(Box::new(|w, c| {
        let paid = gp(w).progress.wallet() as f64 - c.v["wallet0"];
        let done = gp(w).jobs.all().any(|j| j.template == TemplateId::new("first_haul") && j.state == JobState::Completed);
        check(c, done && paid > 0.0 && paid < 300.0, format!("savefile: the loaded job completes and pays, a little less for the worn crate ({paid} of 300)"));
        end(w, c, format!("wallet {}", gp(w).progress.wallet()));
        true
    }));
}
