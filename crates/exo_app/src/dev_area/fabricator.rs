//! The fabricator (#326): a machine in the storage room that turns ore into parts. Rules and recipes:
//! `production_core::fabricator` and `content/gameplay/fabricator/fabricator.json`; this file is the glue.
//!
//! - F on the housing opens the container window (rack `FAB_RACK`): two input places that take ore, one
//!   output place that only gives (`SlotRule::take_only`). While it runs the inputs are locked
//!   (`Container::set_locked`); the output can be taken any time.
//! - The push button is a part of the controls kit (`Machine` with `button_start`). F on it asks the host
//!   (`FabAsk::Push`, as a client or as the host's own press); the host runs the press on the kit
//!   (`PressPart`) and starts the run when the load makes something and the output place has room.
//! - The host owns the state (`FabState`), steps it with the ship's power (the master, as the canister
//!   boxes: `equipment::box_power`) and tells everyone the face in `FabNews` (the screen, the glow, the sounds).
//! - Window: the housing's window with its glow, the button, the screen (`display_core::fabricator`), the hum,
//!   the clunk and the clicks.
use super::*;
use crate::container_window::{ContainerSpec, ContainerSpecs};
use crate::controls_kit::{apply_presses, Machine, PartSound, PressPart};
use crate::dev_area::ship::{counter_fresh, ReactorSim};
use crate::displays::{Bare, DisplayPanel, Panels, ReadOnly};
use crate::items::{Items, Role};
use crate::ship::Ship;
use controls_core::{Controls, Outcome};
use item_core::container::{Access, Container, ItemKind, SlotRule};
use item_core::wire::{FabAsk, FabMode, FabNews};
use item_core::{ItemDefs, RackId};
use production_core::fabricator::{Amount, FabConfig, FabRefusal, Fabricator};

pub const FAB_RACK: RackId = 106;
pub const HOUSING: &str = "fab_housing";
pub const NAME: &str = "Fabricator";
pub const BUTTON: &str = "button_start";
pub const SCREEN_ID: PanelId = PanelId::new("fabricator");
/// The places: two inputs, then the output.
pub const INPUTS: [usize; 2] = [0, 1];
pub const OUTPUT: usize = 2;
/// The most the output place holds; an item's own stack cap applies too.
const OUTPUT_CAP: u32 = 4;
/// Seconds between two snapshots of a changed news (4 Hz). Counters only ever go up, so a late one is not lost.
const SEND_EVERY_S: f64 = 0.25;

/// Against the storage room's west wall, facing east (turn 90), between the corridor's doorway
/// (x -0.8..0.8) and the south wall's crates. TODO(initiator): the place.
pub const AT: [f64; 2] = [-2.58, -0.3];
pub const TURN_DEG: f64 = 90.0;
/// Width, height and depth (m).
pub const SIZE: [f64; 3] = [1.2, 1.3, 0.8];
/// On the housing's front (box axes: x right, y up from the ground, z out of the front): the screen, the button
/// and the chamber's window. TODO(initiator): the arrangement.
const FRONT_Z: f64 = SIZE[2] * 0.5;
const SCREEN_SIZE: [f64; 2] = [0.24, 0.14];
const SCREEN_AT: [f64; 3] = [0.4, 1.05, FRONT_Z + 0.003];
const BUTTON_AT: [f64; 3] = [0.4, 0.78, FRONT_Z + 0.04];
const BUTTON_RADIUS: f64 = 0.07;
const WINDOW_AT: [f64; 3] = [-0.2, 0.85, FRONT_Z];
const WINDOW_SIZE: [f64; 2] = [0.62, 0.5];
/// The glow's pulse period (s) and its share at the dimmest.
const PULSE_S: f32 = 2.5;
const PULSE_LOW: f32 = 0.65;

/// The fabricator's recipes (the shipped config).
#[derive(Resource, Clone)]
pub struct FabCfg(pub FabConfig);

/// Host: the run and the counters the news carries.
#[derive(Resource)]
pub struct FabState {
    pub fab: Fabricator,
    pub pushes: u32,
    pub refusals: u32,
    pub runs: u32,
    sent: Option<FabNews>,
    since: f64,
}

/// The machine in the world: where it stands, and where its button is (the pick point).
#[derive(Component, Clone, Copy, Debug)]
pub struct FabMachine {
    pub centre: DVec3,
    pub rot: DQuat,
    pub button_at: DVec3,
}

impl FabMachine {
    /// A point in box axes (y up from the ground) in the world.
    pub fn world(&self, p: [f64; 3]) -> DVec3 {
        self.centre + self.rot * DVec3::new(p[0], p[1] - centre_y(), p[2])
    }

    /// Whether the view's ray points at the button: within its radius and `reach`.
    pub fn pick_button(&self, eye: DVec3, look: DVec3, reach: f64) -> bool {
        let look = look.normalize_or_zero();
        let d = self.button_at - eye;
        let along = d.dot(look);
        along > 0.0 && d.length() <= reach && (d - look * along).length() <= BUTTON_RADIUS
    }
}

/// Height of the box's centre over the ground (`box_pose`).
fn centre_y() -> f64 {
    (SIZE[1] - SINK) * 0.5
}

pub fn housing() -> PlaceholderBox {
    PlaceholderBox { id: HOUSING.into(), zone: layout::STORAGE_ROOM, local_pos: AT, size: SIZE, turn_deg: TURN_DEG, colour: [0.42, 0.45, 0.5] }
}

/// The rack: two input places taking ore (stacks of three), and the output place that only gives.
pub fn rack() -> Container {
    Container::new(vec![SlotRule::tagged(3, &["ore"], &[]), SlotRule::tagged(3, &["ore"], &[]), SlotRule::take_only(OUTPUT_CAP)], Access::Open)
}

/// What the input places hold, by item, and what the output place holds.
pub fn load_of(c: &Container) -> (Vec<Amount>, Option<(String, u32)>) {
    let inputs = INPUTS.iter().filter_map(|i| c.slot(*i)).map(|s| Amount::new(s.item.kind.as_str(), s.count)).collect();
    let output = c.slot(OUTPUT).map(|s| (s.item.kind.as_str().to_string(), s.count));
    (inputs, output)
}

/// The face the screen shows: the mode and the recipe that belongs to it (255: none).
pub fn mode_of(fab: &Fabricator, powered: bool, inputs: &[Amount], output: Option<(&str, u32)>) -> (FabMode, u8) {
    let index = |i: usize| i.min(254) as u8;
    let running = match fab.state() {
        production_core::fabricator::FabState::Running { recipe, .. } => Some(index(*recipe)),
        _ => None,
    };
    if !powered {
        return (FabMode::NoPower, running.unwrap_or(255));
    }
    if let Some(r) = running {
        return (FabMode::Running, r);
    }
    if let Some((item, _)) = output {
        return (FabMode::Take, fab.recipes().iter().position(|r| r.output.item == item).map_or(255, index));
    }
    if inputs.is_empty() {
        return (FabMode::LoadOre, 255);
    }
    match fab.match_recipe(inputs) {
        Some(i) => (FabMode::Ready, index(i)),
        None => (FabMode::NoRecipe, 255),
    }
}

fn name_of(defs: &ItemDefs, id: &str) -> String {
    defs.get(id).map_or(id, |d| d.name.as_str()).to_uppercase()
}

/// The screen's face from the news (the same on the host and on a client).
pub fn face_of(n: &FabNews, cfg: &FabConfig, defs: &ItemDefs) -> display_core::fabricator::Face {
    use display_core::fabricator::Face;
    let recipe = cfg.recipes.get(n.recipe as usize);
    match (n.mode, recipe) {
        (FabMode::NoPower, _) => Face::NoPower,
        (FabMode::Ready, Some(r)) => {
            let from = r.inputs.iter().map(|a| format!("{} {}", a.count, name_of(defs, &a.item))).collect::<Vec<_>>().join(" + ");
            Face::Ready { from, to: name_of(defs, &r.output.item) }
        }
        (FabMode::Running, Some(r)) => {
            let left = n.left_ds as f64 / 10.0;
            Face::Running { product: name_of(defs, &r.output.item), fraction: 1.0 - left / r.seconds, left_s: left }
        }
        (FabMode::Take, Some(r)) => Face::Take { product: name_of(defs, &r.output.item) },
        (FabMode::NoRecipe, _) => Face::NoRecipe,
        _ => Face::Load,
    }
}

/// The glow of the chamber (0..1): dark unless it runs, then a slow pulse.
pub fn glow_level(mode: FabMode, t: f32) -> f32 {
    if mode != FabMode::Running {
        return 0.0;
    }
    PULSE_LOW + (1.0 - PULSE_LOW) * 0.5 * (1.0 + (t * std::f32::consts::TAU / PULSE_S).sin())
}

pub fn plugin(app: &mut App) {
    let cfg = FabConfig::load(&crate::content_files("gameplay")).unwrap_or_else(|e| panic!("fabricator: {e:?}"));
    app.init_resource::<ContainerSpecs>();
    app.world_mut().resource_mut::<ContainerSpecs>().0.push(ContainerSpec { rack: FAB_RACK, name: NAME.into(), box_id: HOUSING.into(), cols: 3 });
    app.world_mut().resource_mut::<DevAreaDevices>().add(housing());
    app.insert_resource(FabState { fab: Fabricator::new(&cfg), pushes: 0, refusals: 0, runs: 0, sent: None, since: 0.0 }).insert_resource(FabCfg(cfg));
    app.add_systems(Update, spawn_fab.after(spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(FixedUpdate, (put_rack, fab_asks.before(apply_presses), fab_react.after(apply_presses)).in_set(crate::phases::Fx::Walker));
    app.add_systems(FixedUpdate, (fab_step, write_screen).chain().in_set(crate::phases::Fx::Effects));
}

/// The machine, its button and its screen, once per planet.
fn spawn_fab(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    let (centre, rot) = box_pose(&frame, &planet, &housing());
    let mut m = FabMachine { centre, rot, button_at: DVec3::ZERO };
    m.button_at = m.world(BUTTON_AT);
    let mut controls = Controls::new();
    controls.button(BUTTON, None);
    let machine = commands.spawn((m, Machine { controls, ..default() })).id();
    let screen = commands.spawn((DisplayPanel { id: SCREEN_ID, name: NAME.to_uppercase(), size_m: DVec2::from(SCREEN_SIZE), centre: m.world(SCREEN_AT), rot }, Bare, ReadOnly)).id();
    area.entities.extend([machine, screen]);
}

/// Once, when the ship is there: the rack goes on the host's map.
fn put_rack(time: Res<Time>, mut items: ResMut<Items>, ships: Query<(), With<Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    *done = true;
    items.add_container(time.elapsed_secs_f64(), FAB_RACK, rack());
}

/// Host: a push (a client's or the host's own) becomes a press on the kit's button.
fn fab_asks(mut items: ResMut<Items>, machines: Query<Entity, With<FabMachine>>, mut out: MessageWriter<PressPart>) {
    if items.role == Role::Client {
        return;
    }
    let asks = std::mem::take(&mut items.fab.asks);
    let Ok(e) = machines.single() else { return };
    for (_, FabAsk::Push) in asks {
        out.write(PressPart { machine: e, part: BUTTON.into(), hand: None });
    }
}

/// Host: a fired button tries to start a run. Without power, without a recipe or without room in the output
/// place nothing starts and the refusal counter rises (the dull click).
#[allow(clippy::too_many_arguments)]
fn fab_react(mut sounds: MessageReader<PartSound>, machines: Query<(), With<FabMachine>>, time: Res<Time>, sim: Res<ReactorSim>, mut items: ResMut<Items>, mut st: ResMut<FabState>) {
    if items.role == Role::Client {
        return;
    }
    for s in sounds.read() {
        if !machines.contains(s.machine) || s.part != BUTTON || s.outcome != Outcome::Fired {
            continue;
        }
        st.pushes = st.pushes.wrapping_add(1);
        let Some(c) = items.host.racks.get(&FAB_RACK) else { continue };
        let (inputs, output) = load_of(c);
        let defs = &items.defs;
        let cap = |id: &str| defs.get(id).map_or(1, |d| d.stack).min(OUTPUT_CAP);
        let powered = crate::dev_area::equipment::box_power(&sim);
        let started = if powered { st.fab.start(&inputs, output.as_ref().map(|(i, n)| (i.as_str(), *n)), cap) } else { Err(FabRefusal::NoRecipe) };
        match started {
            Ok(()) => items.edit_rack(time.elapsed_secs_f64(), FAB_RACK, |c| INPUTS.iter().for_each(|i| c.set_locked(*i, true))),
            Err(why) => {
                info!("fabricator: refused ({why:?}, powered {powered})");
                st.refusals = st.refusals.wrapping_add(1);
            }
        }
    }
}

/// Host: runs the machine with the ship's power; at the end the inputs go and the product lies in the output
/// place. Then the news for the screen, the glow and the sounds.
fn fab_step(time: Res<Time>, sim: Res<ReactorSim>, mut items: ResMut<Items>, mut st: ResMut<FabState>) {
    if items.role == Role::Client {
        return;
    }
    let (dt, now) = (time.delta_secs_f64(), time.elapsed_secs_f64());
    let powered = crate::dev_area::equipment::box_power(&sim);
    if let Some(done) = st.fab.step(dt, powered) {
        st.runs = st.runs.wrapping_add(1);
        let product = items.defs.get(&done.add.item).map(|d| d.stack_of(done.add.count));
        let mut put = false;
        items.edit_rack(now, FAB_RACK, |c| {
            INPUTS.iter().for_each(|i| c.set_locked(*i, false));
            for a in &done.remove {
                let _ = c.machine_remove(&ItemKind::new(a.item.as_str()), a.count);
            }
            put = product.is_some_and(|p| c.machine_add(OUTPUT, p).is_ok());
        });
        if !put {
            warn!("fabricator: the product of {:?} did not fit the output place", done.add);
        }
    }
    let Some(c) = items.host.racks.get(&FAB_RACK) else { return };
    let (inputs, output) = load_of(c);
    let (mode, recipe) = mode_of(&st.fab, powered, &inputs, output.as_ref().map(|(i, n)| (i.as_str(), *n)));
    let left_ds = st.fab.progress().map_or(0, |(_, l)| (l * 10.0).round().clamp(0.0, u16::MAX as f64) as u16);
    let news = FabNews { mode, recipe, left_ds, pushes: st.pushes, refusals: st.refusals, runs: st.runs };
    items.fab.news = Some(news);
    st.since += dt;
    if items.role == Role::Host && st.sent != Some(news) && st.since >= SEND_EVERY_S {
        items.net_out.push(crate::items::NetOut::Fab { to: None, n: news });
        st.sent = Some(news);
        st.since = 0.0;
    }
}

/// Every process: the screen from the news; a client's button goes down when a push was counted.
fn write_screen(items: Res<Items>, cfg: Res<FabCfg>, mut panels: ResMut<Panels>, mut machines: Query<&mut Machine, With<FabMachine>>, mut seen: Local<Option<u32>>) {
    let Some(n) = items.fab.news else { return };
    panels.set(SCREEN_ID, display_core::fabricator::panel(&face_of(&n, &cfg.0, &items.defs)));
    if items.role == Role::Client && counter_fresh(&mut seen, n.pushes) {
        for mut m in &mut machines {
            m.controls.set_button_down(BUTTON);
        }
    }
}

// ---------- window only ----------

#[derive(Component)]
struct Glow;

#[derive(Component)]
struct ButtonCap;

#[derive(Component)]
struct HumLoop;

#[derive(Resource, Default)]
struct FabSounds {
    clunk: Option<Handle<crate::audio::SynthAudio>>,
    click: Option<Handle<crate::audio::SynthAudio>>,
    refuse: Option<Handle<crate::audio::SynthAudio>>,
}

/// The hum's volume at full and how far it carries (m); the refusal's and the clunk's volume. TODO(initiator): by ear.
const HUM_VOL: f32 = 0.06;
const HUM_RANGE_M: f64 = 9.0;
const CLUNK_VOL: f32 = 0.35;
const CLICK_VOL: f32 = 0.2;
const REFUSE_VOL: f32 = 0.18;
const GLOW_LIGHT: f32 = 60_000.0;
const GLOW_COLOUR: [f32; 3] = [1.0, 0.55, 0.18];

pub fn window_plugin(app: &mut App) {
    app.init_resource::<FabSounds>();
    app.add_systems(Startup, audio_setup);
    app.add_systems(Update, (dress_fab, drive_fab).chain().after(spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(Update, fab_sounds.in_set(crate::phases::Frame::Input));
}

fn dress_fab(mut commands: Commands, new: Query<(Entity, &FabMachine), Added<FabMachine>>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for (e, m) in &new {
        let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let mut mat = |c: [f32; 3], emissive: f32| materials.add(StandardMaterial { base_color: Color::srgb(c[0], c[1], c[2]), emissive: LinearRgba::rgb(c[0] * emissive, c[1] * emissive, c[2] * emissive), perceptual_roughness: 0.7, ..default() });
        let dark = mat([0.04, 0.05, 0.06], 0.0);
        let steel = mat([0.3, 0.33, 0.37], 0.0);
        let amber = mat([1.0, 0.7, 0.1], 0.5);
        let glow = materials.add(StandardMaterial { base_color: Color::srgb(0.1, 0.05, 0.02), emissive: LinearRgba::BLACK, perceptual_roughness: 0.4, ..default() });
        // Box axes (x right, y up from the ground, z front) to the root's: the centre is `centre_y` up.
        let at = |p: [f64; 3]| Vec3::new(p[0] as f32, (p[1] - centre_y()) as f32, p[2] as f32);
        let part = |commands: &mut Commands, parent: Entity, mat: &Handle<StandardMaterial>, p: [f64; 3], size: [f32; 3]| {
            let c = commands.spawn((Mesh3d(cube.clone()), MeshMaterial3d(mat.clone()), Transform { translation: at(p), scale: Vec3::from(size), ..default() })).id();
            commands.entity(parent).add_child(c);
            c
        };
        commands.entity(e).insert((crate::origin::WorldPos(m.centre), Transform::from_rotation(m.rot.as_quat()), Visibility::default()));
        // The chamber: a dark frame, the pane that glows, a light behind the pane.
        part(&mut commands, e, &dark, [WINDOW_AT[0], WINDOW_AT[1], FRONT_Z + 0.004], [WINDOW_SIZE[0] as f32 + 0.06, WINDOW_SIZE[1] as f32 + 0.06, 0.01]);
        let pane = part(&mut commands, e, &glow, [WINDOW_AT[0], WINDOW_AT[1], FRONT_Z + 0.012], [WINDOW_SIZE[0] as f32, WINDOW_SIZE[1] as f32, 0.01]);
        commands.entity(pane).insert(Glow);
        let light = commands.spawn((PointLight { color: Color::srgb(GLOW_COLOUR[0], GLOW_COLOUR[1], GLOW_COLOUR[2]), intensity: 0.0, range: 3.0, shadow_maps_enabled: false, ..default() }, Transform::from_translation(at([WINDOW_AT[0], WINDOW_AT[1], FRONT_Z + 0.3])), Glow)).id();
        commands.entity(e).add_child(light);
        // The hatch: a steel plate low on the front with a handle bar.
        part(&mut commands, e, &steel, [0.0, 0.3, FRONT_Z + 0.008], [0.96, 0.42, 0.016]);
        part(&mut commands, e, &dark, [0.0, 0.46, FRONT_Z + 0.03], [0.3, 0.03, 0.03]);
        // The screen's frame and the button with its collar.
        part(&mut commands, e, &dark, [SCREEN_AT[0], SCREEN_AT[1], FRONT_Z + 0.001], [SCREEN_SIZE[0] as f32 + 0.04, SCREEN_SIZE[1] as f32 + 0.04, 0.004]);
        part(&mut commands, e, &steel, [BUTTON_AT[0], BUTTON_AT[1], FRONT_Z + 0.01], [0.13, 0.13, 0.02]);
        let cap = part(&mut commands, e, &amber, [BUTTON_AT[0], BUTTON_AT[1], BUTTON_AT[2] - 0.01], [0.09, 0.09, 0.04]);
        commands.entity(cap).insert(ButtonCap);
    }
}

/// The glow follows the news; the button cap sinks while the kit's button is down.
fn drive_fab(
    time: Res<Time>,
    items: Res<Items>,
    machines: Query<&Machine, With<FabMachine>>,
    mut glows: Query<(Option<&MeshMaterial3d<StandardMaterial>>, Option<&mut PointLight>), With<Glow>>,
    mut caps: Query<&mut Transform, With<ButtonCap>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rest: Local<Option<f32>>,
) {
    let level = glow_level(items.fab.news.map_or(FabMode::LoadOre, |n| n.mode), time.elapsed_secs());
    for (mat, light) in &mut glows {
        if let Some(mat) = mat
            && let Some(mut m) = materials.get_mut(&mat.0)
        {
            m.emissive = LinearRgba::rgb(GLOW_COLOUR[0] * 6.0 * level, GLOW_COLOUR[1] * 6.0 * level, GLOW_COLOUR[2] * 6.0 * level);
            m.base_color = Color::srgb(0.1 + 0.9 * level, 0.05 + 0.5 * level, 0.02 + 0.15 * level);
        }
        if let Some(mut l) = light {
            l.intensity = GLOW_LIGHT * level;
        }
    }
    let down = machines.iter().any(|m| m.controls.position(BUTTON) > 0.5);
    for mut t in &mut caps {
        let z = *rest.get_or_insert(t.translation.z);
        t.translation.z = z - if down { 0.025 } else { 0.0 };
    }
}

fn audio_setup(mut commands: Commands, mut assets: ResMut<Assets<crate::audio::SynthAudio>>, mut sounds: ResMut<FabSounds>) {
    use crate::audio::{Sound, SynthAudio};
    use bevy::audio::{PlaybackMode, Volume};
    sounds.clunk = Some(assets.add(SynthAudio(Sound::Clunk)));
    sounds.click = Some(assets.add(SynthAudio(Sound::Click)));
    sounds.refuse = Some(assets.add(SynthAudio(Sound::Thud)));
    commands.spawn((AudioPlayer(assets.add(SynthAudio(Sound::PumpHum))), PlaybackSettings { mode: PlaybackMode::Loop, volume: Volume::Linear(0.0), speed: 0.8, ..default() }, HumLoop));
}

/// The hum while it runs (within hearing), the click of a push, the clunk of a finished run and the dull
/// click of a refusal.
#[allow(clippy::too_many_arguments)]
fn fab_sounds(
    mut commands: Commands,
    time: Res<Time>,
    items: Res<Items>,
    players: Query<&crate::walker::Player>,
    machines: Query<&FabMachine>,
    settings: Res<crate::settings::Settings>,
    sounds: Res<FabSounds>,
    mut loops: Query<&mut AudioSink, With<HumLoop>>,
    mut level: Local<f32>,
    mut seen: Local<(Option<u32>, Option<u32>, Option<u32>)>,
) {
    use bevy::audio::AudioSinkPlayback;
    let news = items.fab.news;
    let mut target = 0.0f32;
    if let (Some(n), Ok(pl), Ok(m)) = (news, players.single(), machines.single())
        && n.mode == FabMode::Running
    {
        target = HUM_VOL * crate::audio_slice::falloff((m.centre - pl.w.pos).length(), HUM_RANGE_M);
    }
    let k = 1.0 - (-time.delta_secs() / 0.2).exp();
    *level += (target - *level) * k;
    for mut sink in &mut loops {
        sink.set_volume(bevy::audio::Volume::Linear(crate::audio::loop_volume(*level, &settings)));
    }
    let Some(n) = news else { return };
    let (Some(click), Some(clunk), Some(refuse)) = (&sounds.click, &sounds.clunk, &sounds.refuse) else { return };
    if counter_fresh(&mut seen.0, n.pushes) {
        crate::audio_slice::one_shot(&mut commands, click, CLICK_VOL);
    }
    if counter_fresh(&mut seen.1, n.runs) {
        crate::audio_slice::one_shot(&mut commands, clunk, CLUNK_VOL);
    }
    if counter_fresh(&mut seen.2, n.refusals) {
        crate::audio_slice::one_shot(&mut commands, refuse, REFUSE_VOL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> FabConfig {
        FabConfig::load(&crate::content_files("gameplay")).expect("fabricator.json")
    }

    fn fab() -> Fabricator {
        Fabricator::new(&cfg())
    }

    fn defs() -> ItemDefs {
        crate::items::load_defs()
    }

    #[test]
    fn every_recipe_names_items_that_exist_and_the_ore_goes_into_the_inputs() {
        let d = defs();
        let r = rack();
        for rec in &cfg().recipes {
            for a in &rec.inputs {
                let def = d.get(&a.item).unwrap_or_else(|| panic!("no item {}", a.item));
                assert!(r.rule(0).accepts(&def.item()) && r.rule(1).accepts(&def.item()), "{} goes into an input place", a.item);
                assert!(a.count <= 2 * def.stack, "{}: {} fit the two places", a.item, a.count);
            }
            assert!(d.get(&rec.output.item).is_some_and(|def| rec.output.count <= def.stack.min(OUTPUT_CAP)), "{} fits the output place", rec.output.item);
        }
    }

    #[test]
    fn nobody_puts_a_thing_into_the_output_and_only_ore_into_the_inputs() {
        let d = defs();
        let r = rack();
        let fuse = d.get("fuse").unwrap().item();
        let wrench = d.get("wrench").unwrap().item();
        assert!(!r.rule(OUTPUT).accepts(&fuse) && !r.rule(0).accepts(&fuse) && !r.rule(1).accepts(&wrench));
    }

    #[test]
    fn the_machine_stands_in_the_storage_room_clear_of_the_doorway_and_the_crates() {
        let z = layout::STORAGE_ROOM;
        let at = z.point(AT);
        // Turned 90: the depth runs east-west, the width north-south.
        let (hx, hy) = (SIZE[2] * 0.5, SIZE[0] * 0.5);
        for corner in [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]] {
            assert!(z.contains([at[0] + corner[0] * hx, at[1] + corner[1] * hy]), "inside the room");
        }
        let overlaps = |c: [f64; 2], half: [f64; 2]| (at[0] - c[0]).abs() < hx + half[0] && (at[1] - c[1]).abs() < hy + half[1];
        for c in storage::cabinets() {
            let sideways = (c.turn.abs() - 90.0).abs() < 1.0;
            let half = if sideways { [c.size[2] * 0.5, c.size[0] * 0.5] } else { [c.size[0] * 0.5, c.size[2] * 0.5] };
            assert!(!overlaps(z.point(c.at), half), "{} stands clear of the fabricator", c.id);
        }
        // The doorway is the 1.6 m gap in the north wall, x -0.2..1.4 in the area, 1 m wider each side kept free.
        assert!(at[0] + hx < -0.2 - 1.0, "the doorway stays free");
    }

    #[test]
    fn the_screen_faces_follow_the_state() {
        let d = defs();
        let c = cfg();
        let mut f = fab();
        let ore = |item: &str, n: u32| vec![Amount::new(item, n)];
        assert_eq!(mode_of(&f, true, &[], None), (FabMode::LoadOre, 255));
        assert_eq!(mode_of(&f, true, &ore("grumblite", 1), None), (FabMode::NoRecipe, 255));
        assert_eq!(mode_of(&f, true, &ore("grumblite", 2), None), (FabMode::Ready, 0));
        f.start(&ore("grumblite", 2), None, |_| 4).unwrap();
        assert_eq!(mode_of(&f, true, &ore("grumblite", 2), None).0, FabMode::Running);
        assert_eq!(mode_of(&f, false, &ore("grumblite", 2), None), (FabMode::NoPower, 0), "a paused run says NO POWER");
        f.step(1000.0, true);
        assert_eq!(mode_of(&f, true, &[], Some(("fuse", 1))), (FabMode::Take, 0));
        let news = |mode, recipe, left_ds| FabNews { mode, recipe, left_ds, ..Default::default() };
        let words = |n: FabNews| display_core::fabricator::words(&face_of(&n, &c, &d));
        assert!(words(news(FabMode::Ready, 0, 0)).contains(&"2 GRUMBLITE > FUSE".to_string()));
        assert!(words(news(FabMode::Running, 1, 123)).contains(&"13 S".to_string()), "{:?}", words(news(FabMode::Running, 1, 123)));
        assert!(words(news(FabMode::Take, 2, 0)).contains(&"COOLANT CANISTER".to_string()));
    }

    #[test]
    fn the_glow_is_dark_unless_it_runs_and_never_leaves_its_range() {
        assert_eq!(glow_level(FabMode::NoPower, 1.0), 0.0);
        assert_eq!(glow_level(FabMode::Ready, 1.0), 0.0);
        for i in 0..100 {
            let g = glow_level(FabMode::Running, i as f32 * 0.1);
            assert!((PULSE_LOW - 1e-4..=1.0 + 1e-4).contains(&g), "{g}");
        }
    }
}
