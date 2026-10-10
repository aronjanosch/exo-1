//! Scenario `help` (round 5): F1 (through the bindings) shows the keys that apply right now: on
//! foot the walking keys, seated the ship's switches; F1 again hides the panel.
use crate::help::HelpPanel;
use crate::scenario::{check, put_at_seat, sit, tap, Step};
use bevy::prelude::*;

fn panel(w: &World) -> HelpPanel {
    w.resource::<HelpPanel>().clone()
}

fn has(p: &HelpPanel, keys: &str, what: &str) -> bool {
    p.lines.iter().any(|(k, w)| k == keys && *w == what)
}

fn press(k: KeyCode) -> Step {
    Box::new(move |w, _| {
        tap(w, k);
        true
    })
}

pub fn help_steps(s: &mut Vec<Step>) {
    s.push(press(KeyCode::F1));
    s.push(Box::new(|w, c| {
        let p = panel(w);
        check(c, p.shown && p.title == "On foot", format!("F1 on foot: shown {}, title {:?}", p.shown, p.title));
        check(c, has(&p, "W A S D", "Walk") && has(&p, "Shift", "Run"), format!("on foot: walk and run keys ({} lines)", p.lines.len()));
        true
    }));
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(Box::new(|w, c| {
        let p = panel(w);
        check(c, p.shown && p.title == "Ship", format!("seated: title {:?}", p.title));
        check(c, has(&p, "H", "Gravity compensation") && has(&p, "B", "Master mode SCM / NAV"), "seated: the SC model's switches".into());
        check(c, has(&p, "X", "Brake"), "seated: the brake key".into());
        true
    }));
    s.push(press(KeyCode::F1));
    s.push(Box::new(|w, c| {
        let p = panel(w);
        check(c, !p.shown, format!("F1 again: shown {}", p.shown));
        true
    }));
}
