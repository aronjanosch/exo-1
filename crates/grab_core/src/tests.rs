use super::*;
use glam::{DQuat, DVec3};

const G: f64 = 9.81;
const TABLE: &str = include_str!("../../../content/cargo/crates.json");
const GRAB: &str = include_str!("../../../content/tuning/grab.json");

fn table() -> CrateTable {
    CrateTable::from_json(TABLE).unwrap()
}
fn cfg() -> GrabConfig {
    GrabConfig::from_json(GRAB).unwrap()
}


// ---------- crate table ----------

#[test]
fn table_rejects_missing_field() {
    let bad = r#"{"sizes": [{"name": "small", "extents": [0.5, 0.5, 0.5], "mass": 15.0, "hands": 1}]}"#;
    let e = CrateTable::from_json(bad).unwrap_err();
    assert!(e.contains("holders"), "{e}");
}

#[test]
fn table_rejects_bad_rows() {
    let row = |name: &str, ext: f64, mass: f64, hands: u8, holders: u8| {
        format!(r#"{{"name": "{name}", "extents": [{ext}, {ext}, {ext}], "mass": {mass}, "hands": {hands}, "holders": {holders}}}"#)
    };
    let doc = |rows: &[String]| format!(r#"{{"sizes": [{}]}}"#, rows.join(","));
    assert!(CrateTable::from_json(&doc(&[])).is_err(), "empty");
    assert!(CrateTable::from_json(&doc(&[row("a", 0.5, -1.0, 1, 1)])).unwrap_err().contains("mass"));
    assert!(CrateTable::from_json(&doc(&[row("a", 0.0, 1.0, 1, 1)])).unwrap_err().contains("extents"));
    assert!(CrateTable::from_json(&doc(&[row("a", 0.5, 1.0, 3, 1)])).unwrap_err().contains("hands"));
    assert!(CrateTable::from_json(&doc(&[row("a", 0.5, 1.0, 1, 0)])).unwrap_err().contains("holders"));
    assert!(CrateTable::from_json(&doc(&[row("a", 0.5, 1.0, 1, 1), row("a", 1.0, 2.0, 1, 1)])).unwrap_err().contains("twice"));
    assert!(CrateTable::from_json(&doc(&[row("a", 0.5, 1.0, 1, 1), row("b", 0.7, 2.0, 1, 1)])).unwrap_err().contains("double"));
    let unknown = r#"{"sizes": [{"name": "a", "extents": [1,1,1], "mass": 1, "hands": 1, "holders": 1, "colour": 3}]}"#;
    assert!(CrateTable::from_json(unknown).is_err());
}

// ---------- hold ----------

#[test]
fn force_cap_scales_with_falloff() {
    let c = cfg();
    let far = Holder { target: DVec3::ZERO, target_vel: DVec3::ZERO, source: DVec3::new(0.0, 0.0, -8.0), reach: Reach::Tool };
    // A crate at 8 m from the tool, far from the hold point: the force is capped at the falloff share.
    let out = hold_force(&c, 1000.0, DVec3::new(0.0, 0.0, -8.0) + DVec3::new(0.0, 0.0, 0.0), DVec3::ZERO, DVec3::ZERO, &[Holder { source: DVec3::ZERO, ..far }]);
    let share = falloff(&c, Reach::Tool, 8.0);
    assert!((out.force.length() - c.hand_force * share).abs() < 1e-6, "{} vs {}", out.force.length(), c.hand_force * share);
}

#[test]
fn shared_carry_splits_reaction() {
    let c = cfg();
    let h = |x: f64| Holder { target: DVec3::new(x, 1.0, 0.0), target_vel: DVec3::ZERO, source: DVec3::new(x, 1.0, 1.0), reach: Reach::Hands };
    let out = hold_force(&c, 240.0, DVec3::ZERO, DVec3::ZERO, DVec3::new(0.0, -G, 0.0), &[h(-0.5), h(0.5)]);
    assert_eq!(out.reactions.len(), 2);
    let sum = out.reactions[0] + out.reactions[1];
    assert!((sum + out.force).length() < 1e-9, "reactions balance the force");
    assert!((out.reactions[0] - out.reactions[1]).length() < 1e-9, "equal holders share equally");
}

#[test]
fn reaction_pushes_holder_in_zero_g() {
    let c = cfg();
    let h = Holder { target: DVec3::new(0.0, 0.0, -1.0), target_vel: DVec3::ZERO, source: DVec3::ZERO, reach: Reach::Hands };
    let out = hold_force(&c, 60.0, DVec3::new(0.0, 0.0, -2.0), DVec3::ZERO, DVec3::ZERO, &[h]);
    assert!(out.force.z > 0.0);
    let a = holder_accel(&c, out.reactions[0]);
    assert!(a.z < 0.0 && (a.length() - out.force.length() / c.holder_mass).abs() < 1e-9);
}

#[test]
fn carry_states() {
    let c = cfg();
    let one = carry(&c, 1);
    assert_eq!((one.speed_share, one.can_run, one.can_jump), (1.0, true, true));
    let two = carry(&c, 2);
    assert!(two.speed_share < 1.0 && !two.can_run && !two.can_jump);
}

#[test]
fn cone_picks_targets_ahead() {
    let eye = DVec3::ZERO;
    let look = DVec3::NEG_Z;
    let half = 20f64.to_radians();
    assert!(in_cone(eye, look, DVec3::new(0.0, 0.0, -3.0), half, 5.0).is_some());
    assert!(in_cone(eye, look, DVec3::new(3.0, 0.0, -3.0), half, 5.0).is_none(), "45 deg off");
    assert!(in_cone(eye, look, DVec3::new(0.0, 0.0, -6.0), half, 5.0).is_none(), "too far");
    assert!(in_cone(eye, look, DVec3::new(0.0, 0.0, 3.0), half, 5.0).is_none(), "behind");
}

// ---------- crate body ----------

/// Floor at y = 0 and, optionally, walls of a room |x| < wx, |z| < wz (inner faces).
struct Room {
    walls: Option<(f64, f64)>,
}

fn support(half: DVec3, rot: DQuat, n: DVec3) -> f64 {
    let (x, y, z) = (rot * DVec3::X, rot * DVec3::Y, rot * DVec3::Z);
    half.x * x.dot(n).abs() + half.y * y.dot(n).abs() + half.z * z.dot(n).abs()
}

impl Room {
    /// Planes as (normal into the room, offset): points with p·n >= offset are free.
    fn planes(&self) -> Vec<(DVec3, f64)> {
        let mut p = vec![(DVec3::Y, 0.0)];
        if let Some((wx, wz)) = self.walls {
            p.extend([(DVec3::X, -wx), (DVec3::NEG_X, -wx), (DVec3::Z, -wz), (DVec3::NEG_Z, -wz)]);
        }
        p
    }
}

impl BoxWorld for Room {
    fn sweep(&self, center: DVec3, half: DVec3, rot: DQuat, motion: DVec3) -> Option<walker_core::Hit> {
        let len = motion.length();
        let dir = motion / len;
        let mut best: Option<walker_core::Hit> = None;
        for (n, off) in self.planes() {
            let gap = center.dot(n) - support(half, rot, n) - off;
            let closing = -dir.dot(n);
            if closing <= 1e-12 {
                continue;
            }
            let d = (gap / closing).max(0.0);
            if d <= len && best.is_none_or(|b| d < b.distance) {
                best = Some(walker_core::Hit { distance: d, normal: n, velocity: DVec3::ZERO });
            }
        }
        best
    }
    fn depenetrate(&self, center: DVec3, half: DVec3, rot: DQuat) -> DVec3 {
        let mut push = DVec3::ZERO;
        for (n, off) in self.planes() {
            let gap = (center + push).dot(n) - support(half, rot, n) - off;
            if gap < 0.0 {
                push += n * -gap;
            }
        }
        push
    }
}

fn small_body() -> CrateBody {
    let s = table().get("small").unwrap().clone();
    CrateBody::new(&s, DVec3::new(0.0, 2.0, 0.0), DVec3::NEG_Z)
}

#[test]
fn crate_falls_and_rests_on_floor() {
    let c = cfg();
    let room = Room { walls: None };
    let mut b = small_body();
    let dt = 1.0 / 60.0;
    for _ in 0..180 {
        b.step(&c, &walker_core::Frame::IDENTITY, DVec3::Y, G, DVec3::ZERO, 0.0, &room, dt);
    }
    let bottom = b.pos.y - b.half.y;
    assert!(b.grounded && bottom >= 0.0 && bottom < 0.02, "bottom {bottom}");
    assert!(b.asleep, "at rest it sleeps");
}

/// One step of a crate 1 cm above the floor moving at `vel`, weightless so only the impact acts.
fn after_impact(vel: DVec3) -> DVec3 {
    let mut b = small_body();
    b.pos.y = b.half.y + 0.01;
    b.vel = vel;
    b.step(&cfg(), &walker_core::Frame::IDENTITY, DVec3::Y, 0.0, DVec3::ZERO, 0.0, &Room { walls: None }, 1.0 / 60.0);
    b.vel
}

#[test]
fn impact_friction_cuts_the_slide() {
    let mu = cfg().friction;
    // Fast along the floor, a light touch: it keeps sliding, slower by friction x impact speed.
    let v = after_impact(DVec3::new(6.0, -2.0, 0.0));
    assert!(v.y.abs() < 1e-9, "no bounce: {v}");
    assert!((v.x - (6.0 - mu * 2.0)).abs() < 1e-6, "tangential {v}");
    // Steep and hard: friction eats the whole slide (never reverses it).
    let v = after_impact(DVec3::new(2.0, -4.0, 1.0));
    assert!(v.length() < 1e-9, "stops dead: {v}");
}

#[test]
fn crate_stays_in_room() {
    let c = cfg();
    let room = Room { walls: Some((1.95, 4.0)) };
    let mut b = small_body();
    let dt = 1.0 / 60.0;
    for i in 0..360 {
        let a = if i < 180 { DVec3::new(30.0, 0.0, 20.0) } else { DVec3::new(-30.0, 0.0, -20.0) };
        b.step(&c, &walker_core::Frame::IDENTITY, DVec3::Y, G, a, 2.0, &room, dt);
        let r = b.rot();
        for (n, off) in room.planes() {
            assert!(b.pos.dot(n) - support(b.half, r, n) - off > -1e-6, "outside at tick {i}");
        }
    }
}

#[test]
fn change_frame_keeps_world_pose_and_hands_over_velocity() {
    let mut b = small_body();
    let ship = walker_core::Frame { origin: DVec3::new(100.0, 0.0, 0.0), rot: DQuat::from_rotation_y(0.7) };
    b.vel = DVec3::new(0.0, 0.0, 1.0);
    let world = ship.to_world(b.pos);
    let ship_vel = DVec3::new(50.0, 0.0, 0.0);
    let local_vel_world = ship.rot * b.vel;
    b.change_frame(&ship, &walker_core::Frame::IDENTITY, ship_vel);
    assert!((b.pos - world).length() < 1e-9);
    assert!((b.vel - (local_vel_world + ship_vel)).length() < 1e-9);
    b.change_frame(&walker_core::Frame::IDENTITY, &ship, -ship_vel);
    assert!((ship.to_world(b.pos) - world).length() < 1e-9);
    assert!((ship.rot * b.vel - local_vel_world).length() < 1e-9);
}

// ---------- object budget ----------

const BUDGET: &str = include_str!("../../../content/cargo/budget.json");

fn obj(id: u64, idle: f64) -> budget::Obj {
    budget::Obj { id, protected: false, idle, distance: 10.0, resting: true, here: true }
}

#[test]
fn budget_parses_and_rejects() {
    let b = budget::Budget::from_json(BUDGET).unwrap();
    assert!(b.crates.cap >= b.crates.persistence_cap);
    assert!(budget::Budget::from_json(r#"{"crate": {"cap": 4, "persistence_cap": 5, "timeout_s": 1, "far_m": 1}}"#).is_err());
    assert!(budget::Budget::from_json(r#"{"crate": {"cap": 4, "persistence_cap": 1, "timeout_s": 1}}"#).unwrap_err().contains("far_m"));
}

#[test]
fn budget_cap_drops_longest_untouched_loose() {
    let row = budget::BudgetRow { cap: 3, persistence_cap: 1, timeout_s: 100.0, far_m: 1000.0 };
    let mut objs: Vec<budget::Obj> = (0..5).map(|i| obj(i, i as f64)).collect();
    objs[4].protected = true;
    assert_eq!(budget::over_budget(&row, &objs), vec![3, 2]);
    let all: Vec<budget::Obj> = (0..5).map(|i| budget::Obj { protected: true, ..obj(i, 50.0) }).collect();
    assert!(budget::over_budget(&row, &all).is_empty());
}

#[test]
fn budget_timeout_and_far() {
    let row = budget::BudgetRow { cap: 10, persistence_cap: 1, timeout_s: 100.0, far_m: 1000.0 };
    let objs = [
        obj(1, 101.0),
        obj(2, 99.0),
        budget::Obj { resting: false, distance: 1001.0, ..obj(3, 0.0) },
        budget::Obj { distance: 5000.0, ..obj(4, 5.0) },
        budget::Obj { protected: true, ..obj(5, 1000.0) },
    ];
    assert_eq!(budget::over_budget(&row, &objs), vec![1, 3]);
}

#[test]
fn budget_keeps_persistence_cap_on_a_planet_left_behind() {
    let row = budget::BudgetRow { cap: 10, persistence_cap: 2, timeout_s: 100.0, far_m: 1000.0 };
    let mut objs: Vec<budget::Obj> = (0..5).map(|i| budget::Obj { here: false, ..obj(i, 10.0 * i as f64) }).collect();
    objs[4].idle = 1e6;
    let mut gone = budget::over_budget(&row, &objs);
    gone.sort();
    assert_eq!(gone, vec![2, 3, 4], "the two most recently touched stay");
    let row = budget::BudgetRow { cap: 3, persistence_cap: 2, timeout_s: 100.0, far_m: 1000.0 };
    let objs = [obj(1, 50.0), obj(2, 60.0), budget::Obj { here: false, ..obj(3, 1.0) }, budget::Obj { here: false, ..obj(4, 2.0) }];
    assert_eq!(budget::over_budget(&row, &objs), vec![4]);
}

// ---------- condition ----------

/// Largest impact speed while a small crate falls from `height` (bottom above the floor) and rests.
fn drop_impact(height: f64) -> f64 {
    let c = cfg();
    let room = Room { walls: None };
    let mut b = small_body();
    b.pos.y = b.half.y + height;
    let mut most: f64 = 0.0;
    for _ in 0..240 {
        b.step(&c, &walker_core::Frame::IDENTITY, DVec3::Y, G, DVec3::ZERO, 0.0, &room, 1.0 / 60.0);
        most = most.max(b.impact);
    }
    assert!(b.asleep, "rests after the drop");
    most
}

#[test]
fn impact_speed_matches_the_fall() {
    for h in [0.5, 2.0, 8.0] {
        let want = (2.0 * G * h).sqrt();
        let got = drop_impact(h);
        assert!((got - want).abs() < 0.25, "drop {h} m: impact {got:.2} m/s, free fall {want:.2}");
    }
    assert!(drop_impact(0.0) < 0.5, "settling on the floor is a tiny impact at most");
}
