//! J starts a warp to the selected planet (N selects) and cancels while spooling or calibrating.
//! Holding J during the flight drops out early (emergency exit). J works for whoever is aboard:
//! the pilot, or a walker in the cabin who stood up mid-warp (standing up stays allowed,
//! initiator 2026-10-09).
use crate::controls::{Actions, Tap};
use crate::env::PlanetRes;
use crate::ring::Ring;
use crate::ship::{RemoteShip, Ship};
use crate::terrain::{build_roots, PrebuiltRoots};
use crate::walker::Player;
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use bevy::tasks::{block_on, AsyncComputeTaskPool, Task};
use campaign_core::{Generator, Names, PlanetSet, SystemKind, Template};
use crate::Options;
use daynight_core::DayNight;
use planet_core::ChunkOut;
use warp_core::{Abort, Drive, Event, Load, Loader, Obstacle, Pending, Phase, PlanetDef, PlanetId, ShipView, System};

pub fn plugin(app: &mut App) {
    // A state machine: the order is the logic.
    app.add_systems(FixedUpdate, (warp_input, warp_drive, planet_swap, warp_telemetry.run_if(resource_exists::<WarpTelemetry>)).chain().in_set(crate::phases::Fx::Drive));
}

pub const SYSTEM: &str = include_str!("../../../content/system/system.json");
const CAMPAIGN_SET: &str = include_str!("../../../content/campaign/planet_set.json");
const CAMPAIGN_NAMES: &str = include_str!("../../../content/campaign/names.json");
const CAMPAIGN_HARBOUR: &str = include_str!("../../../content/campaign/template_harbour.json");
const CAMPAIGN_NORMAL: &str = include_str!("../../../content/campaign/template_normal.json");
const CAMPAIGN_HOSTILE: &str = include_str!("../../../content/campaign/template_hostile.json");

/// The run's star system: `content/system/system.json`, or with `--system-seed` a generated one
/// (debug switch). Generated planets get the file's drive and must pass the day/night checks.
pub fn build_system(o: &Options) -> System {
    let Some(seed) = o.system_seed else { return System::from_json(SYSTEM).expect("system.json") };
    let template = match o.system_kind {
        SystemKind::Harbour => CAMPAIGN_HARBOUR,
        SystemKind::Normal => CAMPAIGN_NORMAL,
        SystemKind::Hostile => CAMPAIGN_HOSTILE,
    };
    fn or_panic<T>(r: Result<T, String>, seed: u64) -> T {
        r.unwrap_or_else(|e| panic!("--system-seed {seed}: {e}"))
    }
    let set = or_panic(PlanetSet::from_json(CAMPAIGN_SET), seed);
    let names = or_panic(Names::from_json(CAMPAIGN_NAMES), seed);
    let template = or_panic(Template::from_json(template), seed);
    let drive = System::from_json(SYSTEM).expect("system.json").drive;
    let dn = DayNight::from_json(crate::daynight::DAYNIGHT).unwrap_or_else(|e| panic!("{e}"));
    let sky_ok = |p: &PlanetDef, star: DVec3| dn.check_geometry(&p.recipe, p.centre(), star).is_ok();
    let g = or_panic(Generator::new(set, names, drive).generate(&template, seed, &sky_ok), seed);
    let recipes: Vec<&str> = g.system.planets.iter().map(|p| p.recipe.as_str()).collect();
    let kind = match g.kind {
        SystemKind::Harbour => "harbour",
        SystemKind::Normal => "normal",
        SystemKind::Hostile => "hostile",
    };
    println!(
        "system: generated '{}' ({kind}, seed {}): {} planets ({}), {} stations, {} enemy, {} sites; stations, sites and enemies are not placed in the world yet",
        g.name,
        g.seed,
        g.system.planets.len(),
        recipes.join(", "),
        g.stations.len(),
        g.enemy.is_some() as usize,
        g.sites.len()
    );
    g.system
}

/// Radius of the sphere other ships block the path with (m).
const SHIP_OBSTACLE_RADIUS: f64 = 20.0;
/// While on rails the nose turns to the course at most this fast (rad/s).
const RAILS_TURN_RATE: f64 = 0.6;

#[derive(Resource)]
pub struct SystemRes(pub System);

#[derive(Resource)]
pub struct WarpDrive {
    pub drive: Drive,
    /// Planet selected as the target (N); the jump goes to `System::effective_target` of it.
    pub selected: PlanetId,
    pub last_abort: Option<Abort>,
    pub events: Vec<Event>,
}

impl WarpDrive {
    pub fn new(sys: &System) -> WarpDrive {
        WarpDrive { drive: Drive::new(sys.drive.clone()), selected: PlanetId(1.min(sys.planets.len() as u8 - 1)), last_abort: None, events: Vec::new() }
    }
}

#[derive(Resource, Default)]
pub struct WarpTelemetry {
    /// Every event with the simulated time it happened.
    pub log: Vec<(f64, Event)>,
    pub clock: f64,
    pub warps: u32,
    /// Largest distance the ship moved in one tick (m).
    pub max_tick_move: f64,
    /// Distance of the ship from the drive's end point (exit or drop point) on the tick it got
    /// there, after the ship was placed (m).
    pub end_error: Option<f64>,
    pub extra_obstacles: Vec<Obstacle>,
    /// Every planet swap: time, old planet, new planet, at an emergency drop.
    pub swaps: Vec<(f64, PlanetId, PlanetId, bool)>,
    last_pos: Option<DVec3>,
}

/// The target planet being generated in the background while the ship flies, with its root
/// terrain chunks. `warp_core::Loader` decides what is generated and when it is swapped in.
#[derive(Resource, Default)]
pub struct PendingPlanet {
    task: Option<(PlanetId, Task<(PlanetRes, Vec<ChunkOut>, f64)>)>,
    /// Wall-clock time the last background generation took (ms).
    pub gen_ms: Option<f64>,
    loader: Loader,
}

impl PendingPlanet {
    pub fn start(&mut self, id: PlanetId, def: PlanetDef) {
        if self.task.as_ref().is_some_and(|(t, _)| *t == id) {
            return;
        }
        self.gen_ms = None;
        self.task = Some((
            id,
            AsyncComputeTaskPool::get().spawn(async move {
                let t0 = std::time::Instant::now();
                let p = PlanetRes::load(id, &def);
                let roots = build_roots(&p);
                (p, roots, t0.elapsed().as_secs_f64() * 1000.0)
            }),
        ));
    }

    pub fn busy(&self) -> bool {
        self.task.is_some()
    }

    pub fn ready(&self) -> bool {
        self.task.as_ref().is_some_and(|(_, t)| t.is_finished())
    }

    fn finish(&self) {
        while self.task.as_ref().is_some_and(|(_, t)| !t.is_finished()) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn state(&self) -> Pending {
        match &self.task {
            None => Pending::None,
            Some((p, t)) if t.is_finished() => Pending::Ready(*p),
            Some((p, _)) => Pending::Running(*p),
        }
    }

    /// The finished planet and its roots; a running generation is waited for, and without one the
    /// planet is generated here and now without roots. Only when the loader swaps off rails in a
    /// frame zone (a teleport, an arrival before the generation is done): no flight to hide it in.
    /// A "materialising" state instead of this hitch.
    fn take(&mut self, id: PlanetId, sys: &System) -> (PlanetRes, Option<Vec<ChunkOut>>) {
        match self.task.take() {
            Some((t, task)) if t == id => {
                let (p, roots, ms) = block_on(task);
                self.gen_ms = Some(ms);
                (p, Some(roots))
            }
            _ => (PlanetRes::load(id, sys.planet(id)), None),
        }
    }
}

pub fn swap_planet(commands: &mut Commands, ring: &mut Ring, new: PlanetRes) {
    for (_, (e, ..)) in ring.patches.drain() {
        commands.entity(e).despawn();
    }
    let keep = std::mem::take(&mut ring.anchors);
    *ring = Ring::new(new.radius);
    ring.anchors = keep;
    commands.insert_resource(new);
}

fn course_quat(from: DQuat, dir: DVec3, max_angle: f64) -> DQuat {
    let nose = from * DVec3::NEG_Z;
    if !dir.is_finite() || dir.length_squared() < 1e-24 {
        return from;
    }
    let angle = nose.angle_between(dir);
    if !angle.is_finite() || angle < 1e-9 {
        return from;
    }
    let turn = DQuat::from_rotation_arc(nose, dir);
    DQuat::IDENTITY.slerp(turn, (max_angle / angle).min(1.0)) * from
}

fn obstacles(remotes: &Query<&Position, (With<RemoteShip>, Without<Ship>)>, tel: Option<&WarpTelemetry>) -> Vec<Obstacle> {
    let mut o: Vec<Obstacle> = remotes.iter().map(|p| Obstacle { centre: p.0, radius: SHIP_OBSTACLE_RADIUS }).collect();
    if let Some(t) = tel {
        o.extend(t.extra_obstacles.iter().copied());
    }
    o
}

/// N selects the target, J starts or cancels, J held during the flight drops out.
pub fn warp_input(
    time: Res<Time>,
    sys: Res<SystemRes>,
    mut wd: ResMut<WarpDrive>,
    mut actions: ResMut<Actions>,
    ships: Query<(Entity, &Ship, &Position, &Rotation, &LinearVelocity)>,
    remotes: Query<&Position, (With<RemoteShip>, Without<Ship>)>,
    players: Query<&Player>,
    tel: Option<Res<WarpTelemetry>>,
) {
    let sys = &sys.0;
    let wd = wd.as_mut();
    wd.events.clear();
    let Ok((ship_e, ship, pos, rot, lv)) = ships.single() else { return };
    let aboard = ship.piloted || players.iter().any(|p| p.ship == Some(ship_e));
    if actions.take_tap(Tap::WarpTarget) {
        wd.selected = PlanetId(((wd.selected.index() + 1) % sys.planets.len()) as u8);
    }
    let obstacles = obstacles(&remotes, tel.as_deref());
    let j = actions.take_tap(Tap::Warp);
    if j && !aboard {
        println!("warp: J ignored, nobody is aboard");
    }
    if j && aboard {
        if wd.drive.phase == Phase::Idle {
            let view = ShipView { pos: pos.0, forward: rot.0 * DVec3::NEG_Z, speed: lv.0.length() };
            match wd.drive.begin(sys.effective_target(wd.selected, pos.0), &view, sys, &obstacles) {
                Ok(()) => {
                    wd.last_abort = None;
                    wd.events.push(Event::Phase(Phase::Spooling));
                }
                Err(why) => wd.events.push(Event::Aborted(why)),
            }
        } else if let Some(ev) = wd.drive.cancel() {
            wd.events.push(ev);
        }
    }
    // An empty ship does not jump: everybody left while it spooled or calibrated (initiator
    // 2026-10-09).
    if !aboard && let Some(ev) = wd.drive.cancel() {
        println!("warp: cancelled, nobody is aboard");
        wd.events.push(ev);
    }
    let held = aboard && actions.warp_exit;
    if let Some(ev) = wd.drive.hold_exit(held, time.delta_secs_f64(), sys, &obstacles) {
        wd.events.push(ev);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn warp_drive(
    mut commands: Commands,
    time: Res<Time>,
    sys: Res<SystemRes>,
    mut wd: ResMut<WarpDrive>,
    mut ring: ResMut<Ring>,
    mut ships: Query<(Entity, &mut Ship, &mut Position, &mut Rotation, &mut LinearVelocity, &mut AngularVelocity)>,
    remotes: Query<&Position, (With<RemoteShip>, Without<Ship>)>,
    tel: Option<Res<WarpTelemetry>>,
) {
    let dt = time.delta_secs_f64();
    let sys = &sys.0;
    let wd = wd.as_mut();
    let Ok((e, mut ship, mut pos, mut rot, mut lv, mut av)) = ships.single_mut() else { return };
    let view = ShipView { pos: pos.0, forward: rot.0 * DVec3::NEG_Z, speed: lv.0.length() };
    let obstacles = obstacles(&remotes, tel.as_deref());
    let stepped = wd.drive.step(dt, &view, sys, &obstacles);
    wd.events.extend(stepped);

    for ev in &wd.events {
        match ev {
            Event::Aborted(why) => wd.last_abort = Some(*why),
            Event::Phase(Phase::RampUp) => {
                commands.entity(e).remove::<SweptCcd>();
            }
            Event::Phase(Phase::PostRampDown) => {
                commands.entity(e).insert(SweptCcd::default());
            }
            _ => {}
        }
    }

    // Carried by the drive: the ship is put at the path's pose and has no velocity of its own,
    // so physics does not move it a second time. Nothing collides on the way (no terrain
    // anywhere near the path, other ships are checked before the start).
    if let Some((p, v)) = wd.drive.pose() {
        if wd.drive.phase.on_rails() {
            rot.0 = course_quat(rot.0, v.normalize_or_zero(), RAILS_TURN_RATE * dt);
            lv.0 = DVec3::ZERO;
        } else {
            // Hand-over at the end point: the pilot gets the exit velocity back, nose along it
            // (at an arrival: at the target's centre).
            rot.0 = course_quat(rot.0, v.normalize_or_zero(), std::f64::consts::PI);
            lv.0 = v;
        }
        pos.0 = p;
        av.0 = DVec3::ZERO;
        ship.parked = false;
    }
    if wd.drive.phase.on_rails() {
        // The anchors of the collision ring would reach for patches along the way.
        ring.anchors.clear();
    }
}

/// The ship comes into another planet's frame zone: that planet becomes the simulation's. An
/// emergency drop outside every frame zone makes the nearest planet the simulation's (the
/// old one goes with its terrain). `warp_core::Loader` has the rules: on rails only the
/// target counts and the swap waits for its background generation.
#[allow(clippy::too_many_arguments)]
pub fn planet_swap(
    mut commands: Commands,
    sys: Res<SystemRes>,
    planet: Res<PlanetRes>,
    wd: Res<WarpDrive>,
    mut pending: ResMut<PendingPlanet>,
    mut ring: ResMut<Ring>,
    ships: Query<&Position, With<Ship>>,
    tel: Option<ResMut<WarpTelemetry>>,
) {
    let sys = &sys.0;
    let Ok(pos) = ships.single() else { return };
    let pending = pending.as_mut();
    let dropped = wd.events.contains(&Event::DroppedOut);
    // Scripted runs go faster than real time: there the generation gets the wall time a real
    // flight gives it before the ship reaches its zone or drops out, so the swap lands on the
    // same tick on every machine. The waiting itself is tested in `warp_core` (`Loader`).
    if tel.is_some() && let Pending::Running(p) = pending.state() && (dropped || sys.frame_of(pos.0) == Some(p)) {
        pending.finish();
    }
    let state = pending.state();
    let f = match pending.loader.step(sys, planet.id, pos.0, &wd.drive, dropped, state) {
        Load::Keep => return,
        Load::Start(t) => return pending.start(t, sys.planet(t).clone()),
        Load::Discard => {
            pending.task = None;
            return;
        }
        Load::Swap(f) => f,
    };
    let dropped = sys.frame_of(pos.0).is_none();
    let (new, roots) = pending.take(f, sys);
    if let Some(chunks) = roots {
        commands.insert_resource(PrebuiltRoots { planet: f, chunks });
    }
    swap_planet(&mut commands, &mut ring, new);
    if let Some(mut tel) = tel {
        println!("warp {:7.2} s: planet {} -> {f} ({}){}", tel.clock, planet.id, sys.planet(f).name, if dropped { " at the drop point" } else { "" });
        let clock = tel.clock;
        tel.swaps.push((clock, planet.id, f, dropped));
    }
}

pub fn warp_telemetry(time: Res<Time>, wd: Res<WarpDrive>, mut tel: ResMut<WarpTelemetry>, ships: Query<&Position, With<Ship>>) {
    let tel = tel.as_mut();
    tel.clock += time.delta_secs_f64();
    let Ok(pos) = ships.single() else { return };
    for ev in &wd.events {
        tel.log.push((tel.clock, *ev));
        println!("warp {:7.2} s: {ev:?}", tel.clock);
        if matches!(ev, Event::Arrived | Event::DroppedOut) {
            tel.warps += (*ev == Event::Arrived) as u32;
            let end = if *ev == Event::Arrived { wd.drive.exit() } else { wd.drive.drop_point() };
            tel.end_error = end.map(|p| p.distance(pos.0));
        }
    }
    if let Some(last) = tel.last_pos {
        tel.max_tick_move = tel.max_tick_move.max(pos.0.distance(last));
    }
    tel.last_pos = Some(pos.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(seed: u64, kind: campaign_core::SystemKind) -> Options {
        Options { system_seed: Some(seed), system_kind: kind, ..Options::default() }
    }

    fn planets(sys: &System) -> Vec<(String, i32, [f64; 3])> {
        sys.planets.iter().map(|p| (p.name.clone(), p.seed, p.centre)).collect()
    }

    #[test]
    fn without_a_seed_the_system_is_the_file() {
        let sys = build_system(&Options::default());
        let file = System::from_json(SYSTEM).unwrap();
        assert_eq!(planets(&sys), planets(&file));
    }

    #[test]
    fn generated_systems_are_valid_and_pass_the_game_checks() {
        let dn = daynight_core::DayNight::from_json(crate::daynight::DAYNIGHT).unwrap();
        for kind in campaign_core::SystemKind::ALL {
            for seed in 0..60 {
                let sys = build_system(&seeded(seed, kind));
                sys.validate().unwrap_or_else(|e| panic!("{kind:?} seed {seed}: {e}"));
                for p in &sys.planets {
                    assert!(crate::env::RECIPES.iter().any(|(id, _)| *id == p.recipe), "{kind:?} seed {seed}: recipe {}", p.recipe);
                    dn.check_geometry(&p.recipe, p.centre(), sys.star.position()).unwrap_or_else(|e| panic!("{kind:?} seed {seed}: {e}"));
                }
            }
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_planets() {
        for kind in campaign_core::SystemKind::ALL {
            for seed in [1, 7, 42] {
                assert_eq!(planets(&build_system(&seeded(seed, kind))), planets(&build_system(&seeded(seed, kind))));
            }
        }
    }
}
