//! Air patrols of #231: a hot ship in an official airspace may be hailed to halt. Every rule of
//! the spec has a test with a fixed seed; the tuning is the SHIPPED `content/gameplay/heat/`
//! record, and the texts are the SHIPPED `content/gameplay/text/en.json`.
use std::path::Path;

use content_core::File;
use gameplay_core::save::Envelope;
use gameplay_core::text::TextTable;
use gameplay_core::LocationId;
use heat_core::patrol::{CHANCE_CAP, Patrol, PatrolOut, SECTION, SECTION_VERSION};
use heat_core::{HeatContent, Tuning};

fn shipped_tuning() -> Tuning {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/gameplay/heat");
    let mut files = Vec::new();
    for f in std::fs::read_dir(&root).unwrap() {
        let f = f.unwrap().path();
        files.push(File::new(format!("heat/{}", f.file_name().unwrap().to_str().unwrap()), std::fs::read_to_string(&f).unwrap()));
    }
    HeatContent::load(&files).unwrap().tuning.record
}

fn pad() -> LocationId {
    LocationId::new("slosh_inspection_pad")
}

/// One step of the glue: `air` is the airspace's inspection pad, or None in the free port or
/// outside any airspace.
fn tick(p: &mut Patrol, t: &Tuning, stars: u8, air: Option<LocationId>, speed: f64, dt: f64) -> Vec<PatrolOut> {
    p.step(t, stars, air, speed, dt)
}

/// Steps once a second inside the pad at `stars`, fast, until an order comes. Returns the seconds
/// spent (None if no order came within `max_s`) and every output.
fn until_ordered(p: &mut Patrol, t: &Tuning, stars: u8, max_s: u32) -> Option<u32> {
    for s in 1..=max_s {
        if tick(p, t, stars, Some(pad()), 50.0, 1.0).iter().any(|o| matches!(o, PatrolOut::Ordered { .. })) {
            return Some(s);
        }
    }
    None
}

/// Runs `secs` one-second steps with `air` and `speed`, returning every output.
fn run(p: &mut Patrol, t: &Tuning, stars: u8, air: Option<LocationId>, speed: f64, secs: u32) -> Vec<PatrolOut> {
    (0..secs).flat_map(|_| tick(p, t, stars, air.clone(), speed, 1.0)).collect()
}

/// Gets an order (seed 7, five stars, fast inside) and returns the patrol in Ordered.
fn ordered(t: &Tuning) -> Patrol {
    let mut p = Patrol::new(7);
    assert!(until_ordered(&mut p, t, 5, 3600).is_some(), "seed 7 gives an order within an hour");
    assert_eq!(p.ordered_left(), Some(t.patrol_halt_s));
    p
}

// ---- Who gets hailed

#[test]
fn no_orders_below_the_patrol_stars() {
    let t = shipped_tuning();
    assert_eq!(t.patrol_from_stars, 2);
    for stars in 0..t.patrol_from_stars {
        let mut p = Patrol::new(7);
        let outs = run(&mut p, &t, stars, Some(pad()), 50.0, 3600);
        assert!(outs.is_empty(), "stars {stars}: {outs:?}");
    }
}

#[test]
fn no_orders_in_the_free_port_or_outside_an_airspace() {
    let t = shipped_tuning();
    let mut p = Patrol::new(7);
    let outs = run(&mut p, &t, 5, None, 50.0, 3600);
    assert!(outs.is_empty(), "{outs:?}");
    assert_eq!(p.ordered_left(), None);
}

#[test]
fn time_outside_an_airspace_does_not_count_toward_a_roll() {
    let t = shipped_tuning();
    let mut p = Patrol::new(7);
    run(&mut p, &t, 5, None, 50.0, 1000);
    // Only 29 seconds inside: not one full check yet, so no roll at all.
    let outs = run(&mut p, &t, 5, Some(pad()), 50.0, 29);
    assert!(outs.is_empty(), "{outs:?}");
}

#[test]
fn an_order_comes_on_a_full_check_after_a_roll_hits() {
    let t = shipped_tuning();
    let mut p = Patrol::new(7);
    let secs = until_ordered(&mut p, &t, 5, 3600).expect("seed 7 gives an order within an hour");
    assert_eq!(secs % 30, 0, "rolls come every {} s, the order came at {secs} s", t.patrol_check_s);
    assert!(secs >= 30);
    assert_eq!(p.ordered_left(), Some(t.patrol_halt_s));
}

#[test]
fn the_order_names_the_airspace_pad() {
    let t = shipped_tuning();
    let mut p = Patrol::new(7);
    let mut found = None;
    for _ in 0..3600 {
        for o in tick(&mut p, &t, 5, Some(pad()), 50.0, 1.0) {
            if let PatrolOut::Ordered { at } = o {
                found = Some(at);
            }
        }
        if found.is_some() {
            break;
        }
    }
    assert_eq!(found, Some(pad()));
}

// ---- Halting, running, and what comes after

#[test]
fn slowing_below_the_halt_speed_gives_halted_and_the_scan_starts() {
    let t = shipped_tuning();
    let mut p = ordered(&t);
    let outs = tick(&mut p, &t, 5, Some(pad()), t.patrol_halt_speed_ms - 1.0, 1.0);
    assert_eq!(outs, vec![PatrolOut::Halted { at: pad() }]);
    assert_eq!(p.ordered_left(), None, "the order is over");
    assert!(p.scanning(), "the patrol waits for the scan");
    assert!(run(&mut p, &t, 5, Some(pad()), 0.0, 60).is_empty(), "no second halt while scanning");
}

#[test]
fn exactly_the_halt_speed_counts_as_halted() {
    let t = shipped_tuning();
    let mut p = ordered(&t);
    let outs = tick(&mut p, &t, 5, Some(pad()), t.patrol_halt_speed_ms, 1.0);
    assert_eq!(outs, vec![PatrolOut::Halted { at: pad() }]);
}

#[test]
fn too_slow_for_the_halt_time_gives_fled() {
    let t = shipped_tuning();
    let mut p = ordered(&t);
    let halt = t.patrol_halt_s as u32;
    let outs = run(&mut p, &t, 5, Some(pad()), 50.0, halt);
    assert_eq!(outs, vec![PatrolOut::Fled], "fled exactly when the time runs out");
    assert_eq!(p.ordered_left(), None);
}

#[test]
fn the_order_counts_down_for_the_hud() {
    let t = shipped_tuning();
    let mut p = ordered(&t);
    tick(&mut p, &t, 5, Some(pad()), 50.0, 5.0);
    assert_eq!(p.ordered_left(), Some(t.patrol_halt_s - 5.0));
}

#[test]
fn leaving_the_airspace_gives_fled_at_once_even_when_slow() {
    let t = shipped_tuning();
    let mut p = ordered(&t);
    let outs = tick(&mut p, &t, 5, None, 0.0, 1.0);
    assert_eq!(outs, vec![PatrolOut::Fled], "leaving is not a halt");
    assert_eq!(p.ordered_left(), None);
}

// ---- Cooldowns

#[test]
fn no_new_order_during_the_cooldown_after_fled() {
    let t = shipped_tuning();
    let mut p = ordered(&t);
    run(&mut p, &t, 5, Some(pad()), 50.0, t.patrol_halt_s as u32);
    assert_eq!(p.cooldown_left(), t.patrol_cooldown_s);
    let outs = run(&mut p, &t, 5, Some(pad()), 50.0, t.patrol_cooldown_s as u32 - 1);
    assert!(outs.is_empty(), "{outs:?}");
    assert_eq!(p.cooldown_left(), 1.0);
}

#[test]
fn the_cooldown_after_fled_ends_and_orders_come_again() {
    let t = shipped_tuning();
    let mut p = ordered(&t);
    run(&mut p, &t, 5, Some(pad()), 50.0, t.patrol_halt_s as u32);
    run(&mut p, &t, 5, Some(pad()), 50.0, t.patrol_cooldown_s as u32);
    assert_eq!(p.cooldown_left(), 0.0);
    assert!(until_ordered(&mut p, &t, 5, 3600).is_some(), "orders come again after the cooldown");
}

#[test]
fn scan_over_starts_the_cooldown_and_returns_to_idle() {
    let t = shipped_tuning();
    let mut p = ordered(&t);
    tick(&mut p, &t, 5, Some(pad()), 0.0, 1.0);
    assert!(p.scanning());
    p.scan_over(&t);
    assert!(!p.scanning());
    assert_eq!(p.cooldown_left(), t.patrol_cooldown_s);
    assert!(run(&mut p, &t, 5, Some(pad()), 50.0, t.patrol_cooldown_s as u32 - 1).is_empty());
}

#[test]
fn scan_over_while_not_scanning_does_nothing() {
    // A pad scan's ScanResolved reaches the glue too: it must not start a patrol cooldown.
    let t = shipped_tuning();
    let mut p = Patrol::new(7);
    p.scan_over(&t);
    assert!(!p.scanning());
    assert_eq!(p.cooldown_left(), 0.0);
    let mut q = ordered(&t);
    q.scan_over(&t);
    assert_eq!(q.cooldown_left(), 0.0, "an order that is not halted yet is not a scan");
}

// ---- Chance

#[test]
fn chance_is_stars_times_the_rate() {
    let t = shipped_tuning();
    assert!((Patrol::chance(&t, 2) - 0.24).abs() < 1e-12);
    assert!((Patrol::chance(&t, 5) - 0.6).abs() < 1e-12);
}

#[test]
fn chance_is_capped() {
    let mut t = shipped_tuning();
    t.patrol_chance_per_star = 0.5;
    assert_eq!(Patrol::chance(&t, 5), CHANCE_CAP);
    assert_eq!(CHANCE_CAP, 0.9);
    assert!((Patrol::chance(&t, 1) - 0.5).abs() < 1e-12, "below the cap nothing changes");
}

// ---- Save and content

#[test]
fn save_round_trip_keeps_the_order_and_the_rolls() {
    let t = shipped_tuning();
    let p = ordered(&t);
    let mut env = Envelope::new();
    p.save(&mut env);
    let mut back = Patrol::load(&env).unwrap().expect("patrol section");
    assert_eq!(back, p);
    assert_eq!(back.ordered_left(), p.ordered_left());
    let mut orig = p.clone();
    assert_eq!(tick(&mut orig, &t, 5, Some(pad()), 0.0, 1.0), tick(&mut back, &t, 5, Some(pad()), 0.0, 1.0));
}

#[test]
fn a_save_without_a_patrol_section_loads_as_none() {
    assert!(Patrol::load(&Envelope::new()).unwrap().is_none());
}

#[test]
fn the_save_section_is_named_and_versioned() {
    assert_eq!(SECTION, "patrol");
    assert_eq!(SECTION_VERSION, 1);
}

#[test]
fn shipped_texts_for_the_patrol_exist() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/gameplay/text/en.json");
    let table = TextTable::from_json("text/en.json", &std::fs::read_to_string(path).unwrap()).unwrap();
    for key in ["patrol.ordered", "patrol.halted", "patrol.fled"] {
        let lines = table.lines(key).unwrap_or_else(|| panic!("no text '{key}'"));
        assert!(lines.len() >= 3, "{key} needs 3 lines");
        assert!(lines.iter().all(|l| !l.trim().is_empty()), "{key} has no empty line");
    }
    assert!(table.has("patrol.name"), "the patrol's call sign");
}
