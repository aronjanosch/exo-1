//! Rules of #81 with numbers, plus the crate table (#80) and the crate body.
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

/// One holder pulls a crate of `mass` from rest towards a hold point 1 m up and 1 m ahead, in
/// gravity. Free point mass, no floor. Returns the error after `secs`.
fn pull(cfg: &GrabConfig, mass: f64, holders: usize, secs: f64) -> f64 {
    let target = DVec3::new(0.0, 1.0, -1.0);
    let mut pos = DVec3::ZERO;
    let mut vel = DVec3::ZERO;
    let g = DVec3::new(0.0, -G, 0.0);
    let hs: Vec<Holder> = (0..holders).map(|_| Holder { target, target_vel: DVec3::ZERO, source: DVec3::new(0.0, 1.0, 0.0), reach: Reach::Hands }).collect();
    let dt = 1.0 / 60.0;
    for _ in 0..(secs / dt) as usize {
        let out = hold_force(cfg, mass, pos, vel, g, &hs);
        vel += (out.force / mass + g) * dt;
        pos += vel * dt;
    }
    pos.distance(target)
}

// ---------- crate table (#80) ----------

#[test]
fn table_has_three_doubling_sizes() {
    let t = table();
    assert_eq!(t.sizes.len(), 3);
    let names: Vec<&str> = t.sizes.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["small", "medium", "large"]);
    for w in t.sizes.windows(2) {
        assert!((w[1].extents[0] / w[0].extents[0] - 2.0).abs() < 1e-9);
    }
    assert_eq!((t.sizes[0].hands, t.sizes[1].hands, t.sizes[2].hands), (1, 2, 2));
    assert_eq!((t.sizes[0].holders, t.sizes[1].holders, t.sizes[2].holders), (1, 1, 2));
    assert_eq!(t.get("medium").unwrap().name, "medium");
}

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

// ---------- hold (#81) ----------

#[test]
fn grab_tuning_parses() {
    let c = cfg();
    assert!(c.hand_range > 0.0 && c.tool_full_range > c.hand_range && c.tool_max_range > c.tool_full_range);
}

#[test]
fn lag_grows_with_mass() {
    let c = cfg();
    let t = table();
    // Error after 0.4 s for each size held alone (the large one with two holders).
    let errs: Vec<f64> = t.sizes.iter().map(|s| pull(&c, s.mass, s.holders as usize, 0.4)).collect();
    println!("error after 0.4 s per size: {errs:?}");
    assert!(errs[0] < errs[1] && errs[1] < errs[2], "{errs:?}");
    // The small crate is close to the hand by then; all reach the hold point in the end.
    assert!(errs[0] < 0.15, "{errs:?}");
    for s in &t.sizes {
        assert!(pull(&c, s.mass, s.holders as usize, 4.0) < 0.05, "{} settles", s.name);
    }
}

#[test]
fn speed_cap_falls_with_mass() {
    let c = cfg();
    assert_eq!(speed_cap(&c, c.ref_mass * 0.5), c.max_speed);
    assert!((speed_cap(&c, c.ref_mass * 4.0) - c.max_speed / 4.0).abs() < 1e-9);
    assert!(speed_cap(&c, 1e6) >= c.min_speed);
}

#[test]
fn falloff_curve() {
    let c = cfg();
    // Hands: full force to hand_range, nothing beyond.
    assert_eq!(falloff(&c, Reach::Hands, c.hand_range), 1.0);
    assert_eq!(falloff(&c, Reach::Hands, c.hand_range + 0.01), 0.0);
    // Tool: full to full_range, linear to zero at max_range.
    assert_eq!(falloff(&c, Reach::Tool, 0.5), 1.0);
    assert_eq!(falloff(&c, Reach::Tool, c.tool_full_range), 1.0);
    let mid = 0.5 * (c.tool_full_range + c.tool_max_range);
    assert!((falloff(&c, Reach::Tool, mid) - 0.5).abs() < 1e-9);
    assert_eq!(falloff(&c, Reach::Tool, c.tool_max_range), 0.0);
    assert_eq!(falloff(&c, Reach::Tool, c.tool_max_range + 5.0), 0.0);
}

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
fn one_holder_cannot_lift_large_two_can() {
    let c = cfg();
    let large = table().get("large").unwrap().clone();
    let one = pull(&c, large.mass, 1, 3.0);
    let two = pull(&c, large.mass, 2, 3.0);
    println!("large crate error after 3 s: one holder {one:.2} m, two {two:.3} m");
    // One holder: it falls (no floor here), far below the hold point.
    assert!(one > 5.0, "{one}");
    assert!(two < 0.05, "{two}");
    // And the cap rule itself: one holder's force is below the weight, two are above.
    assert!(c.hand_force < large.mass * G && 2.0 * c.hand_force > large.mass * G);
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
    // Crate 2 m ahead pulled in to 1 m: the holder is pulled forward.
    let out = hold_force(&c, 60.0, DVec3::new(0.0, 0.0, -2.0), DVec3::ZERO, DVec3::ZERO, &[h]);
    assert!(out.force.z > 0.0);
    let a = holder_accel(&c, out.reactions[0]);
    assert!(a.z < 0.0 && (a.length() - out.force.length() / c.holder_mass).abs() < 1e-9);
}

#[test]
fn break_timer() {
    let c = cfg();
    let dt = 1.0 / 60.0;
    let mut b = BreakTimer::default();
    // A snag shorter than the break time does not drop the crate.
    for _ in 0..(0.5 / dt) as usize {
        assert!(!b.step(&c, c.break_distance + 0.1, false, dt));
    }
    assert!(!b.step(&c, 0.1, false, dt));
    assert_eq!(b.t, 0.0, "back in range resets");
    // Sustained error drops it after break_time.
    let mut ticks = 0;
    while !b.step(&c, c.break_distance + 0.1, false, dt) {
        ticks += 1;
        assert!(ticks < 200);
    }
    let t = (ticks + 1) as f64 * dt;
    println!("break after {t:.3} s");
    assert!((t - c.break_time).abs() <= dt + 1e-9, "{t}");
    // Standing on the crate drops it at once.
    assert!(BreakTimer::default().step(&c, 0.0, true, dt));
}

#[test]
fn throw_speed_per_size() {
    let c = cfg();
    let t = table();
    let dir = DVec3::NEG_Z;
    let speeds: Vec<f64> = t.sizes.iter().map(|s| throw_velocity(&c, s.mass, DVec3::ZERO, dir).length()).collect();
    println!("throw speed per size: {speeds:?}");
    assert!((speeds[0] - c.throw_max_speed.min(c.throw_impulse / t.sizes[0].mass)).abs() < 1e-9);
    assert!(speeds[0] > speeds[1] && speeds[1] > speeds[2]);
    // The hand's velocity carries over.
    let v = throw_velocity(&c, 15.0, DVec3::new(1.0, 0.0, 0.0), dir);
    assert!((v.x - 1.0).abs() < 1e-9);
    // The holder gets the opposite impulse.
    let kick = throw_kick(&c, t.sizes[0].mass, dir);
    assert!(kick.z > 0.0 && (kick.length() * c.holder_mass - speeds[0] * t.sizes[0].mass).abs() < 1e-9);
}

#[test]
fn contact_damps_turn() {
    let c = cfg();
    let free = turn_rate(&c, 15.0, 10.0, false);
    let touching = turn_rate(&c, 15.0, 10.0, true);
    assert!((touching - free * c.contact_turn_share).abs() < 1e-9);
    assert!(turn_rate(&c, 15.0, -10.0, false) < 0.0, "keeps the sign");
    assert!(turn_rate(&c, 240.0, 10.0, false) < free, "heavy turns slower");
}

#[test]
fn view_turn_slows_with_mass() {
    let c = cfg();
    let t = table();
    let s: Vec<f64> = t.sizes.iter().map(|z| view_turn_share(&c, z.mass)).collect();
    assert!(s[0] > s[1] && s[1] > s[2] && s[2] > 0.0 && s[0] <= 1.0, "{s:?}");
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

#[test]
fn friction_holds_until_push_beats_it() {
    let c = cfg();
    let room = Room { walls: None };
    let mut b = small_body();
    b.pos.y = b.half.y + 0.005;
    let dt = 1.0 / 60.0;
    for _ in 0..30 {
        b.step(&c, &walker_core::Frame::IDENTITY, DVec3::Y, G, DVec3::ZERO, 0.0, &room, dt);
    }
    let x0 = b.pos.x;
    // Below the friction limit: stays.
    let weak = DVec3::new(c.friction * G * 0.8, 0.0, 0.0);
    for _ in 0..120 {
        b.step(&c, &walker_core::Frame::IDENTITY, DVec3::Y, G, weak, 0.0, &room, dt);
    }
    assert!((b.pos.x - x0).abs() < 1e-6, "{}", b.pos.x);
    // Above it: slides.
    let strong = DVec3::new(c.friction * G * 2.0, 0.0, 0.0);
    for _ in 0..60 {
        b.step(&c, &walker_core::Frame::IDENTITY, DVec3::Y, G, strong, 0.0, &room, dt);
    }
    assert!(b.pos.x > 0.5, "{}", b.pos.x);
}

#[test]
fn crate_stays_in_room() {
    let c = cfg();
    let room = Room { walls: Some((1.95, 4.0)) };
    let mut b = small_body();
    let dt = 1.0 / 60.0;
    // Hard sideways push for 3 s, then the other way, turning.
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
    // Out of the cabin: world velocity = local velocity turned + the ship's velocity.
    let local_vel_world = ship.rot * b.vel;
    b.change_frame(&ship, &walker_core::Frame::IDENTITY, ship_vel);
    assert!((b.pos - world).length() < 1e-9);
    assert!((b.vel - (local_vel_world + ship_vel)).length() < 1e-9);
    // And back.
    b.change_frame(&walker_core::Frame::IDENTITY, &ship, -ship_vel);
    assert!((ship.to_world(b.pos) - world).length() < 1e-9);
    assert!((ship.rot * b.vel - local_vel_world).length() < 1e-9);
}

// ---------- object budget (#85) ----------

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
    // The longest untouched one is held: it stays, the next two go.
    objs[4].protected = true;
    assert_eq!(budget::over_budget(&row, &objs), vec![3, 2]);
    // All protected: nothing goes, even over the cap.
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
        // Resting far away: only the timeout counts for it.
        budget::Obj { distance: 5000.0, ..obj(4, 5.0) },
        budget::Obj { protected: true, ..obj(5, 1000.0) },
    ];
    assert_eq!(budget::over_budget(&row, &objs), vec![1, 3]);
}

#[test]
fn budget_keeps_persistence_cap_on_a_planet_left_behind() {
    let row = budget::BudgetRow { cap: 10, persistence_cap: 2, timeout_s: 100.0, far_m: 1000.0 };
    let mut objs: Vec<budget::Obj> = (0..5).map(|i| budget::Obj { here: false, ..obj(i, 10.0 * i as f64) }).collect();
    // Left behind, the timeout does not apply: they are frozen.
    objs[4].idle = 1e6;
    let mut gone = budget::over_budget(&row, &objs);
    gone.sort();
    assert_eq!(gone, vec![2, 3, 4], "the two most recently touched stay");
    // Over the cap, those left behind go before the ones here (the longer untouched first).
    let row = budget::BudgetRow { cap: 3, persistence_cap: 2, timeout_s: 100.0, far_m: 1000.0 };
    let objs = [obj(1, 50.0), obj(2, 60.0), budget::Obj { here: false, ..obj(3, 1.0) }, budget::Obj { here: false, ..obj(4, 2.0) }];
    assert_eq!(budget::over_budget(&row, &objs), vec![4]);
}

/// A single plane through the origin with normal `n`: points with p·n >= 0 are free.
struct Slope {
    n: DVec3,
}

impl BoxWorld for Slope {
    fn sweep(&self, center: DVec3, half: DVec3, rot: DQuat, motion: DVec3) -> Option<walker_core::Hit> {
        let len = motion.length();
        let closing = -(motion / len).dot(self.n);
        if closing <= 1e-12 {
            return None;
        }
        let gap = center.dot(self.n) - support(half, rot, self.n);
        let d = (gap / closing).max(0.0);
        (d <= len).then_some(walker_core::Hit { distance: d, normal: self.n, velocity: DVec3::ZERO })
    }
    fn depenetrate(&self, center: DVec3, half: DVec3, rot: DQuat) -> DVec3 {
        let gap = center.dot(self.n) - support(half, rot, self.n);
        if gap < 0.0 { self.n * -gap } else { DVec3::ZERO }
    }
}

/// A crate on a slope: below the friction angle it holds and sleeps, above it slides.
fn on_slope(deg: f64) -> (CrateBody, f64) {
    let c = cfg();
    let n = DQuat::from_rotation_z(deg.to_radians()) * DVec3::Y;
    let world = Slope { n };
    let mut b = small_body();
    b.pos = n * (b.half.y + 0.3);
    b.forward = DVec3::NEG_Z;
    let dt = 1.0 / 60.0;
    let mut start = b.pos;
    for i in 0..240 {
        // Measured from 1 s on, after the drop onto the slope.
        if i == 60 {
            start = b.pos;
        }
        b.step(&c, &walker_core::Frame::IDENTITY, DVec3::Y, G, DVec3::ZERO, 0.0, &world, dt);
    }
    let moved = (b.pos - start).length();
    (b, moved)
}

#[test]
fn crate_holds_on_a_gentle_slope_and_slides_on_a_steep_one() {
    let c = cfg();
    let angle = c.friction.atan().to_degrees();
    let (b, moved) = on_slope(angle - 6.0);
    println!("{:.1} deg slope: moved {moved:.3} m, asleep {}", angle - 6.0, b.asleep);
    assert!(b.asleep && moved < 1e-3, "holds and sleeps: moved {moved}");
    let (b, moved) = on_slope(angle + 8.0);
    println!("{:.1} deg slope: moved {moved:.3} m, asleep {}", angle + 8.0, b.asleep);
    assert!(!b.asleep && moved > 1.0, "slides: moved {moved}");
}
