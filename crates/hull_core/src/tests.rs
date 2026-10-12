//! Rules of #286 for hits, breaches, patching and the oxygen generator, with numbers.
use super::*;
use fire_core::FireTuning;

const DT: f64 = 1.0 / 30.0;

/// An engineering room (300 m³) and a side room (80 m³) behind a door, a hull section in each.
fn ship(door_open: f64) -> (Deck, Hull, RoomId, RoomId, SectionId, SectionId) {
    let mut d = Deck::new(FireTuning::default(), 3);
    let a = d.add_room(300.0);
    let b = d.add_room(80.0);
    d.add_gap(a, b, DVec3::new(5.0, 1.0, 0.0), 2.0, door_open);
    let mut h = Hull::new(HullTuning::default(), 9);
    let sa = h.add_section(&mut d, a, DVec3::new(0.0, 1.0, 3.0));
    let sb = h.add_section(&mut d, b, DVec3::new(9.0, 1.0, 3.0));
    (d, h, a, b, sa, sb)
}

fn run(d: &mut Deck, secs: f64) {
    for _ in 0..(secs / DT).round() as usize {
        d.exchange_air(DT);
    }
}

#[test]
fn openness_follows_the_two_thresholds() {
    let t = HullTuning::default();
    assert_eq!(t.openness(0.05), 0.0);
    assert!((t.openness(0.4) - 0.35 * 0.5).abs() < 1e-9);
    assert!((t.openness(0.7) - 0.35).abs() < 1e-9);
    assert!((t.openness(0.85) - 0.55).abs() < 1e-9);
    assert!((t.openness(1.0) - 0.75).abs() < 1e-9);
}

#[test]
fn damage_types_scale_as_the_table_says() {
    let mut out = Vec::new();
    for kind in DamageType::ALL {
        let (mut d, mut h, _, _, sa, _) = ship(0.0);
        let fx = h.apply(&Hit { fire_chance: 0.0, ..Hit::standard(kind, Strength::Medium, sa) }, &mut d);
        let hull = fx.iter().find_map(|e| if let HitEffect::HullDamaged { amount, .. } = e { Some(*amount) } else { None }).unwrap();
        let sys = fx.iter().find_map(|e| if let HitEffect::SystemDamage { amount, .. } = e { Some(*amount) } else { None }).unwrap();
        out.push((kind, hull, sys));
        assert!((hull - 35.0 * kind.scale().hull).abs() < 1e-9 && (sys - 12.0 * kind.scale().system).abs() < 1e-9);
        assert!((h.section(sa).condition.value() - (100.0 - hull)).abs() < 1e-9);
    }
    println!("medium hit, hull / system damage: {out:.1?}");
    assert!(out[0].2 > out[1].2 && out[1].1 < out[2].1, "energy hurts systems most, collision the hull");
}

#[test]
fn a_light_hit_does_not_breach_a_heavy_one_does() {
    let (mut d, mut h, a, _, sa, _) = ship(0.0);
    let light = Hit { fire_chance: 0.0, ..Hit::standard(DamageType::Energy, Strength::Light, sa) };
    let fx = h.apply(&light, &mut d);
    assert!(!fx.iter().any(|e| matches!(e, HitEffect::Breach { .. })) && !h.breached(sa), "9 damage stays under the leak threshold");
    let heavy = Hit { fire_chance: 0.0, ..Hit::standard(DamageType::Physical, Strength::Heavy, sa) };
    let fx = h.apply(&heavy, &mut d);
    let open = h.openness(sa);
    println!("light then heavy: damage {:.2}, opening {open:.3}, hole {} m²", h.section(sa).damage(), h.section(sa).hole);
    assert!(fx.contains(&HitEffect::Breach { section: sa, open, opened: true }));
    assert!(open > 0.35 && h.section(sa).hole == 2.0);
    run(&mut d, 1.0);
    assert!(d.room(a).oxygen < 300.0);
}

#[test]
fn a_breach_drains_its_room_in_the_expected_time() {
    let (mut d, mut h, _, b, _, sb) = ship(0.0);
    h.apply(&Hit { fire_chance: 0.0, ..Hit::standard(DamageType::Physical, Strength::Heavy, sb) }, &mut d);
    // tau = V / (k * area * open): 80 / (4 * 2 * 0.35) = 28.6 s; to 20 % air: tau * ln 5.
    let open = h.openness(sb);
    let tau = 80.0 / (d.tuning().leak_flow * 2.0 * open);
    let mut t = 0.0;
    while d.room(b).oxygen_fraction() > 0.2 {
        d.exchange_air(DT);
        t += DT;
    }
    println!("side room breached (opening {open:.2}): 20 % air after {t:.1} s (expected {:.1} s)", tau * 5f64.ln());
    assert!((t - tau * 5f64.ln()).abs() < 0.5);
}

#[test]
fn a_patch_closes_the_breach_by_skill() {
    let mut times = Vec::new();
    for skill in [0.0, 50.0, 100.0] {
        let (mut d, mut h, a, _, sa, _) = ship(0.0);
        h.apply(&Hit { fire_chance: 0.0, ..Hit::standard(DamageType::Physical, Strength::Heavy, sa) }, &mut d);
        let mut t = 0.0;
        while h.breached(sa) && t < 60.0 {
            h.patch_step(sa, DT, skill, Tool::WRENCH, &mut d);
            d.exchange_air(DT);
            t += DT;
        }
        times.push(t);
        let before = d.room(a).oxygen;
        run(&mut d, 5.0);
        assert_eq!(d.room(a).oxygen, before, "patched: no more loss");
        assert_eq!(d.leak(h.section(sa).leak).open, 0.0);
    }
    // From 30 to 90 condition: 60 % of 30 / 20 / 10 s.
    println!("heavy breach patched shut after {times:.1?} s at skill 0 / 50 / 100");
    assert!((times[0] - 18.0).abs() < 0.2 && (times[1] - 12.0).abs() < 0.2 && (times[2] - 6.0).abs() < 0.2);
}

#[test]
fn without_a_breach_air_is_conserved() {
    let (mut d, mut h, a, b, sa, _) = ship(1.0);
    d.room_mut(b).oxygen = 10.0;
    h.apply(&Hit { fire_chance: 0.0, ..Hit::standard(DamageType::Energy, Strength::Light, sa) }, &mut d);
    let total = d.total_oxygen();
    run(&mut d, 60.0);
    println!("no breach, 60 s: total {:.6} of {total:.6}, rooms {:.3} / {:.3}", d.total_oxygen(), d.room(a).oxygen_fraction(), d.room(b).oxygen_fraction());
    assert!((d.total_oxygen() - total).abs() < 1e-9 && d.lost_to_space() == 0.0);
}

#[test]
fn fire_comes_from_the_seeded_roll() {
    let mut fires = 0;
    for seed in 0..200 {
        let mut d = Deck::new(FireTuning::default(), 1);
        let r = d.add_room(100.0);
        let mut h = Hull::new(HullTuning::default(), seed);
        let s = h.add_section(&mut d, r, DVec3::ZERO);
        // Medium energy: 0.35 x 1.5 = 0.525.
        let fx = h.apply(&Hit::standard(DamageType::Energy, Strength::Medium, s), &mut d);
        let lit = fx.iter().any(|e| matches!(e, HitEffect::Fire { .. }));
        assert_eq!(lit, !d.fires().is_empty());
        fires += lit as u32;
    }
    println!("medium energy hits: {fires} of 200 started a fire (chance 0.525)");
    assert!((85..125).contains(&fires));
    // The same seed gives the same outcome.
    let roll = |seed| {
        let mut d = Deck::new(FireTuning::default(), 1);
        let r = d.add_room(100.0);
        let mut h = Hull::new(HullTuning::default(), seed);
        let s = h.add_section(&mut d, r, DVec3::ZERO);
        h.apply(&Hit::standard(DamageType::Energy, Strength::Medium, s), &mut d)
    };
    assert_eq!(roll(17), roll(17));
}

#[test]
fn a_big_breach_pulls_towards_the_hole() {
    let (mut d, mut h, a, _, sa, _) = ship(0.0);
    let at = DVec3::new(0.0, 1.0, 0.0);
    h.apply(&Hit { fire_chance: 0.0, ..Hit::standard(DamageType::Physical, Strength::Light, sa) }, &mut d);
    h.apply(&Hit { fire_chance: 0.0, ..Hit::standard(DamageType::Physical, Strength::Light, sa) }, &mut d);
    d.exchange_air(DT);
    let small = h.pull(&d, a, at);
    h.apply(&Hit { fire_chance: 0.0, ..Hit::standard(DamageType::Physical, Strength::Heavy, sa) }, &mut d);
    d.exchange_air(DT);
    let big = h.pull(&d, a, at);
    let flow = d.leak(h.section(sa).leak).flow;
    println!("pull 3 m from the hole: small breach {:.2} m/s², big {:.2} m/s² ({flow:.2} m³/s out)", small.length(), big.length());
    assert_eq!(small, DVec3::ZERO);
    assert!(big.length() > 0.5 && big.length() <= 3.0 && big.normalize().dot(DVec3::Z) > 0.99, "{big:?}");
    assert_eq!(h.pull(&d, a, DVec3::new(0.0, 1.0, -10.0)), DVec3::ZERO, "out of reach");
}

#[test]
fn the_generator_refills_by_supply_and_condition() {
    let (mut d, _, a, b, _, _) = ship(0.0);
    d.room_mut(a).oxygen = 150.0;
    d.room_mut(b).oxygen = 40.0;
    let mut g = OxygenGenerator::new(2.0, DeviceTuning::default(), 1);
    let full = g.step(1.0, 1.0, &mut d, &[a, b]);
    g.condition.set_value(50.0);
    let half = g.step(1.0, 1.0, &mut d, &[a, b]);
    let low_power = g.step(1.0, 0.5, &mut d, &[a, b]);
    g.condition.set_value(10.0);
    let broken = g.step(1.0, 1.0, &mut d, &[a, b]);
    println!("generator m³ per s: full {full:.2}, condition 50 {half:.2}, plus half supply {low_power:.2}, broken {broken:.2}");
    assert!((full - 2.0).abs() < 1e-9 && (half - 0.5).abs() < 1e-9 && (low_power - 0.25).abs() < 1e-9 && broken == 0.0);
    // Shared by volume, never past full.
    g.condition.set_value(100.0);
    for _ in 0..1000 {
        g.step(1.0, 1.0, &mut d, &[a, b]);
    }
    assert_eq!((d.room(a).oxygen, d.room(b).oxygen), (300.0, 80.0));
}

#[test]
fn a_volley_is_seeded() {
    let secs: Vec<SectionId> = vec![SectionId(0), SectionId(1)];
    let collect = |seed| {
        let mut v = Volley::new(seed, secs.clone(), 6);
        let mut hits = Vec::new();
        let mut t = 0.0;
        while v.left() > 0 && t < 20.0 {
            if let Some(h) = v.step(DT) {
                hits.push((t, h));
            }
            t += DT;
        }
        hits
    };
    let a = collect(5);
    println!("volley seed 5: {:?}", a.iter().map(|(t, h)| format!("{t:.1}s {} {:.0} on {}", h.kind.name(), h.amount, h.section.0)).collect::<Vec<_>>());
    assert_eq!(a.len(), 6);
    assert_eq!(a, collect(5));
    assert!(a.windows(2).all(|w| w[1].0 - w[0].0 >= 0.6 - 1e-9));
}
