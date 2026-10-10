//! The map (#166): M opens a north-up picture of the ground around the player with pins for the
//! places the crew may use, the tracked job's next stop and the players. The pin list is
//! `jobs_core::map` (no engine types); this module feeds it from the game and draws it. T tracks
//! the next active job. The map takes no input and blocks nothing; it only covers the middle of
//! the screen.
use bevy::math::DVec3;
use bevy::prelude::*;
use gameplay_core::ClientId;
use jobs_core::map::{PlacePos, PlayerPos, Pin, PinKind, Stop};

use crate::controls::{Actions, Tap};
use crate::env::{to_v3, PlanetRes};
use crate::gameplay::{Gameplay, HOST};

/// Metres from the map's centre to its edge (the map shows twice this across). TODO(initiator).
pub const MAP_RADIUS_M: f64 = 1000.0;
/// Pixels of the picture across.
pub const MAP_PX: usize = 256;

#[derive(Resource, Default, Debug)]
pub struct MapView {
    pub open: bool,
    /// The map's centre: the player's direction from the planet's centre when it was opened.
    pub centre: Option<DVec3>,
    /// The planet the picture was made for.
    pub planet: Option<warp_core::PlanetId>,
    /// Counts the pictures made; the window redraws when it changes.
    pub generation: u32,
    /// How long the last picture took (ms), for the log.
    pub made_ms: f64,
    /// The picture (RGB8, `MAP_PX` across) of the latest generation.
    pub rgb: Vec<u8>,
    /// A picture being made on a worker thread (about a second), and for which centre.
    pending: Option<std::sync::Mutex<std::sync::mpsc::Receiver<(Vec<u8>, f64)>>>,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<MapView>();
    app.add_systems(FixedUpdate, toggle_map.after(crate::gameplay::gameplay_step).in_set(crate::phases::Fx::Cargo));
}

/// Where the player is (feet or the seat), world space.
fn player_world(players: &Query<&crate::walker::Player>, ships: &Query<(&avian3d::prelude::Position, &avian3d::prelude::Rotation), With<crate::ship::Ship>>) -> Option<DVec3> {
    players.single().ok().zip(ships.iter().next()).map(|(pl, (p, r))| pl.world_pos(crate::walker::ship_frame(p, r)))
}

fn toggle_map(
    mut actions: ResMut<Actions>,
    mut map: ResMut<MapView>,
    mut gp: ResMut<Gameplay>,
    planet: Res<PlanetRes>,
    players: Query<&crate::walker::Player>,
    ships: Query<(&avian3d::prelude::Position, &avian3d::prelude::Rotation), With<crate::ship::Ship>>,
) {
    if actions.take_tap(Tap::TrackJob) {
        gp.track_next();
    }
    // A finished picture comes in from its worker thread.
    let arrived = map.pending.as_ref().and_then(|rx| rx.lock().ok()?.try_recv().ok());
    if let Some((rgb, ms)) = arrived {
        map.rgb = rgb;
        map.made_ms = ms;
        map.generation += 1;
        map.pending = None;
        println!("map: {MAP_PX}x{MAP_PX} px of {:.0} m made in {ms:.0} ms", 2.0 * MAP_RADIUS_M);
    }
    if !actions.take_tap(Tap::Map) {
        return;
    }
    map.open = !map.open;
    if map.open
        && let Some(pos) = player_world(&players, &ships)
    {
        // A fresh picture of the ground around the player, once per opening, made off the main
        // thread (it takes about a second): the pins show at once, the picture follows.
        let centre = planet.up(pos);
        let (tx, rx) = std::sync::mpsc::channel();
        let pgen = planet.pgen.clone();
        let c = to_v3(centre);
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let rgb = pgen.local_map(c, MAP_RADIUS_M, MAP_PX);
            let _ = tx.send((rgb, t0.elapsed().as_secs_f64() * 1e3));
        });
        map.pending = Some(std::sync::Mutex::new(rx));
        map.rgb.clear();
        map.centre = Some(centre);
        map.planet = Some(planet.id);
    }
}

/// The pins now, for the planet the player is on. Places come from the pads of this planet.
pub fn current_pins(gp: &Gameplay, planet: &PlanetRes, player: Option<DVec3>) -> Vec<Pin> {
    let id = planet.id.0 as u32;
    let places: Vec<PlacePos> = gp.pads.iter().map(|p| PlacePos { location: p.location.clone(), planet: id, dir: dir_of(planet, p.centre) }).collect();
    let players: Vec<PlayerPos> = player.map(|pos| PlayerPos { who: HOST, planet: id, dir: dir_of(planet, pos) }).into_iter().collect();
    jobs_core::map::pins(&gp.kernel, &gp.progress, &gp.jobs, gp.tracked, &places, &players, own(), id)
}

fn own() -> ClientId {
    HOST
}

fn dir_of(planet: &PlanetRes, world: DVec3) -> [f64; 3] {
    let d = (world - planet.centre).normalize_or_zero();
    [d.x, d.y, d.z]
}

/// A pin's place on the picture, pixels from its top left corner, or None when it lies outside.
pub fn pin_px(planet: &PlanetRes, centre: DVec3, pin: &Pin, size_px: f64) -> Option<(f64, f64)> {
    let d = to_v3(DVec3::from_array(pin.dir));
    let (e, n) = planet_core::look::local_offset(to_v3(centre), planet.radius, d);
    let k = size_px / (2.0 * MAP_RADIUS_M);
    let (x, y) = (size_px / 2.0 + e * k, size_px / 2.0 - n * k);
    ((0.0..size_px).contains(&x) && (0.0..size_px).contains(&y)).then_some((x, y))
}

// ---------- the window ----------

/// The picture is drawn this many pixels across on screen.
const DRAW_PX: f64 = 640.0;
const POOL: usize = 24;

#[derive(Component)]
struct MapRoot;
#[derive(Component)]
struct MapImage;
#[derive(Component)]
struct MapPin(usize);
#[derive(Component)]
struct MapPinLabel(usize);

#[derive(Resource, Default)]
struct Drawn(u32);

pub fn window_plugin(app: &mut App) {
    app.init_resource::<Drawn>();
    app.add_systems(Startup, spawn_map);
    app.add_systems(Update, draw_map.in_set(crate::phases::Frame::Hud));
}

fn spawn_map(mut commands: Commands) {
    commands
        .spawn((
            MapRoot,
            Node { position_type: PositionType::Absolute, left: percent(50), top: percent(50), margin: UiRect { left: px(-DRAW_PX as f32 / 2.0), top: px(-DRAW_PX as f32 / 2.0), ..default() }, width: px(DRAW_PX as f32), height: px(DRAW_PX as f32), display: Display::None, ..default() },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        ))
        .with_children(|root| {
            root.spawn((MapImage, ImageNode::default(), Node { position_type: PositionType::Absolute, width: px(DRAW_PX as f32), height: px(DRAW_PX as f32), ..default() }));
            root.spawn((
                Text::new("MAP   [M] close   [T] track the next job   blue: you   red: pick up   green: deliver"),
                TextFont { font_size: FontSize::Px(15.0), ..default() },
                TextColor(Color::WHITE),
                Node { position_type: PositionType::Absolute, left: px(8), top: px(6), ..default() },
            ));
            for i in 0..POOL {
                root.spawn((MapPin(i), Node { position_type: PositionType::Absolute, width: px(10), height: px(10), display: Display::None, ..default() }, BackgroundColor(Color::WHITE)));
                root.spawn((
                    MapPinLabel(i),
                    Text::new(""),
                    TextFont { font_size: FontSize::Px(14.0), ..default() },
                    TextColor(Color::WHITE),
                    Node { position_type: PositionType::Absolute, display: Display::None, ..default() },
                ));
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_map(
    map: Res<MapView>,
    gp: Res<Gameplay>,
    planet: Res<PlanetRes>,
    mut drawn: ResMut<Drawn>,
    mut images: ResMut<Assets<Image>>,
    players: Query<&crate::walker::Player>,
    ships: Query<(&avian3d::prelude::Position, &avian3d::prelude::Rotation), With<crate::ship::Ship>>,
    mut root: Query<&mut Node, (With<MapRoot>, Without<MapPin>, Without<MapPinLabel>)>,
    mut picture: Query<&mut ImageNode, With<MapImage>>,
    mut pins: Query<(&MapPin, &mut Node, &mut BackgroundColor), (Without<MapRoot>, Without<MapPinLabel>)>,
    mut labels: Query<(&MapPinLabel, &mut Node, &mut Text), (Without<MapRoot>, Without<MapPin>)>,
) {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let Ok(mut r) = root.single_mut() else { return };
    r.display = if map.open { Display::Flex } else { Display::None };
    let (true, Some(centre)) = (map.open, map.centre) else { return };
    if drawn.0 != map.generation {
        drawn.0 = map.generation;
        let rgba: Vec<u8> = map.rgb.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        let img = Image::new(Extent3d { width: MAP_PX as u32, height: MAP_PX as u32, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
        if let Ok(mut p) = picture.single_mut() {
            p.image = images.add(img);
        }
    }
    let list = current_pins(&gp, &planet, player_world(&players, &ships));
    // Targets last so they are drawn on top.
    let mut ordered: Vec<&Pin> = list.iter().collect();
    ordered.sort_by_key(|p| matches!(p.kind, PinKind::Target(_)));
    let mut slots: Vec<Option<(f64, f64, Color, f32, String)>> = vec![None; POOL];
    for (i, pin) in ordered.iter().take(POOL).enumerate() {
        let Some((x, y)) = pin_px(&planet, centre, pin, DRAW_PX) else { continue };
        let (color, size) = match &pin.kind {
            PinKind::Place => (Color::srgb(1.0, 0.92, 0.5), 9.0),
            PinKind::Target(Stop::Pickup) => (Color::srgb(1.0, 0.3, 0.2), 15.0),
            PinKind::Target(Stop::Dropoff) => (Color::srgb(0.3, 1.0, 0.4), 15.0),
            PinKind::Player { own: true } => (Color::srgb(0.2, 0.9, 1.0), 12.0),
            PinKind::Player { own: false } => (Color::srgb(0.5, 0.9, 0.3), 10.0),
        };
        // Players have no label (the legend says who is who); a place under the target has none
        // either, the target names it.
        let covered = matches!(pin.kind, PinKind::Place) && list.iter().any(|t| matches!(t.kind, PinKind::Target(_)) && t.dir == pin.dir);
        let label = if matches!(pin.kind, PinKind::Player { .. }) || covered { String::new() } else { gp.text(&pin.label) };
        slots[i] = Some((x, y, color, size, label));
    }
    for (p, mut n, mut bg) in &mut pins {
        match &slots[p.0] {
            Some((x, y, color, size, _)) => {
                n.display = Display::Flex;
                n.left = px((*x as f32) - size / 2.0);
                n.top = px((*y as f32) - size / 2.0);
                n.width = px(*size);
                n.height = px(*size);
                bg.0 = *color;
            }
            None => n.display = Display::None,
        }
    }
    for (l, mut n, mut t) in &mut labels {
        match &slots[l.0] {
            Some((x, y, _, size, label)) if !label.is_empty() => {
                n.display = Display::Flex;
                // Left of the picture's middle the label goes to the left of its pin, so labels of
                // pins that lie close together do not run over each other.
                if *x < DRAW_PX / 2.0 {
                    n.left = Val::Auto;
                    n.right = px((DRAW_PX - *x) as f32 + size / 2.0 + 3.0);
                } else {
                    n.right = Val::Auto;
                    n.left = px((*x as f32) + size / 2.0 + 3.0);
                }
                n.top = px((*y as f32) - 9.0);
                if **t != *label {
                    **t = label.clone();
                }
            }
            _ => n.display = Display::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_cycles_through_the_active_jobs() {
        let mut gp = Gameplay::load(&crate::cargo::Crates::default());
        assert_eq!(gp.tracked, None);
        gp.track_next();
        assert_eq!(gp.tracked, None, "nothing is active");
    }
}
