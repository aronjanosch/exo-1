//! F1 shows the keys that apply right now.
use crate::help::HelpPanel;
use crate::scenario::{check, put_at_seat, sit, tap, Step};
use bevy::prelude::*;

fn panel(w: &World) -> HelpPanel {
    w.resource::<HelpPanel>().clone()
}

fn press(k: KeyCode) -> Step {
    Box::new(move |w, _| {
        tap(w, k);
        true
    })
}

pub fn help_steps(s: &mut Vec<Step>) {
    s.push(press(KeyCode::F1));
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(press(KeyCode::F1));
    s.push(Box::new(|w, c| {
        let p = panel(w);
        check(c, !p.shown, format!("F1 again: shown {}", p.shown));
        true
    }));
}
