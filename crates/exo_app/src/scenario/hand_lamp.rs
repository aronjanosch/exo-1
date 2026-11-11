//! The hand lamp (#329): it is given to the walker, switched on by LMB through `Controls` (the use at the host), the
//! state and a spot light for the walker appear; another bar slot selected, it is off and the light is gone; back
//! on the lamp it stays off until the next click, then on again; a remote figure whose public status says lamp on
//! gets a light too. Headless has no camera: the own light is posed from the walker's own eye, the same entity.
//! The picture (window run only): JB-2 blown, the walker in the dark corridor, the lamp on and pointed at the box.
use super::items::items;
use super::*;
use crate::controls::Controls;
use crate::dev_area::layout::{junction_centre, junction_front, JUNCTION_BOX_SIZE, JUNCTION_LIFT};
use crate::dev_area::ship::{ReactorSim, ShipEvents};
use crate::dev_area::DevArea;
use crate::hand_lamp::{own_on, LampBeam, LampOn};
use crate::items::{Items, LOCAL};
use item_core::wire::PublicStatus;
use walker_core::Frame as WalkFrame;

const DIGITS: [KeyCode; 8] = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8];
/// The box the picture is about: JB-2, the corridor's.
const BOX: usize = 1;

fn ground(w: &mut World, p: [f64; 2], lift: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, lift).expect("the dev area is there").0
}

fn look_box(w: &mut World) {
    let target = ground(w, junction_centre(BOX), JUNCTION_LIFT + JUNCTION_BOX_SIZE[1] * 0.5);
    with_player(w, |p| {
        let up = p.world_up(WalkFrame::IDENTITY);
        let eye = p.world_pos(WalkFrame::IDENTITY) + up * crate::walker::EYE_HEIGHT;
        let d = target - eye;
        let h = d - up * d.dot(up);
        if h.length() > 1e-9 {
            p.w.forward = h.normalize();
        }
        p.pitch = d.dot(up).atan2(h.length());
    });
}

fn beams(w: &mut World) -> Vec<u32> {
    let mut q = w.query::<&LampBeam>();
    let mut v: Vec<u32> = q.iter(w).map(|b| b.owner).collect();
    v.sort();
    v
}

fn on(w: &World) -> bool {
    own_on(w.resource::<Items>(), w.resource::<LampOn>())
}

fn run(name: &'static str, limit: f64, mut body: impl FnMut(&mut World, &mut Ctx) -> Option<String> + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
        }
        if let Some(note) = body(w, c) {
            end(w, c, note);
            return true;
        }
        if c.t > limit {
            check(c, false, format!("{name}: not done within {limit} s"));
            end(w, c, "timed out".into());
            return true;
        }
        false
    })
}

fn slot_of(w: &World, def: &str) -> usize {
    (0..8).find(|&i| items(w).client.view.slot(i).is_some_and(|s| s.item.kind.as_str() == def)).unwrap_or(0)
}

fn select(name: &'static str, def: &'static str) -> Step {
    let mut slot = 0;
    run(name, 2.0, move |w, c| {
        if c.t == 0.0 {
            slot = slot_of(w, def);
            tap(w, DIGITS[slot]);
            return None;
        }
        (items(w).client.view.selected == slot).then(|| format!("slot {} ({def})", slot + 1))
    })
}

/// One click at the lamp, then wait until the lamp state is `want` and the beam list says the same.
fn click_until(name: &'static str, want: bool) -> Step {
    run(name, 4.0, move |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Controls>().click = true;
            return None;
        }
        let there = beams(w).contains(&LOCAL);
        (on(w) == want && there == want).then(|| {
            check(c, true, format!("{name}: lamp {} and its light {}", if want { "on" } else { "off" }, if want { "exists" } else { "is gone" }));
            "done".into()
        })
    })
}

pub(super) fn hand_lamp_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        begin(w, c, "hand_lamp: the dev area and the host's body for the walker");
        w.resource_mut::<Items>().delay = super::items::DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && w.resource::<crate::health::HealthState>().bodies.contains_key(&LOCAL);
        if !ready && c.t < 5.0 {
            return false;
        }
        check(c, ready, "hand_lamp: the dev area and the walker's body are there".into());
        // JB-2 blown, so the corridor is dark for the picture; the walker stands before the box.
        let events = w.resource_mut::<ReactorSim>().room.blow_junction(BOX, false);
        w.resource_mut::<ShipEvents>().0.extend(events);
        let feet = ground(w, junction_front(BOX, 1.8), 0.0);
        place_walker(w, feet);
        // The lamp and a fuse (a second item for the slot change).
        let now = w.resource::<Time>().elapsed_secs_f64();
        let mut it = w.resource_mut::<Items>();
        for id in ["hand_lamp", "fuse"] {
            let stack = it.defs.get(id).unwrap_or_else(|| panic!("{id}.json")).stack_of(1);
            check(c, it.give(now, LOCAL, stack).is_ok(), format!("hand_lamp: the {id} fits in the bar"));
        }
        end(w, c, "lamp and fuse given".into());
        true
    }));
    s.push(wait(0.6));
    s.push(select("hand_lamp: the lamp is selected", "hand_lamp"));
    s.push(run("hand_lamp: off at first, no light", 2.0, |w, c| {
        look_box(w);
        (c.t > 0.5).then(|| {
            check(c, !on(w) && beams(w).is_empty(), "hand_lamp: the lamp starts off and no light exists".into());
            "off".into()
        })
    }));
    s.push(click_until("hand_lamp: LMB switches it on", true));
    s.push(run("hand_lamp: the light is posed at the walker's eye, in front of it", 3.0, |w, c| {
        look_box(w);
        if c.t < 0.3 {
            return None;
        }
        let eye = with_player(w, |p| p.world_pos(WalkFrame::IDENTITY) + p.world_up(WalkFrame::IDENTITY) * crate::walker::EYE_HEIGHT);
        let mut q = w.query::<(&LampBeam, &crate::origin::WorldPose)>();
        let beam = q.iter(w).find(|(b, _)| b.owner == LOCAL).map(|(_, p)| (p.pos, p.rot));
        let near = beam.is_some_and(|(p, _)| (p - eye).length() < 0.5);
        check(c, near, "hand_lamp: the own light is within half a metre of the eye".into());
        let aim = with_player(w, |p| p.world_look(WalkFrame::IDENTITY));
        let ahead = beam.is_some_and(|(_, r)| (r * DVec3::NEG_Z).dot(aim) > 0.99);
        check(c, ahead, "hand_lamp: the light points where the walker looks".into());
        Some("posed".into())
    }));
    // The picture, a few frames to let the beam and the lens appear.
    s.push(wait(0.6));
    s.push(shot_step("hand-lamp-jb2"));
    s.push(wait(0.6));
    s.push(select("hand_lamp: another slot is selected", "fuse"));
    s.push(run("hand_lamp: another slot puts the lamp away: off, no light", 4.0, |w, c| {
        (!on(w) && beams(w).is_empty()).then(|| {
            check(c, true, format!("hand_lamp: off and no light after {:.1} s", c.t));
            "off".into()
        })
    }));
    s.push(select("hand_lamp: back on the lamp", "hand_lamp"));
    s.push(run("hand_lamp: back on the lamp it stays off", 2.0, |w, c| {
        (c.t > 0.6).then(|| {
            check(c, !on(w) && beams(w).is_empty(), "hand_lamp: selecting the lamp again does not switch it on".into());
            "off".into()
        })
    }));
    s.push(click_until("hand_lamp: LMB switches it on again", true));
    s.push(click_until("hand_lamp: LMB switches it off", false));

    // A remote figure whose public status says lamp on (no network in a scenario: the status list is set).
    s.push(run("hand_lamp: a remote player with the lamp on gets a light", 4.0, |w, c| {
        if c.t == 0.0 {
            let at = player_world(w) + DVec3::new(0.0, 0.0, 0.0);
            let (fwd, up) = with_player(w, |p| (p.w.forward, p.up));
            w.spawn((crate::net_live::RemoteWalker { owner: 2 }, crate::origin::WorldPose { pos: at + fwd * 3.0, rot: walker_core::look_rot(-fwd, up) }, Transform::default(), Visibility::default()));
            w.resource_mut::<Items>().public = vec![(2, PublicStatus { lamp: true, ..Default::default() })];
            return None;
        }
        (beams(w) == vec![2]).then(|| {
            check(c, true, "hand_lamp: the remote player's light exists, the own is off".into());
            "remote on".into()
        })
    }));
    s.push(run("hand_lamp: the remote lamp off, the light is gone", 4.0, |w, c| {
        if c.t == 0.0 {
            w.resource_mut::<Items>().public = vec![(2, PublicStatus::default())];
            return None;
        }
        beams(w).is_empty().then(|| {
            check(c, true, "hand_lamp: no light left".into());
            "remote off".into()
        })
    }));
    // Frame times (a window run with `--perf --no-vsync`): the same view with the lamp off, then on; the phase names carry it.
    s.push(run("hand_lamp: frame time with the lamp off", 4.0, |w, c| {
        look_box(w);
        (c.t > 3.0).then(|| "measured".into())
    }));
    s.push(click_until("hand_lamp: LMB switches it on for the measure", true));
    s.push(run("hand_lamp: frame time with the lamp on", 5.0, |w, c| {
        look_box(w);
        (c.t > 3.0).then(|| "measured".into())
    }));
}
