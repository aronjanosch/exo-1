//! Scripted runs: a list of steps, each called once per fixed tick until it returns true. Steps
//! drive the game only through `Controls` and a few test hooks (teleport for test setup, ship
//! test input). A run exits non-zero when a check fails. One module per topic (#52); this one
//! holds the shared script interface (`Step`, `Ctx`, helpers, step builders), `build` and `run_script`.
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

mod boost;
mod cargo;
mod courier;
mod customers;
mod deliver;
mod figure;
mod flight;
mod landing;
mod models;
mod net;
mod reload;
mod space;
mod swap;
mod walk;
mod warp;

pub use self::{cargo::*, net::*, swap::*};
pub(crate) use self::warp::*;
use self::{figure::*, flight::*, reload::*, space::*, walk::*};

/// The script driver and what single scenarios add (`foreign`, `swap`). Not added without a scenario.
pub fn plugin(name: &str, headless: bool) -> impl Plugin {
    let name = name.to_string();
    move |app: &mut App| {
        app.add_systems(FixedUpdate, run_script.run_if(resource_exists::<Script>).before(crate::controls::resolve_actions).in_set(crate::phases::Fx::Input));
        if name == "foreign" {
            // The remote ship is placed before the controllers read it, after net_pre.
            app.add_systems(FixedUpdate, net::foreign_drive.run_if(resource_exists::<ForeignDriver>).after(crate::net_live::net_pre).in_set(crate::phases::Fx::Input));
        }
        if name == "swap" {
            // #14: count what a planet swap leaves behind; headless with the terrain too.
            app.init_resource::<swap::SwapAudit>();
            app.add_systems(FixedUpdate, swap::swap_audit.after(crate::warp::warp_telemetry).in_set(crate::phases::Fx::Drive));
            if headless {
                app.add_systems(Update, swap::headless_view.in_set(crate::phases::Frame::Camera));
            }
        }
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
    check_steady_within(c, what, 0.5);
}

/// `check_steady` with another largest look step (deg per tick).
fn check_steady_within(c: &mut Ctx, what: &str, look_max: f64) {
    let (look, up, eye) = (c.v["look_jump"], c.v["up_jump"], c.v["eye_jump"]);
    check(c, look < look_max && up < 1.0 && eye < 0.03,
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

pub(crate) fn shot(w: &mut World, c: &mut Ctx, script_dir: &std::path::Path, windowed: bool, tag: &str) {
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

/// How far a landed ship may settle from its touchdown spot (#92). The residue depends on the
/// ground under the hull: 0.56 mm on the slope the scenario found before the drainage (#72),
/// 1.1 mm on the one the eroded terrain offers; before #92 a ship slid 553 mm.
pub(crate) const LANDING_SETTLE_M: f64 = 0.002;

/// Hold Ctrl until the ship rests (well below the landed check's 0.05 m/s). From the first hull
/// contact on it must not slide (#92): pressed down onto a slope it used to slide 20 s, and tipping
/// from the first corner onto a 33 degree slope 0.55 m. The touchdown spot is left in `c.p`.
pub(crate) fn land(name: &'static str) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
            keys(w, &[KeyCode::ControlLeft], true);
            c.p.remove("touch");
            c.p.remove("before");
        }
        let pos = ship_frame_of(w).origin;
        // `grounded` is from the last tick's ship step, which found the contact at the position
        // this step saw a tick earlier: that is the touchdown spot.
        if with_ship(w, |s| s.grounded) && !c.p.contains_key("touch") {
            let at = c.p.get("before").copied().unwrap_or(pos);
            c.p.insert("touch", at);
        }
        c.p.insert("before", pos);
        let v = ship_vel(w).length();
        if c.t > 3.0 && v < 0.02 || c.t >= 90.0 {
            keys(w, &[KeyCode::ControlLeft], false);
            let up = planet(w).up(pos);
            let slide = c.p.get("touch").map(|t| {
                let d = pos - *t;
                (d - up * d.dot(up)).length()
            });
            let agl = above_ground(w);
            let mm = slide.unwrap_or(f64::NAN) * 1000.0;
            end(w, c, format!("{:.1} s, ground {agl:.2} m, speed {v:.3} m/s, slid {mm:.2} mm after touchdown", c.t));
            check(c, slide.is_some_and(|s| s < LANDING_SETTLE_M), format!("{name}: no slide after touchdown ({mm:.2} mm)"));
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
        // #90, #91: the boost capacitor drains, cuts out and recharges; the HUD shows it.
        "boost-hud" => boost::boost_hud_steps(&mut s),
        // Spike 13: manoeuvres with the axis flight model, measured as a table.
        "flight-model" => models::flight_model_steps(&mut s),
        // #92: land on a slope below the limit; no drift from touchdown until thrust.
        "slope-landing" => landing::slope_landing_steps(&mut s),
        // #21: edit a tuning file while running (dev builds).
        "reload" => reload_steps(&mut s, out_dir),
        "warp" => warp_steps(&mut s, out_dir, windowed),
        // #14 and #34: three planet swaps and an emergency drop; what each swap leaves behind.
        "swap" => swap_steps(&mut s, out_dir, windowed, swap_rounds.max(3)),
        // #80: a crate in the cabin through take-off, flight, warp and landing; one out over the ramp.
        "crate-ride" => cargo::crate_ride_steps(&mut s, out_dir, windowed),
        // #82: one tap for a crate and the seat, with the HUD prompt.
        "interact" => cargo::interact_steps(&mut s),
        // #83: carry each size, throw, the grab tool from 8 m, the large crate alone.
        "crate-carry" => cargo::crate_carry_steps(&mut s, out_dir, windowed),
        // #84: lock grid: locked, loose and blocked crates through hard acceleration and a warp.
        "crate-lock" => cargo::crate_lock_steps(&mut s, out_dir, windowed),
        // #85: object budget: cap, sleep, persistence cap over a planet swap, timeout, distance.
        "crate-budget" => cargo::crate_budget_steps(&mut s, out_dir, windowed),
        // Night extra E1: unload the parked ship down the ramp by hand and load it again.
        "crate-unload" => cargo::crate_unload_steps(&mut s),
        "deliver" => deliver::deliver_steps(&mut s),
        // #170: courier jobs on foot, carried by hand to the small drops near the start.
        "courier" => courier::courier_steps(&mut s),
        // #168: a customer orders, the order is an offer at the wholesaler, the delivery moves the relationship.
        "customers" => customers::customers_steps(&mut s),
        // #63: fixed viewpoints and an atlas per planet (headless: atlas and statistics only).
        "planet-look" => crate::look::steps(&mut s, out_dir, windowed),
        // #70: walk from outside into a site; the walker stands on its flattened ground.
        "site-walk" => crate::look::site_walk_steps(&mut s),
        // #48: fast-forward a day on every planet; sun direction and brightness against the core.
        "daynight" => crate::daynight::scenario_steps(&mut s),
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
