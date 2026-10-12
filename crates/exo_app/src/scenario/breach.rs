//! #286: hits, breaches and life support in the engineer's corner, driven through `Controls`.
//! The reactor runs on Auto; at the hit simulator a heavy collision tears the engineering room's
//! hull and heavy energy hits start a fire; the air falls; the door is closed so the side room
//! keeps its air; a patch kit from the box, hold F at the breach until it closes (time by skill);
//! the extinguisher puts the fire out; the oxygen generator is repaired and refills the room; the
//! pump and the reactor, hurt by the hits, are repaired. Run with `--start=dev_area`.
use super::dev_area::stand_until_still;
use super::reactor::{aim, click, eng, hold_f, leave_panel, look_at, me, tap_f, until, use_panel, walk, AT_REACTOR_CONSOLE};
use super::*;
use crate::dev_area::engineer::{self, area3, world_of, Engineering, DOOR_AT, ENGINE_SECTION_AT, EXTINGUISHER_AT, GENERATOR_AT, HIT_PANEL, HIT_SIM_AT, KITS_AT, PUMP_AT, REACTOR_AT, REACTOR_CONSOLE_AT, REACTOR_PANEL};
use crate::dev_area::DevArea;
use reactor_core::Mode;

fn o2(e: &Engineering) -> (f64, f64) {
    (e.deck.room(e.room).oxygen_fraction(), e.deck.room(e.side_room).oxygen_fraction())
}

fn breached(e: &Engineering) -> bool {
    e.hull.breached(e.engine_section)
}

fn fire_here(e: &Engineering) -> bool {
    e.deck.fires_in(e.room).next().is_some()
}

const AT_HIT_SIM: [f64; 2] = [HIT_SIM_AT[0], HIT_SIM_AT[1] - 1.3];
const AT_DOOR: [f64; 2] = [DOOR_AT[0], DOOR_AT[1] - 1.5];
const AT_KITS: [f64; 2] = [-7.3, KITS_AT[1]];
const AT_SECTION: [f64; 2] = [-2.2, 11.9];
const AT_EXT: [f64; 2] = [EXTINGUISHER_AT[0], EXTINGUISHER_AT[1] - 1.3];
const AT_GENERATOR: [f64; 2] = [GENERATOR_AT[0], GENERATOR_AT[1] - 1.3];
const AT_PUMP: [f64; 2] = [PUMP_AT[0], PUMP_AT[1] - 2.1];
const AT_REACTOR: [f64; 2] = [REACTOR_AT[0] + 1.0, REACTOR_AT[1] - 2.4];

pub(super) fn breach_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        begin(w, c, "breach: hull, air and life support");
        // This run is about the hull and the air, not the crew's health (#287 health has its own).
        w.resource_mut::<crate::dev_area::health::Medical>().no_harm = true;
        let Some(f) = w.resource::<DevArea>().frame else {
            check(c, false, "breach: the dev area is on the planet".into());
            return true;
        };
        let pl = planet(w);
        place_walker(w, world_of(&f, &pl, area3([AT_REACTOR_CONSOLE[0], AT_REACTOR_CONSOLE[1] - 2.0], 1.0)));
        let e = eng(w);
        check(c, !breached(e) && o2(e).0 > 0.999 && o2(e).1 > 0.999 && e.patch_kits > 0, format!("breach: the hull is tight, air full, {} patch kits", e.patch_kits));
        end(w, c, "placed south of the reactor console".into());
        true
    }));
    // Power for life support.
    s.push(stand_until_still());
    s.push(walk("breach: walk to the reactor console", &[AT_REACTOR_CONSOLE], 10.0));
    s.push(stand_until_still());
    s.push(aim("breach: the prompt names the reactor", REACTOR_CONSOLE_AT, 1.6, engineer::REACTOR_NAME));
    s.push(use_panel("breach: F uses the reactor panel", REACTOR_PANEL));
    s.push(click("breach: POWER ON".into(), REACTOR_PANEL, "power", None, |e| e.reactor.power_on()));
    s.push(click("breach: AUTO".into(), REACTOR_PANEL, "auto", None, |e| e.reactor.mode() == Mode::Auto));
    s.push(until("breach: the reactor feeds the grid", 240.0, |e, _| (e.reactor.output_kw() - e.grid.request_kw()).abs() < 0.03 * e.grid.request_kw()));
    s.push(leave_panel("breach: F leaves the reactor panel"));
    // The hits.
    s.push(walk("breach: walk to the hit simulator", &[[3.6, 5.2], AT_HIT_SIM], 20.0));
    s.push(stand_until_still());
    s.push(aim("breach: the prompt names the hit simulator", HIT_SIM_AT, 1.6, engineer::HIT_NAME));
    s.push(use_panel("breach: F uses the hit simulator", HIT_PANEL));
    s.push(click("breach: pick the engine room".into(), HIT_PANEL, "sec_engine", None, |e| e.hit_pick.section == e.engine_section));
    s.push(click("breach: pick collision".into(), HIT_PANEL, "type_collision", None, |e| e.hit_pick.kind == hull_core::DamageType::Collision));
    s.push(click("breach: pick heavy".into(), HIT_PANEL, "str_heavy", None, |e| e.hit_pick.strength == hull_core::Strength::Heavy));
    s.push(click("breach: FIRE a heavy collision".into(), HIT_PANEL, "fire", None, |e| e.stats.hits == 1));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let r = e.room_readout(e.room);
        check(c, r.breach && r.breach_open > 0.35, format!("breach: {} -> breach {:.0} % open, {:.1} m³/s out", e.last_hit, r.breach_open * 100.0, r.leak_flow));
        c.v.insert("o2_0", r.oxygen);
        true
    }));
    s.push(shot_step("hit-simulator"));
    // Heavy energy hits until one starts a fire (seeded rolls), at most four.
    s.push(click("breach: pick energy".into(), HIT_PANEL, "type_energy", None, |e| e.hit_pick.kind == hull_core::DamageType::Energy));
    s.push(Box::new(|w, c| {
        if fire_here(eng(w)) {
            let e = eng(w);
            check(c, true, format!("breach: a fire after {} hits ({}); reactor {:.0}, pump {:.0}, generator {:.0}", e.stats.hits, e.last_hit, e.reactor.condition().value(), e.pump.value(), e.generator.condition.value()));
            return true;
        }
        let n = c.v.get("energy").copied().unwrap_or(0.0);
        if n >= 4.0 {
            check(c, false, "breach: no fire after four heavy energy hits".into());
            return true;
        }
        if c.t >= n * 0.5 {
            c.v.insert("energy", n + 1.0);
            let at = crate::displays::overlay_point_of(w, &HIT_PANEL, "fire", None).expect("the fire button");
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
        }
        false
    }));
    s.push(wait(5.0));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let r = e.room_readout(e.room);
        check(c, r.oxygen < c.v["o2_0"] && r.oxygen_delta < 0.0, format!("breach: the engine room's air {:.1} % -> {:.1} % ({:+.2} %/s), side room {:.1} %", c.v["o2_0"] * 100.0, r.oxygen * 100.0, r.oxygen_delta * 100.0, o2(e).1 * 100.0));
        true
    }));
    s.push(leave_panel("breach: F leaves the hit simulator"));
    // Contain it: close the door.
    s.push(walk("breach: walk to the door", &[[5.0, 5.2], AT_DOOR], 20.0));
    s.push(stand_until_still());
    s.push(aim("breach: the prompt offers the door", DOOR_AT, 1.1, "close the door"));
    s.push(tap_f("breach: F closes the door", |w| !eng(w).door_open()));
    s.push(Box::new(|w, c| {
        c.v.insert("side", o2(eng(w)).1);
        true
    }));
    s.push(wait(3.0));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let (a, b) = o2(e);
        check(c, b >= c.v["side"] - 0.005, format!("breach: behind the closed door the side room keeps its air ({:.1} % -> {:.1} %); engine room {:.1} %", c.v["side"] * 100.0, b * 100.0, a * 100.0));
        true
    }));
    // A patch kit, then patch the breach.
    s.push(walk("breach: walk to the kit box", &[[3.6, 5.2], [-7.3, 5.2], AT_KITS], 30.0));
    s.push(stand_until_still());
    s.push(aim("breach: the prompt offers a patch kit", KITS_AT, 0.3, "take a patch kit"));
    s.push(tap_f("breach: F takes a patch kit", |w| eng(w).kit_in_hand));
    // A look at the breach and its fire from here, for the picture.
    s.push(Box::new(|w, _| {
        look_at(w, ENGINE_SECTION_AT, engineer::SECTION_HEIGHT);
        true
    }));
    s.push(wait(0.1));
    s.push(shot_step("breach-from-kits"));
    s.push(walk("breach: walk to the breach", &[[-7.3, 11.9], AT_SECTION], 20.0));
    s.push(stand_until_still());
    s.push(Box::new(|w, c| {
        let at = me(w) + DVec3::Y;
        let e = eng(w);
        let pull = e.hull.pull(&e.deck, e.room, at);
        check(c, pull.length() > 0.0, format!("breach: at the hole the air pulls with {:.2} m/s²", pull.length()));
        true
    }));
    s.push(aim("breach: the prompt offers the patch", ENGINE_SECTION_AT, engineer::SECTION_HEIGHT, "patch the hull"));
    s.push(shot_step("breach"));
    s.push(Box::new(|w, c| {
        c.v.insert("mech", w.resource::<crate::dev_area::skills::DevSkills>().mechanical);
        c.v.insert("o2_patch", o2(eng(w)).0);
        true
    }));
    s.push(hold_f("breach: hold F until the breach is patched", 40.0, |e| !breached(e)));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let r = e.room_readout(e.room);
        check(c, !r.breach && r.leak_flow == 0.0 && !e.kit_in_hand && e.stats.patches_done == 1,
            format!("breach: patched at skill {:.0}, the hull at {:.0}, air {:.1} % (was {:.1} % at the start of the patch), the kit used up", c.v["mech"], e.hull.section(e.engine_section).condition.value(), r.oxygen * 100.0, c.v["o2_patch"] * 100.0));
        true
    }));
    // The fire.
    s.push(walk("breach: walk to the extinguisher", &[[-2.2, 7.8], [-0.4, 7.8], [-0.4, 5.2], AT_EXT], 30.0));
    s.push(stand_until_still());
    s.push(aim("breach: the prompt offers the extinguisher", EXTINGUISHER_AT, 0.9, "take the extinguisher"));
    s.push(tap_f("breach: F takes the extinguisher", |w| eng(w).extinguisher.held));
    s.push(walk("breach: back to the fire", &[[-0.4, 5.2], [-0.4, 7.8], [-2.2, 7.8], AT_SECTION], 30.0));
    s.push(Box::new(|w, _| {
        let at = eng(w).deck.fires_in(eng(w).room).next().map(|f| f.pos);
        if let Some(p) = at {
            let local = [p.x - crate::dev_area::ENGINEERING.centre[0], p.z - crate::dev_area::ENGINEERING.centre[1]];
            look_at(w, local, 0.5);
        }
        true
    }));
    s.push(hold_f("breach: hold F, the spray puts the fire out", 40.0, |e| !fire_here(e)));
    s.push(walk("breach: return the extinguisher", &[[-2.2, 7.8], [-0.4, 7.8], [-0.4, 5.2], AT_EXT], 30.0));
    s.push(stand_until_still());
    s.push(aim("breach: the prompt offers to return it", EXTINGUISHER_AT, 0.9, "return the extinguisher"));
    s.push(tap_f("breach: F returns the extinguisher", |w| !eng(w).extinguisher.held));
    // The machines the hits and the fire hurt: the reactor first, life support needs its power.
    s.push(walk("breach: walk to the reactor", &[[-0.4, 5.2], [-0.4, 7.6], AT_REACTOR], 30.0));
    s.push(stand_until_still());
    s.push(Box::new(|w, c| {
        let e = eng(w);
        check(c, true, format!("breach: the reactor at {:.0} after hits and fire (power {}), the generator puts out {:.2} m³/s", e.reactor.condition().value(), e.reactor.power_on(), e.generator.output));
        true
    }));
    s.push(aim("breach: the prompt offers the reactor", [REACTOR_AT[0] + 1.0, REACTOR_AT[1] - 1.5], 1.0, "repair the reactor"));
    s.push(hold_f("breach: hold F until the reactor is whole", 40.0, |e| e.reactor.condition().value() >= 100.0));
    s.push(walk("breach: walk to the reactor console", &[[-0.4, 7.6], [-0.4, 5.2], AT_REACTOR_CONSOLE], 30.0));
    s.push(stand_until_still());
    s.push(aim("breach: the prompt names the reactor again", REACTOR_CONSOLE_AT, 1.6, engineer::REACTOR_NAME));
    s.push(use_panel("breach: F uses the reactor panel again", REACTOR_PANEL));
    s.push(Box::new(|w, c| {
        // Power on only if it went off (a click toggles).
        if c.t == 0.0 && !eng(w).reactor.power_on() {
            let at = crate::displays::overlay_point_of(w, &REACTOR_PANEL, "power", None).expect("the power button");
            let mut ctl = w.resource_mut::<Controls>();
            ctl.pointer = Some(at);
            ctl.click = true;
            return false;
        }
        let e = eng(w);
        if e.reactor.power_on() || c.t >= 1.0 {
            check(c, e.reactor.power_on() && e.reactor.mode() == Mode::Auto, format!("breach: the reactor is on again on {:?}", e.reactor.mode()));
            return true;
        }
        false
    }));
    s.push(until("breach: the reactor feeds the grid again", 240.0, |e, _| (e.reactor.output_kw() - e.grid.request_kw()).abs() < 0.03 * e.grid.request_kw()));
    s.push(leave_panel("breach: F leaves the reactor panel"));
    // Life support.
    s.push(walk("breach: walk to the oxygen generator", &[AT_GENERATOR], 30.0));
    s.push(stand_until_still());
    s.push(aim("breach: the prompt offers the generator", GENERATOR_AT, 0.5, "repair the oxygen generator"));
    s.push(hold_f("breach: hold F until the generator is whole", 40.0, |e| e.generator.condition.value() >= 100.0));
    s.push(Box::new(|w, c| {
        c.v.insert("o2_gen", o2(eng(w)).0);
        true
    }));
    s.push(aim("breach: the prompt names the oxygen panel", GENERATOR_AT, 1.6, engineer::OXYGEN_NAME));
    s.push(use_panel("breach: F uses the oxygen panel", engineer::OXYGEN_PANEL));
    s.push(wait(2.0));
    s.push(shot_step("oxygen-panel"));
    s.push(until("breach: the generator brings the air back to 90 %", 300.0, |e, _| o2(e).0 >= 0.9));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        check(c, e.generator.output > 0.0, format!("breach: air {:.1} % -> {:.1} %, the generator puts out {:.2} m³/s", c.v["o2_gen"] * 100.0, o2(e).0 * 100.0, e.generator.output));
        true
    }));
    s.push(shot_step("oxygen-back"));
    s.push(leave_panel("breach: F leaves the oxygen panel"));
    s.push(walk("breach: walk to the pump", &[[-4.3, 5.2], [-4.3, 7.9], AT_PUMP], 30.0));
    s.push(stand_until_still());
    s.push(aim("breach: the prompt offers the pump", PUMP_AT, 0.7, "repair the coolant pump"));
    s.push(hold_f("breach: hold F until the pump is whole", 40.0, |e| e.pump.value() >= 100.0));
    s.push(Box::new(|w, c| {
        let e = eng(w);
        let [a, b] = e.room_readouts();
        check(c, !a.breach && !a.fire && !b.fire && a.oxygen >= 0.9 && !a.oxygen_alarm,
            format!("breach: the end: {a:?}; side {:.1} %; hits {}, breaches {}, patches {}, fires {}, air lost to space {:.0} m³", b.oxygen * 100.0, e.stats.hits, e.stats.breaches_opened, e.stats.patches_done, e.stats.fires_started, e.deck.lost_to_space()));
        true
    }));
}
