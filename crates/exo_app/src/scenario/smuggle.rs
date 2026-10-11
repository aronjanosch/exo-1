//! Round 6, the smuggling run, headless. One module for the round's scenarios:
//! - `phone`: the phone opens, every tab has rows, it closes.
//! - `hail`: in Bent Spoon's airspace, L calls the tower, a pad is assigned, a landing on it is
//!   graded; a landing without clearance costs a fine.
//! - `empire`: deeds and the tribute raise the family rank; the shop sells an upgrade and goods.
//! - `lab`: carry a base and two ingredients into the kettle, brew, stop the needle, take the
//!   product crates; the mix goes into the recipe book.
//! - `scan`: at high heat the tower sends the ship to the inspection pad; comply loses the
//!   contraband, the hidden hold keeps it, taking off flees.
//! - `dealing`: a buyer orders by effect, a counter-offer, product crates handed over, paid.
//! - `smuggle_run`: all of it in one go.
//!
//! Moving the walker between the lab, the shop and the buyers, and the ship between towns, are
//! test hooks (as in `deliver`); the flight between towns is the flight scenarios' part.
use super::warp::teleport_ship;
use super::*;
use crate::cargo::{cabin_floor_pos, crate_bundle, Crate, Crates};
use crate::gameplay::{Gameplay, HOST};
use crate::ship::basis_for_up;
use crate::smuggling::glue::{buyer_spot, kettles, Stock};
use crate::smuggling::SmuggleHud;
use gameplay_core::{CommodityId, EffectId, ProductSpec, Tag, TrackId, WorldEvent};

fn gp(w: &World) -> &Gameplay {
    w.resource::<Gameplay>()
}

fn push(w: &mut World, e: WorldEvent) {
    w.resource_mut::<Gameplay>().push_world(HOST, e);
}

fn hud(w: &World) -> &SmuggleHud {
    w.resource::<SmuggleHud>()
}

fn wallet(w: &World) -> i64 {
    gp(w).progress.wallet()
}

fn track(w: &World, t: &str) -> i64 {
    gp(w).progress.value(&gp(w).kernel, &TrackId::new(t), None).unwrap_or(0)
}

fn logged(w: &World, needle: &str) -> bool {
    gp(w).smuggling.log.iter().any(|l| l.contains(needle))
}

fn ship_entity(w: &mut World) -> Entity {
    ship_e(w)
}

/// `n` stock crates on the cabin floor (the hold), in a row.
fn stock_in_cabin(w: &mut World, commodity: &str, product: Option<ProductSpec>, n: u32) {
    let ship = ship_entity(w);
    let table = w.resource::<Crates>().0.clone();
    for i in 0..n {
        let pos = cabin_floor_pos(&table, "small", -1.2 + (i % 2) as f64 * 2.4, 1.0 - (i / 2) as f64 * 1.0);
        w.spawn((crate_bundle(&table, "small", Some(ship), pos, DVec3::NEG_Z), Stock { commodity: CommodityId::new(commodity), product: product.clone() }));
    }
}

/// One stock crate on the ground at `at` (world).
fn stock_at(w: &mut World, commodity: &str, product: Option<ProductSpec>, at: DVec3) -> Entity {
    let pl = planet(w);
    let up = pl.up(at);
    let table = w.resource::<Crates>().0.clone();
    let half = table.get("small").map_or(0.25, |s| s.extents[1] * 0.5);
    let dir = up;
    let pos = pl.centre + dir * (pl.surface(dir) + half + 0.05);
    let fwd = DQuat::from_rotation_arc(DVec3::Y, up) * DVec3::NEG_Z;
    w.spawn((crate_bundle(&table, "small", None, pos, fwd), Stock { commodity: CommodityId::new(commodity), product })).id()
}

fn stock_count(w: &mut World, f: impl Fn(&Stock, &Crate) -> bool) -> usize {
    w.query::<(&Stock, &Crate)>().iter(w).filter(|(s, c)| f(s, c)).count()
}

fn aboard(w: &mut World) -> usize {
    let ship = ship_entity(w);
    stock_count(w, |_, c| c.ship == Some(ship))
}

fn product(effects: &[&str]) -> ProductSpec {
    ProductSpec { base: CommodityId::new("blorp_paste"), effects: effects.iter().map(|e| EffectId::new(*e)).collect(), quality: 0.9 }
}

/// The ship level, `height` m above a location's pad centre, at rest.
fn ship_over(w: &mut World, l: &str, height: f64) {
    let p = super::deliver::pad(w, l);
    teleport_ship(w, p.centre + p.up * height, basis_for_up(p.up));
}

/// Seated, the ship hovering (hover assist on).
fn seated() -> Vec<Step> {
    let mut v: Vec<Step> = vec![Box::new(|w, _| {
        put_at_seat(w);
        true
    })];
    v.extend(sit());
    v
}

/// The clearance of the pilot, if any: (pad, inspection).
fn clearance(w: &World) -> Option<(String, bool)> {
    gp(w).smuggling.traffic.clearance(HOST).map(|c| (c.pad.to_string(), c.inspection))
}

/// Hail and wait for a clearance (a hold answers again after 20 s).
fn hail_until_cleared(name: &'static str) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            tap(w, KeyCode::KeyL);
        }
        if clearance(w).is_some() || c.t > 40.0 {
            let cl = clearance(w);
            end(w, c, format!("after {:.1} s: {cl:?}", c.t));
            return true;
        }
        false
    })
}

/// Heat up to at least `stars` (violations, as if broken elsewhere).
fn heat_to(w: &mut World, stars: u8) {
    let t = gp(w).smuggling.heat_c.tuning.record.points_per_star;
    let need = (stars as f64 * t - gp(w).smuggling.heat.points()).max(0.0);
    let per = gp(w).smuggling.heat_c.tuning.record.violation_points.of(gameplay_core::ViolationKind::NoClearance);
    for _ in 0..(need / per).ceil() as u32 {
        push(w, WorldEvent::Violation { town: gameplay_core::TownId::new("drip_rock"), kind: gameplay_core::ViolationKind::NoClearance });
    }
}

/// Moves the phone's selection one row per 0.15 s (one tap per step) until a row containing
/// `needle` (case-insensitive) is selected; at most 40 taps, down then wrapping is not needed.
fn select_row(needle: &'static str) -> Step {
    Box::new(move |w, c| {
        let p = hud(w).phone.clone();
        let hit = |r: &String| r.to_lowercase().contains(needle);
        if p.rows.get(p.selected).is_some_and(hit) {
            c.v.remove("taps");
            return true;
        }
        let target = p.rows.iter().position(hit);
        let taps = c.v.get("taps").copied().unwrap_or(0.0);
        if target.is_none() || taps > 40.0 {
            c.v.remove("taps");
            return true;
        }
        if c.t - c.v.get("tap_t").copied().unwrap_or(-1.0) >= 0.15 {
            c.v.insert("tap_t", c.t);
            c.v.insert("taps", taps + 1.0);
            tap(w, if target.unwrap() > p.selected { KeyCode::ArrowDown } else { KeyCode::ArrowUp });
        }
        false
    })
}

// ---------------------------------------------------------------- phone

pub fn phone_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, c| {
        begin(w, c, "phone: open it, every tab");
        check(c, !hud(w).phone.open, "phone: closed at the start".into());
        tap(w, KeyCode::Backquote);
        true
    }));
    s.push(wait(0.3));
    for i in 0..6 {
        s.push(Box::new(move |w, c| {
            let p = hud(w).phone.clone();
            check(c, p.open && p.tab.index() == i && !p.rows.is_empty(), format!("phone: tab {} shows {} rows (first: '{}')", p.tab.title(), p.rows.len(), p.rows.first().cloned().unwrap_or_default()));
            tap(w, KeyCode::ArrowRight);
            true
        }));
        s.push(wait(0.3));
    }
    s.push(Box::new(|w, c| {
        let p = hud(w).phone.clone();
        check(c, p.tab.index() == 0, format!("phone: right from the last tab wraps to {}", p.tab.title()));
        tap(w, KeyCode::ArrowDown);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let sel = hud(w).phone.selected;
        check(c, sel == 1, format!("phone: down selects row {sel}"));
        check(c, hud(w).phone_text().contains("> "), "phone: the selected row is marked".into());
        tap(w, KeyCode::Backquote);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        check(c, !hud(w).phone.open && hud(w).phone_text().is_empty(), "phone: ` closes it".into());
        end(w, c, "six tabs".into());
        true
    }));
}

// ---------------------------------------------------------------- hail

pub fn hail_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.extend(seated());
    s.push(Box::new(|w, c| {
        begin(w, c, "hail: into Bent Spoon's airspace");
        crate::scenario::cargo::clear_crates(w);
        c.v.insert("wallet0", wallet(w) as f64);
        // 400 m east of the main pad, 80 m up: inside the airspace.
        let p = super::deliver::pad(w, "bent_spoon");
        let east = p.up.any_orthonormal_vector();
        teleport_ship(w, p.centre + east * 400.0 + p.up * 80.0, basis_for_up(p.up));
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let inside = gp(w).smuggling.traffic.inside(HOST).map(|t| t.to_string());
        check(c, inside.as_deref() == Some("bent_spoon"), format!("hail: the ship is in Bent Spoon's airspace ({inside:?})"));
        true
    }));
    s.push(hail_until_cleared("hail: L calls the tower"));
    s.push(Box::new(|w, c| {
        let cl = clearance(w);
        check(c, cl.as_ref().is_some_and(|(p, insp)| p.starts_with("bent_spoon") && !insp), format!("hail: the tower assigns a Bent Spoon pad ({cl:?})"));
        let radio = hud(w).radio.iter().filter(|l| l.who.contains("Bent Spoon")).count();
        check(c, radio >= 1, format!("hail: the tower talks on the radio ({radio} lines, last: '{}')", hud(w).radio.last().map(|l| l.text.clone()).unwrap_or_default()));
        let pad = cl.map(|(p, _)| p).unwrap_or_else(|| "bent_spoon".into());
        c.v.insert("pad_east", 0.0);
        ship_over(w, &pad, 20.0);
        w.resource_mut::<Gameplay>().log.push(format!("scenario: landing on {pad}"));
        true
    }));
    s.push(wait(1.0));
    s.push(land("hail: land on the assigned pad"));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let graded = gp(w).smuggling.log.iter().find(|l| l.starts_with("graded")).cloned();
        check(c, graded.as_ref().is_some_and(|l| l.contains("on time true")), format!("hail: the landing is graded, on time ({graded:?})"));
        check(c, !logged(w, "violation"), "hail: no violation on the assigned pad".into());
        check(c, clearance(w).is_none(), "hail: the clearance is used up".into());
        c.v.insert("wallet1", wallet(w) as f64);
        ship_over(w, "bent_spoon_west", 20.0);
        true
    }));
    s.push(wait(1.0));
    s.push(land("hail: land without clearance"));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let fine = c.v["wallet1"] - wallet(w) as f64;
        check(c, logged(w, "violation NoClearance"), "hail: a landing without clearance is a violation".into());
        check(c, fine >= 40.0, format!("hail: and costs a fine ({fine:.0} cr)"));
        check(c, gp(w).smuggling.heat.points() > 0.0, format!("hail: and heat ({:.0} points)", gp(w).smuggling.heat.points()));
        end(w, c, format!("fine {fine:.0}, heat {:.0}", gp(w).smuggling.heat.points()));
        true
    }));
}

// ---------------------------------------------------------------- empire

pub fn empire_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, c| {
        begin(w, c, "empire: the first rank");
        let line = hud(w).goal_line.clone();
        check(c, line.starts_with("Next:"), format!("empire: the HUD shows the next goal ('{line}')"));
        c.v.insert("rank0", gp(w).smuggling.empire.rank("gribbles") as f64);
        // Two jobs done (as the courier jobs raise them).
        push(w, WorldEvent::FlagRaised { flag: gameplay_core::Flag::new("job_completed:courier_lint_trap") });
        push(w, WorldEvent::FlagRaised { flag: gameplay_core::Flag::new("job_completed:courier_noodle_post") });
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        check(c, track(w, "jobs_done") == 2, format!("empire: two jobs counted ({})", track(w, "jobs_done")));
        let line = hud(w).goal_line.clone();
        check(c, line.contains("pay"), format!("empire: the goal line asks for the tribute ('{line}')"));
        tap(w, KeyCode::Backquote);
        true
    }));
    s.push(wait(0.3));
    s.push(select_row("tribute"));
    s.push(Box::new(|w, c| {
        let p = hud(w).phone.clone();
        let row = p.rows.get(p.selected).cloned().unwrap_or_default();
        check(c, row.starts_with("Tribute"), format!("empire: the tribute row is selected ('{row}')"));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let rank = gp(w).smuggling.empire.rank("gribbles");
        check(c, rank as f64 == c.v["rank0"] + 1.0, format!("empire: F on the tribute row raises the rank ({rank})"));
        check(c, gp(w).progress.has_tag(&Tag::new("lab_access")), "empire: the rank gives lab access".into());
        let line = hud(w).goal_line.clone();
        check(c, line.starts_with("Next:") && !line.contains("pay"), format!("empire: the next goal moves on ('{line}')"));
        // Money for the shop.
        push(w, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 1000, player: None });
        tap(w, KeyCode::ArrowLeft);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        let p = hud(w).phone.clone();
        check(c, p.tab == crate::smuggling::PhoneTab::Shop, format!("empire: left from Goals is the Shop ({})", p.tab.title()));
        true
    }));
    s.push(select_row("hidden hold"));
    s.push(Box::new(|w, c| {
        let p = hud(w).phone.clone();
        let row = p.rows.get(p.selected).cloned().unwrap_or_default();
        check(c, row.contains("idden"), format!("empire: the hidden hold is selected ('{row}')"));
        c.v.insert("wallet", wallet(w) as f64);
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let paid = c.v["wallet"] - wallet(w) as f64;
        check(c, gp(w).progress.has_tag(&Tag::new("ship_hidden_hold")), format!("empire: the hidden hold is bought ({paid:.0} cr)"));
        true
    }));
    s.push(select_row("blorp"));
    s.push(Box::new(|w, c| {
        let row = { let p = &hud(w).phone; p.rows.get(p.selected).cloned().unwrap_or_default() };
        check(c, row.to_lowercase().contains("blorp"), format!("empire: with lab access the base is for sale ('{row}')"));
        c.v.insert("crates0", stock_count(w, |s, _| s.commodity.as_str() == "blorp_paste") as f64);
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let n = stock_count(w, |s, _| s.commodity.as_str() == "blorp_paste") as f64;
        check(c, n == c.v["crates0"] + 1.0, format!("empire: the bought base waits as a crate at the wholesaler ({n})"));
        tap(w, KeyCode::Backquote);
        end(w, c, format!("rank {}, wallet {}", gp(w).smuggling.empire.rank("gribbles"), wallet(w)));
        true
    }));
}

// ---------------------------------------------------------------- lab

/// The walker 1.2 m from kettle 1, a crate of `commodity` 1 m in front of it.
fn at_kettle_with(w: &mut World, commodity: &str) {
    let ks = {
        let town = w.resource::<crate::town::Town>();
        kettles(town, gp(w))
    };
    let (_, k, up) = ks[0].clone();
    let side = up.any_orthonormal_vector();
    let stand = k + side * 1.2;
    let pl = planet(w);
    let d = pl.up(stand);
    place_walker(w, pl.centre + d * (pl.surface(d) + 0.05));
    let toward = (k - stand).normalize();
    stock_at(w, commodity, None, stand + up.cross(side) * 1.0 + toward * 0.0);
    face_towards(w, stand + up.cross(side) * 1.0 - up * 1.0);
}

pub fn lab_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, c| {
        begin(w, c, "lab: fill the kettle");
        crate::scenario::cargo::clear_crates(w);
        let n = kettles(w.resource::<crate::town::Town>(), gp(w)).len();
        check(c, n == 1, format!("lab: one kettle in the city ({n})"));
        true
    }));
    for item in ["blorp_paste", "space_sugar", "moth_dust"] {
        s.push(Box::new(move |w, _| {
            at_kettle_with(w, item);
            true
        }));
        s.push(wait(0.5));
        s.push(Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }));
        s.push(wait(0.6));
        s.push(Box::new(move |w, c| {
            let p = prompt_of(w);
            check(c, p.contains("into the kettle"), format!("lab: holding the {item}, the prompt offers the kettle ('{p}')"));
            tap(w, KeyCode::KeyF);
            true
        }));
        s.push(wait(0.5));
    }
    s.push(Box::new(|w, c| {
        let k = gp(w).smuggling.lab.kettle("kettle_1").cloned();
        check(c, matches!(&k, Some(production_core::Kettle::Loaded { ingredients, .. }) if ingredients.len() == 2), format!("lab: base and two ingredients in the kettle ({k:?})"));
        let p = prompt_of(w);
        check(c, p.contains("start the brew") && p.contains("???"), format!("lab: an unknown mix shows ??? ('{p}')"));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(Box::new(|w, c| {
        // Brewing, then the needle.
        if matches!(gp(w).smuggling.lab.kettle("kettle_1"), Some(production_core::Kettle::Done { .. })) && hud(w).brew.is_some() {
            check(c, c.t > 5.0, format!("lab: the brew takes its time ({:.1} s)", c.t));
            return true;
        }
        c.t > 20.0
    }));
    // The needle sweeps 0 → 1 in half a period (0.8 s): it passes the sweet spot 0.5 at 0.4 s.
    s.push(Box::new(|w, c| {
        let b = hud(w).brew;
        if b.is_some_and(|b| (b.needle - b.sweet).abs() < 0.04) || c.t > 3.0 {
            c.v.insert("needle", b.map_or(-1.0, |b| b.needle));
            tap(w, KeyCode::KeyF);
            return true;
        }
        false
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let k = gp(w).smuggling.lab.kettle("kettle_1").cloned();
        let Some(production_core::Kettle::Ready { product, crates }) = k else {
            check(c, false, format!("lab: the brew is released ({k:?})"));
            return true;
        };
        let want: std::collections::BTreeSet<EffectId> = [EffectId::new("giggly"), EffectId::new("floaty")].into();
        check(c, product.effects == want, format!("lab: sugar then moth dust gives {:?}", product.effects));
        check(c, product.quality > 0.8, format!("lab: the needle stopped at {:.2} gives quality {:.2}", c.v["needle"], product.quality));
        check(c, gp(w).smuggling.lab.book().len() == 1, "lab: the mix is in the recipe book".into());
        check(c, gp(w).shown.iter().any(|l| l.key == production_core::lab::DISCOVERED_KEY) || gp(w).log.iter().any(|l| l.contains("lab.discovered")) || !gp(w).notices.is_empty(), "lab: a discovery notice".into());
        c.v.insert("crates", crates as f64);
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let n = stock_count(w, |s, _| s.product.is_some()) as f64;
        check(c, n == c.v["crates"], format!("lab: F takes {n} product crates"));
        check(c, track(w, "recipes_found") == 1, format!("lab: the family counts the recipe ({})", track(w, "recipes_found")));
        end(w, c, format!("{n} crates"));
        true
    }));
}

fn prompt_of(w: &World) -> String {
    w.resource::<crate::interact::Interaction>().prompt.clone()
}

// ---------------------------------------------------------------- scan

pub fn scan_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.extend(seated());
    s.push(Box::new(|w, c| {
        begin(w, c, "scan: hot, with contraband aboard");
        crate::scenario::cargo::clear_crates(w);
        stock_in_cabin(w, "blorp_paste", Some(product(&["giggly"])), 3);
        heat_to(w, 3);
        let p = super::deliver::pad(w, "bent_spoon");
        let east = p.up.any_orthonormal_vector();
        teleport_ship(w, p.centre + east * 300.0 + p.up * 60.0, basis_for_up(p.up));
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let stars = gp(w).smuggling.heat.stars();
        check(c, stars >= 3, format!("scan: heat at {stars} stars"));
        check(c, aboard(w) == 3, format!("scan: three contraband crates aboard ({})", aboard(w)));
        true
    }));
    s.push(hail_until_cleared("scan: hail at three stars"));
    s.push(Box::new(|w, c| {
        let cl = clearance(w);
        check(c, cl.as_ref().is_some_and(|(p, insp)| *insp && p == "bent_spoon_inspect"), format!("scan: the tower sends the ship to inspection ({cl:?})"));
        ship_over(w, "bent_spoon_inspect", 20.0);
        c.v.insert("wallet", wallet(w) as f64);
        true
    }));
    s.push(wait(1.0));
    s.push(land("scan: land on the inspection pad"));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let open = gp(w).smuggling.open_scan.clone();
        check(c, open.is_some(), format!("scan: the inspection scans ({open:?})"));
        let text = hud(w).scan.clone();
        check(c, text.contains("[1] Comply") && text.contains("[2] Bribe") && text.contains("[3] Talk"), format!("scan: the prompt offers the choices ('{}')", text.lines().next().unwrap_or("")));
        c.v.insert("wallet", wallet(w) as f64);
        tap(w, KeyCode::Digit1);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let fine = c.v["wallet"] - wallet(w) as f64;
        check(c, logged(w, "Comply, confiscate true"), "scan: comply: they take it".into());
        check(c, aboard(w) == 0, format!("scan: the contraband is gone ({} left)", aboard(w)));
        check(c, fine == 145.0, format!("scan: fine 25 + 3 × 40 = {fine:.0} cr"));
        check(c, hud(w).scan.is_empty(), "scan: the prompt closes".into());
        end(w, c, format!("fine {fine:.0}"));
        // Phase 2: the hidden hold.
        begin(w, c, "scan: the hidden hold");
        push(w, WorldEvent::TrackChanged { track: TrackId::new("wallet"), delta: 1000, player: None });
        push(w, WorldEvent::UnlockBought { unlock: gameplay_core::UnlockId::new("hidden_hold") });
        stock_in_cabin(w, "blorp_paste", Some(product(&["giggly"])), 2);
        ship_over(w, "bent_spoon_inspect", 20.0);
        true
    }));
    s.push(wait(1.0));
    s.push(land("scan: land again, the goods in the hidden hold"));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let cargo = gp(w).smuggling.cargo;
        check(c, cargo.hidden == 2 && cargo.visible == 0, format!("scan: the hold hides both ({cargo:?})"));
        let scans = gp(w).smuggling.log.iter().filter(|l| l.contains("Comply, confiscate false, fine 0")).count();
        let landed_clean = scans >= 1 || gp(w).smuggling.log.last().is_some_and(|l| l.contains("scan false"));
        check(c, landed_clean && aboard(w) == 2, format!("scan: a clean scan or none, the crates stay ({} aboard)", aboard(w)));
        let n = aboard(w);
        end(w, c, format!("{n} aboard"));
        // Phase 3: flee.
        begin(w, c, "scan: flee");
        stock_in_cabin(w, "blorp_paste", Some(product(&["giggly"])), 1);
        ship_over(w, "bent_spoon_inspect", 20.0);
        true
    }));
    s.push(wait(1.0));
    s.push(land("scan: land a third time, one crate in sight"));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let open = gp(w).smuggling.open_scan.is_some();
        check(c, open, "scan: one crate in sight: the scan waits".into());
        c.v.insert("heat", gp(w).smuggling.heat.points());
        true
    }));
    s.push(hold_until("scan: take off during the scan", &[KeyCode::Space], 6.0, |w| gp(w).smuggling.open_scan.is_none() && above_ground(w) > 4.0));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let up = gp(w).smuggling.heat.points() - c.v["heat"];
        check(c, logged(w, "Flee"), "scan: taking off is fleeing".into());
        check(c, up > 20.0, format!("scan: fleeing costs heat (+{up:.0})"));
        check(c, aboard(w) == 3, format!("scan: and keeps the goods ({} aboard)", aboard(w)));
        end(w, c, format!("heat +{up:.0}"));
        true
    }));
}

// ---------------------------------------------------------------- dealing

const BUYER: &str = "aunt_brisket";

fn buyer_order(w: &World) -> Option<(gameplay_core::OrderId, customers_core::ProductOrder)> {
    gp(w).customers.product_orders().find(|(_, o)| o.customer.as_str() == BUYER).map(|(i, o)| (i, o.clone()))
}

pub fn dealing_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, c| {
        begin(w, c, "dealing: a buyer orders by effect");
        crate::scenario::cargo::clear_crates(w);
        let ids: Vec<customers_core::CustomerId> = gp(w).customer_content.customers.keys().cloned().collect();
        let mut g = w.resource_mut::<Gameplay>();
        for id in ids {
            g.customers.set_wait(&id, if id.as_str() == BUYER { 0.5 } else { 1e9 });
        }
        true
    }));
    s.push(wait(2.0));
    s.push(Box::new(|w, c| {
        let o = buyer_order(w);
        check(c, o.is_some(), format!("dealing: {BUYER} orders ({:?})", o.as_ref().map(|(_, o)| (&o.wants, o.amount, o.price))));
        let Some((_, o)) = o else { return true };
        c.v.insert("price", o.price as f64);
        tap(w, KeyCode::Backquote);
        tap(w, KeyCode::ArrowRight);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let p = hud(w).phone.clone();
        check(c, p.tab == crate::smuggling::PhoneTab::Orders && p.rows.iter().any(|r| r.contains("Brisket")), format!("dealing: the order is in the phone ('{}')", p.rows.first().cloned().unwrap_or_default()));
        tap(w, KeyCode::KeyZ);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let Some((_, o)) = buyer_order(w) else {
            check(c, false, "dealing: the order is still open".into());
            return true;
        };
        check(c, o.countered, format!("dealing: Z asks for more: answered, price {} → {}", c.v["price"], o.price));
        c.v.insert("price", o.price as f64);
        tap(w, KeyCode::Backquote);
        // Product crates with what the buyer wants, set down near them; the walker beside them.
        let (spot, up) = buyer_spot(gp(w), &o.at, 0).expect("the buyer's pad");
        let side = up.any_orthonormal_vector();
        let p = ProductSpec { base: CommodityId::new("blorp_paste"), effects: o.wants.iter().cloned().collect(), quality: 0.95 };
        for i in 0..o.amount {
            stock_at(w, "blorp_paste", Some(p.clone()), spot + side * (2.0 + i as f64 * 1.2) + up.cross(side) * 2.0);
        }
        let pl = planet(w);
        let d = pl.up(spot + side * 1.0);
        place_walker(w, pl.centre + d * (pl.surface(d) + 0.05));
        face_towards(w, spot);
        c.v.insert("wallet", wallet(w) as f64);
        c.v.insert("heat", gp(w).smuggling.heat.points());
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let p = prompt_of(w);
        check(c, p.contains("hand"), format!("dealing: at the buyer the prompt offers the handover ('{p}')"));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let paid = wallet(w) as f64 - c.v["wallet"];
        check(c, buyer_order(w).is_none(), "dealing: the order is settled".into());
        check(c, paid >= c.v["price"] * 0.9, format!("dealing: paid {paid:.0} of {:.0} (full match)", c.v["price"]));
        check(c, track(w, "deals_done") == 1, format!("dealing: the family counts the deal ({})", track(w, "deals_done")));
        check(c, gp(w).smuggling.heat.points() > c.v["heat"], "dealing: a deal warms the heat".into());
        end(w, c, format!("paid {paid:.0}"));
        true
    }));
}

// ---------------------------------------------------------------- the whole run

pub fn smuggle_run_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, c| {
        begin(w, c, "smuggle_run: rise to the lab");
        crate::scenario::cargo::clear_crates(w);
        c.v.insert("wallet0", wallet(w) as f64);
        push(w, WorldEvent::FlagRaised { flag: gameplay_core::Flag::new("job_completed:courier_lint_trap") });
        push(w, WorldEvent::FlagRaised { flag: gameplay_core::Flag::new("job_completed:courier_noodle_post") });
        push(w, WorldEvent::TributePaid { ladder: "gribbles".into(), amount: 0 });
        let ids: Vec<customers_core::CustomerId> = gp(w).customer_content.customers.keys().cloned().collect();
        let mut g = w.resource_mut::<Gameplay>();
        for id in ids {
            g.customers.set_wait(&id, 1e9);
        }
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        check(c, gp(w).progress.has_tag(&Tag::new("lab_access")), format!("smuggle_run: rank {} opens the lab", gp(w).smuggling.empire.rank("gribbles")));
        c.v.insert("goal0", 0.0);
        let line = hud(w).goal_line.clone();
        w.resource_mut::<Gameplay>().log.push(format!("scenario: goal before: {line}"));
        true
    }));
    // Brew, as in `lab`.
    for item in ["blorp_paste", "space_sugar", "moth_dust"] {
        s.push(Box::new(move |w, _| {
            at_kettle_with(w, item);
            true
        }));
        s.push(wait(0.5));
        s.push(Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }));
        s.push(wait(0.6));
        s.push(Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }));
        s.push(wait(0.5));
    }
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(Box::new(|w, c| matches!(gp(w).smuggling.lab.kettle("kettle_1"), Some(production_core::Kettle::Done { .. })) && hud(w).brew.is_some() || c.t > 20.0));
    s.push(Box::new(|w, c| {
        let b = hud(w).brew;
        if b.is_some_and(|b| (b.needle - b.sweet).abs() < 0.04) || c.t > 3.0 {
            tap(w, KeyCode::KeyF);
            return true;
        }
        false
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let made: Vec<ProductSpec> = w.query::<&Stock>().iter(w).filter_map(|s| s.product.clone()).collect();
        check(c, made.len() == 2, format!("smuggle_run: brewed {} crates of {:?} q{:.2}", made.len(), made.first().map(|p| &p.effects), made.first().map_or(0.0, |p| p.quality)));
        end(w, c, "brewed".into());
        begin(w, c, "smuggle_run: a buyer in Bent Spoon wants it");
        // A Bent Spoon buyer whose taste the brew matches orders now.
        let mut g = w.resource_mut::<Gameplay>();
        g.customers.set_wait(&customers_core::CustomerId::new(BUYER), 0.5);
        true
    }));
    s.push(wait(2.0));
    s.push(Box::new(|w, c| {
        let o = buyer_order(w);
        check(c, o.is_some(), format!("smuggle_run: {BUYER} orders {:?}", o.as_ref().map(|(_, o)| (&o.wants, o.amount, o.price, o.at.to_string()))));
        // Carried aboard (test hook: the product crates move into the hold).
        let made: Vec<(Entity, Stock)> = w.query::<(Entity, &Stock)>().iter(w).filter(|(_, s)| s.product.is_some()).map(|(e, s)| (e, s.clone())).collect();
        let n = made.len() as u32;
        let p = made.first().and_then(|(_, s)| s.product.clone());
        for (e, _) in made {
            w.despawn(e);
        }
        stock_in_cabin(w, "blorp_paste", p, n);
        c.v.insert("aboard", n as f64);
        true
    }));
    s.extend(seated());
    s.push(Box::new(|w, c| {
        let n = aboard(w);
        end(w, c, format!("{n} crates aboard"));
        begin(w, c, "smuggle_run: fly to Bent Spoon, hail");
        heat_to(w, 3);
        let p = super::deliver::pad(w, "bent_spoon");
        let east = p.up.any_orthonormal_vector();
        teleport_ship(w, p.centre + east * 500.0 + p.up * 100.0, basis_for_up(p.up));
        true
    }));
    s.push(wait(1.0));
    s.push(hail_until_cleared("smuggle_run: L calls Bent Spoon's tower"));
    s.push(Box::new(|w, c| {
        let cl = clearance(w);
        check(c, cl.is_some(), format!("smuggle_run: the tower assigns {cl:?}"));
        let pad = cl.map(|(p, _)| p).unwrap_or_else(|| "bent_spoon_inspect".into());
        ship_over(w, &pad, 20.0);
        c.v.insert("wallet_before_scan", wallet(w) as f64);
        true
    }));
    s.push(wait(1.0));
    s.push(land("smuggle_run: land on the assigned pad"));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let open = gp(w).smuggling.open_scan.is_some();
        check(c, open, "smuggle_run: hot as it is, the inspection scans".into());
        tap(w, KeyCode::Digit2);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        let bribe = c.v["wallet_before_scan"] - wallet(w) as f64;
        check(c, logged(w, "Bribe, confiscate false"), format!("smuggle_run: a bribe ({bribe:.0} cr) and they look away"));
        check(c, aboard(w) as f64 == c.v["aboard"], format!("smuggle_run: the goods stay aboard ({})", aboard(w)));
        // Unloaded at the buyer (test hook: the crates move from the hold to the buyer's feet).
        let Some((_, o)) = buyer_order(w) else { return true };
        let ship = ship_entity(w);
        let held: Vec<(Entity, Stock)> = w.query::<(Entity, &Stock, &Crate)>().iter(w).filter(|(_, _, cr)| cr.ship == Some(ship)).map(|(e, s, _)| (e, s.clone())).collect();
        let (spot, up) = buyer_spot(gp(w), &o.at, 0).expect("the buyer's pad");
        let side = up.any_orthonormal_vector();
        for (i, (e, s)) in held.into_iter().enumerate() {
            w.despawn(e);
            stock_at(w, s.commodity.as_str(), s.product, spot + side * (2.0 + i as f64 * 1.2) + up.cross(side) * 2.0);
        }
        // The pilot gets up and walks over.
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let Some((_, o)) = buyer_order(w) else { return true };
        let (spot, up) = buyer_spot(gp(w), &o.at, 0).expect("the buyer's pad");
        let side = up.any_orthonormal_vector();
        with_player(w, |p| {
            p.ship = None;
            p.seated = false;
        });
        let pl = planet(w);
        let d = pl.up(spot + side * 1.0);
        place_walker(w, pl.centre + d * (pl.surface(d) + 0.05));
        face_towards(w, spot);
        c.v.insert("wallet", wallet(w) as f64);
        c.v.insert("price", o.price as f64);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let p = prompt_of(w);
        check(c, p.contains("hand"), format!("smuggle_run: at the buyer: '{p}'"));
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let paid = wallet(w) as f64 - c.v["wallet"];
        check(c, paid > 0.0, format!("smuggle_run: payday: {paid:.0} cr for an order of {:.0}", c.v["price"]));
        check(c, track(w, "deals_done") >= 1 && track(w, "recipes_found") >= 1, format!("smuggle_run: the family counts {} deal(s), {} recipe(s)", track(w, "deals_done"), track(w, "recipes_found")));
        let line = hud(w).goal_line.clone();
        let before = gp(w).log.iter().find_map(|l| l.strip_prefix("scenario: goal before: ")).unwrap_or("").to_string();
        check(c, !line.is_empty() && line != before, format!("smuggle_run: the next goal moved: '{before}' → '{line}'"));
        end(w, c, format!("paid {paid:.0}, net {:+.0} cr since the start", wallet(w) as f64 - c.v["wallet0"]));
        true
    }));
}
