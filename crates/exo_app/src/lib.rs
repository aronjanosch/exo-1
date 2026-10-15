//! EXO-1 game crate: Bevy glue around planet_core, flight_core, walker_core and net_core.
//! Rendering, input, camera, HUD, physics bodies, network transport and scripted scenarios.
//! All game values are the spike test values, not designed.
pub mod audio;
pub mod audio_slice;
pub mod avian_crates;
pub mod cargo;
pub mod controls;
pub mod crew_hud;
pub mod daynight;
pub mod dev;
pub mod dev_area;
pub mod effects;
pub mod displays;
pub mod env;
pub mod flight_events;
pub mod frame_cost;
pub mod gameplay;
pub mod grab;
pub mod help;
pub mod hot_reload;
pub mod hud;
pub mod interact;
pub mod items;
pub mod look;
pub mod map;
pub mod named_material;
pub mod menu;
pub mod net;
pub mod net_live;
pub mod health;
pub mod health_view;
pub mod items_net;
pub mod notices;
pub mod origin;
pub mod perf;
pub mod phases;
pub mod record;
pub mod repair;
pub mod rescue;
pub mod public_status;
pub mod repair_hud;
pub mod ring;
pub mod savefile;
pub mod scatter;
pub mod scenario;
pub mod settings;
pub mod sites;
pub mod sky;
pub mod ship;
pub mod terrain;
pub mod terrain_material;
pub mod ship_showcase;
pub mod town;
pub mod tuning;
pub mod ui_theme;
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
    /// Crates that are Avian bodies: the walker, other crates and the hull hit them, ships do not.
    Crate,
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
    /// Present without vsync (frame time measurements; with vsync a frame takes the display's period).
    pub no_vsync: bool,
    /// Radius of the first planet in metres (default: `content/system/system.json`).
    pub radius: Option<f64>,
    /// Start at this place's main pad instead of the pole (round 6: `--start drip_rock`, the
    /// city's spaceport): the ship parks on the pad, the walker beside it.
    pub start: Option<String>,
    /// Atmosphere height of the first planet in metres (default: the ceilings rule for the radius
    /// with `--radius`, else the file's).
    pub atmosphere: Option<f64>,
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
    /// `--all-planets`: `--radius`/`--atmosphere` apply to every planet, not only the first
    /// (`planet-look` of both planets at one size, #214).
    pub all_planets: bool,
    /// `--spike-ms=`: frames slower than this print a `spike:` line (#213).
    pub spike_ms: f64,
    /// `--mute`: no sound (windowed measurement runs).
    pub mute: bool,
    pub net: Option<net_live::NetConfig>,
    /// `--perf`: step and frame timings per phase (#19).
    pub perf: Option<perf::PerfOptions>,
    /// Dev builds: the tuning files to watch (default the repo's `content/tuning`).
    pub tuning_dir: Option<PathBuf>,
    /// Player settings and user bindings (default `settings/`).
    pub settings_dir: Option<PathBuf>,
    /// `--menu`: the menus also in a scripted windowed run (screenshots of them).
    pub force_menu: bool,
    /// `--swap-rounds=<n>`: planet swaps by warp in the `swap` scenario (default and least 3).
    pub swap_rounds: usize,
    /// `--dev-area`: spawn the dev area's devices wherever the run starts; `--start=dev_area` does
    /// it too.
    pub dev_area: bool,
    /// `--warm-reactor`: the engine room is warm-started once when it is created (`warm_start`).
    pub warm_reactor: bool,
    /// `--showcase`: ship models with colliders next to the spawn, for a walk test (window only).
    pub showcase: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            scenario: None,
            headless: false,
            hidden: false,
            no_vsync: false,
            radius: None,
            start: None,
            atmosphere: None,
            // #214: 4 km keeps f32 render positions within about 0.5 mm and shifts a quarter as
            // often as 1 km did (each shift touches every mesh instance on the render side).
            origin_shift: 4000.0,
            out_dir: PathBuf::from("target/scenario"),
            record: None,
            spawn_offset: 0.0,
            distance: None,
            realtime: false,
            all_planets: false,
            spike_ms: frame_cost::DEFAULT_THRESHOLD_MS,
            mute: false,
            net: None,
            perf: None,
            tuning_dir: None,
            settings_dir: None,
            force_menu: false,
            swap_rounds: 3,
            dev_area: false,
            warm_reactor: false,
            showcase: false,
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
                "--net-host" | "--net-connect" | "--bind" | "--port" | "--slot" | "--rate" | "--buffer" | "--extrapolate" | "--bot" | "--sim-delay" | "--sim-jitter" | "--sim-loss" => {
                    net_args.push((k.to_string(), v.to_string()))
                }
                "--scenario" => o.scenario = Some(v.to_string()),
                "--headless" => o.headless = true,
                "--hidden" => o.hidden = true,
                "--no-vsync" => o.no_vsync = true,
                "--realtime" => o.realtime = true,
                "--all-planets" => o.all_planets = true,
                "--mute" => o.mute = true,
                "--spike-ms" => o.spike_ms = v.parse().expect("spike-ms"),
                "--distance" => o.distance = Some(v.parse().expect("distance")),
                "--radius" => o.radius = Some(v.parse().expect("radius")),
                "--start" => o.start = Some(v.to_string()),
                "--atmosphere" => o.atmosphere = Some(v.parse().expect("atmosphere")),
                "--origin-shift" => o.origin_shift = v.parse().expect("origin-shift"),
                "--out" => o.out_dir = PathBuf::from(v),
                "--record" => o.record = Some(PathBuf::from(v)),
                "--tuning-dir" => o.tuning_dir = Some(PathBuf::from(v)),
                "--settings-dir" => o.settings_dir = Some(PathBuf::from(v)),
                "--menu" => o.force_menu = true,
                "--dev-area" => o.dev_area = true,
                "--warm-reactor" => o.warm_reactor = true,
                "--showcase" => o.showcase = true,
                "--swap-rounds" => o.swap_rounds = v.parse().expect("swap-rounds"),
                "--perf" => o.perf = Some(o.perf.take().unwrap_or_default()),
                "--perf-baseline" => o.perf.get_or_insert_default().baseline = PathBuf::from(v),
                "--perf-save-baseline" => o.perf.get_or_insert_default().save_baseline = true,
                "--perf-tolerance" => o.perf.get_or_insert_default().tolerance = v.parse().expect("perf-tolerance"),
                "--perf-slow" => o.perf.get_or_insert_default().slow_ms = v.parse().expect("perf-slow"),
                _ => panic!("unknown argument {a}"),
            }
        }
        o.net = net_live::NetConfig::parse(&net_args);
        // The dev area's scenarios start there.
        if matches!(o.scenario.as_deref(), Some("dev_area" | "displays" | "items" | "health")) && o.start.is_none() {
            o.start = Some(dev_area::PLACE.into());
        }
        if let Some(p) = &mut o.perf {
            // Under vsync a frame takes the display's period, not the game's time.
            o.no_vsync = true;
            if p.baseline.as_os_str().is_empty() {
                p.baseline = PathBuf::from(format!("target/perf/baseline-{}-{}.json", o.scenario.as_deref().unwrap_or("none"), if o.headless { "headless" } else { "window" }));
            }
        }
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

impl Options {
    /// The dev area's devices are spawned.
    pub fn dev_area(&self) -> bool {
        self.dev_area || self.start.as_deref() == Some(dev_area::PLACE)
    }
    /// Players start in the menu: a window, no script, no network flags.
    pub fn menu(&self) -> bool {
        !self.headless && (self.force_menu || self.scenario.is_none() && self.net.is_none())
    }
}

/// Bevy's asset root, `content/` (props). Dev builds read the repo's folder wherever the binary
/// runs from; release builds expect `content/` next to the executable.
pub fn content_dir() -> String {
    if cfg!(debug_assertions) { concat!(env!("CARGO_MANIFEST_DIR"), "/../../content").to_string() } else { "content".to_string() }
}

/// Every `.json` file under `content/<sub>/` (one folder deep), with its path relative to
/// `content/<sub>/` (`commodity/fizzy_mud.json`), sorted. A missing folder gives none.
pub fn content_files(sub: &str) -> Vec<content_core::File> {
    let root = std::path::Path::new(&content_dir()).join(sub);
    let mut out = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(d) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && d == root {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "json") {
                let rel = p.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                out.push(content_core::File::new(rel, text));
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
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
        app.add_plugins(DefaultPlugins.set(bevy::asset::AssetPlugin { file_path: content_dir(), ..default() }).set(bevy::app::TaskPoolPlugin { task_pool_options: terrain::task_pools() }).set(WindowPlugin {
            primary_window: Some(Window { title: "EXO-1".into(), resolution: (1600u32, 900u32).into(), visible: !o.hidden,
                present_mode: if o.no_vsync { bevy::window::PresentMode::AutoNoVsync } else { bevy::window::PresentMode::AutoVsync },
                ..default()
            }),
            ..default()
        }));
    }
    app.insert_resource(Time::<Fixed>::from_duration(TICK));
    app.add_plugins(PhysicsPlugins::default())
        .insert_resource(Gravity(DVec3::ZERO))
        // The collider trees are tiny; Avian's async optimise task waits behind terrain and scatter
        // jobs in the compute pool, and the end of the step blocks on it (fixed steps of 20 to 800 ms
        // on a 100 km planet, #217).
        .insert_resource(ColliderTreeOptimization { use_async_tasks: false, ..default() })
        .insert_resource(PhysicsTransformConfig { transform_to_position: false, position_to_transform: false, ..default() });

    let mut sys = warp_core::System::from_json(warp::SYSTEM).expect("system.json");
    if let Some(d) = o.distance {
        match sys.set_distance(d) {
            Ok(notes) => notes.iter().for_each(|n| println!("--distance: {n}")),
            Err(e) => panic!("--distance refused: {e}"),
        }
    }
    // The file wins; --radius only when given (first planet, or all with --all-planets).
    // A radius or an atmosphere from the command line brings the ceilings that follow from it
    // (obstruction and arrival radius, #177) with it, or the warp checks refuse the planet.
    let n_planets = if o.all_planets { sys.planets.len() } else { 1 };
    for p in sys.planets.iter_mut().take(n_planets).filter(|_| o.radius.is_some() || o.atmosphere.is_some()) {
        let r = o.radius.unwrap_or(p.radius);
        let c = match o.atmosphere {
            Some(a) => planet_core::Ceilings::with_atmosphere(r, a),
            None => planet_core::Ceilings::for_radius(r),
        };
        println!("ceilings: radius {r} m, atmosphere {:.0} m, tallest terrain {:.0} m, obstruction {:.0} m, arrival {:.0} m", c.atmosphere_height, c.terrain_max, c.obstruction_radius, c.arrival_radius);
        p.radius = r;
        p.atmosphere_height = c.atmosphere_height;
        p.obstruction_radius = c.obstruction_radius;
        p.arrival_radius = c.arrival_radius;
        p.terrain_ceiling_m = Some(c.terrain_max);
        if let Err(e) = p.validate() {
            panic!("--radius/--atmosphere refused: {e}");
        }
    }
    let home = warp_core::PlanetId(0);
    let planet = env::PlanetRes::load(home, sys.planet(home));
    println!("planet: radius {} m, sea {:.2} m, bake {:.0} ms", planet.radius, planet.sea, planet.bake_ms);
    // The start direction: the pole, or 15 m south of a place's main pad, so the ship (parked 15 m
    // ahead of the walker) stands on the pad.
    let start_up = match &o.start {
        None => DVec3::Y,
        Some(place) => {
            let pad = planet.pgen.pad(place, "main").unwrap_or_else(|| panic!("--start: no place '{place}' with a pad 'main' on this planet"));
            (env::from_v3(pad.up) * planet.radius + DVec3::new(0.0, 0.0, 15.0)).normalize()
        }
    };
    let start = planet.centre + start_up * planet.surface(start_up);
    app.insert_resource(origin::RenderOrigin {
        // Without shifting the origin stays at the world origin.
        origin: if o.origin_shift > 0.0 { start.round() } else { DVec3::ZERO },
        threshold: o.origin_shift,
        shifts: 0,
        view: start,
    });
    app.insert_resource(ring::Ring::new(planet.radius));
    app.insert_resource(planet);
    let net = o.net.as_ref().map(|cfg| net_live::Net::new(cfg.clone(), &sys));
    app.insert_resource(warp::WarpDrive::new(&sys)).init_resource::<warp::PendingPlanet>().insert_resource(warp::SystemRes(sys));
    app.init_resource::<controls::Controls>().init_resource::<controls::Actions>().init_resource::<controls::Bindings>().init_resource::<walker::WalkStats>();
    app.insert_resource(tuning::Tuning::load());
    // Settings and user bindings belong to players; scripted and headless runs keep the defaults.
    let settings_dir = o.settings_dir.clone().unwrap_or_else(settings::SettingsDir::default_dir);
    if o.scenario.is_none() && !o.headless {
        let (s, b, notes) = settings::load(&settings_dir);
        notes.iter().for_each(|n| println!("settings: {n}"));
        app.insert_resource(s).insert_resource(b);
    } else {
        app.init_resource::<settings::Settings>();
    }
    if o.mute {
        app.world_mut().resource_mut::<settings::Settings>().sound = false;
    }
    app.insert_resource(settings::SettingsDir(settings_dir));
    app.insert_resource(SpawnOffset(o.spawn_offset));
    if let Some(path) = &o.record {
        app.insert_resource(record::Recorder::new(path.clone()));
        app.add_systems(FixedLast, record::record_tick);
    }
    if let Some(net) = net {
        app.insert_resource(net);
    }
    if let Some(name) = &o.scenario {
        app.world_mut().resource_mut::<controls::Controls>().scripted = true;
        if scenario::uses_direct_mouse(name) {
            app.world_mut().resource_mut::<controls::Bindings>().mouse.ship_mode = controls::ShipMouse::Direct;
        }
        app.init_resource::<warp::WarpTelemetry>();
        if let Some(p) = &o.perf {
            app.insert_resource(perf::Perf::new(p.clone(), !o.headless));
            app.add_plugins(perf::plugin);
        }
        app.insert_resource(scenario::Script {
            name: name.clone(),
            steps: scenario::build(name, &o.out_dir, !o.headless, o.swap_rounds),
            i: 0,
            ctx: Default::default(),
            out_dir: o.out_dir.clone(),
            done: false,
        });
    }

    // Domain plugins. The order between phases is in `phases`; inside a phase each plugin orders its own systems.
    app.insert_resource(frame_cost::FrameSpikes::with_threshold(o.spike_ms));
    app.add_plugins(frame_cost::plugin);
    app.add_plugins((phases::plugin, hot_reload::plugin(o.tuning_dir.clone().unwrap_or_else(hot_reload::HotReload::source_dir)), origin::plugin, daynight::plugin, ring::plugin));
    app.add_plugins((controls::plugin, warp::plugin, ship::plugin, interact::plugin, walker::plugin, grab::plugin, avian_crates::plugin, cargo::plugin, hud::plugin, net_live::plugin));
    app.add_plugins((gameplay::plugin, town::plugin, displays::plugin, items::plugin, crew_hud::plugin, items_net::plugin, health::plugin, repair::plugin, public_status::plugin, health_view::plugin));
    if o.warm_reactor {
        app.insert_resource(dev_area::reactor::WarmAtCreation);
    }
    if o.dev_area() {
        app.add_plugins(dev_area::plugin);
    }
    // The save (#135) belongs to players: scripted and headless runs keep theirs in the output
    // folder and start new.
    let player_run = o.scenario.is_none() && !o.headless;
    savefile::plugin(&mut app, if player_run { savefile::SaveDir::default_dir() } else { o.out_dir.join("saves") }, player_run);
    app.add_plugins((flight_events::plugin, map::plugin, help::plugin, dev::plugin));
    // Scenarios start with the licences earned (their pilots are not the examinees), except the
    // one about the licence itself (#169) and the dev menu's, which grants them.
    if o.scenario.as_deref().is_some_and(|n| n != "licence" && n != "dev-menu") {
        app.add_systems(Startup, gameplay::grant_starting_licences);
    }
    if let Some(name) = &o.scenario {
        app.add_plugins(scenario::plugin(name, o.headless));
    }
    let test_crates = o.scenario.is_none() || o.scenario.as_deref() == Some("full");
    app.add_systems(Startup, move |mut commands: Commands, planet: Res<env::PlanetRes>, tuning: Res<tuning::Tuning>, off: Res<SpawnOffset>, crates: Res<cargo::Crates>| {
        walker::spawn_player_at(&mut commands, &planet, &tuning.walker, start_up, off.0);
        let ship = ship::spawn_ship(&mut commands, &planet, &tuning, start_up, off.0);
        if test_crates {
            // The first object (#80): a test crate on the cabin floor, behind the seat on the right.
            let t = &crates.0;
            commands.spawn(cargo::crate_bundle(t, "small", Some(ship), cargo::cabin_floor_pos(t, "small", 1.2, 1.5), DVec3::NEG_Z));
        }
    });
    if !o.headless {
        app.add_plugins((view::plugin, controls::window_plugin, settings::window_plugin, terrain::plugin, daynight::window_plugin, grab::window_plugin, cargo::window_plugin));
        app.add_plugins((audio::plugin, scatter::plugin, terrain_material::plugin, sky::plugin, sites::plugin, town::window_plugin, named_material::window_plugin, gameplay::window_plugin, notices::window_plugin, map::window_plugin, help::window_plugin, dev::window_plugin));
        app.add_plugins((displays::window_plugin, items::window_plugin, crew_hud::window_plugin, items_net::window_plugin, health::window_plugin, repair_hud::window_plugin, effects::window_plugin, public_status::window_plugin, health_view::window_plugin));
        if o.dev_area() {
            app.add_plugins(dev_area::window_plugin);
        }
        if o.menu() {
            app.add_plugins(menu::plugin);
        }
        if o.showcase || o.scenario.as_deref() == Some("showcase") {
            app.insert_resource(ship_showcase::ShowcaseStart(start_up));
            app.add_plugins(ship_showcase::plugin);
        }
    } else if o.scenario.as_deref() == Some("swap") {
        // #14: the terrain too, headless, to count what a swap leaves behind.
        app.init_asset::<StandardMaterial>().init_asset::<terrain_material::TerrainMaterial>().init_asset::<sky::WaterMaterial>();
        app.add_plugins(terrain::plugin);
    } else if o.scenario.as_deref() == Some("planet-flight") {
        // #213: the streaming work headless (terrain, scatter, sites), to measure its main-thread cost.
        app.init_asset::<StandardMaterial>().init_asset::<terrain_material::TerrainMaterial>().init_asset::<sky::WaterMaterial>();
        // City pieces of the settlements are glTF scenes (#223).
        app.init_asset::<bevy::world_serialization::WorldAsset>();
        app.add_plugins((terrain::plugin, scatter::plugin, sites::plugin));
    }
    app
}
