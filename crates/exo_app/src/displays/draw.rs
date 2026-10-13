//! Drawing the panels (window only): the overlay on the screen and the displays' quads in the
//! world.
//! TODO(initiator): the look (colours, fonts, frame) is a placeholder.
use super::*;
use crate::origin::WorldPos;
use crate::ui_theme as theme;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::window::PrimaryWindow;
use display_core::{level, Level, Tone, Widget};

const BG: Color = Color::srgb(0.03, 0.05, 0.08);
const EDGE: Color = Color::srgb(0.22, 0.42, 0.55);
const BUTTON: Color = Color::srgb(0.11, 0.2, 0.28);
const BUTTON_HOVER: Color = Color::srgb(0.2, 0.36, 0.48);
const BUTTON_DOWN: Color = Color::srgb(0.34, 0.55, 0.68);
const BUTTON_OFF: Color = Color::srgb(0.07, 0.08, 0.09);
const TRACK: Color = Color::srgb(0.07, 0.09, 0.11);
const TOGGLE_ON: Color = Color::srgb(0.1, 0.36, 0.17);
const TOGGLE_OFF: Color = Color::srgb(0.2, 0.09, 0.08);
/// The display's screen in the world.
const SCREEN_IDLE: Color = Color::srgb(0.05, 0.12, 0.16);
const BEZEL: Color = Color::srgb(0.12, 0.12, 0.13);

/// The window's size.
pub fn update_view(mut view: ResMut<DisplayView>, windows: Query<&Window, With<PrimaryWindow>>) {
    if let Ok(w) = windows.single()
        && w.size() != view.window
        && w.size().min_element() > 0.0
    {
        view.window = w.size();
    }
}

/// Materials the quads share.
#[derive(Resource)]
pub struct ScreenLook {
    idle: Handle<StandardMaterial>,
    bezel: Handle<StandardMaterial>,
}

/// Gives every new display its quad (a screen with a dark frame behind it).
pub fn add_quads(
    mut commands: Commands,
    new: Query<(Entity, &DisplayPanel), Added<DisplayPanel>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    look: Option<Res<ScreenLook>>,
) {
    if new.is_empty() {
        return;
    }
    let look = match look {
        Some(l) => ScreenLook { idle: l.idle.clone(), bezel: l.bezel.clone() },
        None => {
            let l = ScreenLook {
                idle: materials.add(StandardMaterial { base_color: SCREEN_IDLE, emissive: LinearRgba::from(SCREEN_IDLE) * 2.0, ..default() }),
                bezel: materials.add(StandardMaterial { base_color: BEZEL, perceptual_roughness: 0.8, ..default() }),
            };
            commands.insert_resource(ScreenLook { idle: l.idle.clone(), bezel: l.bezel.clone() });
            l
        }
    };
    for (e, d) in &new {
        let (w, h) = (d.size_m.x as f32, d.size_m.y as f32);
        commands.entity(e).insert((Mesh3d(meshes.add(Rectangle::new(w, h))), MeshMaterial3d(look.idle.clone()), Transform::from_rotation(d.rot.as_quat()), WorldPos(d.centre))).with_children(|c| {
            c.spawn((Mesh3d(meshes.add(Cuboid::new(w + 0.08, h + 0.08, 0.04))), MeshMaterial3d(look.bezel.clone()), Transform::from_xyz(0.0, 0.0, -0.025)));
        });
    }
}

#[derive(Component)]
pub struct OverlayRoot;

pub fn spawn_overlay(mut commands: Commands) {
    commands.spawn((OverlayRoot, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100), height: percent(100), display: Display::None, ..default() }, GlobalZIndex(40)));
}

/// What a drawing shows; a new one redraws.
type DrawKey = (Panel, Option<String>, Option<String>, Vec2, f32);

/// The panel in use, centred on the screen.
#[allow(clippy::too_many_arguments)]
pub fn draw_overlay(
    mut commands: Commands,
    mut root: Query<(Entity, &mut Node), With<OverlayRoot>>,
    active: Res<ActiveDisplay>,
    pointer: Res<DisplayPointer>,
    panels: Res<Panels>,
    view: Res<DisplayView>,
    displays: Query<&DisplayPanel>,
    mut drawn: Local<Option<DrawKey>>,
) {
    let Ok((root, mut node)) = root.single_mut() else { return };
    let panel = active.using().and_then(|e| displays.get(e).ok()).and_then(|d| panels.get(&d.id));
    let Some(panel) = panel else {
        if node.display != Display::None {
            node.display = Display::None;
            commands.entity(root).despawn_children();
            *drawn = None;
        }
        return;
    };
    if node.display != Display::Flex {
        node.display = Display::Flex;
    }
    let fit = view.fit(panel);
    let key = (panel.clone(), pointer.0.hover.clone(), pointer.0.pressed.clone(), Vec2::new(fit.rect.min.x as f32, fit.rect.min.y as f32), fit.scale as f32);
    if drawn.as_ref() == Some(&key) {
        return;
    }
    commands.entity(root).despawn_children();
    let (origin, scale) = (key.3, key.4);
    commands.entity(root).with_children(|c| spawn_panel(c, panel, Some(&pointer.0), origin, scale));
    *drawn = Some(key);
}

fn abs_node(origin: Vec2, scale: f32, r: &display_core::Rect) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(origin.x + r.min.x as f32 * scale),
        top: px(origin.y + r.min.y as f32 * scale),
        width: px(r.size.x as f32 * scale),
        height: px(r.size.y as f32 * scale),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        ..default()
    }
}

fn text(s: impl Into<String>, size: f32, colour: Color) -> impl Bundle {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(colour))
}

fn tone(t: Tone) -> Color {
    match t {
        Tone::Normal => theme::INK,
        Tone::Dim => theme::DIM,
        Tone::Title => theme::INFO,
        Tone::Good => theme::GOOD,
        Tone::Warn => theme::GOLD,
        Tone::Bad => theme::BAD,
    }
}

/// The panel's background and every widget as absolutely placed UI nodes under `c`: panel pixel
/// `p` lands at `origin + p * scale`.
pub fn spawn_panel(c: &mut ChildSpawnerCommands, panel: &Panel, ptr: Option<&Pointer>, origin: Vec2, scale: f32) {
    let whole = display_core::Rect { min: DVec2::ZERO, size: panel.size };
    let mut frame = abs_node(origin, scale, &whole);
    frame.border = UiRect::all(px(2.0 * scale));
    c.spawn((frame, BackgroundColor(BG), BorderColor::all(EDGE)));
    let hover = |a: &str| ptr.is_some_and(|p| p.hover.as_deref() == Some(a));
    let pressed = |a: &str| ptr.is_some_and(|p| p.pressed.as_deref() == Some(a));
    for w in panel.layout() {
        let r = &w.rect;
        let font = |px: f64| (px.min(r.size.y * 0.6).max(8.0)) as f32 * scale;
        match w.widget {
            Widget::Label { text: s, tone: t, size } => {
                let mut n = abs_node(origin, scale, r);
                n.justify_content = JustifyContent::Start;
                c.spawn(n).with_children(|c| {
                    c.spawn(text(s.clone(), *size as f32 * scale, tone(*t)));
                });
            }
            Widget::Button { label, enabled, action } => {
                let bg = if !enabled {
                    BUTTON_OFF
                } else if pressed(action) {
                    BUTTON_DOWN
                } else if hover(action) {
                    BUTTON_HOVER
                } else {
                    BUTTON
                };
                let mut n = abs_node(origin, scale, r);
                n.border = UiRect::all(px(1.5 * scale));
                c.spawn((n, BackgroundColor(bg), BorderColor::all(if *enabled { EDGE } else { BUTTON_OFF }))).with_children(|c| {
                    c.spawn(text(label.clone(), font(26.0), if *enabled { theme::INK } else { theme::DIM }));
                });
            }
            Widget::Bar { value, label, warn, crit } => {
                let fill = match level(*value, *warn, *crit) {
                    Level::Normal => theme::INFO,
                    Level::Warn => theme::GOLD,
                    Level::Crit => theme::BAD,
                };
                let mut n = abs_node(origin, scale, r);
                n.justify_content = JustifyContent::Start;
                c.spawn((n, BackgroundColor(TRACK))).with_children(|c| {
                    c.spawn((
                        Node { position_type: PositionType::Absolute, left: px(0), top: px(0), height: percent(100), width: percent(value.clamp(0.0, 1.0) as f32 * 100.0), ..default() },
                        BackgroundColor(theme::faded(fill, 0.75)),
                    ));
                    c.spawn(Node { margin: UiRect::left(px(8.0 * scale)), ..default() }).with_children(|c| {
                        c.spawn(text(format!("{label}  {:.0} %", value * 100.0), font(20.0), theme::INK));
                    });
                });
            }
            Widget::Toggle { on, label, action } => {
                let base = if *on { TOGGLE_ON } else { TOGGLE_OFF };
                let bg = if pressed(action) {
                    BUTTON_DOWN
                } else if hover(action) {
                    theme::faded(base, 0.7)
                } else {
                    base
                };
                let mut n = abs_node(origin, scale, r);
                n.border = UiRect::all(px(1.5 * scale));
                c.spawn((n, BackgroundColor(bg), BorderColor::all(EDGE))).with_children(|c| {
                    c.spawn(text(format!("{label}   {}", if *on { "ON" } else { "OFF" }), font(22.0), if *on { theme::GOOD } else { theme::INK }));
                });
            }
            Widget::Slider { value, label, action } => {
                let v = value.clamp(0.0, 1.0) as f32;
                let mut n = abs_node(origin, scale, r);
                n.justify_content = JustifyContent::Start;
                let knob = if pressed(action) || hover(action) { theme::GOLD } else { theme::INK };
                c.spawn((n, BackgroundColor(TRACK))).with_children(|c| {
                    c.spawn((
                        Node { position_type: PositionType::Absolute, left: px(0), top: px(0), height: percent(100), width: percent(v * 100.0), ..default() },
                        BackgroundColor(theme::faded(theme::INFO, 0.45)),
                    ));
                    c.spawn((
                        Node { position_type: PositionType::Absolute, left: percent(v * 100.0), margin: UiRect::left(px(-3.0 * scale)), top: px(0), height: percent(100), width: px(6.0 * scale), ..default() },
                        BackgroundColor(knob),
                    ));
                    c.spawn(Node { margin: UiRect::left(px(8.0 * scale)), ..default() }).with_children(|c| {
                        c.spawn(text(format!("{label}  {:.2}", value), font(20.0), theme::INK));
                    });
                });
            }
            Widget::Row { .. } | Widget::Column { .. } | Widget::Space => {}
        }
    }
}
