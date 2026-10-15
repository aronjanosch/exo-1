//! Scenario hud: what the player reads, through CrewHud without a window.
use super::items::{at_the_table, collect, items, look_at, outside, tap_then, DELAY};
use super::*;
use crate::crew_hud::{CrewHud, Vitals};
use crate::displays::DisplayLook;
use crate::gameplay::{Gameplay, HOST};
use crate::controls::Controls;
use crate::interact::Interaction;
use crate::items::{slot_rect, HudWindow, InventoryUi};
use display_core::hud::{north_at, HintAction, PromptKind, Reference, VitalKind};
use display_core::Tone;
use gameplay_core::{TrackId, WorldEvent};

fn hud(w: &World) -> CrewHud {
    w.resource::<CrewHud>().clone()
}

fn vitals(w: &mut World) -> Mut<'_, Vitals> {
    w.resource_mut::<Vitals>()
}

/// Face a heading seen from the walker: 0 = `forward`, 90 = its right, on the plane around `up`.
fn face_heading(w: &mut World, forward: DVec3, up: DVec3, deg: f64) {
    let right = forward.cross(up);
    let a = deg.to_radians();
    let dir = forward * a.cos() + right * a.sin();
    let at = player_world(w) + dir * 100.0;
    face_towards(w, at);
}

fn kinds(h: &CrewHud) -> Vec<VitalKind> {
    h.vitals.iter().map(|l| l.kind).collect()
}

fn heading_step(what: &'static str, deg: f64, reference: Reference, forward_of: fn(&mut World) -> (DVec3, DVec3)) -> Vec<Step> {
    vec![
        Box::new(move |w, _| {
            let (fwd, up) = forward_of(w);
            face_heading(w, fwd, up, deg);
            true
        }),
        wait(0.1),
        Box::new(move |w, c| {
            let h = hud(w);
            let off = (h.heading - deg + 180.0).rem_euclid(360.0) - 180.0;
            check(c, off.abs() < 0.6 && h.reference == reference, format!("hud: {what}: heading {:.1} (want {deg:.0}), frame {:?} (want {reference:?})", h.heading, h.reference));
            true
        }),
    ]
}

fn planet_axes(w: &mut World) -> (DVec3, DVec3) {
    let p = player_world(w);
    let up = planet(w).up(p);
    (north_at(up), up)
}

fn bow_axes(w: &mut World) -> (DVec3, DVec3) {
    let f = ship_frame_of(w);
    (f.rot * DVec3::NEG_Z, f.rot * DVec3::Y)
}

pub(super) fn hud_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    at_the_table(s);
    s.push(Box::new(|w, c| {
        begin(w, c, "hud: calm on the dev area");
        let h = hud(w);
        check(c, kinds(&h) == [VitalKind::Health] && h.credits.is_none() && h.hints.is_empty() && h.held_name.is_empty(), format!("hud: calm: health only ({:?}), no credits, no hints", kinds(&h)));
        true
    }));
    s.push(shot_step("hud_calm"));

    s.extend(heading_step("north on foot", 0.0, Reference::True, planet_axes));
    s.extend(heading_step("east on foot", 90.0, Reference::True, planet_axes));
    s.extend(heading_step("north-west on foot", 315.0, Reference::True, planet_axes));

    s.push(Box::new(|w, c| {
        {
            let mut v = vitals(w);
            v.health = 0.8;
            v.oxygen = Some(0.7);
            v.burn_level = 2;
        }
        c.v.insert("set", 1.0);
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        let h = hud(w);
        check(c, kinds(&h) == [VitalKind::Burn, VitalKind::Oxygen, VitalKind::Health], format!("hud: vitals with oxygen and burn set show three lines, burn on top ({:?})", kinds(&h)));
        check(c, (h.vitals[2].frac - 0.8).abs() < 1e-9 && (h.vitals[1].frac - 0.7).abs() < 1e-9, "hud: the bars carry the values".into());
        check(c, h.vitals[0].burn_text() == "▲ BURN II" && h.vitals[2].calm() && h.vitals[1].calm(), format!("hud: the burn reads {:?}, health 80 and oxygen 70 are calm", h.vitals[0].burn_text()));
        {
            let mut v = vitals(w);
            v.oxygen = None;
            v.burn_level = 0;
        }
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        let h = hud(w);
        check(c, kinds(&h) == [VitalKind::Health], format!("hud: oxygen and burn gone again, health stays ({:?})", kinds(&h)));
        vitals(w).health = 1.0;
        true
    }));

    s.push(Box::new(|w, c| {
        begin(w, c, "hud: credits flash after a change");
        check(c, hud(w).credits.is_none(), "hud: no credits on screen before a change".into());
        w.resource_mut::<Gameplay>().push_world(HOST, WorldEvent::TrackChanged { track: TrackId::new(gameplay_core::content::WALLET), delta: 300, player: None });
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let cr = hud(w).credits;
        check(c, cr.as_ref().is_some_and(|x| x.delta == 300 && x.alpha == 1.0), format!("hud: +300 shows ({cr:?})"));
        true
    }));
    s.push(shot_step("hud_credits"));
    s.push(Box::new(|w, c| {
        let look = w.resource::<DisplayLook>().0.hud.credits.clone();
        if c.t < look.show_s + look.fade_s + 0.5 {
            return false;
        }
        check(c, hud(w).credits.is_none(), "hud: the credits faded out after about two seconds".into());
        end(w, c, "credits".into());
        true
    }));

    s.push(Box::new(|w, c| {
        let Some((_, _, at)) = outside(w, "extinguisher") else {
            check(c, false, "hud: the extinguisher lies there".into());
            return true;
        };
        look_at(w, at);
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let i = w.resource::<Interaction>();
        check(c, i.kind == PromptKind::Collect && i.at.is_some() && i.prompt.starts_with("[F]"), format!("hud: the prompt is a collect with a world place ({:?}, {:?})", i.kind, i.prompt));
        check(c, i.view.verb == "Collect" && i.view.name == "Extinguisher" && i.view.note.is_empty(), format!("hud: the prompt reads verb over name ({:?})", i.view));
        true
    }));
    s.extend(collect("extinguisher", "Extinguisher"));

    s.extend(tap_then_state("hud: 1 takes the extinguisher into the hand", KeyCode::Digit1, |w| items(w).in_hand(0).is_some_and(|s| s.item.kind.as_str() == "extinguisher")));
    s.push(wait(DELAY * 2.0));
    s.push(Box::new(|w, c| {
        let h = hud(w);
        let acts: Vec<_> = h.hints.iter().map(|x| x.action).collect();
        check(c, h.held_name == "Extinguisher" && acts == [HintAction::Use, HintAction::Drop, HintAction::Throw] && h.hints.iter().all(|x| !x.dim), format!("hud: held name {:?}, hints {acts:?}", h.held_name));
        check(c, h.contrast == 1.0, "hud: the bar is at full contrast right after taking the slot".into());
        true
    }));
    s.push(Box::new(|w, c| {
        let b = w.resource::<DisplayLook>().0.hud.bar.clone();
        if c.t < b.idle_s + b.idle_fade_s + 0.3 {
            return false;
        }
        let h = hud(w);
        check(c, (h.contrast - b.idle_contrast).abs() < 1e-9, format!("hud: after {:.0} idle seconds the bar is at {:.2} contrast", b.idle_s, h.contrast));
        true
    }));
    s.extend(tap_then_state("hud: slot key wakes the bar", KeyCode::Digit1, |w| hud(w).contrast == 1.0));

    s.push(Box::new(|w, c| {
        begin(w, c, "hud: oxygen drops, burning");
        let mut v = vitals(w);
        v.health = 0.58;
        v.oxygen = Some(1.0);
        v.burn_level = 2;
        v.burning = true;
        true
    }));
    s.push(Box::new(|w, c| {
        let o = (1.0 - c.t / 1.5 * 0.55).max(0.45);
        vitals(w).oxygen = Some(o);
        c.t >= 1.5
    }));
    s.push(wait(0.2));
    s.push(shot_step("hud_held"));
    s.push(Box::new(|w, _| {
        let mut v = vitals(w);
        v.health = 0.23;
        v.oxygen = Some(0.18);
        v.burn_level = 3;
        v.burning = true;
        true
    }));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let h = hud(w);
        check(c, h.vitals[0].burn_text() == "■ BURN III · ON FIRE" && h.vitals.iter().all(|l| l.tone == Tone::Bad), format!("hud: critical reads bad on every line ({:?})", h.vitals));
        true
    }));
    s.push(shot_step("hud_crit"));
    s.push(Box::new(|w, _| {
        let mut v = vitals(w);
        v.health = 0.58;
        v.oxygen = Some(0.45);
        v.burn_level = 2;
        v.burning = false;
        true
    }));
    s.push(tap_then("hud: Tab opens the inventory", KeyCode::Tab, |w| w.resource::<InventoryUi>().open));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let h = hud(w);
        check(c, h.contrast == 1.0, "hud: open, the whole bar is at full contrast".into());
        true
    }));
    s.push(shot_step("hud_tab"));
    s.push(pointer_step(0, true));
    s.push(wait(0.2));
    s.push(pointer_step(3, true));
    s.push(wait(0.2));
    s.push(shot_step("hud_drag"));
    s.push(pointer_step(0, true));
    s.push(wait(0.1));
    s.push(pointer_step(0, false));
    s.push(wait(0.2));
    s.push(tap_then("hud: Tab closes it", KeyCode::Tab, |w| !w.resource::<InventoryUi>().open));
    s.push(Box::new(|w, _| {
        *vitals(w) = Vitals::default();
        true
    }));

    s.push(Box::new(|w, c| {
        begin(w, c, "hud: the compass in a flying ship");
        put_at_seat(w);
        true
    }));
    s.push(wait(0.3));
    s.extend(sit());
    s.push(hold_until("hud: lift off", &[KeyCode::Space, KeyCode::ShiftLeft], 40.0, |w| above_ground(w) > 8.0));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let up = with_player(w, |p| (p.ship.is_some(), p.seated));
        check(c, up == (true, false), format!("hud: standing in the cabin of the flying ship ({up:?})"));
        true
    }));
    s.extend(heading_step("cabin, nose is 0", 0.0, Reference::Bow, bow_axes));
    s.extend(heading_step("cabin, starboard is 90", 90.0, Reference::Bow, bow_axes));
    s.extend(heading_step("cabin, astern is 180", 180.0, Reference::Bow, bow_axes));
    s.extend(back_to_seat());
    s.push(land("hud: land"));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.extend(heading_step("landed ship: north", 0.0, Reference::True, planet_axes));
    s.extend(heading_step("landed ship: east", 90.0, Reference::True, planet_axes));
}

fn pointer_step(i: usize, down: bool) -> Step {
    Box::new(move |w, _| {
        let (win, bar) = (w.resource::<HudWindow>().0, w.resource::<DisplayLook>().0.hud.bar.clone());
        let (at, size) = slot_rect(i, true, win, &bar);
        let mut ctl = w.resource_mut::<Controls>();
        ctl.pointer = Some(at + Vec2::splat(size * 0.5));
        ctl.pointer_down = down;
        true
    })
}

fn tap_then_state(what: &'static str, k: KeyCode, done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Vec<Step> {
    vec![tap_then(what, k, done)]
}
