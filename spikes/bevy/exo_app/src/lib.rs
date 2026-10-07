//! EXO-1 spike 9: the Godot spikes 1, 3, 5, 8 rebuilt on Bevy + Avian. Simulation glue here,
//! logic in planet_core, flight_core and walker_core.
pub mod controls;
pub mod env;
pub mod origin;
pub mod ring;
pub mod scenario;
pub mod ship;
pub mod terrain;
pub mod view;
pub mod walker;

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
}

/// One physics tick at 60 Hz (Godot's default, used by all spikes).
pub const TICK: Duration = Duration::from_nanos(16_666_667);

#[derive(Clone, Debug)]
pub struct Options {
    pub scenario: Option<String>,
    pub headless: bool,
    pub radius: f64,
    pub planet_offset: DVec3,
    /// Render-origin shift threshold in metres, 0 = off.
    pub origin_shift: f64,
    pub out_dir: PathBuf,
}

impl Default for Options {
    fn default() -> Self {
        Options { scenario: None, headless: false, radius: 5000.0, planet_offset: DVec3::ZERO, origin_shift: 1000.0, out_dir: PathBuf::from("results") }
    }
}

impl Options {
    pub fn from_args(args: impl Iterator<Item = String>) -> Options {
        let mut o = Options::default();
        for a in args {
            let (k, v) = a.split_once('=').unwrap_or((a.as_str(), ""));
            match k {
                "--scenario" => o.scenario = Some(v.to_string()),
                "--headless" => o.headless = true,
                "--radius" => o.radius = v.parse().expect("radius"),
                "--origin-shift" => o.origin_shift = v.parse().expect("origin-shift"),
                "--out" => o.out_dir = PathBuf::from(v),
                "--planet-offset" => {
                    let c: Vec<f64> = v.split(',').map(|x| x.parse().expect("offset")).collect();
                    o.planet_offset = DVec3::new(c[0], c[1], c[2]);
                }
                _ => panic!("unknown argument {a}"),
            }
        }
        o
    }
}

pub fn build_app(o: &Options) -> App {
    let mut app = App::new();
    if o.headless {
        app.add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::ZERO)),
            TransformPlugin,
            bevy::asset::AssetPlugin::default(),
            bevy::mesh::MeshPlugin,
        ));
        // Each update is exactly one physics tick, independent of wall time.
        app.insert_resource(TimeUpdateStrategy::ManualDuration(TICK));
    } else {
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "EXO-1 spike 9 (Bevy)".into(), resolution: (1600u32, 900u32).into(), ..default() }),
            ..default()
        }));
    }
    app.insert_resource(Time::<Fixed>::from_duration(TICK));
    app.add_plugins(PhysicsPlugins::default())
        .insert_resource(Gravity(DVec3::ZERO))
        .insert_resource(PhysicsTransformConfig { transform_to_position: false, position_to_transform: false, ..default() });

    let t0 = std::time::Instant::now();
    let planet = env::PlanetRes::load(o.radius, o.planet_offset);
    println!("planet: radius {} m, sea {:.2} m, bake {:.0} ms (total load {:.0} ms)", planet.radius, planet.sea, planet.bake_ms, t0.elapsed().as_secs_f64() * 1000.0);
    let start = planet.centre + DVec3::Y * planet.surface(DVec3::Y);
    app.insert_resource(origin::RenderOrigin {
        // Without shifting the origin stays at the world origin (spike 5 comparison).
        origin: if o.origin_shift > 0.0 { start.round() } else { DVec3::ZERO },
        threshold: o.origin_shift,
        shifts: 0,
        shift_ms_max: 0.0,
        shift_ms_sum: 0.0,
        view: start,
    });
    app.insert_resource(ring::Ring::new(planet.radius));
    app.insert_resource(planet);
    app.init_resource::<controls::Controls>().init_resource::<walker::WalkStats>();
    app.add_plugins(origin::plugin);
    app.add_systems(Startup, |mut commands: Commands, planet: Res<env::PlanetRes>| {
        walker::spawn_player(&mut commands, &planet);
        ship::spawn_ship(&mut commands, &planet, DVec3::Y);
    });
    app.add_systems(FixedUpdate, (scenario::run_script.run_if(resource_exists::<scenario::Script>), ship::ship_control, walker::walker_step).chain());
    app.add_systems(Update, ring::update_ring);

    if !o.headless {
        app.init_resource::<view::ViewState>().insert_resource(ClearColor(Color::BLACK));
        app.add_systems(Startup, (terrain::setup_terrain, view::setup_view));
        app.add_systems(FixedLast, view::record_player_view);
        app.add_systems(
            Update,
            (controls::read_input, view::add_ship_visuals, view::update_camera, terrain::update_terrain, view::update_hud).chain().after(ring::update_ring),
        );
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
            windowed: !o.headless,
        });
    }
    app
}
