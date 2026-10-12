//! #287: the dev area's storage through `Controls` and the pointer only. A syringe from the
//! medical cabinet is refused by the ingredient shelf; goods from the shelf go into the locker; the
//! secure locker refuses without the ID card and opens with the card from the locker; a stack moves
//! from the medical cabinet to the supply crate; "store all" empties the hands. Nothing is lost.
//! Run with `--start=dev_area`.
use super::dev_area::{active, tap_until};
use super::lab_devices::{click_until, display_of, go_use, hands, hands_slot, take_from};
use super::*;
use crate::dev_area::storage::{self, Storage};
use crate::displays::ActiveDisplay;
use container_core::Access;
use display_core::PanelId;
use std::collections::BTreeMap;

/// Items by kind in every cabinet and the hands.
fn totals(w: &World) -> BTreeMap<String, u32> {
    let mut t = BTreeMap::new();
    let mut add = |c: &container_core::Container<crate::dev_area::science::LabItem>| {
        for i in 0..c.len() {
            if let Some(s) = c.slot(i) {
                *t.entry(s.item.kind.to_string()).or_insert(0) += s.count;
            }
        }
    };
    for cab in &w.resource::<Storage>().cabinets {
        add(&cab.container);
    }
    add(&hands(w).0);
    t
}

fn cabinet(w: &World, id: &PanelId) -> crate::dev_area::storage::Cabinet {
    w.resource::<Storage>().get(id).expect("cabinet").clone()
}

fn line(w: &World, id: &PanelId) -> String {
    cabinet(w, id).line.0
}

/// The slot of the first stack of `kind` in a cabinet, as an action `take:<i>`.
fn take_kind(id: PanelId, kind: &'static str) -> impl Fn(&World) -> Option<String> + Send + Sync + 'static {
    move |w| {
        let c = cabinet(w, &id).container;
        (0..c.len()).find(|&i| c.slot(i).is_some_and(|s| s.item.kind.as_str() == kind)).map(|i| format!("take:{i}"))
    }
}

/// The hands slot of `kind`, as an action `store:<i>`.
fn store_kind(kind: &'static str) -> impl Fn(&World) -> Option<String> + Send + Sync + 'static {
    move |w| hands_slot(w, kind).map(|i| format!("store:{i}"))
}

fn count(w: &World, id: &PanelId, kind: &str) -> u32 {
    cabinet(w, id).container.count(&container_core::ItemKind::new(kind))
}

pub(super) fn cabinets_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        begin(w, c, "cabinets: the storage row");
        let ids = [storage::SHELF, storage::MEDICAL, storage::LOCKER, storage::SECURE, storage::CRATE];
        let ok = ids.iter().all(|id| display_of(w, id).is_some());
        check(c, ok, "cabinets: shelf, medical cabinet, locker, secure locker and supply crate stand with their displays".into());
        let t = totals(w);
        println!("cabinets: start totals {t:?}");
        for (k, n) in &t {
            c.v.insert(Box::leak(format!("start:{k}").into_boxed_str()), *n as f64);
        }
        let secure = cabinet(w, &storage::SECURE);
        check(c, matches!(secure.container.access(), Access::NeedsKey(_)), "cabinets: the secure locker needs a key".into());
        true
    }));
    // A syringe from the medical cabinet.
    s.extend(go_use(storage::MEDICAL));
    s.push(click_until(storage::MEDICAL, take_kind(storage::MEDICAL, "syringe"), None, "cabinets: a syringe from the medical cabinet into the hands", 1.0, |w| hands_slot(w, "syringe").is_some()));
    s.push(shot_step("medical"));
    // The shelf takes only lab goods.
    s.extend(go_use(storage::SHELF));
    s.push(click_until(storage::SHELF, store_kind("syringe"), None, "cabinets: the shelf refuses the syringe", 1.0, |w| line(w, &storage::SHELF).contains("does not take that")));
    s.push(Box::new(|w, c| {
        let l = line(w, &storage::SHELF);
        check(c, hands_slot(w, "syringe").is_some(), format!("cabinets: the syringe stays in the hands, the panel says {l:?}"));
        true
    }));
    s.push(shot_step("shelf-refused"));
    s.push(take_from(storage::SHELF, "moth_dust", "cabinets: moth dust from the shelf (paging)"));
    // The secure locker without the card.
    s.extend(go_use(storage::SECURE));
    s.push(click_until(storage::SECURE, store_kind("moth_dust"), None, "cabinets: the secure locker refuses without the ID card", 1.0, |w| line(w, &storage::SECURE).contains("needs an id card")));
    s.push(Box::new(|w, c| {
        let panel = w.resource::<crate::displays::Panels>().get(&storage::SECURE).cloned().unwrap();
        let hidden = panel.find("take:0").is_none();
        check(c, hidden && count(w, &storage::SECURE, "moth_dust") == 0, "cabinets: locked, its contents are hidden and nothing went in".into());
        true
    }));
    s.push(shot_step("secure-locked"));
    // The card from the locker, and the goods into the locker.
    s.extend(go_use(storage::LOCKER));
    s.push(click_until(storage::LOCKER, take_kind(storage::LOCKER, "id_card"), None, "cabinets: the ID card from the locker", 1.0, |w| hands_slot(w, "id_card").is_some()));
    s.push(click_until(storage::LOCKER, store_kind("moth_dust"), None, "cabinets: the moth dust into the locker", 1.0, |w| count(w, &storage::LOCKER, "moth_dust") > 0 && hands_slot(w, "moth_dust").is_none()));
    s.push(shot_step("locker"));
    // With the card the secure locker opens.
    s.extend(go_use(storage::SECURE));
    s.push(click_until(storage::SECURE, take_kind(storage::SECURE, "syringe"), None, "cabinets: with the card the secure locker opens and gives its syringe", 1.0, |w| count(w, &storage::SECURE, "syringe") == 0));
    s.push(shot_step("secure-open"));
    // A stack from the medical cabinet to the supply crate.
    s.extend(go_use(storage::MEDICAL));
    s.push(Box::new(|w, c| {
        c.v.insert("crate0", count(w, &storage::CRATE, "syringe") as f64);
        c.v.insert("medical0", count(w, &storage::MEDICAL, "syringe") as f64);
        true
    }));
    s.push(click_until(storage::MEDICAL, crate::scenario::lab_devices::act("take_all"), None, "cabinets: take all from the medical cabinet", 1.0, |w| count(w, &storage::MEDICAL, "syringe") == 0));
    s.extend(go_use(storage::CRATE));
    s.push(click_until(storage::CRATE, crate::scenario::lab_devices::act("store_all"), None, "cabinets: store all into the supply crate", 1.0, |w| hands(w).0.is_empty()));
    s.push(shot_step("crate"));
    s.push(Box::new(|w, c| {
        let moved = count(w, &storage::CRATE, "syringe") as f64 - c.v["crate0"];
        // The crate got the medical cabinet's syringe and the two others from the hands.
        check(c, moved == c.v["medical0"] + 2.0, format!("cabinets: {moved} syringes in the supply crate now ({} from the medical cabinet)", c.v["medical0"]));
        let t = totals(w);
        let lost: Vec<String> = t.iter().filter(|(k, n)| c.v.get(format!("start:{k}").as_str()).copied() != Some(**n as f64)).map(|(k, n)| format!("{k}: {n}")).collect();
        let kinds_same = c.v.keys().filter(|k| k.starts_with("start:")).count() == t.len();
        check(c, lost.is_empty() && kinds_same, format!("cabinets: nothing lost or duplicated ({t:?}; differs: {lost:?})"));
        end(w, c, format!("totals {t:?}"));
        true
    }));
    s.push(tap_until(KeyCode::KeyF, "cabinets: F leaves the crate", 1.0, |w| active(w) == ActiveDisplay::Idle));
}
