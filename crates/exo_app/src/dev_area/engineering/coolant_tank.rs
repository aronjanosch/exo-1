//! The coolant tank on the engine room's east wall (#331): a wall bracket with a tall glass tube, a column in it whose
//! height is the tank's level (blue above the LOW line, amber at or under it), a fixed mark on the back plate at that line, and a
//! filler block under the glass. No screen. With a coolant canister selected and the tank in view, LMB held for `HOLD_S`
//! empties the canister into it: the item authority uses the canister up (`refill_coolant`), `coolant::host_refills` adds
//! `coolant_canister` per cent and gives the canister back when the tank was full. The level reaches every process on the
//! reactor news (`ReactorNews::coolant`), so the glass, the hint and the glug are the same on a client.
//!
//! Layout (model frame: x right, y up from the floor, z out of the wall), greybox TODO(initiator): the places and sizes.
use crate::controls::Controls;
use crate::dev_area::layout::ROOM;
use crate::dev_area::DevArea;
use crate::displays::{ActiveDisplay, USE_RANGE};
use crate::env::PlanetRes;
use crate::interact::{interaction, Interaction};
use crate::items::Items;
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use display_core::hud::{PromptKind, PromptView};
use display_core::Tone;
use item_core::Request;
use walker_core::Frame;

pub use super::coolant::{HOLD_S, REFILL};

/// The tank's wall point on the floor plan: the engine room's east wall (inner face x -9.0), level with the reactor, 3.5 m from
/// it, clear of the door (north wall), the junction box (south wall) and the reactor's start stand. It faces west.
/// TODO(initiator): the place.
pub const TANK_AT: [f64; 2] = [ROOM.centre[0] + ROOM.half[0] - 0.02, ROOM.centre[1] - 1.0];
pub const TANK_TURN: f64 = -90.0;
/// The glass: 0.16 m wide, 1.2 m tall, its foot 0.5 m above the floor; its depth; the plate behind it; the filler under it.
pub const TUBE_W: f64 = 0.16;
pub const TUBE_H: f64 = 1.2;
pub const TUBE_FOOT: f64 = 0.5;
const TUBE_D: f64 = 0.08;
const PLATE: [f64; 3] = [0.32, 1.5, 0.03];
const PLATE_FOOT: f64 = 0.38;
const FILLER: [f64; 3] = [0.22, 0.12, 0.14];
const COLUMN_W: f64 = 0.11;
const COLUMN_D: f64 = 0.05;
const MARK_H: f64 = 0.04;
/// The hit volume the view aims at: plate, glass and filler (model frame, centre and size).
const BODY_SIZE: [f64; 3] = [0.34, 1.54, 0.2];
const BODY_CENTRE: [f64; 3] = [0.0, PLATE_FOOT + BODY_SIZE[1] * 0.5 - 0.04, BODY_SIZE[2] * 0.5];
/// From this level up the glass reads FULL and the hold refuses (a tank that just drained a hair is still full).
const FULL_WORD_AT: f64 = 97.0;
const FULL_AT: f64 = FULL_WORD_AT;
/// A rise of the news level by more than this between two frames is a refill (the glug).
const GLUG_RISE: f64 = 5.0;

/// What the glass says in a word, never a percent. DRY at 0, LOW under the line (the news' `coolant_low`), FULL near the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelWord {
    Dry,
    Low,
    Ok,
    Full,
}

impl LevelWord {
    pub fn of(level: f64, low: bool) -> LevelWord {
        if level <= 0.05 {
            LevelWord::Dry
        } else if low {
            LevelWord::Low
        } else if level >= FULL_WORD_AT {
            LevelWord::Full
        } else {
            LevelWord::Ok
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            LevelWord::Dry => "DRY",
            LevelWord::Low => "LOW",
            LevelWord::Ok => "OK",
            LevelWord::Full => "FULL",
        }
    }
}

/// Whether a canister would add something: the tank is not full.
pub fn takes_coolant(level: f64) -> bool {
    level < FULL_AT
}

/// The column's height (m) for a level in per cent.
pub fn column_height(level: f64) -> f64 {
    TUBE_H * (level / 100.0).clamp(0.0, 1.0)
}

/// Height of the fixed mark above the floor (m) for the LOW line in per cent.
pub fn mark_height(low_line: f64) -> f64 {
    TUBE_FOOT + column_height(low_line)
}

/// The tank in the world: its wall point on the area's ground, turned to face out of the wall.
#[derive(Component, Clone, Copy, Debug)]
pub struct CoolantTank {
    pub origin: DVec3,
    pub rot: DQuat,
}

impl CoolantTank {
    pub fn world(&self, model: [f64; 3]) -> DVec3 {
        self.origin + self.rot * DVec3::from(model)
    }

    /// Whether the view's ray hits the tank's body within `reach`.
    pub fn aims(&self, eye: DVec3, look: DVec3, reach: f64) -> bool {
        let inv = self.rot.inverse();
        let o = inv * (eye - self.origin) - DVec3::from(BODY_CENTRE);
        let v = inv * look.normalize_or_zero();
        device_core::minigame::ray_hits_box(o.to_array(), v.to_array(), [BODY_SIZE[0] * 0.5, BODY_SIZE[1] * 0.5, BODY_SIZE[2] * 0.5], reach)
    }
}

/// The hold at the tank: how long LMB has been held, and what the others were last told (`Items::ask_refill`).
#[derive(Resource, Default, Debug)]
pub struct CoolantHold {
    pub held: f64,
    pub sent: bool,
    /// A refill went through while the button stayed down: the next one needs a new press.
    latched: bool,
}

impl CoolantHold {
    /// The hold's progress (0..1) for the reticle's ring.
    pub fn progress(&self) -> Option<f64> {
        (self.held > 0.0).then(|| (self.held / HOLD_S).clamp(0.0, 1.0))
    }

    fn sync(&mut self, items: &mut Items, running: bool) {
        if self.sent != running {
            self.sent = running;
            items.ask_refill(running);
        }
    }

    fn reset(&mut self, items: &mut Items) {
        self.held = 0.0;
        self.sync(items, false);
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<CoolantHold>();
    app.add_systems(Update, spawn_tank.after(crate::dev_area::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(
        FixedUpdate,
        coolant_hold
            .after(crate::scenario::run_script)
            .after(crate::items_net::items_net_in)
            .before(crate::health::use_input)
            .before(crate::displays::use_display)
            .in_set(crate::phases::Fx::Input),
    );
    app.add_systems(FixedUpdate, tank_hint.after(interaction).in_set(crate::phases::Fx::Walker));
}

fn spawn_tank(mut commands: Commands, planet: Res<PlanetRes>, mut area: ResMut<DevArea>, mut done: Local<Option<warp_core::PlanetId>>) {
    if *done == Some(planet.id) {
        return;
    }
    let Some(frame) = area.frame else { return };
    *done = Some(planet.id);
    let (origin, up) = frame.ground(&planet, TANK_AT[0], TANK_AT[1], 0.0);
    let rot = frame.rotation(up, TANK_TURN);
    let e = commands.spawn(CoolantTank { origin, rot }).id();
    area.entities.push(e);
}

/// A coolant canister is the selected bar slot.
pub fn holds_canister(items: &Items) -> bool {
    let slot = items.client.view.selected;
    items.client.view.slot(slot).and_then(|s| items.defs.of(s)).is_some_and(|d| d.use_effect.as_deref() == Some(REFILL))
}

fn aimed_tank(pl: &Player, tanks: &Query<&CoolantTank>) -> Option<CoolantTank> {
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let look = pl.world_look(Frame::IDENTITY);
    tanks.iter().find(|t| t.aims(eye, look, USE_RANGE)).copied()
}

/// LMB held at the tank with a canister in hand: after `HOLD_S` the use goes to the host. A full tank (by the news) does not start
/// the hold, so nothing is used up.
#[allow(clippy::too_many_arguments)]
fn coolant_hold(time: Res<Time>, mut controls: ResMut<Controls>, mut items: ResMut<Items>, active: Res<ActiveDisplay>, mut hold: ResMut<CoolantHold>, players: Query<&Player>, tanks: Query<&CoolantTank>) {
    let dt = time.delta_secs_f64();
    let Ok(pl) = players.single() else {
        hold.reset(&mut items);
        return;
    };
    if !controls.primary_held {
        hold.latched = false;
    }
    let on_foot = pl.ship.is_none() && !pl.seated && !pl.fly && active.using().is_none() && !controls.free_cursor;
    let level = items.reactor.map(|n| n.coolant);
    let can = on_foot && holds_canister(&items) && level.is_some_and(takes_coolant) && aimed_tank(pl, &tanks).is_some();
    if !can || !controls.primary_held || hold.latched {
        hold.reset(&mut items);
        return;
    }
    hold.held += dt;
    if hold.held >= HOLD_S {
        hold.held = 0.0;
        hold.latched = true;
        controls.click = false;
        let slot = items.client.view.selected;
        let _ = items.request(time.elapsed_secs_f64(), Request::Use { slot, target: crate::items::LOCAL });
        hold.sync(&mut items, false);
        return;
    }
    hold.sync(&mut items, true);
}

/// The prompt when the crosshair is on the tank and nothing else is offered: with a canister "hold LMB: fill coolant", else the
/// tank's name and a level word. Sets the prompt as `interaction` does; F does nothing there.
#[allow(clippy::too_many_arguments)]
fn tank_hint(controls: Res<Controls>, active: Res<ActiveDisplay>, items: Res<Items>, players: Query<&Player>, tanks: Query<&CoolantTank>, mut inter: ResMut<Interaction>) {
    if inter.target.is_some() || !inter.prompt.is_empty() || controls.free_cursor || active.using().is_some() {
        return;
    }
    let (Ok(pl), Some(n)) = (players.single(), items.reactor) else { return };
    if pl.ship.is_some() || pl.seated || pl.fly {
        return;
    }
    let Some(tank) = aimed_tank(pl, &tanks) else { return };
    let word = LevelWord::of(n.coolant, n.coolant_low);
    let mut view = PromptView { name: "Coolant tank".into(), ..PromptView::default() };
    inter.kind = PromptKind::Use;
    if holds_canister(&items) {
        if takes_coolant(n.coolant) {
            view.verb = "Hold LMB".into();
            view.note = "fill coolant".into();
            inter.prompt = "[LMB] hold: fill coolant".into();
        } else {
            view.verb = "Fill".into();
            view.note = "FULL".into();
            inter.kind = PromptKind::Blocked;
            inter.prompt = "Coolant tank: FULL".into();
        }
    } else {
        view.verb = "Look".into();
        view.note = word.text().into();
        if matches!(word, LevelWord::Low | LevelWord::Dry) {
            view.note_tone = Tone::Warn;
        }
        inter.prompt = format!("Coolant tank: {}", word.text());
    }
    inter.view = view;
    inter.at = Some(tank.world([0.0, TUBE_FOOT + TUBE_H * 0.5, 0.1]));
}

// ---------------------------------------------------------------------------------------------
// Window: the glass, the column, the mark and the glug.

#[derive(Component)]
struct Column;

#[derive(Resource, Default)]
struct Glug(Option<Handle<crate::audio::SynthAudio>>);

const GLUG_VOL: f32 = 0.4;
const BLUE: [f32; 3] = [0.1, 0.45, 1.0];
const AMBER: [f32; 3] = [1.0, 0.6, 0.05];
const GLOW: f32 = 2.5;

pub fn window_plugin(app: &mut App) {
    app.init_resource::<Glug>();
    app.add_systems(Startup, glug_setup);
    app.add_systems(Update, (dress_tank, drive_column).chain().after(crate::dev_area::spawn_devices).in_set(crate::phases::Frame::World));
    app.add_systems(Update, glug.in_set(crate::phases::Frame::Input));
}

fn dress_tank(
    mut commands: Commands,
    new: Query<(Entity, &CoolantTank), Added<CoolantTank>>,
    cfg: Option<Res<crate::dev_area::ship::ReactorConfig>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let low = cfg.map_or(25.0, |c| c.0.room.coolant_low);
    for (e, t) in &new {
        let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let mut solid = |c: [f32; 3], rough: f32| materials.add(StandardMaterial { base_color: Color::srgb(c[0], c[1], c[2]), perceptual_roughness: rough, ..default() });
        let plate = solid([0.05, 0.06, 0.07], 0.8);
        let steel = solid([0.3, 0.33, 0.37], 0.6);
        let glass = materials.add(StandardMaterial { base_color: Color::srgba(0.6, 0.8, 1.0, 0.12), alpha_mode: AlphaMode::Blend, perceptual_roughness: 0.1, ..default() });
        let mark = materials.add(StandardMaterial { base_color: Color::srgb(0.95, 0.95, 0.9), emissive: LinearRgba::rgb(1.2, 1.2, 1.1), perceptual_roughness: 0.5, ..default() });
        let column = materials.add(StandardMaterial { base_color: Color::srgb(BLUE[0], BLUE[1], BLUE[2]), emissive: LinearRgba::rgb(BLUE[0] * GLOW, BLUE[1] * GLOW, BLUE[2] * GLOW), perceptual_roughness: 0.3, ..default() });
        let part = |commands: &mut Commands, mat: &Handle<StandardMaterial>, at: [f64; 3], size: [f64; 3]| {
            let c = commands
                .spawn((Mesh3d(cube.clone()), MeshMaterial3d(mat.clone()), Transform { translation: Vec3::new(at[0] as f32, at[1] as f32, at[2] as f32), scale: Vec3::new(size[0] as f32, size[1] as f32, size[2] as f32), ..default() }))
                .id();
            commands.entity(e).add_child(c);
            c
        };
        commands.entity(e).insert((crate::origin::WorldPos(t.origin), Transform::from_rotation(t.rot.as_quat()), Visibility::default()));
        part(&mut commands, &plate, [0.0, PLATE_FOOT + PLATE[1] * 0.5, PLATE[2] * 0.5], PLATE);
        part(&mut commands, &steel, [0.0, TUBE_FOOT - FILLER[1] * 0.5, FILLER[2] * 0.5], FILLER);
        // The LOW line: a bar across the plate, behind the glass, in the plate's width.
        part(&mut commands, &mark, [0.0, mark_height(low), PLATE[2] + 0.004], [PLATE[0] - 0.02, MARK_H, 0.008]);
        let col = part(&mut commands, &column, [0.0, TUBE_FOOT, PLATE[2] + COLUMN_D * 0.5 + 0.01], [COLUMN_W, 0.0, COLUMN_D]);
        commands.entity(col).insert(Column);
        part(&mut commands, &glass, [0.0, TUBE_FOOT + TUBE_H * 0.5, PLATE[2] + TUBE_D * 0.5], [TUBE_W, TUBE_H, TUBE_D]);
    }
}

/// The column's height follows the news; blue above the LOW line, amber at or under it.
fn drive_column(items: Res<Items>, mut columns: Query<(&mut Transform, &mut Visibility, &MeshMaterial3d<StandardMaterial>), With<Column>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let Some(n) = items.reactor else { return };
    let h = column_height(n.coolant);
    let c = if n.coolant_low { AMBER } else { BLUE };
    for (mut t, mut v, mat) in &mut columns {
        t.scale.y = h.max(1e-4) as f32;
        t.translation.y = (TUBE_FOOT + h * 0.5) as f32;
        *v = if h > 0.001 { Visibility::Inherited } else { Visibility::Hidden };
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color = Color::srgb(c[0], c[1], c[2]);
            m.emissive = LinearRgba::rgb(c[0] * GLOW, c[1] * GLOW, c[2] * GLOW);
        }
    }
}

fn glug_setup(mut assets: ResMut<Assets<crate::audio::SynthAudio>>, mut glug: ResMut<Glug>) {
    // A low sweep down reads as a glug. TODO(initiator): by ear; a soft drip while the level is LOW is left out for now.
    glug.0 = Some(assets.add(crate::audio::SynthAudio(crate::audio::Sound::SweepDown)));
}

/// The glug when the level jumps up (a refill by anyone, seen on the news).
fn glug(mut commands: Commands, items: Res<Items>, sounds: Res<Glug>, mut last: Local<Option<f64>>) {
    let Some(n) = items.reactor else { return };
    let before = last.replace(n.coolant);
    if let (Some(before), Some(h)) = (before, &sounds.0)
        && n.coolant - before > GLUG_RISE
    {
        crate::audio_slice::one_shot(&mut commands, h, GLUG_VOL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tank() -> CoolantTank {
        CoolantTank { origin: DVec3::ZERO, rot: DQuat::IDENTITY }
    }

    #[test]
    fn the_level_word_follows_the_tank_and_never_shows_a_percent() {
        assert_eq!(LevelWord::of(0.0, true), LevelWord::Dry);
        assert_eq!(LevelWord::of(10.0, true), LevelWord::Low);
        assert_eq!(LevelWord::of(60.0, false), LevelWord::Ok);
        assert_eq!(LevelWord::of(100.0, false), LevelWord::Full);
        assert!(["DRY", "LOW", "OK", "FULL"].contains(&LevelWord::of(42.0, false).text()));
    }

    #[test]
    fn a_full_tank_takes_no_canister_and_the_column_is_clamped() {
        assert!(takes_coolant(50.0) && takes_coolant(90.0));
        assert!(!takes_coolant(100.0) && !takes_coolant(99.0) && !takes_coolant(97.0));
        assert_eq!(column_height(0.0), 0.0);
        assert!((column_height(50.0) - TUBE_H * 0.5).abs() < 1e-12);
        assert!((column_height(250.0) - TUBE_H).abs() < 1e-12 && column_height(-3.0) == 0.0);
        assert!(mark_height(25.0) > TUBE_FOOT && mark_height(25.0) < TUBE_FOOT + TUBE_H);
    }

    #[test]
    fn the_view_aims_at_the_tank_body() {
        let t = tank();
        let eye = DVec3::new(0.0, 1.3, 1.8);
        assert!(t.aims(eye, DVec3::NEG_Z, USE_RANGE), "straight at the glass");
        assert!(!t.aims(eye, DVec3::Z, USE_RANGE), "away");
        assert!(!t.aims(DVec3::new(2.0, 1.3, 1.8), DVec3::NEG_Z, USE_RANGE), "beside it");
        assert!(!t.aims(DVec3::new(0.0, 1.3, USE_RANGE + 1.0), DVec3::NEG_Z, USE_RANGE), "out of reach");
    }

    #[test]
    fn the_tank_stands_inside_the_engine_room_on_the_east_wall_clear_of_the_reactor() {
        assert!(ROOM.contains(TANK_AT) || (TANK_AT[0] - (ROOM.centre[0] + ROOM.half[0])).abs() < 0.05);
        let reactor = crate::dev_area::layout::REACTOR_AT;
        assert!(TANK_AT[0] - reactor[0] > crate::dev_area::layout::REACTOR_SIZE[0] * 0.5 + 1.0, "the reactor's drum is far from the tank");
    }

    #[test]
    fn the_coolant_canister_is_the_refill_item() {
        let defs = crate::items::load_defs();
        assert_eq!(defs.get("coolant_canister").expect("coolant_canister.json").use_effect.as_deref(), Some(REFILL));
    }
}
