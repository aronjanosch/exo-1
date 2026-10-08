//! The quantum drive in the game (spike 11): the `warp_core` state machine driven by the piloted
//! ship, the ship carried along the path, the planets swapped when the ship enters another
//! planet's frame zone. Real movement through the shared f64 world, no loading screen.
//!
//! J starts a warp to the selected planet (N selects) and cancels while spooling or calibrating.
use crate::controls::Controls;
use crate::env::PlanetRes;
use crate::ring::Ring;
use crate::ship::{RemoteShip, Ship};
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use bevy::tasks::{block_on, AsyncComputeTaskPool, Task};
use warp_core::{Abort, Drive, Event, Obstacle, Phase, ShipView, System};

pub const SYSTEM: &str = include_str!("../../../content/system/system.json");

/// Radius of the sphere other ships block the path with (m).
const SHIP_OBSTACLE_RADIUS: f64 = 20.0;
/// While on rails the nose turns to the course at most this fast (rad/s).
const RAILS_TURN_RATE: f64 = 0.6;

#[derive(Resource)]
pub struct SystemRes(pub System);

#[derive(Resource)]
pub struct WarpDrive {
    pub drive: Drive,
    /// Planet selected as the target (N).
    pub selected: usize,
    /// Why the last start was refused or the last warp aborted.
    pub last_abort: Option<Abort>,
    /// Every event with the simulated time it happened (scenario reports and HUD).
    pub log: Vec<(f64, Event)>,
    pub clock: f64,
    pub warps: u32,
    /// Largest distance the ship moved in one tick (m).
    pub max_tick_move: f64,
    /// Obstacles added by hand (scenarios), besides remote ships.
    pub extra_obstacles: Vec<Obstacle>,
    last_pos: Option<DVec3>,
}

impl WarpDrive {
    pub fn new(sys: &System) -> WarpDrive {
        WarpDrive {
            drive: Drive::new(sys.drive.clone()),
            selected: 1,
            last_abort: None,
            log: Vec::new(),
            clock: 0.0,
            warps: 0,
            max_tick_move: 0.0,
            extra_obstacles: Vec::new(),
            last_pos: None,
        }
    }
}

/// The target planet being generated in the background while the ship flies.
#[derive(Resource, Default)]
pub struct PendingPlanet {
    pub id: usize,
    task: Option<Task<(PlanetRes, f64)>>,
    /// Wall-clock time the last background generation took (ms).
    pub gen_ms: Option<f64>,
}

impl PendingPlanet {
    pub fn start(&mut self, id: usize, def: warp_core::PlanetDef) {
        if self.task.is_some() && self.id == id {
            return;
        }
        self.id = id;
        self.gen_ms = None;
        self.task = Some(AsyncComputeTaskPool::get().spawn(async move {
            let t0 = std::time::Instant::now();
            let p = PlanetRes::load_def(id, &def);
            (p, t0.elapsed().as_secs_f64() * 1000.0)
        }));
    }

    pub fn ready(&self) -> bool {
        self.task.as_ref().is_some_and(|t| t.is_finished())
    }

    /// The finished planet, or generates it here and now (a teleport has no flight to hide it in).
    fn take(&mut self, id: usize, sys: &System) -> PlanetRes {
        match self.task.take() {
            Some(t) if self.id == id => {
                let (p, ms) = block_on(t);
                self.gen_ms = Some(ms);
                p
            }
            _ => PlanetRes::load_def(id, &sys.planets[id]),
        }
    }
}

/// Swaps the simulation's planet: the resource, and the collision ring (its patches belong to
/// the old planet). The view rebuilds its terrain when it sees the id change.
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
    let angle = nose.angle_between(dir);
    if angle < 1e-9 {
        return from;
    }
    let turn = DQuat::from_rotation_arc(nose, dir);
    DQuat::IDENTITY.slerp(turn, (max_angle / angle).min(1.0)) * from
}

#[allow(clippy::too_many_arguments)]
pub fn warp_step(
    mut commands: Commands,
    time: Res<Time>,
    sys: Res<SystemRes>,
    planet: Res<PlanetRes>,
    mut wd: ResMut<WarpDrive>,
    mut pending: ResMut<PendingPlanet>,
    mut controls: ResMut<Controls>,
    mut ring: ResMut<Ring>,
    mut ships: Query<(Entity, &mut Ship, &mut Position, &mut Rotation, &mut LinearVelocity, &mut AngularVelocity)>,
    remotes: Query<&Position, (With<RemoteShip>, Without<Ship>)>,
) {
    let dt = time.delta_secs_f64();
    let sys = &sys.0;
    let wd = wd.as_mut();
    wd.clock += dt;
    let Ok((e, mut ship, mut pos, mut rot, mut lv, mut av)) = ships.single_mut() else { return };

    let n = sys.planets.len();
    if controls.take_tap(KeyCode::KeyN) {
        wd.selected = (wd.selected + 1) % n;
    }
    let mut obstacles: Vec<Obstacle> = remotes.iter().map(|p| Obstacle { centre: p.0, radius: SHIP_OBSTACLE_RADIUS }).collect();
    obstacles.extend(wd.extra_obstacles.iter().copied());
    let view = ShipView { pos: pos.0, forward: rot.0 * DVec3::NEG_Z, speed: lv.0.length() };

    let mut events = Vec::new();
    let j = controls.take_tap(KeyCode::KeyJ);
    if j && !ship.piloted {
        println!("warp: J ignored, nobody is piloting");
    }
    if j && ship.piloted {
        if wd.drive.phase == Phase::Idle {
            let target = if wd.selected == sys.nearest(pos.0) { (wd.selected + 1) % n } else { wd.selected };
            match wd.drive.begin(target, &view, sys, &obstacles) {
                Ok(()) => {
                    wd.last_abort = None;
                    events.push(Event::Phase(Phase::Spooling));
                }
                Err(why) => {
                    wd.last_abort = Some(why);
                    events.push(Event::Aborted(why));
                }
            }
        } else if let Some(ev) = wd.drive.cancel() {
            events.push(ev);
        }
    }
    events.extend(wd.drive.step(dt, &view, sys, &obstacles));

    for ev in &events {
        let t = wd.clock;
        wd.log.push((t, *ev));
        match ev {
            Event::Aborted(why) => wd.last_abort = Some(*why),
            // Start generating the target while the ship still ramps up.
            Event::Phase(Phase::RampUp) => {
                if let Some(t) = wd.drive.target {
                    pending.start(t, sys.planets[t].clone());
                    commands.entity(e).remove::<SweptCcd>();
                }
            }
            Event::Phase(Phase::PostRampDown) => {
                commands.entity(e).insert(SweptCcd::default());
            }
            Event::Arrived => wd.warps += 1,
            _ => {}
        }
        if matches!(ev, Event::Phase(_) | Event::Aborted(_) | Event::Arrived) {
            println!("warp {:7.2} s: {ev:?}", t);
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
            // Hand-over at the exit point: the pilot gets the exit velocity back.
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
    if let Some(last) = wd.last_pos {
        wd.max_tick_move = wd.max_tick_move.max(pos.0.distance(last));
    }
    wd.last_pos = Some(pos.0);

    // The ship comes into another planet's frame zone: that planet becomes the simulation's.
    if let Some(f) = sys.frame_of(pos.0)
        && f != planet.id
    {
        let new = pending.take(f, sys);
        println!("warp {:7.2} s: planet {} -> {} ({})", wd.clock, planet.id, f, sys.planets[f].name);
        swap_planet(&mut commands, &mut ring, new);
    }
}
