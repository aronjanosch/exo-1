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
    /// #16: the received velocity is replaced by this one (a ship at warp speed held in place).
    pub hold_vel: Option<DVec3>,
    /// #16: the ship warps along its nose (wins over `parked`).
    pub warp: Option<ForeignWarp>,
}

/// A remote ship's warp for the `foreign` scenario: still at `p0` until `t0`, then the drive's
/// two acceleration stages up to its top speed, then cruise (`content/system/system.json`).
#[derive(Clone, Copy)]
pub struct ForeignWarp {
    pub t0: f64,
    pub p0: DVec3,
    pub q: DQuat,
    pub accel_one: f64,
    pub switch_speed: f64,
    pub accel_two: f64,
    pub top_speed: f64,
}

impl ForeignWarp {
    /// Distance along the nose and speed at time `t`.
    fn at(&self, t: f64) -> (f64, f64) {
        let tau = (t - self.t0).max(0.0);
        let (a1, vs, a2, vt) = (self.accel_one, self.switch_speed, self.accel_two, self.top_speed);
        let (t1, t2) = (vs / a1, (vt - vs) / a2);
        let (s1, s2) = (0.5 * a1 * t1 * t1, vs * t2 + 0.5 * a2 * t2 * t2);
        if tau < t1 {
            (0.5 * a1 * tau * tau, a1 * tau)
        } else if tau < t1 + t2 {
            let u = tau - t1;
            (s1 + vs * u + 0.5 * a2 * u * u, vs + a2 * u)
        } else {
            (s1 + s2 + vt * (tau - t1 - t2), vt)
        }
    }

    /// Seconds from `t0` to the top speed.
    fn ramp_time(&self) -> f64 {
        self.switch_speed / self.accel_one + (self.top_speed - self.switch_speed) / self.accel_two
    }
}

const FOREIGN_SPEED: f64 = 350.0;
const FOREIGN_YAW_RATE: f64 = 0.003 * 60.0;

fn foreign_truth(d: &ForeignDriver, t: f64) -> (DVec3, bevy::math::DQuat, DVec3) {
    if let Some(wp) = d.warp {
        let nose = wp.q * DVec3::NEG_Z;
        let (s, v) = wp.at(t);
        return (wp.p0 + nose * s, wp.q, nose * v);
    }
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
        if d.warp.is_some() {
            // The owner sits in its own ship, as a real pilot sends it (the default walker pose is
            // planet-relative and would be out of the walker range at warp speed).
            s.frame = net_core::snapshot::FrameKind::Ship;
            s.frame_id = 2;
            s.wp = SEAT_POS;
            s.wv = DVec3::ZERO;
        }
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
    v.0 = d.hold_vel.unwrap_or(sample.s.v);
    w.0 = if d.parked.is_some() || d.warp.is_some() { DVec3::ZERO } else { DVec3::new(0.0, FOREIGN_YAW_RATE, 0.0) };
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
pub(crate) fn tap(w: &mut World, k: KeyCode) {
    w.resource_mut::<Controls>().taps.push(k);
}
pub(crate) fn planet(w: &World) -> PlanetRes {
    w.resource::<PlanetRes>().clone()
}
pub(crate) fn ship_e(w: &mut World) -> Entity {
    w.query_filtered::<Entity, With<Ship>>().single(w).unwrap()
}
pub(crate) fn ship_frame_of(w: &mut World) -> Frame {
    let e = ship_e(w);
    let (p, r) = (w.get::<Position>(e).unwrap(), w.get::<Rotation>(e).unwrap());
    ship_frame(p, r)
}
pub(crate) fn ship_vel(w: &mut World) -> DVec3 {
    let e = ship_e(w);
    w.get::<LinearVelocity>(e).unwrap().0
}
pub(crate) fn with_ship<R>(w: &mut World, f: impl FnOnce(&mut Ship) -> R) -> R {
    let e = ship_e(w);
    f(&mut w.get_mut::<Ship>(e).unwrap())
}
pub(crate) fn with_player<R>(w: &mut World, f: impl FnOnce(&mut Player) -> R) -> R {
    let mut q = w.query::<&mut Player>();
    let mut p = q.single_mut(w).unwrap();
    f(&mut p)
}
pub(crate) fn player_world(w: &mut World) -> DVec3 {
    let f = ship_frame_of(w);
    with_player(w, |p| p.world_pos(f))
}
/// Position of whatever the player controls (ship when seated).
pub(crate) fn active_pos(w: &mut World) -> DVec3 {
    if with_player(w, |p| p.seated) { ship_frame_of(w).origin } else { player_world(w) }
}
pub(crate) fn above_ground(w: &mut World) -> f64 {
    let p = active_pos(w);
    planet(w).above_ground(p)
}
pub(crate) fn altitude(w: &mut World) -> f64 {
    let p = active_pos(w);
    let pl = planet(w);
    (p - pl.centre).length() - pl.radius
}
/// Put the walker on the ground at a world point (test setup).
pub(crate) fn place_walker(w: &mut World, at: DVec3) {
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
pub(crate) fn face_towards(w: &mut World, target: DVec3) {
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

pub(crate) fn check(c: &mut Ctx, ok: bool, note: String) {
    let line = format!("{} {note}", if ok { "PASS" } else { "FAIL" });
    println!("{line}");
    c.report.push(line);
    c.failures += !ok as u32;
}

pub(crate) fn begin(w: &mut World, c: &mut Ctx, name: &str) {
    c.phase = name.to_string();
    c.stats0 = w.resource::<WalkStats>().clone();
    c.rescues0 = c.stats0.rescues;
    println!("PHASE '{name}' tick {}", c.ticks);
}

pub(crate) fn end(w: &mut World, c: &mut Ctx, note: String) {
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

pub(crate) fn wait(sec: f64) -> Step {
    Box::new(move |_, c| c.t >= sec)
}

pub(crate) fn settle() -> Step {
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
pub(crate) fn put_at_seat(w: &mut World) {
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

pub(crate) fn sit() -> Vec<Step> {
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
pub(crate) fn hold_until(name: &'static str, ks: &'static [KeyCode], limit: f64, mut done: impl FnMut(&mut World) -> bool + Send + Sync + 'static) -> Step {
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

/// Hold Ctrl until the ship rests (well below the landed check's 0.05 m/s). From the first hull
/// contact on it must not slide: pressed down onto a slope it used to slide 20 s. Tipping from the
/// first corner onto the slope moves the centre a little (0.3 m on the 14 degree slope of `full`).
pub(crate) fn land(name: &'static str) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            keys(w, &[KeyCode::ControlLeft], true);
            c.p.remove("touch");
        }
        let pos = ship_frame_of(w).origin;
        if with_ship(w, |s| s.grounded) && !c.p.contains_key("touch") {
            c.p.insert("touch", pos);
        }
        let v = ship_vel(w).length();
        if c.t > 3.0 && v < 0.02 || c.t >= 90.0 {
            keys(w, &[KeyCode::ControlLeft], false);
            let up = planet(w).up(pos);
            let slide = c.p.get("touch").map(|t| {
                let d = pos - *t;
                (d - up * d.dot(up)).length()
            });
            let agl = above_ground(w);
            end(w, c, format!("{:.1} s, ground {agl:.2} m, speed {v:.3} m/s, slid {:.3} m after touchdown", c.t, slide.unwrap_or(f64::NAN)));
            check(c, slide.is_some_and(|s| s < 0.5), format!("{name}: no slide after touchdown ({:.3} m)", slide.unwrap_or(f64::NAN)));
            return true;
        }
        false
    })
}

/// Point the nose like a player with the mouse: yaw/pitch rate proportional to the error,
/// at most the controller's turn rate. `elevation` is the wanted angle above the horizon.
pub(crate) fn aim(name: &'static str, elevation_deg: f64, secs: f64) -> Step {
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

pub(crate) fn fly_to_space_and_back() -> Vec<Step> {
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
    // The stamps are placed by the planet's budget (#69): starts come from its look spots.
    let sys = warp_core::System::from_json(crate::warp::SYSTEM).expect("system.json");
    let home = PlanetRes::load(PlanetId(0), sys.planet(PlanetId(0)));
    let pg = &home.pgen;
    let r = home.radius;
    let back = |s: planet_core::Spot, m: f64| crate::env::from_v3(planet_core::look::walk(s.dir, s.facing, m, r));
    let mut out = vec![("spawn, heading east", DVec3::Y, DVec3::X)];
    if let Some(s) = pg.spot("basin") {
        out.push(("basin shore, heading to the centre", crate::env::from_v3(s.dir), crate::env::from_v3(s.facing)));
    }
    // The rim spot stands on top facing down: start 300 m below it, heading up the step.
    if let Some(s) = pg.spot("rim") {
        let start = back(s, 300.0);
        out.push(("escarpment foot, heading up the step", start, (crate::env::from_v3(s.dir) - start).normalize()));
    }
    // The plateau spot is near its edge facing out: start 400 m outside, heading in.
    if let Some(s) = pg.spot("plateau") {
        let start = back(s, 400.0);
        out.push(("plateau approach, heading to the centre", start, (crate::env::from_v3(s.dir) - start).normalize()));
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
            let here = pl.pgen.sample(crate::env::to_v3(pl.up(p)));
            let slope = here.slope_deg;
            // Biome rows walked through (#68), as a bit set.
            let seen = c.v.entry("biomes").or_insert(0.0);
            *seen = (*seen as u64 | 1u64 << here.biome.clamp(0, 63)) as f64;
            let e = c.v.entry("slope").or_insert(0.0);
            *e = e.max(slope);
            let climb = c.v.entry("climb").or_insert(0.0);
            if slope > 50.0 { *climb += 1.0; }
            if c.t >= secs {
                keys(w, &[KeyCode::KeyW], false);
                let rows = (c.v["biomes"] as u64).count_ones();
                let note = format!("path {:.0} m, height above sea {:+.1}..{:+.1} m, steepest ground under the walker {:.1} deg, ticks on ground steeper than 50 deg {}, biome rows {rows}", c.v["path"], c.v["hmin"], c.v["hmax"], c.v["slope"], c.v["climb"]);
                c.v.remove("slope");
                c.v.remove("climb");
                c.v.remove("biomes");
                end(w, c, note);
                check(c, w.resource::<WalkStats>().rescues == c.rescues0, format!("{name}: no fall-through"));
                check(c, rows >= 2, format!("{name}: crosses {rows} biome rows (at least 2)"));
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
        w.insert_resource(ForeignDriver { proxy, buf: Buffer::new(), tick: 0, p0, parked: None, max_pos_err: 0.0, max_rot_err_deg: 0.0, hold_vel: None, warp: None });
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
    foreign_warp_extra_steps(s);
}

/// The same speed as `foreign_warp`: the drive's top speed (1e6 m/s).
const FOREIGN_WARP_SPEED: f64 = 1.0e6;

/// #16 without a network: a remote ship at warp speed next to the walking walker, then warping
/// with the walker in its cabin. Through the real snapshot path like the rest of `foreign`.
fn foreign_warp_extra_steps(s: &mut Vec<Step>) {
    // 7. The landed ship carries 1e6 m/s along its nose while it stands (a 17 km swept AABB and
    //    that relative velocity in the walker's moving-collider sweeps); the walker walks away
    //    from it outside.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "walk 5 s beside the landed foreign ship carrying 1e6 m/s");
            let proxy = w.resource::<ForeignDriver>().proxy;
            let (pp, pr) = (w.get::<Position>(proxy).unwrap().0, w.get::<Rotation>(proxy).unwrap().0);
            w.resource_mut::<ForeignDriver>().hold_vel = Some(pr * DVec3::NEG_Z * FOREIGN_WARP_SPEED);
            place_walker(w, pp + pr * DVec3::new(7.0, 0.0, 0.0));
            c.v.insert("ready", 0.0);
            return false;
        }
        // Land first (placed 0.1 m above the ground).
        if c.v["ready"] == 0.0 {
            if c.t < 1.0 {
                return false;
            }
            let proxy = w.resource::<ForeignDriver>().proxy;
            let pp = w.get::<Position>(proxy).unwrap().0;
            let p = player_world(w);
            face_towards(w, p + (p - pp));
            c.v.insert("ready", c.t);
            c.p.insert("start", p);
            c.p.insert("last", p);
            c.v.insert("depen0", w.resource::<WalkStats>().depenetrations as f64);
            c.v.insert("g", 0.0);
            c.v.insert("n", 0.0);
            c.v.insert("jump", 0.0);
            keys(w, &[KeyCode::KeyW], true);
            return false;
        }
        let p = player_world(w);
        *c.v.get_mut("jump").unwrap() = c.v["jump"].max(p.distance(c.p["last"]));
        c.p.insert("last", p);
        *c.v.get_mut("g").unwrap() += with_player(w, |pl| pl.w.grounded) as u32 as f64;
        *c.v.get_mut("n").unwrap() += 1.0;
        if c.t - c.v["ready"] >= 5.0 {
            keys(w, &[KeyCode::KeyW], false);
            let d = p.distance(c.p["start"]);
            let ws = w.resource::<WalkStats>().clone();
            let (dep, resc) = (ws.depenetrations as f64 - c.v["depen0"], ws.rescues - c.rescues0);
            let (g, n, jump) = (c.v["g"], c.v["n"], c.v["jump"]);
            let proxy = w.resource::<ForeignDriver>().proxy;
            let pv = w.get::<LinearVelocity>(proxy).unwrap().0.length();
            end(w, c, format!("walked {d:.2} m, grounded {g}/{n} ticks, largest step {:.3} m, {dep} depenetrations, foreign ship at {:.0} km/s", jump, pv / 1000.0));
            // Walk speed 5 m/s for 5 s, less the step-off (as in `foreign_warp`).
            check(c, d > 22.0 && d < 26.0 && g / n > 0.95 && resc == 0 && jump < 0.2 && pv > 0.99 * FOREIGN_WARP_SPEED && p.is_finite(),
                format!("foreign ship at {:.0} km/s next to the walker: walked {d:.2} m in 5 s (free walk 24.5), grounded {:.1} %, {resc} rescues, {dep} depenetrations, largest step {jump:.3} m", pv / 1000.0, 100.0 * g / n));
            w.resource_mut::<ForeignDriver>().hold_vel = None;
            return true;
        }
        false
    }));
    // 8. The ship lifts to 3 km (no terrain on its course), the walker boards (test setup), the
    //    ship ramps up like the quantum drive to 1e6 m/s and cruises 3 s; at the end the walker
    //    walks sideways in the cabin.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "in the cabin of the foreign ship warping to 1e6 m/s");
            let pl = planet(w);
            let (pos, _) = w.resource::<ForeignDriver>().parked.unwrap();
            let up = pl.up(pos);
            let p0 = pl.centre + up * (pl.surface(up) + 3000.0);
            let q = crate::ship::basis_for_up(up);
            let cfg = w.resource::<SystemRes>().0.drive.clone();
            let mut d = w.resource_mut::<ForeignDriver>();
            let t0 = d.tick as f64 * DT + 1.5;
            // In flight: the snapshots send the cabin gravity on again.
            d.parked = None;
            d.warp = Some(ForeignWarp { t0, p0, q, accel_one: cfg.accel_stage_one, switch_speed: cfg.stage_switch_speed, accel_two: cfg.accel_stage_two, top_speed: FOREIGN_WARP_SPEED });
            for k in ["boarded", "n", "g", "out", "drift", "ymin", "jump", "vmax", "walk_n"] {
                c.v.remove(k);
            }
            return false;
        }
        let d = w.resource::<ForeignDriver>();
        let (proxy, wp, now) = (d.proxy, d.warp.unwrap(), d.tick as f64 * DT);
        // Board once the ship holds still at 3 km (after the 150 ms playout).
        if !c.v.contains_key("boarded") {
            if c.t < 1.0 {
                return false;
            }
            with_player(w, |p| {
                p.ship = Some(proxy);
                p.w.pos = DVec3::new(0.0, 0.4, -1.0);
                p.w.halt();
                p.w.forward = DVec3::NEG_Z;
                p.cabin_up = DVec3::Y;
            });
            c.v.insert("boarded", 1.0);
            return false;
        }
        if now < wp.t0 + 0.3 {
            // Settle on the deck while it stands.
            c.p.insert("start", with_player(w, |p| p.w.pos));
            c.p.insert("last", c.p["start"]);
            c.v.insert("depen0", w.resource::<WalkStats>().depenetrations as f64);
            c.v.insert("rescue0", w.resource::<WalkStats>().rescues as f64);
            for k in ["n", "g", "out", "drift", "jump", "vmax"] {
                c.v.insert(k, 0.0);
            }
            c.v.insert("ymin", f64::MAX);
            return false;
        }
        let (pos, grounded, inside) = with_player(w, |p| (p.w.pos, p.w.grounded, p.ship == Some(proxy)));
        let pv = w.get::<LinearVelocity>(proxy).unwrap().0.length();
        let walking = c.v.contains_key("walk_n");
        let start = c.p["start"];
        if !walking {
            *c.v.get_mut("drift").unwrap() = c.v["drift"].max(((pos.x - start.x).powi(2) + (pos.z - start.z).powi(2)).sqrt());
            *c.v.get_mut("ymin").unwrap() = c.v["ymin"].min(pos.y);
            *c.v.get_mut("jump").unwrap() = c.v["jump"].max(pos.distance(c.p["last"]));
        }
        c.p.insert("last", pos);
        *c.v.get_mut("vmax").unwrap() = c.v["vmax"].max(pv);
        *c.v.get_mut("n").unwrap() += 1.0;
        *c.v.get_mut("g").unwrap() += grounded as u32 as f64;
        *c.v.get_mut("out").unwrap() += !inside as u32 as f64;
        let cruise_end = wp.t0 + wp.ramp_time() + 3.0;
        if now >= cruise_end && !walking {
            c.v.insert("walk_n", 0.0);
            c.p.insert("walk_start", pos);
            keys(w, &[KeyCode::KeyD], true);
            return false;
        }
        if walking {
            *c.v.get_mut("walk_n").unwrap() += 1.0;
            if c.v["walk_n"] < 12.0 {
                return false;
            }
            keys(w, &[KeyCode::KeyD], false);
            let dx = pos.x - c.p["walk_start"].x;
            let ws = w.resource::<WalkStats>().clone();
            let dep = ws.depenetrations as f64 - c.v["depen0"];
            let resc = ws.rescues as f64 - c.v["rescue0"];
            let (n, g, out, drift, ymin, jump, vmax) = (c.v["n"], c.v["g"], c.v["out"], c.v["drift"], c.v["ymin"], c.v["jump"], c.v["vmax"]);
            let (dist, _) = wp.at(now);
            end(w, c, format!(
                "ship top speed {:.0} km/s, {:.0} km flown, ramp {:.1} s + 3 s cruise; deck contact {g}/{n} ticks, left cabin {out} ticks, standing drift {:.2} mm, lowest feet {ymin:.3} m, largest standing step {:.2} mm, {dep} depenetrations, {resc} rescues; walked {dx:.3} m sideways at top speed",
                vmax / 1000.0, dist / 1000.0, wp.ramp_time(), drift * 1000.0, jump * 1000.0));
            check(c, vmax > 0.99 * FOREIGN_WARP_SPEED && out == 0.0 && g / n > 0.99 && drift < 0.05 && ymin > 0.28 && jump < 0.01 && resc == 0.0 && dep == 0.0 && pos.is_finite(),
                format!("walker in the cabin of a foreign ship warping to {:.0} km/s: deck contact {:.1} %, drift {:.2} mm, largest step {:.2} mm, {dep} depenetrations, {resc} rescues", vmax / 1000.0, 100.0 * g / n, drift * 1000.0, jump * 1000.0));
            check(c, dx > 0.5 && dx < 1.5, format!("walking inside the foreign cabin at {:.0} km/s uses the local frame ({dx:.3} m)", vmax / 1000.0));
            return true;
        }
        false
    }));
}


// ---------- warp ----------

pub(crate) const HEARTH: PlanetId = PlanetId(0);
pub(crate) const CINDER: PlanetId = PlanetId(1);
/// The ship must be this close to the drive's end point on the tick after it got there (m):
/// it is placed exactly, then flies one tick at the exit speed (6.7 m) under the pilot.
const END_TOLERANCE: f64 = 10.0;
/// Nose within this angle of the target's centre on the first tick after the arrival (deg).
const NOSE_TOLERANCE: f64 = 2.0;

pub(crate) fn warp_state(w: &World) -> (Phase, Option<Abort>) {
    let wd = w.resource::<WarpDrive>();
    (wd.drive.phase, wd.last_abort)
}

fn tel(w: &World) -> &WarpTelemetry {
    w.resource::<WarpTelemetry>()
}

/// Place the ship (test setup): pose, no velocity.
pub(crate) fn teleport_ship(w: &mut World, pos: DVec3, rot: DQuat) {
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
pub(crate) enum Flight {
    /// The pilot stands up when the ramp-up starts and the walker stays in the cabin (deck
    /// contact and drift; walking at top speed; the cabin view of the cruise).
    Passenger,
    /// The pilot stays seated (chase camera: the view from outside).
    Seated,
    /// Seated; the pilot holds J from the middle of the path on: emergency exit.
    Emergency,
    /// Seated; the pilot holds J from a fifth of the path on: the drop ends nearer the planet left.
    EarlyEmergency,
}

impl Flight {
    fn emergency(self) -> bool {
        matches!(self, Flight::Emergency | Flight::EarlyEmergency)
    }

    /// Share of the path from which the pilot holds J.
    fn hold_from(self) -> f64 {
        if self == Flight::EarlyEmergency { 0.2 } else { 0.5 }
    }
}

/// One warp from where the ship is placed (`start`) to planet `to`, flown by script: the pilot
/// holds the course. Ends 2.5 s after the arrival or drop. Screenshots of the cruise, the exit
/// and 2 s after it.
pub(crate) fn warp_flight(name: &'static str, tag: &'static str, start: Option<PlanetId>, to: PlanetId, how: Flight, dir: std::path::PathBuf, windowed: bool) -> Step {
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
            c.v.insert("planet0", w.resource::<PlanetRes>().id.0 as f64);
            c.v.insert("swaps0", tel(w).swaps.len() as f64);
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
        // Emergency exit: hold J from the middle of the path (early: a fifth) until the drop starts.
        if how.emergency() {
            let past = {
                let wd = w.resource::<WarpDrive>();
                let pos = ship_frame_of_ro(w);
                wd.drive.path().is_some_and(|p| pos.distance(p.at(0.0).0) > p.length() * how.hold_from())
            };
            let hold = phase.on_rails() && phase != Phase::EmergencyDrop && past;
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
            let label = if how.emergency() { "dropped" } else { "exit" };
            shot(w, c, &dir, windowed, &format!("{tag}-{label}"));
        }
        if let Some(&te) = c.v.get("t_end")
            && c.t - te >= 2.0
            && !c.v.contains_key("terrain_n")
        {
                let label = if how.emergency() { "dropped" } else { "exit" };
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
            if how.emergency() {
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
                if c.v["planet0"] == to.0 as f64 {
                    // On from a drop point: the target became the simulation's planet at the drop (#14).
                    let (busy, swaps) = (w.resource::<PendingPlanet>().busy(), tel(w).swaps.len() as f64 - c.v["swaps0"]);
                    check(c, !busy && swaps == 0.0, format!("{name}: target loaded since the drop: nothing generated (busy {busy}), {swaps} planet swaps"));
                } else {
                    check(c, gen_ms >= 0.0 && gen_ms < dur * 1000.0, format!("{name}: target generated in {gen_ms:.0} ms in the background during {dur:.1} s of flight"));
                }
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
pub(crate) fn wait_drive_idle() -> Step {
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

/// `swap_rounds`: planet swaps by warp in the `swap` scenario (at least 3).
pub fn build(name: &str, out_dir: &std::path::Path, windowed: bool, swap_rounds: usize) -> Vec<Step> {
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
        // #16: a warping remote ship (about 1e6 m/s) right next to the walking walker.
        "foreign_warp" => foreign_warp_steps(&mut s),
        // #32 and #30: another player's figure outside and in the cabin; with --menu the menus.
        "figure" => figure_steps(&mut s, &shot_step),
        // Sprint 2 feel: input ramp, virtual-joystick mouse, boost, decoupled (#24, #25, #26).
        "flight" => flight_steps(&mut s, &shot_step, out_dir, windowed),
        // #21: edit a tuning file while running (dev builds).
        "reload" => reload_steps(&mut s, out_dir),
        "warp" => warp_steps(&mut s, out_dir, windowed),
        // #14 and #34: three planet swaps and an emergency drop; what each swap leaves behind.
        "swap" => swap_steps(&mut s, out_dir, windowed, swap_rounds.max(3)),
        // #80: a crate in the cabin through take-off, flight, warp and landing; one out over the ramp.
        "crate-ride" => crate::cargo_scenario::crate_ride_steps(&mut s, out_dir, windowed),
        // #82: one tap for a crate and the seat, with the HUD prompt.
        "interact" => crate::cargo_scenario::interact_steps(&mut s),
        // #83: carry each size, throw, the grab tool from 8 m, the large crate alone.
        "crate-carry" => crate::cargo_scenario::crate_carry_steps(&mut s),
        // #84: lock grid: locked, loose and blocked crates through hard acceleration and a warp.
        "crate-lock" => crate::cargo_scenario::crate_lock_steps(&mut s, out_dir, windowed),
        // #85: object budget: cap, sleep, persistence cap over a planet swap, timeout, distance.
        "crate-budget" => crate::cargo_scenario::crate_budget_steps(&mut s, out_dir, windowed),
        // Night extra E1: unload the parked ship down the ramp by hand and load it again.
        "crate-unload" => crate::cargo_scenario::crate_unload_steps(&mut s),
        // #63: fixed viewpoints and an atlas per planet (headless: atlas and statistics only).
        "planet-look" => crate::look::steps(&mut s, out_dir, windowed),
        // #70: walk from outside into a site; the walker stands on its flattened ground.
        "site-walk" => crate::look::site_walk_steps(&mut s),
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
            s.push(land("land"));
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
            // #80: the test crate rode along the whole flight in the cabin.
            s.push(Box::new(|w, c| {
                let ship = ship_e(w);
                let crates: Vec<(Option<Entity>, DVec3)> = w.query::<&crate::cargo::Crate>().iter(w).map(|c| (c.ship, c.body.pos)).collect();
                let inside = crates.iter().filter(|(s, p)| *s == Some(ship) && crate::ship::cabin_contains(*p, 0.0)).count();
                check(c, crates.len() == 1 && inside == 1, format!("test crate still in the cabin after the flights ({inside} of {} crates inside)", crates.len()));
                true
            }));
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
    s.push(land("land"));
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

/// #16: a remote proxy held 40 m beside the walker with the velocity of a ship in a warp
/// (1e6 m/s): its collider AABB grows to kilometres and the walker's moving-collider sweep sees
/// 16.7 km of relative motion per tick. The walk must be the same as without it.
// ---------- planet swaps (#14, #34) ----------

/// Ticks between a planet swap and its count: the freed meshes leave `Assets` a frame later,
/// and the ship is still about 1000 km out (only the new planet's roots, no refinement yet).
const SWAP_AUDIT_DELAY: u64 = 30;

/// What one planet swap left behind, counted `SWAP_AUDIT_DELAY` ticks after it.
#[derive(Clone, Debug)]
pub struct SwapRow {
    pub from: PlanetId,
    pub to: PlanetId,
    pub drop: bool,
    /// Terrain chunks, entities (all of its scene) and meshes of the departed planet still there.
    pub old_chunks: usize,
    pub old_entities: usize,
    pub old_meshes: usize,
    /// The departed planet's generator is freed (no `Arc` left anywhere).
    pub old_freed: bool,
    /// Whole world: terrain chunks, entities, meshes.
    pub chunks: usize,
    pub entities: u32,
    pub meshes: usize,
    /// Root chunks built on the main thread on the swap frame (#34).
    pub roots_built_here: usize,
    pub rss_mb: Option<f64>,
}

/// Scenario `swap`: watches `WarpTelemetry::swaps` and counts what each swap left behind.
#[derive(Resource, Default)]
pub struct SwapAudit {
    tick: u64,
    seen: usize,
    /// The current planet's generator, kept weak (taken every tick, so at a swap it is the old one's).
    last: Option<std::sync::Weak<planet_core::Planet>>,
    due: Vec<(u64, PlanetId, PlanetId, bool, Vec<AssetId<Mesh>>, std::sync::Weak<planet_core::Planet>)>,
    pub rows: Vec<SwapRow>,
}

/// Runs after `planet_swap` in the fixed step, before the terrain is rebuilt (Update): the old
/// terrain still knows its meshes.
pub fn swap_audit(w: &mut World) {
    let swaps = w.resource::<WarpTelemetry>().swaps.clone();
    let old_meshes = w.get_resource::<crate::terrain::Terrain>().map(|t| t.mesh_ids()).unwrap_or_default();
    let weak = std::sync::Arc::downgrade(&w.resource::<PlanetRes>().pgen);
    let mut a = w.resource_mut::<SwapAudit>();
    a.tick += 1;
    while a.seen < swaps.len() {
        let (_, from, to, drop) = swaps[a.seen];
        let old = a.last.clone().unwrap_or_default();
        let due = a.tick + SWAP_AUDIT_DELAY;
        a.due.push((due, from, to, drop, old_meshes.clone(), old));
        a.seen += 1;
    }
    a.last = Some(weak);
    let tick = a.tick;
    let ready: Vec<_> = a.due.iter().filter(|d| d.0 <= tick).cloned().collect();
    a.due.retain(|d| d.0 > tick);
    for (_, from, to, drop, meshes, old) in ready {
        let mut q = w.query::<(&crate::terrain::PlanetScene, Has<crate::terrain::TerrainChunk>)>();
        let (mut old_chunks, mut old_entities, mut chunks) = (0, 0, 0);
        for (ps, chunk) in q.iter(w) {
            old_entities += (ps.0 == from) as usize;
            old_chunks += (ps.0 == from && chunk) as usize;
            chunks += chunk as usize;
        }
        let assets = w.resource::<Assets<Mesh>>();
        let row = SwapRow {
            from,
            to,
            drop,
            old_chunks,
            old_entities,
            old_meshes: meshes.iter().filter(|id| assets.contains(**id)).count(),
            old_freed: old.upgrade().is_none(),
            chunks,
            entities: w.entities().count_spawned(),
            meshes: assets.len(),
            roots_built_here: w.get_resource::<crate::terrain::Terrain>().filter(|t| t.for_planet == to).map_or(usize::MAX, |t| t.roots_built_here),
            rss_mb: crate::perf::rss_mb(),
        };
        println!(
            "swap {} {from} -> {to}{}: departed planet left {} terrain chunks, {} entities, {} of its {} meshes, generator freed {}; world {} terrain chunks, {} entities, {} meshes; root chunks built on the swap frame {}; RSS {}",
            w.resource::<SwapAudit>().rows.len() + 1,
            if drop { " (emergency drop)" } else { "" },
            row.old_chunks,
            row.old_entities,
            row.old_meshes,
            meshes.len(),
            row.old_freed,
            row.chunks,
            row.entities,
            row.meshes,
            row.roots_built_here,
            row.rss_mb.map_or("n/a".into(), |m| format!("{m:.0} MB")),
        );
        w.resource_mut::<SwapAudit>().rows.push(row);
    }
}

/// Without a window nobody moves the view: the terrain refines around the own ship.
pub fn headless_view(mut origin: ResMut<RenderOrigin>, ships: Query<&Position, With<Ship>>) {
    if let Ok(p) = ships.single() {
        origin.view = p.0;
    }
}

/// Share the counts after a later swap may differ from the first swap's (#14).
const SWAP_COUNT_TOLERANCE: f64 = 0.05;

fn swap_steps(s: &mut Vec<Step>, dir: &std::path::Path, windowed: bool, rounds: usize) {
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        true
    }));
    s.extend(sit());
    let name = |n: String| -> &'static str { Box::leak(n.into_boxed_str()) };
    let pname = |p: PlanetId| if p == HEARTH { "Hearth" } else { "Cinder" };
    let mut at = HEARTH;
    for i in 0..rounds {
        let to = if at == HEARTH { CINDER } else { HEARTH };
        s.push(warp_flight(name(format!("swap {}: warp {} -> {}", i + 1, pname(at), pname(to))), name(format!("swap{}", i + 1)), Some(at), to, Flight::Seated, dir.to_path_buf(), windowed));
        s.push(wait_drive_idle());
        at = to;
    }
    let other = if at == HEARTH { CINDER } else { HEARTH };
    // Late drop (past the middle): the nearest planet is the target, it becomes the simulation's.
    s.push(warp_flight(name(format!("swap {}: late emergency exit {} -> {}", rounds + 1, pname(at), pname(other))), "swap-late", Some(at), other, Flight::Emergency, dir.to_path_buf(), windowed));
    s.push(Box::new(move |w, c| {
        let now = w.resource::<PlanetRes>().id;
        check(c, now == other, format!("swap: after the late drop the simulation's planet is {now}, the nearest (the warp's target {other}), no longer {at}"));
        true
    }));
    s.push(wait_drive_idle());
    // Early drop (a fifth into the path): the nearest planet is the one left, it stays; the
    // target is kept generated for a jump on.
    s.push(Box::new(|w, c| {
        c.v.insert("swaps_before_early", tel(w).swaps.len() as f64);
        true
    }));
    s.push(warp_flight(name(format!("early emergency exit {} -> {}", pname(other), pname(at))), "early", Some(other), at, Flight::EarlyEmergency, dir.to_path_buf(), windowed));
    s.push(Box::new(move |w, c| {
        let now = w.resource::<PlanetRes>().id;
        let swaps = tel(w).swaps.len() as f64 - c.v["swaps_before_early"];
        let busy = w.resource::<PendingPlanet>().busy();
        check(c, now == other && swaps == 0.0 && busy, format!("swap: after the early drop the simulation's planet stays {now}, the nearest; {swaps} planet swaps; target kept generated for a jump on {busy}"));
        true
    }));
    s.push(wait_drive_idle());
    // On from the drop point: the kept target, swapped in when the ship enters its zone.
    s.push(warp_flight(name(format!("swap {}: warp on from the drop point to {}", rounds + 2, pname(at))), "swap-on", None, at, Flight::Seated, dir.to_path_buf(), windowed));
    s.push(wait(1.0));
    s.push(Box::new(move |w, c| {
        begin(w, c, "what the planet swaps left behind");
        let rows = w.resource::<SwapAudit>().rows.clone();
        let drops = rows.iter().filter(|r| r.drop).count();
        check(c, rows.len() == rounds + 2 && drops == 1, format!("swap: {} planet swaps, {drops} of them at an emergency drop (expected {} and 1)", rows.len(), rounds + 2));
        let Some(first) = rows.first().cloned() else { return true };
        let near = |a: f64, b: f64| (a - b).abs() <= SWAP_COUNT_TOLERANCE * b.max(1.0);
        for (i, r) in rows.iter().enumerate() {
            check(c, r.old_chunks == 0 && r.old_entities == 0 && r.old_meshes == 0 && r.old_freed,
                format!("swap {}: departed planet {} left {} terrain chunks, {} entities, {} meshes; generator freed {}", i + 1, r.from, r.old_chunks, r.old_entities, r.old_meshes, r.old_freed));
            check(c, near(r.chunks as f64, first.chunks as f64) && near(r.entities as f64, first.entities as f64) && near(r.meshes as f64, first.meshes as f64),
                format!("swap {}: world {} terrain chunks, {} entities, {} meshes; within {:.0} % of the first swap's {}, {}, {}", i + 1, r.chunks, r.entities, r.meshes, SWAP_COUNT_TOLERANCE * 100.0, first.chunks, first.entities, first.meshes));
            // #34: the roots come from the pool, built during the flight.
            check(c, r.roots_built_here == 0, format!("swap {}: {} root chunks built on the swap frame (prebuilt on the pool)", i + 1, r.roots_built_here));
        }
        let rss: Vec<String> = rows.iter().map(|r| r.rss_mb.map_or("n/a".into(), |m| format!("{m:.0}"))).collect();
        end(w, c, format!("RSS after each swap (MB, reported, not checked): {}", rss.join(", ")));
        true
    }));
}

fn foreign_warp_steps(s: &mut Vec<Step>) {
    const WARP_SPEED: f64 = 1.0e6;
    s.push(Box::new(|w, _| {
        // Away from the own parked ship (15 m ahead of the spawn).
        let p = player_world(w);
        let own = ship_frame_of(w).origin;
        face_towards(w, p + (p - own));
        let pl = planet(w);
        let feet = player_world(w);
        let up = pl.up(feet);
        let (fwd, right) = with_player(w, |p| (p.w.forward, p.w.forward.cross(p.up)));
        let pos = feet + right * 40.0 + up * 3.0;
        let mut commands = w.commands();
        let proxy = crate::net_live::spawn_proxy(&mut commands, 2, pos, walker_core::look_rot(fwd, up));
        w.flush();
        w.insert_resource(WarpingProxy { proxy, pos, vel: fwd * WARP_SPEED });
        true
    }));
    s.push(Box::new(|w, c| {
        hold_proxy(w);
        if c.t == 0.0 {
            begin(w, c, "walk 5 s beside a remote ship at 1e6 m/s");
            c.p.insert("start", player_world(w));
            keys(w, &[KeyCode::KeyW], true);
        }
        if c.t >= 5.0 {
            keys(w, &[KeyCode::KeyW], false);
            let d = player_world(w).distance(c.p["start"]);
            let (v, steps) = (with_player(w, |p| p.w.vel.length()), w.resource::<WalkStats>().steps);
            end(w, c, format!("walked {d:.2} m, speed {v:.2} m/s, {steps} steps"));
            // Walk speed 5 m/s for 5 s, less the step-off.
            check(c, d > 22.0 && d < 26.0 && d.is_finite(), format!("foreign warp: walked {d:.2} m in 5 s next to a ship at 1e6 m/s (free walk 24.5)"));
            return true;
        }
        false
    }));
    s.push(Box::new(|w, c| {
        let ok = with_player(w, |p| p.w.pos.is_finite() && p.ship.is_none());
        check(c, ok, "foreign warp: walker stays outside, finite".into());
        true
    }));
}

#[derive(Resource)]
struct WarpingProxy {
    proxy: Entity,
    pos: DVec3,
    vel: DVec3,
}

/// Back to its place each tick, with the warp's velocity (the physics step moves it 16.7 km).
fn hold_proxy(w: &mut World) {
    let Some(wp) = w.get_resource::<WarpingProxy>() else { return };
    let (e, pos, vel) = (wp.proxy, wp.pos, wp.vel);
    if let Some(mut p) = w.get_mut::<Position>(e) {
        p.0 = pos;
    }
    if let Some(mut v) = w.get_mut::<LinearVelocity>(e) {
        v.0 = vel;
    }
}

fn figure_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    use crate::menu::{Back, Menu, Screen};
    let menu_shot = |screen: Screen, tag: &'static str, shot_step: &dyn Fn(&'static str) -> Step| -> Vec<Step> {
        vec![
            Box::new(move |w: &mut World, _: &mut Ctx| {
                if let Some(mut m) = w.get_resource_mut::<Menu>() {
                    m.screen = screen;
                }
                true
            }),
            wait(0.3),
            shot_step(tag),
            wait(0.3),
        ]
    };
    for (screen, tag) in [(Screen::Main, "menu-main"), (Screen::Join, "menu-join"), (Screen::Settings(Back::Main), "menu-settings"), (Screen::Paused, "menu-paused")] {
        s.extend(menu_shot(screen, tag, shot_step));
    }
    s.extend(menu_shot(Screen::None, "menu-closed", shot_step));
    // Another player's figure 4 m in front, facing the walker (test hook: a remote walker without
    // a network).
    s.push(Box::new(|w, _| {
        let p = player_world(w);
        let own = ship_frame_of(w).origin;
        face_towards(w, p + (p - own));
        let (fwd, up) = with_player(w, |pl| (pl.w.forward, pl.up));
        let at = p + fwd * 4.0;
        w.spawn((crate::net_live::RemoteWalker { owner: 2 }, crate::origin::WorldPose { pos: at, rot: walker_core::look_rot(-fwd, up) }, Transform::default(), Visibility::default()));
        true
    }));
    s.push(wait(0.5));
    s.push(shot_step("figure-outside"));
    // A screenshot is taken a few frames later; keep the scene until then.
    s.push(wait(0.5));
    // In the cabin: the walker stands at the seat, the figure near the back, both looking at it.
    s.push(Box::new(|w, _| {
        put_at_seat(w);
        let f = ship_frame_of(w);
        let at = f.to_world(DVec3::new(0.0, 0.32, 2.0));
        let up = f.rot * DVec3::Y;
        let mut q = w.query_filtered::<&mut crate::origin::WorldPose, With<crate::net_live::RemoteWalker>>();
        for mut pose in q.iter_mut(w) {
            pose.pos = at;
            pose.rot = walker_core::look_rot(f.rot * DVec3::NEG_Z, up);
        }
        face_towards(w, at);
        true
    }));
    s.push(wait(0.6));
    s.push(shot_step("figure-cabin"));
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

