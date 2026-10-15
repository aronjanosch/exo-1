//! The lab: mixing, the brew timing, the kettles, the recipe book, the save.
//! Every test loads the shipped `content/gameplay` unless it says it builds its own records.
use std::collections::BTreeSet;
use std::path::Path;

use content_core::Loaded;
use gameplay_core::notice::NoticeKind;
use gameplay_core::save::Envelope;
use gameplay_core::text::TextTable;
use gameplay_core::{ClientId, CommodityId, Content, EffectId, Event, File, ProductSpec, Tag, WorldEvent};
use production_core::*;

fn content_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/gameplay")
}

fn shipped_files() -> Vec<File> {
    let root = content_root();
    let mut out = Vec::new();
    for d in std::fs::read_dir(&root).unwrap() {
        let d = d.unwrap().path();
        if !d.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&d).unwrap() {
            let f = f.unwrap().path();
            out.push(File::new(f.strip_prefix(&root).unwrap().to_str().unwrap(), std::fs::read_to_string(&f).unwrap()));
        }
    }
    out
}

fn kernel() -> Content {
    Content::load(&shipped_files(), &["small", "medium", "large"]).expect("shipped kernel loads")
}

fn lab_content() -> LabContent {
    LabContent::load(&shipped_files(), &kernel()).unwrap_or_else(|e| panic!("shipped lab content: {e:#?}"))
}

fn cid(s: &str) -> CommodityId {
    CommodityId::new(s)
}

fn eid(s: &str) -> EffectId {
    EffectId::new(s)
}

fn set(xs: &[&str]) -> BTreeSet<EffectId> {
    xs.iter().map(|x| eid(x)).collect()
}

/// The shipped files with one replaced (or added) and an optional one removed.
fn with_file(path: &str, text: Option<&str>) -> Vec<File> {
    let mut f: Vec<File> = shipped_files().into_iter().filter(|f| f.path != path).collect();
    if let Some(t) = text {
        f.push(File::new(path, t));
    }
    f
}

fn load_with(path: &str, text: &str) -> Vec<String> {
    LabContent::load(&with_file(path, Some(text)), &kernel()).unwrap_err()
}

/// The brewed event and the notice of a release: (product, crates, discovered).
fn brewed_of(out: &[LabOutcome]) -> (ProductSpec, u32, bool) {
    out.iter()
        .find_map(|o| match o {
            LabOutcome::Emit(WorldEvent::Brewed { product, crates, discovered, .. }) => Some((product.clone(), *crates, *discovered)),
            _ => None,
        })
        .expect("a release emits Brewed")
}

/// Fills an empty kettle with a base and ingredients, brews it through, releases at the sweet
/// spot and takes the product.
fn brew_once(lab: &mut Lab, lc: &LabContent, station: &str, base: &str, ings: &[&str]) -> (ProductSpec, u32, bool) {
    lab.insert(lc, station, &cid(base)).unwrap();
    for g in ings {
        lab.insert(lc, station, &cid(g)).unwrap();
    }
    lab.start(lc, station).unwrap();
    lab.step(lc.tuning.brew_s);
    let t = lc.tuning.sweet_spot * lc.tuning.needle_period_s / 2.0;
    let out = lab.release(lc, station, t).unwrap();
    let (product, crates, discovered) = brewed_of(&out);
    assert_eq!(lab.take(station), Some((product.clone(), crates)), "the released brew is taken");
    (product, crates, discovered)
}

// ---------- mixing (pure) ----------

#[test]
fn mix_order_matters() {
    let lc = lab_content();
    let base = &lc.bases[&cid("blorp_paste")].record;
    let a = &lc.ingredients[&cid("space_sugar")].record;
    let b = &lc.ingredients[&cid("battery_juice")].record;
    let ab = mix(base, &[a, b]);
    let ba = mix(base, &[b, a]);
    assert_ne!(ab, ba, "A then B must not equal B then A");
    assert_eq!(ab, set(&["glowing", "tingly"]));
    assert_eq!(ba, set(&["tingly", "giggly"]));
}

fn ingredient(id: &str, adds: Option<&str>, transforms: &[(&str, &str)]) -> Ingredient {
    Ingredient {
        id: cid(id),
        adds: adds.map(eid),
        transforms: transforms.iter().map(|(f, t)| Transform { from: eid(f), to: eid(t) }).collect(),
    }
}

fn base_with(effects: &[&str], max: u8) -> ProductBase {
    ProductBase { id: cid("test_base"), effects: effects.iter().map(|e| eid(e)).collect(), max_effects: max, crates_out: 2 }
}

#[test]
fn a_transform_replaces_an_effect_only_when_its_target_is_absent() {
    let base = base_with(&["sleepy"], 6);
    let replace = ingredient("x", None, &[("sleepy", "giggly")]);
    assert_eq!(mix(&base, &[&replace]), set(&["giggly"]), "from present, to absent: replaced");

    let base2 = base_with(&["sleepy", "giggly"], 6);
    assert_eq!(mix(&base2, &[&replace]), set(&["sleepy", "giggly"]), "to already present: nothing changes");

    let none = ingredient("y", None, &[("floaty", "time_lag")]);
    assert_eq!(mix(&base, &[&none]), set(&["sleepy"]), "from absent: nothing changes");
}

#[test]
fn an_ingredient_transforms_before_it_adds() {
    let base = base_with(&[], 6);
    let adds_and_changes_itself = ingredient("z", Some("floaty"), &[("floaty", "time_lag")]);
    assert_eq!(mix(&base, &[&adds_and_changes_itself]), set(&["floaty"]), "its own add is not transformed by it");
}

#[test]
fn the_effect_cap_refuses_new_effects_and_keeps_the_old() {
    let base = base_with(&[], 2);
    let a = ingredient("a", Some("sleepy"), &[]);
    let b = ingredient("b", Some("giggly"), &[]);
    let c = ingredient("c", Some("tingly"), &[]);
    let again = ingredient("d", Some("sleepy"), &[]);
    assert_eq!(mix(&base, &[&a, &b, &c]), set(&["sleepy", "giggly"]), "the third new effect is refused");
    assert_eq!(mix(&base, &[&a, &again]), set(&["sleepy"]), "an effect already in the mix is not added twice");
}

#[test]
fn the_needle_is_a_triangle_wave_over_one_period() {
    assert!(needle(0.0, 1.6).abs() < 1e-9);
    assert!((needle(0.4, 1.6) - 0.5).abs() < 1e-9);
    assert!((needle(0.8, 1.6) - 1.0).abs() < 1e-9);
    assert!((needle(1.2, 1.6) - 0.5).abs() < 1e-9);
    assert!(needle(1.6, 1.6).abs() < 1e-9);
    assert!((needle(2.4, 1.6) - 1.0).abs() < 1e-9, "the second sweep starts again");
}

#[test]
fn quality_peaks_at_the_sweet_spot_and_falls_to_zero_at_the_width() {
    let t = lab_content().tuning;
    let (s, w) = (t.sweet_spot, t.sweet_width);
    assert_eq!(quality(s, &t), 1.0);
    assert!((quality(s + w / 2.0, &t) - 0.5).abs() < 1e-9);
    assert!((quality(s - w / 2.0, &t) - 0.5).abs() < 1e-9);
    assert_eq!(quality(s + w, &t), 0.0);
    assert_eq!(quality(s - w, &t), 0.0);
    assert_eq!(quality(s + 3.0 * w, &t), 0.0, "clamped, never negative");
}

#[test]
fn price_is_base_times_effect_factor_times_quality_factor() {
    let k = kernel();
    let blorp = k.commodities[&cid("blorp_paste")].record.base_price;
    let plain = ProductSpec { base: cid("blorp_paste"), effects: BTreeSet::new(), quality: 1.0 };
    assert_eq!(price(&plain, &k), Some(blorp), "no effects, full quality: the base price");

    let bad = ProductSpec { quality: 0.0, ..plain.clone() };
    assert_eq!(price(&bad, &k), Some((blorp as f64 * 0.5).round() as i64), "quality 0 halves it");

    let dg = k.commodities[&cid("dream_goo")].record.base_price;
    let sleepy = k.effects[&eid("sleepy")].record.value;
    let fancy = ProductSpec { base: cid("dream_goo"), effects: set(&["sleepy"]), quality: 1.0 };
    assert_eq!(price(&fancy, &k), Some((dg as f64 * (1.0 + sleepy)).round() as i64), "an effect adds its value");

    let half = ProductSpec { quality: 0.5, ..fancy.clone() };
    assert_eq!(price(&half, &k), Some((dg as f64 * (1.0 + sleepy) * 0.75).round() as i64), "quality 0.5 is factor 0.75");

    let unknown = ProductSpec { base: cid("not_a_good"), effects: BTreeSet::new(), quality: 1.0 };
    assert_eq!(price(&unknown, &k), None, "an unknown base has no price");
}

// ---------- kettles ----------

#[test]
fn the_kettle_refuses_what_does_not_fit() {
    let lc = lab_content();
    let mut lab = Lab::new();
    assert_eq!(lab.insert(&lc, "vat", &cid("parcel")), Err(LabRefusal::UnknownGoods));
    assert_eq!(lab.insert(&lc, "vat", &cid("space_sugar")), Err(LabRefusal::IngredientBeforeBase));
    assert_eq!(lab.start(&lc, "vat"), Err(LabRefusal::NoBase));
    assert_eq!(lab.kettle("vat"), None, "refused inserts leave no kettle behind");

    lab.insert(&lc, "vat", &cid("blorp_paste")).unwrap();
    assert_eq!(lab.insert(&lc, "vat", &cid("dream_goo")), Err(LabRefusal::SecondBase));
    for g in ["space_sugar", "battery_juice", "pickle_brine"] {
        lab.insert(&lc, "vat", &cid(g)).unwrap();
    }
    assert_eq!(lab.insert(&lc, "vat", &cid("moth_dust")), Err(LabRefusal::TooManyIngredients));
    assert_eq!(lab.release(&lc, "vat", 0.0).err(), Some(LabRefusal::NotDone), "a loaded kettle is not brewed");

    lab.start(&lc, "vat").unwrap();
    assert_eq!(lab.insert(&lc, "vat", &cid("moth_dust")), Err(LabRefusal::Busy));
    assert_eq!(lab.start(&lc, "vat"), Err(LabRefusal::Busy));

    lab.step(lc.tuning.brew_s);
    assert_eq!(lab.insert(&lc, "vat", &cid("moth_dust")), Err(LabRefusal::Busy), "a done kettle takes nothing");

    let t = lc.tuning.sweet_spot * lc.tuning.needle_period_s / 2.0;
    lab.release(&lc, "vat", t).unwrap();
    assert_eq!(lab.insert(&lc, "vat", &cid("moth_dust")), Err(LabRefusal::Busy), "a ready kettle takes nothing");
    assert_eq!(lab.start(&lc, "vat"), Err(LabRefusal::Busy));

    lab.insert(&lc, "other", &cid("blorp_paste")).unwrap();
    assert!(matches!(lab.kettle("other"), Some(Kettle::Loaded { .. })), "other stations are independent");
}

#[test]
fn a_full_brew_goes_from_kettle_to_release_to_take() {
    let lc = lab_content();
    let mut lab = Lab::new();
    lab.insert(&lc, "vat", &cid("blorp_paste")).unwrap();
    lab.insert(&lc, "vat", &cid("space_sugar")).unwrap();
    lab.insert(&lc, "vat", &cid("battery_juice")).unwrap();
    lab.start(&lc, "vat").unwrap();

    lab.step(lc.tuning.brew_s / 2.0);
    assert!(matches!(lab.kettle("vat"), Some(Kettle::Brewing { .. })), "still brewing halfway");
    lab.step(lc.tuning.brew_s);
    assert!(matches!(lab.kettle("vat"), Some(Kettle::Done { .. })), "done waits for the release");

    let t = lc.tuning.sweet_spot * lc.tuning.needle_period_s / 2.0;
    let out = lab.release(&lc, "vat", t).unwrap();
    let (product, crates, discovered) = brewed_of(&out);
    assert_eq!(product.base, cid("blorp_paste"));
    assert_eq!(product.effects, set(&["glowing", "tingly"]));
    assert!((product.quality - 1.0).abs() < 1e-9, "released at the sweet spot");
    assert_eq!(crates, lc.bases[&cid("blorp_paste")].record.crates_out);
    assert!(discovered, "the first brew of this mix is a discovery");
    assert!(
        out.iter().any(|o| matches!(o, LabOutcome::Notice(n) if n.kind == NoticeKind::Unlock && n.key.as_str() == production_core::lab::DISCOVERED_KEY)),
        "a discovery gets the fanfare notice"
    );
    assert!(matches!(lab.kettle("vat"), Some(Kettle::Ready { .. })));

    assert_eq!(lab.take("vat"), Some((product, crates)));
    assert_eq!(lab.kettle("vat"), Some(&Kettle::Empty), "taking empties the kettle");
    assert_eq!(lab.take("vat"), None, "nothing to take twice");
}

#[test]
fn the_release_moment_sets_the_quality() {
    let lc = lab_content();
    let period = lc.tuning.needle_period_s;
    let release_quality = |t: f64| {
        let mut lab = Lab::new();
        lab.insert(&lc, "vat", &cid("blorp_paste")).unwrap();
        lab.start(&lc, "vat").unwrap();
        lab.step(lc.tuning.brew_s);
        let out = lab.release(&lc, "vat", t).unwrap();
        brewed_of(&out).0.quality
    };
    let at_sweet = release_quality(lc.tuning.sweet_spot * period / 2.0);
    assert!((at_sweet - 1.0).abs() < 1e-9, "the sweet spot gives quality 1");
    let at_top = release_quality(period / 2.0);
    assert!(at_top.abs() < 1e-9, "the needle at the top of its sweep (1.0) is outside the sweet width: quality 0");
    // 0.5 s into a 1.6 s sweep the needle is at 0.625: half the sweet width away, quality 0.5.
    let between = release_quality(0.5);
    assert!((between - 0.5).abs() < 1e-9, "the needle at 0.625 is half a width off: {between}");
}

#[test]
fn a_mix_is_discovered_only_the_first_time() {
    let lc = lab_content();
    let mut lab = Lab::new();
    let (_, _, first) = brew_once(&mut lab, &lc, "vat", "blorp_paste", &["space_sugar", "battery_juice"]);
    let (_, _, second) = brew_once(&mut lab, &lc, "vat", "blorp_paste", &["space_sugar", "battery_juice"]);
    assert!(first, "the first brew discovers the mix");
    assert!(!second, "the same mix again is not a discovery");
    let (_, _, other) = brew_once(&mut lab, &lc, "vat", "blorp_paste", &["battery_juice", "space_sugar"]);
    assert!(other, "the reverse order is another mix, a new discovery");
    assert_eq!(lab.book().len(), 2);
}

#[test]
fn preview_shows_the_effects_only_for_a_mix_already_in_the_book() {
    let lc = lab_content();
    let mut lab = Lab::new();
    assert_eq!(lab.preview(&lc, "vat"), None, "no kettle, nothing to preview");

    lab.insert(&lc, "vat", &cid("blorp_paste")).unwrap();
    lab.insert(&lc, "vat", &cid("space_sugar")).unwrap();
    lab.insert(&lc, "vat", &cid("battery_juice")).unwrap();
    assert_eq!(lab.preview(&lc, "vat"), None, "an unknown mix shows ???");
    lab.start(&lc, "vat").unwrap();
    lab.step(lc.tuning.brew_s);
    let t = lc.tuning.sweet_spot * lc.tuning.needle_period_s / 2.0;
    lab.release(&lc, "vat", t).unwrap();
    lab.take("vat").unwrap();

    lab.insert(&lc, "vat", &cid("blorp_paste")).unwrap();
    lab.insert(&lc, "vat", &cid("space_sugar")).unwrap();
    lab.insert(&lc, "vat", &cid("battery_juice")).unwrap();
    assert_eq!(lab.preview(&lc, "vat"), Some(set(&["glowing", "tingly"])), "a known mix shows its effects");

    lab.insert(&lc, "other", &cid("blorp_paste")).unwrap();
    lab.insert(&lc, "other", &cid("battery_juice")).unwrap();
    lab.insert(&lc, "other", &cid("space_sugar")).unwrap();
    assert_eq!(lab.preview(&lc, "other"), None, "the reverse order is not in the book, so it stays ???");
}

#[test]
fn time_passing_counts_the_brew_down_through_the_events() {
    let lc = lab_content();
    let mut lab = Lab::new();
    lab.insert(&lc, "vat", &cid("blorp_paste")).unwrap();
    lab.start(&lc, "vat").unwrap();
    let ev = Event::new(ClientId(1), 0, WorldEvent::TimePassed { dt: lc.tuning.brew_s });
    assert!(lab.apply_world(&ev).is_empty(), "the lab raises nothing on time");
    assert!(matches!(lab.kettle("vat"), Some(Kettle::Done { .. })), "the brew is done after its time");
}

#[test]
fn the_lab_round_trips_and_continues_the_same() {
    let lc = lab_content();
    let mut lab = Lab::new();
    brew_once(&mut lab, &lc, "vat", "blorp_paste", &["space_sugar"]);
    lab.insert(&lc, "other", &cid("dream_goo")).unwrap();
    lab.insert(&lc, "other", &cid("foam_gum")).unwrap();
    lab.start(&lc, "other").unwrap();
    lab.step(1.0);

    let mut env = Envelope::new();
    lab.save(&mut env);
    let back = Lab::load(&Envelope::from_json(&env.to_json()).unwrap()).unwrap().expect("the section is there");
    assert_eq!(back, lab);
    assert_eq!(back.book().len(), 1, "the book is saved");

    let (mut a, mut b) = (lab.clone(), back);
    a.step(lc.tuning.brew_s);
    b.step(lc.tuning.brew_s);
    assert_eq!(a, b, "both continue the same");
    assert!(matches!(a.kettle("other"), Some(Kettle::Done { .. })));
    assert!(Lab::load(&Envelope::new()).unwrap().is_none(), "no section, no lab");
}

// ---------- shipped content ----------

#[test]
fn shipped_content_loads_and_every_ingredient_effect_exists() {
    let k = kernel();
    let lc = lab_content();
    assert_eq!(lc.bases.len(), 2, "two product bases");
    assert_eq!(lc.ingredients.len(), 6, "six ingredients");
    for Loaded { record: ing, .. } in lc.ingredients.values() {
        let c = &k.commodities[&ing.id].record;
        assert!(!c.tags.contains(&Tag::new("contraband")), "{} is a legal good", ing.id);
        let adds = ing.adds.as_ref().unwrap_or_else(|| panic!("{} adds one effect", ing.id));
        assert!(k.effects.contains_key(adds), "{}: adds '{adds}' exists", ing.id);
        assert!((1..=3).contains(&ing.transforms.len()), "{} has 1 to 3 transforms", ing.id);
        for t in &ing.transforms {
            assert!(k.effects.contains_key(&t.from), "{}: from '{}' exists", ing.id, t.from);
            assert!(k.effects.contains_key(&t.to), "{}: to '{}' exists", ing.id, t.to);
        }
    }
    for Loaded { record: b, .. } in lc.bases.values() {
        let c = &k.commodities[&b.id].record;
        assert!(c.tags.contains(&Tag::new("contraband")), "{} is contraband", b.id);
        assert_eq!((b.max_effects, b.crates_out), (6, 2), "{}: starting numbers", b.id);
    }
    assert!(lc.bases[&cid("blorp_paste")].record.effects.is_empty());
    assert_eq!(lc.bases[&cid("dream_goo")].record.effects, vec![eid("sleepy")]);
    assert_eq!(lc.tuning.max_ingredients, 3);
}

#[test]
fn every_shipped_name_and_lab_line_has_its_text() {
    let text = std::fs::read_to_string(content_root().join("text/en.json")).unwrap();
    let table = TextTable::from_json("content/gameplay/text/en.json", &text).unwrap();
    let k = kernel();
    let lc = lab_content();
    for id in lc.bases.keys().chain(lc.ingredients.keys()) {
        let key = format!("commodity.{id}.name");
        assert!(table.has(&key), "no text '{key}'");
    }
    for ing in lc.ingredients.values() {
        let e = ing.record.adds.as_ref().unwrap();
        assert!(table.has(&format!("effect.{e}.name")), "no text for effect '{e}'");
        assert!(k.effects.contains_key(e));
    }
    for key in [production_core::lab::BREWED_KEY, production_core::lab::DISCOVERED_KEY] {
        let lines = table.lines(key).unwrap_or_else(|| panic!("no text '{key}'"));
        assert!(lines.len() >= 3, "'{key}' is a pool of at least 3 lines");
    }
}

// ---------- loader ----------

#[test]
fn loader_errors_name_file_and_field() {
    let e = load_with("ingredient/space_sugar.json", r#"{"id": "space_sugar", "adds": "nope", "transforms": []}"#);
    assert!(e.iter().any(|m| m.starts_with("ingredient/space_sugar.json") && m.contains("adds") && m.contains("nope")), "{e:?}");

    let e = load_with("ingredient/space_sugar.json", r#"{"id": "space_sugar", "adds": "giggly", "transforms": [{"from": "sleepy", "to": "nope"}]}"#);
    assert!(e.iter().any(|m| m.starts_with("ingredient/space_sugar.json") && m.contains("transforms[0].to")), "{e:?}");

    let e = load_with("ingredient/space_sugar.json", r#"{"id": "space_sugar", "adds": null, "transforms": []}"#);
    assert!(e.iter().any(|m| m.starts_with("ingredient/space_sugar.json") && m.contains("needs")), "an ingredient does something: {e:?}");

    let e = load_with("ingredient/space_sugar.json", r#"{"id": "space_sugar", "adds": "giggly", "transforms": [], "colour": 1}"#);
    assert!(e.iter().any(|m| m.starts_with("ingredient/space_sugar.json")), "unknown fields are errors: {e:?}");

    let e = load_with("ingredient/space_sugar.json", r#"{"id": "blorp_paste", "adds": "giggly", "transforms": []}"#);
    assert!(e.iter().any(|m| m.starts_with("ingredient/space_sugar.json") && m.contains("contraband")), "ingredients are legal: {e:?}");

    let e = load_with("product_base/blorp_paste.json", r#"{"id": "space_sugar", "effects": [], "max_effects": 6, "crates_out": 2}"#);
    assert!(e.iter().any(|m| m.starts_with("product_base/blorp_paste.json") && m.contains("contraband")), "bases are contraband: {e:?}");

    let e = load_with("product_base/blorp_paste.json", r#"{"id": "blorp_paste", "effects": ["sleepy", "tingly"], "max_effects": 1, "crates_out": 2}"#);
    assert!(e.iter().any(|m| m.starts_with("product_base/blorp_paste.json") && m.contains("max_effects")), "{e:?}");

    let e = load_with("product_base/blorp_paste.json", r#"{"id": "blorp_paste", "effects": [], "max_effects": 6, "crates_out": 0}"#);
    assert!(e.iter().any(|m| m.starts_with("product_base/blorp_paste.json") && m.contains("crates_out")), "{e:?}");
}

#[test]
fn tuning_is_checked_and_required() {
    let good = std::fs::read_to_string(content_root().join("lab/tuning.json")).unwrap();
    let bad = good.replace("\"sweet_width\": 0.25", "\"sweet_width\": 0.0");
    assert_ne!(good, bad, "the test edits the shipped value");
    let e = LabContent::load(&with_file("lab/tuning.json", Some(&bad)), &kernel()).unwrap_err();
    assert!(e.iter().any(|m| m.starts_with("lab/tuning.json") && m.contains("sweet_width")), "{e:?}");

    let e = LabContent::load(&with_file("lab/tuning.json", None), &kernel()).unwrap_err();
    assert!(e.iter().any(|m| m.contains("lab/tuning.json")), "a missing tuning file is an error: {e:?}");
}
