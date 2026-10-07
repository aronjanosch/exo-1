//! Scripted runs: a list of steps, each called once per fixed tick until it returns true. Steps
//! drive the game only through `Controls` and a few test hooks (teleport for test setup, ship
//! test input). A run exits non-zero when a check fails.
use crate::controls::Controls;
use crate::env::PlanetRes;
use crate::origin::RenderOrigin;
use crate::ring::Ring;
use crate::ship::{Ship, SEAT_POS};
use crate::view::ViewState;
use crate::walker::{ship_frame, Player, WalkStats};
use avian3d::prelude::*;
use bevy::math::DVec3;
use bevy::prelude::*;
use flight_core::{FlightInput, PlanetEnv};
use std::collections::HashMap;
use walker_core::Frame;
use net_core::buffer::Buffer;
use net_core::replay::DT;
use net_core::snapshot::Snapshot;
use crate::ship::RemoteShip;

/// `foreign` scenario: a remote ship (kinematic proxy) is driven through the real
/// snapshot path (encode, decode, buffer, 150 ms playout) along a known path, so the walker in its
/// cabin can be checked against exact truth.
#[derive(Resource)]
pub struct ForeignDriver {
    pub proxy: Entity,
    pub buf: Buffer,
    pub tick: u64,
    /// Planet-relative start of the flight; the ship moves along -z at 350 m/s and yaws 0.003 rad/tick.
    pub p0: DVec3,
    /// When set the ship stands still at this pose (walker beside a parked foreign ship).
    pub parked: Option<(DVec3, bevy::math::DQuat)>,
    pub max_pos_err: f64,
    pub max_rot_err_deg: f64,
}

const FOREIGN_SPEED: f64 = 350.0;
const FOREIGN_YAW_RATE: f64 = 0.003 * 60.0;

fn foreign_truth(d: &ForeignDriver, t: f64) -> (DVec3, bevy::math::DQuat, DVec3) {
    if let Some((p, q)) = d.parked {
        return (p, q, DVec3::ZERO);
    }
    (d.p0 + DVec3::new(0.0, 0.0, -FOREIGN_SPEED * t), bevy::math::DQuat::from_rotation_y(FOREIGN_YAW_RATE * t), DVec3::new(0.0, 0.0, -FOREIGN_SPEED))
}

pub fn foreign_drive(
    mut d: ResMut<ForeignDriver>,
    mut origin: ResMut<RenderOrigin>,
    mut q: Query<(&mut Position, &mut Rotation, &mut LinearVelocity, &mut AngularVelocity), With<RemoteShip>>,
) {
    d.tick += 1;
    let t = d.tick as f64 * DT;
    if d.tick % 2 == 0 {
        let (p, qn, v) = foreign_truth(&d, t);
        let mut s = Snapshot::new(2, t, p, v, qn);
        s.seq = d.tick as u32;
        // Through the wire format, like a received packet.
        let s = Snapshot::decode(&s.encode()).expect("own snapshot decodes");
        d.buf.push(s);
    }
    let target = t - 0.15;
    let Some(sample) = d.buf.sample(target) else { return };
    let Ok((mut p, mut r, mut v, mut w)) = q.get_mut(d.proxy) else { return };
    p.0 = sample.s.p;
    r.0 = sample.s.q;
    v.0 = sample.s.v;
    w.0 = if d.parked.is_some() { DVec3::ZERO } else { DVec3::new(0.0, FOREIGN_YAW_RATE, 0.0) };
    origin.view = sample.s.p;
    if sample.mode == net_core::buffer::Mode::Interpolate && target > 0.5 {
        let (tp, tq, _) = foreign_truth(&d, target);
        d.max_pos_err = d.max_pos_err.max(sample.s.p.distance(tp));
        d.max_rot_err_deg = d.max_rot_err_deg.max(sample.s.q.angle_between(tq).to_degrees());
    }
}

pub type Step = Box<dyn FnMut(&mut World, &mut Ctx) -> bool + Send + Sync>;

#[derive(Default)]
pub struct Ctx {
    /// Simulated seconds since the step started.
    pub t: f64,
    pub dt: f64,
    pub ticks: u64,
    pub phase: String,
    pub report: Vec<String>,
    pub v: HashMap<&'static str, f64>,
    pub p: HashMap<&'static str, DVec3>,
    rescues0: u32,
    stats0: WalkStats,
    pub failures: u32,
    shot_n: u32,
}

#[derive(Resource)]
pub struct Script {
    pub name: String,
    pub steps: Vec<Step>,
    pub i: usize,
    pub ctx: Ctx,
    pub out_dir: std::path::PathBuf,
    pub done: bool,
}

// ---------- helpers ----------

pub fn keys(w: &mut World, ks: &[KeyCode], on: bool) {
    let mut c = w.resource_mut::<Controls>();
    for &k in ks {
        if on {
            c.held.insert(k);
        } else {
            c.held.remove(&k);
        }
    }
}
fn tap(w: &mut World, k: KeyCode) {
    w.resource_mut::<Controls>().taps.push(k);
}
fn planet(w: &World) -> PlanetRes {
    w.resource::<PlanetRes>().clone()
}
fn ship_e(w: &mut World) -> Entity {
    w.query_filtered::<Entity, With<Ship>>().single(w).unwrap()
}
fn ship_frame_of(w: &mut World) -> Frame {
    let e = ship_e(w);
    let (p, r) = (w.get::<Position>(e).unwrap(), w.get::<Rotation>(e).unwrap());
    ship_frame(p, r)
}
fn ship_vel(w: &mut World) -> DVec3 {
    let e = ship_e(w);
    w.get::<LinearVelocity>(e).unwrap().0
}
fn with_ship<R>(w: &mut World, f: impl FnOnce(&mut Ship) -> R) -> R {
    let e = ship_e(w);
    f(&mut w.get_mut::<Ship>(e).unwrap())
}
fn with_player<R>(w: &mut World, f: impl FnOnce(&mut Player) -> R) -> R {
    let mut q = w.query::<&mut Player>();
    let mut p = q.single_mut(w).unwrap();
    f(&mut p)
}
fn player_world(w: &mut World) -> DVec3 {
    let f = ship_frame_of(w);
    with_player(w, |p| p.world_pos(f))
}
/// Position of whatever the player controls (ship when seated).
fn active_pos(w: &mut World) -> DVec3 {
    if with_player(w, |p| p.seated) { ship_frame_of(w).origin } else { player_world(w) }
}
fn above_ground(w: &mut World) -> f64 {
    let p = active_pos(w);
    planet(w).above_ground(p)
}
fn altitude(w: &mut World) -> f64 {
    let p = active_pos(w);
    let pl = planet(w);
    (p - pl.centre).length() - pl.radius
}
/// Put the walker on the ground at a world point (test setup).
fn place_walker(w: &mut World, at: DVec3) {
    let pl = planet(w);
    let dir = pl.up(at);
    let pos = pl.centre + dir * (pl.surface(dir) + 0.1);
    with_player(w, |p| {
        p.ship = None;
        p.w.pos = pos;
        p.w.halt();
        p.w.grounded = false;
    });
}
/// Turn the walker towards a world point (heading only, pitch level).
fn face_towards(w: &mut World, target: DVec3) {
    let f = ship_frame_of(w);
    let pl = planet(w);
    with_player(w, |p| {
        let pos = p.world_pos(f);
        let up = p.world_up(f, &pl);
        let mut d = target - pos;
        d -= up * d.dot(up);
        let d = d.normalize();
        p.w.forward = if p.ship.is_some() { f.rot.inverse() * d } else { d };
        p.pitch = 0.0;
    });
}

/// Where the walker looks, world space (camera direction without interpolation).
fn world_look(w: &mut World) -> DVec3 {
    let f = ship_frame_of(w);
    let pl = planet(w);
    with_player(w, |p| {
        let fwd = if p.ship.is_some() { f.rot * p.w.forward } else { p.w.forward };
        walker_core::look_dir(fwd, p.world_up(f, &pl), p.pitch)
    })
}

/// Issue #7: largest change of the look direction from one tick to the next (degrees), in
/// `c.v["look_jump"]`. Call every tick of a step; the first tick only starts it (setup turns).
fn track_look(w: &mut World, c: &mut Ctx) {
    let l = world_look(w);
    if c.t > 0.0 {
        let jump = l.angle_between(c.p["look"]).to_degrees();
        c.v.insert("look_jump", c.v.get("look_jump").copied().unwrap_or(0.0).max(jump));
    } else {
        c.v.insert("look_jump", 0.0);
    }
    c.p.insert("look", l);
}

fn check(c: &mut Ctx, ok: bool, note: String) {
    let line = format!("{} {note}", if ok { "PASS" } else { "FAIL" });
    println!("{line}");
    c.report.push(line);
    c.failures += !ok as u32;
}

fn begin(w: &mut World, c: &mut Ctx, name: &str) {
    c.phase = name.to_string();
    c.stats0 = w.resource::<WalkStats>().clone();
    c.rescues0 = c.stats0.rescues;
    println!("PHASE '{name}' tick {}", c.ticks);
}

fn end(w: &mut World, c: &mut Ctx, note: String) {
    let s = w.resource::<WalkStats>().clone();
    let steps = (s.steps - c.stats0.steps).max(1);
    let line = format!(
        "{}: {} | rescues {}, walker grounded {:.1} %, net-only {}",
        c.phase,
        note,
        s.rescues - c.rescues0,
        100.0 * (s.grounded - c.stats0.grounded) as f64 / steps as f64,
        s.net_only - c.stats0.net_only,
    );
    println!("{line}");
    c.report.push(line);
    c.phase.clear();
}

fn shot(w: &mut World, c: &mut Ctx, script_dir: &std::path::Path, windowed: bool, tag: &str) {
    if !windowed {
        return;
    }
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    c.shot_n += 1;
    let _ = std::fs::create_dir_all(script_dir);
    let path = script_dir.join(format!("shot-{:02}-{tag}.png", c.shot_n));
    w.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
}

// ---------- step builders ----------

fn wait(sec: f64) -> Step {
    Box::new(move |_, c| c.t >= sec)
}

fn settle() -> Step {
    Box::new(|w, c| {
        let r = w.resource::<Ring>();
        let ring_ok = r.pending() == 0 && !r.patches.is_empty();
        let terrain_ok = w.get_resource::<crate::terrain::Terrain>().is_none_or(|t| t.pending == 0);
        if c.t > 2.0 && ring_ok && terrain_ok || c.t > 30.0 {
            println!("settled after {:.1} s simulated, {} patches", c.t, w.resource::<Ring>().patches.len());
            return true;
        }
        false
    })
}

fn stand_still(name: &'static str) -> Vec<Step> {
    vec![
        wait(1.0),
        Box::new(move |w, c| {
            let p = active_pos(w);
            if c.t == 0.0 {
                begin(w, c, name);
                c.p.insert("start", p);
                c.p.insert("last", p);
                c.v.insert("max_step", 0.0);
            }
            let step = p.distance(c.p["last"]);
            *c.v.get_mut("max_step").unwrap() = c.v["max_step"].max(step);
            c.p.insert("last", p);
            if c.t >= 5.0 {
                let drift = p.distance(c.p["start"]);
                let note = format!("max step {:.4} mm, drift {:.4} mm, {:.1} m from planet centre", c.v["max_step"] * 1000.0, drift * 1000.0, (p - planet(w).centre).length());
                end(w, c, note);
                check(c, drift < 0.002, format!("{name}: drift {:.4} mm < 2 mm", drift * 1000.0));
                return true;
            }
            false
        }),
    ]
}

fn walk(name: &'static str, secs: f64, away_from_ship: bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            if away_from_ship {
                let p = player_world(w);
                let s = ship_frame_of(w).origin;
                face_towards(w, p + (p - s));
            }
            begin(w, c, name);
            c.p.insert("start", player_world(w));
            c.v.insert("slow", 0.0);
            c.v.insert("uncovered", 0.0);
            keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], true);
        }
        let p = player_world(w);
        if !w.resource::<Ring>().has_patch_near(p) {
            *c.v.get_mut("uncovered").unwrap() += 1.0;
        }
        let vel = with_player(w, |pl| pl.w.vel);
        let up = planet(w).up(p);
        if c.t > 0.5 && (vel - up * vel.dot(up)).length() < 6.0 {
            *c.v.get_mut("slow").unwrap() += 1.0;
        }
        if c.t >= secs {
            keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], false);
            let walked = p.distance(c.p["start"]);
            let note = format!(
                "walked {walked:.0} m, slow ticks {}, ticks without patch {}, {:.1} km from world origin",
                c.v["slow"], c.v["uncovered"], p.length() / 1000.0
            );
            end(w, c, note);
            let rescues = w.resource::<WalkStats>().rescues - c.rescues0;
            check(c, rescues == 0 && c.v["uncovered"] == 0.0, format!("{name}: 0 rescues ({rescues}), always on a patch"));
            return true;
        }
        false
    })
}

/// Walk from 12 m behind the ship up the ramp to the seat (spike 3).
fn board(name: &'static str, from_outside: bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            let f = ship_frame_of(w);
            if from_outside {
                place_walker(w, f.to_world(DVec3::new(0.0, 0.0, 12.0)));
            }
            face_towards(w, f.to_world(if from_outside { DVec3::new(0.0, 1.5, 0.0) } else { SEAT_POS }));
            keys(w, &[KeyCode::KeyW], true);
        }
        track_look(w, c);
        if c.t >= 5.0 {
            keys(w, &[KeyCode::KeyW], false);
        }
        if c.t >= 5.3 {
            let jump = c.v["look_jump"];
            check(c, jump < 0.5, format!("{name}: look direction steady when entering the cabin (largest step {jump:.3} deg)"));
            let e = ship_e(w);
            let (inside, at_seat) = with_player(w, |p| (p.ship == Some(e), p.ship == Some(e) && p.w.pos.distance(SEAT_POS) < 1.8));
            let f = ship_frame_of(w);
            let pl = planet(w);
            let ramp_end = f.to_world(DVec3::new(0.0, 0.0, 5.8));
            let tilt = (f.rot * DVec3::Y).angle_between(pl.up(f.origin)).to_degrees();
            check(c, at_seat, format!("{name}: reached the seat over the ramp"));
            end(w, c, format!("in cabin {inside}, at seat {at_seat}, ramp end {:.2} m above ground, ship tilt {tilt:.0} deg", pl.above_ground(ramp_end)));
            if !at_seat {
                // Keep the run going: put the walker at the seat.
                let fr = ship_frame_of(w);
                let v = ship_vel(w);
                with_player(w, |p| {
                    if p.ship.is_none() {
                        p.w.change_frame(&Frame::IDENTITY, &fr, -v);
                        p.ship = Some(e);
                    }
                    p.w.pos = DVec3::new(0.0, 0.32, -2.5);
                });
            }
            return true;
        }
        false
    })
}

fn sit() -> Vec<Step> {
    vec![
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }),
        wait(0.5),
        Box::new(|w, _| {
            with_ship(w, |s| s.ctl.hover_assist = true);
            true
        }),
    ]
}

/// Hold keys until a condition or a time limit.
fn hold_until(name: &'static str, ks: &'static [KeyCode], limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            keys(w, ks, true);
        }
        if done(w) || c.t >= limit {
            keys(w, ks, false);
            let (alt, agl, v) = (altitude(w), above_ground(w), ship_vel(w).length());
            end(w, c, format!("{:.1} s, altitude {alt:.0} m, ground {agl:.1} m, speed {v:.1} m/s", c.t));
            return true;
        }
        false
    })
}

/// Point the nose like a player with the mouse: yaw/pitch rate proportional to the error,
/// at most the controller's turn rate. `elevation` is the wanted angle above the horizon.
fn aim(name: &'static str, elevation_deg: f64, secs: f64) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
        }
        let f = ship_frame_of(w);
        let up = planet(w).up(f.origin);
        let nose = f.rot * DVec3::NEG_Z;
        let mut flat = nose - up * nose.dot(up);
        if flat.length_squared() < 1e-6 {
            flat = f.rot * DVec3::Y;
            flat -= up * flat.dot(up);
        }
        let flat = flat.normalize();
        let e = elevation_deg.to_radians();
        let target = flat * e.cos() + up * e.sin();
        let l = f.rot.inverse() * target;
        let yaw_err = (-l.x).atan2(-l.z);
        let pitch_err = l.y.atan2((l.x * l.x + l.z * l.z).sqrt());
        let dt = c.dt.max(1e-6);
        let rate = |err: f64| (err * 2.0).clamp(-2.0, 2.0) * dt; // rad this tick
        // Roll the ship's up towards the planet's up (Q/E, bang-bang with a dead band).
        let back = f.rot * DVec3::Z;
        let ship_up = f.rot * DVec3::Y;
        let up_proj = (up - back * up.dot(back)).normalize_or_zero();
        let roll_err = ship_up.cross(up_proj).dot(back).atan2(ship_up.dot(up_proj));
        let mut m = w.resource_mut::<Controls>();
        m.mouse.x += (-rate(yaw_err) / 0.002) as f32;
        m.mouse.y += (-rate(pitch_err) / 0.002) as f32;
        let (q, e) = (roll_err > 0.05, roll_err < -0.05);
        keys(w, &[KeyCode::KeyQ], q);
        keys(w, &[KeyCode::KeyE], e);
        if c.t >= secs {
            keys(w, &[KeyCode::KeyQ, KeyCode::KeyE], false);
            let nose_el = nose.dot(up).asin().to_degrees();
            end(w, c, format!("nose {nose_el:+.1} deg above horizon (wanted {elevation_deg:+.0})"));
            return true;
        }
        false
    })
}

fn fly_to_space_and_back() -> Vec<Step> {
    vec![
        hold_until("climb to 300 m above ground", &[KeyCode::Space, KeyCode::ShiftLeft], 120.0, |w| above_ground(w) > 300.0),
        aim("pitch up", 30.0, 3.0),
        hold_until("fly to space (7000 m, outside the field)", &[KeyCode::KeyW, KeyCode::ShiftLeft], 240.0, |w| altitude(w) > 7000.0),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "in space: field strength");
                let p = active_pos(w);
                let pl = planet(w);
                c.v.insert("field", pl.field_strength_at(p));
                c.v.insert("g", pl.gravity_at(p).length());
            }
            let note = format!("field strength {:.3}, gravity {:.3} m/s²", c.v["field"], c.v["g"]);
            end(w, c, note);
            check(c, c.v["field"] == 0.0, "reached space (outside the planetary field)".into());
            true
        }),
    ]
}

/// Cabin at speed: stand, then walk, while the ship boosts and rolls with the assist off.
fn cabin_at_speed(name: &'static str, secs: f64, assist: bool, roll: f64) -> Vec<Step> {
    vec![
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF); // stand up
            true
        }),
        wait(1.0),
        Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, name);
                let p = with_player(w, |p| p.w.pos);
                c.p.insert("start", p);
                c.v.insert("drift", 0.0);
                c.v.insert("ymin", f64::MAX);
                c.v.insert("ymax", f64::MIN);
                c.v.insert("left", 0.0);
                c.v.insert("vmax", 0.0);
                with_ship(w, |s| {
                    s.ctl.hover_assist = assist;
                    s.test_input = FlightInput { thrust: DVec3::new(0.0, 0.0, -1.0), boost: true, roll, ..default() };
                });
            }
            let e = ship_e(w);
            let (pos, inside, grounded) = with_player(w, |p| (p.w.pos, p.ship == Some(e), p.w.grounded));
            let half = secs * 0.5;
            // First half stand, then walk forward 1.5 s and back 1 s.
            let walk_t = c.t - half;
            keys(w, &[KeyCode::KeyW], (0.0..1.5).contains(&walk_t));
            keys(w, &[KeyCode::KeyS], (1.5..2.5).contains(&walk_t));
            if c.t < half {
                *c.v.get_mut("drift").unwrap() = c.v["drift"].max(pos.distance(c.p["start"]));
            }
            if inside {
                *c.v.get_mut("ymin").unwrap() = c.v["ymin"].min(pos.y);
                *c.v.get_mut("ymax").unwrap() = c.v["ymax"].max(pos.y);
            } else {
                c.v.insert("left", 1.0);
            }
            let _ = grounded;
            let v = ship_vel(w).length();
            *c.v.get_mut("vmax").unwrap() = c.v["vmax"].max(v);
            if v > 400.0 {
                // Keep rolling, stop boosting at 400 m/s.
                with_ship(w, |s| s.test_input.thrust = DVec3::ZERO);
            }
            if c.t >= secs {
                keys(w, &[KeyCode::KeyW, KeyCode::KeyS], false);
                with_ship(w, |s| {
                    s.test_input = FlightInput::default();
                    s.ctl.hover_assist = true;
                });
                let note = format!(
                    "ship up to {:.0} m/s, roll {roll} (assist {assist}), standing drift {:.4} m, walker height in cabin {:.4}..{:.4} m, left ship {}",
                    c.v["vmax"], c.v["drift"], c.v["ymin"], c.v["ymax"], c.v["left"] > 0.0
                );
                end(w, c, note);
                check(c, c.v["drift"] < 0.001 && c.v["left"] == 0.0 && c.v["ymax"] - c.v["ymin"] < 0.02,
                    format!("{name}: walker stays in the cabin at {:.0} m/s, drift {:.4} m", c.v["vmax"], c.v["drift"]));
                return true;
            }
            false
        }),
        // Back to the seat (test shortcut) and sit.
        Box::new(|w, _| {
            with_player(w, |p| {
                p.w.pos = DVec3::new(0.0, 0.32, -2.5);
                p.w.halt();
            });
            true
        }),
        wait(0.3),
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }),
        wait(0.3),
    ]
}

/// Issue #5: stand up in space, walk out of the back and drift. Outside the field the walker
/// keeps the velocity it left the cabin with (ship velocity plus its walking speed) and nothing
/// pulls it. `drift` gives the ship a speed towards the planet first (stopped not exactly).
/// `push` is the allowed extra speed at the exit: outside the cabin the walker sweeps against the
/// ship colliders of the previous tick, so a moving ramp gives it a small push (issue #9).
/// `careful`: W only in 0.1 s taps every 0.5 s, a careful step out: the walker leaves at step-off
/// speed (3 m/s) at most.
fn step_out_in_space(name: &'static str, drift: f64, push: f64, careful: bool) -> Vec<Step> {
    vec![
        Box::new(move |w, _| {
            tap(w, KeyCode::KeyF); // stand up
            if drift != 0.0 {
                with_ship(w, |s| s.ctl.hover_assist = false);
                let e = ship_e(w);
                let up = planet(w).up(ship_frame_of(w).origin);
                w.get_mut::<LinearVelocity>(e).unwrap().0 = -up * drift;
            }
            true
        }),
        wait(0.5),
        Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, name);
                c.v.remove("out_t");
                c.v.remove("rel_out");
                let f = ship_frame_of(w);
                face_towards(w, f.to_world(DVec3::new(0.0, 1.0, 12.0)));
                keys(w, &[KeyCode::KeyW], true);
            }
            let e = ship_e(w);
            let outside = with_player(w, |p| p.ship != Some(e));
            if careful && !outside {
                keys(w, &[KeyCode::KeyW], c.t % 0.5 < 0.1);
            }
            if outside && !c.v.contains_key("out_t") {
                keys(w, &[KeyCode::KeyW], false);
                c.v.insert("out_t", c.t);
            }
            if c.v.get("out_t").is_some_and(|t| c.t - t >= 0.2) && !c.v.contains_key("rel_out") {
                let v = with_player(w, |p| p.w.vel);
                c.p.insert("v_out", v);
                c.v.insert("rel_out", (v - ship_vel(w)).length());
            }
            let Some(&out_t) = c.v.get("out_t") else {
                if c.t >= 30.0 {
                    keys(w, &[KeyCode::KeyW], false);
                    end(w, c, "never left the cabin".into());
                    check(c, false, format!("{name}: walked out of the ship"));
                    return true;
                }
                return false;
            };
            if c.t - out_t >= 5.2 {
                let pl = planet(w);
                let p = player_world(w);
                let v = with_player(w, |p| p.w.vel);
                let dv = (v - c.p["v_out"]).length();
                let (rel, ship_radial) = (c.v["rel_out"], ship_vel(w).dot(pl.up(ship_frame_of(w).origin)));
                end(w, c, format!(
                    "{:.0} m from planet centre, gravity {:.3} m/s², ship radial {ship_radial:+.3} m/s, walker relative to ship after exit {rel:.3} m/s (walk speed 5), velocity change over 5 s drift {dv:.6} m/s",
                    (p - pl.centre).length(), pl.gravity_at(p).length()
                ));
                let ok = if careful { rel <= 3.01 } else { (rel - 5.0).abs() < 0.01 + push };
                check(c, ok && dv < 1e-6,
                    format!("{name}: walker keeps the ship's velocity plus its own and drifts ({rel:.3} m/s relative, change {dv:.6} m/s)"));
                return true;
            }
            false
        }),
        // Back to the seat (test shortcut) and sit.
        Box::new(|w, _| {
            with_ship(w, |s| s.ctl.hover_assist = true);
            let e = ship_e(w);
            let fr = ship_frame_of(w);
            let v = ship_vel(w);
            with_player(w, |p| {
                if p.ship.is_none() {
                    p.w.change_frame(&Frame::IDENTITY, &fr, -v);
                    p.ship = Some(e);
                }
                p.w.pos = DVec3::new(0.0, 0.32, -2.5);
                p.w.halt();
            });
            true
        }),
        wait(0.3),
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }),
        wait(0.3),
    ]
}

/// Issue #8: the suit in space. Walk out of the stopped ship, brake to rest (X), roll (Q), turn
/// to the ship with the mouse and fly back into the cabin (W), all through `Controls`.
fn suit_in_space() -> Vec<Step> {
    vec![
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF); // stand up
            true
        }),
        wait(0.5),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: walk out of the stopped ship");
                c.v.remove("out_t");
                let f = ship_frame_of(w);
                face_towards(w, f.to_world(DVec3::new(0.0, 1.0, 12.0)));
                keys(w, &[KeyCode::KeyW], true);
            }
            let e = ship_e(w);
            let (outside, suit) = with_player(w, |p| (p.ship != Some(e), p.body.is_some()));
            if outside && !c.v.contains_key("out_t") {
                c.v.insert("out_t", c.t);
            }
            // The suit takes over on the next step after leaving the cabin.
            if c.v.get("out_t").is_some_and(|t| c.t - t > 0.05) || c.t > 10.0 {
                keys(w, &[KeyCode::KeyW], false);
                end(w, c, format!("outside {outside}, suit mode {suit}"));
                check(c, outside && suit, "suit: outside the ship in space the suit takes over".into());
                return true;
            }
            false
        }),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: brake to rest (X)");
                keys(w, &[KeyCode::KeyX], true);
            }
            let v = with_player(w, |p| p.w.vel).length();
            if v < 0.01 || c.t > 6.0 {
                keys(w, &[KeyCode::KeyX], false);
                end(w, c, format!("{v:.4} m/s after {:.2} s", c.t));
                check(c, v < 0.01, format!("suit: X brakes to rest ({v:.4} m/s in {:.2} s)", c.t));
                return true;
            }
            false
        }),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: roll 1 s (Q)");
                let b = with_player(w, |p| p.body.unwrap_or_default());
                c.p.insert("look0", b * DVec3::NEG_Z);
                c.p.insert("head0", b * DVec3::Y);
                keys(w, &[KeyCode::KeyQ], true);
            }
            if c.t >= 1.0 {
                keys(w, &[KeyCode::KeyQ], false);
                let b = with_player(w, |p| p.body.unwrap_or_default());
                let turned = (b * DVec3::Y).angle_between(c.p["head0"]).to_degrees();
                let look = (b * DVec3::NEG_Z).angle_between(c.p["look0"]).to_degrees();
                end(w, c, format!("head turned {turned:.1} deg, look moved {look:.3} deg"));
                check(c, (turned - 1.5f64.to_degrees()).abs() < 3.0 && look < 0.01, format!("suit: Q rolls about the look axis ({turned:.1} deg in 1 s)"));
                return true;
            }
            false
        }),
        // Back into the field (test setup: put the walker 3000 m above the ground for a moment):
        // the suit hands over to walking, the look direction stays, gravity pulls.
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: back in the planetary field (teleport to 3000 m)");
                let pl = planet(w);
                c.p.insert("look", world_look(w));
                let back = with_player(w, |p| p.w.pos);
                c.p.insert("back", back);
                let dir = pl.up(back);
                with_player(w, |p| {
                    p.w.pos = pl.centre + dir * (pl.surface(dir) + 3000.0);
                    p.w.halt();
                });
                return false;
            }
            if c.t >= 0.5 {
                let look = world_look(w).angle_between(c.p["look"]).to_degrees();
                let (suit, v) = with_player(w, |p| (p.body.is_some(), p.w.vel));
                let pl = planet(w);
                let fall = -v.dot(pl.up(player_world(w)));
                let back = c.p["back"];
                with_player(w, |p| {
                    p.w.pos = back;
                    p.w.halt();
                });
                end(w, c, format!("suit {suit}, look moved {look:.4} deg, falling {fall:.2} m/s after 0.5 s"));
                check(c, !suit && look < 0.01 && fall > 0.5, format!("suit: in the field the walker walks again, look kept ({look:.4} deg), falls ({fall:.2} m/s)"));
                return true;
            }
            false
        }),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: turn to the ship and fly back in (mouse, W)");
            }
            let e = ship_e(w);
            let f = ship_frame_of(w);
            let (inside, pos, b) = with_player(w, |p| (p.ship == Some(e), p.w.pos, p.body));
            if inside || c.t > 30.0 {
                keys(w, &[KeyCode::KeyW], false);
                end(w, c, format!("back in the cabin {inside} after {:.1} s", c.t));
                check(c, inside, "suit: flew back into the cabin".into());
                return true;
            }
            let Some(b) = b else { return false };
            // Aim at the middle of the cabin like a player with the mouse (rate proportional to
            // the error), thrust once roughly on target.
            let l = b.inverse() * (f.to_world(DVec3::new(0.0, 1.2, 0.0)) - pos).normalize();
            let yaw_err = (-l.x).atan2(-l.z);
            let pitch_err = l.y.atan2((l.x * l.x + l.z * l.z).sqrt());
            let sens = 0.0025;
            let mut m = w.resource_mut::<Controls>();
            m.mouse.x += (-(yaw_err * 0.2) / sens) as f32;
            m.mouse.y += (-(pitch_err * 0.2) / sens) as f32;
            keys(w, &[KeyCode::KeyW], yaw_err.abs() + pitch_err.abs() < 0.1);
            false
        }),
        // Sit again for the rest of the run.
        Box::new(|w, _| {
            with_player(w, |p| {
                p.w.pos = DVec3::new(0.0, 0.32, -2.5);
                p.w.halt();
            });
            true
        }),
        wait(0.3),
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF);
            true
        }),
        wait(0.3),
    ]
}

/// Long walks (spike 8 T5): 1.8 m/s for 300 s from four starts; steep slopes may stop the walker.
fn t5_starts() -> Vec<(&'static str, DVec3, DVec3)> {
    let recipe: serde_json::Value = serde_json::from_str(crate::env::RECIPE).unwrap();
    let off = |d: DVec3, t: DVec3, m: f64| {
        let a = m / 5000.0;
        (d * a.cos() + t * a.sin()).normalize()
    };
    let toward = |from: DVec3, to: DVec3| (to - from * from.dot(to)).normalize();
    let mut out = vec![("spawn, heading east", DVec3::Y, DVec3::X)];
    for st in recipe["stamps"].as_array().unwrap() {
        let c = st["center"].as_array().unwrap();
        let c = DVec3::new(c[0].as_f64().unwrap(), c[1].as_f64().unwrap(), c[2].as_f64().unwrap()).normalize();
        let t = c.cross(DVec3::Y).normalize();
        match st["type"].as_str().unwrap() {
            "basin" => {
                let s = off(c, t, 900.0);
                out.push(("basin shore, heading to the centre", s, toward(s, c)));
            }
            "escarpment" => {
                let n = c.cross(t).normalize();
                let s = off(c, n, -300.0);
                out.push(("escarpment foot, heading up the step", s, toward(s, c)));
            }
            "plateau" => {
                let s = off(c, t, 1000.0);
                out.push(("plateau approach, heading to the centre", s, toward(s, c)));
            }
            _ => {}
        }
    }
    out
}

fn t5_walk(name: &'static str, dir: DVec3, heading: DVec3, secs: f64) -> Vec<Step> {
    vec![
        Box::new(move |w, _| {
            let pl = planet(w);
            place_walker(w, pl.centre + dir * pl.surface(dir));
            with_player(w, |p| {
                p.w.forward = heading;
                p.w.cfg.walk_speed = 1.8;
            });
            w.resource_mut::<Ring>().force_update();
            true
        }),
        settle(),
        Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, name);
                c.p.insert("last", player_world(w));
                c.v.insert("path", 0.0);
                c.v.insert("hmin", f64::MAX);
                c.v.insert("hmax", f64::MIN);
                keys(w, &[KeyCode::KeyW], true);
            }
            let p = player_world(w);
            *c.v.get_mut("path").unwrap() += p.distance(c.p["last"]);
            c.p.insert("last", p);
            let pl = planet(w);
            let h = (p - pl.centre).length() - pl.radius - pl.sea;
            *c.v.get_mut("hmin").unwrap() = c.v["hmin"].min(h);
            *c.v.get_mut("hmax").unwrap() = c.v["hmax"].max(h);
            let slope = pl.pgen.sample(crate::env::to_v3(pl.up(p))).slope_deg;
            let e = c.v.entry("slope").or_insert(0.0);
            *e = e.max(slope);
            let climb = c.v.entry("climb").or_insert(0.0);
            if slope > 50.0 { *climb += 1.0; }
            if c.t >= secs {
                keys(w, &[KeyCode::KeyW], false);
                let note = format!("path {:.0} m, height above sea {:+.1}..{:+.1} m, steepest ground under the walker {:.1} deg, ticks on ground steeper than 50 deg {}", c.v["path"], c.v["hmin"], c.v["hmax"], c.v["slope"], c.v["climb"]);
                c.v.remove("slope");
                c.v.remove("climb");
                end(w, c, note);
                check(c, w.resource::<WalkStats>().rescues == c.rescues0, format!("{name}: no fall-through"));
                return true;
            }
            false
        }),
    ]
}

/// One flight of the network bot: take off, cruise, turn, brake, descend, land, idle.
fn net_cycle() -> Vec<Step> {
    vec![
        hold_until("takeoff", &[KeyCode::Space, KeyCode::ShiftLeft], 40.0, |w| above_ground(w) > 80.0),
        hold_until("cruise", &[KeyCode::KeyW, KeyCode::ShiftLeft], 8.0, |_| false),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "turn");
                keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], true);
            }
            // About 0.5 rad/s of yaw (the controller turns 0.002 rad per pixel).
            w.resource_mut::<Controls>().mouse.x += 4.2;
            if c.t >= 4.0 {
                keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], false);
                end(w, c, "turned".into());
                return true;
            }
            false
        }),
        hold_until("brake", &[KeyCode::KeyX], 15.0, |w| ship_vel(w).length() < 1.0),
        hold_until("descend", &[KeyCode::ControlLeft, KeyCode::ShiftLeft], 90.0, |w| above_ground(w) < 25.0),
        hold_until("land", &[KeyCode::ControlLeft], 60.0, {
            let mut t = 0.0;
            move |w| {
                t += 1.0 / 60.0;
                t > 3.0 && ship_vel(w).length() < 0.05
            }
        }),
        wait(3.0),
    ]
}

fn foreign_steps(s: &mut Vec<Step>) {
    // 1. A proxy ship 20 km above the planet, flying through the snapshot path.
    s.push(Box::new(|w, _| {
        let pl = planet(w);
        let p0 = DVec3::new(0.0, pl.radius + 20_000.0, 0.0);
        let mut commands = w.commands();
        let proxy = crate::net_live::spawn_proxy(&mut commands, 2, p0, bevy::math::DQuat::IDENTITY);
        w.flush();
        w.insert_resource(ForeignDriver { proxy, buf: Buffer::new(), tick: 0, p0, parked: None, max_pos_err: 0.0, max_rot_err_deg: 0.0 });
        true
    }));
    s.push(wait(1.0));
    // 2. Put the walker into the foreign cabin (test setup, not a boarding mechanic).
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "board the foreign ship (350 m/s, yawing)");
            let proxy = w.resource::<ForeignDriver>().proxy;
            with_player(w, |p| {
                p.ship = Some(proxy);
                p.w.pos = DVec3::new(0.0, 0.31, 1.0);
                p.w.halt();
                p.fly = false;
            });
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        let proxy = w.resource::<ForeignDriver>().proxy;
        let inside = with_player(w, |p| p.ship == Some(proxy));
        check(c, inside, "walker is in the cabin of the remote ship".into());
        true
    }));
    // 3. Stand 6 s (360 ticks) at 350 m/s: drift, deck contact, height.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "stand in the foreign cabin, 6 s");
            let p = with_player(w, |p| p.w.pos);
            c.p.insert("start", p);
            c.v.insert("drift", 0.0);
            c.v.insert("ymin", f64::MAX);
            c.v.insert("ticks", 0.0);
            c.v.insert("floor", 0.0);
            c.v.insert("left", 0.0);
            c.v.insert("shifts0", w.resource::<RenderOrigin>().shifts as f64);
        }
        let proxy = w.resource::<ForeignDriver>().proxy;
        let (pos, grounded, inside) = with_player(w, |p| (p.w.pos, p.w.grounded, p.ship == Some(proxy)));
        let start = c.p["start"];
        *c.v.get_mut("drift").unwrap() = c.v["drift"].max(((pos.x - start.x).powi(2) + (pos.z - start.z).powi(2)).sqrt());
        *c.v.get_mut("ymin").unwrap() = c.v["ymin"].min(pos.y);
        *c.v.get_mut("ticks").unwrap() += 1.0;
        *c.v.get_mut("floor").unwrap() += grounded as u32 as f64;
        if !inside {
            c.v.insert("left", 1.0);
        }
        if c.v["ticks"] >= 360.0 {
            let d = w.resource::<ForeignDriver>();
            let (pe, re) = (d.max_pos_err, d.max_rot_err_deg);
            let shifts = w.resource::<RenderOrigin>().shifts as f64 - c.v["shifts0"];
            let note = format!(
                "lateral standing drift {:.4} m, deck contact {}/360 ticks, lowest feet {:.3} m, left cabin {}, {shifts:.0} render-origin shifts, proxy vs exact path: max {:.4} mm, {:.4} deg",
                c.v["drift"], c.v["floor"], c.v["ymin"], c.v["left"] > 0.0, pe * 1000.0, re
            );
            end(w, c, note);
            check(c, c.v["drift"] < 0.05 && c.v["ymin"] > 0.28 && c.v["floor"] > 300.0 && c.v["left"] == 0.0,
                format!("walker on interpolated 350 m/s foreign ship: drift {:.4} m, deck contact {}/360", c.v["drift"], c.v["floor"]));
            return true;
        }
        false
    }));
    // 3b. The snapshot of the passenger names the OWNER OF THE FOREIGN SHIP as its frame (it used
    //     to say "my own ship", so the pilot composed it into the wrong ship and it vanished).
    s.push(Box::new(|w, c| {
        let pl = planet(w);
        let proxy = w.resource::<ForeignDriver>().proxy;
        let owner = w.get::<RemoteShip>(proxy).unwrap().owner;
        let ship = w.query_filtered::<(&Position, &Rotation, &LinearVelocity), With<Ship>>().single(w).map(|(a, b, c)| (*a, *b, *c)).unwrap();
        let mut q = w.query::<&Player>();
        let player = q.single(w).unwrap();
        let s = crate::net::build_snapshot(1, owner, 0, 2.0, 5, &pl, (&ship.0, &ship.1, &ship.2), player);
        let dec = Snapshot::decode(&s.encode()).unwrap();
        check(c, dec.frame == net_core::snapshot::FrameKind::Ship && dec.frame_id == 2 && dec.wp.distance(player.w.pos) < 1e-6,
            "passenger snapshot names the foreign ship's owner and the local pose".into());
        true
    }));
    // 4. Walk sideways in the local frame for 12 ticks (about 1 m).
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "walk inside the foreign cabin");
            c.p.insert("start", with_player(w, |p| p.w.pos));
            c.v.insert("n", 0.0);
        }
        keys(w, &[KeyCode::KeyD], true);
        *c.v.get_mut("n").unwrap() += 1.0;
        if c.v["n"] >= 12.0 {
            keys(w, &[KeyCode::KeyD], false);
            let dx = with_player(w, |p| p.w.pos.x) - c.p["start"].x;
            end(w, c, format!("moved {dx:.3} m along the cabin x axis"));
            check(c, dx > 0.5 && dx < 1.5, "walking inside the foreign cabin uses the local frame".into());
            return true;
        }
        false
    }));
    // 5. Leave: back to the planet frame at the world pose, and park the remote ship on the ground
    //    next to the spawn.
    s.push(Box::new(|w, c| {
        begin(w, c, "foreign ship parked on the ground, walker beside it");
        let pl = planet(w);
        let dir = (DVec3::Y * pl.radius + DVec3::new(30.0, 0.0, 0.0)).normalize();
        let rot = crate::ship::basis_for_up(dir);
        let mut ground = f64::MIN;
        for cc in [DVec3::ZERO, DVec3::new(2., 0., 4.), DVec3::new(-2., 0., 4.), DVec3::new(2., 0., -4.), DVec3::new(-2., 0., -4.)] {
            let d = (dir * pl.radius + rot * cc).normalize();
            ground = ground.max(pl.surface(d) - pl.radius);
        }
        let pos = pl.centre + dir * (pl.radius + ground + 0.05);
        w.resource_mut::<ForeignDriver>().parked = Some((pos, rot));
        w.resource_mut::<RenderOrigin>().origin = pos.round();
        place_walker(w, pos + rot * DVec3::new(7.0, 0.0, 0.0));
        c.v.insert("n", 0.0);
        c.v.insert("floor", 0.0);
        true
    }));
    s.push(wait(1.5));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            c.p.insert("start", with_player(w, |p| p.w.pos));
            c.v.insert("n", 0.0);
            c.v.insert("floor", 0.0);
        }
        keys(w, &[KeyCode::KeyD], true);
        let grounded = with_player(w, |p| p.w.grounded);
        *c.v.get_mut("n").unwrap() += 1.0;
        *c.v.get_mut("floor").unwrap() += grounded as u32 as f64;
        if c.v["n"] >= 24.0 {
            keys(w, &[KeyCode::KeyD], false);
            let moved = with_player(w, |p| p.w.pos).distance(c.p["start"]);
            end(w, c, format!("moved {moved:.2} m, grounded {}/24 ticks", c.v["floor"]));
            check(c, moved > 1.0 && c.v["floor"] > 15.0, format!("planet-frame walker moves beside the parked foreign ship; floor {}/24", c.v["floor"]));
            // The wire round trip of that pose.
            let pl = planet(w);
            let snap = with_player(w, |p| (p.w.pos, p.w.forward, p.ship));
            let ship = w.query_filtered::<(&Position, &Rotation, &LinearVelocity), With<Ship>>().single(w).map(|(a, b, c)| (*a, *b, *c)).unwrap();
            let mut q = w.query::<&Player>();
            let player = q.single(w).unwrap();
            let s = crate::net::build_snapshot(1, 1, 0, 2.0, 5, &pl, (&ship.0, &ship.1, &ship.2), player);
            let dec = Snapshot::decode(&s.encode()).unwrap();
            check(c, dec.frame == net_core::snapshot::FrameKind::Planet && dec.wp.distance(snap.0 - pl.centre) < 1e-6 && snap.2.is_none(),
                "outside walker snapshot uses the shared planet frame".into());
            return true;
        }
        false
    }));
}

pub fn build(name: &str, out_dir: &std::path::Path, windowed: bool) -> Vec<Step> {
    let dir = out_dir.to_path_buf();
    let shot_step = move |tag: &'static str| -> Step {
        let dir = dir.clone();
        Box::new(move |w, c| {
            shot(w, c, &dir, windowed, tag);
            true
        })
    };
    let mut s: Vec<Step> = vec![settle()];
    match name {
        // Walking only.
        "walk" => {
            s.extend(stand_still("stand still 5 s (walker)"));
            s.push(walk("walk 20 s (run)", 20.0, true));
        }
        "t5" => {
            for (n, d, h) in t5_starts() {
                s.extend(t5_walk(n, d, h, 300.0));
            }
        }
        // Network bot: sit in the parked ship (test shortcut) and fly cycles until the run ends.
        "net" => {
            s.push(Box::new(|w, _| {
                let e = ship_e(w);
                let fr = ship_frame_of(w);
                let v = ship_vel(w);
                with_player(w, |p| {
                    if p.ship.is_none() {
                        p.w.change_frame(&Frame::IDENTITY, &fr, -v);
                        p.ship = Some(e);
                    }
                    p.w.pos = DVec3::new(0.0, 0.32, -2.5);
                    p.w.halt();
                });
                true
            }));
            s.extend(sit());
            for _ in 0..60 {
                s.extend(net_cycle());
            }
        }
        "foreign" => foreign_steps(&mut s),
        // Issue #5: step out of the ship in space (seat by test shortcut, then fly up).
        "space" => {
            s.push(Box::new(|w, _| {
                let e = ship_e(w);
                with_player(w, |p| {
                    p.ship = Some(e);
                    p.w.pos = DVec3::new(0.0, 0.32, -2.5);
                    p.w.halt();
                });
                true
            }));
            s.extend(sit());
            s.extend(fly_to_space_and_back());
            s.push(hold_until("firm brake in space", &[KeyCode::KeyX], 30.0, |w| ship_vel(w).length() < 0.5));
            s.push(aim("nose up like the climb", 30.0, 3.0));
            s.extend(step_out_in_space("in space: walk out of the stopped ship", 0.0, 0.0, false));
            s.extend(step_out_in_space("in space: walk out of a ship drifting at 3 m/s", 3.0, 0.6, false));
            s.push(hold_until("firm brake in space", &[KeyCode::KeyX], 30.0, |w| ship_vel(w).length() < 0.01));
            s.extend(step_out_in_space("in space: step out carefully (tap W)", 0.0, 0.0, true));
            s.push(hold_until("firm brake in space", &[KeyCode::KeyX], 30.0, |w| ship_vel(w).length() < 0.01));
            s.extend(suit_in_space());
        }
        "full" => {
            s.push(shot_step("ground"));
            s.extend(stand_still("stand still 5 s (walker)"));
            s.push(walk("walk 20 s (run)", 20.0, true));
            s.push(board("walk up the ramp into the parked ship", true));
            s.extend(sit());
            s.extend(fly_to_space_and_back());
            s.push(shot_step("space"));
            s.extend(cabin_at_speed("in space: stand + walk, boost + roll, assist off", 8.0, false, 0.3));
            s.push(hold_until("firm brake in space", &[KeyCode::KeyX], 30.0, |w| ship_vel(w).length() < 0.5));
            s.push(aim("turn back towards the planet", -80.0, 4.0));
            s.push(hold_until("dive back", &[KeyCode::KeyW, KeyCode::ShiftLeft], 240.0, |w| above_ground(w) < 800.0));
            s.push(hold_until("firm brake", &[KeyCode::KeyX], 15.0, |w| ship_vel(w).length() < 0.5));
            s.push(aim("level out", 0.0, 3.0));
            s.push(hold_until("descend to 120 m above ground", &[KeyCode::ControlLeft, KeyCode::ShiftLeft], 180.0, |w| above_ground(w) < 120.0));
            s.push(hold_until("land", &[KeyCode::ControlLeft], 60.0, {
                let mut t = 0.0;
                move |w| {
                    t += 1.0 / 60.0;
                    t > 3.0 && ship_vel(w).length() < 0.05
                }
            }));
            s.push(Box::new(|w, c| {
                let (v, agl) = (ship_vel(w).length(), above_ground(w));
                check(c, v < 0.05 && agl < 1.0, format!("landed: {v:.3} m/s, {agl:.2} m above ground"));
                true
            }));
            s.push(shot_step("landed"));
            s.extend(stand_still("idle 5 s (landed ship)"));
            s.push(Box::new(|w, _| {
                tap(w, KeyCode::KeyF);
                true
            }));
            s.push(wait(0.5));
            s.push(Box::new(|w, c| {
                if c.t == 0.0 {
                    begin(w, c, "walk out of the landed ship");
                    let f = ship_frame_of(w);
                    face_towards(w, f.to_world(DVec3::new(0.0, 1.0, 12.0)));
                    keys(w, &[KeyCode::KeyW], true);
                }
                track_look(w, c);
                if c.t >= 4.0 {
                    keys(w, &[KeyCode::KeyW], false);
                }
                if c.t >= 4.5 {
                    let outside = with_player(w, |p| p.ship.is_none());
                    let agl = above_ground(w);
                    end(w, c, format!("outside {outside}, {agl:.2} m above ground"));
                    check(c, outside && agl.abs() < 0.5, "walked out of the landed ship onto the ground".into());
                    let jump = c.v["look_jump"];
                    check(c, jump < 0.5, format!("look direction steady when leaving the cabin (largest step {jump:.3} deg)"));
                    return true;
                }
                false
            }));
            s.push(board("walk back in to the seat", false));
            s.extend(sit());
            s.push(hold_until("climb to 400 m above ground", &[KeyCode::Space, KeyCode::ShiftLeft], 60.0, |w| above_ground(w) > 400.0));
            s.push(wait(3.0));
            s.extend(cabin_at_speed("in atmosphere: stand + walk, boost + roll, assist on", 8.0, true, 0.3));
            s.push(Box::new(|w, _| {
                if let Some(mut v) = w.get_resource_mut::<ViewState>() {
                    v.orbit = true;
                }
                true
            }));
            s.push(wait(1.0));
            s.push(shot_step("orbit"));
            s.push(wait(0.2));
        }
        other => panic!("unknown scenario {other}"),
    }
    s
}

pub fn run_script(w: &mut World) {
    let dt = w.resource::<Time>().delta_secs_f64();
    w.resource_scope(|w, mut sc: Mut<Script>| {
        if sc.done {
            return;
        }
        let sc = &mut *sc;
        sc.ctx.dt = dt;
        sc.ctx.ticks += 1;
        if sc.i >= sc.steps.len() {
            sc.done = true;
            let st = w.resource::<WalkStats>().clone();
            let summary = format!(
                "TOTAL: rescues {}, walker grounded {:.1} % of {} steps, net-only {}, depenetrations {}, origin shifts {}, simulated {:.1} s",
                st.rescues,
                100.0 * st.grounded as f64 / st.steps.max(1) as f64,
                st.steps,
                st.net_only,
                st.depenetrations,
                w.resource::<RenderOrigin>().shifts,
                sc.ctx.ticks as f64 / 60.0
            );
            println!("{summary}");
            sc.ctx.report.push(summary);
            let _ = std::fs::create_dir_all(&sc.out_dir);
            let path = sc.out_dir.join(format!("{}.txt", sc.name));
            let _ = std::fs::write(&path, sc.ctx.report.join("\n") + "\n");
            println!("results: {}", path.display());
            println!("CHECKS: {} failures", sc.ctx.failures);
            w.write_message(if sc.ctx.failures == 0 && st.rescues == 0 { AppExit::Success } else { AppExit::error() });
            return;
        }
        let i = sc.i;
        if (sc.steps[i])(w, &mut sc.ctx) {
            sc.i += 1;
            sc.ctx.t = 0.0;
        } else {
            sc.ctx.t += dt;
        }
    });
}

