//! Helpers the job scenarios share (savefile, licence, map, town): the gameplay resource, pads,
//! goods crates, the walker on a pad, the counter's offers.
use super::*;
use crate::cargo::Crate;
use crate::gameplay::{Gameplay, Goods};
use gameplay_core::LocationId;
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

/// With the counter open: page through the offers (the keyless `next_offer`) until the panel shows the template (the next
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
            w.resource_mut::<Controls>().actions.push(crate::controls::Tap::NextOffer);
        }
        false
    })
}
