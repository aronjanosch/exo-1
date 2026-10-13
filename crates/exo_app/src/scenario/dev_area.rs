//! #285: the dev area and the test console. The walker starts on the level rectangle, walks to the
//! console, the prompt names it, F frees the cursor (the walker stands still with W held), three
//! clicks on + count to 3 and fill the bar to 0.3, the toggle switches, F leaves and the walker
//! walks again; the same with Escape. Then the two sample screens of the design system: the
//! warmer (S) counts up, the reactor (M) goes to manual and a high setpoint and reads hot, and the
//! reactor's screen in the world shows that live. Run with `--start=dev_area`.
use super::*;
use crate::dev_area::{look_samples, test_console, DevArea, Zone, ENGINEERING, HALF_EXTENT, LAB};
use crate::displays::{aim_at, world_point_of, ActiveDisplay, DisplayPanel, DisplayPointer, Panels};
use crate::interact::Interaction;

/// The test console's display entity.
pub(crate) fn console(w: &mut World) -> Option<Entity> {
    w.query::<(Entity, &DisplayPanel)>().iter(w).find(|(_, d)| d.id == test_console::ID).map(|(e, _)| e)
}

pub(crate) fn console_state(w: &World) -> test_console::TestConsole {
    w.resource::<test_console::TestConsole>().clone()
}

pub(crate) fn active(w: &World) -> ActiveDisplay {
    *w.resource::<ActiveDisplay>()
}

fn prompt(w: &World) -> String {
    w.resource::<Interaction>().prompt.clone()
}

/// Area coordinates (m east, m north of the area's centre) of a world point.
fn area_xy(w: &World, p: DVec3) -> [f64; 2] {
    let pl = planet(w);
    let f = w.resource::<DevArea>().frame.expect("dev area frame");
    let d = pl.up(p) - f.dir;
    [d.dot(f.east) * pl.radius, d.dot(f.north) * pl.radius]
}

/// The test console's display.
pub(crate) fn console_display(w: &mut World) -> DisplayPanel {
    let e = console(w).expect("the test console is spawned");
    w.get::<DisplayPanel>(e).unwrap().clone()
}

/// Point the cursor at a widget of the console's screen in the world.
pub(crate) fn aim_at_widget(w: &mut World, action: &str, t: Option<f64>) {
    let d = console_display(w);
    let at = world_point_of(w, &d, action, t).unwrap_or_else(|| panic!("no widget '{action}' on the console"));
    aim_at(w, at);
}

/// Point the cursor at a widget of the console and click.
pub(crate) fn click_on(w: &mut World, action: &str, t: Option<f64>) {
    aim_at_widget(w, action, t);
    w.resource_mut::<Controls>().click = true;
}

/// Put the walker `back` metres south of the console, facing it, then walk (W) until the prompt
/// names it and stand still.
pub(crate) fn approach(name: &'static str, back: f64) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            begin(w, c, name);
            let pl = planet(w);
            let area = w.resource::<DevArea>();
            let (at, _) = area.ground(&pl, LAB.point([test_console::AT[0], test_console::AT[1] - back]), 1.0).expect("dev area");
            place_walker(w, at);
            let e = console(w).expect("the test console is spawned");
            let target = w.get::<DisplayPanel>(e).unwrap().centre;
            face_towards(w, target);
            true
        }),
        hold_walk_until(&[KeyCode::KeyW], 10.0, |w| prompt(w).contains(test_console::NAME)),
        stand_until_still(),
        Box::new(move |w, c| {
            let p = prompt(w);
            check(c, p == format!("[F] use {}", test_console::NAME), format!("{name}: the prompt names the console ({p:?})"));
            let d = console_distance(w);
            end(w, c, format!("at the console, {d:.2} m from its screen"));
            true
        }),
    ]
}

fn console_distance(w: &mut World) -> f64 {
    let e = console(w).unwrap();
    let centre = w.get::<DisplayPanel>(e).unwrap().centre;
    let eye = player_world(w) + planet(w).up(player_world(w)) * crate::walker::EYE_HEIGHT;
    eye.distance(centre)
}

/// Hold keys until a condition holds or the time runs out (walker on foot).
pub(crate) fn hold_walk_until(ks: &'static [KeyCode], limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            keys(w, ks, true);
        }
        if done(w) || c.t >= limit {
            keys(w, ks, false);
            if c.t >= limit {
                check(c, false, format!("walking: no outcome within {limit} s"));
            }
            return true;
        }
        false
    })
}

/// Wait until the walker stands (speed below 5 cm/s), at most 3 s.
pub(crate) fn stand_until_still() -> Step {
    Box::new(|w, c| with_player(w, |p| p.w.vel.length()) < 0.05 && c.t > 0.1 || c.t > 3.0)
}

/// Tap a key and wait until a condition holds (at most `limit` s).
pub(crate) fn tap_until(k: KeyCode, what: &'static str, limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            tap(w, k);
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

/// F at the console: the display is in use, the cursor free.
pub(crate) fn use_console(what: &'static str) -> Step {
    tap_until(KeyCode::KeyF, what, 1.0, |w| {
        let e = console(w);
        active(w).using() == e && e.is_some() && w.resource::<Controls>().free_cursor
    })
}

/// A key held for 1 s moves the walker at least `min` m (or, with `min` 0, less than 1 cm).
fn walk_check(name: &'static str, k: KeyCode, moves: bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            c.p.insert("from", player_world(w));
            keys(w, &[k], true);
        }
        if c.t >= 1.0 {
            keys(w, &[k], false);
            let d = player_world(w).distance(c.p["from"]);
            if moves {
                check(c, d > 0.5, format!("{name}: {k:?} for 1 s walks ({d:.2} m)"));
            } else {
                check(c, d < 0.01, format!("{name}: {k:?} for 1 s does not move the walker ({:.1} mm)", d * 1000.0));
            }
            return true;
        }
        false
    })
}

/// Click a widget and wait until the console's state says so (at most 1 s).
pub(crate) fn click_until(action: &'static str, t: Option<f64>, what: String, mut done: impl FnMut(&test_console::TestConsole) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            click_on(w, action, t);
            return false;
        }
        let s = console_state(w);
        if done(&s) {
            check(c, true, format!("{what} ({:.2} s)", c.t));
            return true;
        }
        if c.t >= 1.0 {
            check(c, false, format!("{what}: not within 1 s ({s:?})"));
            return true;
        }
        false
    })
}

pub(super) fn dev_area_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        begin(w, c, "dev area: the level rectangle");
        let Some(frame) = w.resource::<DevArea>().frame else {
            check(c, false, "dev area: the place 'dev_area' is on the planet".into());
            return true;
        };
        let pl = planet(w);
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for i in -10..=10 {
            for j in -6..=6 {
                let (p, _) = frame.ground(&pl, i as f64 * HALF_EXTENT[0] / 10.0, j as f64 * HALF_EXTENT[1] / 10.0, 0.0);
                let h = (p - pl.centre).length() - pl.radius;
                (lo, hi) = (lo.min(h), hi.max(h));
            }
        }
        check(c, hi - lo < 0.05, format!("dev area: level across {:.0} x {:.0} m (heights differ by {:.3} m)", HALF_EXTENT[0] * 2.0, HALF_EXTENT[1] * 2.0, hi - lo));
        check(c, hi - pl.sea > 1.0, format!("dev area: dry ({:.1} m above the sea)", hi - pl.sea));
        let p = player_world(w);
        let xy = area_xy(w, p);
        let (grounded, agl) = (with_player(w, |p| p.w.grounded), pl.above_ground(p));
        check(c, xy[0].abs() < HALF_EXTENT[0] && xy[1].abs() < HALF_EXTENT[1] && grounded && agl.abs() < 0.1,
            format!("dev area: the walker starts on it (at {:.0} m east, {:.0} m north, {agl:.3} m above the ground, grounded {grounded})", xy[0], xy[1]));
        check(c, console(w).is_some(), "dev area: the test console stands in the lab zone".into());
        end(w, c, format!("level within {:.3} m", hi - lo));
        true
    }));
    s.push(shot_step("start"));
    s.extend(approach("dev area: walk to the console", 5.0));
    s.push(shot_step("console"));
    s.push(use_console("dev area: F uses the console"));
    s.push(walk_check("dev area: at the console", KeyCode::KeyW, false));
    for n in 1..=3u32 {
        s.push(click_until("inc", None, format!("dev area: click {n} on + counts to {n}"), move |st| st.count == n));
    }
    s.push(Box::new(|w, c| {
        let bar = w.resource::<Panels>().get(&test_console::ID).and_then(|p| {
            p.layout().into_iter().find_map(|x| if let display_core::Widget::Bar { value, .. } = x.widget { Some(*value) } else { None })
        });
        let count = console_state(w).count;
        check(c, count == 3 && bar.is_some_and(|b| (b - 0.3).abs() < 1e-9), format!("dev area: the counter reads {count}, the bar {bar:?}"));
        true
    }));
    s.push(click_until("lights", None, "dev area: the toggle switches the lights on".into(), |st| st.lights));
    s.push(Box::new(|w, _| {
        // Hover the + button for the picture.
        aim_at_widget(w, "inc", None);
        true
    }));
    s.push(wait(0.1));
    s.push(shot_step("console-world"));
    s.push(tap_until(KeyCode::KeyF, "dev area: F leaves the console", 1.0, |w| active(w) == ActiveDisplay::Idle && !w.resource::<Controls>().free_cursor));
    s.push(walk_check("dev area: after F", KeyCode::KeyS, true));
    // Again, leaving with Escape.
    s.extend(approach("dev area: back to the console", 3.0));
    s.push(use_console("dev area: F uses the console again"));
    s.push(tap_until(KeyCode::Escape, "dev area: Escape leaves the console", 1.0, |w| active(w) == ActiveDisplay::Idle && w.resource::<DisplayPointer>().0 == Default::default()));
    s.push(walk_check("dev area: after Escape", KeyCode::KeyS, true));
}

/// The display entity showing a panel.
fn display_of(w: &mut World, id: &display_core::PanelId) -> Option<Entity> {
    w.query::<(Entity, &DisplayPanel)>().iter(w).find(|(_, d)| &d.id == id).map(|(e, _)| e)
}

/// Put the walker 2.5 m south of a device, facing its display, walk until the prompt names it,
/// stand, and press F until the display is in use.
fn visit(name: &'static str, id: display_core::PanelId, zone: Zone, at: [f64; 2], label: &'static str) -> Vec<Step> {
    let id2 = id.clone();
    vec![
        Box::new(move |w, c| {
            begin(w, c, name);
            let pl = planet(w);
            let (p, _) = w.resource::<DevArea>().ground(&pl, zone.point([at[0], at[1] - 2.5]), 1.0).expect("dev area");
            place_walker(w, p);
            let e = display_of(w, &id).expect("the device is spawned");
            let target = w.get::<DisplayPanel>(e).unwrap().centre;
            face_towards(w, target);
            true
        }),
        hold_walk_until(&[KeyCode::KeyW], 10.0, move |w| prompt(w).contains(label)),
        stand_until_still(),
        tap_until(KeyCode::KeyF, name, 1.0, move |w| {
            let e = display_of(w, &id2);
            e.is_some() && active(w).using() == e
        }),
    ]
}

/// Point the cursor at a widget of a panel's screen in the world.
fn aim_panel(w: &mut World, id: &display_core::PanelId, action: &str, t: Option<f64>) {
    let e = display_of(w, id).unwrap_or_else(|| panic!("no display for {id}"));
    let d = w.get::<DisplayPanel>(e).unwrap().clone();
    let at = world_point_of(w, &d, action, t).unwrap_or_else(|| panic!("no widget '{action}' on {id}"));
    aim_at(w, at);
}

/// Point at a widget of a panel's screen and click it.
fn click_panel(id: display_core::PanelId, action: &'static str, t: Option<f64>) -> Step {
    Box::new(move |w, _| {
        aim_panel(w, &id, action, t);
        w.resource_mut::<Controls>().click = true;
        true
    })
}

pub(super) fn look_sample_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    use look_samples::{ReactorSample, Warmer, LAB_ID, REACTOR_ID};
    s.extend(visit("dev area: F uses the warmer (S screen)", LAB_ID, LAB, [-3.5, -4.5], "Warmer"));
    s.push(click_panel(LAB_ID, "temp_up", None));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let t = w.resource::<Warmer>().temp;
        check(c, t == 38, format!("dev area: + on the warmer reads {t} °C (38)"));
        aim_panel(w, &LAB_ID, "run", None);
        true
    }));
    s.push(wait(0.1));
    s.push(shot_step("lab-world"));
    s.push(tap_until(KeyCode::Escape, "dev area: Escape leaves the warmer", 1.0, |w| active(w) == ActiveDisplay::Idle));
    s.extend(visit("dev area: F uses the reactor panel (M screen)", REACTOR_ID, ENGINEERING, [-4.0, -4.5], "Reactor 2"));
    s.push(click_panel(REACTOR_ID, "mode", None));
    s.push(wait(0.2));
    s.push(click_panel(REACTOR_ID, "setpoint", Some(0.6)));
    s.push(wait(0.2));
    s.push(Box::new(|w, c| {
        let r = w.resource::<ReactorSample>().clone();
        let status = w.resource::<Panels>().get(&REACTOR_ID).and_then(|p| p.plate.clone()).and_then(|p| p.status).map(|s| s.0);
        check(c, !r.auto && r.setpoint == 60 && status.as_deref() == Some("HOT"), format!("dev area: manual at 60 % reads hot ({r:?}, {status:?})"));
        aim_panel(w, &REACTOR_ID, "scram", None);
        true
    }));
    s.push(wait(0.1));
    s.push(shot_step("reactor-cursor"));
    s.push(tap_until(KeyCode::Escape, "dev area: Escape leaves the reactor panel", 1.0, |w| active(w) == ActiveDisplay::Idle));
    // The screen in the world shows the panel live (window only; give it a redraw).
    s.push(wait(0.6));
    s.push(shot_step("reactor-world"));
}
