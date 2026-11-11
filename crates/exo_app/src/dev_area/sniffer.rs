//! The sniffer and the fizz pockets (#333), a second errand outside beside the ore veins (#324). Hidden pockets of
//! fizz gas lie on the free ground around the dig site (`dig_core::pocket`, values in `gameplay/dig/dig.json`).
//! - The sniffer (`content/items/sniffer.json`) ticks faster and its back bar grows the nearer a pocket is. Every
//!   client computes its own signal from the pocket places the host sends, so the tick is instant and local.
//! - LMB held with the sniffer selected within the tap radius of a hidden pocket sends the tap (`TapAsk`, as the
//!   welder's cut aim); the host steps it and the pocket opens: a vent pops out and puffs.
//! - F at an open vent with an empty jar selected (`Request::Load`, the fuse's path at a junction box) turns one jar
//!   into a fizz jar; the host takes the jar's gas from the pocket. Three jars spend a pocket, a new one forms.
//! The host's state is `PocketHost`; `Items::dig.pockets` is what every machine reads. The bar, the vent, the puffs
//! and the sounds are window only.
use crate::controls::{Actions, Controls, Tap};
use crate::dev_area::{area_of, layout, DevArea, DIG_SITE, HALF_EXTENT, ZONES};
use crate::displays::ActiveDisplay;
use crate::env::PlanetRes;
use crate::interact::{interaction, Interaction};
use crate::items::{Items, Role};
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::DVec3;
use bevy::prelude::*;
use dig_core::pocket::{signal_at, Ground, PocketField, PocketState, Rect};
use dig_core::DigConfig;
use display_core::hud::{PromptKind, PromptView};
use display_core::Tone;
use item_core::wire::{PocketNew, PocketNews, TapAsk};
use item_core::{PlayerId, Request};
use std::collections::BTreeMap;
use walker_core::Frame as WalkFrame;

/// The sniffer's item id and the empty and full jars'.
pub const SNIFFER: &str = "sniffer";
pub const FIZZ_JAR: &str = "fizz_jar";
/// The jar's `use` in `content/items/jar.json`.
pub const FILL_JAR: &str = "fill_jar";
/// The fixed seed of the pocket places. TODO(initiator): per save or per run instead of one fixed field?
pub const SEED: u64 = 0x00F1_22C0_FFEE;
/// Seconds between two tap asks while it is held (the host forgets a tap that stays quiet for twice that).
pub const TAP_EVERY_S: f64 = 0.25;
/// Seconds between two snapshots of changed pockets (4 Hz).
pub const SEND_EVERY_S: f64 = 0.25;
/// A vent's chimney: width and height (m). TODO(initiator): the size.
pub const VENT_SIZE: [f32; 2] = [0.25, 0.3];
/// F at a vent reaches a point at this half angle around the look (rad, about 26 degrees); the vent is small and low.
const VENT_CONE: f64 = 0.45;

#[derive(Resource, Clone)]
pub struct PocketCfg(pub DigConfig);

/// Host: the pockets and who holds a tap where (area metres) and when that was last said (s).
#[derive(Resource)]
pub struct PocketHost {
    pub field: PocketField,
    pub tappers: BTreeMap<PlayerId, ([f64; 2], f64)>,
    /// Jars handed out so far (for the scenario and the log).
    pub filled: u32,
    sent: Option<PocketNews>,
    since: f64,
}

/// The own sniffer's reading, from the pocket news and the own feet: `on` while the sniffer is the selected item.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct Sniffing {
    pub on: bool,
    pub signal: f64,
    /// The tap's progress on the nearest tapped pocket, 0..1, and whether LMB is held for it now.
    pub tap: f64,
    pub tapping: bool,
}

#[derive(Resource, Default)]
struct TapClient {
    on: bool,
    since: f64,
}

/// Where pockets may lie: the dig site's field minus the site, the mini ship's rooms, the pad zone and the other
/// open zones, inside the dev area's flattened rectangle.
pub fn ground(cfg: &DigConfig) -> Ground {
    let h = cfg.pocket.field_half_m;
    let c = DIG_SITE.centre;
    let lo = [(c[0] - h).max(-HALF_EXTENT[0]), (c[1] - h).max(-HALF_EXTENT[1])];
    let hi = [(c[0] + h).min(HALF_EXTENT[0]), (c[1] + h).min(HALF_EXTENT[1])];
    let rooms = [layout::ROOM, layout::CORRIDOR, layout::BRIDGE, layout::TURRET_ROOM, layout::LAB_ROOM, layout::MEDBAY_ROOM, layout::STORAGE_ROOM, layout::EQUIPMENT, layout::AIRLOCK];
    let blocked = ZONES.iter().chain(rooms.iter()).map(|z| Rect { centre: z.centre, half: z.half }).collect();
    Ground { lo, hi, blocked }
}

pub fn plugin(app: &mut App) {
    let cfg = DigConfig::load(&crate::content_files("gameplay")).unwrap_or_else(|e| panic!("dig: {e:?}"));
    let field = PocketField::new(&cfg.pocket, ground(&cfg), SEED);
    app.insert_resource(PocketHost { field, tappers: BTreeMap::new(), filled: 0, sent: None, since: 0.0 })
        .insert_resource(PocketCfg(cfg))
        .init_resource::<Sniffing>()
        .init_resource::<TapClient>();
    app.add_systems(FixedUpdate, tap_input.after(crate::scenario::run_script).in_set(crate::phases::Fx::Input));
    app.add_systems(FixedUpdate, pocket_step.after(tap_input).after(crate::items_net::items_net_in).in_set(crate::phases::Fx::Input));
    app.add_systems(FixedUpdate, sniffing.after(pocket_step).in_set(crate::phases::Fx::Input));
    app.add_systems(FixedUpdate, vent_press.before(interaction).in_set(crate::phases::Fx::Walker));
    app.add_systems(FixedUpdate, vent_hint.after(interaction).in_set(crate::phases::Fx::Walker));
}

fn selected_is(items: &Items, effect_of: &str) -> bool {
    let v = &items.client.view;
    v.slot(v.selected).is_some_and(|s| s.item.kind.as_str() == effect_of)
}

/// The selected slot holds an empty jar (the item whose use is `fill_jar`).
pub fn holds_jar(items: &Items) -> bool {
    let v = &items.client.view;
    v.slot(v.selected).and_then(|s| items.defs.of(s)).is_some_and(|d| d.use_effect.as_deref() == Some(FILL_JAR))
}

/// Whether the bar has room for the fizz jar a fill makes, once the selected jar is used up.
pub fn room_for_fizz(items: &Items) -> bool {
    let v = &items.client.view;
    let Some(def) = items.defs.get(FIZZ_JAR) else { return false };
    let mut bar = v.bar.clone();
    bar.take(&[], v.selected, 1).is_ok() && matches!(bar.try_insert(&[], def.stack_of(1)), Ok(None))
}

fn on_foot(pl: &Player) -> bool {
    pl.ship.is_none() && !pl.seated && !pl.fly
}

fn tap_input(time: Res<Time>, controls: Res<Controls>, active: Res<ActiveDisplay>, players: Query<&Player>, mut items: ResMut<Items>, mut client: ResMut<TapClient>) {
    let now = time.elapsed_secs_f64();
    let hold = !controls.free_cursor
        && controls.primary_held
        && active.using().is_none()
        && players.single().is_ok_and(on_foot)
        && selected_is(&items, SNIFFER);
    match (hold, players.single()) {
        (true, Ok(pl)) => {
            if !client.on || now - client.since >= TAP_EVERY_S {
                items.ask_tap(TapAsk::Hold { at: pl.w.pos.to_array() });
                client.on = true;
                client.since = now;
            }
        }
        _ => {
            if client.on {
                items.ask_tap(TapAsk::Stop);
            }
            *client = TapClient::default();
        }
    }
}

fn significant(a: &PocketNews, b: &PocketNews) -> bool {
    a.pockets.len() != b.pockets.len() || a.pockets.iter().zip(&b.pockets).any(|(x, y)| x.at != y.at || x.jars != y.jars || (x.open_ds > 0) != (y.open_ds > 0) || (x.tap > 0) != (y.tap > 0))
}

/// Host: steps the pockets with the taps held now, fills the jars taken out of hands, and sends the news.
#[allow(clippy::too_many_arguments)]
fn pocket_step(time: Res<Time>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, cfg: Res<PocketCfg>, mut items: ResMut<Items>, mut host: ResMut<PocketHost>) {
    if items.role == Role::Client {
        return;
    }
    let (dt, now) = (time.delta_secs_f64(), time.elapsed_secs_f64());
    let frame = area.as_deref().and_then(|a| a.frame);
    for (who, a) in std::mem::take(&mut items.dig.tap_asks) {
        match (a, frame) {
            (TapAsk::Hold { at }, Some(frame)) => {
                let p = DVec3::from_array(at);
                if p.is_finite() {
                    host.tappers.insert(who, (area_of(&frame, &planet, p), now));
                }
            }
            (TapAsk::Hold { .. }, None) => {}
            (TapAsk::Stop, _) => {
                host.tappers.remove(&who);
            }
        }
    }
    // A tapper without the sniffer in the hand, or who has gone quiet, stops tapping.
    let sniffing: Vec<PlayerId> = host
        .tappers
        .iter()
        .filter(|(who, (_, seen))| now - *seen <= 2.0 * TAP_EVERY_S + 0.1 && items.host.inventories.get(who).and_then(|inv| inv.slot(inv.selected)).is_some_and(|s| s.item.kind.as_str() == SNIFFER))
        .map(|(who, _)| *who)
        .collect();
    host.tappers.retain(|who, _| sniffing.contains(who));
    let at: Vec<[f64; 2]> = host.tappers.values().map(|(p, _)| *p).collect();
    host.field.step(dt, &at);
    for (who, into) in std::mem::take(&mut items.dig.jar_fills) {
        let gave = matches!(host.field.fill(into as usize), dig_core::pocket::Fill::Jar { .. });
        // The jar's gas, or the jar back when the vent is closed or spent (two players, one vent).
        let back = if gave { items.defs.get(FIZZ_JAR) } else { items.defs.get("jar") };
        if let Some(def) = back.cloned() {
            if gave {
                host.filled += 1;
            }
            if let Err(stack) = items.give(now, who, def.stack_of(1)) {
                warn!("sniffer: no room to hand a {} to player {who}, {} lost", def.id, stack.count);
            }
        }
    }
    let tap_s = cfg.0.pocket.tap_s;
    let news = PocketNews {
        pockets: host
            .field
            .pockets()
            .iter()
            .map(|p| PocketNew {
                at: p.at,
                jars: p.jars_left.min(255) as u8,
                open_ds: match p.state {
                    PocketState::Open { left_s } => (left_s * 10.0).ceil().clamp(1.0, u16::MAX as f64) as u16,
                    _ => 0,
                },
                tap: (p.tap / tap_s * 1000.0).round().clamp(0.0, 1000.0) as u16,
            })
            .collect(),
    };
    items.dig.pockets = Some(news.clone());
    host.since += dt;
    if items.role == Role::Host && host.sent.as_ref().is_none_or(|s| significant(s, &news)) && host.since >= SEND_EVERY_S {
        items.net_out.push(crate::items::NetOut::Pockets { to: None, n: news.clone() });
        host.sent = Some(news);
        host.since = 0.0;
    }
}

/// The own sniffer's reading, on every machine, from the news and the own feet.
fn sniffing(planet: Res<PlanetRes>, area: Option<Res<DevArea>>, cfg: Res<PocketCfg>, items: Res<Items>, controls: Res<Controls>, players: Query<&Player>, mut out: ResMut<Sniffing>) {
    let (Some(frame), Ok(pl), Some(news)) = (area.as_deref().and_then(|a| a.frame), players.single(), items.dig.pockets.as_ref()) else {
        *out = Sniffing::default();
        return;
    };
    if !selected_is(&items, SNIFFER) || !on_foot(pl) {
        *out = Sniffing::default();
        return;
    }
    let me = area_of(&frame, &planet, pl.w.pos);
    let signal = signal_at(&cfg.0.pocket, news.pockets.iter().map(|p| p.at), me);
    let r = cfg.0.pocket.tap_radius_m;
    let tap = news.pockets.iter().filter(|p| ((p.at[0] - me[0]).powi(2) + (p.at[1] - me[1]).powi(2)).sqrt() <= r).map(|p| p.tap as f64 / 1000.0).fold(0.0, f64::max);
    *out = Sniffing { on: true, signal, tap, tapping: controls.primary_held && !controls.free_cursor };
}

/// The open vent the look rests on within reach: its index and its world point.
pub fn aimed_vent(frame: &crate::town::Frame, planet: &PlanetRes, cfg: &DigConfig, news: &PocketNews, eye: DVec3, look: DVec3) -> Option<(usize, DVec3)> {
    news.pockets
        .iter()
        .enumerate()
        .filter(|(_, p)| p.open_ds > 0)
        .filter_map(|(i, p)| {
            let (g, up) = frame.ground(planet, p.at[0], p.at[1], VENT_SIZE[1] as f64 * 0.5);
            grab_core::in_cone(eye, look, g, VENT_CONE, cfg.pocket.reach_m).map(|d| (i, g, d, up))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(i, g, _, _)| (i, g))
}

/// F at an open vent with an empty jar selected: asks the host to fill it. Before `interaction`, which takes the tap
/// otherwise; without a jar the tap is left to it.
#[allow(clippy::too_many_arguments)]
fn vent_press(time: Res<Time>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, cfg: Res<PocketCfg>, mut actions: ResMut<Actions>, controls: Res<Controls>, active: Res<ActiveDisplay>, cw: Res<crate::container_window::ContainerWindow>, mut items: ResMut<Items>, players: Query<&Player>) {
    if !actions.taps().contains(&Tap::Interact) || controls.free_cursor || active.using().is_some() || cw.rack.is_some() || !holds_jar(&items) {
        return;
    }
    let (Some(frame), Ok(pl)) = (area.as_deref().and_then(|a| a.frame), players.single()) else { return };
    let Some(news) = items.dig.pockets.as_ref() else { return };
    if !on_foot(pl) {
        return;
    }
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let Some((i, _)) = aimed_vent(&frame, &planet, &cfg.0, news, eye, pl.world_look(WalkFrame::IDENTITY)) else { return };
    actions.take_tap(Tap::Interact);
    if room_for_fizz(&items) {
        let slot = items.client.view.selected;
        let _ = items.request(time.elapsed_secs_f64(), Request::Load { slot, into: i as u8 });
    }
}

/// The hint at an open vent when nothing else is offered: F fills a jar, or what is missing. Sets the prompt as `dig_hint` does.
#[allow(clippy::too_many_arguments)]
fn vent_hint(planet: Res<PlanetRes>, area: Option<Res<DevArea>>, cfg: Res<PocketCfg>, bindings: Res<crate::controls::Bindings>, controls: Res<Controls>, active: Res<ActiveDisplay>, items: Res<Items>, players: Query<&Player>, mut inter: ResMut<Interaction>) {
    if inter.target.is_some() || !inter.prompt.is_empty() || controls.free_cursor || active.using().is_some() {
        return;
    }
    let (Some(frame), Ok(pl)) = (area.as_deref().and_then(|a| a.frame), players.single()) else { return };
    let Some(news) = items.dig.pockets.as_ref() else { return };
    if !on_foot(pl) {
        return;
    }
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let Some((_, p)) = aimed_vent(&frame, &planet, &cfg.0, news, eye, pl.world_look(WalkFrame::IDENTITY)) else { return };
    let key = crate::interact::key_label(&bindings, Tap::Interact);
    let mut view = PromptView { name: "Fizz vent".into(), ..PromptView::default() };
    inter.kind = PromptKind::Use;
    if !holds_jar(&items) {
        view.verb = "Fill jar".into();
        view.note = "▲ Needs an empty jar".into();
        view.note_tone = Tone::Warn;
        inter.kind = PromptKind::Blocked;
        inter.prompt = format!("[{key}] fizz vent: needs an empty jar");
    } else if !room_for_fizz(&items) {
        view.verb = "Fill jar".into();
        view.note = "▲ Bar full: free a slot".into();
        view.note_tone = Tone::Warn;
        inter.kind = PromptKind::Blocked;
        inter.prompt = format!("[{key}] fizz vent: the bar is full, free a slot");
    } else {
        view.verb = "Fill jar".into();
        inter.prompt = format!("[{key}] fill jar");
    }
    inter.view = view;
    inter.at = Some(p);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev_area::layout;

    fn cfg() -> DigConfig {
        DigConfig::load(&crate::content_files("gameplay")).expect("dig.json")
    }

    #[test]
    fn the_free_ground_is_big_and_no_pocket_lands_in_a_zone_or_a_room() {
        let c = cfg();
        let g = ground(&c);
        for seed in 0..50 {
            let f = PocketField::new(&c.pocket, g.clone(), seed);
            assert_eq!(f.pockets().len(), c.pocket.pockets as usize, "seed {seed}: there is room for every pocket");
            for p in f.pockets() {
                let near = |z: &crate::dev_area::Zone| (p.at[0] - z.centre[0]).abs() <= z.half[0] && (p.at[1] - z.centre[1]).abs() <= z.half[1];
                assert!(!ZONES.iter().any(near), "{:?} lies in an open zone", p.at);
                for z in [layout::ROOM, layout::CORRIDOR, layout::BRIDGE, layout::TURRET_ROOM, layout::LAB_ROOM, layout::MEDBAY_ROOM, layout::STORAGE_ROOM, layout::EQUIPMENT, layout::AIRLOCK] {
                    assert!(!near(&z), "{:?} lies in the {}", p.at, z.name);
                }
                assert!(p.at[0].abs() <= HALF_EXTENT[0] && p.at[1].abs() <= HALF_EXTENT[1], "{:?} is off the flattened area", p.at);
            }
        }
    }

    #[test]
    fn the_sniffer_and_the_jars_are_items_with_the_right_uses_and_tags() {
        let d = crate::items::load_defs();
        let s = d.get(SNIFFER).expect("sniffer.json");
        assert_eq!((s.stack, s.hands, s.keeps_on_use), (1, 1, true));
        assert!(s.tags.iter().any(|t| t == "tool"));
        let j = d.get("jar").expect("jar.json");
        assert_eq!((j.stack, j.use_effect.as_deref()), (4, Some(FILL_JAR)));
        let f = d.get(FIZZ_JAR).expect("fizz_jar.json");
        assert!(f.tags.iter().any(|t| t == "chem") && f.use_effect.is_none());
    }

    #[test]
    fn the_fizz_recipe_makes_a_full_extinguisher_from_two_fizz_jars() {
        let d = crate::items::load_defs();
        let fab = production_core::fabricator::FabConfig::load(&crate::content_files("gameplay")).expect("fabricator.json");
        let r = fab.recipes.iter().find(|r| r.output.item == "extinguisher").expect("the extinguisher recipe");
        assert_eq!((r.inputs.len(), r.inputs[0].item.as_str(), r.inputs[0].count, r.seconds), (1, FIZZ_JAR, 2, 20.0));
        assert_eq!(d.get("extinguisher").unwrap().stack_of(1).item.condition, Some(100.0), "the made one is full");
    }
}

// ---------- window only ----------

/// The bar on the sniffer's back (the face toward the player in the first-person hold, `items::held_offset`): the box is
/// 0.08 x 0.06 x 0.22, its back face lies at z +0.11 in the held item's frame. Width and height of the bar (m).
const BAR_W: f32 = 0.07;
const BAR_H: f32 = 0.014;
const BAR_Z: f32 = 0.1125;
/// Tick rate (per second) at the edge of the range and on the pocket; above `BUZZ_FROM` the ticks become a buzz.
/// TODO(initiator): by ear.
const TICK_RATE: (f64, f64) = (0.67, 12.0);
const BUZZ_FROM: f64 = 0.95;
const TICK_VOL: f32 = 0.18;
const BUZZ_VOL: f32 = 0.05;
/// The buzz is the pump's hum played this much faster (60 Hz times this).
const BUZZ_SPEED: f32 = 4.0;
/// The hiss of an open vent: its volume and how far it carries (m).
const HISS_VOL: f32 = 0.12;
const HISS_RANGE_M: f64 = 14.0;
/// Seconds a vent takes to pop out and to sink back.
const POP_S: f32 = 0.3;
const SINK_S: f32 = 0.8;
/// Seconds between two puffs of an open vent, and a puff's size (m) and life (s).
const PUFF_EVERY_S: f64 = 0.07;
const PUFF_SIZE: f32 = 0.3;
const PUFF_LIFE_S: f32 = 1.7;

#[derive(Component)]
struct BarFill {
    mat: Handle<StandardMaterial>,
}

#[derive(Component)]
struct BarBack;

#[derive(Resource)]
struct BarAssets {
    cube: Handle<Mesh>,
    back: Handle<StandardMaterial>,
}

#[derive(Component)]
struct Vent {
    /// How far it is out of the ground, 0..1, and where it stands now (area metres).
    k: f32,
    at: Option<[f64; 2]>,
    chimney: Entity,
    patch: Entity,
}

#[derive(Resource, Default)]
struct VentVisuals {
    planet: Option<warp_core::PlanetId>,
    roots: Vec<Entity>,
    chimney_mesh: Option<Handle<Mesh>>,
    chimney_mat: Option<Handle<StandardMaterial>>,
    puff: f64,
}

#[derive(Resource, Default)]
struct SniffSounds {
    click: Option<Handle<crate::audio::SynthAudio>>,
}

#[derive(Component)]
struct BuzzLoop;

#[derive(Component)]
struct HissLoop;

pub fn window_plugin(app: &mut App) {
    app.init_resource::<VentVisuals>().init_resource::<SniffSounds>();
    app.add_systems(Startup, (bar_assets, audio_setup));
    app.add_systems(Update, bar_sync.in_set(crate::phases::Frame::World));
    app.add_systems(Update, vent_sync.in_set(crate::phases::Frame::Sync));
    app.add_systems(Update, sniffer_sounds.in_set(crate::phases::Frame::Input));
}

fn bar_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let back = materials.add(StandardMaterial { base_color: Color::srgb(0.02, 0.05, 0.05), unlit: true, ..default() });
    commands.insert_resource(BarAssets { cube, back });
}

/// The bar's colour for a signal: green, yellow, white.
pub fn bar_colour(signal: f64) -> [f32; 3] {
    let s = signal.clamp(0.0, 1.0) as f32;
    const GREEN: [f32; 3] = [0.1, 0.9, 0.2];
    const YELLOW: [f32; 3] = [1.0, 0.9, 0.1];
    const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
    let (a, b, k) = if s < 0.5 { (GREEN, YELLOW, s * 2.0) } else { (YELLOW, WHITE, (s - 0.5) * 2.0) };
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

/// A bar and its dark track on the held sniffer; the bar's length follows the signal and it pulses while the tap runs.
#[allow(clippy::too_many_arguments)]
fn bar_sync(
    mut commands: Commands,
    time: Res<Time>,
    assets: Option<Res<BarAssets>>,
    sniff: Res<Sniffing>,
    held: Query<(Entity, &crate::items::HeldModel, Option<&Children>)>,
    mut bars: Query<(&BarFill, &mut Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(assets) = assets else { return };
    let t = time.elapsed_secs();
    for (e, m, children) in &held {
        let existing = children.and_then(|c| c.iter().find(|ch| bars.contains(*ch)));
        if m.kind() != Some(SNIFFER) {
            continue;
        }
        let Some(bar) = existing else {
            let mat = materials.add(StandardMaterial { base_color: Color::BLACK, emissive: LinearRgba::rgb(0.4, 3.6, 0.8), unlit: true, ..default() });
            commands.entity(e).with_children(|c| {
                c.spawn((BarBack, bevy::light::NotShadowCaster, Mesh3d(assets.cube.clone()), MeshMaterial3d(assets.back.clone()), Transform::from_xyz(0.0, 0.0, 0.111).with_scale(Vec3::new(BAR_W + 0.006, BAR_H + 0.006, 0.002))));
                c.spawn((BarFill { mat: mat.clone() }, bevy::light::NotShadowCaster, Mesh3d(assets.cube.clone()), MeshMaterial3d(mat), Transform::from_xyz(-BAR_W * 0.5, 0.0, BAR_Z).with_scale(Vec3::new(0.004, BAR_H, 0.003))));
            });
            continue;
        };
        let Ok((fill, mut tr)) = bars.get_mut(bar) else { continue };
        let signal = sniff.signal as f32;
        let len = 0.004 + (BAR_W - 0.004) * signal;
        tr.scale = Vec3::new(len, BAR_H, 0.003);
        tr.translation = Vec3::new(-BAR_W * 0.5 + len * 0.5, 0.0, BAR_Z);
        // While the tap runs the bar pulses, faster the nearer the pocket is to opening.
        let pulse = if sniff.tapping && sniff.tap > 0.0 { 0.35 + 0.65 * (0.5 + 0.5 * (t * std::f32::consts::TAU * (2.0 + 6.0 * sniff.tap as f32)).sin()) } else { 1.0 };
        let c = bar_colour(sniff.signal);
        if let Some(mut m) = materials.get_mut(&fill.mat) {
            m.emissive = LinearRgba::rgb(c[0] * 4.0 * pulse, c[1] * 4.0 * pulse, c[2] * 4.0 * pulse);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn vent_sync(
    mut commands: Commands,
    time: Res<Time>,
    planet: Res<PlanetRes>,
    area: Option<Res<DevArea>>,
    items: Res<Items>,
    fx: Option<Res<crate::effects::FxAssets>>,
    mut vis: ResMut<VentVisuals>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut vents: Query<(&mut Vent, &mut crate::origin::WorldPos, &mut Transform), Without<VentPart>>,
    mut parts: Query<(&mut Transform, &mut Visibility), With<VentPart>>,
) {
    let Some(frame) = area.as_deref().and_then(|a| a.frame) else { return };
    if vis.planet != Some(planet.id) {
        vis.planet = Some(planet.id);
        for e in vis.roots.drain(..) {
            commands.entity(e).despawn();
        }
        vis.chimney_mesh = Some(meshes.add(Cuboid::new(1.0, 1.0, 1.0)));
        vis.chimney_mat = Some(materials.add(StandardMaterial { base_color: Color::srgb(0.08, 0.08, 0.09), perceptual_roughness: 0.9, ..default() }));
    }
    let (Some(news), Some(mesh), Some(mat)) = (items.dig.pockets.as_ref(), vis.chimney_mesh.clone(), vis.chimney_mat.clone()) else { return };
    while vis.roots.len() < news.pockets.len() {
        let chimney = commands.spawn((VentPart, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden)).id();
        let patch = commands.spawn((VentPart, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::default(), Visibility::Hidden)).id();
        let root = commands.spawn((Vent { k: 0.0, at: None, chimney, patch }, crate::origin::WorldPos(DVec3::ZERO), Transform::default(), Visibility::default())).id();
        commands.entity(root).add_child(chimney).add_child(patch);
        vis.roots.push(root);
    }
    let dt = time.delta_secs();
    let mut open_tops: Vec<(DVec3, DVec3)> = Vec::new();
    for (i, root) in vis.roots.clone().iter().enumerate() {
        let Ok((mut v, mut wp, mut tr)) = vents.get_mut(*root) else { continue };
        let Some(p) = news.pockets.get(i) else { continue };
        let open = p.open_ds > 0;
        // A vent that has to move (its pocket was spent and a new one formed) sinks first.
        let same = v.at == Some(p.at);
        if !same && v.k <= 0.001 {
            v.at = Some(p.at);
            let (g, up) = frame.ground(&planet, p.at[0], p.at[1], 0.0);
            wp.0 = g;
            tr.rotation = frame.rotation(up, 0.0).as_quat();
        }
        let want = open && v.at == Some(p.at);
        let step = dt / if want { POP_S } else { SINK_S };
        v.k = if want { (v.k + step).min(1.0) } else { (v.k - step).max(0.0) };
        let ease = 1.0 - (1.0 - v.k).powi(2);
        let [w, h] = VENT_SIZE;
        if let Ok((mut t, mut vi)) = parts.get_mut(v.chimney) {
            t.translation = Vec3::new(0.0, ease * h - h * 0.5, 0.0);
            t.scale = Vec3::new(w, h, w);
            *vi = if v.k > 0.001 { Visibility::Inherited } else { Visibility::Hidden };
        }
        if let Ok((mut t, mut vi)) = parts.get_mut(v.patch) {
            t.translation = Vec3::new(0.0, 0.01, 0.0);
            t.scale = Vec3::new(w * 2.2 * ease, 0.02, w * 2.2 * ease);
            *vi = if v.k > 0.001 { Visibility::Inherited } else { Visibility::Hidden };
        }
        if want && v.k > 0.6 {
            let (top, up) = frame.ground(&planet, p.at[0], p.at[1], (h * ease) as f64);
            open_tops.push((top, up));
        }
    }
    vis.puff += time.delta_secs_f64();
    if let Some(fx) = fx.as_deref()
        && vis.puff >= PUFF_EVERY_S
    {
        vis.puff = 0.0;
        let t = time.elapsed_secs_f64();
        for (k, (top, up)) in open_tops.iter().enumerate() {
            let seed = (t * 1000.0) as u64 ^ ((k as u64) << 24);
            crate::effects::spawn_particle(&mut commands, fx, &mut materials, crate::effects::Kind::Steam, *top, crate::effects::jet(*up, seed, 1.3, 0.25), 0.0, PUFF_SIZE, PUFF_LIFE_S);
        }
    }
}

#[derive(Component)]
struct VentPart;

fn audio_setup(mut commands: Commands, mut assets: ResMut<Assets<crate::audio::SynthAudio>>, mut sounds: ResMut<SniffSounds>) {
    use crate::audio::{Sound, SynthAudio};
    use bevy::audio::{PlaybackMode, PlaybackSettings, Volume};
    sounds.click = Some(assets.add(SynthAudio(Sound::Click)));
    let buzz = assets.add(SynthAudio(Sound::PumpHum));
    let hiss = assets.add(SynthAudio(Sound::Hiss));
    commands.spawn((AudioPlayer(buzz), PlaybackSettings { mode: PlaybackMode::Loop, volume: Volume::Linear(0.0), speed: BUZZ_SPEED, ..default() }, BuzzLoop));
    commands.spawn((AudioPlayer(hiss), PlaybackSettings { mode: PlaybackMode::Loop, volume: Volume::Linear(0.0), ..default() }, HissLoop));
}

/// The tick (faster the nearer a pocket is), the buzz on top of it, and the hiss of open vents within hearing.
#[allow(clippy::too_many_arguments)]
fn sniffer_sounds(
    mut commands: Commands,
    time: Res<Time>,
    sniff: Res<Sniffing>,
    items: Res<Items>,
    planet: Res<PlanetRes>,
    area: Option<Res<DevArea>>,
    players: Query<&Player>,
    settings: Res<crate::settings::Settings>,
    sounds: Res<SniffSounds>,
    mut buzz: Query<&mut AudioSink, (With<BuzzLoop>, Without<HissLoop>)>,
    mut hiss: Query<&mut AudioSink, (With<HissLoop>, Without<BuzzLoop>)>,
    mut acc: Local<f64>,
    mut levels: Local<(f32, f32)>,
) {
    use bevy::audio::AudioSinkPlayback;
    let dt = time.delta_secs_f64();
    let buzzing = sniff.on && sniff.signal >= BUZZ_FROM;
    if sniff.on && sniff.signal > 0.0 && !buzzing {
        *acc += dt * (TICK_RATE.0 + (TICK_RATE.1 - TICK_RATE.0) * sniff.signal);
        if *acc >= 1.0 {
            *acc = 0.0;
            if let Some(click) = &sounds.click {
                crate::audio_slice::one_shot(&mut commands, click, TICK_VOL);
            }
        }
    } else {
        *acc = 0.0;
    }
    // The hiss follows the nearest open vent.
    let mut hiss_target = 0.0f32;
    if let (Some(frame), Ok(pl), Some(news)) = (area.as_deref().and_then(|a| a.frame), players.single(), items.dig.pockets.as_ref()) {
        for p in news.pockets.iter().filter(|p| p.open_ds > 0) {
            let (g, _) = frame.ground(&planet, p.at[0], p.at[1], 0.0);
            hiss_target = hiss_target.max(HISS_VOL * crate::audio_slice::falloff((g - pl.w.pos).length(), HISS_RANGE_M));
        }
    }
    let k = 1.0 - (-time.delta_secs() / 0.1).exp();
    levels.0 += ((if buzzing { BUZZ_VOL } else { 0.0 }) - levels.0) * k;
    levels.1 += (hiss_target - levels.1) * k;
    for mut sink in &mut buzz {
        sink.set_volume(bevy::audio::Volume::Linear(crate::audio::loop_volume(levels.0, &settings)));
    }
    for mut sink in &mut hiss {
        sink.set_volume(bevy::audio::Volume::Linear(crate::audio::loop_volume(levels.1, &settings)));
    }
}
