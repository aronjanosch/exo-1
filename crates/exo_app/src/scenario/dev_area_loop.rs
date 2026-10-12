//! #288 phase 3: the dev area as one crew loop, through `Controls` and the pointer (the walker is
//! placed in front of a console or device as test setup, `DevRole` says which role the player
//! plays). The reactor runs on Auto; the captain closes the door to the side room; the engineer
//! sets a bad manual setting; the reactor runs hot and starts a fire; the captain's displays show
//! it (reactor hot, fire in the engine room, oxygen falling) and the alert goes yellow by itself;
//! from the overview's alarm rows the captain orders the bot to put the fire out, which it does;
//! then to repair the pump (and the reactor if the fire broke it) at its skill, and to restart the
//! reactor; the status returns to normal and the captain calls green. On the way the captain
//! releases 100 credits to the engineer, who buys a rod pack with them. Run with
//! `--start=dev_area`.
use super::captain_displays::{click, crew, display_of, go_to, gp, logged, press};
use super::dev_area::{active, stand_until_still, tap_until};
use super::*;
use crate::dev_area::captain::live_status::{self, LiveShip, OrderMarker};
use crate::dev_area::captain::{bots, overview, ship_state, DevRole};
use crate::dev_area::engineer::{self, area3, world_of, Engineering, DOOR_AT, REACTOR_CONSOLE_AT, REACTOR_PANEL};
use crate::dev_area::{DevArea, ENGINEERING};
use crate::displays::{overlay_point_of, ActiveDisplay};
use crate::interact::Interaction;
use crew_core::orders::OrderKind;
use crew_core::status::{ReactorMode, ShipSystem, ShipStatus, TempBand};
use crew_core::{AlertCause, AlertLevel, Role, Target};
use display_core::PanelId;
use reactor_core::{AlarmKind, Mode};

fn eng(w: &World) -> &Engineering {
    w.resource::<Engineering>()
}

fn live(w: &World) -> ShipStatus {
    w.resource::<LiveShip>().get(w.resource::<crate::dev_area::captain::CrewState>())
}

fn bot(w: &World) -> bots::BotDemo {
    w.resource::<bots::BotDemo>().clone()
}

fn role(r: Role) -> Step {
    Box::new(move |w, _| {
        w.resource_mut::<DevRole>().0 = r;
        true
    })
}

/// Look straight at an engineering-zone point (heading and pitch).
fn look_at(w: &mut World, local: [f64; 2], h: f64) {
    let (f, pl) = (w.resource::<DevArea>().frame.expect("dev area"), planet(w));
    let target = world_of(&f, &pl, area3(local, h));
    face_towards(w, target);
    with_player(w, |p| {
        let up = p.world_up(walker_core::Frame::IDENTITY);
        let d = target - (p.w.pos + up * crate::walker::EYE_HEIGHT);
        let v = d.dot(up);
        p.pitch = v.atan2((d - up * v).length());
    });
}

/// Leave a display in use (if any), stand at an engineering-zone point and look at another.
fn stand_at(what: &'static str, at: [f64; 2], look: [f64; 2], h: f64) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            if active(w).using().is_some() {
                if c.t == 0.0 {
                    tap(w, KeyCode::KeyF);
                }
                return c.t > 0.3;
            }
            let (f, pl) = (w.resource::<DevArea>().frame.expect("dev area"), planet(w));
            place_walker(w, world_of(&f, &pl, area3(at, 1.0)));
            true
        }),
        stand_until_still(),
        Box::new(move |w, c| {
            look_at(w, look, h);
            check(c, true, format!("{what}: standing at the engineer corner ({:.1}, {:.1})", at[0], at[1]));
            true
        }),
    ]
}

/// Aim at a point until the prompt contains `text`, then tap F until `done`.
fn aim_and_tap(what: &'static str, at: [f64; 2], h: f64, text: &'static str, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            look_at(w, at, h);
            let p = w.resource::<Interaction>().prompt.clone();
            if p.contains(text) || c.t >= 1.0 {
                check(c, p.contains(text), format!("{what}: the prompt says {p:?}"));
                return true;
            }
            false
        }),
        tap_until(KeyCode::KeyF, what, 1.0, move |w| done(w)),
    ]
}

/// Use the reactor panel from in front of its console.
fn use_reactor_panel(what: &'static str) -> Vec<Step> {
    let mut s = stand_at(what, [REACTOR_CONSOLE_AT[0], REACTOR_CONSOLE_AT[1] - 1.3], REACTOR_CONSOLE_AT, 1.6);
    s.extend(aim_and_tap(what, REACTOR_CONSOLE_AT, 1.6, engineer::REACTOR_NAME, |w| {
        let e = display_of(w, &REACTOR_PANEL);
        e.is_some() && w.resource::<ActiveDisplay>().using() == e
    }));
    s
}

/// Click a widget (or a slider at `t`) and wait until `done`.
fn click_at(what: &'static str, panel: PanelId, action: &'static str, t: Option<f64>, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            let at = overlay_point_of(w, &panel, action, t).unwrap_or_else(|| panic!("no widget '{action}' on {panel}"));
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
            return false;
        }
        let ok = done(w);
        if ok || c.t >= 1.0 {
            check(c, ok, format!("{what} ({:.2} s)", c.t));
            return true;
        }
        false
    })
}

/// Wait until `done` or the limit; `note` says what was seen.
fn until(what: &'static str, limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static, note: impl Fn(&mut World, &Ctx) -> String + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        let ok = done(w);
        if ok || c.t >= limit {
            let n = note(w, c);
            check(c, ok, format!("{what} ({:.1} s): {n}", c.t));
            return true;
        }
        false
    })
}

/// The overview's alarm row whose order matches, clicked: the wheel takes its order.
fn alarm_order(what: &'static str, kind: OrderKind, target: Target) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            let rows = live_status::alarms(eng(w));
            let Some(i) = rows.iter().position(|a| a.order.as_ref().is_some_and(|(k, t)| *k == kind && *t == target)) else {
                check(c, false, format!("{what}: no alarm row orders {kind:?} {target:?} ({:?})", rows.iter().map(|a| &a.text).collect::<Vec<_>>()));
                return true;
            };
            let at = overlay_point_of(w, &overview::ID, &format!("alarm:{i}"), None).expect("the alarm row's button");
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
            return false;
        }
        let ui = w.resource::<overview::OverviewUi>().clone();
        let ship = live(w);
        let picked = overview::targets(ui.kind, &ship).get(ui.target).cloned();
        let ok = ui.kind == kind && picked.as_ref() == Some(&target);
        if ok || c.t >= 1.0 {
            check(c, ok, format!("{what}: the alarm row fills the wheel with {:?} {picked:?}", ui.kind));
            return true;
        }
        false
    })
}

/// A step that runs only when the flag `key` in `c.v` is set (1).
fn only_if(key: &'static str, mut step: Step) -> Step {
    Box::new(move |w, c| c.v.get(key).copied().unwrap_or(0.0) == 0.0 || step(w, c))
}

/// A step that runs only when the fire broke the reactor.
fn only_if_broken(step: Step) -> Step {
    only_if("reactor_broken", step)
}

fn give_to_bot(what: &'static str, kind: OrderKind, target: Target) -> Vec<Step> {
    vec![
        press(overview::ID, "to:3", |w| w.resource::<overview::OverviewUi>().to == crew_core::Addressee::Slot(Role::Tactical)),
        alarm_order(what, kind, target.clone()),
        click(overview::ID, "give".into(), format!("{what}: given to the tactical slot (the bot)"), 1.0, move |w| {
            crew(w).orders.for_role(Role::Tactical).is_some_and(|o| o.kind == kind && o.target.as_ref() == Some(&target))
        }),
        Box::new(|w, c| {
            c.v.insert("t_order", eng(w).time);
            true
        }),
    ]
}

pub(super) fn dev_area_loop_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    let engine_room = Target::Room(crew_core::RoomId::new(live_status::ENGINE_ROOM));
    let pump = Target::Device(crew_core::DeviceId::new(engineer::PUMP));
    let reactor = Target::Device(crew_core::DeviceId::new(engineer::REACTOR));
    let console = Target::Station(crew_core::StationId::new(live_status::REACTOR_CONSOLE));

    s.push(Box::new(|w, c| {
        begin(w, c, "loop: the dev area as one crew");
        let e = eng(w);
        let ok = !e.reactor.power_on() && !e.pump.functioning() && crew(w).alert.level() == AlertLevel::Green;
        check(c, ok, format!("loop: the reactor is off, the pump broken at {:.0}, the alert green; the bot at the turret: {}", e.pump.value(), bot(w).doing()));
        let l = live(w);
        check(c, l.reactor.output_kw == Some(0.0) && l.rooms.iter().any(|r| r.id.as_str() == live_status::SIDE_ROOM), format!("loop: the captain's status reads the real reactor ({:?} kW) and lists the side room", l.reactor.output_kw));
        end(w, c, "start".into());
        true
    }));
    s.push(role(Role::Engineer));
    // The reactor on Auto, until it holds.
    s.extend(use_reactor_panel("loop: the engineer uses the reactor panel"));
    s.push(click_at("loop: POWER ON", REACTOR_PANEL, "power", None, |w| eng(w).reactor.power_on()));
    s.push(click_at("loop: AUTO", REACTOR_PANEL, "auto", None, |w| eng(w).reactor.mode() == Mode::Auto));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let r = &e.reactor;
        let load = e.grid.request_kw();
        let steady = r.bands().temp_optimal.contains(r.temperature()) && (r.output_kw() - load).abs() < 0.03 * load;
        let since = if steady { c.v.get("steady").copied().unwrap_or(c.t) } else { c.t };
        c.v.insert("steady", since);
        if c.t - since >= 5.0 || c.t >= 240.0 {
            let ok = c.t - since >= 5.0;
            let l = live(w);
            check(c, ok && l.reactor.mode == ReactorMode::Running && l.reactor.band == TempBand::Normal,
                format!("loop: Auto holds after {since:.1} s, temp {:.1}, output {:.0} of {load:.0} kW; the captain sees {:?} {:?}", r.temperature(), r.output_kw(), l.reactor.mode, l.reactor.band));
            c.v.remove("steady");
            return true;
        }
        false
    }));
    // The captain closes the door to the side room first.
    s.push(role(Role::Captain));
    s.extend(stand_at("loop: the captain at the side room's door", [DOOR_AT[0], DOOR_AT[1] - 1.5], DOOR_AT, 1.1));
    s.extend(aim_and_tap("loop: the captain closes the door", DOOR_AT, 1.1, "close the door", |w| !eng(w).door_open()));
    // The engineer's bad setting.
    s.push(role(Role::Engineer));
    s.extend(use_reactor_panel("loop: the engineer back at the reactor"));
    s.push(click_at("loop: AUTO off", REACTOR_PANEL, "auto", None, |w| eng(w).reactor.mode() == Mode::Manual));
    s.push(click_at("loop: fission to the top", REACTOR_PANEL, "fission", Some(0.98), |w| eng(w).reactor.targets().0 > 90.0));
    s.push(click_at("loop: turbine to 30", REACTOR_PANEL, "turbine", Some(0.3), |w| (eng(w).reactor.targets().1 - 30.0).abs() < 5.0));
    s.push(Box::new(|w, c| {
        c.v.insert("t0", eng(w).time);
        true
    }));
    s.push(until("loop: the captain's status shows the reactor hot", 90.0, |w| matches!(live(w).reactor.band, TempBand::Hot | TempBand::Critical), |w, c| {
        let l = live(w);
        format!("{:?} at {:.0} %, {:.1} s after the bad setting", l.reactor.band, l.reactor.temp * 100.0, eng(w).time - c.v["t0"])
    }));
    s.push(until("loop: the alert goes yellow by itself", 5.0, |w| crew(w).alert.level() == AlertLevel::Yellow && crew(w).alert.log().last().is_some_and(|l| l.cause == AlertCause::Auto), |w, _| format!("{:?}", crew(w).alert.log().last())));
    s.push(until("loop: fire in the engine room", 90.0, |w| live(w).room(&crew_core::RoomId::new(live_status::ENGINE_ROOM)).is_some_and(|r| r.fire), |w, c| {
        let l = live(w);
        format!("{:.1} s after the bad setting; engineering bar fire {}, temp {:.0} %", eng(w).time - c.v["t0"], l.system(ShipSystem::Engineering).fire, l.reactor.temp * 100.0)
    }));
    s.push(until("loop: the safety scrams the reactor", 60.0, |w| eng(w).stats.shutdowns > 0, |w, c| format!("{:.1} s after the bad setting, the captain sees {:?}", eng(w).time - c.v["t0"], live(w).reactor.mode)));
    s.push(until("loop: the oxygen falls", 30.0, |w| live(w).oxygen.delta_per_s < -1e-4, |w, _| {
        let l = live(w);
        format!("ship O2 {:.1} %, change {:+.3} %/s", l.oxygen.level * 100.0, l.oxygen.delta_per_s * 100.0)
    }));

    // The captain: alarms on the overview, the order to the bot, money for the engineer.
    s.push(role(Role::Captain));
    s.extend(go_to("loop: the captain walks to the overview", overview::ID, overview::NAME, -21.0));
    s.push(Box::new(|w, c| {
        let rows = live_status::alarms(eng(w));
        let texts: Vec<_> = rows.iter().map(|a| format!("{} ({})", a.text, a.room)).collect();
        let panel_rows = w.resource::<crate::displays::Panels>().get(&overview::ID).map_or(0, |p| (0..rows.len()).filter(|i| p.find(&format!("alarm:{i}")).is_some()).count());
        check(c, rows.first().is_some_and(|a| a.text.starts_with("FIRE")) && panel_rows > 0, format!("loop: the overview lists the alarms {texts:?} ({panel_rows} rows on the panel)"));
        true
    }));
    s.push(wait(0.2));
    s.push(shot_step("overview-alarms"));
    s.extend(give_to_bot("loop: extinguish the engine room", OrderKind::Extinguish, engine_room.clone()));
    s.push(Box::new(|w, c| {
        // The order's marker shows for the role it is for (the player plays tactical for a tick).
        w.resource_mut::<DevRole>().0 = Role::Tactical;
        if c.t == 0.0 {
            return false;
        }
        let at = w.resource::<OrderMarker>().at;
        let want = live_status::target_point(&Target::Room(crew_core::RoomId::new(live_status::ENGINE_ROOM)));
        let line = crate::dev_area::captain::hud_order_line(gp(w), crew(w), Role::Tactical);
        check(c, at.is_some() && at == want && line.contains("engine room"), format!("loop: the tactical role sees the order {line:?} and a marker over the engine room {at:?}"));
        w.resource_mut::<DevRole>().0 = Role::Captain;
        true
    }));
    s.push(click(overview::ID, "rel:engineer".into(), "loop: the captain releases 100 credits to the engineer".into(), 1.0, |w| crew(w).budget.remaining(Role::Engineer) == 100));

    // The ship state during the fire; the engineer buys a rod pack.
    s.extend(go_to("loop: the captain at the ship state", ship_state::ID, ship_state::NAME, -7.0));
    s.push(Box::new(|w, c| {
        let p = w.resource::<crate::displays::Panels>().get(&ship_state::ID).cloned();
        let fire_bar = p.as_ref().is_some_and(|p| p.layout().iter().any(|x| matches!(x.widget, display_core::Widget::Bar { label, .. } if label.starts_with("ENGINEERING") && label.contains("FIRE"))));
        let l = live(w);
        let room = l.room(&crew_core::RoomId::new(live_status::ENGINE_ROOM)).cloned();
        check(c, fire_bar && room.as_ref().is_some_and(|r| r.fire) && l.oxygen.delta_per_s < 0.0,
            format!("loop: the ship state shows the fire (bar {fire_bar}), engine room O2 {:.0} %, ship O2 change {:+.3} %/s, reactor {:?}",
                room.map_or(0.0, |r| r.oxygen * 100.0), l.oxygen.delta_per_s * 100.0, l.reactor.mode));
        true
    }));
    s.push(wait(0.2));
    s.push(shot_step("ship-state-fire"));
    s.push(role(Role::Engineer));
    s.push(Box::new(|w, c| {
        c.v.insert("rods", eng(w).rods_in_crate as f64);
        c.v.insert("wallet", gp(w).progress.wallet() as f64);
        true
    }));
    s.push(click(ship_state::ID, "buy_rods".into(), "loop: the engineer buys a rod pack from the allowance".into(), 1.0, |w| logged(w, "captain: bought rods as Engineer")));
    s.push(wait(0.1));
    s.push(Box::new(|w, c| {
        let (rods, wallet) = (eng(w).rods_in_crate as f64 - c.v["rods"], c.v["wallet"] - gp(w).progress.wallet() as f64);
        check(c, rods == ship_state::ROD_PACK.0 as f64 && wallet == ship_state::ROD_PACK.1 as f64 && crew(w).budget.remaining(Role::Engineer) == 0,
            format!("loop: {rods} rods into the crate, {wallet} credits from the wallet, engineer allowance left {}", crew(w).budget.remaining(Role::Engineer)));
        true
    }));
    s.push(role(Role::Captain));

    // The bot at the fire.
    s.extend(stand_at("loop: watch the engine room", [-1.0, 2.0], engineer::FIRE_AT, 0.5));
    s.push(until("loop: the bot reaches the fire and sprays", 90.0, |w| bot(w).work.doing.as_deref() == Some("spraying"), |w, c| {
        let b = bot(w);
        format!("{} after the order: {}, at {:.1} m from the fire", eng(w).time - c.v["t_order"], b.doing(), {
            let f = ENGINEERING.point(engineer::FIRE_AT);
            ((b.pos[0] - f[0]).powi(2) + (b.pos[1] - f[1]).powi(2)).sqrt()
        })
    }));
    s.push(Box::new(|w, _| {
        look_at(w, engineer::FIRE_AT, 0.5);
        true
    }));
    s.push(wait(0.5));
    s.push(shot_step("bot-at-fire"));
    s.push(until("loop: the bot puts the fire out", 90.0, |w| eng(w).deck.fires().is_empty(), |w, c| {
        let (b, e) = (bot(w), eng(w));
        format!("{:.1} s after the order, {:.1} s of spray, its extinguisher at {:.0} %, side room fires {}, spreads {}", e.time - c.v["t_order"], b.work.sprayed_s, b.work.charge * 100.0, e.deck.fires_in(e.side_room).count(), e.stats.spreads.len())
    }));

    // The pump, and the reactor if the fire broke it.
    s.extend(go_to("loop: the captain back at the overview", overview::ID, overview::NAME, -21.0));
    s.extend(give_to_bot("loop: repair the pump", OrderKind::Repair, pump.clone()));
    s.push(until("loop: the bot repairs the pump by skill", 240.0, |w| eng(w).pump.value() >= 100.0, |w, c| {
        let (b, e) = (bot(w), eng(w));
        let full = e.pump.repair_seconds(bots::BOT_SKILL, device_core::Tool::WRENCH);
        format!("{:.1} s after the order, {:.1} s of hands-on work ({full:.1} s from 0 to full at skill {:.0}), {} failed tries, pump {:.0}", e.time - c.v["t_order"], b.work.repair_s, bots::BOT_SKILL, b.work.repair_fails, e.pump.value())
    }));
    s.push(Box::new(move |w, c| {
        let e = eng(w);
        c.v.insert("reactor_broken", if !e.reactor.condition().functioning() { 1.0 } else { 0.0 });
        check(c, true, format!("loop: the reactor's condition after the fire {:.0} (works {})", e.reactor.condition().value(), e.reactor.condition().functioning()));
        true
    }));
    // Only when the fire broke it: an order to repair the reactor first.
    for step in give_to_bot("loop: repair the broken reactor", OrderKind::Repair, reactor) {
        s.push(only_if_broken(step));
    }
    s.push(only_if_broken(until("loop: the bot repairs the broken reactor", 240.0, |w| eng(w).reactor.condition().functioning(), |w, c| {
        format!("reactor at {:.0}, {:.1} s after the order", eng(w).reactor.condition().value(), eng(w).time - c.v["t_order"])
    })));
    s.extend(give_to_bot("loop: restart the reactor", OrderKind::Operate, console));
    s.push(until("loop: the bot restarts the reactor on Auto after the lockout", 120.0, |w| eng(w).reactor.power_on() && eng(w).reactor.mode() == Mode::Auto, |w, c| {
        format!("{:.1} s after the order, restarts {}", eng(w).time - c.v["t_order"], bot(w).work.restarts)
    }));
    // The fire wore the reactor: an order to repair it to full while it runs.
    s.push(Box::new(|w, c| {
        let e = eng(w);
        c.v.insert("reactor_worn", if e.reactor.condition().needs_repair() { 1.0 } else { 0.0 });
        true
    }));
    for step in give_to_bot("loop: repair the worn reactor", OrderKind::Repair, Target::Device(crew_core::DeviceId::new(engineer::REACTOR))) {
        s.push(only_if("reactor_worn", step));
    }
    s.push(only_if("reactor_worn", until("loop: the bot repairs the reactor to full", 240.0, |w| eng(w).reactor.condition().value() >= 100.0, |w, c| {
        let b = bot(w);
        format!("{:.1} s after the order, {} repairs ({} failed tries)", eng(w).time - c.v["t_order"], b.work.repairs_done, b.work.repair_fails)
    })));
    s.push(until("loop: the status returns to normal", 240.0, |w| {
        let (e, l) = (eng(w), live(w));
        let load = e.grid.request_kw();
        l.reactor.mode == ReactorMode::Running && l.reactor.band == TempBand::Normal && !l.rooms.iter().any(|r| r.fire) && (e.reactor.output_kw() - load).abs() < 0.03 * load && !e.reactor.alarm(AlarmKind::Shutdown)
            && live_status::alarms(e).is_empty() && l.system(ShipSystem::Engineering).condition >= Some(18)
    }, |w, _| {
        let (e, l) = (eng(w), live(w));
        format!("reactor {:?} {:?}, output {:.0} of {:.0} kW, engineering {:?}/20, life support {:?}/20, ship O2 {:.0} % ({:+.3} %/s)", l.reactor.mode, l.reactor.band, e.reactor.output_kw(), e.grid.request_kw(),
            l.system(ShipSystem::Engineering).condition, l.system(ShipSystem::LifeSupport).condition, l.oxygen.level * 100.0, l.oxygen.delta_per_s * 100.0)
    }));
    s.push(click(overview::ID, "clear".into(), "loop: the captain takes the bot's order back".into(), 1.0, |w| crew(w).orders.for_role(Role::Tactical).is_none()));
    s.push(click(overview::ID, "alert:green".into(), "loop: the captain calls green".into(), 1.0, |w| crew(w).alert.level() == AlertLevel::Green));
    s.push(Box::new(|w, c| {
        let b = bot(w);
        let e = eng(w);
        check(c, true, format!("loop: done: fires started {}, scrams {}, spreads {}, bot sprayed {:.1} s, repaired {} ({} failed, {:.1} s), restarts {}; bot now: {}",
            e.stats.fires_started, e.stats.shutdowns, e.stats.spreads.len(), b.work.sprayed_s, b.work.repairs_done, b.work.repair_fails, b.work.repair_s, b.work.restarts, b.doing()));
        true
    }));
    s.push(shot_step("overview-normal"));
    s.push(tap_until(KeyCode::KeyF, "loop: F leaves the overview", 1.0, |w| active(w) == ActiveDisplay::Idle));
}
