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
