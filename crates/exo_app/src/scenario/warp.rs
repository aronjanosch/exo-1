//! Warp between the planets (quantum drive): flights, refused starts, aborts, the drop point.
use super::*;

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

pub(super) fn tel(w: &World) -> &WarpTelemetry {
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

pub(super) fn warp_steps(s: &mut Vec<Step>, dir: &std::path::Path, windowed: bool) {
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
