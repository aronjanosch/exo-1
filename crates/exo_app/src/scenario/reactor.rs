//! #286: the engineer's corner of the dev area, driven through `Controls` only. Walk to the reactor
//! console, F, power on, Auto until it holds steady; the power board sheds the weapons; a bad
//! manual setting overheats the reactor, starts a fire and scrams it; hold F to repair the broken
//! coolant pump; take the extinguisher, close the door to the side room, put the fire out; the
//! side room's fire (if it spread) starves behind the closed door; return the extinguisher, repair
//! the reactor if the fire broke it, restart it on Auto. Run with `--start=dev_area`.
use super::dev_area::stand_until_still;
use super::*;
use crate::dev_area::engineer::{self, area3, area_of, world_of, Engineering, DOOR_AT, EXTINGUISHER_AT, POWER_CONSOLE_AT, POWER_PANEL, PUMP_AT, REACTOR_AT, REACTOR_CONSOLE_AT, REACTOR_PANEL};
use crate::dev_area::{DevArea, ENGINEERING};
use crate::displays::{overlay_point_of, ActiveDisplay, DisplayPanel};
use crate::interact::Interaction;
use display_core::PanelId;
use reactor_core::{AlarmKind, Group, Mode};

fn eng(w: &World) -> &Engineering {
    w.resource::<Engineering>()
}

fn prompt(w: &World) -> String {
    w.resource::<Interaction>().prompt.clone()
}

fn frame(w: &World) -> crate::town::Frame {
    w.resource::<DevArea>().frame.expect("dev area frame")
}

/// The player's feet in the area frame.
fn me(w: &mut World) -> DVec3 {
    let (f, pl) = (frame(w), planet(w));
    let p = player_world(w);
    area_of(&f, &pl, p)
}

fn face(w: &mut World, local: [f64; 2], h: f64) {
    let (f, pl) = (frame(w), planet(w));
    face_towards(w, world_of(&f, &pl, area3(local, h)));
}

/// Face a point and look straight at it (heading and pitch), like a player aiming with the mouse.
fn look_at(w: &mut World, local: [f64; 2], h: f64) {
    let (f, pl) = (frame(w), planet(w));
    let target = world_of(&f, &pl, area3(local, h));
    face_towards(w, target);
    with_player(w, |p| {
        let up = p.world_up(walker_core::Frame::IDENTITY);
        let d = target - (p.w.pos + up * crate::walker::EYE_HEIGHT);
        let v = d.dot(up);
        p.pitch = v.atan2((d - up * v).length());
    });
}

fn display_of(w: &mut World, id: &PanelId) -> Option<Entity> {
    w.query::<(Entity, &DisplayPanel)>().iter(w).find(|(_, d)| &d.id == id).map(|(e, _)| e)
}

/// Walk (W) through zone-local waypoints, turning towards each.
fn walk(what: &'static str, path: &'static [[f64; 2]], limit: f64) -> Step {
    let mut i = 0usize;
    Box::new(move |w, c| {
        if c.t == 0.0 {
            i = 0;
        }
        let at = me(w);
        let t = area3(path[i], 0.0);
        if DVec2::new(t.x - at.x, t.z - at.z).length() < 0.3 {
            i += 1;
            if i == path.len() {
                keys(w, &[KeyCode::KeyW], false);
                check(c, true, format!("{what} ({:.1} s)", c.t));
                return true;
            }
        }
        face(w, path[i], 0.0);
        keys(w, &[KeyCode::KeyW], true);
        if c.t >= limit {
            keys(w, &[KeyCode::KeyW], false);
            check(c, false, format!("{what}: not there within {limit} s (at {:.1}, {:.1})", at.x - ENGINEERING.centre[0], at.z - ENGINEERING.centre[1]));
            return true;
        }
        false
    })
}

/// Face a point and wait until the prompt says `text`.
fn aim(what: &'static str, local: [f64; 2], h: f64, text: &'static str) -> Step {
    Box::new(move |w, c| {
        look_at(w, local, h);
        let p = prompt(w);
        if p.contains(text) {
            check(c, true, format!("{what}: {p:?}"));
            return true;
        }
        if c.t >= 1.0 {
            check(c, false, format!("{what}: the prompt says {p:?}, not '{text}'"));
            return true;
        }
        false
    })
}

/// Tap F and wait until `done` (at most 1 s).
fn tap_f(what: &'static str, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            tap(w, KeyCode::KeyF);
            return false;
        }
        if done(w) || c.t >= 1.0 {
            let ok = done(w);
            check(c, ok, format!("{what}{}", if ok { String::new() } else { ": not within 1 s".into() }));
            return true;
        }
        false
    })
}

fn use_panel(what: &'static str, id: PanelId) -> Step {
    tap_f(what, move |w| {
        let e = display_of(w, &id);
        e.is_some() && w.resource::<ActiveDisplay>().using() == e
    })
}

fn leave_panel(what: &'static str) -> Step {
    tap_f(what, |w| w.resource::<ActiveDisplay>().using().is_none())
}

/// Click a widget of a panel (overlay) and wait until `done` (at most 1 s).
fn click(what: String, panel: PanelId, action: &'static str, t: Option<f64>, mut done: impl FnMut(&Engineering) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            let at = overlay_point_of(w, &panel, action, t).unwrap_or_else(|| panic!("no widget '{action}' on {panel}"));
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
            return false;
        }
        if done(eng(w)) || c.t >= 1.0 {
            let ok = done(eng(w));
            check(c, ok, format!("{what}{}", if ok { format!(" ({:.2} s)", c.t) } else { ": not within 1 s".into() }));
            return true;
        }
        false
    })
}

/// Hold F until `done` or the limit.
fn hold_f(what: &'static str, limit: f64, mut done: impl FnMut(&Engineering) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            keys(w, &[KeyCode::KeyF], true);
        }
        let ok = done(eng(w));
        if ok || c.t >= limit {
            keys(w, &[KeyCode::KeyF], false);
            check(c, ok, format!("{what} ({:.1} s)", c.t));
            return true;
        }
        false
    })
}

/// Wait until `done` (polled every tick) or the limit.
fn until(what: &'static str, limit: f64, mut done: impl FnMut(&Engineering, f64) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        let ok = done(eng(w), c.t);
        if ok || c.t >= limit {
            check(c, ok, format!("{what} ({:.1} s)", c.t));
            return true;
        }
        false
    })
}

/// Stand south of the reactor console and use it.
const AT_REACTOR_CONSOLE: [f64; 2] = [REACTOR_CONSOLE_AT[0], REACTOR_CONSOLE_AT[1] - 1.3];
const AT_POWER_CONSOLE: [f64; 2] = [POWER_CONSOLE_AT[0], POWER_CONSOLE_AT[1] - 1.3];

pub(super) fn reactor_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        begin(w, c, "reactor: the engineer's corner");
        // This run is about the fire, not the burns (#287 health).
        w.resource_mut::<crate::dev_area::health::Medical>().no_harm = true;
        let Some(f) = w.resource::<DevArea>().frame else {
            check(c, false, "reactor: the dev area is on the planet".into());
            return true;
        };
        let pl = planet(w);
        let at = world_of(&f, &pl, area3([AT_REACTOR_CONSOLE[0], AT_REACTOR_CONSOLE[1] - 2.0], 1.0));
        place_walker(w, at);
        let e = eng(w);
        check(c, !e.reactor.power_on() && e.reactor.mode() == Mode::Manual && !e.pump.functioning(),
            format!("reactor: starts cold and off, {} rods, the pump broken at {:.0}", e.reactor.rods().len(), e.pump.value()));
        end(w, c, "placed south of the reactor console".into());
        true
    }));
    s.push(stand_until_still());
    s.push(walk("reactor: walk to the reactor console", &[AT_REACTOR_CONSOLE], 10.0));
    s.push(stand_until_still());
    s.push(aim("reactor: the prompt names the reactor", REACTOR_CONSOLE_AT, 1.6, engineer::REACTOR_NAME));
    s.push(use_panel("reactor: F uses the reactor panel", REACTOR_PANEL));
    s.push(click("reactor: POWER ON".into(), REACTOR_PANEL, "power", None, |e| e.reactor.power_on()));
    s.push(click("reactor: AUTO".into(), REACTOR_PANEL, "auto", None, |e| e.reactor.mode() == Mode::Auto));
    // Auto holds when the temperature is in the green and the output meets the load for 5 s.
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let r = &e.reactor;
        let load = e.grid.request_kw();
        let steady = r.bands().temp_optimal.contains(r.temperature()) && (r.output_kw() - load).abs() < 0.03 * load;
        let since = if steady { c.v.get("steady").copied().unwrap_or(c.t) } else { c.t };
        c.v.insert("steady", since);
        if c.t - since >= 5.0 || c.t >= 240.0 {
            let ok = c.t - since >= 5.0;
            let b = r.bands();
            check(c, ok, format!("reactor: Auto holds steady after {:.1} s: temp {:.1} (green {:.0}-{:.0}), output {:.0} of {load:.0} kW, fission {:.1}, turbine {:.1}",
                since, r.temperature(), b.temp_optimal.lo, b.temp_optimal.hi, r.output_kw(), r.fission(), r.turbine()));
            c.v.remove("steady");
            return true;
        }
        false
    }));
    s.push(shot_step("reactor-auto"));
    s.push(leave_panel("reactor: F leaves the reactor panel"));
    // The power board: weapons off frees 200 kW, the lights stay lit.
    s.push(walk("reactor: walk to the power board", &[AT_POWER_CONSOLE], 10.0));
    s.push(stand_until_still());
    s.push(aim("reactor: the prompt names the power board", POWER_CONSOLE_AT, 1.6, engineer::POWER_NAME));
    s.push(use_panel("reactor: F uses the power board", POWER_PANEL));
    s.push(Box::new(|w, c| {
        c.v.insert("asked", eng(w).grid.request_kw());
        true
    }));
    s.push(click("reactor: weapons limit to 0".into(), POWER_PANEL, "limit_weapons", Some(0.01), |e| e.grid.group(Group::Weapons).limit < 0.01));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let (before, now) = (c.v["asked"], e.grid.request_kw());
        let (weapons, aux) = (e.grid.factor(Group::Weapons), e.grid.factor(Group::Aux));
        check(c, (before - now - 200.0).abs() < 2.0 && weapons == 0.0 && aux > 0.9,
            format!("reactor: the board asks {before:.1} -> {now:.1} kW; weapons work {:.1} %, lights {:.1} %", weapons * 100.0, aux * 100.0));
        true
    }));
    s.push(shot_step("power-board"));
    s.push(leave_panel("reactor: F leaves the power board"));
    s.push(walk("reactor: back to the reactor console", &[AT_REACTOR_CONSOLE], 10.0));
    s.push(stand_until_still());
    s.push(aim("reactor: the prompt names the reactor again", REACTOR_CONSOLE_AT, 1.6, engineer::REACTOR_NAME));
    s.push(use_panel("reactor: F uses the reactor panel again", REACTOR_PANEL));
    // A bad manual setting: fission all the way up, turbine low.
    s.push(click("reactor: AUTO off".into(), REACTOR_PANEL, "auto", None, |e| e.reactor.mode() == Mode::Manual));
    s.push(click("reactor: fission slider to the top".into(), REACTOR_PANEL, "fission", Some(0.98), |e| e.reactor.targets().0 > 90.0));
    s.push(click("reactor: turbine slider to 30".into(), REACTOR_PANEL, "turbine", Some(0.3), |e| (e.reactor.targets().1 - 30.0).abs() < 5.0));
    s.push(Box::new(|w, c| {
        c.v.insert("t0", eng(w).time);
        true
    }));
    s.push(until("reactor: it overheats and starts a fire", 90.0, |e, _| e.stats.fires_started > 0 && !e.deck.fires().is_empty()));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        check(c, e.reactor.alarm(AlarmKind::TempHigh), format!("reactor: fire {:.1} s after the bad setting, temp {:.1}, the HOT alarm on", e.time - c.v["t0"], e.reactor.temperature()));
        true
    }));
    s.push(shot_step("reactor-fire"));
    s.push(until("reactor: the safety scrams it", 60.0, |e, _| e.stats.shutdowns > 0 && !e.reactor.power_on()));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let shown = w.resource::<crate::gameplay::Gameplay>().shown.iter().any(|l| l.key == "reactor.shutdown") || !w.resource::<crate::gameplay::Gameplay>().notices.is_empty();
        check(c, e.reactor.alarm(AlarmKind::Shutdown) && e.reactor.lockout_s() > 0.0 && shown,
            format!("reactor: scram {:.1} s after the bad setting, locked {:.1} s, alarm notice queued or shown {shown}", e.time - c.v["t0"], e.reactor.lockout_s()));
        true
    }));
    s.push(leave_panel("reactor: F leaves the reactor panel"));
    // The pump.
    const AT_PUMP: [f64; 2] = [PUMP_AT[0], PUMP_AT[1] - 2.1];
    s.push(walk("reactor: walk to the pump", &[[-0.4, 5.4], [-0.4, 7.8], AT_PUMP], 20.0));
    s.push(stand_until_still());
    s.push(aim("reactor: the prompt offers the pump", PUMP_AT, 0.7, "repair the coolant pump"));
    s.push(shot_step("pump-prompt"));
    s.push(Box::new(|w, c| {
        c.v.insert("mech", w.resource::<crate::dev_area::skills::DevSkills>().mechanical);
        true
    }));
    s.push(hold_f("reactor: hold F until the pump works", 10.0, |e| e.pump.functioning()));
    s.push(Box::new(|w, c| {
        let p = prompt(w);
        check(c, p.contains('#') && p.contains('%'), format!("reactor: the prompt shows the progress {p:?}"));
        true
    }));
    s.push(shot_step("pump-repair"));
    s.push(hold_f("reactor: hold F until the pump is fixed", 30.0, |e| e.pump.value() >= 100.0));
    s.push(Box::new(|w, c| {
        let gain = w.resource::<crate::dev_area::skills::DevSkills>().mechanical - c.v["mech"];
        let e = eng(w);
        check(c, e.pump.factor() == 1.0 && gain > 0.0 && e.stats.repairs_done == 1, format!("reactor: the pump at {:.0}, factor {:.2}, mechanical +{gain:.3}", e.pump.value(), e.pump.factor()));
        true
    }));
    // The extinguisher, the door, the fire.
    const AT_EXT: [f64; 2] = [EXTINGUISHER_AT[0], EXTINGUISHER_AT[1] - 1.3];
    s.push(walk("reactor: walk to the extinguisher", &[[-0.4, 7.8], [-0.4, 5.2], AT_EXT], 20.0));
    s.push(stand_until_still());
    s.push(aim("reactor: the prompt offers the extinguisher", EXTINGUISHER_AT, 0.9, "take the extinguisher"));
    s.push(tap_f("reactor: F takes the extinguisher", |w| eng(w).extinguisher.held));
    const AT_DOOR: [f64; 2] = [DOOR_AT[0], DOOR_AT[1] - 1.5];
    s.push(walk("reactor: walk to the door", &[[3.6, 5.2], [5.0, 7.0], AT_DOOR], 20.0));
    s.push(stand_until_still());
    s.push(aim("reactor: the prompt offers the door", DOOR_AT, 1.1, "close the door"));
    // With the door open the fire may spread (a chance every 5 s once it reaches the door).
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let spread = e.stats.spreads.first().map(|(t, _)| *t);
        if spread.is_some() || c.t >= 60.0 {
            let size: f64 = e.deck.fires_in(e.room).map(|f| f.size).sum();
            check(c, true, format!("reactor: with the door open the fire {} (fire {size:.1} m)", match spread {
                Some(t) => format!("spread to the side room at {t:.0} s"),
                None => "did not spread within 60 s".into(),
            }));
            return true;
        }
        false
    }));
    s.push(Box::new(|w, _| {
        look_at(w, engineer::FIRE_AT, 0.5);
        true
    }));
    s.push(wait(0.1));
    s.push(shot_step("fire-from-door"));
    s.push(aim("reactor: the prompt offers the door again", DOOR_AT, 1.1, "close the door"));
    s.push(tap_f("reactor: F closes the door", |w| !eng(w).door_open()));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let side = e.deck.fires_in(e.side_room).count();
        c.v.insert("spreads", e.stats.spreads.len() as f64);
        check(c, true, format!("reactor: door closed; spreads so far {}, fires in the side room {side}, side room oxygen {:.0} %", e.stats.spreads.len(), e.deck.room(e.side_room).oxygen_fraction() * 100.0));
        true
    }));
    // South-west of the fire, clear of the extinguisher's mount.
    const AT_FIRE: [f64; 2] = [1.0, 7.0];
    s.push(walk("reactor: walk to the fire", &[[5.0, 7.2], AT_FIRE], 20.0));
    s.push(Box::new(|w, _| {
        look_at(w, engineer::FIRE_AT, 0.5);
        true
    }));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let size: f64 = e.deck.fires_in(e.room).map(|f| f.size).sum();
        c.v.insert("size", size);
        check(c, size > 0.0, format!("reactor: the fire at the reactor is {size:.2} m, oxygen {:.0} %", e.deck.room(e.room).oxygen_fraction() * 100.0));
        true
    }));
    s.push(shot_step("fire"));
    s.push(hold_f("reactor: hold F, the spray puts the fire out", 30.0, |e| e.deck.fires_in(e.room).next().is_none()));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        check(c, e.extinguisher.charge > 0.0, format!("reactor: out after {:.1} s of spray, {:.0} % left in the extinguisher, burning {:.1} s", e.stats.sprayed_s, e.extinguisher.charge * 100.0, e.stats.burning_s));
        true
    }));
    s.push(until("reactor: behind the closed door the side room's fire starves, nothing spreads", 180.0, |e, _| e.deck.fires().is_empty()));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let n = e.stats.spreads.len() as f64 - c.v["spreads"];
        check(c, n == 0.0, format!("reactor: no spread since the door closed ({n}), side room oxygen {:.0} %", e.deck.room(e.side_room).oxygen_fraction() * 100.0));
        true
    }));
    // Back: return the extinguisher, repair the reactor if the fire broke it, restart.
    s.push(walk("reactor: back to the extinguisher's mount", &[[1.0, 5.2], AT_EXT], 20.0));
    s.push(stand_until_still());
    s.push(aim("reactor: the prompt offers to return it", EXTINGUISHER_AT, 0.9, "return the extinguisher"));
    s.push(tap_f("reactor: F returns the extinguisher", |w| !eng(w).extinguisher.held));
    const AT_REACTOR: [f64; 2] = [REACTOR_AT[0] + 1.0, REACTOR_AT[1] - 2.4];
    s.push(walk("reactor: walk to the reactor", &[[3.6, 5.2], [3.6, 7.6], AT_REACTOR], 20.0));
    s.push(stand_until_still());
    s.push(aim("reactor: the prompt offers the reactor", [REACTOR_AT[0] + 1.0, REACTOR_AT[1] - 1.5], 1.0, "repair the reactor"));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        check(c, true, format!("reactor: its condition after the fire {:.0}", e.reactor.condition().value()));
        true
    }));
    s.push(hold_f("reactor: hold F until the reactor is fully repaired", 30.0, |e| e.reactor.condition().value() >= 100.0));
    s.push(walk("reactor: back to the reactor console", &[[-0.4, 5.4], AT_REACTOR_CONSOLE], 20.0));
    s.push(stand_until_still());
    s.push(aim("reactor: the prompt names the reactor once more", REACTOR_CONSOLE_AT, 1.6, engineer::REACTOR_NAME));
    s.push(use_panel("reactor: F uses the reactor panel once more", REACTOR_PANEL));
    s.push(until("reactor: the lockout is over", 30.0, |e, _| e.reactor.lockout_s() <= 0.0));
    s.push(click("reactor: POWER ON again".into(), REACTOR_PANEL, "power", None, |e| e.reactor.power_on()));
    s.push(click("reactor: AUTO again".into(), REACTOR_PANEL, "auto", None, |e| e.reactor.mode() == Mode::Auto));
    s.push(until("reactor: it runs again and feeds the grid", 180.0, |e, _| (e.reactor.output_kw() - e.grid.request_kw()).abs() < 0.03 * e.grid.request_kw() && !e.reactor.alarm(AlarmKind::Shutdown)));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        check(c, true, format!("reactor: restarted, output {:.0} of {:.0} kW, temp {:.1}, fires started {}, scrams {}", e.reactor.output_kw(), e.grid.request_kw(), e.reactor.temperature(), e.stats.fires_started, e.stats.shutdowns));
        true
    }));
    s.push(shot_step("restarted"));
    s.push(leave_panel("reactor: F leaves the reactor panel at the end"));
}
