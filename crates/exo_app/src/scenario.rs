//! Scripted runs: a list of steps, each called once per fixed tick until it returns true. Steps
//! drive the game only through `Controls` and a few test hooks (teleport for test setup, ship
//! test input). A run exits non-zero when a check fails.
use crate::controls::{Bindings, Controls};
use crate::env::PlanetRes;
use crate::origin::RenderOrigin;
use crate::ring::Ring;
use crate::ship::{Ship, SEAT_POS};
use crate::view::ViewState;
use crate::walker::{ship_frame, Player, WalkStats};
use crate::warp::{PendingPlanet, SystemRes, WarpDrive, WarpTelemetry};
use warp_core::{Abort, Drive, Event, Obstacle, Phase, PlanetId};
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec2, DVec3};
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
    mut q: Query<(&mut Position, &mut Rotation, &mut LinearVelocity, &mut AngularVelocity, &mut RemoteShip)>,
) {
    d.tick += 1;
    let t = d.tick as f64 * DT;
    if d.tick % 2 == 0 {
        let (p, qn, v) = foreign_truth(&d, t);
        let mut s = Snapshot::new(2, t, p, v, qn);
        s.seq = d.tick as u32;
        // Parked means landed: its cabin gravity is off.
        s.lag = if d.parked.is_some() { 0.0 } else { 1.0 };
        // Through the wire format, like a received packet.
        let s = Snapshot::decode(&s.encode()).expect("own snapshot decodes");
        d.buf.push(s);
    }
    let target = t - 0.15;
    let Some(sample) = d.buf.sample(target) else { return };
    let Ok((mut p, mut r, mut v, mut w, mut rs)) = q.get_mut(d.proxy) else { return };
    rs.lag = sample.s.lag;
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
    with_player(w, |p| {
        let pos = p.world_pos(f);
        let up = p.world_up(f);
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
    with_player(w, |p| p.world_look(f))
}

/// Issue #7: largest change from one tick to the next of the look direction and of the camera's
/// up (degrees) and of the eye position less the walk (m), in `c.v["look_jump"]`, `["up_jump"]`,
/// `["eye_jump"]`. Call every tick of a step; the first tick only starts it (setup turns).
fn track_look(w: &mut World, c: &mut Ctx) {
    let l = world_look(w);
    let f = ship_frame_of(w);
    let (feet, up) = with_player(w, |p| (p.world_pos(f), p.view_up));
    let eye = feet + up * crate::walker::EYE_HEIGHT;
    // Skip the first ticks: setup may teleport, and up follows on the next step.
    if c.t > 0.05 {
        let mut max = |k: &'static str, x: f64| {
            c.v.insert(k, c.v.get(k).copied().unwrap_or(0.0).max(x));
        };
        max("look_jump", l.angle_between(c.p["look"]).to_degrees());
        max("up_jump", up.angle_between(c.p["up"]).to_degrees());
        max("eye_jump", ((eye - c.p["eye"]) - (feet - c.p["feet"])).length());
    } else {
        for k in ["look_jump", "up_jump", "eye_jump"] {
            c.v.insert(k, 0.0);
        }
    }
    c.p.insert("look", l);
    c.p.insert("up", up);
    c.p.insert("eye", eye);
    c.p.insert("feet", feet);
}

/// No step in the view: no tick turns the look by more than 0.5 deg or the camera's up by more
/// than 1 deg, or moves the eye more than 3 cm against the feet (entering a 6 deg tilted ship
/// once turned up by 5.6 deg in one tick at the cabin edge).
fn check_steady(c: &mut Ctx, what: &str) {
    let (look, up, eye) = (c.v["look_jump"], c.v["up_jump"], c.v["eye_jump"]);
    check(c, look < 0.5 && up < 1.0 && eye < 0.03,
        format!("{what}: view steady (largest step: look {look:.3} deg, up {up:.3} deg, eye {:.1} mm)", eye * 1000.0));
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
            check_steady(c, &format!("{name}: entering the cabin"));
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
                put_at_seat(w);
            }
            return true;
        }
        false
    })
}

/// Test shortcut: put the walker at rest in the own cabin, just behind the seat.
fn put_at_seat(w: &mut World) {
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
}

/// Back to the seat (test shortcut) and sit.
fn back_to_seat() -> Vec<Step> {
    vec![
        Box::new(|w, _| {
            put_at_seat(w);
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
            let lag = with_ship(w, |s| s.lag);
            check(c, !lag.landed && lag.level == 1.0, format!("cabin gravity on in flight ({:.0} %)", lag.level * 100.0));
            true
        }),
    ]
}

/// Cabin at speed: stand, then walk, while the ship boosts and rolls with the assist off.
fn cabin_at_speed(name: &'static str, secs: f64, assist: bool, roll: f64) -> Vec<Step> {
    let mut steps: Vec<Step> = vec![
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
    ];
    steps.extend(back_to_seat());
    steps
}

/// Issue #5: stand up in space, walk out of the back and drift. Outside the field the walker
/// keeps the velocity it left the cabin with (ship velocity plus its walking speed) and nothing
/// pulls it. `drift` gives the ship a speed towards the planet first (stopped not exactly).
/// `careful`: W only in 0.1 s taps every 0.5 s, a careful step out: the walker leaves at step-off
/// speed (3 m/s) at most.
fn step_out_in_space(name: &'static str, drift: f64, careful: bool) -> Vec<Step> {
    let mut steps: Vec<Step> = vec![
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
            track_look(w, c);
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
                let ok = if careful { rel <= 3.01 } else { (rel - 5.0).abs() < 0.01 };
                check(c, ok && dv < 1e-6,
                    format!("{name}: walker keeps the ship's velocity plus its own and drifts ({rel:.3} m/s relative, change {dv:.6} m/s)"));
                // The ship's nose is 30 deg up: leaving keeps the cabin's orientation.
                check_steady(c, name);
                return true;
            }
            false
        }),
        Box::new(|w, _| {
            with_ship(w, |s| s.ctl.hover_assist = true);
            true
        }),
    ];
    steps.extend(back_to_seat());
    steps
}

/// Issue #8: the suit in space. Walk out of the stopped ship, brake to rest (X), roll (Q), turn
/// to the ship with the mouse and fly back into the cabin (W), all through `Controls`.
fn suit_in_space() -> Vec<Step> {
    let mut steps: Vec<Step> = vec![
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
        // the suit hands over to walking, the look direction stays, gravity pulls, and the walker
        // rights itself from the rolled suit orientation without a step.
        Box::new(|w, c| {
            track_look(w, c);
            if c.t == 0.0 {
                begin(w, c, "suit: back in the planetary field (teleport to 3000 m)");
                let pl = planet(w);
                c.p.insert("look_before", world_look(w));
                let back = with_player(w, |p| p.w.pos);
                c.p.insert("back", back);
                let dir = pl.up(back);
                with_player(w, |p| {
                    p.w.pos = pl.centre + dir * (pl.surface(dir) + 3000.0);
                    p.w.halt();
                });
                return false;
            }
            if c.t >= 3.5 {
                let look = world_look(w).angle_between(c.p["look_before"]).to_degrees();
                let (suit, v, up) = with_player(w, |p| (p.body.is_some(), p.w.vel, p.view_up));
                let pl = planet(w);
                let fall = -v.dot(pl.up(player_world(w)));
                let tilt = up.angle_between(pl.up(player_world(w))).to_degrees();
                let step = c.v["up_jump"];
                let back = c.p["back"];
                with_player(w, |p| {
                    p.w.pos = back;
                    p.w.halt();
                });
                end(w, c, format!("suit {suit}, look moved {look:.4} deg, falling {fall:.2} m/s, up {tilt:.3} deg from the planet's, largest righting step {step:.2} deg after 3.5 s"));
                check(c, !suit && look < 0.01 && fall > 0.5, format!("suit: in the field the walker walks again, look kept ({look:.4} deg), falls ({fall:.2} m/s)"));
                check(c, tilt < 0.5 && step < 1.55, format!("suit: rights itself slowly in the field ({tilt:.3} deg left, at most {step:.2} deg per tick)"));
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
    ];
    steps.extend(back_to_seat());
    steps
}

/// Issue #9: the ship coasts at 20 m/s, the walker drifts with it in the suit 0.4 m behind the
/// end of its ramp. The ship's colliders sit one tick (0.33 m) behind its body; swept as if they
/// stood still they stopped the walker. It must drift on with the ship.
fn drift_behind_moving_ship() -> Vec<Step> {
    let mut steps: Vec<Step> = vec![
        Box::new(|w, _| {
            tap(w, KeyCode::KeyF); // stand up
            true
        }),
        wait(0.3),
        Box::new(|w, _| {
            with_ship(w, |s| s.ctl.hover_assist = false);
            let e = ship_e(w);
            let f = ship_frame_of(w);
            w.get_mut::<LinearVelocity>(e).unwrap().0 = f.rot * DVec3::new(0.0, 0.0, -20.0);
            true
        }),
        wait(0.5),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "suit: drift with a ship coasting at 20 m/s, 0.4 m behind its ramp");
                let f = ship_frame_of(w);
                let v = ship_vel(w);
                with_player(w, |p| {
                    p.ship = None;
                    p.w.pos = f.to_world(DVec3::new(0.0, -1.2, 6.6 + 0.35 + 0.4));
                    p.w.vel = v;
                    p.w.move_vel = v;
                    p.body = Some(f.rot);
                    p.w.forward = f.rot * DVec3::NEG_Z;
                    p.pitch = 0.0;
                    p.view_up = f.rot * DVec3::Y;
                });
                c.v.insert("gap0", f.to_local(player_world(w)).z);
            }
            if c.t >= 2.0 {
                let f = ship_frame_of(w);
                let gap = f.to_local(player_world(w)).z;
                let dv = (with_player(w, |p| p.w.vel) - ship_vel(w)).length();
                let moved = gap - c.v["gap0"];
                end(w, c, format!("walker moved {moved:+.4} m against the ship in 2 s, velocity off the ship's by {dv:.4} m/s"));
                check(c, moved.abs() < 0.05 && dv < 0.01, format!("suit: drifts on with a moving ship it touches ({moved:+.4} m, {dv:.4} m/s)"));
                return true;
            }
            false
        }),
        Box::new(|w, _| {
            let e = ship_e(w);
            w.get_mut::<LinearVelocity>(e).unwrap().0 = DVec3::ZERO;
            with_ship(w, |s| s.ctl.hover_assist = true);
            true
        }),
    ];
    steps.extend(back_to_seat());
    steps
}

/// G in the landed ship: cabin gravity comes up (up turns to the floor's up over 1 s, no step),
/// and goes down again (up back to the planet's).
fn lag_by_hand() -> Vec<Step> {
    let toggle = |name: &'static str, on: bool| -> Step {
        Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, name);
                tap(w, KeyCode::KeyG);
            }
            track_look(w, c);
            if c.t >= 1.3 {
                let f = ship_frame_of(w);
                let pl = planet(w);
                let up = with_player(w, |p| p.world_up(f));
                let want = if on { f.rot * DVec3::Y } else { pl.up(f.origin) };
                let off = up.angle_between(want).to_degrees();
                let tilt = (f.rot * DVec3::Y).angle_between(pl.up(f.origin)).to_degrees();
                let level = with_ship(w, |s| s.lag.level);
                end(w, c, format!("gravity {:.0} %, up {off:.3} deg from the {} up, ship tilt {tilt:.1} deg", level * 100.0, if on { "floor's" } else { "planet's" }));
                check(c, off < 0.1 && level == if on { 1.0 } else { 0.0 }, format!("{name}: up follows the cabin gravity ({off:.3} deg)"));
                check_steady(c, name);
                return true;
            }
            false
        })
    };
    vec![toggle("G in the landed ship: cabin gravity on", true), toggle("G again: cabin gravity off", false)]
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
    // 6. Issue #11: the parked (landed) foreign ship stands 8 deg tilted and sends its cabin
    //    gravity off; a walker in its cabin stands about the planet's up, not the tilted floor's.
    s.push(Box::new(|w, _| {
        let mut d = w.resource_mut::<ForeignDriver>();
        let (pos, rot) = d.parked.unwrap();
        d.parked = Some((pos + rot * DVec3::new(0.0, 0.6, 0.0), rot * bevy::math::DQuat::from_rotation_x(8f64.to_radians())));
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "in the cabin of the tilted, landed foreign ship");
            let proxy = w.resource::<ForeignDriver>().proxy;
            let (p, r) = (w.get::<Position>(proxy).unwrap().0, w.get::<Rotation>(proxy).unwrap().0);
            let f = Frame { origin: p, rot: r };
            with_player(w, |pl| {
                pl.ship = Some(proxy);
                pl.w.pos = DVec3::new(0.0, 0.4, -1.0);
                pl.w.halt();
                pl.w.forward = DVec3::NEG_Z;
                pl.cabin_up = DVec3::Y;
                pl.view_up = f.rot * DVec3::Y;
            });
        }
        if c.t >= 1.0 {
            let proxy = w.resource::<ForeignDriver>().proxy;
            let (p, r) = (w.get::<Position>(proxy).unwrap().0, w.get::<Rotation>(proxy).unwrap().0);
            let lag = w.get::<RemoteShip>(proxy).unwrap().lag;
            let pl = planet(w);
            let up = with_player(w, |pl| r * pl.cabin_up);
            let off = up.angle_between(pl.up(p)).to_degrees();
            let tilt = (r * DVec3::Y).angle_between(pl.up(p)).to_degrees();
            end(w, c, format!("received cabin gravity {:.0} %, up {off:.3} deg from the planet's, ship tilt {tilt:.1} deg", lag * 100.0));
            check(c, lag == 0.0 && off < 0.5 && tilt > 7.0, format!("landed foreign ship: its cabin gravity is off over the wire, walker stands about the planet's up ({off:.3} deg)"));
            return true;
        }
        false
    }));
}


// ---------- warp ----------

const HEARTH: PlanetId = PlanetId(0);
const CINDER: PlanetId = PlanetId(1);
/// The ship must be this close to the drive's end point on the tick after it got there (m):
/// it is placed exactly, then flies one tick at the exit speed (6.7 m) under the pilot.
const END_TOLERANCE: f64 = 10.0;
/// Nose within this angle of the target's centre on the first tick after the arrival (deg).
const NOSE_TOLERANCE: f64 = 2.0;

fn warp_state(w: &World) -> (Phase, Option<Abort>) {
    let wd = w.resource::<WarpDrive>();
    (wd.drive.phase, wd.last_abort)
}

fn tel(w: &World) -> &WarpTelemetry {
    w.resource::<WarpTelemetry>()
}

/// Place the ship (test setup): pose, no velocity.
fn teleport_ship(w: &mut World, pos: DVec3, rot: DQuat) {
    let e = ship_e(w);
    w.get_mut::<Position>(e).unwrap().0 = pos;
    w.get_mut::<Rotation>(e).unwrap().0 = rot;
    w.get_mut::<LinearVelocity>(e).unwrap().0 = DVec3::ZERO;
    w.get_mut::<AngularVelocity>(e).unwrap().0 = DVec3::ZERO;
}

fn nose_along(dir: DVec3) -> DQuat {
    DQuat::from_rotation_arc(DVec3::NEG_Z, dir)
}

/// Orbit point of planet `i`: 7000 m from the centre, on its +y side, nose along the course
/// to planet `to` (what a pilot would aim at; the course is the drive's own path start).
fn orbit_pose(w: &World, i: PlanetId, to: PlanetId) -> (DVec3, DQuat) {
    let sys = &w.resource::<SystemRes>().0;
    let pos = sys.planet(i).centre() + DVec3::Y * 7000.0;
    let view = warp_core::ShipView { pos, forward: DVec3::X, speed: 0.0 };
    let mut d = Drive::new(sys.drive.clone());
    d.begin(to, &view, sys, &[]).expect("orbit start is free");
    (pos, nose_along(d.path().unwrap().start_dir()))
}

/// Like a pilot holding the course while the drive spools and calibrates.
fn hold_course(w: &mut World) {
    let (phase, dir) = {
        let wd = w.resource::<WarpDrive>();
        (wd.drive.phase, wd.drive.path().map(|p| p.start_dir()))
    };
    if matches!(phase, Phase::Spooling | Phase::Calibrating)
        && let Some(d) = dir
    {
        let e = ship_e(w);
        w.get_mut::<Rotation>(e).unwrap().0 = nose_along(d);
        w.get_mut::<AngularVelocity>(e).unwrap().0 = DVec3::ZERO;
    }
}

fn events_since(w: &World, from: usize) -> Vec<(f64, Event)> {
    tel(w).log[from..].to_vec()
}

/// How the scripted flight is flown.
#[derive(Clone, Copy, PartialEq)]
enum Flight {
    /// The pilot stands up when the ramp-up starts and the walker stays in the cabin (deck
    /// contact and drift; walking at top speed; the cabin view of the cruise).
    Passenger,
    /// The pilot stays seated (chase camera: the view from outside).
    Seated,
    /// Seated; the pilot holds J from the middle of the path on: emergency exit.
    Emergency,
}

/// One warp from where the ship is placed (`start`) to planet `to`, flown by script: the pilot
/// holds the course. Ends 2.5 s after the arrival or drop. Screenshots of the cruise, the exit
/// and 2 s after it.
fn warp_flight(name: &'static str, tag: &'static str, start: Option<PlanetId>, to: PlanetId, how: Flight, dir: std::path::PathBuf, windowed: bool) -> Step {
    Box::new(move |w, c| {
        let lim = 240.0;
        if c.t == 0.0 {
            begin(w, c, name);
            for k in ["stood", "drift", "g0", "s0", "shot_due", "walk_t0", "walk_z0", "walk_g0", "walk_s0", "walk_far", "walk_done", "cabin_shot", "t_end", "nose", "end_err", "held", "terrain_ok", "terrain_n", "started"] {
                c.v.remove(k);
            }
            c.p.remove("stand_pos");
            if let Some(from) = start {
                let (p, r) = orbit_pose(w, from, to);
                teleport_ship(w, p, r);
            }
            c.v.insert("log0", tel(w).log.len() as f64);
            c.v.insert("grounded0", w.resource::<WalkStats>().grounded as f64);
            c.v.insert("steps0", w.resource::<WalkStats>().steps as f64);
            c.v.insert("min_clear", f64::MAX);
            c.v.insert("depen0", w.resource::<WalkStats>().depenetrations as f64);
            w.resource_mut::<WarpDrive>().selected = to;
            tel_reset_move(w);
        }
        if c.t < 0.2 {
            return false;
        }
        if !c.v.contains_key("started") {
            c.v.insert("started", 1.0);
            tap(w, KeyCode::KeyJ);
            return false;
        }
        hold_course(w);
        let phase = warp_state(w).0;
        // The cruise from outside (seated: chase camera), 1 s into it.
        if phase == Phase::Cruise && how == Flight::Seated && !c.v.contains_key("shot_due") {
            c.v.insert("shot_due", c.t + 1.0);
        }
        if c.v.get("shot_due").is_some_and(|&due| due > 0.0 && c.t >= due) {
            c.v.insert("shot_due", -1.0);
            shot(w, c, &dir, windowed, &format!("{tag}-cruise-outside"));
        }
        // Passenger: pilot out of the seat when the drive takes the ship.
        if how == Flight::Passenger && phase == Phase::RampUp && !c.v.contains_key("stood") {
            c.v.insert("stood", 1.0);
            tap(w, KeyCode::KeyF);
        }
        if how == Flight::Passenger && c.v.contains_key("stood") && !c.p.contains_key("stand_pos") && with_player(w, |p| !p.seated) {
            let local = with_player(w, |p| p.w.pos);
            c.p.insert("stand_pos", local);
            c.v.insert("g0", w.resource::<WalkStats>().grounded as f64);
            c.v.insert("s0", w.resource::<WalkStats>().steps as f64);
            c.v.insert("drift", 0.0);
        }
        // Walk back and forth in the cabin at top speed: S for 1 s, W for 0.6 s (5 m/s walking),
        // starting in the cruise (2.2 s long, so the walk may end in the ramp-down).
        // The cabin picture is taken at the back of the cabin, looking at the window.
        let walking = c.v.contains_key("walk_t0") || phase == Phase::Cruise;
        if how == Flight::Passenger && c.p.contains_key("stand_pos") && walking && phase.on_rails() && !c.v.contains_key("walk_done") {
            let st = w.resource::<WalkStats>().clone();
            let z = with_player(w, |p| p.w.pos.z);
            if !c.v.contains_key("walk_t0") {
                c.v.insert("walk_t0", c.t);
                c.v.insert("walk_z0", z);
                c.v.insert("walk_g0", st.grounded as f64);
                c.v.insert("walk_s0", st.steps as f64);
                keys(w, &[KeyCode::KeyS], true);
            }
            let wt = c.t - c.v["walk_t0"];
            if wt >= 1.0 && !c.v.contains_key("cabin_shot") {
                c.v.insert("cabin_shot", 1.0);
                shot(w, c, &dir, windowed, &format!("{tag}-cruise-cabin"));
            }
            if (1.0..1.6).contains(&wt) {
                keys(w, &[KeyCode::KeyS], false);
                keys(w, &[KeyCode::KeyW], true);
                c.v.insert("walk_far", c.v.get("walk_far").copied().unwrap_or(z).max(z));
            } else if wt < 1.0 {
                c.v.insert("walk_far", z);
            } else {
                keys(w, &[KeyCode::KeyW], false);
                c.v.insert("walk_done", 1.0);
                let (g, n) = (st.grounded as f64 - c.v["walk_g0"], (st.steps as f64 - c.v["walk_s0"]).max(1.0));
                let (far, back) = (c.v["walk_far"] - c.v["walk_z0"], z - c.v["walk_z0"]);
                let in_cabin = with_player(w, |p| p.ship.is_some());
                check(c, in_cabin && g / n > 0.99 && far > 2.0 && back.abs() < 3.0, format!("{name}: walking in the cabin at {:.0} km/s at the end: {far:.2} m back, {back:.2} m from the start after walking forward again, deck contact {:.1} % of {n:.0} steps", w.resource::<WarpDrive>().drive.speed() / 1000.0, 100.0 * g / n));
            }
        }
        if let Some(&sp) = c.p.get("stand_pos") {
            let local = with_player(w, |p| p.w.pos);
            let d = local.distance(sp);
            // Drift is measured while standing still: before the walk starts.
            if !c.v.contains_key("walk_t0") {
                let e = c.v.get_mut("drift").unwrap();
                *e = e.max(d);
            }
        }
        // Emergency exit: hold J from the middle of the path until the drop starts.
        if how == Flight::Emergency {
            let past_half = {
                let wd = w.resource::<WarpDrive>();
                let pos = ship_frame_of_ro(w);
                wd.drive.path().is_some_and(|p| pos.distance(p.at(0.0).0) > p.length() * 0.5)
            };
            let hold = phase.on_rails() && phase != Phase::EmergencyDrop && past_half;
            if hold && !c.v.contains_key("held") {
                c.v.insert("held", c.t);
            }
            keys(w, &[KeyCode::KeyJ], hold);
        }
        // Distance to every planet's centre, each tick (the core checks the swept path).
        let ship = ship_frame_of(w).origin;
        let sys = w.resource::<SystemRes>().0.clone();
        for p in &sys.planets {
            let m = c.v.get_mut("min_clear").unwrap();
            *m = m.min(ship.distance(p.centre()) - p.obstruction_radius);
        }
        // The tick after the arrival or drop: end point error, nose, picture of the exit.
        let log0 = c.v["log0"] as usize;
        let ended = events_since(w, log0).iter().any(|(_, e)| matches!(e, Event::Arrived | Event::DroppedOut));
        if ended && !c.v.contains_key("t_end") {
            c.v.insert("t_end", c.t);
            c.v.insert("end_err", tel(w).end_error.unwrap_or(f64::NAN));
            let (pos, nose) = {
                let e = ship_e(w);
                (w.get::<Position>(e).unwrap().0, w.get::<Rotation>(e).unwrap().0 * DVec3::NEG_Z)
            };
            c.v.insert("nose", nose.angle_between(sys.planet(to).centre() - pos).to_degrees());
            c.p.insert("end_pos", pos);
            c.v.insert("end_speed", ship_vel(w).length());
            let st = w.resource::<WalkStats>();
            c.v.insert("g_end", st.grounded as f64);
            c.v.insert("s_end", st.steps as f64);
            let label = if how == Flight::Emergency { "dropped" } else { "exit" };
            shot(w, c, &dir, windowed, &format!("{tag}-{label}"));
        }
        if let Some(&te) = c.v.get("t_end")
            && c.t - te >= 2.0
            && !c.v.contains_key("terrain_n")
        {
                let label = if how == Flight::Emergency { "dropped" } else { "exit" };
                shot(w, c, &dir, windowed, &format!("{tag}-{label}-2s"));
                let (for_planet, visible) = w.get_resource::<crate::terrain::Terrain>().map_or((None, 0), |t| (Some(t.for_planet), t.visible));
                c.v.insert("terrain_n", visible as f64);
                c.v.insert("terrain_ok", (for_planet == Some(to) && visible > 0) as u8 as f64);
        }
        let done = c.v.get("t_end").is_some_and(|&te| c.t - te >= 2.5);
        if done || c.t >= lim {
            keys(w, &[KeyCode::KeyJ, KeyCode::KeyW, KeyCode::KeyS], false);
            if how == Flight::Passenger && !c.v.contains_key("walk_done") {
                check(c, false, format!("{name}: the walk in the cabin did not finish on rails"));
            }
            let moved = tel(w).max_tick_move;
            let ev = events_since(w, log0);
            let at = |p: Phase| ev.iter().find(|(_, e)| *e == Event::Phase(p)).map(|(t, _)| *t);
            let end = ev.iter().find(|(_, e)| matches!(e, Event::Arrived | Event::DroppedOut)).map(|(t, e)| (*t, *e));
            let (Some(t_ramp), Some((t_arr, end_ev))) = (at(Phase::RampUp), end) else {
                check(c, false, format!("{name}: arrived or dropped out (events {ev:?})"));
                return true;
            };
            let cfg = sys.drive.clone();
            let dur = t_arr - t_ramp;
            let err = c.v["end_err"];
            let pos = c.p["end_pos"];
            let gen_ms = w.resource::<PendingPlanet>().gen_ms.unwrap_or(-1.0);
            let planet_id = w.resource::<PlanetRes>().id;
            check(c, err <= END_TOLERANCE, format!("{name}: ship {err:.2} m from the drive's end point on the tick after {end_ev:?} (tolerance {END_TOLERANCE} m)"));
            check(c, c.v["min_clear"] > 0.0, format!("{name}: stayed {:.0} m outside every obstruction radius (sampled per tick)", c.v["min_clear"]));
            let line = format!(
                "{name}: flight {dur:.1} s (guide value 30 s), spool+calibration {:.1} s, top speed set {:.0} km/s, largest step {:.0} km per tick, walker depenetrations {:.0}",
                t_ramp - at(Phase::Spooling).unwrap_or(0.0),
                cfg.top_speed / 1000.0,
                moved / 1000.0,
                w.resource::<WalkStats>().depenetrations as f64 - c.v["depen0"]
            );
            println!("{line}");
            c.report.push(line);
            if how == Flight::Emergency {
                let t_drop = at(Phase::EmergencyDrop).unwrap_or(f64::NAN);
                let speed = c.v["end_speed"];
                let mut open = true;
                let mut nearest = f64::MAX;
                for p in &sys.planets {
                    let d = pos.distance(p.centre());
                    nearest = nearest.min(d);
                    open &= d > p.keep_out();
                }
                let frame = sys.frame_of(pos);
                check(c, end_ev == Event::DroppedOut && !ev.iter().any(|(_, e)| *e == Event::Arrived), format!("{name}: dropped out, did not arrive"));
                check(
                    c,
                    open && frame.is_none() && (speed - cfg.exit_speed).abs() < 50.0,
                    format!(
                        "{name}: in open space after the drop: nearest planet centre {:.0} km away (keep-out {:.1} km), in no frame zone ({frame:?}), {speed:.0} m/s (exit speed {:.0}); drop took {:.2} s (set {:.1} s) from {:.0} km into the path",
                        nearest / 1000.0,
                        sys.planets[0].keep_out() / 1000.0,
                        cfg.exit_speed,
                        t_arr - t_drop,
                        cfg.emergency_drop_time,
                        pos.distance(sys.planet(HEARTH).centre()) / 1000.0
                    ),
                );
            } else {
                let p = sys.planet(to);
                let alt = pos.distance(p.centre()) - p.radius;
                check(c, end_ev == Event::Arrived, format!("{name}: arrived at the exit point"));
                check(c, planet_id == to, format!("{name}: simulation's planet is now {planet_id} ({})", p.name));
                check(
                    c,
                    c.v["nose"] < NOSE_TOLERANCE && alt > p.min_jump_altitude(),
                    format!("{name}: at the exit the nose is {:.3} deg off {}'s centre (tolerance {NOSE_TOLERANCE} deg), {:.0} m from the centre, {alt:.0} m above the radius ({:.0} m above the atmosphere top)", c.v["nose"], p.name, pos.distance(p.centre()), alt - p.atmosphere_height),
                );
                check(c, gen_ms >= 0.0 && gen_ms < dur * 1000.0, format!("{name}: target generated in {gen_ms:.0} ms in the background during {dur:.1} s of flight"));
                if windowed {
                    let n = c.v.get("terrain_n").copied().unwrap_or(0.0);
                    check(c, c.v.get("terrain_ok") == Some(&1.0), format!("{name}: terrain of {} drawn 2 s after the exit ({n:.0} chunks visible)", p.name));
                }
            }
            if how == Flight::Passenger {
                // Deck contact from the ramp-up to the arrival tick, and the
                // 2.5 s after it (the pilot has left the seat; the ship is handed over at 400 m/s).
                let st = w.resource::<WalkStats>();
                let (g, s0) = (c.v["g_end"] - c.v["g0"], c.v["s_end"] - c.v["s0"]);
                let (ga, sa) = (st.grounded as f64 - c.v["g_end"], st.steps as f64 - c.v["s_end"]);
                let in_cabin = with_player(w, |p| p.ship.is_some());
                let drift = c.v.get("drift").copied().unwrap_or(f64::NAN);
                check(c, in_cabin && g / s0.max(1.0) > 0.99 && drift < 0.05, format!("{name}: walker in the cabin through the warp: deck contact {:.1} % of {s0:.0} steps, drift {:.1} mm; after the arrival {:.1} % of {sa:.0} steps", 100.0 * g / s0.max(1.0), drift * 1000.0, 100.0 * ga / sa.max(1.0)));
            }
            c.v.remove("started");
            return true;
        }
        false
    })
}

fn tel_reset_move(w: &mut World) {
    w.resource_mut::<WarpTelemetry>().max_tick_move = 0.0;
}

fn ship_frame_of_ro(w: &World) -> DVec3 {
    let mut q = w.try_query_filtered::<&Position, With<Ship>>().expect("ship query");
    q.single(w).map(|p| p.0).unwrap_or_default()
}

/// Wait until the drive is idle again (cooldown over).
fn wait_drive_idle() -> Step {
    Box::new(|w, c| warp_state(w).0 == Phase::Idle || c.t > 30.0)
}

/// A start refused below the jump altitude, or allowed above it (then cancelled).
fn start_at_altitude(name: &'static str, alt: f64, refused: bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            let pl = planet(w);
            teleport_ship(w, pl.centre + DVec3::Y * (pl.radius + alt), DQuat::IDENTITY);
            w.resource_mut::<WarpDrive>().last_abort = None;
        }
        if c.t > 0.2 && !c.v.contains_key("alt_tapped") {
            c.v.insert("alt_tapped", 1.0);
            tap(w, KeyCode::KeyJ);
        }
        if c.t >= 0.5 {
            c.v.remove("alt_tapped");
            let (phase, why) = warp_state(w);
            let limit = {
                let sys = &w.resource::<SystemRes>().0;
                sys.planet(w.resource::<PlanetRes>().id).min_jump_altitude()
            };
            if refused {
                check(c, phase == Phase::Idle && why == Some(Abort::TooLow), format!("start refused at {alt:.0} m (jump altitude {limit:.0} m = 1.5 x atmosphere): {phase:?}, {why:?}"));
            } else {
                check(c, phase == Phase::Spooling, format!("start allowed at {alt:.0} m (jump altitude {limit:.0} m): {phase:?}, {why:?}"));
                tap(w, KeyCode::KeyJ);
            }
            w.resource_mut::<WarpDrive>().last_abort = None;
            return true;
        }
        false
    })
}

fn warp_steps(s: &mut Vec<Step>, dir: &std::path::Path, windowed: bool) {
    // Seat by test shortcut, hover in the field.
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(Box::new(|w, c| {
        let sys = w.resource::<SystemRes>().0.clone();
        // The obstruction radius must hold the highest terrain, the arrival radius must be
        // above the jump altitude, frame zones below half the distance.
        for (id, def) in sys.ids().zip(&sys.planets) {
            let pl = PlanetRes::load(id, def);
            let high = pl.radius + pl.relief;
            check(
                c,
                high < def.obstruction_radius && def.radius + def.min_jump_altitude() < def.arrival_radius,
                format!("{}: highest terrain at {high:.0} m from the centre, obstruction radius {:.0} m, jump altitude {:.0} m, arrival radius {:.0} m", def.name, def.obstruction_radius, def.radius + def.min_jump_altitude(), def.arrival_radius),
            );
        }
        let d = sys.planets[0].centre().distance(sys.planets[1].centre());
        check(c, sys.planets.iter().all(|p| p.frame_radius < d * 0.5), format!("frame zones {:.0} km below half the distance ({:.0} km)", sys.planets[0].frame_radius / 1000.0, d / 2000.0));
        true
    }));
    // A snapshot with a planet id the system does not know is dropped, no panic.
    s.push(Box::new(|w, c| {
        use net_core::snapshot::FrameKind;
        let sys = w.resource::<SystemRes>().0.clone();
        let lim = crate::net_live::limits(&sys);
        let centres: Vec<DVec3> = sys.planets.iter().map(|p| p.centre()).collect();
        let mut s = Snapshot::new(2, 1.0, DVec3::new(0.0, 7000.0, 0.0), DVec3::ZERO, DQuat::IDENTITY);
        s.frame = FrameKind::Ship;
        s.frame_id = 2;
        s.wp = DVec3::new(0.0, 0.3, -2.0);
        let mut ok = true;
        for bad in [2u32, 7, 255, u32::MAX] {
            s.planet = bad;
            let mut r = Snapshot::decode(&s.encode()).expect("decodes");
            ok &= !lim.admits(&r) && !r.to_frame_of(&centres, 0) && sys.id(bad).is_none();
        }
        s.planet = 1;
        let r = Snapshot::decode(&s.encode()).unwrap();
        check(c, ok && lim.admits(&r), format!("snapshots with planet ids 2, 7, 255, 2^32-1 dropped without a panic, planet 1 admitted (limits {lim:?})"));
        true
    }));
    // Refused below 1.5 x the atmosphere height, also just above the atmosphere.
    s.push(start_at_altitude("start refused: inside the atmosphere", 600.0, true));
    s.push(start_at_altitude("start refused: above the atmosphere, below 1.5 x", 1300.0, true));
    s.push(start_at_altitude("start refused: just below the jump altitude", 1790.0, true));
    s.push(start_at_altitude("start allowed: just above the jump altitude", 1810.0, false));
    s.push(wait(0.5));
    // Refused: another ship on the path.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "start refused: ship on the path");
            let (p, r) = orbit_pose(w, HEARTH, CINDER);
            teleport_ship(w, p, r);
            // A ship-sized sphere at the middle of the course.
            let sys = w.resource::<SystemRes>().0.clone();
            let view = warp_core::ShipView { pos: p, forward: DVec3::X, speed: 0.0 };
            let mut d = Drive::new(sys.drive.clone());
            d.begin(CINDER, &view, &sys, &[]).unwrap();
            let path = d.path().unwrap();
            let (mid, _) = path.at(path.length() * 0.5);
            w.resource_mut::<WarpTelemetry>().extra_obstacles.push(Obstacle { centre: mid, radius: 20.0 });
        }
        if c.t > 0.2 && c.t < 0.22 {
            tap(w, KeyCode::KeyJ);
        }
        if c.t >= 0.5 {
            let (phase, why) = warp_state(w);
            check(c, phase == Phase::Idle && matches!(why, Some(Abort::Obstructed(_))), format!("start refused with a ship on the path ({phase:?}, {why:?})"));
            w.resource_mut::<WarpTelemetry>().extra_obstacles.clear();
            w.resource_mut::<WarpDrive>().last_abort = None;
            return true;
        }
        false
    }));
    // Aborted: the aim drifts 20 degrees away in the middle of the calibration.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "calibration lost");
            for k in ["tapped", "tapped2", "turned"] {
                c.v.remove(k);
            }
            let (p, r) = orbit_pose(w, HEARTH, CINDER);
            teleport_ship(w, p, r);
        }
        if c.t > 0.2 && !c.v.contains_key("tapped") {
            c.v.insert("tapped", 1.0);
            tap(w, KeyCode::KeyJ);
            return false;
        }
        let (phase, why) = warp_state(w);
        let gauge = w.resource::<WarpDrive>().drive.gauge;
        if phase == Phase::Calibrating && gauge > 0.4 && !c.v.contains_key("turned") {
            c.v.insert("turned", gauge);
            let e = ship_e(w);
            let rot = w.get::<Rotation>(e).unwrap().0;
            w.get_mut::<Rotation>(e).unwrap().0 = DQuat::from_rotation_y(20f64.to_radians()) * rot;
        } else if !c.v.contains_key("turned") {
            hold_course(w);
        }
        if (phase == Phase::Idle && c.t > 0.5) || c.t > 30.0 {
            check(c, phase == Phase::Idle && why == Some(Abort::CalibrationLost), format!("aim lost at gauge {:.0} %: aborted ({phase:?}, {why:?})", c.v.get("turned").copied().unwrap_or(0.0) * 100.0));
            w.resource_mut::<WarpDrive>().last_abort = None;
            c.v.remove("tapped");
            return true;
        }
        false
    }));
    // Cancelled while spooling.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "cancel while spooling");
            for k in ["tapped", "tapped2"] {
                c.v.remove(k);
            }
            let (p, r) = orbit_pose(w, HEARTH, CINDER);
            teleport_ship(w, p, r);
        }
        if c.t > 0.2 && !c.v.contains_key("tapped") {
            c.v.insert("tapped", 1.0);
            tap(w, KeyCode::KeyJ);
        }
        if c.t > 1.5 && !c.v.contains_key("tapped2") {
            c.v.insert("tapped2", 1.0);
            tap(w, KeyCode::KeyJ);
        }
        if c.t >= 2.0 {
            let (phase, why) = warp_state(w);
            check(c, phase == Phase::Idle && why == Some(Abort::Cancelled), format!("cancelled while spooling ({phase:?}, {why:?})"));
            w.resource_mut::<WarpDrive>().last_abort = None;
            c.v.remove("tapped");
            return true;
        }
        false
    }));
    s.push(warp_flight("warp Hearth -> Cinder (walker in the cabin)", "a2b", Some(HEARTH), CINDER, Flight::Passenger, dir.to_path_buf(), windowed));
    s.push(wait_drive_idle());
    // Land on Cinder: ship on the ground, walker standing next to it.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "land on Cinder");
            let pl = planet(w);
            let dir = DVec3::Y;
            let rot = crate::ship::basis_for_up(dir);
            teleport_ship(w, pl.centre + dir * (pl.surface(dir) + 5.0), rot);
            let p = pl.centre + dir * pl.surface(dir);
            place_walker(w, p + DVec3::new(10.0, 0.0, 0.0));
        }
        if c.t >= 8.0 {
            let (agl, v) = (above_ground(w), ship_vel(w).length());
            let (g, pid) = (with_player(w, |p| p.w.grounded), w.resource::<PlanetRes>().id);
            check(c, pid == CINDER && g && v < 1.0, format!("on Cinder: planet {pid}, walker grounded {g}, ship {agl:.1} m above ground at {v:.2} m/s, walker depenetrations so far {}", w.resource::<WalkStats>().depenetrations));
            return true;
        }
        false
    }));
    s.extend(back_to_seat());
    s.push(warp_flight("warp Cinder -> Hearth (seated, view from outside)", "b2a", Some(CINDER), HEARTH, Flight::Seated, dir.to_path_buf(), windowed));
    s.push(wait_drive_idle());
    s.push(warp_flight("emergency exit Hearth -> Cinder at mid-flight", "emergency", Some(HEARTH), CINDER, Flight::Emergency, dir.to_path_buf(), windowed));
    s.push(wait_drive_idle());
    // From open space on to Cinder: the effective target is Cinder, the path starts straight.
    s.push(warp_flight("warp from the drop point on to Cinder", "drop2b", None, CINDER, Flight::Seated, dir.to_path_buf(), windowed));
    s.push(wait(1.0));
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
                put_at_seat(w);
                true
            }));
            s.extend(sit());
            for _ in 0..60 {
                s.extend(net_cycle());
            }
        }
        "foreign" => foreign_steps(&mut s),
        // Sprint 2 feel: input ramp, virtual-joystick mouse, boost, decoupled (#24, #25, #26).
        "flight" => flight_steps(&mut s, &shot_step, out_dir, windowed),
        // #21: edit a tuning file while running (dev builds).
        "reload" => reload_steps(&mut s, out_dir),
        "warp" => warp_steps(&mut s, out_dir, windowed),
        // Issue #5: step out of the ship in space (seat by test shortcut, then fly up).
        "space" => {
            s.push(Box::new(|w, _| {
                put_at_seat(w);
                true
            }));
            s.extend(sit());
            s.extend(fly_to_space_and_back());
            s.push(hold_until("firm brake in space", &[KeyCode::KeyX], 30.0, |w| ship_vel(w).length() < 0.5));
            s.push(aim("nose up like the climb", 30.0, 3.0));
            s.extend(step_out_in_space("in space: walk out of the stopped ship", 0.0, false));
            s.extend(step_out_in_space("in space: walk out of a ship drifting at 3 m/s", 3.0, false));
            s.push(hold_until("firm brake in space", &[KeyCode::KeyX], 30.0, |w| ship_vel(w).length() < 0.01));
            s.extend(step_out_in_space("in space: step out carefully (tap W)", 0.0, true));
            s.push(hold_until("firm brake in space", &[KeyCode::KeyX], 30.0, |w| ship_vel(w).length() < 0.01));
            s.extend(suit_in_space());
            s.extend(drift_behind_moving_ship());
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
            // Until well below the check's 0.05 m/s: a ship that touched down on a slope slides
            // and settles slowly, and ending at the check's own threshold made it a coin toss.
            s.push(hold_until("land", &[KeyCode::ControlLeft], 60.0, {
                let mut t = 0.0;
                move |w| {
                    t += 1.0 / 60.0;
                    t > 3.0 && ship_vel(w).length() < 0.02
                }
            }));
            s.push(Box::new(|w, c| {
                let (v, agl) = (ship_vel(w).length(), above_ground(w));
                check(c, v < 0.05 && agl < 1.0, format!("landed: {v:.3} m/s, {agl:.2} m above ground"));
                true
            }));
            s.push(shot_step("landed"));
            s.extend(stand_still("idle 5 s (landed ship)"));
            s.push(Box::new(|w, c| {
                let lag = with_ship(w, |s| s.lag);
                check(c, lag.landed && lag.level == 0.0, format!("cabin gravity off after landing ({:.0} %)", lag.level * 100.0));
                true
            }));
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
                    check_steady(c, "leaving the cabin");
                    return true;
                }
                false
            }));
            s.push(board("walk back in to the seat", false));
            s.extend(lag_by_hand());
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

/// Scenarios written for the direct mouse (`aim`, the net bot's turn: pixels at 0.002 rad) keep
/// it; `flight` uses the shipped default, the virtual joystick.
pub fn uses_direct_mouse(name: &str) -> bool {
    name != "flight"
}

/// Hold keys from rest and measure how long the ramped input takes to reach full deflection
/// (`ShipController::ramp.out[axis]`), against the tuning value.
fn ramp_check(name: &'static str, ks: &'static [KeyCode], axis: usize, want: fn(&flight_core::ShipTuning) -> f64) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            keys(w, ks, true);
            return false;
        }
        let out = with_ship(w, |s| s.ctl.ramp.out[axis]);
        if out.abs() >= 1.0 - 1e-6 || c.t > 3.0 {
            keys(w, ks, false);
            let want = want(&w.resource::<crate::tuning::Tuning>().ship);
            end(w, c, format!("full after {:.3} s (tuning {want:.3} s)", c.t));
            check(c, (c.t - want).abs() <= c.dt * 1.01, format!("{name}: full deflection after {:.3} s, tuning {want:.3} s", c.t));
            return true;
        }
        false
    })
}

/// Move the virtual stick to an angle (radians, mouse axes) by mouse pixels through `Controls`.
fn stick_to(w: &mut World, target: DVec2) {
    let off = with_ship(w, |s| s.stick.offset);
    let sens = w.resource::<Bindings>().mouse.ship_sensitivity;
    let d = (target - off) / sens;
    w.resource_mut::<Controls>().mouse += Vec2::new(d.x as f32, d.y as f32);
}

/// Yaw rate of the ship about its own up (rad/s, left positive), without the horizon follow.
fn yaw_rate(w: &mut World) -> f64 {
    let e = ship_e(w);
    let (r, av) = (w.get::<Rotation>(e).unwrap().0, w.get::<AngularVelocity>(e).unwrap().0);
    av.dot(r * DVec3::Y)
}

/// Stick right by a share of its travel past the dead zone, hold, check the yaw rate is that
/// share of the turn rate; 0 centres the stick.
fn stick_yaw(name: &'static str, share: f64) -> Step {
    Box::new(move |w, c| {
        let (dz, max) = {
            let m = &w.resource::<Bindings>().mouse;
            (m.vjoy_deadzone, m.vjoy_max_angle)
        };
        if c.t == 0.0 {
            begin(w, c, name);
            let angle = if share == 0.0 { 0.0 } else { dz + share * (max - dz) };
            stick_to(w, DVec2::new(angle, 0.0));
        }
        if c.t >= 1.5 {
            let rate = yaw_rate(w);
            let want = -share * w.resource::<crate::tuning::Tuning>().ship.turn_rate;
            end(w, c, format!("yaw rate {rate:+.3} rad/s, wanted {want:+.3}"));
            check(c, (rate - want).abs() <= 0.05 * want.abs().max(1.0), format!("{name}: yaw {rate:+.3} rad/s for {share} of the stick (wanted {want:+.3})"));
            return true;
        }
        false
    })
}

fn flight_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step, out_dir: &std::path::Path, windowed: bool) {
    let dir = out_dir.to_path_buf();
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    s.push(hold_until("climb to 300 m above ground", &[KeyCode::Space, KeyCode::ShiftLeft], 60.0, |w| above_ground(w) > 300.0));
    s.push(hold_until("hover", &[], 10.0, |w| ship_vel(w).length() < 0.5));
    // #25: thrust and rotation ramp to full deflection.
    s.push(ramp_check("ramp: W to full thrust", &[KeyCode::KeyW], 2, |t| t.linear_ramp_time));
    s.push(hold_until("hover", &[], 10.0, |w| ship_vel(w).length() < 0.5));
    s.push(Box::new(|w, c| {
        // Stick to full right: the turn ramps like any rotation.
        if c.t == 0.0 {
            begin(w, c, "ramp: stick to full yaw");
            let max = w.resource::<Bindings>().mouse.vjoy_max_angle;
            stick_to(w, DVec2::new(max * 2.0, 0.0));
            return false;
        }
        let out = with_ship(w, |s| s.ctl.ramp.out[5]);
        if out.abs() >= 1.0 - 1e-6 || c.t > 3.0 {
            let want = w.resource::<crate::tuning::Tuning>().ship.angular_ramp_time;
            end(w, c, format!("full after {:.3} s (tuning {want:.3} s)", c.t));
            check(c, (c.t - want).abs() <= c.dt * 1.01, format!("ramp: stick to full yaw after {:.3} s, tuning {want:.3} s", c.t));
            return true;
        }
        false
    }));
    // Virtual joystick: yaw rate is deflection times turn rate; inside the dead zone nothing.
    s.push(stick_yaw("stick: full right", 1.0));
    s.push(shot_step("stick-full-right"));
    s.push(Box::new(|w, c| {
        // Still at full right: the view leads the turn to the right, capped (#27).
        let look = w.resource::<crate::ship::CameraEffects>().0.look;
        let max = w.resource::<crate::tuning::Tuning>().camera.look_ahead_max_yaw_deg.to_radians();
        check(c, (look.y + max).abs() < 0.01 * max && look.x.abs() < 0.01, format!("camera: look-ahead {:+.2} deg yaw in a full right turn (cap {:.0})", look.y.to_degrees(), max.to_degrees()));
        true
    }));
    s.push(stick_yaw("stick: half right", 0.5));
    s.push(stick_yaw("stick: centred", 0.0));
    s.push(Box::new(|w, c| {
        let dz = w.resource::<Bindings>().mouse.vjoy_deadzone;
        if c.t == 0.0 {
            begin(w, c, "stick: inside the dead zone");
            stick_to(w, DVec2::new(dz * 0.8, 0.0));
        }
        if c.t >= 1.0 {
            let rate = yaw_rate(w);
            end(w, c, format!("yaw rate {rate:+.4} rad/s"));
            check(c, rate.abs() < 0.01, format!("stick: inside the dead zone the ship does not turn ({rate:+.4} rad/s)"));
            stick_to(w, DVec2::ZERO);
            return true;
        }
        false
    }));
    // #29: the pad's right stick through the same axes, without a device.
    s.push(Box::new(|w, c| {
        let stick = 0.8f32;
        if c.t == 0.0 {
            begin(w, c, "pad: right stick at 0.8");
            w.resource_mut::<Controls>().pad_axes.insert(GamepadAxis::RightStickX, stick);
        }
        if c.t >= 1.5 {
            w.resource_mut::<Controls>().pad_axes.clear();
            let shaped = w.resource::<Bindings>().axis(crate::controls::Axis::TurnYaw).shape(stick as f64);
            let want = -shaped * w.resource::<crate::tuning::Tuning>().ship.turn_rate;
            let rate = yaw_rate(w);
            end(w, c, format!("yaw rate {rate:+.3} rad/s, wanted {want:+.3}"));
            check(c, (rate - want).abs() <= 0.05 * want.abs(), format!("pad: stick 0.8 right yaws {rate:+.3} rad/s (dead zone and curve: {want:+.3})"));
            return true;
        }
        false
    }));
    s.push(stick_yaw("stick: centred", 0.0));
    // #24: boost is a speed stage; it raises the limit and drops back on release.
    s.push(hold_until("cruise", &[KeyCode::KeyW], 6.0, |_| false));
    {
        let dir = dir.clone();
        s.push(Box::new(move |w, c| {
        let (limit, v) = (with_ship(w, |s| s.ctl.forward_speed_limit), ship_vel(w).length());
        if c.t == 0.0 {
            begin(w, c, "boost");
            c.v.insert("limit0", limit);
            c.v.insert("v0", v);
            keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], true);
        }
        if c.t >= 6.0 {
            // Screenshot while boost is still held, so the HUD shows it.
            shot(w, c, &dir, windowed, "boost");
            keys(w, &[KeyCode::ShiftLeft], false);
            let (l0, v0) = (c.v["limit0"], c.v["v0"]);
            c.v.insert("limit1", limit);
            c.v.insert("v1", v);
            end(w, c, format!("limit {l0:.0} -> {limit:.0} m/s, speed {v0:.0} -> {v:.0} m/s"));
            check(c, limit > 1.5 * l0 && v > v0 + 20.0, format!("boost: limit {l0:.0} -> {limit:.0} m/s, speed {v0:.0} -> {v:.0} m/s"));
            let (fov, base) = (w.resource::<crate::ship::CameraEffects>().0.fov_deg, w.resource::<crate::tuning::Tuning>().camera.fov_curve.eval(0.0));
            check(c, fov > base + 1.5, format!("camera: field of view {fov:.1} deg at {v:.0} m/s (at rest {base:.0})"));
            return true;
        }
        false
    }));
    }
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "boost released");
        }
        if c.t >= 8.0 {
            keys(w, &[KeyCode::KeyW], false);
            let (limit, v) = (with_ship(w, |s| s.ctl.forward_speed_limit), ship_vel(w).length());
            let (l0, l1, v1) = (c.v["limit0"], c.v["limit1"], c.v["v1"]);
            end(w, c, format!("limit {l1:.0} -> {limit:.0} m/s, speed {v1:.0} -> {v:.0} m/s"));
            check(c, limit < 0.6 * l1 && (limit - l0).abs() < 0.3 * l0 && v < v1 - 20.0, format!("boost released: limit {l1:.0} -> {limit:.0} m/s (before {l0:.0}), speed {v1:.0} -> {v:.0} m/s"));
            return true;
        }
        false
    }));
    s.push(hold_until("firm brake", &[KeyCode::KeyX], 15.0, |w| ship_vel(w).length() < 0.5));
    // #26: decoupled blends the damping out over decouple_time; the ship keeps gliding.
    s.push(hold_until("cruise", &[KeyCode::KeyW], 4.0, |_| false));
    s.push(Box::new(|w, c| {
        let time = w.resource::<crate::tuning::Tuning>().ship.decouple_time;
        if c.t == 0.0 {
            begin(w, c, "decouple (C) while cruising");
            keys(w, &[KeyCode::KeyW], true);
            tap(w, KeyCode::KeyC);
            return false;
        }
        let level = with_ship(w, |s| s.ctl.coupling);
        if (c.t - time * 0.5).abs() < c.dt * 0.5 {
            check(c, (level - 0.5).abs() < 0.02, format!("decouple: coupling {level:.3} halfway through the blend"));
        }
        if c.t >= time + 0.2 {
            keys(w, &[KeyCode::KeyW], false);
            c.v.insert("v_release", ship_vel(w).length());
            end(w, c, format!("coupling {level:.3} after {:.1} s", c.t));
            check(c, level == 0.0, format!("decouple: coupling {level:.3} after {:.1} s (blend {time} s)", c.t));
            return true;
        }
        false
    }));
    {
        let dir = dir.clone();
        s.push(Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, "decoupled glide, no input");
        }
        if (c.t - 2.0).abs() < c.dt * 0.5 {
            shot(w, c, &dir, windowed, "decoupled");
        }
        if c.t >= 3.0 {
            let (v0, v) = (c.v["v_release"], ship_vel(w).length());
            end(w, c, format!("speed {v0:.1} -> {v:.1} m/s"));
            check(c, v > 0.97 * v0, format!("decoupled: keeps gliding without input, {v0:.1} -> {v:.1} m/s in 3 s"));
            tap(w, KeyCode::KeyC);
            return true;
        }
        false
    }));
    }
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "coupled again, no input");
            c.v.insert("v_couple", ship_vel(w).length());
        }
        if c.t >= 8.0 {
            let (v0, v) = (c.v["v_couple"], ship_vel(w).length());
            let level = with_ship(w, |s| s.ctl.coupling);
            end(w, c, format!("speed {v0:.1} -> {v:.1} m/s, coupling {level:.2}"));
            check(c, level == 1.0 && v < 0.7 * v0, format!("coupled again: the assist damps, {v0:.1} -> {v:.1} m/s in 8 s"));
            return true;
        }
        false
    }));
    s.push(hold_until("firm brake", &[KeyCode::KeyX], 15.0, |w| ship_vel(w).length() < 0.5));
    // Touchdown gives a camera bump (#27); on a slope the second side may give another.
    s.push(Box::new(|w, c| {
        c.v.insert("bumps0", w.resource::<crate::ship::CameraEffects>().0.bumps as f64);
        true
    }));
    s.push(hold_until("land", &[KeyCode::ControlLeft], 90.0, {
        let mut t = 0.0;
        move |w| {
            t += 1.0 / 60.0;
            t > 3.0 && ship_vel(w).length() < 0.02
        }
    }));
    s.push(Box::new(|w, c| {
        let bumps = w.resource::<crate::ship::CameraEffects>().0.bumps as f64 - c.v["bumps0"];
        check(c, (1.0..=2.0).contains(&bumps), format!("camera: {bumps} touchdown bump(s) on landing"));
        true
    }));
    s.push(shot_step("landed"));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::F3);
        true
    }));
    s.push(wait(0.3));
    s.push(shot_step("debug-hud-f3"));
    // Screenshots are written a few frames later.
    s.push(wait(1.0));
}

/// Copies the shipped tuning files to `<out>/tuning`, watches that copy, edits it mid-run.
fn reload_steps(s: &mut Vec<Step>, out_dir: &std::path::Path) {
    let dir = out_dir.join("tuning");
    let ship_file = dir.join("ship.json");
    let ship_text = crate::tuning::SHIP.to_string();
    {
        let dir = dir.clone();
        s.push(Box::new(move |w, _| {
            std::fs::create_dir_all(&dir).expect("tuning copy dir");
            for (f, t) in [("ship.json", crate::tuning::SHIP), ("walker.json", crate::tuning::WALKER), ("suit.json", crate::tuning::SUIT), ("camera.json", crate::tuning::CAMERA), ("bindings.json", crate::controls::BINDINGS)] {
                std::fs::write(dir.join(f), t).expect("tuning copy");
            }
            w.resource_mut::<crate::hot_reload::HotReload>().dir = dir.clone();
            true
        }));
    }
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let n = w.resource::<crate::hot_reload::HotReload>().reloads;
        check(c, n == 0, format!("reload: unchanged files change nothing ({n} reloads)"));
        true
    }));
    let edit = |name: &'static str, file: std::path::PathBuf, text: String, then: fn(&mut World) -> Option<String>| -> Step {
        Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, name);
                std::fs::write(&file, &text).expect("edit tuning");
            }
            if let Some(note) = then(w) {
                end(w, c, format!("after {:.2} s simulated", c.t));
                check(c, true, format!("{name}: {note}"));
                return true;
            }
            if c.t > 30.0 {
                end(w, c, "timed out".into());
                check(c, false, format!("{name}: no effect within 30 s"));
                return true;
            }
            false
        })
    };
    let slower = ship_text.replacen("\"turn_rate\": 2.5", "\"turn_rate\": 1.25", 1);
    assert_ne!(slower, ship_text, "fixture: turn_rate in ship.json");
    s.push(edit("reload: ship.json turn_rate 2.5 -> 1.25", ship_file.clone(), slower, |w| {
        let (ship, res) = (with_ship(w, |s| s.ctl.tuning.turn_rate), w.resource::<crate::tuning::Tuning>().ship.turn_rate);
        (ship == 1.25 && res == 1.25).then(|| format!("the ship turns at {ship} rad/s now"))
    }));
    let broken = ship_text.replacen("\"drag_k\"", "\"drag_kk\": 1, \"drag_k\"", 1);
    s.push(edit("reload: a broken ship.json is refused", ship_file.clone(), broken, |w| {
        let hr = w.resource::<crate::hot_reload::HotReload>();
        let err = hr.last_error.clone()?;
        let rate = with_ship(w, |s| s.ctl.tuning.turn_rate);
        (rate == 1.25 && err.contains("drag_kk")).then(|| format!("old value {rate} stays, error: {err}"))
    }));
    s.push(edit("reload: ship.json restored", ship_file, ship_text, |w| {
        let rate = with_ship(w, |s| s.ctl.tuning.turn_rate);
        (rate == 2.5 && w.resource::<crate::hot_reload::HotReload>().last_error.is_none()).then(|| format!("turn rate {rate} again"))
    }));
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
            if let Some(perf) = w.get_resource::<crate::perf::Perf>() {
                for (ok, line) in crate::perf::finish(perf, &sc.name, &sc.out_dir) {
                    check(&mut sc.ctx, ok, line);
                }
            }
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

