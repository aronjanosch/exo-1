//! The dig run (#324): three ore veins in the dig site, cut with the welder. The welder's `use` is
//! `cut`; while LMB is held with it selected the client sends its aim (as the extinguisher's spray),
//! the host casts the aim at the veins, steps each vein (`dig_core`) and drops a loose ore item at
//! the cut for a chunk that comes off. Inside the site's bounds a body breathes nothing (`health`).
//! Values: `content/gameplay/dig/dig.json`. The veins, the glow, the sparks, the haze and the sounds
//! are window only and read the news (`Items::dig`), so a client sees and hears what the host has.
use crate::controls::Controls;
use crate::dev_area::{area_of, DevArea, PlaceholderBox, DIG_SITE};
use crate::displays::ActiveDisplay;
use crate::env::PlanetRes;
use crate::interact::{interaction, Interaction};
use crate::items::{Items, Role};
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::DVec3;
use bevy::prelude::*;
use dig_core::{ray_box, DigConfig, Vein};
use display_core::hud::{PromptKind, PromptView};
use display_core::Tone;
use item_core::wire::{CutAsk, DigNews, VeinNew};
use item_core::{FrameRef, Placement, PlayerId};
use std::collections::BTreeMap;
use walker_core::Frame as WalkFrame;

/// The welder's `use` in `content/items/welder.json`.
pub const CUT: &str = "cut";
/// Seconds between two snapshots of changed veins (4 Hz), and between two cut aims (10 Hz).
pub const SEND_EVERY_S: f64 = 0.25;
pub const AIM_EVERY_S: f64 = 0.1;
/// A vein's box (m): width east, height, depth north. It is the collision and the cut's target, the
/// look is drawn inside it. TODO(initiator): the size.
pub const VEIN_SIZE: [f64; 3] = [1.6, 1.2, 1.4];
/// The box reaches this far into the ground (`dev_area::SINK`), so its middle is a little lower.
const SINK: f64 = 0.2;
/// How far the chunk is tossed toward the cutter (m, about), and its start speed up. TODO(initiator): the feel.
const TOSS_TOWARD_MPS: f64 = 3.0;
const TOSS_UP_MPS: f64 = 2.0;
const MAX_CUTS: usize = 4;

#[derive(Resource, Clone)]
pub struct DigCfg(pub DigConfig);

/// Host: the veins and who cuts where.
#[derive(Resource)]
pub struct DigState {
    pub veins: Vec<Vein>,
    /// Each cutter's aim: the eye and the look direction (world metres).
    pub cutters: BTreeMap<PlayerId, ([f64; 3], [f64; 3])>,
    sent: Option<DigNews>,
    since: f64,
}

#[derive(Resource, Default)]
struct CutClient {
    on: bool,
    last: Option<CutAsk>,
    since: f64,
}

pub fn box_id(i: usize) -> String {
    format!("dig_vein_{i}")
}

/// Whether a box id is a vein's collision box (`dress_boxes` leaves it bare: the look is `dig`'s).
pub fn is_vein_box(id: &str) -> bool {
    id.starts_with("dig_vein_")
}

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let cfg = DigConfig::load(&files).unwrap_or_else(|e| panic!("dig: {e:?}"));
    {
        let mut devices = app.world_mut().resource_mut::<crate::dev_area::DevAreaDevices>();
        for (i, v) in cfg.veins.iter().enumerate() {
            devices.add(PlaceholderBox { id: box_id(i), zone: DIG_SITE, local_pos: v.at, size: VEIN_SIZE, turn_deg: 0.0, colour: [0.35, 0.33, 0.3] });
        }
    }
    let veins = cfg.veins.iter().map(|v| Vein::new(&v.ore, &cfg)).collect();
    app.insert_resource(DigState { veins, cutters: BTreeMap::new(), sent: None, since: 0.0 })
        .insert_resource(DigCfg(cfg))
        .init_resource::<CutClient>();
    app.add_systems(FixedUpdate, cut_input.after(crate::scenario::run_script).in_set(crate::phases::Fx::Input));
    app.add_systems(FixedUpdate, dig_step.after(cut_input).after(crate::items_net::items_net_in).in_set(crate::phases::Fx::Input));
    app.add_systems(FixedUpdate, dig_hint.after(interaction).in_set(crate::phases::Fx::Walker));
}

/// Whether the item in a stack's def is the welder's cut.
fn is_cutter(items: &Items, stack: &item_core::container::Stack) -> bool {
    items.defs.of(stack).and_then(|d| d.use_effect.as_deref()) == Some(CUT)
}

fn cut_aim(controls: &Controls, active: &ActiveDisplay, players: &Query<&Player>, items: &Items) -> Option<CutAsk> {
    if controls.free_cursor || !controls.primary_held || active.using().is_some() {
        return None;
    }
    let pl = players.single().ok()?;
    if pl.ship.is_some() || pl.seated || pl.fly {
        return None;
    }
    let v = &items.client.view;
    if !is_cutter(items, v.slot(v.selected)?) {
        return None;
    }
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let dir = pl.world_look(WalkFrame::IDENTITY).normalize_or_zero();
    Some(CutAsk::Aim { eye: eye.to_array(), dir: dir.to_array() })
}

fn cut_input(time: Res<Time>, controls: Res<Controls>, active: Res<ActiveDisplay>, players: Query<&Player>, mut items: ResMut<Items>, mut client: ResMut<CutClient>) {
    let now = time.elapsed_secs_f64();
    match cut_aim(&controls, &active, &players, &items) {
        Some(aim) => {
            if !client.on || (client.last != Some(aim) && now - client.since >= AIM_EVERY_S) {
                items.ask_cut(aim);
                client.on = true;
                client.last = Some(aim);
                client.since = now;
            }
        }
        None => {
            if client.on {
                items.ask_cut(CutAsk::Stop);
            }
            *client = CutClient::default();
        }
    }
}

/// A point of the world in the area's own frame: east, north, height over the ground there. The
/// area is a few dozen metres wide, so a straight ray stays straight in it.
fn local_of(frame: &crate::town::Frame, planet: &PlanetRes, p: DVec3) -> [f64; 3] {
    let [x, y] = area_of(frame, planet, p);
    let (g, up) = frame.ground(planet, x, y, 0.0);
    [x, y, (p - g).dot(up)]
}

/// The vein's box in the area frame: the centre and the half sizes.
pub fn vein_box(cfg: &DigConfig, i: usize) -> ([f64; 3], [f64; 3]) {
    let at = DIG_SITE.point(cfg.veins[i].at);
    ([at[0], at[1], (VEIN_SIZE[1] - SINK) * 0.5], [VEIN_SIZE[0] * 0.5, VEIN_SIZE[2] * 0.5, (VEIN_SIZE[1] + SINK) * 0.5])
}

/// Where the aim meets a vein within the welder's reach: the nearest vein and the world point.
pub fn aim_at_vein(frame: &crate::town::Frame, planet: &PlanetRes, cfg: &DigConfig, eye: DVec3, dir: DVec3) -> Option<(usize, DVec3)> {
    let dir = dir.normalize_or_zero();
    if dir == DVec3::ZERO {
        return None;
    }
    let (a, b) = (local_of(frame, planet, eye), local_of(frame, planet, eye + dir * 10.0));
    let d = DVec3::new(b[0] - a[0], b[1] - a[1], b[2] - a[2]).normalize_or_zero();
    (0..cfg.veins.len())
        .filter_map(|i| {
            let (c, h) = vein_box(cfg, i);
            ray_box(a, d.to_array(), c, h).filter(|t| *t <= cfg.reach_m).map(|t| (i, t))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(i, t)| (i, eye + dir * t))
}

#[allow(clippy::too_many_arguments)]
fn dig_step(time: Res<Time>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, cfg: Res<DigCfg>, mut items: ResMut<Items>, mut st: ResMut<DigState>) {
    if items.role == Role::Client {
        return;
    }
    for (who, a) in std::mem::take(&mut items.dig.asks) {
        match a {
            CutAsk::Aim { eye, dir } => {
                st.cutters.insert(who, (eye, dir));
            }
            CutAsk::Stop => {
                st.cutters.remove(&who);
            }
        }
    }
    // A cutter that no longer has the welder in the hand stops cutting.
    let held: Vec<PlayerId> = st
        .cutters
        .keys()
        .copied()
        .filter(|who| items.host.inventories.get(who).and_then(|inv| inv.slot(inv.selected)).is_some_and(|s| is_cutter(&items, s)))
        .collect();
    st.cutters.retain(|who, _| held.contains(who));
    let dt = time.delta_secs_f64();
    let now = time.elapsed_secs_f64();
    let n = st.veins.len();
    let mut hit: Vec<Option<(PlayerId, DVec3, DVec3)>> = vec![None; n];
    if let Some(frame) = area.as_deref().and_then(|a| a.frame) {
        for (who, (eye, dir)) in &st.cutters {
            let (eye, dir) = (DVec3::from_array(*eye), DVec3::from_array(*dir));
            if let Some((i, p)) = aim_at_vein(&frame, &planet, &cfg.0, eye, dir)
                && hit[i].is_none()
            {
                hit[i] = Some((*who, p, eye));
            }
        }
    }
    let mut cuts = Vec::new();
    let mut chunks = Vec::new();
    for (i, h) in hit.iter().enumerate() {
        if let Some((_, p, _)) = h {
            cuts.push(p.to_array());
        }
        if let Some(ore) = st.veins[i].step(&cfg.0, dt, h.is_some()) {
            if let (Some((_, p, eye)), Some(def)) = (h, items.defs.get(&ore)) {
                chunks.push((def.stack_of(1), *p, *eye));
            }
        }
    }
    for (stack, p, eye) in chunks {
        let up = (p - planet.centre).normalize_or_zero();
        let toward = (eye - p - up * (eye - p).dot(up)).normalize_or_zero();
        // Off the rock's face toward the cutter, a hand's width up, tossed so it lands at their feet.
        let at = Placement { frame: FrameRef::Planet(planet.id.0 as u32), pos: (p + toward * 0.3 + up * 0.3).to_array(), vel: (toward * TOSS_TOWARD_MPS + up * TOSS_UP_MPS).to_array(), forward: toward.to_array() };
        items.spawn(now, stack, at);
    }
    let news = DigNews { veins: st.veins.iter().map(|v| VeinNew { chunks: v.chunks_left.min(255) as u8, heat: (v.heat * 1000.0).round() as u16 }).collect(), cuts };
    items.dig.news = Some(news.clone());
    st.since += dt;
    if items.role == Role::Host && st.sent.as_ref() != Some(&news) && st.since >= SEND_EVERY_S {
        items.net_out.push(crate::items::NetOut::Dig { to: None, d: news.clone() });
        st.sent = Some(news);
        st.since = 0.0;
    }
}

/// The hint when the crosshair is on a vein and nothing else is offered: hold LMB to cut with the
/// welder, or what is missing. Sets the prompt as `interaction` does; F does nothing there.
#[allow(clippy::too_many_arguments)]
fn dig_hint(planet: Res<PlanetRes>, area: Option<Res<DevArea>>, cfg: Res<DigCfg>, controls: Res<Controls>, active: Res<ActiveDisplay>, items: Res<Items>, players: Query<&Player>, mut inter: ResMut<Interaction>) {
    if inter.target.is_some() || !inter.prompt.is_empty() || controls.free_cursor || active.using().is_some() {
        return;
    }
    let (Some(frame), Ok(pl)) = (area.as_deref().and_then(|a| a.frame), players.single()) else { return };
    if pl.ship.is_some() || pl.seated || pl.fly {
        return;
    }
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let Some((i, p)) = aim_at_vein(&frame, &planet, &cfg.0, eye, pl.world_look(WalkFrame::IDENTITY)) else { return };
    let ore = &cfg.0.veins[i].ore;
    let name = format!("{} vein", items.defs.get(ore).map_or(ore.as_str(), |d| d.name.as_str()));
    let v = &items.client.view;
    let welder = v.slot(v.selected).is_some_and(|s| is_cutter(&items, s));
    let empty = items.dig.news.as_ref().and_then(|n| n.veins.get(i)).is_some_and(|v| v.chunks == 0);
    let mut view = PromptView { name: name.clone(), ..PromptView::default() };
    inter.kind = PromptKind::Use;
    if empty {
        view.verb = "Cut".into();
        view.note = "EMPTY · GROWS BACK".into();
        inter.kind = PromptKind::Blocked;
    } else if welder {
        view.verb = "Hold to cut".into();
    } else {
        view.verb = "Cut".into();
        view.note = "▲ Needs a welder".into();
        view.note_tone = Tone::Warn;
        inter.kind = PromptKind::Blocked;
    }
    inter.prompt = format!("[LMB] {}", if welder { format!("hold to cut: {name}") } else { format!("{name}: a welder is needed") });
    inter.view = view;
    inter.at = Some(p);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_veins_stand_inside_the_site_and_apart() {
        let cfg = DigConfig::load(&crate::content_files("gameplay")).expect("dig.json");
        for (i, v) in cfg.veins.iter().enumerate() {
            let at = DIG_SITE.point(v.at);
            let edge = 0.5 * VEIN_SIZE[0].max(VEIN_SIZE[2]);
            assert!(DIG_SITE.contains([at[0] - edge, at[1] - edge]) && DIG_SITE.contains([at[0] + edge, at[1] + edge]), "vein {i} sticks out of the site");
            for w in &cfg.veins[i + 1..] {
                let d = ((v.at[0] - w.at[0]).powi(2) + (v.at[1] - w.at[1]).powi(2)).sqrt();
                assert!(d > VEIN_SIZE[0] + 0.5, "two veins stand too close");
            }
        }
    }
}

// ---------- window only ----------

#[derive(Component)]
struct VeinCluster;

#[derive(Component)]
struct VeinStub;

#[derive(Component)]
struct CutGlow;

#[derive(Component)]
struct Flash {
    age: f32,
}

#[derive(Component)]
struct CutLoop;

struct VeinVis {
    cluster: Entity,
    stub: Entity,
}

#[derive(Resource, Default)]
struct DigVisuals {
    planet: Option<warp_core::PlanetId>,
    veins: Vec<VeinVis>,
    glows: Vec<Entity>,
    flash: Option<(Handle<Mesh>, Handle<StandardMaterial>)>,
    /// Seconds since the last spark burst at each cut slot.
    spark: f64,
    /// The chunks of the last snapshot, to see one come off.
    prev: Vec<u8>,
}

#[derive(Resource, Default)]
struct DigSounds {
    crack: Option<Handle<crate::audio::SynthAudio>>,
    snap: Option<Handle<crate::audio::SynthAudio>>,
}

/// The haze over the site: its height (m) and colour. TODO(initiator): the look.
const HAZE_HEIGHT: f32 = 0.6;
const HAZE_COLOUR: [f32; 4] = [0.45, 0.72, 0.2, 0.3];
const ROCK_COLOUR: [f32; 3] = [0.34, 0.32, 0.30];
/// The flash when a chunk comes off: its life (s) and its light.
const FLASH_LIFE_S: f32 = 0.22;
const FLASH_LIGHT: f32 = 400_000.0;
/// The glow's light at full heat, and its share at zero heat.
const GLOW_LIGHT: f32 = 90_000.0;
const GLOW_BASE: f32 = 0.3;
/// Sparks per second at zero and at full heat.
const SPARKS_PER_S: (f64, f64) = (10.0, 40.0);
const SPARK_LIFE_S: f32 = 0.35;
/// The cut's loop volume at full heat, the share at zero heat, and how far it carries (m). TODO(initiator): by ear.
const CUT_VOL: f32 = 0.08;
const CUT_BASE: f32 = 0.4;
const CUT_RANGE_M: f64 = 14.0;
const CRACK_VOL: f32 = 0.12;

pub fn window_plugin(app: &mut App) {
    app.init_resource::<DigVisuals>().init_resource::<DigSounds>();
    app.add_systems(Startup, audio_setup);
    app.add_systems(Update, (dig_visuals, dig_flash).chain().in_set(crate::phases::Frame::Sync));
    app.add_systems(Update, dig_sounds.in_set(crate::phases::Frame::Input));
}

/// The nearest vein to a world point, by the veins' places.
fn nearest(places: &[DVec3], p: DVec3) -> Option<usize> {
    (0..places.len()).min_by(|&a, &b| places[a].distance(p).total_cmp(&places[b].distance(p)))
}

#[allow(clippy::too_many_arguments)]
fn dig_visuals(
    mut commands: Commands,
    time: Res<Time>,
    planet: Res<PlanetRes>,
    area: Option<Res<DevArea>>,
    cfg: Res<DigCfg>,
    items: Res<Items>,
    fx: Option<Res<crate::effects::FxAssets>>,
    origin: Res<crate::origin::RenderOrigin>,
    mut vis: ResMut<DigVisuals>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut clusters: Query<(&mut Transform, &mut Visibility), (With<VeinCluster>, Without<VeinStub>, Without<CutGlow>, Without<Flash>)>,
    mut stubs: Query<&mut Visibility, (With<VeinStub>, Without<VeinCluster>)>,
    mut glows: Query<(&mut Transform, &mut Visibility, &mut PointLight, &MeshMaterial3d<StandardMaterial>), (With<CutGlow>, Without<VeinCluster>, Without<VeinStub>, Without<Flash>)>,
) {
    let Some(frame) = area.as_deref().and_then(|a| a.frame) else { return };
    let to_render = |w: DVec3| (w - origin.origin).as_vec3();
    let place = |at: [f64; 2], lift: f64| frame.ground(&planet, DIG_SITE.point(at)[0], DIG_SITE.point(at)[1], lift);
    if vis.planet != Some(planet.id) {
        vis.planet = Some(planet.id);
        vis.veins.clear();
        vis.glows.clear();
        vis.prev.clear();
        let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let rock = materials.add(StandardMaterial { base_color: Color::srgb(ROCK_COLOUR[0], ROCK_COLOUR[1], ROCK_COLOUR[2]), perceptual_roughness: 1.0, ..default() });
        let stub_mat = materials.add(StandardMaterial { base_color: Color::srgb(0.1, 0.1, 0.11), perceptual_roughness: 1.0, ..default() });
        for v in &cfg.0.veins {
            let (g, up) = place(v.at, 0.0);
            let colour = items.defs.get(&v.ore).map_or([0.8, 0.8, 0.8], |d| d.shape.colour);
            let seam = materials.add(StandardMaterial { base_color: Color::srgb(colour[0], colour[1], colour[2]), emissive: LinearRgba::rgb(colour[0] * 0.5, colour[1] * 0.5, colour[2] * 0.5), perceptual_roughness: 0.5, ..default() });
            let part = |commands: &mut Commands, parent: Entity, mat: &Handle<StandardMaterial>, at: Vec3, size: Vec3, turn: f32| {
                let e = commands.spawn((Mesh3d(cube.clone()), MeshMaterial3d(mat.clone()), Transform { translation: at, rotation: Quat::from_rotation_y(turn), scale: size })).id();
                commands.entity(parent).add_child(e);
            };
            let root = commands.spawn((crate::origin::WorldPos(g), Transform::from_rotation(frame.rotation(up, 0.0).as_quat()), Visibility::default())).id();
            let cluster = commands.spawn((VeinCluster, Transform::default(), Visibility::default())).id();
            commands.entity(root).add_child(cluster);
            // Three rock lumps and a band of the ore's colour round the big one.
            part(&mut commands, cluster, &rock, Vec3::new(0.0, 0.6, 0.0), Vec3::new(1.3, 1.2, 1.1), 0.0);
            part(&mut commands, cluster, &rock, Vec3::new(0.55, 0.42, 0.45), Vec3::new(0.8, 0.84, 0.7), 0.5);
            part(&mut commands, cluster, &rock, Vec3::new(-0.5, 0.33, -0.4), Vec3::new(0.7, 0.66, 0.8), -0.4);
            part(&mut commands, cluster, &seam, Vec3::new(0.0, 0.72, 0.0), Vec3::new(1.34, 0.16, 1.14), 0.0);
            let stub = commands.spawn((VeinStub, Transform::default(), Visibility::Hidden)).id();
            commands.entity(root).add_child(stub);
            part(&mut commands, stub, &stub_mat, Vec3::new(0.0, 0.1, 0.0), Vec3::new(1.0, 0.2, 0.9), 0.3);
            vis.veins.push(VeinVis { cluster, stub });
        }
        // The haze: one translucent box over the site, drawn from both sides.
        let (g, up) = frame.ground(&planet, DIG_SITE.centre[0], DIG_SITE.centre[1], HAZE_HEIGHT as f64 * 0.5);
        let haze_mesh = meshes.add(Cuboid::new(DIG_SITE.half[0] as f32 * 2.0, HAZE_HEIGHT, DIG_SITE.half[1] as f32 * 2.0));
        let haze_mat = materials.add(StandardMaterial { base_color: Color::srgba(HAZE_COLOUR[0], HAZE_COLOUR[1], HAZE_COLOUR[2], HAZE_COLOUR[3]), unlit: true, alpha_mode: AlphaMode::Blend, cull_mode: None, double_sided: true, ..default() });
        commands.spawn((Mesh3d(haze_mesh), MeshMaterial3d(haze_mat), Transform::from_rotation(frame.rotation(up, 0.0).as_quat()), crate::origin::WorldPos(g)));
        let ball = meshes.add(Sphere::new(0.5));
        for _ in 0..MAX_CUTS {
            let mat = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.8, 0.5), emissive: LinearRgba::rgb(8.0, 3.0, 0.6), unlit: true, ..default() });
            let e = commands.spawn((CutGlow, Mesh3d(ball.clone()), MeshMaterial3d(mat), Transform::default(), Visibility::Hidden, PointLight { color: Color::srgb(1.0, 0.6, 0.25), intensity: 0.0, range: 8.0, shadow_maps_enabled: false, ..default() })).id();
            vis.glows.push(e);
        }
        let flash_mat = materials.add(StandardMaterial { base_color: Color::srgb(1.0, 0.95, 0.8), emissive: LinearRgba::rgb(20.0, 16.0, 8.0), unlit: true, ..default() });
        vis.flash = Some((ball, flash_mat));
        return;
    }
    let news = items.dig.news.as_ref();
    let places: Vec<DVec3> = cfg.0.veins.iter().map(|v| place(v.at, 0.0).0).collect();
    // The vein's size by chunks left; the stub when empty.
    for (i, vv) in vis.veins.iter().enumerate() {
        let chunks = news.and_then(|n| n.veins.get(i)).map_or(cfg.0.chunks as f64, |v| v.chunks as f64);
        let size = Vein { ore: String::new(), chunks_left: chunks as u32, heat: 0.0, regrow: 0.0 }.size(&cfg.0) as f32;
        if let Ok((mut tr, mut v)) = clusters.get_mut(vv.cluster) {
            tr.scale = Vec3::splat(size.max(0.01));
            *v = if chunks > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        }
        if let Ok(mut v) = stubs.get_mut(vv.stub) {
            *v = if chunks > 0.0 { Visibility::Hidden } else { Visibility::Inherited };
        }
    }
    // A flash where a chunk came off (the cut's last place, else the vein's top).
    let now: Vec<u8> = news.map_or(Vec::new(), |n| n.veins.iter().map(|v| v.chunks).collect());
    let mut flashes = Vec::new();
    for (i, (&a, &b)) in vis.prev.iter().zip(&now).enumerate() {
        if b < a {
            let at = news.and_then(|n| n.cuts.iter().map(|c| DVec3::from_array(*c)).find(|c| nearest(&places, *c) == Some(i))).unwrap_or(places[i] + DVec3::ZERO);
            flashes.push(at);
        }
    }
    vis.prev = now;
    if let Some((mesh, mat)) = vis.flash.clone() {
        for at in flashes {
            commands.spawn((Flash { age: 0.0 }, Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), Transform::from_translation(to_render(at)).with_scale(Vec3::splat(0.5)), PointLight { color: Color::srgb(1.0, 0.85, 0.5), intensity: FLASH_LIGHT, range: 10.0, shadow_maps_enabled: false, ..default() }));
        }
    }
    // The glow at each cut, by the heat of the vein it is on; sparks leave it.
    let t = time.elapsed_secs_f64();
    let cuts: Vec<(DVec3, f64)> = news.map_or(Vec::new(), |n| {
        n.cuts.iter().map(|c| DVec3::from_array(*c)).map(|c| (c, nearest(&places, c).and_then(|i| n.veins.get(i)).map_or(0.0, |v| v.heat as f64 / 1000.0))).collect()
    });
    for (k, e) in vis.glows.iter().enumerate() {
        let Ok((mut tr, mut v, mut light, mat)) = glows.get_mut(*e) else { continue };
        match cuts.get(k) {
            Some((at, heat)) => {
                let flicker = crate::effects::flame_flicker(t.to_bits() ^ (k as u64).wrapping_mul(0x9E37_79B9));
                let level = (GLOW_BASE + (1.0 - GLOW_BASE) * *heat as f32) * flicker;
                *v = Visibility::Inherited;
                tr.translation = to_render(*at);
                tr.scale = Vec3::splat(0.05 + 0.14 * *heat as f32);
                light.intensity = GLOW_LIGHT * level;
                if let Some(mut m) = materials.get_mut(&mat.0) {
                    m.emissive = LinearRgba::rgb(8.0 * level, 3.0 * level, 0.6 * level);
                }
            }
            None => {
                *v = Visibility::Hidden;
                light.intensity = 0.0;
            }
        }
    }
    if let Some(fx) = fx.as_deref() {
        vis.spark += time.delta_secs_f64();
        let heat = cuts.iter().map(|c| c.1).fold(0.0, f64::max);
        let every = 1.0 / (SPARKS_PER_S.0 + (SPARKS_PER_S.1 - SPARKS_PER_S.0) * heat);
        if !cuts.is_empty() && vis.spark >= every {
            vis.spark = 0.0;
            for (k, (at, _)) in cuts.iter().enumerate() {
                let up = (*at - planet.centre).normalize_or_zero();
                let seed = (t * 1000.0) as u64 ^ ((k as u64) << 20);
                crate::effects::spawn_particle(&mut commands, fx, &mut materials, crate::effects::Kind::Spark, *at, crate::effects::jet(up, seed, 1.2, 2.5), 0.0, crate::effects::SPARK_SIZE, SPARK_LIFE_S);
            }
        }
    }
}

fn dig_flash(mut commands: Commands, time: Res<Time>, mut flashes: Query<(Entity, &mut Flash, &mut Transform, &mut PointLight)>) {
    for (e, mut f, mut tr, mut light) in &mut flashes {
        f.age += time.delta_secs();
        let k = 1.0 - f.age / FLASH_LIFE_S;
        if k <= 0.0 {
            commands.entity(e).despawn();
            continue;
        }
        tr.scale = Vec3::splat(0.5 * k + 0.1);
        light.intensity = FLASH_LIGHT * k;
    }
}

fn audio_setup(mut commands: Commands, mut assets: ResMut<Assets<crate::audio::SynthAudio>>, mut sounds: ResMut<DigSounds>) {
    use crate::audio::{Sound, SynthAudio};
    use bevy::audio::{PlaybackMode, Volume};
    let crackle = assets.add(SynthAudio(Sound::Crackle(1)));
    sounds.crack = Some(assets.add(SynthAudio(Sound::Thud)));
    sounds.snap = Some(assets.add(SynthAudio(Sound::Zap)));
    commands.spawn((AudioPlayer(crackle), PlaybackSettings { mode: PlaybackMode::Loop, volume: Volume::Linear(0.0), ..default() }, CutLoop));
}

/// The cutting loop follows the hottest cut within hearing (louder and higher with the heat); a
/// crack when a chunk comes off.
#[allow(clippy::too_many_arguments)]
fn dig_sounds(mut commands: Commands, time: Res<Time>, items: Res<Items>, players: Query<&Player>, settings: Res<crate::settings::Settings>, sounds: Res<DigSounds>, mut loops: Query<&mut AudioSink, With<CutLoop>>, mut level: Local<f32>, mut prev: Local<Vec<u8>>) {
    use bevy::audio::AudioSinkPlayback;
    let news = items.dig.news.as_ref();
    let me = players.single().ok().map(|p| p.w.pos);
    let mut target = 0.0f32;
    let mut heat = 0.0f32;
    if let (Some(n), Some(me)) = (news, me) {
        for c in &n.cuts {
            let near = crate::audio_slice::falloff((DVec3::from_array(*c) - me).length(), CUT_RANGE_M);
            let h = n.veins.iter().map(|v| v.heat).max().unwrap_or(0) as f32 / 1000.0;
            let v = CUT_VOL * (CUT_BASE + (1.0 - CUT_BASE) * h) * near;
            if v > target {
                target = v;
                heat = h;
            }
        }
    }
    let k = 1.0 - (-time.delta_secs() / 0.1).exp();
    *level += (target - *level) * k;
    for mut sink in &mut loops {
        sink.set_volume(bevy::audio::Volume::Linear(crate::audio::loop_volume(*level, &settings)));
        sink.set_speed(0.8 + 0.6 * heat);
    }
    let now: Vec<u8> = news.map_or(Vec::new(), |n| n.veins.iter().map(|v| v.chunks).collect());
    if prev.iter().zip(&now).any(|(a, b)| b < a)
        && let (Some(crack), Some(snap)) = (&sounds.crack, &sounds.snap)
    {
        crate::audio_slice::one_shot(&mut commands, crack, CRACK_VOL);
        crate::audio_slice::one_shot(&mut commands, snap, CRACK_VOL * 0.8);
    }
    *prev = now;
}
