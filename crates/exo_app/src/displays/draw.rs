use super::*;
use crate::origin::{RenderOrigin, WorldPos, WorldPose};
use crate::view::MainCamera;
use bevy::asset::RenderAssetUsages;
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use display_core::{level, list_row_at, Glyph, Level, ListRow, Rect, Rgb, Shape, TextFace, Theme, Tone, Widget};

pub fn update_view(mut view: ResMut<DisplayView>, windows: Query<&Window, With<PrimaryWindow>>, controls: Res<Controls>, cam: Query<(&WorldPose, &Projection), With<MainCamera>>) {
    if let Ok(w) = windows.single()
        && w.size() != view.window
        && w.size().min_element() > 0.0
    {
        view.window = w.size();
    }
    let ray = match (controls.pointer, cam.single()) {
        (Some(p), Ok((pose, Projection::Perspective(proj)))) => {
            let win = DVec2::new(view.window.x as f64, view.window.y as f64);
            Some(display_core::pointer_ray(pose.pos, pose.rot, proj.fov as f64, win, DVec2::new(p.x as f64, p.y as f64)))
        }
        _ => None,
    };
    if view.ray != ray {
        view.ray = ray;
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct DisplayFonts {
    paths: [String; 4],
    pub display: Handle<Font>,
    pub semibold: Handle<Font>,
    pub bold: Handle<Font>,
    pub mono: Handle<Font>,
}

pub fn load_fonts(mut commands: Commands, look: Res<DisplayLook>, fonts: Option<Res<DisplayFonts>>, assets: Res<AssetServer>) {
    let f = &look.0.fonts;
    let paths = [f.display.clone(), f.display_semibold.clone(), f.display_bold.clone(), f.mono.clone()];
    if fonts.is_some_and(|x| x.paths == paths) {
        return;
    }
    commands.insert_resource(DisplayFonts { display: assets.load(&paths[0]), semibold: assets.load(&paths[1]), bold: assets.load(&paths[2]), mono: assets.load(&paths[3]), paths });
}

fn col(c: Rgb) -> Color {
    Color::srgb(c.0[0], c.0[1], c.0[2])
}

fn col_a(c: Rgb, a: f32) -> Color {
    Color::srgba(c.0[0], c.0[1], c.0[2], a)
}

/// Whether the stroke at `at` px along it is on: `dash` is (on, off) px, None is always on.
fn on_dash(at: f64, dash: Option<(f64, f64)>) -> bool {
    match dash {
        None => true,
        Some((on, off)) => at.rem_euclid((on + off).max(1e-9)) < on,
    }
}

#[derive(Resource)]
pub struct ScreenLook {
    idle: Handle<StandardMaterial>,
    bezel: Handle<StandardMaterial>,
}

pub fn add_quads(
    mut commands: Commands,
    new: Query<(Entity, &DisplayPanel), Added<DisplayPanel>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    look: Option<Res<ScreenLook>>,
    theme: Res<DisplayLook>,
) {
    if new.is_empty() {
        return;
    }
    let look = match look {
        Some(l) => ScreenLook { idle: l.idle.clone(), bezel: l.bezel.clone() },
        None => {
            let idle = col(theme.0.colours.screen_idle);
            let l = ScreenLook {
                idle: materials.add(StandardMaterial { base_color: idle, emissive: LinearRgba::from(idle) * 2.0, ..default() }),
                bezel: materials.add(StandardMaterial { base_color: col(theme.0.colours.bezel), perceptual_roughness: 0.8, ..default() }),
            };
            commands.insert_resource(ScreenLook { idle: l.idle.clone(), bezel: l.bezel.clone() });
            l
        }
    };
    for (e, d) in &new {
        let (w, h) = (d.size_m.x as f32, d.size_m.y as f32);
        commands.entity(e).insert((Mesh3d(meshes.add(Rectangle::new(w, h))), MeshMaterial3d(look.idle.clone()), Transform::from_rotation(d.rot.as_quat()), WorldPos(d.centre))).with_children(|c| {
            c.spawn((Mesh3d(meshes.add(Cuboid::new(w + 0.06, h + 0.06, 0.04))), MeshMaterial3d(look.bezel.clone()), Transform::from_xyz(0.0, 0.0, -0.025)));
        });
    }
}

/// What a drawing shows; a new one redraws. The pointer position counts only over a list (its
/// row hover), so moving over a button does not redraw; the cursor mark moves on its own.
type DrawKey = (Panel, Option<String>, Option<String>, Option<(i64, i64)>, f32);

fn draw_key(panel: &Panel, ptr: Option<&Pointer>, scale: f32) -> DrawKey {
    let over_list = |p: &&Pointer| p.hover.as_ref().is_some_and(|h| panel.find(h).is_some_and(|w| matches!(w.widget, Widget::List { .. })));
    let list_at = ptr.filter(over_list).and_then(|p| p.at).map(|a| (a.x as i64, (a.y / 4.0) as i64));
    (panel.clone(), ptr.and_then(|p| p.hover.clone()), ptr.and_then(|p| p.pressed.clone()), list_at, scale)
}

#[derive(Component)]
pub struct ScreenTarget {
    camera: Entity,
    root: Entity,
    cursor: Entity,
    material: Handle<StandardMaterial>,
    drawn: Option<DrawKey>,
    /// Where the cursor mark stands (texture pixels), None when hidden.
    cursor_at: Option<(i32, i32)>,
    last: f64,
    /// Frames the camera still renders after a redraw (the new nodes need a frame for layout).
    frames_left: u8,
}

fn spawn_cursor(commands: &mut Commands, camera: Entity, theme: &Theme, scale: f32) -> Entity {
    let k = &theme.colours;
    let (arm, thick, edge) = (10.0 * scale, 2.0 * scale, 1.5 * scale);
    let bar = |w: f32, h: f32| Node { position_type: PositionType::Absolute, left: px(-w * 0.5), top: px(-h * 0.5), width: px(w), height: px(h), ..default() };
    commands
        .spawn((Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: px(0), height: px(0), display: Display::None, ..default() }, UiTargetCamera(camera), GlobalZIndex(10)))
        .with_children(|c| {
            for (colour, grow) in [(col(k.bg_0), edge), (col(k.fg_title), 0.0)] {
                c.spawn((bar(2.0 * arm + 2.0 * grow, thick + 2.0 * grow), BackgroundColor(colour)));
                c.spawn((bar(thick + 2.0 * grow, 2.0 * arm + 2.0 * grow), BackgroundColor(colour)));
            }
        })
        .id()
}

#[allow(clippy::too_many_arguments)]
pub fn draw_world(
    mut commands: Commands,
    mut screens: Query<(Entity, &DisplayPanel, &mut MeshMaterial3d<StandardMaterial>, Option<&mut ScreenTarget>)>,
    mut cameras: Query<&mut Camera>,
    mut cursors: Query<&mut Node>,
    active: Res<ActiveDisplay>,
    pointer: Res<DisplayPointer>,
    panels: Res<Panels>,
    look: Res<DisplayLook>,
    fonts: Option<Res<DisplayFonts>>,
    origin: Res<RenderOrigin>,
    view_cam: Query<&WorldPose, With<MainCamera>>,
    quads: Option<Res<ScreenLook>>,
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (Some(quads), Some(fonts)) = (quads, fonts) else { return };
    let w = look.0.world;
    let forward = view_cam.single().ok().map(|p| p.rot * DVec3::NEG_Z);
    let now = time.elapsed_secs_f64();
    for (e, d, mut mat, target) in &mut screens {
        let panel = panels.get(&d.id);
        let to = d.centre - origin.view;
        let in_front = forward.is_none_or(|f| to.dot(f) > -0.5);
        let in_use = active.using() == Some(e);
        let want = panel.is_some() && (in_use || (to.length() < w.range_m && in_front));
        match (want, target) {
            (true, None) => {
                let panel = panel.unwrap();
                let ts = w.texture_scale as f32;
                let size = Extent3d { width: (panel.size.x as f32 * ts) as u32, height: (panel.size.y as f32 * ts) as u32, ..default() };
                let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
                image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
                let image = images.add(image);
                let camera = commands.spawn((Camera2d, Camera { order: -1, clear_color: ClearColorConfig::Custom(col(look.0.colours.bg_0)), ..default() }, RenderTarget::Image(image.clone().into()))).id();
                let root = commands.spawn((Node { width: percent(100), height: percent(100), ..default() }, UiTargetCamera(camera))).id();
                let cursor = spawn_cursor(&mut commands, camera, &look.0, ts);
                let material = materials.add(StandardMaterial { base_color_texture: Some(image), unlit: true, ..default() });
                mat.0 = material.clone();
                commands.entity(e).insert(ScreenTarget { camera, root, cursor, material, drawn: None, cursor_at: None, last: f64::NEG_INFINITY, frames_left: 0 });
            }
            (true, Some(mut t)) => {
                if mat.0 != t.material {
                    mat.0 = t.material.clone();
                }
                if look.is_changed() {
                    t.drawn = None;
                }
                let panel = panel.unwrap();
                let ptr = in_use.then_some(&pointer.0);
                let ts = w.texture_scale as f32;
                let key = draw_key(panel, ptr, ts);
                if t.drawn.as_ref() != Some(&key) && (in_use || now - t.last >= w.redraw_s) {
                    commands.entity(t.root).despawn_children();
                    let pen = Pen { theme: &look.0, fonts: &fonts, scale: ts };
                    commands.entity(t.root).with_children(|c| pen.panel(c, panel, ptr));
                    t.drawn = Some(key);
                    t.last = now;
                    t.frames_left = 3;
                }
                let at = ptr.and_then(|p| p.at).map(|a| ((a.x as f32 * ts).round() as i32, (a.y as f32 * ts).round() as i32));
                if t.cursor_at != at {
                    t.cursor_at = at;
                    if let Ok(mut n) = cursors.get_mut(t.cursor) {
                        match at {
                            Some((x, y)) => {
                                n.display = Display::Flex;
                                n.left = px(x as f32);
                                n.top = px(y as f32);
                            }
                            None => n.display = Display::None,
                        }
                    }
                    t.frames_left = t.frames_left.max(3);
                }
                let on = t.frames_left > 0;
                t.frames_left = t.frames_left.saturating_sub(1);
                if let Ok(mut cam) = cameras.get_mut(t.camera)
                    && cam.is_active != on
                {
                    cam.is_active = on;
                }
            }
            (false, Some(mut t)) => {
                if let Ok(mut cam) = cameras.get_mut(t.camera)
                    && cam.is_active
                {
                    cam.is_active = false;
                }
                t.drawn = None;
                t.cursor_at = None;
                if let Ok(mut n) = cursors.get_mut(t.cursor) {
                    n.display = Display::None;
                }
                if mat.0 != quads.idle {
                    mat.0 = quads.idle.clone();
                }
            }
            (false, None) => {}
        }
    }
}

// ---------- the pen: panel pixels to UI nodes ----------

struct Pen<'a> {
    theme: &'a Theme,
    fonts: &'a DisplayFonts,
    scale: f32,
}

#[derive(Clone, Copy)]
struct Cuts {
    tl: bool,
    tr: bool,
    br: bool,
    bl: bool,
}

const CUT_TR: Cuts = Cuts { tl: false, tr: true, br: false, bl: false };
const CUT_TL_BR: Cuts = Cuts { tl: true, tr: false, br: true, bl: false };

#[derive(Clone, Copy)]
enum Face {
    Display,
    Semibold,
    Bold,
    Mono,
}

impl Pen<'_> {
    fn c(&self) -> &display_core::theme::Colours {
        &self.theme.colours
    }

    fn node(&self, x: f64, y: f64, w: f64, h: f64) -> Node {
        let s = self.scale;
        Node { position_type: PositionType::Absolute, left: px(x as f32 * s), top: px(y as f32 * s), width: px(w as f32 * s), height: px(h as f32 * s), ..default() }
    }

    fn rect(&self, c: &mut ChildSpawnerCommands, x: f64, y: f64, w: f64, h: f64, colour: Color) {
        if w > 0.0 && h > 0.0 {
            c.spawn((self.node(x, y, w, h), BackgroundColor(colour)));
        }
    }

    fn line(&self, c: &mut ChildSpawnerCommands, a: DVec2, b: DVec2, thick: f64, colour: Color) {
        let d = b - a;
        let len = d.length();
        if len <= 1e-6 {
            return;
        }
        let mid = (a + b) * 0.5;
        let angle = d.y.atan2(d.x) as f32;
        c.spawn((self.node(mid.x - len * 0.5, mid.y - thick * 0.5, len, thick), BackgroundColor(colour), UiTransform::from_rotation(Rot2::radians(angle))));
    }

    /// A turned square of `side` centred on `(cx, cy)` inside a clipping box at `(x, y, w, h)`:
    /// the part inside the box is a triangle when the centre sits on the box's edge or corner.
    #[allow(clippy::too_many_arguments)]
    fn clipped_diamond(&self, c: &mut ChildSpawnerCommands, x: f64, y: f64, w: f64, h: f64, cx: f64, cy: f64, side: f64, colour: Color) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let mut clip = self.node(x, y, w, h);
        clip.overflow = Overflow::clip();
        let s = self.scale as f64;
        c.spawn(clip).with_children(|c| {
            c.spawn((
                Node { position_type: PositionType::Absolute, left: px(((cx - side * 0.5) * s) as f32), top: px(((cy - side * 0.5) * s) as f32), width: px((side * s) as f32), height: px((side * s) as f32), ..default() },
                BackgroundColor(colour),
                UiTransform::from_rotation(Rot2::degrees(45.0)),
            ));
        });
    }

    fn corner_fill(&self, c: &mut ChildSpawnerCommands, x: f64, y: f64, size: f64, inner: (bool, bool), colour: Color) {
        let (cx, cy) = (if inner.0 { size } else { 0.0 }, if inner.1 { size } else { 0.0 });
        self.clipped_diamond(c, x, y, size, size, cx, cy, size * std::f64::consts::SQRT_2, colour);
    }

    #[allow(clippy::too_many_arguments)]
    fn cut_box(&self, c: &mut ChildSpawnerCommands, x: f64, y: f64, w: f64, h: f64, cut: f64, cuts: Cuts, colour: Color) {
        let k = cut.min(w * 0.5).min(h * 0.5).max(0.0);
        self.rect(c, x + k, y, w - 2.0 * k, h, colour);
        let off = |on: bool| if on { k } else { 0.0 };
        self.rect(c, x, y + off(cuts.tl), k, h - off(cuts.tl) - off(cuts.bl), colour);
        self.rect(c, x + w - k, y + off(cuts.tr), k, h - off(cuts.tr) - off(cuts.br), colour);
        if cuts.tl {
            self.corner_fill(c, x, y, k, (true, true), colour);
        }
        if cuts.tr {
            self.corner_fill(c, x + w - k, y, k, (false, true), colour);
        }
        if cuts.br {
            self.corner_fill(c, x + w - k, y + h - k, k, (false, false), colour);
        }
        if cuts.bl {
            self.corner_fill(c, x, y + h - k, k, (true, false), colour);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn plate(&self, c: &mut ChildSpawnerCommands, x: f64, y: f64, w: f64, h: f64, cut: f64, cuts: Cuts, outline: Color, fill: Color) {
        let b = self.theme.strokes.border;
        self.cut_box(c, x, y, w, h, cut, cuts, outline);
        self.cut_box(c, x + b, y + b, w - 2.0 * b, h - 2.0 * b, (cut - b * 0.6).max(0.0), cuts, fill);
    }

    fn hatch(&self, c: &mut ChildSpawnerCommands, x: f64, y: f64, w: f64, h: f64, colour: Color) {
        let stripe = self.theme.strokes.hatch;
        if stripe <= 0.0 || w <= 0.0 || h <= 0.0 {
            return;
        }
        let mut clip = self.node(x, y, w, h);
        clip.overflow = Overflow::clip();
        let sub = Pen { theme: self.theme, fonts: self.fonts, scale: self.scale };
        c.spawn(clip).with_children(|c| {
            let mut t = -h;
            while t < w {
                sub.line(c, DVec2::new(t, h), DVec2::new(t + h, 0.0), stripe, colour);
                t += stripe * 2.0 * std::f64::consts::SQRT_2;
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn text(&self, c: &mut ChildSpawnerCommands, x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: Face, colour: Color, caps: bool, centred: bool) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let mut n = self.node(x, y, w, h);
        n.align_items = AlignItems::Center;
        n.justify_content = if centred { JustifyContent::Center } else { JustifyContent::Start };
        n.overflow = Overflow::clip();
        let font = match face {
            Face::Display => &self.fonts.display,
            Face::Semibold => &self.fonts.semibold,
            Face::Bold => &self.fonts.bold,
            Face::Mono => &self.fonts.mono,
        };
        let px_size = size as f32 * self.scale;
        let spacing = if caps { self.theme.strokes.caps_tracking as f32 * px_size } else { 0.0 };
        let s = if caps { s.to_uppercase() } else { s.to_string() };
        c.spawn(n).with_children(|c| {
            c.spawn((Text::new(s), TextFont { font: FontSource::Handle(font.clone()), font_size: FontSize::Px(px_size), ..default() }, TextColor(colour), bevy::text::LetterSpacing::Px(spacing), TextLayout::no_wrap()));
        });
    }

    fn glyph(&self, c: &mut ChildSpawnerCommands, g: Glyph, at: DVec2, size: f64, colour: Color) {
        let h = size * 0.5;
        match g {
            Glyph::Square => self.rect(c, at.x - h * 0.85, at.y - h * 0.85, size * 0.85, size * 0.85, colour),
            Glyph::Circle => {
                let mut n = self.node(at.x - h, at.y - h, size, size);
                n.border_radius = BorderRadius::all(percent(50));
                c.spawn((n, BackgroundColor(colour)));
            }
            Glyph::Triangle => self.clipped_diamond(c, at.x - h, at.y - h * 0.5, size, h, h, h, h * std::f64::consts::SQRT_2, colour),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn label(&self, c: &mut ChildSpawnerCommands, x: f64, y: f64, w: f64, h: f64, s: &str, tone: Tone, size: f64, mono: bool) {
        let colour = col(self.theme.tone(tone));
        let mut x0 = x;
        if let Some(g) = tone.glyph() {
            let gs = (size * 0.55).max(6.0);
            self.glyph(c, g, DVec2::new(x + gs * 0.5, y + h * 0.5), gs, colour);
            x0 += gs + 8.0;
        }
        let (face, caps) = match tone {
            Tone::Title => (Face::Bold, true),
            Tone::Micro => (Face::Display, true),
            _ if mono => (Face::Mono, false),
            _ => (Face::Display, false),
        };
        self.text(c, x0, y, (w - (x0 - x)).max(0.0), h, s, size.min(h).max(6.0), face, colour, caps, false);
    }

    fn panel(&self, c: &mut ChildSpawnerCommands, panel: &Panel, ptr: Option<&Pointer>) {
        let (w, h) = (panel.size.x, panel.size.y);
        let k = self.c();
        match &panel.plate {
            Some(p) => {
                self.plate(c, 0.0, 0.0, w, h, display_core::plate::CUT, CUT_TR, col(k.line_2), col(k.bg_1));
                if let Some(hd) = panel.header() {
                    let (hx, hy, hh) = (hd.min.x, hd.min.y, hd.size.y);
                    self.rect(c, hx + 10.0, hy + hh * 0.5 - 5.0, 10.0, 10.0, col(k.accent));
                    let title_w = (p.title.chars().count() as f64 * 14.0 + 8.0).min(hd.size.x * 0.6);
                    self.text(c, hx + 30.0, hy, title_w, hh, &p.title, display_core::font::LABEL, Face::Bold, col(k.fg_title), true, false);
                    self.text(c, hx + 30.0 + title_w + 6.0, hy, 140.0, hh, &p.tag, display_core::font::SMALL, Face::Mono, col(k.fg_2), false, false);
                    if let Some((s, t)) = &p.status {
                        let sw = s.chars().count() as f64 * 11.0 + 30.0;
                        self.label(c, hx + hd.size.x - sw - 18.0, hy, sw, hh, s, *t, display_core::font::SMALL, false);
                    }
                }
                if let Some(wl) = panel.well() {
                    self.rect(c, wl.min.x, wl.min.y, wl.size.x, wl.size.y, col(k.line_1));
                    let b = display_core::plate::WELL_BORDER;
                    self.rect(c, wl.min.x + b, wl.min.y + b, wl.size.x - 2.0 * b, wl.size.y - 2.0 * b, col(k.bg_0));
                }
            }
            None => {
                self.rect(c, 0.0, 0.0, w, h, col(k.line_2));
                let b = self.theme.strokes.border;
                self.rect(c, b, b, w - 2.0 * b, h - 2.0 * b, col(k.bg_0));
            }
        }
        let hover = |a: &str| ptr.is_some_and(|p| p.hover.as_deref() == Some(a));
        let pressed = |a: &str| ptr.is_some_and(|p| p.pressed.as_deref() == Some(a));
        for pl in panel.layout() {
            let r = pl.rect;
            match pl.widget {
                Widget::Label { text, tone, size, mono } => self.label(c, r.min.x, r.min.y, r.size.x, r.size.y, text, *tone, *size, *mono),
                Widget::Button { label, enabled, action, tone } => self.button(c, r, label, *enabled, *tone, hover(action), pressed(action)),
                Widget::Bar { value, label, warn, crit } => self.bar(c, r, *value, label, *warn, *crit),
                Widget::Toggle { on, label, action, labels, enabled } => self.toggle(c, r, *on, label, labels, *enabled, hover(action), pressed(action)),
                Widget::Slider { value, label, action, enabled } => self.slider(c, r, *value, label, *enabled, hover(action), pressed(action)),
                Widget::Dial { value, text, label, warn, crit } => self.dial(c, r, *value, text, label, *warn, *crit),
                Widget::Trend { values, warn, crit } => self.trend(c, r, values, *warn, *crit),
                Widget::List { rows, selected, action, row_h } => {
                    let hover_row = action.as_deref().filter(|a| hover(a)).and_then(|_| ptr.and_then(|p| p.at)).and_then(|at| list_row_at(&r, *row_h, rows.len(), at));
                    self.list(c, r, rows, *selected, hover_row, *row_h);
                }
                Widget::Row { .. } | Widget::Column { .. } | Widget::Space => {}
                Widget::Canvas { shapes, .. } => self.canvas(c, r.min, shapes),
            }
        }
        let every = self.theme.strokes.scanline_every;
        if every >= 2.0 {
            let area = panel.well().unwrap_or(Rect::new(0.0, 0.0, w, h));
            let shade = Color::srgba(0.0, 0.0, 0.0, 0.18);
            let mut ly = area.min.y + every - 1.0;
            while ly < area.max().y {
                self.rect(c, area.min.x, ly, area.size.x, 1.0, shade);
                ly += every;
            }
        }
    }

    /// A canvas: its shapes in its own pixels from `o` (the box's top left). Arcs and dashes are
    /// short turned bars (`line`), a circle is a bordered node, a rect is its fill and outline.
    fn canvas(&self, c: &mut ChildSpawnerCommands, o: DVec2, shapes: &[Shape]) {
        let face = |f: TextFace| match f {
            TextFace::Display => Face::Display,
            TextFace::DisplayBold => Face::Bold,
            TextFace::Mono => Face::Mono,
        };
        for s in shapes {
            match s {
                Shape::Arc { cx, cy, r, from, to, stroke, ink, dash } => {
                    let colour = col(self.theme.ink(*ink));
                    let len = r * (to - from).abs().to_radians();
                    let n = ((len / 2.0).ceil() as usize).clamp(4, 400);
                    let at = |a: f64| DVec2::new(o.x + cx + r * a.to_radians().cos(), o.y + cy + r * a.to_radians().sin());
                    for i in 0..n {
                        let t0 = i as f64 / n as f64;
                        let t1 = (i as f64 + 1.0) / n as f64;
                        if !on_dash(len * (t0 + t1) * 0.5, *dash) {
                            continue;
                        }
                        self.line(c, at(from + (to - from) * t0), at(from + (to - from) * t1), *stroke, colour);
                    }
                }
                Shape::Line { a, b, width, ink, dash } => {
                    let colour = col(self.theme.ink(*ink));
                    let (a, b) = (o + DVec2::new(a[0], a[1]), o + DVec2::new(b[0], b[1]));
                    let len = (b - a).length();
                    match dash {
                        None => self.line(c, a, b, *width, colour),
                        Some(d) => {
                            let n = ((len / 2.0).ceil() as usize).clamp(1, 400);
                            for i in 0..n {
                                let t0 = i as f64 / n as f64;
                                let t1 = (i as f64 + 1.0) / n as f64;
                                if on_dash(len * (t0 + t1) * 0.5, Some(*d)) {
                                    self.line(c, a + (b - a) * t0, a + (b - a) * t1, *width, colour);
                                }
                            }
                        }
                    }
                }
                Shape::Circle { cx, cy, r, fill, stroke } => {
                    let mut n = self.node(o.x + cx - r, o.y + cy - r, r * 2.0, r * 2.0);
                    n.border_radius = BorderRadius::all(percent(50));
                    let bg = fill.map_or(Color::NONE, |f| col(self.theme.ink(f)));
                    match stroke {
                        Some((ink, w)) => {
                            n.border = UiRect::all(px(*w as f32 * self.scale));
                            c.spawn((n, BackgroundColor(bg), BorderColor::all(col(self.theme.ink(*ink)))));
                        }
                        None => {
                            c.spawn((n, BackgroundColor(bg)));
                        }
                    }
                }
                Shape::Rect { x, y, w, h, fill, outline } => {
                    let (x, y) = (o.x + x, o.y + y);
                    if let Some(f) = fill {
                        self.rect(c, x, y, *w, *h, col(self.theme.ink(*f)));
                    }
                    if let Some((ink, t)) = outline {
                        let k = col(self.theme.ink(*ink));
                        self.rect(c, x, y, *w, *t, k);
                        self.rect(c, x, y + h - t, *w, *t, k);
                        self.rect(c, x, y, *t, *h, k);
                        self.rect(c, x + w - t, y, *t, *h, k);
                    }
                }
                Shape::Hatch { x, y, w, h, ink } => self.hatch(c, o.x + x, o.y + y, *w, *h, col(self.theme.ink(*ink))),
                Shape::Text { x, y, w, h, text, size, face: f, ink, centred } => {
                    self.text(c, o.x + x, o.y + y, *w, *h, text, *size, face(*f), col(self.theme.ink(*ink)), false, *centred);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn button(&self, c: &mut ChildSpawnerCommands, r: Rect, label: &str, enabled: bool, tone: Tone, hover: bool, pressed: bool) {
        let k = self.c();
        let st = &self.theme.strokes;
        let hue = match tone {
            Tone::Bad => k.bad,
            Tone::Warn => k.warn,
            _ => k.accent,
        };
        let plain = !matches!(tone, Tone::Bad | Tone::Warn);
        let (outline, fill, edge, fg, drop) = if !enabled {
            (k.line_1, k.bg_1, 0.0, k.fg_3, 0.0)
        } else if pressed {
            (hue, hue, 1.0, k.fg_on_accent, st.press_drop)
        } else if tone == Tone::Good {
            (k.accent, k.accent, 0.0, k.fg_on_accent, 0.0)
        } else if hover {
            (if plain { k.hud } else { hue }, self.theme.tone_dim(tone), st.edge, k.fg_title, 0.0)
        } else {
            (if plain { k.hud_dim } else { hue }, k.bg_2, st.edge, if plain { k.fg_1 } else { hue }, 0.0)
        };
        let y = r.min.y + drop;
        let h = r.size.y - drop;
        self.plate(c, r.min.x, y, r.size.x, h, st.control_cut, CUT_TL_BR, col(outline), col(fill));
        if edge > 0.0 {
            self.rect(c, r.min.x + st.control_cut, y + h - st.border - edge, r.size.x - 2.0 * st.control_cut, edge, col(k.bg_0));
        }
        if !enabled {
            self.hatch(c, r.min.x + st.border + st.control_cut * 0.5, y + st.border, r.size.x - 2.0 * st.border - st.control_cut, h - 2.0 * st.border, col(k.bg_0));
        }
        let size = display_core::font::LABEL.min(h * 0.55);
        self.text(c, r.min.x, y, r.size.x, h - edge, label, size, Face::Semibold, col(fg), true, true);
    }

    fn caption(&self, c: &mut ChildSpawnerCommands, r: Rect, label: &str, min_rest: f64) -> Rect {
        let cap = display_core::font::SMALL + 2.0;
        if label.is_empty() || r.size.y < min_rest + cap {
            return r;
        }
        self.text(c, r.min.x, r.min.y, r.size.x, cap, label, display_core::font::SMALL, Face::Display, col(self.c().hud_dim), true, false);
        Rect::new(r.min.x, r.min.y + cap, r.size.x, r.size.y - cap)
    }

    fn zone_colour(&self, lvl: Level) -> Rgb {
        let k = self.c();
        match lvl {
            Level::Normal => k.good,
            Level::Warn => k.warn,
            Level::Crit => k.bad,
        }
    }

    fn bar(&self, c: &mut ChildSpawnerCommands, r: Rect, value: f64, label: &str, warn: Option<f64>, crit: Option<f64>) {
        let k = self.c();
        let r = self.caption(c, r, label, 20.0);
        let marks = 8.0_f64.min(r.size.y * 0.25);
        let (x, y, w, h) = (r.min.x, r.min.y, r.size.x, r.size.y - marks);
        let b = self.theme.strokes.border;
        self.rect(c, x, y, b, h, col(k.hud_dim));
        self.rect(c, x + w - b, y, b, h, col(k.hud_dim));
        self.rect(c, x, y + h - b, w, b, col(k.hud_dim));
        let ticks = ((w / 9.0).floor() as usize).clamp(8, 40);
        let inner = w - 2.0 * b - 6.0;
        let tw = (inner - 2.0 * (ticks as f64 - 1.0)) / ticks as f64;
        let f = value.clamp(0.0, 1.0);
        for i in 0..ticks {
            let t = (i as f64 + 0.5) / ticks as f64;
            let lvl = level(t, warn, crit);
            let share = if i % 5 == 0 {
                1.0
            } else {
                match lvl {
                    Level::Normal => 0.6,
                    Level::Warn => 0.8,
                    Level::Crit => 1.0,
                }
            };
            let th = (h - b - 3.0) * share;
            let tx = x + b + 3.0 + i as f64 * (tw + 2.0);
            let colour = if t <= f { col(self.zone_colour(lvl)) } else { col_a(k.bg_3, 0.7) };
            self.rect(c, tx, y + h - b - th, tw, th, colour);
        }
        for (t, colour) in [(warn, k.warn), (crit, k.bad)] {
            if let Some(t) = t {
                self.rect(c, x + t.clamp(0.0, 1.0) * w - 3.0, y + h + 1.0, 6.0, marks - 1.0, col(colour));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn toggle(&self, c: &mut ChildSpawnerCommands, r: Rect, on: bool, label: &str, labels: &[String; 2], enabled: bool, hover: bool, pressed: bool) {
        let k = self.c();
        let st = &self.theme.strokes;
        let mut r = r;
        if !label.is_empty() {
            let lw = (r.size.x * 0.4).min(label.chars().count() as f64 * 11.0 + 12.0);
            self.text(c, r.min.x, r.min.y, lw, r.size.y, label, display_core::font::SMALL, Face::Display, col(k.hud_dim), true, false);
            r = Rect::new(r.min.x + lw, r.min.y, r.size.x - lw, r.size.y);
        }
        let outline = if !enabled {
            k.line_1
        } else if pressed {
            k.accent
        } else if hover {
            k.hud
        } else {
            k.hud_dim
        };
        let fill = if !enabled {
            k.bg_0
        } else if hover {
            k.accent_dim
        } else {
            k.bg_1
        };
        self.plate(c, r.min.x, r.min.y, r.size.x, r.size.y, st.control_cut, CUT_TL_BR, col(outline), col(fill));
        let pad = st.border + 3.0;
        let sw = (r.size.x - 2.0 * pad - 3.0) / 2.0;
        for (i, txt) in labels.iter().enumerate() {
            let active = (i == 1) == on;
            let sx = r.min.x + pad + i as f64 * (sw + 3.0);
            let fg = match (active, enabled) {
                (true, true) => k.fg_on_accent,
                (false, true) => k.fg_2,
                _ => k.fg_3,
            };
            if active {
                let bg = if !enabled {
                    k.bg_3
                } else if pressed {
                    k.fg_title
                } else {
                    k.accent
                };
                self.rect(c, sx, r.min.y + pad, sw, r.size.y - 2.0 * pad, col(bg));
            }
            self.text(c, sx, r.min.y + pad, sw, r.size.y - 2.0 * pad, txt, display_core::font::LABEL.min(r.size.y * 0.45), Face::Semibold, col(fg), true, true);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn slider(&self, c: &mut ChildSpawnerCommands, r: Rect, value: f64, label: &str, enabled: bool, hover: bool, pressed: bool) {
        let k = self.c();
        let r = self.caption(c, r, label, 40.0);
        let (x, y, w, h) = (r.min.x, r.min.y, r.size.x, r.size.y);
        let f = value.clamp(0.0, 1.0);
        let u = h / 48.0;
        let edge = if enabled { k.hud_dim } else { k.line_1 };
        self.rect(c, x, y + 14.0 * u, w, 2.0, col(edge));
        self.rect(c, x, y + 14.0 * u + 2.0, w, 4.0 * u, col(k.bg_3));
        self.rect(c, x, y + 14.0 * u + 2.0, w * f, 4.0 * u, if enabled { col(k.accent) } else { col_a(k.fg_3, 0.4) });
        for i in 0..=20 {
            let tx = x + (w - 2.0) * i as f64 / 20.0;
            let th = if i % 5 == 0 { 10.0 } else { 5.0 } * u;
            self.rect(c, tx, y + 26.0 * u, 2.0, th, col(edge));
        }
        let handle = if !enabled {
            k.fg_3
        } else if pressed {
            k.fg_title
        } else if hover {
            k.accent
        } else {
            k.hud
        };
        let hw = if pressed { 14.0 } else { 16.0 };
        let hx = x + w * f - hw * 0.5;
        self.rect(c, hx, y + 2.0 * u + hw * 0.5, hw, 34.0 * u - hw * 0.5, col(handle));
        self.clipped_diamond(c, hx, y + 2.0 * u, hw, hw * 0.5, hw * 0.5, hw * 0.5, hw * 0.5 * std::f64::consts::SQRT_2, col(handle));
    }

    #[allow(clippy::too_many_arguments)]
    fn dial(&self, c: &mut ChildSpawnerCommands, r: Rect, value: f64, text: &str, label: &str, warn: Option<f64>, crit: Option<f64>) {
        let k = self.c();
        let cap = if label.is_empty() { 0.0 } else { display_core::font::LABEL + 4.0 };
        let size = r.size.x.min((r.size.y - cap) / 0.86);
        if size <= 40.0 {
            return;
        }
        let centre = DVec2::new(r.min.x + r.size.x * 0.5, r.min.y + size * 0.5);
        let rad = size * 0.5 - 18.0;
        let f = value.clamp(0.0, 1.0);
        let lvl = level(value, warn, crit);
        let at = |deg: f64, rr: f64| centre + DVec2::new(deg.to_radians().cos(), deg.to_radians().sin()) * rr;
        let (a0, sweep) = (135.0, 270.0);
        for i in 0..28 {
            let a = a0 + sweep * i as f64 / 27.0;
            let len = if i % 3 == 0 { 7.0 } else { 4.0 };
            self.line(c, at(a, rad + 16.0), at(a, rad + 16.0 + len), 2.0, col(k.hud_dim));
        }
        let segs = 30;
        let thick = match lvl {
            Level::Normal => 12.0,
            Level::Warn => 15.0,
            Level::Crit => 18.0,
        };
        for i in 0..segs {
            let (t0, t1) = (i as f64 / segs as f64, (i as f64 + 0.8) / segs as f64);
            let lit = (t0 + t1) * 0.5 <= f;
            let colour = if lit { col(self.zone_colour(lvl)) } else { col(k.bg_3) };
            self.line(c, at(a0 + sweep * t0, rad), at(a0 + sweep * t1, rad), if lit { thick } else { 12.0 }, colour);
        }
        for (t, colour) in [(warn, k.warn), (crit, k.bad)] {
            if let Some(t) = t {
                let a = a0 + sweep * t.clamp(0.0, 1.0);
                self.line(c, at(a, rad - 12.0), at(a, rad + 13.0), 4.0, col(colour));
            }
        }
        self.line(c, centre, at(a0 + sweep * f, rad - 16.0), 3.0, col(k.hud));
        let mut hub = self.node(centre.x - 6.0, centre.y - 6.0, 12.0, 12.0);
        hub.border = UiRect::all(px(2.0 * self.scale));
        hub.border_radius = BorderRadius::all(percent(50));
        c.spawn((hub, BackgroundColor(col(k.bg_0)), BorderColor::all(col(k.hud))));
        self.text(c, centre.x - size * 0.5, centre.y + rad * 0.78 - 18.0, size, 36.0, text, display_core::font::TITLE, Face::Mono, col(k.fg_1), false, true);
        if !label.is_empty() {
            self.text(c, r.min.x, r.min.y + size * 0.86 + 4.0, r.size.x, cap - 4.0, label, display_core::font::LABEL, Face::Display, col(k.fg_2), true, true);
        }
    }

    fn trend(&self, c: &mut ChildSpawnerCommands, r: Rect, values: &[f64], warn: Option<f64>, crit: Option<f64>) {
        let k = self.c();
        let (x, y, w, h) = (r.min.x, r.min.y, r.size.x, r.size.y);
        let b = self.theme.strokes.border;
        self.rect(c, x, y, w, h, col(k.line_1));
        self.rect(c, x + b, y + b, w - 2.0 * b, h - 2.0 * b, col(k.bg_0));
        for g in [0.25, 0.5, 0.75] {
            self.rect(c, x + b, y + h * g, w - 2.0 * b, 1.0, col(k.line_1));
        }
        let py = |v: f64| y + h - 4.0 - v.clamp(0.0, 1.0) * (h - 8.0);
        for (t, colour, dash) in [(warn, k.warn, 10.0_f64), (crit, k.bad, 3.0)] {
            if let Some(t) = t {
                let mut dx = x + b;
                while dx < x + w - b {
                    self.rect(c, dx, py(t) - 1.0, dash.min(x + w - b - dx), 2.0, col(colour));
                    dx += dash * 1.6;
                }
            }
        }
        let n = values.len();
        if n == 0 {
            return;
        }
        let last = values[n - 1];
        let colour = col(self.zone_colour(level(last, warn, crit)));
        let px_at = |i: usize| x + 4.0 + if n < 2 { 0.0 } else { i as f64 / (n - 1) as f64 * (w - 8.0) };
        for i in 1..n {
            self.line(c, DVec2::new(px_at(i - 1), py(values[i - 1])), DVec2::new(px_at(i), py(values[i])), 3.0, colour);
        }
        self.rect(c, px_at(n - 1) - 5.0, py(last) - 5.0, 10.0, 10.0, colour);
    }

    fn list(&self, c: &mut ChildSpawnerCommands, r: Rect, rows: &[ListRow], selected: Option<usize>, hover: Option<usize>, row_h: f64) {
        let k = self.c();
        let b = self.theme.strokes.border;
        let shown = ((r.size.y - 2.0 * b) / row_h).floor().max(0.0) as usize;
        let used = rows.len().min(shown) as f64 * row_h;
        self.rect(c, r.min.x, r.min.y, r.size.x, used + 2.0 * b, col(k.line_1));
        self.rect(c, r.min.x + b, r.min.y + b, r.size.x - 2.0 * b, used, col(k.bg_0));
        for (i, row) in rows.iter().enumerate().take(shown) {
            let ry = r.min.y + b + i as f64 * row_h;
            let (rx, rw) = (r.min.x + b, r.size.x - 2.0 * b);
            if selected == Some(i) {
                self.rect(c, rx, ry, rw, row_h, col(k.accent_dim));
                self.rect(c, rx, ry, 4.0, row_h, col(k.accent));
            } else if hover == Some(i) {
                self.rect(c, rx, ry, rw, row_h, col(k.bg_3));
            }
            if i > 0 {
                self.rect(c, rx, ry, rw, 1.0, col(k.line_1));
            }
            let gs = 12.0;
            let gc = match row.status.glyph() {
                Some(g) => {
                    let gc = col(self.theme.tone(row.status));
                    self.glyph(c, g, DVec2::new(rx + 12.0 + gs * 0.5, ry + row_h * 0.5), gs, gc);
                    gc
                }
                None => {
                    let gc = col(k.fg_3);
                    let mut n = self.node(rx + 12.0, ry + row_h * 0.5 - gs * 0.5, gs, gs);
                    n.border = UiRect::all(px(2.0 * self.scale));
                    n.border_radius = BorderRadius::all(percent(50));
                    c.spawn((n, BorderColor::all(gc)));
                    gc
                }
            };
            let vw = if row.value.is_empty() { 0.0 } else { (row.value.chars().count() as f64 * 12.0 + 12.0).min(rw * 0.4) };
            self.text(c, rx + 36.0, ry, rw - vw - 44.0, row_h, &row.label, display_core::font::LABEL, Face::Display, col(k.fg_1), false, false);
            if vw > 0.0 {
                let vc = if row.status.glyph().is_some() { gc } else { col(k.fg_2) };
                self.text(c, rx + rw - vw - 8.0, ry, vw, row_h, &row.value, display_core::font::LABEL, Face::Mono, vc, false, false);
            }
        }
    }
}
