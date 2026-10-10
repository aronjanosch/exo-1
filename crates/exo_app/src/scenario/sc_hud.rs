//! Scenario `sc-hud` (#197): the flight panel through `Controls`. Sit (axis model), F7 to the SC
//! model, C decouples (the blend runs 4 s), H drops gravity compensation, B takes NAV, Page Down
//! lowers the limiter, X brakes, F7 back to the axis model. Each change shows as a toast and as a
//! badge; the blend bar is shown only while the coupling moves.
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

/// One step after a tap: the toast and the badge state the change caused.
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
        let assist = badge(&r, "ASSIST");
        check(c, badge(&r, "MODEL") == Some((true, "AXIS".into())) && assist == Some((true, "ASSIST".into())), format!("seated, axis model: badges {:?}", r.badges));
        check(c, r.blend.is_none() && r.cap_text.ends_with("m/s") && r.g_text.ends_with(" g"), format!("seated: cap {:?}, g {:?}, blend {:?}", r.cap_text, r.g_text, r.blend));
        true
    }));

    // F7: the SC model, its badges, and the toast.
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::F7);
        true
    }));
    s.extend(after_tap("F7 to SC", "MODEL SC", |r| badge(r, "MODEL") == Some((true, "SC".into())), "F7"));

    // C: decouple. The toast is on at once, the blend runs from 1 to 0 over 4 s and is gone after.
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

    // H: gravity compensation off (the SC meaning of H).
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyH);
        true
    }));
    s.extend(after_tap("H", "GRAV COMP OFF", |r| badge(r, "GRAV COMP") == Some((false, "GRAV COMP".into())), "H"));

    // B: master mode NAV, SCM dimmed.
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyB);
        true
    }));
    s.extend(after_tap("B", "NAV", |r| badge(r, "NAV") == Some((true, "NAV".into())) && badge(r, "SCM") == Some((false, "SCM".into())), "B"));

    // Page Down: the limiter to 90 %, its badge appears.
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::PageDown);
        true
    }));
    s.extend(after_tap("PageDown", "LIMIT 90 %", |r| badge(r, "LIMIT") == Some((true, "LIMIT 90 %".into())), "PageDown"));

    // X held: the brake badge is on while it is held.
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

    // F7 back: the axis model's badges.
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::F7);
        true
    }));
    s.extend(after_tap("F7 to axis", "MODEL AXIS", |r| badge(r, "MODEL") == Some((true, "AXIS".into())) && badge(r, "ASSIST").is_some() && badge(r, "GRAV COMP").is_none(), "F7 back"));
}
