//! The lab device chain (#287): skill curves, heater, centrifuge, syringe filler, the tuning file
//! and the whole chain mixer -> heater -> centrifuge -> syringe. Loads the shipped content.
use std::collections::BTreeSet;
use std::path::Path;

use gameplay_core::{CommodityId, Content, EffectId, File};
use production_core::devices::*;
use production_core::skill::{self, Dice};
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

fn tuning() -> DeviceTuning {
    DeviceTuning::load(&shipped_files(), &kernel()).unwrap_or_else(|e| panic!("shipped device tuning: {e:#?}"))
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

/// A batch straight from the kettle: dream goo (sleepy) with space sugar (giggly).
fn batch(t: &DeviceTuning, quality: f64) -> Batch {
    Batch {
        base: cid("dream_goo"),
        ingredients: vec![cid("space_sugar")],
        effects: set(&["tingly", "giggly"]),
        quality,
        volume: 8.0,
        temp_c: t.ambient_c,
        heat: 0.0,
        burnt: false,
        separation: 0.0,
        bonus_tiers: None,
    }
}

const DT: f64 = 0.1;

// ---------- skill ----------

#[test]
fn skill_speed_is_one_at_the_requirement_and_faster_above() {
    let s = &tuning().skill;
    assert!((skill::speed(30.0, 30.0, s) - 1.0).abs() < 1e-9);
    assert!((skill::speed(100.0, 0.0, s) - s.fastest).abs() < 1e-9, "far above: the fastest");
    assert!(skill::speed(0.0, 100.0, s) >= s.slowest, "far below: never slower than the slowest");
    let mut last = 0.0;
    for k in 0..=100 {
        let v = skill::speed(k as f64, 30.0, s);
        assert!(v >= last, "speed grows with skill");
        last = v;
    }
    // No jump at the requirement.
    let below = skill::speed(29.999, 30.0, s);
    assert!((below - 1.0).abs() < 0.01, "continuous at the requirement: {below}");
}

#[test]
fn bonus_chances_follow_the_crafting_shape() {
    let s = &tuning().skill;
    assert_eq!(skill::bonus_chances(49.0, 5.0, s), (0.0, 0.0), "no bonus below the first threshold");
    let (p1, p2) = skill::bonus_chances(60.0, 5.0, s);
    assert!((p1 - 36.0 / 76.0).abs() < 1e-9, "skill 60 at requirement 5: {p1}");
    assert_eq!(p2, 0.0, "the second tier needs its own threshold");
    assert_eq!(skill::bonus_chances(100.0, 30.0, s), (1.0, 1.0), "a master always gets both");
    let (_, p2) = skill::bonus_chances(100.0, 95.0, s);
    assert_eq!(p2, 0.0, "a very hard requirement leaves no second tier");
}

#[test]
fn dice_are_seeded_and_chances_hold_at_the_ends() {
    let (mut a, mut b) = (Dice::new(7), Dice::new(7));
    for _ in 0..50 {
        let r = a.roll();
        assert_eq!(r, b.roll());
        assert!((0.0..1.0).contains(&r));
    }
    let mut d = Dice::new(3);
    assert!((0..100).all(|_| d.chance(1.0)));
    assert!((0..100).all(|_| !d.chance(0.0)));
}

// ---------- heater ----------

/// Runs the heater for `secs` at the setpoint.
fn heat(h: &mut Heater, t: &DeviceTuning, setpoint: f64, secs: f64, skill: f64) {
    h.set_setpoint(setpoint, t);
    for _ in 0..(secs / DT).round() as usize {
        h.step(DT, skill, t);
    }
}

#[test]
fn the_batch_follows_the_setpoint_with_lag() {
    let t = tuning();
    let mut h = Heater::new(&t);
    h.load(batch(&t, 0.5)).unwrap();
    heat(&mut h, &t, 75.0, 1.0, 30.0);
    let after_1s = h.temperature().unwrap();
    assert!(after_1s > t.ambient_c && after_1s < t.heater.band_low_c, "still cool after 1 s: {after_1s}");
    let mut last = after_1s;
    for _ in 0..100 {
        h.step(DT, 30.0, &t);
        let now = h.temperature().unwrap();
        assert!(now >= last - 1e-9 && now <= 75.0 + 1e-6, "rises towards the setpoint without passing it");
        last = now;
    }
}

#[test]
fn holding_the_band_raises_quality_once_and_turns_effects() {
    let t = tuning();
    let mut h = Heater::new(&t);
    h.load(batch(&t, 0.5)).unwrap();
    heat(&mut h, &t, 75.0, 40.0, 30.0);
    let b = h.unload().unwrap();
    assert_eq!(b.heat, 1.0, "held long enough");
    assert!(!b.burnt);
    assert!((b.quality - (0.5 + t.heater.quality_bonus)).abs() < 1e-9, "one bonus, not one per step: {}", b.quality);
    assert_eq!(b.effects, set(&["glowing", "giggly"]), "heat turns tingly into glowing");
    assert!(h.unload().is_none(), "unloaded");
}

#[test]
fn full_heat_burns_the_batch() {
    let t = tuning();
    let mut h = Heater::new(&t);
    h.load(batch(&t, 0.5)).unwrap();
    heat(&mut h, &t, t.heater.max_setpoint_c, 40.0, 30.0);
    let b = h.unload().unwrap();
    assert!(b.burnt);
    assert!(b.heat < 1.0, "it rushed through the band");
    assert!((b.quality - 0.5 * t.heater.burn_quality_factor).abs() < 1e-9, "burnt once: {}", b.quality);
    assert_eq!(b.effects, set(&["tingly", "giggly"]), "no heat effect");
}

#[test]
fn turning_down_late_overshoots_the_band() {
    // Crank it up, turn it to the band's middle when the batch reaches it: the hot element
    // keeps pushing the batch past the band.
    let t = tuning();
    let mut h = Heater::new(&t);
    h.load(batch(&t, 0.5)).unwrap();
    h.set_setpoint(t.heater.max_setpoint_c, &t);
    let mid = (t.heater.band_low_c + t.heater.band_high_c) / 2.0;
    while h.temperature().unwrap() < mid {
        h.step(DT, 30.0, &t);
    }
    heat(&mut h, &t, mid, 20.0, 30.0);
    println!("overshoot peak {:.1} C", h.peak_c());
    assert!(h.peak_c() > t.heater.band_high_c, "overshoot: peak {}", h.peak_c());
}

#[test]
fn a_skilled_hand_holds_the_band_faster() {
    let t = tuning();
    let time_to_heat = |skill: f64| {
        let mut h = Heater::new(&t);
        h.load(batch(&t, 0.5)).unwrap();
        h.set_setpoint(75.0, &t);
        let mut s = 0.0;
        while h.batch().unwrap().heat < 1.0 {
            h.step(DT, skill, &t);
            s += DT;
            assert!(s < 200.0);
        }
        s
    };
    assert!(time_to_heat(90.0) < time_to_heat(10.0));
}

#[test]
fn a_device_takes_one_batch_at_a_time() {
    let t = tuning();
    let mut h = Heater::new(&t);
    h.load(batch(&t, 0.5)).unwrap();
    assert_eq!(h.load(batch(&t, 0.5)), Err(DeviceRefusal::Busy));
    let mut c = Centrifuge::new();
    c.load(batch(&t, 0.5)).unwrap();
    assert_eq!(c.load(batch(&t, 0.5)), Err(DeviceRefusal::Busy));
}

// ---------- centrifuge ----------

/// Spins a fresh batch at `rpm` for `secs` and returns it.
fn spin(t: &DeviceTuning, rpm: f64, secs: f64, skill: f64) -> (Batch, Spin) {
    let mut c = Centrifuge::new();
    c.load(batch(t, 0.5)).unwrap();
    c.start(rpm, secs, t).unwrap();
    for _ in 0..((secs + 1.0) / DT).round() as usize {
        c.step(DT, skill, t);
    }
    let state = c.state();
    (c.unload().unwrap(), state)
}

#[test]
fn too_slow_does_nothing() {
    let t = tuning();
    let (b, state) = spin(&t, t.centrifuge.sweet_low_rpm * 0.5, 10.0, 30.0);
    assert_eq!(state, Spin::Done);
    assert_eq!((b.quality, b.volume, b.separation), (0.5, 8.0, 0.0));
}

#[test]
fn the_sweet_spot_purifies_without_waste() {
    let t = tuning();
    let mid = (t.centrifuge.sweet_low_rpm + t.centrifuge.sweet_high_rpm) / 2.0;
    let (b, state) = spin(&t, mid, t.centrifuge.separate_s, 30.0);
    assert_eq!(state, Spin::Done);
    assert!((b.separation - 1.0).abs() < 1e-6, "fully separated: {}", b.separation);
    assert!((b.quality - (0.5 + t.centrifuge.quality_bonus)).abs() < 1e-6, "{}", b.quality);
    assert!((b.volume - 8.0).abs() < 1e-6, "no waste: {}", b.volume);
}

#[test]
fn too_long_or_too_fast_wastes_volume() {
    let t = tuning();
    let mid = (t.centrifuge.sweet_low_rpm + t.centrifuge.sweet_high_rpm) / 2.0;
    let (long, _) = spin(&t, mid, t.centrifuge.separate_s * 3.0, 30.0);
    assert!(long.volume < 8.0 - 1e-3, "too long: {}", long.volume);
    assert!((long.quality - (0.5 + t.centrifuge.quality_bonus)).abs() < 1e-6, "the quality is the same");
    let fast_rpm = (t.centrifuge.sweet_high_rpm + t.centrifuge.wobble_rpm) / 2.0;
    let (fast, state) = spin(&t, fast_rpm, t.centrifuge.separate_s, 30.0);
    assert_eq!(state, Spin::Done, "below the wobble speed it runs through");
    assert!(fast.volume < 8.0 - 1e-3, "too fast: {}", fast.volume);
}

#[test]
fn high_speed_wobbles_until_it_trips() {
    let t = tuning();
    let (b, state) = spin(&t, t.centrifuge.max_rpm, 60.0, 30.0);
    assert_eq!(state, Spin::Tripped, "it stops itself");
    assert!(b.volume <= 8.0 * (1.0 - t.centrifuge.trip_loss) + 1e-9, "a trip spills: {}", b.volume);
}

#[test]
fn the_centrifuge_needs_a_batch_and_a_stop_to_unload() {
    let t = tuning();
    let mut c = Centrifuge::new();
    assert_eq!(c.start(3000.0, 5.0, &t), Err(DeviceRefusal::Empty));
    c.load(batch(&t, 0.5)).unwrap();
    c.start(3000.0, 5.0, &t).unwrap();
    assert_eq!(c.state(), Spin::Spinning);
    assert!(c.unload().is_none(), "no unload while spinning");
    c.stop();
    assert!(c.unload().is_some());
}

// ---------- syringe ----------

struct Patient {
    doses: Vec<Dose>,
}

impl Target for Patient {
    fn receive(&mut self, dose: &Dose) {
        self.doses.push(dose.clone());
    }
}

#[test]
fn filling_draws_what_the_batch_and_the_syringe_allow() {
    let t = tuning();
    let mut b = batch(&t, 0.5);
    b.volume = 4.5;
    let mut dice = Dice::new(1);
    let s = fill_syringe(&mut b, 10, 30.0, &t, &mut dice).unwrap();
    assert_eq!(s.doses, t.syringe.capacity, "capped by the syringe");
    assert!((b.volume - (4.5 - t.syringe.capacity as f64)).abs() < 1e-9);
    let s2 = fill_syringe(&mut b, 10, 30.0, &t, &mut dice).unwrap();
    assert_eq!(s2.doses, 1, "only whole doses: 1.5 left gives 1");
    assert_eq!(fill_syringe(&mut b, 1, 30.0, &t, &mut dice), Err(DeviceRefusal::Empty));
    assert_eq!(s.product.effects, b.effects);
    assert_eq!(s.product.base, b.base);
}

#[test]
fn one_batch_rolls_its_bonus_once() {
    let t = tuning();
    let mut b = batch(&t, 0.5);
    b.volume = 40.0;
    let mut dice = Dice::new(11);
    let q: Vec<f64> = (0..10).map(|_| fill_syringe(&mut b, 1, 70.0, &t, &mut dice).unwrap().product.quality).collect();
    assert!(q.iter().all(|x| *x == q[0]), "every syringe of a batch is as good: {q:?}");
}

#[test]
fn injecting_gives_the_effects_and_uses_a_dose() {
    let t = tuning();
    let mut b = batch(&t, 1.0);
    let mut s = fill_syringe(&mut b, 2, 30.0, &t, &mut Dice::new(1)).unwrap();
    let mut p = Patient { doses: Vec::new() };
    let mut dice = Dice::new(5);
    let d = s.inject(&mut p, 100.0, &t, &mut dice).unwrap();
    assert!(d.success);
    assert_eq!(d.effects, b.effects);
    assert!((d.strength - 1.0).abs() < 1e-9, "full quality, full strength");
    assert_eq!(s.doses, 1);
    s.inject(&mut p, 100.0, &t, &mut dice).unwrap();
    assert_eq!(p.doses.len(), 2);
    assert_eq!(s.inject(&mut p, 100.0, &t, &mut dice), Err(DeviceRefusal::Empty));
}

#[test]
fn an_unskilled_hand_sometimes_injects_weaker() {
    let t = tuning();
    let mut b = batch(&t, 1.0);
    b.volume = 1000.0;
    let mut dice = Dice::new(9);
    let mut p = Patient { doses: Vec::new() };
    for _ in 0..100 {
        let mut s = fill_syringe(&mut b, 1, 30.0, &t, &mut dice).unwrap();
        s.inject(&mut p, 0.0, &t, &mut dice).unwrap();
    }
    let fails = p.doses.iter().filter(|d| !d.success).count();
    assert!(fails > 5 && fails < 40, "skill 0 against requirement 20 fails about one in five: {fails}");
    for d in p.doses.iter().filter(|d| !d.success) {
        assert!((d.strength - t.syringe.fail_strength).abs() < 1e-9);
    }
}

// ---------- tuning file ----------

fn with_file(path: &str, text: Option<&str>) -> Vec<File> {
    let mut f: Vec<File> = shipped_files().into_iter().filter(|f| f.path != path).collect();
    if let Some(t) = text {
        f.push(File::new(path, t));
    }
    f
}

#[test]
fn device_tuning_is_checked_and_required() {
    let good = std::fs::read_to_string(content_root().join(DEVICES_PATH)).unwrap();
    let bad = good.replace("\"burn_c\": 95.0", "\"burn_c\": 50.0");
    assert_ne!(good, bad, "the test edits the shipped value");
    let e = DeviceTuning::load(&with_file(DEVICES_PATH, Some(&bad)), &kernel()).unwrap_err();
    assert!(e.iter().any(|m| m.starts_with(DEVICES_PATH) && m.contains("burn_c")), "{e:?}");

    let bad = good.replace("\"to\": \"glowing\"", "\"to\": \"nope\"");
    let e = DeviceTuning::load(&with_file(DEVICES_PATH, Some(&bad)), &kernel()).unwrap_err();
    assert!(e.iter().any(|m| m.contains("heater.transforms[0].to")), "{e:?}");

    let bad = good.replacen('{', "{ \"colour\": 1,", 1);
    assert!(DeviceTuning::load(&with_file(DEVICES_PATH, Some(&bad)), &kernel()).is_err(), "unknown fields are errors");

    let e = DeviceTuning::load(&with_file(DEVICES_PATH, None), &kernel()).unwrap_err();
    assert!(e.iter().any(|m| m.contains(DEVICES_PATH)), "a missing file is an error: {e:?}");
}

#[test]
fn the_shipped_quality_shares_add_up_to_one() {
    let t = tuning();
    let sum = t.mixer_share + t.heater.quality_bonus + t.centrifuge.quality_bonus + 2.0 * t.skill.quality_per_tier;
    assert!((sum - 1.0).abs() < 1e-9, "a perfect chain at full skill reaches 1: {sum}");
}

// ---------- the kettle as the mixer ----------

#[test]
fn the_kettle_hands_a_batch_and_a_skilled_hand_brews_faster() {
    let lc = lab_content();
    let t = tuning();
    let mut lab = Lab::new();
    lab.insert(&lc, "mixer", &cid("dream_goo")).unwrap();
    lab.insert(&lc, "mixer", &cid("space_sugar")).unwrap();
    lab.start_at_speed(&lc, "mixer", 2.0).unwrap();
    lab.step(lc.tuning.brew_s / 2.0 + 1e-6);
    let t_sweet = lc.tuning.sweet_spot * lc.tuning.needle_period_s / 2.0;
    lab.release(&lc, "mixer", t_sweet).expect("done in half the time at double speed");
    let b = lab.take_batch("mixer", &t).unwrap();
    assert_eq!(b.base, cid("dream_goo"));
    assert_eq!(b.ingredients, vec![cid("space_sugar")]);
    assert_eq!(b.effects, set(&["tingly", "giggly"]));
    assert!((b.quality - t.mixer_share).abs() < 1e-9, "a perfect needle gives the mixer's share");
    assert_eq!(b.volume, (2 * t.doses_per_crate) as f64, "crates times doses");
    assert_eq!(b.temp_c, t.ambient_c);
    assert!(lab.take("mixer").is_none(), "taken");
}

// ---------- the whole chain ----------

#[derive(Clone, Copy)]
struct Plan {
    skill: f64,
    /// Needle position (0..1) at the release.
    needle: f64,
    setpoint_c: f64,
    rpm: f64,
}

struct Run {
    quality: f64,
    secs: f64,
    doses_left: f64,
    effects: BTreeSet<EffectId>,
}

/// Runs mixer -> heater -> centrifuge -> syringe with one plan.
fn chain(p: Plan) -> Run {
    let lc = lab_content();
    let t = tuning();
    let speed = skill::speed(p.skill, t.skill.required, &t.skill);
    let mut secs = 0.0;
    let mut lab = Lab::new();
    lab.insert(&lc, "mixer", &cid("dream_goo")).unwrap();
    lab.insert(&lc, "mixer", &cid("space_sugar")).unwrap();
    lab.start_at_speed(&lc, "mixer", speed).unwrap();
    while !matches!(lab.kettle("mixer"), Some(Kettle::Done { .. })) {
        lab.step(DT);
        secs += DT;
    }
    lab.release(&lc, "mixer", p.needle * lc.tuning.needle_period_s / 2.0).unwrap();
    let b = lab.take_batch("mixer", &t).unwrap();

    let mut h = Heater::new(&t);
    h.load(b).unwrap();
    h.set_setpoint(p.setpoint_c, &t);
    for _ in 0..600 {
        h.step(DT, p.skill, &t);
        secs += DT;
        if h.batch().unwrap().heat >= 1.0 {
            break;
        }
    }
    let b = h.unload().unwrap();

    let mut c = Centrifuge::new();
    c.load(b).unwrap();
    c.start(p.rpm, t.centrifuge.separate_s / speed + 0.5, &t).unwrap();
    while c.state() == Spin::Spinning {
        c.step(DT, p.skill, &t);
        secs += DT;
    }
    let mut b = c.unload().unwrap();

    let s = fill_syringe(&mut b, 3, p.skill, &t, &mut Dice::new(42)).unwrap();
    Run { quality: s.product.quality, secs, doses_left: b.volume, effects: s.product.effects }
}

#[test]
fn the_chain_rewards_skill_and_each_step_done_well() {
    let good = Plan { skill: 100.0, needle: 0.5, setpoint_c: 75.0, rpm: 3000.0 };
    let best = chain(good);
    assert!(best.quality > 0.99, "a master doing every step right: {}", best.quality);
    assert_eq!(best.effects, set(&["glowing", "giggly"]), "the heat turned tingly into glowing");

    let novice = chain(Plan { skill: 20.0, ..good });
    println!("best: quality {:.3}, {:.1} s, {:.2} doses left; novice: quality {:.3}, {:.1} s", best.quality, best.secs, best.doses_left, novice.quality, novice.secs);
    assert!(novice.quality < best.quality - 0.05, "skill shows in the quality: {} vs {}", novice.quality, best.quality);
    assert!(novice.secs > best.secs * 1.2, "and in the time: {} s vs {} s", novice.secs, best.secs);

    for (what, plan) in [
        ("needle off the sweet spot", Plan { needle: 0.8, ..good }),
        ("never heated", Plan { setpoint_c: 40.0, ..good }),
        ("burnt", Plan { setpoint_c: 115.0, ..good }),
        ("spun too slow", Plan { rpm: 1000.0, ..good }),
    ] {
        let r = chain(plan);
        println!("{what}: quality {:.3}, {:.1} s", r.quality, r.secs);
        assert!(r.quality < best.quality - 0.05, "{what}: {} vs {}", r.quality, best.quality);
    }
    let fast = chain(Plan { rpm: 3900.0, ..good });
    assert!(fast.doses_left < best.doses_left, "spun too fast wastes doses: {} vs {}", fast.doses_left, best.doses_left);
}
