//! Scripted runs (port of auto_test.gd): a list of steps, each called once per fixed tick
//! until it returns true. Steps drive the game only through `Controls` and a few test hooks
//! (teleport for test setup, ship test input), like the Godot test bot.
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
    pub windowed: bool,
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
/// Put the walker on the ground at a world point (test setup, like the Godot bot).
fn place_walker(w: &mut World, at: DVec3) {
    let pl = planet(w);
    let dir = pl.up(at);
    let pos = pl.centre + dir * (pl.surface(dir) + 0.1);
    with_player(w, |p| {
        p.ship = None;
        p.w.pos = pos;
        p.w.vel = DVec3::ZERO;
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

fn begin(w: &mut World, c: &mut Ctx, name: &str) {
    c.phase = name.to_string();
    c.stats0 = w.resource::<WalkStats>().clone();
    c.rescues0 = c.stats0.rescues;
    if let Some(mut v) = w.get_resource_mut::<ViewState>() {
        v.frame_ms.clear();
    }
    println!("PHASE '{name}' tick {}", c.ticks);
}

fn end(w: &mut World, c: &mut Ctx, note: String) {
    let s = w.resource::<WalkStats>().clone();
    let steps = (s.steps - c.stats0.steps).max(1);
    let mut line = format!(
        "{}: {} | rescues {}, walker grounded {:.1} %, net-only {}",
        c.phase,
        note,
        s.rescues - c.rescues0,
        100.0 * (s.grounded - c.stats0.grounded) as f64 / steps as f64,
        s.net_only - c.stats0.net_only,
    );
    if let Some(v) = w.get_resource::<ViewState>() {
        let mut f = v.frame_ms.clone();
        if !f.is_empty() {
            f.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let n = f.len();
            let mean = f.iter().sum::<f64>() / n as f64;
            line += &format!(
                " | frames {n}, mean {mean:.2} ms, p99 {:.2} ms, max {:.2} ms, >33 ms {}",
                f[(n * 99 / 100).min(n - 1)],
                f[n - 1],
                f.iter().filter(|&&x| x > 33.3).count()
            );
        }
    }
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
    let path = script_dir.join(format!("shot-{:02}-{tag}.png", c.shot_n));
    w.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    if let Some(mut v) = w.get_resource_mut::<ViewState>() {
        v.skip_frames = 4;
    }
}

/// Calculated GPU error for a point 2 m in front of the camera (same idea as spike 5's
/// jitter probe): the f32 path model translation + local vertex - camera translation,
/// against the exact difference. Returns pixels at 1080 lines and 75 degree FOV.
pub fn probe_px(cam: DVec3, fwd: DVec3, origin: DVec3) -> f64 {
    let p = cam + fwd * 2.0;
    let chunk = p + DVec3::new(11.0, -7.0, 13.0); // a vertex about 19 m from its chunk centre
    let t = (chunk - origin).as_vec3();
    let v = (p - chunk).as_vec3();
    let ct = (cam - origin).as_vec3();
    let gpu = (t + v) - ct;
    let err = (gpu.as_dvec3() - (p - cam)).length();
    err / (2.0 * 2.0 * (37.5f64).to_radians().tan()) * 1080.0
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
            c.v.insert("probe", 0.0);
            keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], true);
        }
        let p = player_world(w);
        if !w.resource::<Ring>().has_patch_near(p) {
            *c.v.get_mut("uncovered").unwrap() += 1.0;
        }
        let (vel, fwd) = with_player(w, |pl| (pl.w.vel, pl.w.forward));
        let up = planet(w).up(p);
        if c.t > 0.5 && (vel - up * vel.dot(up)).length() < 6.0 {
            *c.v.get_mut("slow").unwrap() += 1.0;
        }
        let origin = w.resource::<RenderOrigin>().origin;
        let px = probe_px(p + up * 1.7, fwd, origin);
        *c.v.get_mut("probe").unwrap() = c.v["probe"].max(px);
        if c.t >= secs {
            keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], false);
            let walked = p.distance(c.p["start"]);
            let note = format!(
                "walked {walked:.0} m, slow ticks {}, ticks without patch {}, {:.1} km from world origin, GPU error 2 m ahead {:.3} px (calculated)",
                c.v["slow"], c.v["uncovered"], p.length() / 1000.0, c.v["probe"]
            );
            end(w, c, note);
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
        if c.t >= 5.0 {
            keys(w, &[KeyCode::KeyW], false);
        }
        if c.t >= 5.3 {
            let e = ship_e(w);
            let (inside, at_seat) = with_player(w, |p| (p.ship == Some(e), p.ship == Some(e) && p.w.pos.distance(SEAT_POS) < 1.8));
            let f = ship_frame_of(w);
            let pl = planet(w);
            let ramp_end = f.to_world(DVec3::new(0.0, 0.0, 5.8));
            let tilt = (f.rot * DVec3::Y).angle_between(pl.up(f.origin)).to_degrees();
            end(w, c, format!("in cabin {inside}, at seat {at_seat}, ramp end {:.2} m above ground, ship tilt {tilt:.0} deg", pl.above_ground(ramp_end)));
            if !at_seat {
                // Keep the run going: put the walker at the seat (as the Godot bot did).
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

/// Turn the nose by feeding mouse movement (radians per tick as the controller sees it).
#[allow(dead_code)]
fn pitch(name: &'static str, rad_per_tick: f64, secs: f64) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, name);
        }
        // Controls.mouse is in pixels; the ship uses 0.002 rad per pixel.
        w.resource_mut::<Controls>().mouse.y += (-rad_per_tick / 0.002) as f32;
        if c.t >= secs {
            let f = ship_frame_of(w);
            let up = planet(w).up(f.origin);
            let nose = (f.rot * DVec3::NEG_Z).dot(up).asin().to_degrees();
            end(w, c, format!("nose {nose:+.1} deg above horizon"));
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
            true
        }),
    ]
}

/// Spike 3 at speed: stand, then walk, while the ship boosts and rolls with the assist off.
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
                // Spike 3 range: keep rolling, stop boosting at 400 m/s.
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
                return true;
            }
            false
        }),
        // Back to the seat (test shortcut, as in Godot) and sit.
        Box::new(|w, _| {
            with_player(w, |p| {
                p.w.pos = DVec3::new(0.0, 0.32, -2.5);
                p.w.vel = DVec3::ZERO;
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
        // Walking only: spike 5 and spike 8 T5 style.
        "walk" => {
            s.extend(stand_still("stand still 5 s (walker)"));
            s.push(walk("walk 20 s (run)", 20.0, true));
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
                if c.t >= 4.0 {
                    keys(w, &[KeyCode::KeyW], false);
                }
                if c.t >= 4.5 {
                    let outside = with_player(w, |p| p.ship.is_none());
                    let agl = above_ground(w);
                    end(w, c, format!("outside {outside}, {agl:.2} m above ground"));
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
            let o = w.resource::<RenderOrigin>();
            let ring = w.resource::<Ring>();
            let summary = format!(
                "TOTAL: rescues {}, walker grounded {:.1} % of {} steps, net-only {}, depenetrations {}, origin shifts {} (max {:.3} ms, mean {:.3} ms), patch build mean {:.2} ms max {:.2} ms, simulated {:.1} s",
                st.rescues,
                100.0 * st.grounded as f64 / st.steps.max(1) as f64,
                st.steps,
                st.net_only,
                st.depenetrations,
                o.shifts,
                o.shift_ms_max,
                o.shift_ms_sum / o.shifts.max(1) as f64,
                ring.build_ms_sum / ring.build_count.max(1) as f64,
                ring.build_ms_max,
                sc.ctx.ticks as f64 / 60.0
            );
            println!("{summary}");
            sc.ctx.report.push(summary);
            let _ = std::fs::create_dir_all(&sc.out_dir);
            let path = sc.out_dir.join(format!("{}.txt", sc.name));
            let _ = std::fs::write(&path, sc.ctx.report.join("\n") + "\n");
            println!("results: {}", path.display());
            w.write_message(AppExit::Success);
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

