//! #287 (#254): health through `Controls` and the pointer. A bad reactor setting starts a fire in
//! the engineer corner; standing in it burns (the burn grows with the exposure); the scientist
//! brews a burn salve (blorp paste and frost moss) through the whole lab chain, fills a syringe
//! and injects it at the medbay: the burn falls by the dose; the bed patient gets a bandage and a
//! dose too. Then the side room loses its air (test setup: vented, door shut): oxygen low, out
//! cold, dead, a banner, and back at the pads with a share of the skills lost. Run with
//! `--start=dev_area`.
use super::dev_area::active;
use super::lab_devices::{act, brew, click, click_until, go_use, hands, heat_spin_fill, take_from, wait_until_s};
use super::*;
use crate::dev_area::engineer::{area3, area_of, world_of, Engineering, FIRE_AT, REACTOR_PANEL};
use crate::dev_area::health::{Medical, Who, MEDBAY};
use crate::dev_area::science::LabItem;
use crate::dev_area::skills::DevSkills;
use crate::dev_area::{storage, DevArea, PADS};
use crate::displays::ActiveDisplay;
use health_core::Life;

fn med(w: &World) -> &Medical {
    w.resource::<Medical>()
}

fn eng(w: &World) -> &Engineering {
    w.resource::<Engineering>()
}

fn burn(w: &World) -> f64 {
    med(w).me.strength("burn")
}

/// The hands slot of the first syringe or first aid item of `kind` ("syringe", "bandage").
fn medicine_slot(w: &World, kind: &str) -> Option<usize> {
    let h = &hands(w).0;
    (0..h.len()).find(|&i| {
        h.slot(i).is_some_and(|s| match &s.item.payload {
            Some(LabItem::Syringe(y)) => kind == "syringe" && y.doses > 0,
            Some(LabItem::FirstAid(k)) => k == kind,
            _ => false,
        })
    })
}

fn skills(w: &World) -> [f64; 5] {
    let s = w.resource::<DevSkills>();
    [s.piloting, s.weapons, s.mechanical, s.electrical, s.medical]
}

/// Leave any display (F) and put the walker at an engineering-zone point (test setup).
fn put_at(what: &'static str, local: [f64; 2]) -> Vec<Step> {
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
            let f = w.resource::<DevArea>().frame.expect("dev area");
            let pl = planet(w);
            place_walker(w, world_of(&f, &pl, area3(local, 0.0)));
            println!("{what}");
            c.t >= 0.0
        }),
    ]
}

pub(super) fn health_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        begin(w, c, "health: the player and the medbay");
        let m = med(w);
        check(c, m.me.vitality() == 100.0 && m.me.afflictions().is_empty() && m.bed.strength("burn") > 0.0,
            format!("health: the player starts healthy, the bed patient burnt ({:.0})", m.bed.strength("burn")));
        true
    }));
    // A fire: the reactor in manual with fission up and the turbine low.
    s.extend(go_use(REACTOR_PANEL));
    s.push(click_until(REACTOR_PANEL, act("power"), None, "health: the reactor powers on", 1.0, |w| eng(w).reactor.power_on()));
    s.push(click_until(REACTOR_PANEL, act("fission"), Some(0.98), "health: fission to the top", 1.0, |w| eng(w).reactor.targets().0 > 90.0));
    s.push(click_until(REACTOR_PANEL, act("turbine"), Some(0.3), "health: turbine low", 1.0, |w| (eng(w).reactor.targets().1 - 30.0).abs() < 5.0));
    s.push(wait_until_s("health: the reactor overheats and starts a fire".into(), 120.0, |w| !eng(w).deck.fires().is_empty()));
    // Stand in the fire until the burn reaches 40.
    s.extend(put_at("health: into the fire", [FIRE_AT[0], FIRE_AT[1] - 0.8]));
    s.push(Box::new(|w, c| {
        let b = burn(w);
        let marks = [1.0, 2.0, 3.0];
        for (i, m) in marks.iter().enumerate() {
            let k: &'static str = ["b1", "b2", "b3"][i];
            if c.t >= *m && !c.v.contains_key(k) {
                c.v.insert(k, b);
            }
        }
        if b >= 40.0 || c.t >= 20.0 {
            let size = eng(w).deck.fires().iter().map(|f| f.size).fold(0.0, f64::max);
            let (b1, b2) = (c.v.get("b1").copied().unwrap_or(b), c.v.get("b2").copied().unwrap_or(b));
            check(c, b >= 40.0 && b2 >= b1, format!("health: in the fire (largest {size:.2} m) the burn grows: {b1:.1} after 1 s, {b2:.1} after 2 s, {b:.1} after {:.1} s; vitality {:.1}", c.t, med(w).me.vitality()));
            for k in ["b1", "b2", "b3"] {
                c.v.remove(k);
            }
            return true;
        }
        false
    }));
    s.push(shot_step("burning"));
    s.extend(put_at("health: out of the fire, to the lab", [-40.0, -10.0]));
    // The burn salve through the lab.
    s.extend(go_use(storage::SHELF));
    for kind in ["blorp_paste", "frost_moss"] {
        s.push(take_from(storage::SHELF, kind, format!("health: {kind} from the shelf")));
    }
    brew(s, "salve", ["blorp_paste", "frost_moss"]);
    heat_spin_fill(s, "salve", &|_| wait(0.0));
    s.push(Box::new(|w, c| {
        let ok = hands(w).0.slot(medicine_slot(w, "syringe").unwrap_or(usize::MAX)).and_then(|s| match &s.item.payload {
            Some(LabItem::Syringe(y)) => Some(y.product.effects.iter().any(|e| e.as_str() == "cooling")),
            _ => None,
        });
        check(c, ok == Some(true), "health: the syringe holds a cooling salve".into());
        true
    }));
    // A bandage for the bed patient.
    s.extend(go_use(storage::MEDICAL));
    s.push(take_from(storage::MEDICAL, "bandage", "health: bandages from the medical cabinet"));
    // The medbay: the salve on the player.
    s.extend(go_use(MEDBAY));
    s.push(Box::new(|w, c| {
        c.v.insert("before", burn(w));
        c.v.insert("skill", w.resource::<DevSkills>().medical);
        let slot = medicine_slot(w, "syringe").expect("a syringe in the hands");
        click(w, &MEDBAY, &format!("use:{slot}"), None);
        true
    }));
    s.push(wait_until_s("health: the salve takes the burn down".into(), 1.0, |w| burn(w) < 30.0));
    s.push(shot_step("medbay"));
    s.push(Box::new(|w, c| {
        let (b0, b1) = (c.v["before"], burn(w));
        let line = med(w).line.0.clone();
        check(c, b0 - b1 > 20.0, format!("health: the salve at medical skill {:.1}: burn {b0:.1} -> {b1:.1} ({line})", c.v["skill"]));
        true
    }));
    // The bed patient: a bandage, then a dose.
    s.push(click_until(MEDBAY, act("who"), None, "health: the medbay turns to the bed patient", 1.0, |w| med(w).who == Who::Bed));
    for kind in ["bandage", "syringe"] {
        s.push(Box::new(move |w, c| {
            if c.t == 0.0 {
                c.v.insert("bed0", med(w).bed.strength("burn"));
                let Some(slot) = medicine_slot(w, kind) else {
                    check(c, false, format!("health: a {kind} in the hands"));
                    return true;
                };
                click(w, &MEDBAY, &format!("use:{slot}"), None);
                return false;
            }
            let b = med(w).bed.strength("burn");
            if b < c.v["bed0"] - 1.0 || c.t > 1.0 {
                check(c, b < c.v["bed0"] - 1.0, format!("health: the {kind} on the bed patient: burn {:.1} -> {b:.1} ({})", c.v["bed0"], med(w).line.0));
                return true;
            }
            false
        }));
    }
    s.push(shot_step("medbay-bed"));
    // No air: the side room vented and shut (test setup), the walker in it.
    s.push(Box::new(|w, c| {
        let mut e = w.resource_mut::<Engineering>();
        let (door, side) = (e.door, e.side_room);
        e.deck.set_open(door, 0.0);
        e.deck.room_mut(side).oxygen = 0.0;
        c.v.insert("respawns", med(w).stats.respawns as f64);
        let k = skills(w);
        for (i, v) in k.iter().enumerate() {
            c.v.insert(["k0", "k1", "k2", "k3", "k4"][i], *v);
        }
        true
    }));
    s.extend(put_at("health: into the vented side room", [8.0, 10.0]));
    s.push(wait_until_s("health: no air raises oxygen low past 50".into(), 20.0, |w| med(w).me.strength("oxygen_low") > 50.0));
    s.push(Box::new(|w, c| {
        let m = med(w);
        check(c, m.air < 0.3, format!("health: the air at the player is {:.0} %, oxygen low {:.0}, vitality {:.1}", m.air * 100.0, m.me.strength("oxygen_low"), m.me.vitality()));
        true
    }));
    s.push(shot_step("no-air"));
    s.push(wait_until_s("health: out cold".into(), 60.0, |w| med(w).me.state() == Life::Unconscious));
    s.push(wait_until_s("health: dead".into(), 200.0, |w| med(w).me.state() == Life::Dead));
    s.push(Box::new(|w, c| {
        let m = med(w);
        check(c, m.stats.deaths >= 1, format!("health: died of {} at vitality {:.1}", m.stats.last_cause, m.me.vitality()));
        true
    }));
    s.push(wait(0.5));
    s.push(shot_step("dead"));
    s.push(wait_until_s("health: back after the respawn time".into(), 10.0, |w| med(w).stats.respawns as f64 > 0.0));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let m = med(w).clone();
        let f = w.resource::<DevArea>().frame.unwrap();
        let pl = planet(w);
        let p = player_world(w);
        let a = area_of(&f, &pl, p);
        let k = skills(w);
        let lost = m.tuning.respawn_skill_loss;
        let ok_skills = k.iter().enumerate().all(|(i, v)| (v - c.v[["k0", "k1", "k2", "k3", "k4"][i]] * (1.0 - lost)).abs() < 0.01);
        check(c, m.me.state() == Life::Conscious && m.me.vitality() == 100.0 && PADS.contains([a.x, a.z]),
            format!("health: back at the pads ({:.0}, {:.0}), vitality {:.0}", a.x, a.z, m.me.vitality()));
        check(c, ok_skills, format!("health: every skill lost {:.0} %: medical {:.2} -> {:.2}", lost * 100.0, c.v["k4"], k[4]));
        end(w, c, format!("burned {:.0} in total, deaths {}, respawns {}", m.stats.burned, m.stats.deaths, m.stats.respawns));
        true
    }));
    s.push(shot_step("respawn"));
}
