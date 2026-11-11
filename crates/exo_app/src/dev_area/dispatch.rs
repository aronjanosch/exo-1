//! The dispatch chute (#334): a hatch in the storage room's north wall with a SEND lever and an order slate. The orders
//! and the rule are `production_core::orders` and `content/gameplay/orders/orders.json`; this file is the glue.
//!
//! - F on the hatch opens the container window (rack `DISPATCH_RACK`, six places, any small or medium item, as the crates).
//! - SEND is a lever of the controls kit (`Machine`, two detents). F on it asks the host (`DispatchAsk::Send`, as a client or
//!   as the host's own pull); the host runs the press on the kit (`PressPart`) and, on the lever's step to SEND, asks the
//!   book. A complete load goes (exactly the order's counts, through `Items::edit_rack`: the extras stay, a window somebody
//!   has open sees it), a short one stays and the missing lines blink. The lever springs back to rest by itself.
//! - The host owns the book (`DispatchState`) and tells everyone the slate in `DispatchNews` (4 Hz at most, while it changes);
//!   the counters in it move the lever on a client and play the sounds. A sent order is a notice to every player.
//! - The slate (`display_core::dispatch`) is read-only and runs without ship power: it is the crew's goal, and a dark ship
//!   must still show what is asked.
//! - The book is a save section (`dispatch`); an older save has none and starts with a fresh book.
use super::*;
use crate::container_window::{ContainerSpec, ContainerSpecs};
use crate::controls_kit::{apply_presses, ease, Machine, PartSound, PressPart};
use crate::dev_area::ship::counter_fresh;
use crate::displays::{Bare, DisplayPanel, Panels, ReadOnly};
use crate::gameplay::Gameplay;
use crate::items::{Items, Role};
use crate::ship::Ship;
use controls_core::{Controls, Outcome};
use gameplay_core::notice::{Arg, Notice, NoticeKind};
use gameplay_core::save::{Envelope, SaveError};
use item_core::container::{Access, Container, ItemKind, SlotRule};
use item_core::wire::{DispatchAsk, DispatchFace, DispatchLine, DispatchNews};
use item_core::{ItemDefs, RackId};
use production_core::fabricator::Amount;
use production_core::orders::{OrderBook, OrderConfig, Saved};

/// 107 and 108 are the ore tubs, 106 the fabricator.
pub const DISPATCH_RACK: RackId = 109;
pub const HOUSING: &str = "dispatch_chute";
pub const NAME: &str = "Dispatch chute";
pub const LEVER: &str = "lever_send";
pub const SCREEN_ID: PanelId = PanelId::new("dispatch");
pub const PLACES: usize = 6;
pub const COLS: usize = 3;
/// The fixed seed of the order sequence. TODO(initiator): per save or per run instead of one fixed sequence?
pub const SEED: u64 = 0x0D15_9A7C_4005;
/// Seconds the DONE and the missing-lines faces last. TODO(initiator): starting value.
pub const FLASH_S: f64 = 2.0;
/// Seconds the lever stays pulled before it springs back.
const LEVER_HOLD_S: f64 = 0.45;
/// Seconds between two snapshots of a changed news (4 Hz).
const SEND_EVERY_S: f64 = 0.25;
/// The slate blinks the unmet lines at this rate (Hz).
const BLINK_HZ: f64 = 3.0;

/// On the storage room's north wall east of the doorway (x -0.8..0.8 from the room's centre), facing south into the room.
/// Clear of the extinguisher cabinet (east wall, y 0.7..1.7) and of the ore tub's spot. TODO(initiator): the place.
pub const AT: [f64; 2] = [2.15, 2.3];
pub const TURN_DEG: f64 = 0.0;
/// Width, height and depth (m).
pub const SIZE: [f64; 3] = [1.4, 1.5, 0.4];
/// On the housing's front (box axes: x right, y up from the ground, z out of the front). TODO(initiator): the arrangement.
const FRONT_Z: f64 = SIZE[2] * 0.5;
const HATCH_AT: [f64; 2] = [-0.2, 0.8];
const HATCH_SIZE: [f64; 2] = [0.9, 0.7];
const SLATE_SIZE: [f64; 2] = [0.30, 0.22];
const SLATE_AT: [f64; 3] = [0.47, 1.22, FRONT_Z + 0.003];
/// The lever's pivot on the front and its arm's length; the knob (the pick point) sits at the arm's tip at rest.
const PIVOT_AT: [f64; 3] = [0.47, 0.6, FRONT_Z + 0.04];
const ARM_LEN: f64 = 0.3;
const KNOB_RADIUS: f64 = 0.14;
/// How far the lever tilts out at SEND (degrees).
const PULL_DEG: f32 = 55.0;

#[derive(Resource, Clone)]
pub struct DispatchCfg(pub OrderConfig);

/// Host: the book, the face the slate shows now and the counters the news carries.
#[derive(Resource)]
pub struct DispatchState {
    pub book: OrderBook,
    pub face: DispatchFace,
    pub face_left_s: f64,
    pub pulls: u32,
    pub sends: u32,
    pub refusals: u32,
    sent: Option<DispatchNews>,
    since: f64,
}

impl DispatchState {
    pub fn new(cfg: &OrderConfig) -> DispatchState {
        DispatchState { book: OrderBook::new(cfg, SEED), face: DispatchFace::Order, face_left_s: 0.0, pulls: 0, sends: 0, refusals: 0, sent: None, since: 0.0 }
    }
}

/// The machine in the world: where it stands and where its lever's knob is (the pick point).
#[derive(Component, Clone, Copy, Debug)]
pub struct DispatchMachine {
    pub centre: DVec3,
    pub rot: DQuat,
    pub lever_at: DVec3,
}

impl DispatchMachine {
    /// A point in box axes (y up from the ground) in the world.
    pub fn world(&self, p: [f64; 3]) -> DVec3 {
        self.centre + self.rot * DVec3::new(p[0], p[1] - centre_y(), p[2])
    }

    /// Whether the view's ray points at the lever: within its radius and `reach`.
    pub fn pick_lever(&self, eye: DVec3, look: DVec3, reach: f64) -> bool {
        let look = look.normalize_or_zero();
        let d = self.lever_at - eye;
        let along = d.dot(look);
        along > 0.0 && d.length() <= reach && (d - look * along).length() <= KNOB_RADIUS
    }
}

fn centre_y() -> f64 {
    (SIZE[1] - SINK) * 0.5
}

pub fn housing() -> PlaceholderBox {
    PlaceholderBox { id: HOUSING.into(), zone: layout::STORAGE_ROOM, local_pos: AT, size: SIZE, turn_deg: TURN_DEG, colour: [0.34, 0.37, 0.42] }
}

/// Six places taking any small or medium item, as the crates do.
pub fn rack() -> Container {
    Container::new(vec![SlotRule::tagged(32, &["small", "medium"], &["carrier"]); PLACES], Access::Open)
}

/// What the chute holds of each item the order asks for (the book adds up the rest by itself).
pub fn contents_for(c: &Container, book: &OrderBook) -> Vec<Amount> {
    book.current().lines.iter().map(|l| Amount::new(&l.item, c.count(&ItemKind::new(l.item.as_str())))).collect()
}

/// The slate's news: the order, the lines the chute covers now, the face and the counters.
pub fn news_of(st: &DispatchState, c: &Container) -> DispatchNews {
    let met = st.book.check(&contents_for(c, &st.book));
    let lines = st.book.current().lines.iter().zip(met).map(|(l, met)| DispatchLine { item: l.item.clone(), count: l.count, met }).collect();
    DispatchNews { done: st.book.done(), lines, face: st.face, pulls: st.pulls, sends: st.sends, refusals: st.refusals }
}

fn name_of(defs: &ItemDefs, id: &str) -> String {
    defs.get(id).map_or(id, |d| d.name.as_str()).to_uppercase()
}

/// The slate's face from the news (the same on the host and on a client). `blink_on` is the clock's phase.
pub fn face_of(n: &DispatchNews, defs: &ItemDefs, blink_on: bool) -> display_core::dispatch::Face {
    use display_core::dispatch::{Face, Line};
    let lines: Vec<Line> = n.lines.iter().map(|l| Line { text: format!("{} {}", l.count, name_of(defs, &l.item)), met: l.met }).collect();
    match n.face {
        DispatchFace::Order => Face::Order { number: n.done.saturating_add(1), lines, done: n.done },
        DispatchFace::Done => Face::Done { number: n.done, done: n.done },
        DispatchFace::Missing => Face::Missing { number: n.done.saturating_add(1), lines, done: n.done, blink_on },
    }
}

pub fn blink_on(t: f64) -> bool {
    (t * BLINK_HZ * 2.0).rem_euclid(2.0) < 1.0
}

pub fn plugin(app: &mut App) {
    let cfg = OrderConfig::load(&crate::content_files("gameplay")).unwrap_or_else(|e| panic!("orders: {e:?}"));
    app.init_resource::<ContainerSpecs>();
    app.world_mut().resource_mut::<ContainerSpecs>().0.push(ContainerSpec { rack: DISPATCH_RACK, name: NAME.into(), box_id: HOUSING.into(), cols: COLS });
    app.world_mut().resource_mut::<DevAreaDevices>().add(housing());
    app.insert_resource(DispatchState::new(&cfg)).insert_resource(DispatchCfg(cfg));
    app.add_systems(Update, spawn_dispatch.after(spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(FixedUpdate, (put_rack, dispatch_asks.before(apply_presses), dispatch_react.after(apply_presses), spring_lever).in_set(crate::phases::Fx::Walker));
    app.add_systems(FixedUpdate, (dispatch_step, write_slate, order_notice).chain().in_set(crate::phases::Fx::Effects));
}

/// The machine, its lever and its slate, once per planet.
fn spawn_dispatch(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    let (centre, rot) = box_pose(&frame, &planet, &housing());
    let mut m = DispatchMachine { centre, rot, lever_at: DVec3::ZERO };
    m.lever_at = m.world([PIVOT_AT[0], PIVOT_AT[1] + ARM_LEN, PIVOT_AT[2]]);
    let mut controls = Controls::new();
    controls.lever(LEVER, 2, None);
    let mut kit = Machine { controls, ..default() };
    // Back to rest is the lever's own spring, never a press.
    kit.refused.insert((LEVER.into(), 0));
    let machine = commands.spawn((m, kit)).id();
    let slate = commands.spawn((DisplayPanel { id: SCREEN_ID, name: "ORDER SLATE".into(), size_m: DVec2::from(SLATE_SIZE), centre: m.world(SLATE_AT), rot }, Bare, ReadOnly)).id();
    area.entities.extend([machine, slate]);
}

/// Once, when the ship is there: the rack goes on the host's map.
fn put_rack(time: Res<Time>, mut items: ResMut<Items>, ships: Query<(), With<Ship>>, mut done: Local<bool>) {
    if *done || ships.is_empty() {
        return;
    }
    *done = true;
    items.add_container(time.elapsed_secs_f64(), DISPATCH_RACK, rack());
}

/// Host: a pull (a client's or the host's own) becomes a press on the kit's lever.
fn dispatch_asks(mut items: ResMut<Items>, machines: Query<Entity, With<DispatchMachine>>, mut out: MessageWriter<PressPart>) {
    if items.role == Role::Client {
        return;
    }
    let asks = std::mem::take(&mut items.dispatch.asks);
    let Ok(e) = machines.single() else { return };
    for (_, DispatchAsk::Send) in asks {
        out.write(PressPart { machine: e, part: LEVER.into(), hand: None });
    }
}

/// What a pull did to the chute: a complete load goes, a short one stays.
pub enum Sent {
    /// The order's number.
    Gone(u32),
    /// Lines (indexes) that are not met.
    Short(Vec<usize>),
}

/// Host: the lever's step to SEND asks the book about what the chute holds. Complete: exactly the order's counts leave
/// (the extras stay), the DONE face runs. Short: nothing leaves, the missing lines blink.
pub fn send(items: &mut Items, st: &mut DispatchState, now: f64) -> Sent {
    st.pulls = st.pulls.wrapping_add(1);
    let held = items.host.racks.get(&DISPATCH_RACK).map(|c| contents_for(c, &st.book)).unwrap_or_default();
    match st.book.send(&held) {
        Ok(taken) => {
            items.edit_rack(now, DISPATCH_RACK, |c| {
                for a in &taken.remove {
                    let _ = c.machine_remove(&ItemKind::new(a.item.as_str()), a.count);
                }
            });
            st.sends = st.sends.wrapping_add(1);
            st.face = DispatchFace::Done;
            st.face_left_s = FLASH_S;
            Sent::Gone(taken.number)
        }
        Err(missing) => {
            st.refusals = st.refusals.wrapping_add(1);
            st.face = DispatchFace::Missing;
            st.face_left_s = FLASH_S;
            Sent::Short(missing.0)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_react(mut sounds: MessageReader<PartSound>, machines: Query<(), With<DispatchMachine>>, time: Res<Time>, mut items: ResMut<Items>, mut st: ResMut<DispatchState>) {
    if items.role == Role::Client {
        return;
    }
    for s in sounds.read() {
        if !machines.contains(s.machine) || s.part != LEVER || s.outcome != (Outcome::Lever { index: 1 }) {
            continue;
        }
        match send(&mut items, &mut st, time.elapsed_secs_f64()) {
            Sent::Gone(n) => info!("dispatch: order {n} sent"),
            Sent::Short(lines) => info!("dispatch: refused, lines {lines:?} short"),
        }
    }
}

/// Every process: the lever springs back to rest after a moment.
fn spring_lever(time: Res<Time>, mut machines: Query<&mut Machine, With<DispatchMachine>>, mut held: Local<f64>) {
    let Ok(mut m) = machines.single_mut() else { return };
    if m.controls.lever_index(LEVER).unwrap_or(0) == 0 {
        *held = 0.0;
        return;
    }
    *held += time.delta_secs_f64();
    if *held >= LEVER_HOLD_S {
        m.controls.drop_to_off(LEVER);
        *held = 0.0;
    }
}

/// Host: the flash runs out; then the news for the slate, the lever and the sounds.
fn dispatch_step(time: Res<Time>, mut items: ResMut<Items>, mut st: ResMut<DispatchState>) {
    if items.role == Role::Client {
        return;
    }
    let dt = time.delta_secs_f64();
    if st.face != DispatchFace::Order {
        st.face_left_s -= dt;
        if st.face_left_s <= 0.0 {
            st.face = DispatchFace::Order;
            st.face_left_s = 0.0;
        }
    }
    let Some(c) = items.host.racks.get(&DISPATCH_RACK) else { return };
    let news = news_of(&st, c);
    items.dispatch.news = Some(news.clone());
    st.since += dt;
    if items.role == Role::Host && st.sent.as_ref() != Some(&news) && st.since >= SEND_EVERY_S {
        items.net_out.push(crate::items::NetOut::Dispatch { to: None, n: news.clone() });
        st.sent = Some(news);
        st.since = 0.0;
    }
}

/// Every process: the slate from the news; a client's lever moves when a pull was counted.
fn write_slate(time: Res<Time>, items: Res<Items>, mut panels: ResMut<Panels>, mut machines: Query<&mut Machine, With<DispatchMachine>>, mut seen: Local<Option<u32>>) {
    let Some(n) = items.dispatch.news.as_ref() else { return };
    panels.set(SCREEN_ID, display_core::dispatch::panel(&face_of(n, &items.defs, blink_on(time.elapsed_secs_f64()))));
    if items.role == Role::Client && counter_fresh(&mut seen, n.pulls) {
        for mut m in &mut machines {
            m.controls.set_lever(LEVER, 1);
        }
    }
}

/// Every process: a sent order is a notice to everyone (the crew is spread over the ship).
fn order_notice(items: Res<Items>, mut seen: Local<Option<u32>>, mut gp: ResMut<Gameplay>) {
    let Some(n) = items.dispatch.news.as_ref() else { return };
    if counter_fresh(&mut seen, n.sends) {
        gp.notify(Notice::new(NoticeKind::Updated, "notice.dispatch.sent").arg("n", Arg::Number(i64::from(n.done))));
    }
}

// ---------- save ----------

pub const SECTION: &str = "dispatch";
pub const VERSION: u32 = 1;

pub fn save_section(st: &DispatchState, env: &mut Envelope) {
    env.put(SECTION, VERSION, &st.book.saved());
}

/// Reads the section (read first: a refused section changes nothing). An older save has none.
pub fn read_section(env: &Envelope) -> Result<Option<Saved>, SaveError> {
    env.get::<Saved>(SECTION, VERSION)
}

pub fn apply_section(st: &mut DispatchState, cfg: &OrderConfig, saved: Option<Saved>) {
    // No section: a fresh book, so a load of an older save does not carry the running session's orders over.
    st.book = match saved {
        Some(s) => OrderBook::restore(cfg, s, SEED),
        None => OrderBook::new(cfg, SEED),
    };
    st.face = DispatchFace::Order;
    st.face_left_s = 0.0;
}

// ---------- window only ----------

#[derive(Component)]
struct LeverPivot;

#[derive(Resource, Default)]
struct DispatchSounds {
    whoosh: Option<Handle<crate::audio::SynthAudio>>,
    click: Option<Handle<crate::audio::SynthAudio>>,
    clunk: Option<Handle<crate::audio::SynthAudio>>,
}

/// Volumes of the whoosh, the lever's click and the dull clunk of a refusal. TODO(initiator): by ear.
const WHOOSH_VOL: f32 = 0.35;
const CLICK_VOL: f32 = 0.2;
const CLUNK_VOL: f32 = 0.3;

pub fn window_plugin(app: &mut App) {
    app.init_resource::<DispatchSounds>();
    app.add_systems(Startup, audio_setup);
    app.add_systems(Update, (dress_dispatch, drive_lever).chain().after(spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(Update, dispatch_sounds.in_set(crate::phases::Frame::Input));
}

fn dress_dispatch(mut commands: Commands, new: Query<Entity, Added<DispatchMachine>>, machines: Query<&DispatchMachine>, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    for e in &new {
        let Ok(m) = machines.get(e) else { continue };
        let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let mut mat = |c: [f32; 3], emissive: f32| materials.add(StandardMaterial { base_color: Color::srgb(c[0], c[1], c[2]), emissive: LinearRgba::rgb(c[0] * emissive, c[1] * emissive, c[2] * emissive), perceptual_roughness: 0.7, ..default() });
        let dark = mat([0.04, 0.05, 0.06], 0.0);
        let panel = mat([0.10, 0.11, 0.13], 0.0);
        let steel = mat([0.3, 0.33, 0.37], 0.0);
        let red = mat([0.9, 0.18, 0.08], 0.35);
        let at = |p: [f64; 3]| Vec3::new(p[0] as f32, (p[1] - centre_y()) as f32, p[2] as f32);
        let part = |commands: &mut Commands, parent: Entity, mat: &Handle<StandardMaterial>, p: [f64; 3], size: [f32; 3]| {
            let c = commands.spawn((Mesh3d(cube.clone()), MeshMaterial3d(mat.clone()), Transform { translation: at(p), scale: Vec3::from(size), ..default() })).id();
            commands.entity(parent).add_child(c);
            c
        };
        commands.entity(e).insert((crate::origin::WorldPos(m.centre), Transform::from_rotation(m.rot.as_quat()), Visibility::default()));
        // The hatch: a steel frame, a darker door panel inside it and a handle bar at its lower edge.
        part(&mut commands, e, &steel, [HATCH_AT[0], HATCH_AT[1], FRONT_Z + 0.004], [HATCH_SIZE[0] as f32 + 0.1, HATCH_SIZE[1] as f32 + 0.1, 0.008]);
        part(&mut commands, e, &panel, [HATCH_AT[0], HATCH_AT[1], FRONT_Z + 0.012], [HATCH_SIZE[0] as f32, HATCH_SIZE[1] as f32, 0.016]);
        part(&mut commands, e, &dark, [HATCH_AT[0], HATCH_AT[1] - HATCH_SIZE[1] * 0.5 + 0.08, FRONT_Z + 0.04], [0.34, 0.03, 0.03]);
        // The slate's frame.
        part(&mut commands, e, &dark, [SLATE_AT[0], SLATE_AT[1], FRONT_Z + 0.0005], [SLATE_SIZE[0] as f32 + 0.04, SLATE_SIZE[1] as f32 + 0.04, 0.002]);
        // The lever: a backplate with a slot, the pivot that turns, an arm and a red knob at its tip.
        part(&mut commands, e, &steel, [PIVOT_AT[0], PIVOT_AT[1] + ARM_LEN * 0.5 - 0.02, FRONT_Z + 0.006], [0.12, ARM_LEN as f32 + 0.2, 0.012]);
        part(&mut commands, e, &dark, [PIVOT_AT[0], PIVOT_AT[1] + ARM_LEN * 0.5 + 0.02, FRONT_Z + 0.013], [0.035, ARM_LEN as f32 + 0.06, 0.004]);
        let pivot = commands.spawn((Transform::from_translation(at(PIVOT_AT)), Visibility::default(), LeverPivot)).id();
        commands.entity(e).add_child(pivot);
        let arm = commands.spawn((Mesh3d(cube.clone()), MeshMaterial3d(dark.clone()), Transform { translation: Vec3::new(0.0, ARM_LEN as f32 * 0.5, 0.0), scale: Vec3::new(0.035, ARM_LEN as f32, 0.035), ..default() })).id();
        let knob = commands.spawn((Mesh3d(cube.clone()), MeshMaterial3d(red.clone()), Transform { translation: Vec3::new(0.0, ARM_LEN as f32 + 0.03, 0.0), scale: Vec3::new(0.09, 0.1, 0.09), ..default() })).id();
        commands.entity(pivot).add_children(&[arm, knob]);
    }
}

/// The lever follows the kit's position (rest 0, SEND 1), tilting out toward the player.
fn drive_lever(time: Res<Time>, machines: Query<&Machine, With<DispatchMachine>>, mut pivots: Query<&mut Transform, With<LeverPivot>>, mut shown: Local<f32>) {
    let Ok(m) = machines.single() else { return };
    *shown = ease(*shown, m.controls.position(LEVER) as f32, crate::controls_kit::EASE_PER_S * time.delta_secs());
    for mut t in &mut pivots {
        t.rotation = Quat::from_rotation_x((*shown * PULL_DEG).to_radians());
    }
}

fn audio_setup(mut assets: ResMut<Assets<crate::audio::SynthAudio>>, mut sounds: ResMut<DispatchSounds>) {
    use crate::audio::{Sound, SynthAudio};
    sounds.whoosh = Some(assets.add(SynthAudio(Sound::SweepDown)));
    sounds.click = Some(assets.add(SynthAudio(Sound::Click)));
    sounds.clunk = Some(assets.add(SynthAudio(Sound::Thud)));
}

/// The lever's click, the whoosh of a sent order and the dull clunk of a refused one.
fn dispatch_sounds(mut commands: Commands, items: Res<Items>, sounds: Res<DispatchSounds>, mut seen: Local<(Option<u32>, Option<u32>, Option<u32>)>) {
    let Some(n) = items.dispatch.news.as_ref() else { return };
    let (Some(click), Some(whoosh), Some(clunk)) = (&sounds.click, &sounds.whoosh, &sounds.clunk) else { return };
    if counter_fresh(&mut seen.0, n.pulls) {
        crate::audio_slice::one_shot(&mut commands, click, CLICK_VOL);
    }
    if counter_fresh(&mut seen.1, n.sends) {
        crate::audio_slice::one_shot(&mut commands, whoosh, WHOOSH_VOL);
    }
    if counter_fresh(&mut seen.2, n.refusals) {
        crate::audio_slice::one_shot(&mut commands, clunk, CLUNK_VOL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> OrderConfig {
        OrderConfig::load(&crate::content_files("gameplay")).expect("orders.json")
    }

    fn defs() -> ItemDefs {
        crate::items::load_defs()
    }

    #[test]
    fn every_order_item_exists_and_fits_the_chute() {
        let d = defs();
        let c = cfg();
        let r = rack();
        for (t, tier) in c.tiers.iter().enumerate() {
            for l in &tier.pool {
                let def = d.get(&l.item).unwrap_or_else(|| panic!("tier {t}: no item {}", l.item));
                assert!(r.rule(0).accepts(&def.item()), "tier {t}: the chute takes {}", l.item);
            }
            // The worst order of the tier: the `lines` items that need the most places.
            let mut places: Vec<usize> = tier.pool.iter().map(|l| (l.max as usize).div_ceil(d.get(&l.item).unwrap().stack.min(32) as usize)).collect();
            places.sort_unstable_by(|a, b| b.cmp(a));
            let worst: usize = places.iter().take(tier.lines).sum();
            assert!(worst <= PLACES, "tier {t}: the worst order needs {worst} of {PLACES} places");
        }
    }

    #[test]
    fn the_chute_takes_small_and_medium_things_only() {
        let d = defs();
        let r = rack();
        assert_eq!(r.len(), PLACES);
        for id in ["fuse", "grumblite", "extinguisher", "fizz_jar"] {
            assert!(r.rule(0).accepts(&d.get(id).unwrap().item()), "{id}");
        }
    }

    #[test]
    fn the_chute_stands_in_the_storage_room_clear_of_the_doorway_the_cabinets_and_the_crates() {
        let z = layout::STORAGE_ROOM;
        let at = z.point(AT);
        let (hx, hy) = (SIZE[0] * 0.5, SIZE[2] * 0.5);
        for corner in [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]] {
            assert!(z.contains([at[0] + corner[0] * hx, at[1] + corner[1] * hy]), "inside the room");
        }
        let overlaps = |c: [f64; 2], half: [f64; 2]| (at[0] - c[0]).abs() < hx + half[0] && (at[1] - c[1]).abs() < hy + half[1];
        for c in storage::cabinets() {
            let sideways = (c.turn.abs() - 90.0).abs() < 1.0;
            let half = if sideways { [c.size[2] * 0.5, c.size[0] * 0.5] } else { [c.size[0] * 0.5, c.size[2] * 0.5] };
            assert!(!overlaps(z.point(c.at), half), "{} stands clear of the chute", c.id);
        }
        // The doorway is x -0.2..1.4 in the area; the chute starts a metre east of it.
        assert!(at[0] - hx > 1.4, "the doorway stays free");
        let fab = z.point(super::fabricator::AT);
        assert!((at[0] - fab[0]).abs() > 2.0, "the fabricator is across the room");
    }

    #[test]
    fn the_slate_follows_the_load_the_book_and_the_face() {
        let d = defs();
        let c = cfg();
        let mut st = DispatchState::new(&c);
        let mut rack = rack();
        let first = st.book.current().clone();
        let l = first.lines[0].clone();
        let n0 = news_of(&st, &rack);
        assert!(n0.lines.iter().all(|x| !x.met) && n0.face == DispatchFace::Order);
        // One short: unmet. Then enough: met.
        rack.try_insert(&[], d.get(&l.item).unwrap().stack_of(l.count - 1)).unwrap();
        assert!(!news_of(&st, &rack).lines[0].met);
        rack.try_insert(&[], d.get(&l.item).unwrap().stack_of(1)).unwrap();
        let n1 = news_of(&st, &rack);
        assert!(n1.lines[0].met);
        let words = |f| display_core::dispatch::words(&f);
        assert!(words(face_of(&n1, &d, true)).contains(&"ORDER 1".to_string()));
        assert_eq!(display_core::dispatch::ticks(&face_of(&n1, &d, true)), 1);
        st.book.send(&contents_for(&rack, &st.book)).unwrap();
        st.face = DispatchFace::Done;
        let n2 = news_of(&st, &rack);
        assert!(words(face_of(&n2, &d, true)).contains(&"ORDER 1 SENT".to_string()), "{:?}", words(face_of(&n2, &d, true)));
        st.face = DispatchFace::Missing;
        assert!(words(face_of(&news_of(&st, &rack), &d, true)).contains(&"ORDER 2".to_string()));
    }

    #[test]
    fn the_blink_alternates() {
        let phases: Vec<bool> = (0..12).map(|i| blink_on(i as f64 / (BLINK_HZ * 2.0) + 0.01)).collect();
        assert!(phases.iter().any(|p| *p) && phases.iter().any(|p| !*p));
    }

    #[test]
    fn the_book_survives_a_save() {
        let c = cfg();
        let mut st = DispatchState::new(&c);
        let o = st.book.current().clone();
        st.book.send(&o.lines).unwrap();
        let mut env = Envelope::new();
        save_section(&st, &mut env);
        let env = Envelope::from_json(&env.to_json()).unwrap();
        let mut fresh = DispatchState::new(&c);
        apply_section(&mut fresh, &c, read_section(&env).unwrap());
        assert_eq!((fresh.book.done(), fresh.book.current()), (1, st.book.current()));
        apply_section(&mut fresh, &c, read_section(&Envelope::new()).unwrap());
        assert_eq!(fresh.book.done(), 0, "an older save starts a fresh book");
        let mut bad = Envelope::new();
        bad.put(SECTION, VERSION + 1, &0u32);
        assert!(read_section(&bad).is_err());
    }
}
