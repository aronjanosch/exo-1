//! The pointer on a display: hover, press and release rules, the slider value.
use super::dev_area::{active, approach, click_on, console_state, use_console};
use super::*;
use crate::dev_area::test_console;
use crate::displays::{aim_at, aim_off, world_point, ActiveDisplay, DisplayPointer};

fn pointer(w: &World) -> display_core::Pointer {
    w.resource::<DisplayPointer>().0.clone()
}

fn point_at(w: &mut World, action: &str, t: Option<f64>) {
    super::dev_area::aim_at_widget(w, action, t);
}

fn set_down(w: &mut World, down: bool) {
    w.resource_mut::<Controls>().pointer_down = down;
}

fn act_until(what: &'static str, mut act: impl FnMut(&mut World) + Send + Sync + 'static, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            act(w);
            return false;
        }
        if done(w) {
            check(c, true, format!("{what} ({:.2} s)", c.t));
            return true;
        }
        if c.t >= 1.0 {
            check(c, false, format!("{what}: not within 1 s"));
            return true;
        }
        false
    })
}

pub(super) fn displays_steps(s: &mut Vec<Step>) {
    s.extend(approach("displays: walk to the console", 4.0));
    s.push(use_console("displays: F uses the console"));
    s.push(act_until("displays: the hint reads LEAVE while in use", |_w: &mut World| {}, |w| {
        let hud = w.resource::<crate::crew_hud::CrewHud>();
        let labels: Vec<&str> = hud.hints.iter().map(|h| display_core::hud::hint_label(h.action)).collect();
        labels == ["LEAVE"] && hud.hints.iter().all(|h| h.action == display_core::hud::HintAction::Leave)
    }));
    s.push(act_until("displays: hover names the + button", |w| point_at(w, "inc", None), |w| pointer(w).hover.as_deref() == Some("inc")));
    s.push(act_until("displays: the padding is no widget", |w| {
        let d = super::dev_area::console_display(w);
        let at = world_point(w, &d, bevy::math::DVec2::new(2.0, 2.0)).expect("the console's panel");
        aim_at(w, at);
    }, |w| pointer(w).at.is_some() && pointer(w).hover.is_none()));
    s.push(act_until("displays: a disabled button (- at zero) has no hover", |w| point_at(w, "dec", None), |w| pointer(w).at.is_some() && pointer(w).hover.is_none()));
    s.push(act_until("displays: press on +", |w| {
        point_at(w, "inc", None);
        set_down(w, true);
    }, |w| pointer(w).pressed.as_deref() == Some("inc")));
    s.push(act_until("displays: released over the toggle", |w| point_at(w, "lights", None), |w| pointer(w).hover.as_deref() == Some("lights")));
    s.push(act_until("displays: the release ends the press", |w| set_down(w, false), |w| pointer(w).pressed.is_none()));
    s.push(Box::new(|w, c| {
        let st = console_state(w);
        check(c, st.count == 0 && !st.lights, format!("displays: press and release elsewhere is no click (count {}, lights {})", st.count, st.lights));
        true
    }));
    s.push(act_until("displays: a click on the disabled - button", |w| click_on(w, "dec", None), |w| pointer(w).pressed.is_none() && !w.resource::<Controls>().click));
    s.push(wait(0.05));
    s.push(Box::new(|w, c| {
        let n = console_state(w).count;
        check(c, n == 0, format!("displays: the disabled button does not click (count {n})"));
        true
    }));
    s.push(act_until("displays: a click off the screen", |w| {
        aim_off(w);
        w.resource_mut::<Controls>().click = true;
    }, |w| !w.resource::<Controls>().click && pointer(w).pressed.is_none()));
    s.push(Box::new(|w, c| {
        let st = console_state(w);
        check(c, st == test_console::TestConsole::default(), format!("displays: the console is untouched ({st:?})"));
        true
    }));
    s.push(act_until("displays: a press at a quarter of the slider", |w| {
        point_at(w, "trim", Some(0.25));
        set_down(w, true);
    }, |w| (console_state(w).trim - 0.25).abs() < 0.01));
    s.push(act_until("displays: dragged to three quarters", |w| point_at(w, "trim", Some(0.75)), |w| (console_state(w).trim - 0.75).abs() < 0.01));
    s.push(act_until("displays: released, the value stays", |w| set_down(w, false), |w| pointer(w).pressed.is_none()));
    s.push(Box::new(|w, c| {
        let t = console_state(w).trim;
        check(c, (t - 0.75).abs() < 0.01, format!("displays: the slider reads {t:.3} (0.75)"));
        true
    }));
    s.push(act_until("displays: + clicks", |w| click_on(w, "inc", None), |w| console_state(w).count == 1));
    s.push(act_until("displays: - is enabled at 1 and clicks", |w| click_on(w, "dec", None), |w| console_state(w).count == 0));
    s.push(act_until("displays: Escape leaves", |w| tap(w, KeyCode::Escape), |w| active(w) == ActiveDisplay::Idle));
}
