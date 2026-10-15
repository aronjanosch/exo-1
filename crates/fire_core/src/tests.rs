use super::*;
use glam::DVec2;

const DT: f64 = 1.0 / 30.0;

/// Two rooms of 30 m³ side by side, a 2 m² door between them at x = 3.
fn two_rooms(open: f64) -> (Deck, RoomId, RoomId, GapId) {
    let mut d = Deck::new(FireTuning::default(), 11);
    let a = d.add_room(30.0);
    let b = d.add_room(30.0);
    let door = d.add_gap(a, b, DVec3::new(3.0, 0.0, 0.0), 2.0, open);
    (d, a, b, door)
}

fn run(d: &mut Deck, secs: f64, targets: &[Target]) -> Vec<(f64, FireEvent)> {
    let mut out = Vec::new();
    let mut t = 0.0;
    for _ in 0..(secs / DT).round() as usize {
        t += DT;
        out.extend(d.step(DT, targets).into_iter().map(|e| (t, e)));
    }
    out
}

#[test]
fn closed_room_starves_its_fire() {
    let (mut d, a, b, _) = two_rooms(0.0);
    let f = d.ignite(a, DVec3::ZERO, 0.1);
    let ev = run(&mut d, 120.0, &[]);
    let (t_out, _) = *ev.iter().find(|(_, e)| *e == FireEvent::Out { fire: f, room: a }).expect("starved");
    println!("closed 30 m³ room: fire out after {t_out:.1} s, oxygen left {:.1} %", d.room(a).oxygen_fraction() * 100.0);
    assert_eq!(d.room(b).oxygen, 30.0, "the closed door kept the other room's air");
}

#[test]
fn closed_door_stops_the_spread() {
    let mut spread_at = Vec::new();
    for open in [0.0, 1.0] {
        let (mut d, a, b, _) = two_rooms(open);
        // Plenty of air so the fire keeps going; the fire sits 1 m from the door.
        d.room_mut(a).volume = 400.0;
        d.room_mut(a).oxygen = 400.0;
        d.ignite(a, DVec3::new(2.0, 0.0, 0.0), 0.5);
        let ev = run(&mut d, 120.0, &[]);
        let t = ev.iter().find(|(_, e)| matches!(e, FireEvent::Spread { room, .. } if *room == b)).map(|(t, _)| *t);
        spread_at.push(t);
    }
    println!("spread to the next room: door closed {:?}, open {:?}", spread_at[0], spread_at[1]);
    assert!(spread_at[0].is_none());
}

#[test]
fn oxygen_is_conserved_across_a_gap() {
    let mut d = Deck::new(FireTuning::default(), 1);
    let a = d.add_room(30.0);
    let b = d.add_room(12.0);
    let c = d.add_room(50.0);
    d.room_mut(b).oxygen = 1.0;
    d.room_mut(c).oxygen = 20.0;
    d.add_gap(a, b, DVec3::ZERO, 2.0, 1.0);
    let bc = d.add_gap(b, c, DVec3::X, 0.5, 0.4);
    let total = d.total_oxygen();
    for _ in 0..(60.0 / DT) as usize {
        d.exchange_air(DT);
        assert!((d.total_oxygen() - total).abs() < 1e-9);
        for r in [a, b, c] {
            assert!(d.room(r).oxygen >= 0.0 && d.room(r).oxygen <= d.room(r).volume + 1e-9);
        }
    }
    let f: Vec<f64> = [a, b, c].iter().map(|&r| d.room(r).oxygen_fraction()).collect();
    println!("after 60 s: fractions {f:.4?}, total {:.6} of {total:.6}", d.total_oxygen());
    // From 1.0 / 0.08 / 0.4 at the start: the spread has at least halved.
    d.set_open(bc, 1.0);
    d.room_mut(c).oxygen = 0.0;
    d.exchange_air(1000.0);
    assert!(d.room(b).oxygen_fraction() >= d.room(c).oxygen_fraction() - 1e-9);
    let mut d2 = Deck::new(FireTuning::default(), 1);
    let x = d2.add_room(10.0);
    let y = d2.add_room(10.0);
    d2.room_mut(y).oxygen = 0.0;
    d2.add_gap(x, y, DVec3::ZERO, 2.0, 0.0);
    d2.exchange_air(10.0);
    assert_eq!((d2.room(x).oxygen, d2.room(y).oxygen), (10.0, 0.0));
}

#[test]
fn touching_fires_merge() {
    let (mut d, a, _, _) = two_rooms(0.0);
    let f1 = d.ignite(a, DVec3::ZERO, 1.0);
    let f2 = d.ignite(a, DVec3::new(0.6, 0.0, 0.0), 0.4);
    assert_eq!(f1, f2, "a fire inside another one joins it");
    let fire = d.fire(f1).unwrap();
    assert!((fire.size - 1.3).abs() < 1e-9, "span -0.5..0.8, {}", fire.size);
    assert!((fire.pos.x - 0.15).abs() < 1e-9);
    let f3 = d.ignite(a, DVec3::new(4.0, 0.0, 0.0), 0.2);
    assert_ne!(f1, f3);
    assert_eq!(d.fires().len(), 2);
}

#[test]
fn a_leak_drains_its_room_exponentially() {
    // 80 m³ room, a 1 m² hole half open: tau = V / (k * A * open) = 80 / (4 * 0.5) = 40 s.
    let mut d = Deck::new(FireTuning::default(), 1);
    let r = d.add_room(80.0);
    let leak = d.add_leak(r, DVec3::ZERO, 1.0, 0.5);
    let tau = 80.0 / (d.tuning().leak_flow * 0.5);
    for _ in 0..(40.0 / DT) as usize {
        d.exchange_air(DT);
    }
    let f = d.room(r).oxygen_fraction();
    println!("leak: tau {tau:.1} s, after 40 s {:.3} (exp {:.3}), lost {:.2} m³, flow now {:.3} m³/s", f, (-40.0 / tau).exp(), d.lost_to_space(), d.leak(leak).flow);
    assert!((f - (-40.0f64 / tau).exp()).abs() < 0.01);
    assert!((d.lost_to_space() + d.room(r).oxygen - 80.0).abs() < 1e-9, "what left went to space");
    d.set_leak(leak, 1.0, 0.0);
    let before = d.room(r).oxygen;
    d.exchange_air(10.0);
    assert_eq!(d.room(r).oxygen, before);
    assert_eq!(d.leak(leak).flow, 0.0);
}

#[test]
fn a_closed_door_keeps_the_leak_to_one_room() {
    let (mut d, a, b, door) = two_rooms(1.0);
    d.add_leak(b, DVec3::new(6.0, 0.0, 0.0), 1.0, 0.75);
    for _ in 0..(10.0 / DT) as usize {
        d.exchange_air(DT);
    }
    let open_a = d.room(a).oxygen_fraction();
    let (mut d2, a2, b2, _) = two_rooms(0.0);
    d2.add_leak(b2, DVec3::new(6.0, 0.0, 0.0), 1.0, 0.75);
    for _ in 0..(10.0 / DT) as usize {
        d2.exchange_air(DT);
    }
    println!("leak next door for 10 s: this room {:.3} with the door open, {:.3} closed", open_a, d2.room(a2).oxygen_fraction());
    assert!(d2.room(a2).oxygen_fraction() == 1.0);
    let _ = door;
}

#[test]
fn a_spray_lands_on_a_fire_in_the_cone_and_not_outside() {
    let eye = DVec3::ZERO;
    let dir = DVec3::new(0.0, 0.0, -1.0);
    let half = 20.0f64.to_radians();
    let ahead = DVec3::new(0.0, 0.0, -3.0);
    assert!((cone_landing(eye, dir, ahead, 0.5, 4.0, half).unwrap() - 3.0).abs() < 1e-9);
    // Beside the aim at 3 m, 2 m off: out of a 20 degree cone (tan 20 * 3 = 1.09 m, plus the radius).
    assert!(cone_landing(eye, dir, DVec3::new(2.0, 0.0, -3.0), 0.5, 4.0, half).is_none());
    // A fire of 1 m radius at 2 m off with 3 m along is in (1.09 + 1.0 = 2.09 m).
    assert!(cone_landing(eye, dir, DVec3::new(2.0, 0.0, -3.0), 1.0, 4.0, half).is_some());
    assert!(cone_landing(eye, dir, DVec3::new(0.0, 0.0, 2.0), 0.5, 4.0, half).is_none());
    assert!(cone_landing(eye, dir, DVec3::new(0.0, 0.0, -9.0), 0.5, 4.0, half).is_none());
    // Beyond the reach but its edge in reach: the spray lands at the reach.
    assert!((cone_landing(eye, dir, DVec3::new(0.0, 0.0, -4.8), 1.0, 4.0, half).unwrap() - 4.0).abs() < 1e-9);
}

#[test]
fn a_target_is_measured_to_the_nearest_point_of_its_footprint() {
    // A 2 m by 2 m box on the floor, centred at the origin: a point is at 0 inside it.
    let box_ = Target { id: 1, kind: TargetKind::Device, room: RoomId(0), pos: DVec3::ZERO, half: DVec2::splat(1.0) };
    assert_eq!(box_.distance_to(DVec3::new(0.5, -0.2, 0.0)), 0.0);
    assert!((box_.distance_to(DVec3::new(1.3, 0.0, 0.0)) - 0.3).abs() < 1e-12, "0.3 m out of the east face");
    assert!((box_.distance_to(DVec3::new(2.0, 2.0, 0.0)) - 2f64.sqrt()).abs() < 1e-12, "to the corner");
    // A person is a point: the distance to its centre.
    let person = Target { half: DVec2::ZERO, ..box_ };
    assert!((person.distance_to(DVec3::new(3.0, 4.0, 0.0)) - 5.0).abs() < 1e-12);
}

#[test]
fn a_fire_at_a_device_face_burns_it_at_once_and_one_further_out_by_its_range() {
    // Size 1 m: the reach past the fire's edge is sqrt(1) x 1.0 = 1.0 m, so 0.5 + 1.0 = 1.5 m
    // from the fire's centre to the footprint.
    let device = |id| Target { id, kind: TargetKind::Device, room: RoomId(0), pos: DVec3::ZERO, half: DVec2::splat(1.0) };
    let burn = |gap: f64| {
        let (mut d, a, _, _) = two_rooms(0.0);
        // The fire sits `gap` m out from the device's east face, 0.3 m for a machine's front face.
        d.ignite(a, DVec3::new(1.0 + gap, 0.0, 0.0), 1.0);
        let ev = d.step(DT, &[Target { room: a, ..device(3) }]);
        ev.iter().filter_map(|e| match e { FireEvent::Damage { target: 3, amount, .. } => Some(*amount), _ => None }).sum::<f64>()
    };
    assert!(burn(0.3) > 0.0, "at the face, the same step");
    assert!(burn(1.4) > 0.0, "inside the range");
    assert_eq!(burn(1.6), 0.0, "beyond the range");
}

// The oxygen generator (`oxygen.rs`) and `Deck::add_oxygen`. The engine room is 10 x 8 x 3.2 m.
const ROOM_M3: f64 = 10.0 * 8.0 * 3.2;

#[test]
fn full_power_makes_the_rate_and_more_power_than_full_makes_no_more() {
    assert_eq!(generator_flow(1.0, 2.0, 1.0), 2.0);
    assert_eq!(generator_flow(1.5, 2.0, 1.0), 2.0, "the voltage is capped at full");
}

#[test]
fn half_power_makes_half_and_below_the_minimum_makes_nothing() {
    assert!((generator_flow(0.5, 2.0, 1.0) - 1.0).abs() < 1e-12);
    assert_eq!(generator_flow(0.49, 2.0, 1.0), 0.0);
    assert_eq!(generator_flow(0.0, 2.0, 1.0), 0.0);
    assert_eq!(generator_flow(f64::NAN, 2.0, 1.0), 0.0, "a NaN voltage makes nothing");
}

#[test]
fn the_condition_counts_squared_like_barotrauma() {
    assert!((generator_flow(1.0, 2.0, 0.5) - 0.5).abs() < 1e-12, "half condition gives a quarter of the air: 2 × 0.5² = 0.5");
    assert_eq!(generator_flow(1.0, 2.0, 0.0), 0.0);
}

#[test]
fn adding_air_tops_the_room_up_and_clamps_at_full() {
    let mut d = Deck::new(FireTuning::default(), 1);
    let r = d.add_room(ROOM_M3);
    d.room_mut(r).oxygen = ROOM_M3 * 0.5;
    d.add_oxygen(r, 10.0);
    assert!((d.room(r).oxygen - (ROOM_M3 * 0.5 + 10.0)).abs() < 1e-9);
    d.add_oxygen(r, 1.0e6);
    assert_eq!(d.room(r).oxygen, ROOM_M3, "never past full");
    d.add_oxygen(r, -5.0);
    assert_eq!(d.room(r).oxygen, ROOM_M3, "a negative amount takes nothing");
}

#[test]
fn full_power_refills_the_engine_room_from_half_in_about_a_minute() {
    // 2.13 m³/s is the shipped `room.oxygen_generator_rate` (reactor.json).
    let mut d = Deck::new(FireTuning::default(), 1);
    let r = d.add_room(ROOM_M3);
    d.room_mut(r).oxygen = ROOM_M3 * 0.5;
    let rate = generator_flow(1.0, 2.13, 1.0);
    for _ in 0..60 {
        d.add_oxygen(r, rate);
    }
    assert!(d.room(r).oxygen_fraction() > 0.99, "after 60 s: {}", d.room(r).oxygen_fraction());
}

/// Barotrauma `Hull.OxygenConsumptionSpeed` (700 px²/s per character) over our engine room's
/// 1000 × 320 px hull: 0.56 m³/s per breather (`room.breath_m3_per_s`, reactor.json).
const BREATH: f64 = 0.56;

/// The sealed room of `ROOM_M3`, full, no gap and no leak.
fn sealed_room() -> (Deck, RoomId) {
    let mut d = Deck::new(FireTuning::default(), 3);
    let r = d.add_room(ROOM_M3);
    (d, r)
}

/// `breathers` breathers at `rate` m³/s each, and a generator at `generator` m³/s, for `secs`.
/// The glue does the same per step: the generator's air in, then the breathing out.
fn breathe_for(d: &mut Deck, r: RoomId, breathers: usize, rate: f64, generator: f64, secs: f64) {
    for _ in 0..(secs / DT).round() as usize {
        d.add_oxygen(r, generator * DT);
        d.breathe(r, breathers as f64 * rate * DT);
    }
}

#[test]
fn four_breathers_in_a_sealed_room_lose_about_two_and_a_quarter_m3_a_second() {
    let (mut d, r) = sealed_room();
    let before = d.room(r).oxygen;
    breathe_for(&mut d, r, 4, BREATH, 0.0, 10.0);
    let lost = before - d.room(r).oxygen;
    assert!((lost - 2.24 * 10.0).abs() < 1e-6, "lost {lost} m³ in 10 s");
}

#[test]
fn a_full_generator_holds_three_breathers_in_a_sealed_room() {
    // 3 × 0.56 = 1.68 m³/s is under the generator's 2.13: the air does not fall.
    let (mut d, r) = sealed_room();
    d.room_mut(r).oxygen = ROOM_M3 * 0.5;
    let start = d.room(r).oxygen;
    breathe_for(&mut d, r, 3, BREATH, 2.13, 60.0);
    assert!(d.room(r).oxygen >= start, "after 60 s: {}", d.room(r).oxygen);
}

#[test]
fn breathing_never_takes_more_than_the_room_has_and_a_negative_amount_takes_nothing() {
    let (mut d, r) = sealed_room();
    d.room_mut(r).oxygen = 0.1;
    d.breathe(r, 1.0);
    assert_eq!(d.room(r).oxygen, 0.0, "clamped at empty");
    d.add_oxygen(r, 5.0);
    d.breathe(r, -5.0);
    assert_eq!(d.room(r).oxygen, 5.0, "a negative amount takes nothing");
}

#[test]
fn the_flow_splits_by_volume_among_the_open_vents() {
    // Barotrauma OxygenGenerator.UpdateVents: each vent gets the flow times its hull's volume share.
    let s = split_by_volume(4.0, &[(30.0, true), (10.0, true)]);
    assert_eq!(s.len(), 2);
    assert!((s[0] - 3.0).abs() < 1e-12 && (s[1] - 1.0).abs() < 1e-12, "{s:?}");
}

#[test]
fn a_closed_vent_sends_all_the_flow_to_the_other() {
    assert_eq!(split_by_volume(4.0, &[(30.0, false), (10.0, true)]), vec![0.0, 4.0]);
    assert_eq!(split_by_volume(4.0, &[(30.0, true), (10.0, false)]), vec![4.0, 0.0]);
}

#[test]
fn all_vents_closed_means_no_flow_goes_anywhere() {
    assert_eq!(split_by_volume(4.0, &[(30.0, false), (10.0, false)]), vec![0.0, 0.0]);
    // No vents at all: the generator does nothing (Barotrauma: `!ventList.Any()` returns).
    assert!(split_by_volume(4.0, &[]).is_empty());
}

#[test]
fn the_split_sums_to_the_flow_whenever_a_vent_is_open() {
    for flow in [0.0, 0.7, 4.0, 123.5] {
        for vents in [
            vec![(30.0, true), (10.0, false), (5.0, true)],
            vec![(1.0, true)],
            vec![(12.0, true), (12.0, true), (0.0, true)],
        ] {
            let s = split_by_volume(flow, &vents);
            let sum: f64 = s.iter().sum();
            assert!((sum - flow).abs() < 1e-9, "flow {flow}, {vents:?} gave {s:?}");
            assert!(s.iter().all(|x| *x >= 0.0));
        }
    }
}
