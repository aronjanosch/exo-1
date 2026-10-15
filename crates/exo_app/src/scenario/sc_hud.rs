//! The flight panel: badges, the coupling blend and the toast.
use crate::hud::HudReadout;
use crate::scenario::{check, hold_until, keys, put_at_seat, sit, tap, Step};
use bevy::prelude::*;

fn readout(w: &World) -> HudReadout {
    w.resource::<HudReadout>().clone()
}

/// The badge with this key, if the panel shows it: (on, label).
fn badge(r: &HudReadout, key: &str) -> Option<(bool, String)> {
    r.badges.iter().find(|b| b.key == key).map(|b| (b.on, b.label.clone()))
}

fn toast(r: &HudReadout) -> &str {
    r.toast.as_deref().unwrap_or("")
}

fn after_tap(name: &'static str, want_toast: &'static str, check_badges: fn(&HudReadout) -> bool, what: &'static str) -> Vec<Step> {
    vec![
        hold_until(name, &[], 0.2, |_| false),
        Box::new(move |w, c| {
            let r = readout(w);
            check(c, toast(&r) == want_toast, format!("{what}: toast {:?}, want {want_toast:?}", toast(&r)));
            check(c, check_badges(&r), format!("{what}: badges {:?}", r.badges.iter().map(|b| (b.key, b.on)).collect::<Vec<_>>()));
            true
        }),
    ]
}

pub fn sc_hud_steps(s: &mut Vec<Step>) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(Box::new(|w, c| {
        let r = readout(w);
        check(c, badge(&r, "SCM") == Some((true, "SCM".into())), format!("seated, SC model: badges {:?}", r.badges));
        check(c, r.blend.is_none() && r.cap_text.ends_with("m/s") && r.g_text.ends_with(" g"), format!("seated: cap {:?}, g {:?}, blend {:?}", r.cap_text, r.g_text, r.blend));
        true
    }));

    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyC);
        true
    }));
    s.extend(after_tap("C", "DECOUPLED", |r| badge(r, "DECOUPLED") == Some((true, "DECOUPLED".into())) && badge(r, "COUPLED") == Some((false, "COUPLED".into())), "C"));
    s.push(hold_until("blend 1.8 s", &[], 1.8, |_| false));
    s.push(Box::new(|w, c| {
        let r = readout(w);
        let b = r.blend.unwrap_or(f64::NAN);
        check(c, (0.3..0.7).contains(&b), format!("blend at about 2 s: {b:.2}, want 0.3 to 0.7"));
        true
    }));
    s.push(hold_until("blend to rest 2.5 s", &[], 2.5, |_| false));
    s.push(Box::new(|w, c| {
        let r = readout(w);
        check(c, r.blend.is_none(), format!("blend after 4.5 s: {:?}, want none", r.blend));
        check(c, badge(&r, "DECOUPLED").is_some_and(|(on, _)| on), "decoupled at rest: DECOUPLED on".to_string());
        true
    }));

    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyH);
        true
    }));
    s.extend(after_tap("H", "GRAV COMP OFF", |r| badge(r, "GRAV COMP") == Some((false, "GRAV COMP".into())), "H"));

    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyB);
        true
    }));
    s.extend(after_tap("B", "NAV", |r| badge(r, "NAV") == Some((true, "NAV".into())) && badge(r, "SCM") == Some((false, "SCM".into())), "B"));

    s.push(Box::new(|w, _| {
        tap(w, KeyCode::PageDown);
        true
    }));
    s.extend(after_tap("PageDown", "LIMIT 90 %", |r| badge(r, "LIMIT") == Some((true, "LIMIT 90 %".into())), "PageDown"));

    s.push(Box::new(|w, _| {
        keys(w, &[KeyCode::KeyX], true);
        true
    }));
    s.push(hold_until("brake 0.3 s", &[], 0.3, |_| false));
    s.push(Box::new(|w, c| {
        let r = readout(w);
        check(c, badge(&r, "BRAKE") == Some((true, "BRAKE".into())), format!("X held: badge {:?}", badge(&r, "BRAKE")));
        keys(w, &[KeyCode::KeyX], false);
        true
    }));
}
