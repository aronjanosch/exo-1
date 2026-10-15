//! Showcase models from outside, and the starting ship's deck (window only).
use super::*;
use crate::ship_showcase::{Showcase, ShowcaseReady};

const OWN: &str = "ship/starting-ship-study.glb";

fn ready(w: &mut World) -> Vec<(&'static str, ShowcaseReady)> {
    let mut v: Vec<_> = w.query::<(&Showcase, &ShowcaseReady)>().iter(w).map(|(s, r)| (s.path, *r)).collect();
    v.sort_by_key(|(p, _)| *p);
    v
}

fn player_spawn(w: &mut World) -> DVec3 {
    let pl = planet(w);
    let up = w.resource::<crate::ship_showcase::ShowcaseStart>().0;
    pl.centre + up * pl.surface(up)
}

pub fn showcase_steps(s: &mut Vec<Step>, out_dir: &std::path::Path, windowed: bool) {
    let dir = out_dir.to_path_buf();
    s.push(Box::new(|w, c| {
        let (all, done) = (w.query::<&Showcase>().iter(w).count(), w.query::<&ShowcaseReady>().iter(w).count());
        if all > 0 && done == all {
            check(c, true, format!("showcase: {done} models with colliders"));
            return true;
        }
        if c.t > 60.0 {
            check(c, false, format!("showcase: {done} of {all} models ready after 60 s"));
            return true;
        }
        false
    }));
    let n = MODEL_SHOTS;
    for i in 0..n {
        s.push(Box::new(move |w, c| {
            let models = ready(w);
            let Some((path, r)) = models.get(i).copied() else { return true };
            let pl = planet(w);
            let spawn = player_spawn(w);
            let up = pl.up(r.origin);
            let mut away = spawn - r.origin;
            away -= up * away.dot(up);
            let away = away.try_normalize().unwrap_or(up.any_orthonormal_vector());
            place_walker(w, r.origin + away * (r.radius + 12.0));
            face_towards(w, r.origin);
            c.v.insert("shot_i", i as f64);
            println!("showcase: picture of {path}");
            true
        }));
        s.push(wait(2.0));
        let dir = dir.clone();
        s.push(Box::new(move |w, c| {
            if ready(w).len() > i {
                shot(w, c, &dir, windowed, "outside");
            }
            true
        }));
        s.push(wait(0.3));
    }
    s.push(Box::new(|w, c| {
        let Some((_, r)) = ready(w).into_iter().find(|(p, _)| *p == OWN) else {
            check(c, false, "showcase: the starting ship is there".into());
            return true;
        };
        let rot = w.query::<(&Showcase, &ShowcaseReady)>().iter(w).find(|(s, _)| s.path == OWN).map(|(s, _)| s.rot).unwrap();
        let up = rot * DVec3::Y;
        with_player(w, |p| {
            p.ship = None;
            p.w.pos = r.origin + up * 0.3;
            p.w.halt();
            p.w.grounded = false;
        });
        face_towards(w, r.origin - rot * DVec3::X * 10.0);
        c.v.insert("deck_h", 0.0);
        true
    }));
    s.push(wait(2.0));
    let dir2 = dir.clone();
    s.push(Box::new(move |w, c| {
        let Some((_, r)) = ready(w).into_iter().find(|(p, _)| *p == OWN) else { return true };
        let pl = planet(w);
        let up = pl.up(r.origin);
        let h = (player_world(w) - r.origin).dot(up);
        check(c, h.abs() < 0.3, format!("showcase: the walker stands on the starting ship's deck ({h:+.2} m from the floor)"));
        shot(w, c, &dir2, windowed, "deck");
        true
    }));
    s.push(wait(0.5));
}

const MODEL_SHOTS: usize = 4;
