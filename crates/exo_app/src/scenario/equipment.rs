//! The equipment room (#311): a crew member takes a suit from a locker, puts it on, swaps its air
//! canister for a full one from a canister box (the old one fills there), calls the airlock out, stands
//! in the chamber's vacuum on the suit's air, runs the canister dry (no air from the suit), comes back
//! through the airlock, takes the suit off and stores it. All through `Controls` (aim, F, pointer),
//! with the host's inventories read for the checks. Every step the suits and canisters of the world
//! are counted: none may be lost or duplicated. A hook sets the suit's canister where the scenario
//! needs it (`drain_worn`, the host's own call).
use super::items::{items, look_at, tap_then, DELAY};
use super::reactor::{click_until, use_display};
use super::*;
use crate::container_window::{place_centre, ContainerSpecs, ContainerWindow, Place};
use crate::dev_area::equipment::{box_rack, suit_rack, ALL_SUIT_LOCKERS, SUIT_LOCKERS};
use crate::dev_area::gear::{MaskAir, SuitWorn};
use crate::dev_area::hull::panels::{AIRLOCK_IN_ID, AIRLOCK_IN_NAME, AIRLOCK_OUT_ID, AIRLOCK_OUT_NAME, AIRLOCK_SHIP_ID, AIRLOCK_SHIP_NAME};
use crate::dev_area::layout::{AIRLOCK, AIRLOCK_PANEL_AT, EQUIPMENT};
use crate::dev_area::{DevArea, DevBox};
use crate::health::HealthState;
use crate::items::{HudWindow, InventoryUi, Items};
use item_core::container::ItemKind;
use item_core::{RackId, BAR_SLOTS};
use item_core::wire::{AirlockCycle, AirlockNews};

fn cw(w: &World) -> &ContainerWindow {
    w.resource::<ContainerWindow>()
}

fn lock(w: &World) -> Option<AirlockNews> {
    w.resource::<Items>().reactor.map(|r| r.airlock)
}

fn me(w: &World) -> u32 {
    items(w).me()
}

/// (suits, canisters) in the world without the pad zone's lockers (they hold their own stock):
/// bars, worn places, racks; a suit's own canister counts.
fn census(w: &World) -> (u32, u32) {
    let it = items(w);
    let (mut suits, mut cans) = (0, 0);
    let mut count = |s: &item_core::container::Stack| match s.item.kind.as_str() {
        "suit" => {
            suits += s.count;
            cans += s.count * s.item.condition.is_some() as u32;
        }
        "air_canister" => cans += s.count,
        _ => {}
    };
    for inv in it.host.inventories.values() {
        (0..BAR_SLOTS).filter_map(|i| inv.slot(i)).for_each(&mut count);
        inv.worn.iter().for_each(&mut count);
    }
    let pad: Vec<_> = (SUIT_LOCKERS..ALL_SUIT_LOCKERS).flat_map(|i| [suit_rack(i), box_rack(i)]).collect();
    for c in it.host.racks.iter().filter(|(r, _)| !pad.contains(r)).map(|(_, c)| c) {
        (0..c.len()).filter_map(|i| c.slot(i)).for_each(&mut count);
    }
    (suits, cans)
}

fn air_worn(w: &World) -> Option<f64> {
    items(w).host.inventory(me(w)).worn_air()
}

fn bar_slot_of(w: &World, kind: &str) -> Option<usize> {
    (0..BAR_SLOTS).find(|i| items(w).client.view.slot(*i).is_some_and(|s| s.item.kind.as_str() == kind))
}

fn in_rack(w: &World, rack: RackId, kind: &str) -> u32 {
    items(w).host.racks.get(&rack).map_or(0, |c| c.count(&ItemKind::new(kind)))
}

fn box_fill(w: &World, rack: RackId, slot: usize) -> Option<f64> {
    items(w).host.racks.get(&rack)?.slot(slot)?.item.condition
}

fn centre(w: &World, rack: Option<RackId>, p: Place) -> Vec2 {
    let n = rack.and_then(|r| items(w).client.racks.get(&r)).map_or(0, |c| c.len());
    let cols = rack.and_then(|r| w.resource::<ContainerSpecs>().get(r)).map_or(4, |s| s.cols);
    place_centre(w.resource::<HudWindow>().0, n, cols, &w.resource::<crate::displays::DisplayLook>().0.hud.bar, p)
}

fn pointer_at(w: &mut World, p: Option<Vec2>, down: bool) {
    let mut ctl = w.resource_mut::<Controls>();
    ctl.pointer = p;
    ctl.pointer_down = down;
}

fn ground_at(w: &mut World, p: [f64; 2]) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, 0.0).expect("the dev area is there").0
}

/// A check that the world's suits and canisters are still the four and two of the start.
fn conserved(what: &'static str) -> Step {
    Box::new(move |w, c| {
        let (suits, cans) = census(w);
        check(c, suits == 2 && cans == 4, format!("equipment: {what}: nothing lost or duplicated ({suits} suits, {cans} canisters)"));
        true
    })
}

/// Stands the walker at `stand`, looks at the box of a container, opens its window with F.
fn open(what: &'static str, rack: RackId, box_id: &'static str, stand: [f64; 2]) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            begin(w, c, what);
            let at = ground_at(w, stand);
            place_walker(w, at);
            let mut q = w.query::<(&DevBox, &avian3d::prelude::Position)>();
            let target = q.iter(w).find(|(b, _)| b.id == box_id).map(|(_, p)| p.0).expect("the container's box");
            look_at(w, target);
            true
        }),
        wait(0.25),
        tap_then(what, KeyCode::KeyF, move |w| cw(w).rack == Some(rack) && w.resource::<InventoryUi>().open && items(w).client.holder(rack) == Some(me(w))),
        wait(3.0 * DELAY),
    ]
}

/// A click on a place, then the button up.
fn click(what: &'static str, rack: Option<RackId>, at: fn(&World) -> Option<Place>) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            let Some(p) = at(w) else {
                check(c, false, format!("equipment: {what}: the place exists"));
                return true;
            };
            let pos = centre(w, rack, p);
            pointer_at(w, Some(pos), true);
            true
        }),
        wait(0.15),
        Box::new(|w, _| {
            w.resource_mut::<Controls>().pointer_down = false;
            true
        }),
        wait(3.0 * DELAY + 0.1),
    ]
}

/// A drag from a place to another place: press, move, release.
fn drag(what: &'static str, rack: Option<RackId>, from: fn(&World) -> Option<Place>, to: fn(&World) -> Option<Place>) -> Vec<Step> {
    vec![
        Box::new(move |w, c| {
            let Some(p) = from(w) else {
                check(c, false, format!("equipment: {what}: the place to drag from exists"));
                return true;
            };
            let pos = centre(w, rack, p);
            pointer_at(w, Some(pos), true);
            true
        }),
        wait(0.1),
        Box::new(move |w, c| {
            let Some(p) = to(w) else {
                check(c, false, format!("equipment: {what}: the place to drop on exists"));
                return true;
            };
            let pos = centre(w, rack, p);
            pointer_at(w, Some(pos), true);
            c.t >= 0.1
        }),
        Box::new(|w, _| {
            w.resource_mut::<Controls>().pointer_down = false;
            true
        }),
        wait(3.0 * DELAY + 0.1),
    ]
}

fn wait_for(what: &'static str, limit: f64, done: fn(&World) -> bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, what);
            return false;
        }
        if done(w) {
            end(w, c, format!("{:.1} s", c.t));
            return true;
        }
        if c.t >= limit {
            check(c, false, format!("{what}: not within {limit} s"));
            return true;
        }
        false
    })
}

/// The walker's health share (0..1) and what the body breathes.
fn vitals(w: &World) -> (f64, Option<f64>) {
    let st = w.resource::<HealthState>();
    let b = st.bodies.get(&me(w));
    (b.map_or(1.0, |b| crate::health::vitals_of(&st.tuning, b).health), st.air.get(&me(w)).copied())
}

pub(super) fn equipment_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    const SUIT1: RackId = 201;
    const BOX1: RackId = 211;
    debug_assert_eq!((suit_rack(0), box_rack(0)), (SUIT1, BOX1));
    // In front of the first suit locker and its box, on the walkway.
    let front = |i: usize, back: f64| [EQUIPMENT.centre[0] + (i as f64 - 2.2), EQUIPMENT.centre[1] - EQUIPMENT.half[1] + 0.6 + back];

    s.push(Box::new(|w, c| {
        begin(w, c, "equipment: the lockers stand with their stock");
        w.resource_mut::<Items>().delay = DELAY;
        let ready = w.resource::<DevArea>().frame.is_some() && items(w).client.racks.contains_key(&suit_rack(0)) && items(w).client.racks.contains_key(&box_rack(0));
        if !ready {
            if c.t > 5.0 {
                check(c, false, "equipment: the lockers stand there".into());
                return true;
            }
            return false;
        }
        let (suits, cans) = census(w);
        check(c, suits == 2 && cans == 4, format!("equipment: two suits with a canister each and two spare canisters ({suits} suits, {cans} canisters)"));
        check(c, in_rack(w, SUIT1, "suit") == 1 && in_rack(w, BOX1, "air_canister") == 1, "equipment: suit locker 1 holds a suit, canister box 1 a canister".into());
        check(c, lock(w).is_some_and(|n| !n.inside && n.cycling == AirlockCycle::Idle), format!("equipment: the airlock rests outside ({:?})", lock(w)));
        end(w, c, String::new());
        true
    }));
    s.push(wait(0.3));

    // 1. The suit locker: F opens it, one click puts the suit on.
    s.extend(open("equipment: F opens suit locker 1", SUIT1, "equip_suit_1", front(0, 1.2)));
    s.push(Box::new(|w, c| {
        check(c, w.resource::<crate::interact::Interaction>().prompt.contains("Suit locker 1") || cw(w).rack == Some(SUIT1), "equipment: the window is suit locker 1".into());
        true
    }));
    s.push(shot_step("equipment-locker"));
    s.extend(click("equipment: click the suit in the locker", Some(SUIT1), |w| (!items(w).client.racks[&SUIT1].is_empty()).then_some(Place::Box(0))));
    s.push(Box::new(|w, c| {
        let inv = items(w).host.inventory(me(w));
        check(c, inv.worn.is_some() && in_rack(w, SUIT1, "suit") == 0, "equipment: a click on the locker's suit put it on".into());
        check(c, inv.count("suit") == 0, "equipment: the suit is in the worn place, not in a bar slot".into());
        check(c, inv.worn_air().is_some_and(|a| a > 99.0), "equipment: with its full canister (it already breathes a little)".into());
        check(c, w.resource::<SuitWorn>().0.contains(&me(w)), "equipment: the health step counts the body as suited".into());
        check(c, items(w).client.view.worn.is_some(), "equipment: the client's view shows the worn suit".into());
        true
    }));
    s.push(conserved("suit worn"));
    s.push(shot_step("equipment-worn"));
    s.push(tap_then("equipment: F closes the locker", KeyCode::KeyF, |w| cw(w).rack.is_none()));
    s.push(wait(0.3));

    // The hook: the suit's canister is at 40 %.
    s.push(Box::new(|w, _c| {
        let now = w.resource::<Time>().elapsed_secs_f64();
        let who = me(w);
        let air = air_worn(w).unwrap_or(0.0);
        w.resource_mut::<Items>().drain_worn(now, who, air - 40.0);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let a = air_worn(w).unwrap_or(-1.0);
        check(c, (a - 40.0).abs() < 1.0, format!("equipment: the worn canister stands near 40 % ({a:.1})"));
        true
    }));

    // 3. The canister box: take its full canister, swap it into the suit, put the old one into the box.
    s.extend(open("equipment: F opens canister box 1", BOX1, "equip_box_1", front(0, 1.2)));
    s.extend(click("equipment: click the canister in the box", Some(BOX1), |w| (0..4).find(|i| items(w).client.racks[&BOX1].slot(*i).is_some()).map(Place::Box)));
    s.push(Box::new(|w, c| {
        check(c, bar_slot_of(w, "air_canister").is_some() && in_rack(w, BOX1, "air_canister") == 0, "equipment: a click moved the canister into the bar".into());
        true
    }));
    s.extend(drag("equipment: drag the full canister onto the suit's canister place", Some(BOX1), |w| bar_slot_of(w, "air_canister").map(Place::Bar), |_| Some(Place::Air)));
    s.push(Box::new(|w, c| {
        let a = air_worn(w).unwrap_or(-1.0);
        check(c, (a - 100.0).abs() < 0.5, format!("equipment: the suit holds the full canister ({a:.1} %)"));
        let old = bar_slot_of(w, "air_canister").and_then(|i| items(w).host.inventory(me(w)).slot(i).and_then(|s| s.item.condition));
        check(c, old.is_some_and(|o| (o - 40.0).abs() < 1.0), format!("equipment: the old 40 % canister took the slot ({old:?})"));
        true
    }));
    s.push(conserved("after the swap"));
    // Anything but a canister is refused on the canister place: the (empty-handed) welder case is the unit tests'; here the suit goes on the box.
    s.extend(drag("equipment: drag the old canister into the box", Some(BOX1), |w| bar_slot_of(w, "air_canister").map(Place::Bar), |_| Some(Place::Box(2))));
    s.push(Box::new(|w, c| {
        let fill = box_fill(w, BOX1, 2);
        check(c, fill.is_some_and(|f| (f - 40.0).abs() < 1.0) && bar_slot_of(w, "air_canister").is_none(), format!("equipment: the old canister lies in the box ({fill:?})"));
        c.v.insert("fill0", fill.unwrap_or(0.0));
        true
    }));
    s.push(shot_step("equipment-box"));
    s.push(tap_then("equipment: F closes the box", KeyCode::KeyF, |w| cw(w).rack.is_none()));
    s.push(wait(6.0));
    s.push(Box::new(|w, c| {
        let (f0, f1) = (c.v["fill0"], box_fill(w, BOX1, 2).unwrap_or(0.0));
        check(c, f1 > f0 + 3.0 && f1 <= 100.0 && f1.is_finite(), format!("equipment: the box fills the canister while the ship has power ({f0:.1} -> {f1:.1} %)"));
        true
    }));
    s.push(conserved("while the box fills"));

    // 4. Out: the airlock is called in from the ship, the walker goes into the chamber and cycles out.
    use_display(s, AIRLOCK_SHIP_NAME, AIRLOCK_SHIP_ID, [AIRLOCK_PANEL_AT[2][0] - 0.5, AIRLOCK_PANEL_AT[2][1] + 0.15]);
    s.push(click_until(AIRLOCK_SHIP_ID, "push", None, "equipment: PUSH in the room calls the airlock in", |w| lock(w).is_some_and(|n| n.cycling == AirlockCycle::Inner)));
    s.push(wait_for("equipment: the inner door opens (10 s)", 10.0, |w| lock(w).is_some_and(|n| n.cycling == AirlockCycle::Idle && n.inner_open)));
    use_display(s, AIRLOCK_IN_NAME, AIRLOCK_IN_ID, [AIRLOCK.centre[0], AIRLOCK.centre[1]]);
    s.push(click_until(AIRLOCK_IN_ID, "push", None, "equipment: PUSH in the chamber starts the cycle out", |w| lock(w).is_some_and(|n| n.cycling == AirlockCycle::Outer)));
    s.push(wait_for("equipment: the outer door opens (10 s)", 10.0, |w| lock(w).is_some_and(|n| n.cycling == AirlockCycle::Idle && n.outer_open)));
    s.push(Box::new(|w, c| {
        begin(w, c, "equipment: in the empty chamber on the suit's air");
        let (h, _) = vitals(w);
        c.v.insert("h0", h);
        c.v.insert("air0", air_worn(w).unwrap_or(0.0));
        true
    }));
    s.push(wait(4.0));
    s.push(Box::new(|w, c| {
        let (h, oxy) = vitals(w);
        let (a0, a1) = (c.v["air0"], air_worn(w).unwrap_or(0.0));
        check(c, h >= c.v["h0"] - 1e-9, format!("equipment: no vacuum damage in a worn suit ({:.3} -> {h:.3})", c.v["h0"]));
        check(c, a1 < a0 && a1 > a0 - 5.0, format!("equipment: the canister drains at the mask's rate ({a0:.2} -> {a1:.2} %)"));
        check(c, w.resource::<MaskAir>().0.get(&me(w)) == Some(&1.0) && oxy == Some(1.0), format!("equipment: the body breathes the suit's air ({oxy:?})"));
        true
    }));
    // The canister runs dry: no air from the suit, the existing oxygen-low path takes over.
    s.push(Box::new(|w, c| {
        begin(w, c, "equipment: an empty canister gives no air");
        let now = w.resource::<Time>().elapsed_secs_f64();
        let who = me(w);
        let air = air_worn(w).unwrap_or(0.0);
        w.resource_mut::<Items>().drain_worn(now, who, air);
        true
    }));
    s.push(wait(1.5));
    s.push(Box::new(|w, c| {
        let (_, oxy) = vitals(w);
        check(c, air_worn(w) == Some(0.0), "equipment: the canister is empty (and stays at 0)".into());
        check(c, w.resource::<SuitWorn>().0.contains(&me(w)) && !w.resource::<MaskAir>().0.contains_key(&me(w)), "equipment: still suited, but the suit gives no air".into());
        // The suit's air is exactly 1.0; the chamber's air (open to the outside) is less, so the body is on the room's.
        check(c, oxy.is_some_and(|o| o < 1.0 - 1e-9), format!("equipment: the body falls back to the room's air ({oxy:?})"));
        end(w, c, String::new());
        true
    }));
    // A spare canister back in (the hook fills the suit), then out of the chamber into the open.
    s.push(Box::new(|w, _c| {
        let now = w.resource::<Time>().elapsed_secs_f64();
        let who = me(w);
        w.resource_mut::<Items>().drain_worn(now, who, -100.0);
        let at = ground_at(w, [AIRLOCK.centre[0] + 3.0, AIRLOCK.centre[1]]);
        place_walker(w, at);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let (h, _) = vitals(w);
        check(c, h > 0.0 && h.is_finite() && air_worn(w).is_some_and(|a| a.is_finite() && (0.0..=100.0).contains(&a)), format!("equipment: outside with a canister in range, health {h:.2}"));
        true
    }));

    // 5. Back in: PUSH on the outer panel, the airlock cycles in.
    use_display(s, AIRLOCK_OUT_NAME, AIRLOCK_OUT_ID, [AIRLOCK_PANEL_AT[1][0], AIRLOCK_PANEL_AT[1][1] + 0.8]);
    s.push(click_until(AIRLOCK_OUT_ID, "push", None, "equipment: PUSH outside calls the airlock in", |w| lock(w).is_some_and(|n| n.cycling == AirlockCycle::Inner)));
    s.push(wait_for("equipment: the inner door opens again (10 s)", 10.0, |w| lock(w).is_some_and(|n| n.cycling == AirlockCycle::Idle && n.inner_open)));
    s.push(conserved("after the airlock"));

    // 6. A click on the worn suit with the locker open stores it in the locker's free place.
    s.extend(open("equipment: F opens suit locker 1 again", SUIT1, "equip_suit_1", front(0, 1.2)));
    s.extend(click("equipment: click the worn suit", Some(SUIT1), |_| Some(Place::Suit)));
    s.push(Box::new(|w, c| {
        let inv = items(w).host.inventory(me(w));
        check(c, inv.worn.is_none() && in_rack(w, SUIT1, "suit") == 1 && inv.count("suit") == 0, "equipment: a click on the worn suit stored it in the locker".into());
        check(c, !w.resource::<SuitWorn>().0.contains(&me(w)), "equipment: the body is no longer suited".into());
        true
    }));
    s.push(conserved("suit stored"));
    s.push(tap_then("equipment: F closes the locker", KeyCode::KeyF, |w| cw(w).rack.is_none()));
    s.push(Box::new(|w, c| {
        pointer_at(w, None, false);
        end(w, c, "equipment room round".into());
        true
    }));
}
