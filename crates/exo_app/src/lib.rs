//! EXO-1 game crate: Bevy glue around planet_core, flight_core, walker_core and net_core.
//! Rendering, input, camera, HUD, physics bodies, network transport and scripted scenarios.
//! All game values are the spike test values, not designed.
pub mod controls;
pub mod env;
pub mod net;
pub mod net_live;
pub mod origin;
pub mod record;
pub mod ring;
pub mod scenario;
pub mod ship;
pub mod terrain;
pub mod view;
pub mod walker;
pub mod warp;

use avian3d::prelude::*;
use avian3d::physics_transform::PhysicsTransformConfig;
use bevy::app::ScheduleRunnerPlugin;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use std::path::PathBuf;
use std::time::Duration;

#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum Layer {
    #[default]
    World,
    Ship,
    Ramp,
    /// Hull of another player's ship (proxy): the walker stands on it, ships do not hit it.
    Remote,
}

/// Sideways spawn offset in metres (players of one network session start 20 m apart).
#[derive(Resource)]
pub struct SpawnOffset(pub f64);

/// One physics tick at 60 Hz.
pub const TICK: Duration = Duration::from_nanos(16_666_667);

#[derive(Clone, Debug)]
pub struct Options {
    pub scenario: Option<String>,
    pub headless: bool,
    /// Window not shown (screenshots without any visible window), if the platform renders it.
    pub hidden: bool,
    pub radius: f64,
    /// Render-origin shift threshold in metres, 0 = off.
    pub origin_shift: f64,
    /// Scenario reports and screenshots.
    pub out_dir: PathBuf,
    /// Write the scripted run's ship and walker path (net_core Trajectory) to this file.
    pub record: Option<PathBuf>,
    pub spawn_offset: f64,
    /// Distance between the planet centres in metres (default: `content/system/system.json`).
    pub distance: Option<f64>,
    /// Headless run paced at 60 physics ticks per wall second (network runs).
    pub realtime: bool,
    pub net: Option<net_live::NetConfig>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            scenario: None,
            headless: false,
            hidden: false,
            radius: 5000.0,
            origin_shift: 1000.0,
            out_dir: PathBuf::from("target/scenario"),
            record: None,
            spawn_offset: 0.0,
            distance: None,
            realtime: false,
            net: None,
        }
    }
}

impl Options {
    pub fn from_args(args: impl Iterator<Item = String>) -> Options {
        let mut o = Options::default();
        let mut net_args: Vec<(String, String)> = Vec::new();
        for a in args {
            let (k, v) = a.split_once('=').unwrap_or((a.as_str(), ""));
            match k {
                "--net-host" | "--net-connect" | "--bind" | "--port" | "--slot" | "--rate" | "--buffer" | "--extrapolate" | "--bot" => {
                    net_args.push((k.to_string(), v.to_string()))
                }
                "--scenario" => o.scenario = Some(v.to_string()),
                "--headless" => o.headless = true,
                "--hidden" => o.hidden = true,
                "--distance" => o.distance = Some(v.parse().expect("distance")),
                "--radius" => o.radius = v.parse().expect("radius"),
                "--origin-shift" => o.origin_shift = v.parse().expect("origin-shift"),
                "--out" => o.out_dir = PathBuf::from(v),
                "--record" => o.record = Some(PathBuf::from(v)),
                _ => panic!("unknown argument {a}"),
            }
        }
        o.net = net_live::NetConfig::parse(&net_args);
        if let Some(n) = &mut o.net {
            n.headless = o.headless;
            o.spawn_offset = (n.slot as f64 - 1.0) * 20.0;
            o.realtime = true;
            if n.bot {
                o.scenario = Some("net".into());
            }
        }
        o
    }
}

pub fn build_app(o: &Options) -> App {
    let mut app = App::new();
    if o.headless {
        app.add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(if o.realtime { TICK } else { Duration::ZERO })),
            TransformPlugin,
            bevy::asset::AssetPlugin::default(),
            bevy::mesh::MeshPlugin,
        ));
        // Each update is exactly one physics tick, independent of wall time.
        app.insert_resource(TimeUpdateStrategy::ManualDuration(TICK));
    } else {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "EXO-1".into(), resolution: (1600u32, 900u32).into(), visible: !o.hidden, ..default() }),
            ..default()
        }));
    }
    app.insert_resource(Time::<Fixed>::from_duration(TICK));
    app.add_plugins(PhysicsPlugins::default())
        .insert_resource(Gravity(DVec3::ZERO))
        .insert_resource(PhysicsTransformConfig { transform_to_position: false, position_to_transform: false, ..default() });

    let mut sys = warp_core::System::from_json(warp::SYSTEM).expect("system.json");
    if let Some(d) = o.distance {
        sys.set_distance(d);
    }
    // --radius sets the first planet's size, as before the registry.
    sys.planets[0].radius = o.radius;
    let planet = env::PlanetRes::load_def(0, &sys.planets[0]);
    println!("planet: radius {} m, sea {:.2} m, bake {:.0} ms", planet.radius, planet.sea, planet.bake_ms);
    let start = planet.centre + DVec3::Y * planet.surface(DVec3::Y);
    app.insert_resource(origin::RenderOrigin {
        // Without shifting the origin stays at the world origin.
        origin: if o.origin_shift > 0.0 { start.round() } else { DVec3::ZERO },
        threshold: o.origin_shift,
        shifts: 0,
        view: start,
    });
    app.insert_resource(ring::Ring::new(planet.radius));
    app.insert_resource(planet);
    app.insert_resource(warp::WarpDrive::new(&sys)).init_resource::<warp::PendingPlanet>().insert_resource(warp::SystemRes(sys));
    app.init_resource::<controls::Controls>().init_resource::<walker::WalkStats>();
    app.add_plugins(origin::plugin);
    app.insert_resource(SpawnOffset(o.spawn_offset));
    app.add_systems(Startup, |mut commands: Commands, planet: Res<env::PlanetRes>, off: Res<SpawnOffset>| {
        walker::spawn_player(&mut commands, &planet, off.0);
        ship::spawn_ship(&mut commands, &planet, DVec3::Y, off.0);
    });
    app.add_systems(FixedUpdate, (scenario::run_script.run_if(resource_exists::<scenario::Script>), warp::warp_step, ship::ship_control, walker::walker_step).chain());
    app.add_systems(Update, ring::update_ring);
    if let Some(path) = &o.record {
        app.insert_resource(record::Recorder::new(path.clone()));
        app.add_systems(FixedLast, record::record_tick);
    }

    if !o.headless {
        app.init_resource::<view::ViewState>().init_resource::<terrain::TerrainSwaps>().insert_resource(ClearColor(Color::BLACK));
        app.add_systems(Startup, (terrain::setup_terrain, view::setup_view));
        app.add_systems(Startup, view::setup_warp_view.after(view::setup_view));
        app.add_systems(FixedLast, view::record_player_view);
        app.add_systems(
            Update,
            (controls::read_input, view::add_ship_visuals, view::add_remote_walker_visuals, view::update_camera, terrain::update_terrain, view::update_impostors, view::update_tunnel, view::update_hud).chain().after(ring::update_ring),
        );
    }
    if let Some(cfg) = &o.net {
        app.insert_resource(net_live::Net::new(cfg.clone()));
        app.add_systems(FixedUpdate, net_live::net_pre.before(ship::ship_control));
        app.add_systems(FixedLast, net_live::net_post);
    }
    if o.scenario.as_deref() == Some("foreign") {
        // Ahead of the controllers like net_pre: the remote ship is placed before the walker steps.
        app.add_systems(FixedUpdate, scenario::foreign_drive.run_if(resource_exists::<scenario::ForeignDriver>).before(ship::ship_control));
    }
    if let Some(name) = &o.scenario {
        app.world_mut().resource_mut::<controls::Controls>().scripted = true;
        app.insert_resource(scenario::Script {
            name: name.clone(),
            steps: scenario::build(name, &o.out_dir, !o.headless),
            i: 0,
            ctx: Default::default(),
            out_dir: o.out_dir.clone(),
            done: false,
        });
    }
    app
}
