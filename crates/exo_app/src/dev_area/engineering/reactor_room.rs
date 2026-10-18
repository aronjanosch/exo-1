//! The reactor of the engine room as a machine (#316): `content/ship/reactor.glb` at the room's
//! centre, its collision, and the physical parts bound with the controls kit (#315).
//!
//! Parts (node names, travel: `art/ship/reactor.md`):
//! - SCRAM: `cover_scram` guards `button_scram`. F on the button with the cover open vents the chamber
//!   (`reactor_core::Reactor::scram`: injection gone at once, a share of the fill lost).
//! - Core eject: `cover_eject` guards `lever_eject`. The lever with the cover open throws the core out
//!   (this replaces the old screen's ARM step: the cover is the arm). The old screen's ARM and EJECT still work (#321 removes them).
//! - `slot_1`, `slot_2`: the two canister slots. They show the canisters the reactor holds (`ReactorNews::canisters`);
//!   F with a canister in hand loads it into the slot looked at (`Request::Load`), F with empty hands takes that slot's one out
//!   (the brake, `ToHost::Pull(Some(slot))`). A taken slot refuses a load, an empty one a pull.
//! - The start-up stand (#319, `start_stand.rs`): `toggle_boot_1..3`, the rotary `lever_start` (OFF, PRIME, RUN) and `lamp_prime_1..5`.
//!   `EmergencyStop` is the hook: a SCRAM and an eject send it, and the stand drops the lever and the three toggles to off.
//!
//! Host-owned state: the `Machine` on the room entity is the host's. A client sends `ReactorAsk::Press(i)` (the index into
//! `PRESS`) and shows the host's `ReactorNews` (covers, SCRAMs, canisters, ejected). Slots are not pressed through the kit:
//! the item system seats and pulls, the kit's slot only mirrors it.
//!
//! Window: the model, the glows (`core_glow` from the output, a flicker when a canister goes in, dark after a SCRAM or an eject),
//! the rings' spin, the slot lamps, the canisters at the slots, and the core's fall on an eject.
use crate::controls_kit::{apply_presses, Binding, KitBindings, Machine, PartSound, PressPart};
use crate::dev_area::engineering::canisters::holds_canister;
use crate::dev_area::layout::REACTOR_AT;
use crate::dev_area::ship::{counter_fresh, host_asks, route, ReactorCounts, ReactorSim, ShipSet};
use crate::dev_area::*;
use crate::gameplay::Gameplay;
use crate::items::Items;
use controls_core::{Controls, Outcome};
use gameplay_core::notice::{Notice, NoticeKind};
use item_core::wire::{ReactorAsk, ReactorNews, CANISTER_SLOTS};

/// The main screen `screen_main` (#317): the quad of `reactor.glb` (1.10 x 0.62 m, tier L, 1280 x 720 panel px). Its centre in
/// model metres (glTF axes, `art/ship/reactor.py`: the face point at 1.40 m plus the 3.5 cm of the recess) and its tilt back
/// about the model's X axis. The panel's quad stands 3 mm in front of the model's quad.
pub const MAIN_ID: display_core::PanelId = display_core::PanelId::new("reactor_main");
pub const MAIN_NAME: &str = "REACTOR MAIN";
pub const MAIN_SIZE: [f64; 2] = [1.10, 0.62];
pub const MAIN_AT: [f64; 3] = [0.0, 1.39366, 1.44558];
pub const MAIN_TILT_DEG: f64 = -10.437;
const MAIN_OUT: f64 = 0.003;

pub const MODEL: &str = "ship/reactor.glb";
pub const CANISTER_MODEL: &str = "ship/canister_antimatter.glb";
/// The model's +Z (the console's front) points to the door side: north in the area.
pub const TURN_DEG: f64 = 180.0;

pub const COVER_SCRAM: &str = "cover_scram";
pub const BUTTON_SCRAM: &str = "button_scram";
pub const COVER_EJECT: &str = "cover_eject";
pub const LEVER_EJECT: &str = "lever_eject";
pub const SLOTS: [&str; CANISTER_SLOTS] = ["slot_1", "slot_2"];
/// The start-up stand (#319): three boot toggles and the rotary START lever.
pub const TOGGLES: [&str; 3] = ["toggle_boot_1", "toggle_boot_2", "toggle_boot_3"];
pub const LEVER_START: &str = "lever_start";
/// `ReactorAsk::Press(i)` names a part by its index here.
pub const PRESS: [&str; 8] = [COVER_SCRAM, BUTTON_SCRAM, COVER_EJECT, LEVER_EJECT, TOGGLES[0], TOGGLES[1], TOGGLES[2], LEVER_START];

/// A part the walker can look at: where it is in the model (m, glTF axes, origin on the floor under the column), where it is
/// with its cover open (covers only), and how close to the view's ray it must be (m).
pub struct PartSpec {
    pub name: &'static str,
    pub at: [f64; 3],
    pub open_at: Option<[f64; 3]>,
    pub radius: f64,
    /// The cover that must be open for this part to answer F; looking at it with the cover shut picks the cover.
    pub guard: Option<&'static str>,
}

/// The first eight are `PRESS` in order (the stand's points, #319: the grips above the pivots in `art/ship/reactor.md`), then the slots. Points are the parts' centres worked out from the node poses in
/// `art/ship/reactor.md` (rest, and open for the covers); the slots' are their mouths.
pub const PARTS: [PartSpec; 10] = [
    PartSpec { name: COVER_SCRAM, at: [0.5, 1.011, 1.72], open_at: Some([0.5, 1.103, 1.592]), radius: 0.14, guard: None },
    PartSpec { name: BUTTON_SCRAM, at: [0.5, 0.98, 1.71], open_at: None, radius: 0.08, guard: Some(COVER_SCRAM) },
    PartSpec { name: COVER_EJECT, at: [-0.866, 1.25, 0.866], open_at: Some([-1.005, 1.573, 1.005]), radius: 0.16, guard: None },
    PartSpec { name: LEVER_EJECT, at: [-0.813, 1.22, 0.813], open_at: None, radius: 0.1, guard: Some(COVER_EJECT) },
    PartSpec { name: TOGGLES[0], at: [-1.66, 1.07, -0.15], open_at: None, radius: 0.06, guard: None },
    PartSpec { name: TOGGLES[1], at: [-1.66, 1.07, 0.0], open_at: None, radius: 0.06, guard: None },
    PartSpec { name: TOGGLES[2], at: [-1.66, 1.07, 0.15], open_at: None, radius: 0.06, guard: None },
    PartSpec { name: LEVER_START, at: [-1.49, 1.17, 0.0], open_at: None, radius: 0.12, guard: None },
    PartSpec { name: "slot_1", at: [1.6, 1.12, 0.25], open_at: None, radius: 0.13, guard: None },
    PartSpec { name: "slot_2", at: [1.6, 1.12, -0.25], open_at: None, radius: 0.13, guard: None },
];

pub fn part_index(name: &str) -> Option<usize> {
    PARTS.iter().position(|p| p.name == name)
}

/// The reactor's place in the world (area ground, turned to face the door side).
#[derive(Component, Clone, Copy, Debug)]
pub struct ReactorRoom {
    pub origin: DVec3,
    pub rot: DQuat,
}

impl ReactorRoom {
    pub fn world(&self, model: [f64; 3]) -> DVec3 {
        self.origin + self.rot * DVec3::from(model)
    }

    /// Where a part is now (a cover moves when it is open).
    pub fn part_at(&self, i: usize, m: &Machine) -> DVec3 {
        let p = &PARTS[i];
        let open = m.controls.is_open(p.name);
        self.world(p.open_at.filter(|_| open).unwrap_or(p.at))
    }

    /// The part the view's ray points at: the one whose point is nearest the ray within its radius and `reach`. A part under a shut
    /// cover is the cover.
    pub fn pick(&self, m: &Machine, eye: DVec3, look: DVec3, reach: f64) -> Option<usize> {
        let look = look.normalize_or_zero();
        let best = (0..PARTS.len())
            .filter_map(|i| {
                let d = self.part_at(i, m) - eye;
                let along = d.dot(look);
                (along > 0.0 && d.length() <= reach).then(|| ((d - look * along).length(), i)).filter(|(off, i)| *off <= PARTS[*i].radius)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))?
            .1;
        match PARTS[best].guard {
            Some(g) if !m.controls.is_open(g) => part_index(g),
            _ => Some(best),
        }
    }
}

/// Why the reactor stopped by hand. The start lever and the three boot toggles fall to off with it (#319).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmergencyCause {
    Scram,
    Eject,
}

/// Hook for #319: a SCRAM or a core eject happened on the host. TODO(#319): drop `lever_start` and `toggle_boot_1..3` with
/// `Controls::drop_to_off` where this is read.
#[derive(Message, Clone, Copy, Debug)]
pub struct EmergencyStop {
    pub machine: Entity,
    pub cause: EmergencyCause,
}

pub fn machine() -> Machine {
    let mut c = Controls::new();
    c.cover(COVER_SCRAM).button(BUTTON_SCRAM, Some(COVER_SCRAM)).cover(COVER_EJECT).lever(LEVER_EJECT, 2, Some(COVER_EJECT)).slot(SLOTS[0]).slot(SLOTS[1]);
    for t in TOGGLES {
        c.toggle(t, None);
    }
    c.lever_with(LEVER_START, 3, controls_core::LeverEnd::Wrap, None);
    // The dev area starts with a warm, running reactor: the stand matches it (toggles on, lever at RUN).
    for t in TOGGLES {
        c.set_toggle(t, true);
    }
    c.set_lever(LEVER_START, reactor_core::startup::LEVER_RUN);
    Machine { controls: c, ..default() }
}

/// What the kit moves: travel from `art/ship/reactor.md`.
pub fn bindings() -> Vec<Binding> {
    vec![
        Binding::rotate(COVER_SCRAM, COVER_SCRAM, Vec3::X, -110.0),
        Binding::slide(BUTTON_SCRAM, BUTTON_SCRAM, Vec3::NEG_Z, 0.015),
        Binding::rotate(COVER_EJECT, COVER_EJECT, Vec3::X, -110.0),
        Binding::rotate(LEVER_EJECT, LEVER_EJECT, Vec3::X, 60.0),
        // `art/ship/reactor.md`: each lever detent is +35 degrees, a toggle is -40 degrees from rest when on.
        Binding::rotate(LEVER_START, LEVER_START, Vec3::X, 35.0),
        Binding::rotate(TOGGLES[0], TOGGLES[0], Vec3::X, -40.0),
        Binding::rotate(TOGGLES[1], TOGGLES[1], Vec3::X, -40.0),
        Binding::rotate(TOGGLES[2], TOGGLES[2], Vec3::X, -40.0),
    ]
}

/// Collision of what is not the base drum (the drum is the `engine_reactor` box): the console, the start-up stand, the feed bay,
/// the four pylons and the glass column. Centre and size in model metres, glass column as a cylinder (radius, height).
enum Shape {
    Box([f64; 3]),
    Cylinder(f64, f64),
}

const HULL: [([f64; 3], Shape); 8] = [
    ([0.0, 1.8, 0.0], Shape::Cylinder(0.5, 3.6)),
    ([0.0, 0.5, 1.36], Shape::Box([1.7, 1.0, 1.0])),
    ([-1.47, 0.55, 0.0], Shape::Box([0.6, 1.1, 1.0])),
    ([1.4, 0.65, 0.0], Shape::Box([0.5, 1.3, 1.1])),
    ([0.86, 1.8, 0.86], Shape::Box([0.3, 3.6, 0.3])),
    ([-0.86, 1.8, 0.86], Shape::Box([0.3, 3.6, 0.3])),
    ([0.86, 1.8, -0.86], Shape::Box([0.3, 3.6, 0.3])),
    ([-0.86, 1.8, -0.86], Shape::Box([0.3, 3.6, 0.3])),
];

pub fn plugin(app: &mut App) {
    app.add_message::<EmergencyStop>();
    app.add_systems(Update, spawn_room.after(crate::dev_area::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(FixedUpdate, host_presses.in_set(ShipSet::Asks));
    app.add_plugins(super::start_stand::plugin);
    app.add_systems(FixedUpdate, fill_news.in_set(ShipSet::Fill));
    app.add_systems(FixedUpdate, (mirror_news, scram_notice).in_set(ShipSet::Effects));
    app.add_systems(FixedUpdate, react.after(apply_presses).in_set(crate::phases::Fx::Walker));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (dress_room, bind_visuals, drive_visuals, seat_canisters).chain().after(crate::dev_area::spawn_devices).in_set(crate::phases::Frame::World));
}

fn spawn_room(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    let (ground, up) = frame.ground(&planet, REACTOR_AT[0], REACTOR_AT[1], 0.0);
    let rot = frame.rotation(up, TURN_DEG);
    let shapes: Vec<(Position, Rotation, Collider)> = HULL
        .iter()
        .map(|(at, s)| {
            let c = match s {
                Shape::Box(b) => Collider::cuboid(b[0], b[1], b[2]),
                Shape::Cylinder(r, h) => Collider::cylinder(*r, *h),
            };
            (Position(DVec3::from(*at)), Rotation(DQuat::IDENTITY), c)
        })
        .collect();
    let hull = commands.spawn((RigidBody::Static, Collider::compound(shapes), Position(ground), Rotation(rot), CollisionLayers::new(Layer::World, LayerMask::ALL))).id();
    let room = commands.spawn((ReactorRoom { origin: ground, rot }, machine())).id();
    let screen_rot = rot * DQuat::from_rotation_x(MAIN_TILT_DEG.to_radians());
    let centre = ground + rot * DVec3::from(MAIN_AT) + screen_rot * DVec3::Z * MAIN_OUT;
    let screen = commands.spawn((crate::displays::DisplayPanel { id: MAIN_ID, name: MAIN_NAME.into(), size_m: DVec2::from(MAIN_SIZE), centre, rot: screen_rot }, crate::displays::Bare)).id();
    area.entities.extend([hull, room, screen]);
}

/// Host: a client's (or the host's own) F on a part becomes a press on the kit's machine. The refused set says what the machine will not
/// do now: no second eject on an ejected core.
pub(super) fn host_presses(mut items: ResMut<Items>, sim: Res<ReactorSim>, mut machines: Query<(Entity, &mut Machine), With<ReactorRoom>>, mut out: MessageWriter<PressPart>) {
    let Ok((e, mut m)) = machines.single_mut() else { return };
    m.refused.clear();
    if sim.room.reactor().ejected() {
        m.refused.insert((LEVER_EJECT.into(), 1));
    }
    for a in host_asks(&mut items, |a| matches!(a, ReactorAsk::Press(_))) {
        if let ReactorAsk::Press(i) = a
            && let Some(part) = PRESS.get(i as usize)
        {
            out.write(PressPart { machine: e, part: (*part).into(), hand: None });
        }
    }
}

/// Host: what a press did to the machine. The SCRAM button vents the chamber; the eject lever throws the core out.
pub(super) fn react(mut sounds: MessageReader<PartSound>, mut machines: Query<&mut Machine, With<ReactorRoom>>, mut sim: ResMut<ReactorSim>, mut counts: ResMut<ReactorCounts>, mut stops: MessageWriter<EmergencyStop>) {
    for s in sounds.read() {
        let Ok(mut m) = machines.get_mut(s.machine) else { continue };
        match (s.part.as_str(), &s.outcome) {
            (BUTTON_SCRAM, Outcome::Fired) => match sim.room.scram() {
                Some(lost) => {
                    counts.scrams += 1;
                    info!("reactor: SCRAM, {lost:.0} fill lost");
                    m.controls.close_cover(COVER_SCRAM);
                    stops.write(EmergencyStop { machine: s.machine, cause: EmergencyCause::Scram });
                }
                None => info!("reactor: SCRAM refused (no core)"),
            },
            (LEVER_EJECT, Outcome::Lever { index: 1 }) => {
                sim.eject_left = None;
                for e in sim.room.eject_core() {
                    route(e, &mut counts);
                }
                stops.write(EmergencyStop { machine: s.machine, cause: EmergencyCause::Eject });
            }
            _ => {}
        }
    }
}

/// Host: the parts' state goes into the news the others get.
fn fill_news(mut items: ResMut<Items>, counts: Res<ReactorCounts>, machines: Query<&Machine, With<ReactorRoom>>) {
    if items.role == crate::items::Role::Client {
        return;
    }
    let (Some(news), Ok(m)) = (items.reactor.as_mut(), machines.single()) else { return };
    news.scram_cover = m.controls.is_open(COVER_SCRAM);
    news.eject_cover = m.controls.is_open(COVER_EJECT);
    news.scrams = counts.scrams;
    news.toggles = TOGGLES.map(|t| m.controls.is_on(t));
    news.lever = m.controls.lever_index(LEVER_START).unwrap_or(0);
}

/// Every process: the slots show the canisters in the reactor, the eject lever stays pulled while the core is out (and resets with a new
/// one). A client also takes the covers and the SCRAM button from the host's news.
fn mirror_news(items: Res<Items>, mut machines: Query<&mut Machine, With<ReactorRoom>>, mut seen: Local<Option<u32>>) {
    let (Some(n), Ok(mut m)) = (items.reactor, machines.single_mut()) else { return };
    let c = &mut m.controls;
    for (i, slot) in SLOTS.iter().enumerate() {
        c.set_slot(slot, n.canisters[i].canister.map(|_| i as u32 + 1));
    }
    if n.ejected {
        c.set_lever(LEVER_EJECT, 1);
    } else if c.lever_index(LEVER_EJECT) == Some(1) {
        c.drop_to_off(LEVER_EJECT);
        c.close_cover(COVER_EJECT);
    }
    if items.role == crate::items::Role::Client {
        for (t, on) in TOGGLES.iter().zip(n.toggles) {
            c.set_toggle(t, on);
        }
        c.set_lever(LEVER_START, n.lever);
        c.set_cover(COVER_SCRAM, n.scram_cover);
        c.set_cover(COVER_EJECT, n.eject_cover);
        if counter_fresh(&mut seen, n.scrams) {
            c.set_button_down(BUTTON_SCRAM);
        }
    }
}

fn scram_notice(items: Res<Items>, mut seen: Local<Option<u32>>, mut gp: ResMut<Gameplay>) {
    let Some(news) = items.reactor else { return };
    // The client's mirror above and this notice each keep their own baseline.
    if counter_fresh(&mut seen, news.scrams) {
        gp.notify(Notice::new(NoticeKind::Warning, "notice.reactor.scram"));
    }
}

/// How a part reads on the prompt: the verb, the name and, when F does nothing, why.
pub struct Describe {
    pub verb: &'static str,
    pub name: &'static str,
    pub blocked: Option<&'static str>,
}

pub fn describe(i: usize, m: &Machine, hand_has_canister: bool) -> Describe {
    let p = &PARTS[i];
    let open = m.controls.is_open(p.name);
    let shut = |g: &str| !m.controls.is_open(g);
    match p.name {
        COVER_SCRAM => Describe { verb: if open { "Close" } else { "Open" }, name: "SCRAM cover", blocked: None },
        BUTTON_SCRAM => Describe { verb: "SCRAM", name: "SCRAM button", blocked: shut(COVER_SCRAM).then_some("Cover shut") },
        COVER_EJECT => Describe { verb: if open { "Close" } else { "Open" }, name: "Core eject cover", blocked: None },
        LEVER_EJECT => Describe { verb: "Pull", name: "Core eject lever", blocked: shut(COVER_EJECT).then_some("Cover shut") },
        "toggle_boot_1" | "toggle_boot_2" | "toggle_boot_3" => {
            let name = match p.name {
                "toggle_boot_1" => "Boot toggle 1 (battery bus)",
                "toggle_boot_2" => "Boot toggle 2 (trap)",
                _ => "Boot toggle 3 (injector)",
            };
            Describe { verb: if m.controls.is_on(p.name) { "Switch off" } else { "Switch on" }, name, blocked: None }
        }
        LEVER_START => {
            let at = m.controls.lever_index(LEVER_START).unwrap_or(0);
            let next = (at + 1) % 3;
            let verb = ["Set to PRIME", "Set to RUN", "Set to OFF"][at.min(2) as usize];
            Describe { verb, name: "START lever", blocked: m.refused.contains(&(LEVER_START.into(), next)).then_some("Start not ready") }
        }
        _ => {
            let (name, seated) = if p.name == SLOTS[0] { ("Canister slot 1", m.controls.seated(SLOTS[0])) } else { ("Canister slot 2", m.controls.seated(SLOTS[1])) };
            match slot_action(seated.is_some(), hand_has_canister) {
                SlotAction::Load => Describe { verb: "Insert", name, blocked: None },
                SlotAction::Pull => Describe { verb: "Take out", name, blocked: None },
                SlotAction::Refuse(why) => Describe { verb: if hand_has_canister { "Insert" } else { "Take out" }, name, blocked: Some(why) },
            }
        }
    }
}

/// What F does at a canister slot: the slot looked at decides, never the first free one or the one with most life.
#[derive(Debug, PartialEq)]
pub enum SlotAction {
    Load,
    Pull,
    Refuse(&'static str),
}

pub fn slot_action(seated: bool, hand_has_canister: bool) -> SlotAction {
    match (seated, hand_has_canister) {
        (false, true) => SlotAction::Load,
        (true, false) => SlotAction::Pull,
        (true, true) => SlotAction::Refuse("Slot taken"),
        (false, false) => SlotAction::Refuse("Slot empty"),
    }
}

/// F on part `i`: a press for the host, or the item system's load and pull for a slot.
pub fn press(i: usize, items: &mut Items, now: f64) {
    if i < PRESS.len() {
        items.ask_reactor(ReactorAsk::Press(i as u8));
        return;
    }
    let slot = i - PRESS.len();
    let seated = items.reactor.is_some_and(|r| r.canisters[slot].canister.is_some());
    match slot_action(seated, holds_canister(items)) {
        SlotAction::Load => {
            let bar = items.client.view.selected;
            let _ = items.request(now, item_core::Request::Load { slot: bar, into: slot as u8 });
        }
        SlotAction::Pull => items.ask_pull_from(Some(slot as u8)),
        SlotAction::Refuse(_) => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Window: the model and what the sim drives on it.

/// Rings turn this fast while the reactor runs (rad/s); the second turns the other way.
const RING_SPIN: f32 = 0.6;
/// Seconds the core's glow flickers after a canister goes in.
const FLICKER_S: f32 = 1.2;
/// Seconds the core takes to leave the base, and how far down it goes (m).
const EJECT_S: f32 = 1.5;
const EJECT_DROP: f32 = 3.2;
/// The core's glow when the reactor runs at no output (1 is full output).
const GLOW_IDLE: f32 = 0.25;

#[derive(Component, Default)]
pub struct ReactorVisual {
    bound: bool,
}

#[derive(Component)]
struct Spin(f32);

#[derive(Component)]
struct CoreNode {
    rest: Vec3,
    drop: f32,
}

#[derive(Component)]
struct SlotNode {
    index: usize,
    canister: Option<Entity>,
}

fn dress_room(mut commands: Commands, new: Query<(Entity, &ReactorRoom), Added<ReactorRoom>>, assets: Res<AssetServer>) {
    for (e, r) in &new {
        commands.entity(e).insert((
            bevy::world_serialization::WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(MODEL))),
            Transform::from_rotation(r.rot.as_quat()),
            crate::origin::WorldPos(r.origin),
            KitBindings(bindings()),
            crate::named_material::WantMaterials(&["core_glow", "ring_glow", "lamp_slot_1", "lamp_slot_2", "lamp_prime_1", "lamp_prime_2", "lamp_prime_3", "lamp_prime_4", "lamp_prime_5"]),
            ReactorVisual::default(),
        ));
    }
}

/// Finds the nodes the sim drives (the kit binds the pressable ones).
fn bind_visuals(mut commands: Commands, mut roots: Query<(Entity, &mut ReactorVisual)>, children: Query<&Children>, names: Query<(&Name, &Transform)>) {
    for (root, mut v) in &mut roots {
        if v.bound {
            continue;
        }
        let find = |n: &str| children.iter_descendants(root).find_map(|e| names.get(e).ok().filter(|(x, _)| x.as_str() == n).map(|(_, t)| (e, *t)));
        let (Some((r1, _)), Some((r2, _)), Some((core, ct)), Some((s1, _)), Some((s2, _))) = (find("ring_1"), find("ring_2"), find("core"), find("slot_1"), find("slot_2")) else { continue };
        v.bound = true;
        commands.entity(r1).insert(Spin(1.0));
        commands.entity(r2).insert(Spin(-1.0));
        commands.entity(core).insert(CoreNode { rest: ct.translation, drop: 0.0 });
        commands.entity(s1).insert(SlotNode { index: 0, canister: None });
        commands.entity(s2).insert(SlotNode { index: 1, canister: None });
    }
}

#[derive(Default)]
struct Glow {
    level: f32,
    spin: f32,
    flicker: f32,
    seated: usize,
    base: std::collections::HashMap<&'static str, (LinearRgba, LinearRgba)>,
    /// The first news has been seen: a canister already seated then does not flicker, an ejected core is already out.
    first: bool,
}

/// Pure: the core's glow target from the news: dark with the power off or the core out, else idle up to full with the output.
pub fn glow_target(n: &ReactorNews, max_output_kw: f64) -> f32 {
    if n.ejected || !n.power_on {
        return 0.0;
    }
    let share = if max_output_kw > 0.0 { (n.output_kw / max_output_kw).clamp(0.0, 1.0) as f32 } else { 0.0 };
    GLOW_IDLE + (1.0 - GLOW_IDLE) * share
}

#[allow(clippy::too_many_arguments)]
fn drive_visuals(
    time: Res<Time>,
    items: Res<Items>,
    cfg: Res<crate::dev_area::ship::ReactorConfig>,
    roots: Query<&crate::named_material::NamedMaterials, With<ReactorVisual>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut glow: Local<Glow>,
    mut spins: Query<(&mut Transform, &Spin), Without<CoreNode>>,
    mut cores: Query<(&mut Transform, &mut Visibility, &mut CoreNode)>,
) {
    let Some(n) = items.reactor else { return };
    let dt = time.delta_secs();
    let g = &mut *glow;
    let running = n.power_on && !n.ejected;
    g.level += (glow_target(&n, cfg.0.reactor.max_output_kw) - g.level).clamp(-6.0 * dt, 3.0 * dt);
    g.spin += ((running as u8 as f32) - g.spin).clamp(-dt, dt);
    let seated = n.canisters.iter().filter(|c| c.canister.is_some()).count();
    if g.first && seated > g.seated {
        g.flicker = FLICKER_S;
    }
    g.seated = seated;
    let first = g.first;
    g.first = true;
    g.flicker = (g.flicker - dt).max(0.0);
    let t = time.elapsed_secs();
    let flick = if g.flicker > 0.0 { 0.45 + 0.55 * ((t * 53.0).sin() * (t * 17.0).cos()).abs() } else { 1.0 };
    for (mut tr, s) in &mut spins {
        tr.rotate_local_y(s.0 * RING_SPIN * g.spin * dt);
    }
    for (mut tr, mut vis, mut c) in &mut cores {
        let target = n.ejected as u8 as f32;
        c.drop = if !first && n.ejected { 1.0 } else { c.drop + (target - c.drop).clamp(-dt / EJECT_S, dt / EJECT_S) };
        tr.translation.y = c.rest.y - c.drop * EJECT_DROP;
        *vis = if c.drop >= 1.0 { Visibility::Hidden } else { Visibility::Inherited };
    }
    for named in &roots {
        for (name, scale) in [("core_glow", g.level * flick), ("ring_glow", g.level)] {
            let Some(h) = named.0.get(name) else { continue };
            let Some(mut m) = mats.get_mut(h) else { continue };
            let (emissive, colour) = *g.base.entry(name).or_insert((m.emissive, m.base_color.to_linear()));
            m.emissive = emissive * scale;
            // The lit colour reads as a glow by itself: it goes dark with the emission.
            m.base_color = (colour * (0.06 + 0.94 * scale.min(1.0))).into();
        }
        for (i, name) in ["lamp_slot_1", "lamp_slot_2"].into_iter().enumerate() {
            let Some(mut m) = named.0.get(name).and_then(|h| mats.get_mut(h)) else { continue };
            let on = n.canisters[i].canister.is_some() && n.canisters[i].life > 0.0;
            m.emissive = if on { LinearRgba::rgb(0.1, 1.0, 0.25) * 3.0 } else { LinearRgba::BLACK };
            m.base_color = if on { Color::srgb(0.1, 0.9, 0.25) } else { Color::srgb(0.02, 0.04, 0.05) };
        }
        // The prime column (#319): the segments light one after another, bottom to top, as the trap charges.
        for (i, name) in ["lamp_prime_1", "lamp_prime_2", "lamp_prime_3", "lamp_prime_4", "lamp_prime_5"].into_iter().enumerate() {
            let Some(mut m) = named.0.get(name).and_then(|h| mats.get_mut(h)) else { continue };
            let on = super::start_stand::prime_lamp_on(&n, i);
            m.emissive = if on { LinearRgba::rgb(1.0, 0.7, 0.1) * 3.0 } else { LinearRgba::BLACK };
            m.base_color = if on { Color::srgb(1.0, 0.7, 0.1) } else { Color::srgb(0.02, 0.04, 0.05) };
        }
    }
}

/// A canister sits at each slot that holds one, in the slot node's own frame (the node's +Y points out of the bay).
fn seat_canisters(mut commands: Commands, items: Res<Items>, mut slots: Query<(Entity, &mut SlotNode)>, assets: Res<AssetServer>) {
    let Some(n) = items.reactor else { return };
    for (e, mut s) in &mut slots {
        let seated = n.canisters[s.index].canister.is_some();
        match (seated, s.canister) {
            (true, None) => {
                let c = commands.spawn((bevy::world_serialization::WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(CANISTER_MODEL))), Transform::default())).id();
                commands.entity(e).add_child(c);
                s.canister = Some(c);
            }
            (false, Some(c)) => {
                commands.entity(c).despawn();
                s.canister = None;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> ReactorRoom {
        ReactorRoom { origin: DVec3::ZERO, rot: DQuat::IDENTITY }
    }

    /// Looking at the SCRAM button under a shut cover picks the cover; with the cover open it picks the button.
    #[test]
    fn a_part_under_a_shut_cover_is_the_cover() {
        let r = room();
        let mut m = machine();
        let eye = DVec3::new(0.5, 1.7, 2.8);
        let look = (r.world(PARTS[1].at) - eye).normalize();
        assert_eq!(r.pick(&m, eye, look, 2.5), part_index(COVER_SCRAM));
        m.controls.press(COVER_SCRAM, None, |_| true, |_| true);
        let look = (r.part_at(1, &m) - eye).normalize();
        assert_eq!(r.pick(&m, eye, look, 2.5), part_index(BUTTON_SCRAM));
        assert_eq!(r.pick(&m, eye, look, 0.5), None, "out of reach");
    }

    #[test]
    fn the_glow_is_dark_without_power_or_a_core() {
        let on = ReactorNews { power_on: true, output_kw: 1000.0, ..default() };
        assert_eq!(glow_target(&ReactorNews { power_on: false, ..on }, 2000.0), 0.0);
        assert_eq!(glow_target(&ReactorNews { ejected: true, ..on }, 2000.0), 0.0);
        assert!(glow_target(&on, 2000.0) > GLOW_IDLE);
        assert_eq!(glow_target(&on, 0.0), GLOW_IDLE, "no NaN with no max output");
    }
}

#[cfg(test)]
mod slot_tests {
    use super::*;

    #[test]
    fn f_at_a_slot_follows_the_slot_and_the_hand() {
        assert_eq!(slot_action(false, true), SlotAction::Load);
        assert_eq!(slot_action(true, false), SlotAction::Pull);
        assert_eq!(slot_action(true, true), SlotAction::Refuse("Slot taken"));
        assert_eq!(slot_action(false, false), SlotAction::Refuse("Slot empty"));
    }
}
