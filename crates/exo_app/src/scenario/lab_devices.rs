//! #287: the scientist's lab in the dev area, through `Controls` and the pointer only. The walker
//! takes dream goo and space sugar from the ingredient shelf, brews them in the mixer and releases
//! the needle at the sweet spot, heats the batch in the band, spins it at the sweet speed, fills a
//! syringe and injects the test dummy. A second batch is burnt in the heater and comes out worse.
//! Run with `--start=dev_area`. Also the helpers the `cabinets` scenario uses.
use super::dev_area::{active, hold_walk_until, stand_until_still, tap_until};
use super::*;
use crate::dev_area::science::{self, Hands, LabItem, ScienceLab};
use crate::dev_area::skills::DevSkills;
use crate::dev_area::storage::{self, Storage};
use crate::displays::{overlay_point_of, ActiveDisplay, DisplayPanel, Panels};
use crate::interact::Interaction;
use display_core::PanelId;
use production_core::devices::Spin;
use production_core::Kettle;

/// The display entity of a panel.
pub(super) fn display_of(w: &mut World, id: &PanelId) -> Option<(Entity, DisplayPanel)> {
    w.query::<(Entity, &DisplayPanel)>().iter(w).find(|(_, d)| &d.id == id).map(|(e, d)| (e, d.clone()))
}

pub(super) fn lab(w: &World) -> &ScienceLab {
    w.resource::<ScienceLab>()
}

pub(super) fn hands(w: &World) -> &Hands {
    w.resource::<Hands>()
}

/// Leave the display in use (if any), stand 3 m in front of the panel `id`, walk up until the
/// prompt names it, and use it with F.
pub(super) fn go_use(id: PanelId) -> Vec<Step> {
    let (id1, id2, id3) = (id.clone(), id.clone(), id.clone());
    vec![
        Box::new(|w, c| {
            if active(w) == ActiveDisplay::Idle {
                return true;
            }
            if c.t == 0.0 {
                tap(w, KeyCode::KeyF);
            }
            c.t > 1.0
        }),
        Box::new(move |w, c| {
            let Some((_, d)) = display_of(w, &id1) else {
                check(c, false, format!("the display '{id1}' is spawned"));
                return true;
            };
            let up = planet(w).up(d.centre);
            let mut front = d.rot * DVec3::Z;
            front = (front - up * front.dot(up)).normalize();
            place_walker(w, d.centre + front * 3.0);
            face_towards(w, d.centre);
            true
        }),
        hold_walk_until(&[KeyCode::KeyW], 6.0, move |w| {
            let name = display_of(w, &id2).map(|(_, d)| d.name).unwrap_or_default();
            w.resource::<Interaction>().prompt.ends_with(&format!("use {name}"))
        }),
        stand_until_still(),
        Box::new(move |w, c| {
            if c.t == 0.0 {
                tap(w, KeyCode::KeyF);
                return false;
            }
            let e = display_of(w, &id3).map(|(e, _)| e);
            if e.is_some() && active(w).using() == e {
                check(c, true, format!("F uses the {id3} ({:.2} s)", c.t));
                return true;
            }
            if c.t >= 1.0 {
                let p = w.resource::<Interaction>().prompt.clone();
                check(c, false, format!("F uses the {id3}: not within 1 s (prompt {p:?})"));
                return true;
            }
            false
        }),
    ]
}

/// Point at a widget of a panel (overlay) and click it; `t` is a place along a slider.
pub(super) fn click(w: &mut World, id: &PanelId, action: &str, t: Option<f64>) -> bool {
    let Some(at) = overlay_point_of(w, id, action, t) else { return false };
    let mut c = w.resource_mut::<Controls>();
    c.pointer = Some(at);
    c.click = true;
    true
}

/// Click a widget, then wait until `done` holds (at most `limit` s).
pub(super) fn click_until(id: PanelId, action: impl Fn(&World) -> Option<String> + Send + Sync + 'static, t: Option<f64>, what: impl Into<String>, limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    let what = what.into();
    Box::new(move |w, c| {
        if c.t == 0.0 {
            let a = action(w);
            let ok = a.as_ref().is_some_and(|a| click(w, &id, a, t));
            if !ok {
                check(c, false, format!("{what}: no widget {a:?} on {id}"));
                return true;
            }
            return false;
        }
        if done(w) {
            check(c, true, format!("{what} ({:.2} s)", c.t));
            return true;
        }
        if c.t >= limit {
            check(c, false, format!("{what}: not within {limit} s"));
            return true;
        }
        false
    })
}

/// Take the first stack of `kind` out of a cabinet: page through its panel until the stack shows,
/// click its take, wait until the hands hold it (at most 3 s).
pub(super) fn take_from(id: PanelId, kind: &'static str, what: impl Into<String>) -> Step {
    let what = what.into();
    Box::new(move |w, c| {
        if c.t >= 3.0 {
            check(c, false, format!("{what}: not within 3 s"));
            c.v.remove("next_at");
            return true;
        }
        if hands_slot(w, kind).is_some() && c.v.contains_key("next_at") {
            check(c, true, format!("{what} ({:.2} s)", c.t));
            c.v.remove("next_at");
            return true;
        }
        if c.v.get("next_at").is_some_and(|&t| c.t < t) {
            return false;
        }
        let slot = {
            let Some(cab) = w.resource::<Storage>().get(&id) else { return false };
            (0..cab.container.len()).find(|&i| cab.container.slot(i).is_some_and(|s| s.item.kind.as_str() == kind))
        };
        let Some(slot) = slot else {
            check(c, false, format!("{what}: no {kind} in {id}"));
            return true;
        };
        let action = format!("take:{slot}");
        let visible = w.resource::<Panels>().get(&id).is_some_and(|p| p.find(&action).is_some());
        click(w, &id, if visible { &action } else { "next" }, None);
        c.v.insert("next_at", c.t + 0.1);
        false
    })
}

/// A fixed action id.
pub(super) fn act(a: &'static str) -> impl Fn(&World) -> Option<String> + Send + Sync + 'static {
    move |_| Some(a.to_string())
}

/// Wait until `done` holds (at most `limit` s).
pub(super) fn wait_until(what: &'static str, limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if done(w) {
            check(c, true, format!("{what} ({:.1} s)", c.t));
            return true;
        }
        if c.t >= limit {
            check(c, false, format!("{what}: not within {limit} s"));
            return true;
        }
        false
    })
}

/// The hands slot holding `kind`.
pub(super) fn hands_slot(w: &World, kind: &str) -> Option<usize> {
    let h = &hands(w).0;
    (0..h.len()).find(|&i| h.slot(i).is_some_and(|s| s.item.kind.as_str() == kind))
}

fn count_batches(w: &World) -> usize {
    let h = &hands(w).0;
    (0..h.len()).filter(|&i| matches!(h.slot(i).and_then(|s| s.item.payload.as_ref()), Some(LabItem::Batch(_)))).count()
}

/// The syringes in the hands: (quality, doses).
fn syringes(w: &World) -> Vec<(f64, u32)> {
    let h = &hands(w).0;
    (0..h.len())
        .filter_map(|i| match h.slot(i).and_then(|s| s.item.payload.as_ref()) {
            Some(LabItem::Syringe(y)) => Some((y.product.quality, y.doses)),
            _ => None,
        })
        .collect()
}

/// The first syringe in the hands with `doses` doses: (quality, doses).
fn syringe_with(w: &World, doses: u32) -> Option<(f64, u32)> {
    syringes(w).into_iter().find(|s| s.1 == doses)
}

/// Mixer: dream goo and space sugar, brew, release when the needle is at the sweet spot, take.
fn brew(s: &mut Vec<Step>, tag: &'static str) {
    s.extend(go_use(science::MIXER));
    for kind in ["dream_goo", "space_sugar"] {
        s.push(click_until(science::MIXER, move |w| hands_slot(w, kind).map(|i| format!("add:{i}")), None, format!("{tag}: {kind} goes into the mixer"), 1.0, move |w| match lab(w).kettle() {
            Kettle::Loaded { base, ingredients } => base.as_str() == kind || ingredients.iter().any(|i| i.as_str() == kind),
            _ => false,
        }));
    }
    s.push(click_until(science::MIXER, act("start"), None, format!("{tag}: the brew starts"), 1.0, |w| matches!(lab(w).kettle(), Kettle::Brewing { .. })));
    s.push(wait_until("the brew is done", 30.0, |w| matches!(lab(w).kettle(), Kettle::Done { .. })));
    // The click acts one tick later: release when the needle will be at the sweet spot then.
    s.push(Box::new(move |w, c| {
        let l = lab(w);
        let (t, tun) = (l.needle_t, l.lc.tuning.clone());
        let next = production_core::needle(t + crate::TICK.as_secs_f64(), tun.needle_period_s);
        if !c.v.contains_key("clicked") && (next - tun.sweet_spot).abs() < 0.011 {
            c.v.insert("clicked", 1.0);
            click(w, &science::MIXER, "release", None);
            return false;
        }
        if let Kettle::Ready { product, .. } = lab(w).kettle() {
            check(c, product.quality > 0.9, format!("{tag}: released at the sweet spot, needle quality {:.3}", product.quality));
            c.v.remove("clicked");
            return true;
        }
        if c.t > 3.0 {
            check(c, false, format!("{tag}: the release did not happen within 3 s"));
            c.v.remove("clicked");
            return true;
        }
        false
    }));
    s.push(click_until(science::MIXER, act("take"), None, format!("{tag}: the batch goes into the hands"), 1.0, |w| count_batches(w) == 1 && matches!(lab(w).kettle(), Kettle::Empty)));
}

pub(super) fn lab_devices_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        begin(w, c, "lab devices: the lab and its storage");
        let ok = [science::MIXER, science::HEATER, science::CENTRIFUGE, science::FILLER, science::DUMMY, storage::SHELF].iter().all(|id| display_of(w, id).is_some());
        check(c, ok, "lab devices: the five lab devices and the shelf stand with their displays".into());
        c.v.insert("skill0", w.resource::<DevSkills>().medical);
        true
    }));
    // Goods from the shelf.
    s.extend(go_use(storage::SHELF));
    s.push(shot_step("shelf"));
    for kind in ["dream_goo", "space_sugar"] {
        s.push(take_from(storage::SHELF, kind, format!("lab devices: {kind} from the shelf into the hands")));
    }
    // The good run.
    brew(s, "good run");
    s.push(shot_step("mixer"));
    s.extend(go_use(science::HEATER));
    s.push(click_until(science::HEATER, act("put"), None, "good run: the batch goes into the heater", 1.0, |w| lab(w).heater.batch().is_some()));
    s.push(Box::new(|w, c| {
        let t = lab(w).tuning.clone();
        let target = (t.heater.band_low_c + t.heater.band_high_c) * 0.5;
        let v = (target - t.ambient_c) / (t.heater.max_setpoint_c - t.ambient_c);
        click(w, &science::HEATER, "setpoint", Some(v));
        c.v.insert("target", target);
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        let sp = lab(w).heater.setpoint();
        check(c, (sp - c.v["target"]).abs() < 1.5, format!("good run: the slider sets the heater to {sp:.1} C"));
        true
    }));
    s.push(wait_until("good run: the batch is heated through in the band", 40.0, |w| lab(w).heater.batch().is_some_and(|b| b.heat >= 1.0)));
    s.push(shot_step("heater"));
    s.push(Box::new(|w, c| {
        let b = lab(w).heater.batch().unwrap();
        check(c, !b.burnt, format!("good run: not burnt (peak {:.1} C)", lab(w).heater.peak_c()));
        true
    }));
    s.push(click_until(science::HEATER, act("take"), None, "good run: the batch back into the hands", 1.0, |w| count_batches(w) == 1));
    s.extend(go_use(science::CENTRIFUGE));
    s.push(click_until(science::CENTRIFUGE, act("put"), None, "good run: the batch goes into the centrifuge", 1.0, |w| lab(w).centrifuge.batch().is_some()));
    s.push(Box::new(|w, _| {
        let t = lab(w).tuning.centrifuge.clone();
        click(w, &science::CENTRIFUGE, "rpm", Some((t.sweet_low_rpm + t.sweet_high_rpm) * 0.5 / t.max_rpm));
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, _| {
        click(w, &science::CENTRIFUGE, "time", Some(6.0 / science::MAX_SPIN_S));
        true
    }));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        let l = lab(w);
        let (lo, hi) = (l.tuning.centrifuge.sweet_low_rpm, l.tuning.centrifuge.sweet_high_rpm);
        check(c, l.spin_rpm > lo && l.spin_rpm < hi && (l.spin_s - 6.0).abs() < 0.3, format!("good run: the sliders set {:.0} rpm for {:.1} s", l.spin_rpm, l.spin_s));
        true
    }));
    s.push(click_until(science::CENTRIFUGE, act("start"), None, "good run: the centrifuge spins", 1.0, |w| lab(w).centrifuge.state() == Spin::Spinning));
    s.push(wait(1.0));
    s.push(shot_step("centrifuge"));
    s.push(wait_until("good run: the run ends", 10.0, |w| lab(w).centrifuge.state() != Spin::Spinning));
    s.push(Box::new(|w, c| {
        let ce = &lab(w).centrifuge;
        let sep = ce.batch().map(|b| b.separation).unwrap_or(0.0);
        check(c, ce.state() == Spin::Done && sep > 0.99, format!("good run: separated {:.0} % without a trip ({:?})", sep * 100.0, ce.state()));
        true
    }));
    s.push(click_until(science::CENTRIFUGE, act("take"), None, "good run: the batch back into the hands", 1.0, |w| count_batches(w) == 1));
    s.extend(go_use(science::FILLER));
    s.push(click_until(science::FILLER, act("put"), None, "good run: the batch goes into the filler", 1.0, |w| lab(w).filler.is_some()));
    s.push(click_until(science::FILLER, act("fill"), None, "good run: a syringe is filled", 1.0, |w| !syringes(w).is_empty()));
    s.push(shot_step("filler"));
    s.push(Box::new(|w, c| {
        let (q, doses) = syringes(w)[0];
        let left = lab(w).filler.as_ref().map(|b| b.volume).unwrap_or(0.0);
        c.v.insert("good", q);
        check(c, q > 0.8 && doses == lab(w).tuning.syringe.capacity, format!("good run: the syringe holds {doses} doses at quality {q:.3}; {left:.2} doses left in the filler"));
        true
    }));
    s.extend(go_use(science::DUMMY));
    s.push(click_until(science::DUMMY, act("inject"), None, "good run: the dummy gets a dose", 1.0, |w| lab(w).last_dose.is_some()));
    s.push(shot_step("dummy"));
    s.push(Box::new(|w, c| {
        let d = lab(w).last_dose.clone().unwrap();
        let left = syringe_with(w, 2).map(|x| x.1);
        let fx: Vec<&str> = d.effects.iter().map(|e| e.as_str()).collect();
        check(c, d.strength > 0.5 && left == Some(2), format!("good run: dose {fx:?} at strength {:.2} (success {}), {left:?} doses left", d.strength, d.success));
        true
    }));
    // The bad run: the same brew, burnt in the heater, straight into the filler.
    brew(s, "bad run");
    s.extend(go_use(science::HEATER));
    s.push(click_until(science::HEATER, act("put"), None, "bad run: the batch goes into the heater", 1.0, |w| lab(w).heater.batch().is_some()));
    s.push(Box::new(|w, _| {
        click(w, &science::HEATER, "setpoint", Some(0.98));
        true
    }));
    s.push(wait_until("bad run: full heat burns the batch", 30.0, |w| lab(w).heater.batch().is_some_and(|b| b.burnt)));
    s.push(shot_step("burnt"));
    s.push(Box::new(|w, _| {
        click(w, &science::HEATER, "setpoint", Some(0.0));
        true
    }));
    s.push(wait(0.1));
    s.push(click_until(science::HEATER, act("take"), None, "bad run: the burnt batch into the hands", 1.0, |w| count_batches(w) == 1));
    s.extend(go_use(science::FILLER));
    s.push(click_until(science::FILLER, act("discard"), None, "bad run: the good rest is poured out", 1.0, |w| lab(w).filler.is_none()));
    s.push(click_until(science::FILLER, act("put"), None, "bad run: the burnt batch goes into the filler", 1.0, |w| lab(w).filler.is_some()));
    s.push(click_until(science::FILLER, act("fill"), None, "bad run: a syringe is filled", 1.0, |w| syringe_with(w, 3).is_some()));
    s.push(Box::new(|w, c| {
        let (q, _) = syringe_with(w, 3).unwrap();
        let good = c.v["good"];
        check(c, q < good - 0.2, format!("bad run: the burnt syringe is worse: quality {q:.3} against {good:.3}"));
        let (k0, k1) = (c.v["skill0"], w.resource::<DevSkills>().medical);
        check(c, k1 > k0 && k1 <= 100.0, format!("lab devices: medical skill rose from {k0:.2} to {k1:.2}"));
        let panel_ok = w.resource::<Panels>().get(&science::FILLER).is_some();
        end(w, c, format!("good syringe {good:.3}, burnt {q:.3}, panels written {panel_ok}"));
        true
    }));
    s.push(tap_until(KeyCode::KeyF, "lab devices: F leaves the filler", 1.0, |w| active(w) == ActiveDisplay::Idle));
}
