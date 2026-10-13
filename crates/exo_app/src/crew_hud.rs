//! The crew HUD (Crew HUD v3, Claude Design project "EXO-1 Ship Displays", decision "Minimal HUD"
//! in the concept repo): as little on screen as possible. Six elements: the compass (top centre),
//! the vitals (bottom left), the interaction prompt (16 px above its target), the action bar
//! (`items.rs`), the key hints (bottom right) and the reticle; the credits show for a moment after
//! a change. Sizes come from the `hud` section of `content/ui/displays.json`, colours and fonts
//! from the same file's tokens (`display_core::Theme`).
//!
//! The simulation half (`plugin`, headless too) fills `CrewHud` every fixed step from the world, so
//! scenarios check what the player would read; the window half only draws it. The rules are in
//! `display_core::hud`.
use crate::controls::{Bindings, Controls, Tap};
use crate::displays::DisplayLook;
use crate::env::PlanetRes;
use crate::gameplay::Gameplay;
use crate::grab::Grab;
use crate::interact::{key_label, Interaction};
use crate::items::{HudWindow, InventoryUi, Items};
use crate::origin::{RenderOrigin, WorldPose};
use crate::ship::Ship;
use crate::view::{MainCamera, ReadoutRow, ViewState};
use crate::walker::Player;
use avian3d::prelude::{Position, Rotation};
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use display_core::hud::{
    compass_ticks, credits_text, delta_text, heading_deg, heading_text, hints, north_at, prompt_anchor, reference, vital_lines, BarIdle, CreditsFlash, CreditsView, Hint, HintAction, HudLook,
    PromptKind, Reference, VitalKind, VitalLine,
};
use display_core::{Rgb, Tone};

/// Health, oxygen and burn of the local player. Nothing writes them yet (there is no health system
/// in the game): later systems set them; a scenario can too. Oxygen and burn are `None` while they
/// do not apply, and the HUD then shows neither.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Vitals {
    /// 0..1.
    pub health: f64,
    /// 0..1 while the air runs out or the suit has a tank; `None` when breathing is no issue.
    pub oxygen: Option<f64>,
    /// 0..1, how hard the burn is; `None` when not burning.
    pub burn: Option<f64>,
}

impl Default for Vitals {
    fn default() -> Self {
        Vitals { health: 1.0, oxygen: None, burn: None }
    }
}

/// What the HUD shows now, written every fixed step (headless too).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct CrewHud {
    /// Degrees 0..360 in `reference`'s frame.
    pub heading: f64,
    pub reference: Reference,
    pub vitals: Vec<VitalLine>,
    pub credits: Option<CreditsView>,
    /// Contrast of every slot but the held one, 1 down to the idle level.
    pub contrast: f64,
    pub hints: Vec<Hint>,
    /// The held item's name (empty with nothing in hand).
    pub held_name: String,
}

impl Default for CrewHud {
    fn default() -> Self {
        CrewHud { heading: 0.0, reference: Reference::True, vitals: Vec::new(), credits: None, contrast: 1.0, hints: Vec::new(), held_name: String::new() }
    }
}

pub fn plugin(app: &mut App) {
    use crate::phases::Fx;
    app.init_resource::<Vitals>().init_resource::<CrewHud>();
    app.add_systems(FixedUpdate, (update_compass, update_vitals, update_credits, update_bar).in_set(Fx::Effects));
}

fn update_compass(
    planet: Res<PlanetRes>,
    players: Query<&Player>,
    ships: Query<(&Ship, &Position, &Rotation)>,
    mut hud: ResMut<CrewHud>,
) {
    let (Ok(pl), Some((ship, sp, sr))) = (players.single(), ships.iter().next()) else { return };
    let frame = walker_core::Frame { origin: sp.0, rot: sr.0 };
    let near = |p: DVec3| (p - planet.centre).length() < crate::sky::impostor_from(planet.radius, planet.field.atmosphere_height);
    let inside = pl.ship.is_some() || pl.seated;
    let here = if inside { sp.0 } else { pl.w.pos };
    let r = reference(inside, ship.lag.landed, near(here));
    let bow = sr.0 * DVec3::NEG_Z;
    let look = if pl.seated { bow } else { pl.world_look(frame) };
    let (up, fwd) = match r {
        Reference::True => {
            let up = planet.up(here);
            (up, north_at(up))
        }
        Reference::Bow => (sr.0 * DVec3::Y, bow),
    };
    // Straight up or down has no heading: the compass keeps what it shows.
    if let Some(h) = heading_deg(look, up, fwd) {
        hud.heading = h;
    }
    hud.reference = r;
}

fn update_vitals(vitals: Res<Vitals>, look: Res<DisplayLook>, mut hud: ResMut<CrewHud>) {
    let lines = vital_lines(vitals.health, vitals.oxygen, vitals.burn, &look.0.hud.vitals);
    if hud.vitals != lines {
        hud.vitals = lines;
    }
}

fn update_credits(time: Res<Time>, gp: Res<Gameplay>, look: Res<DisplayLook>, mut flash: Local<CreditsFlash>, mut hud: ResMut<CrewHud>) {
    let view = flash.step(gp.progress.wallet(), time.delta_secs_f64(), &look.0.hud.credits);
    if hud.credits != view {
        hud.credits = view;
    }
}

/// What counts as the bar being used: the slots, the hand, cargo in the hands, Tab.
type BarState = (usize, Vec<Option<(String, u32)>>, u8, bool);

fn update_bar(
    time: Res<Time>,
    look: Res<DisplayLook>,
    items: Res<Items>,
    grab: Res<Grab>,
    mut ui: ResMut<InventoryUi>,
    mut idle: Local<BarIdle>,
    mut seen: Local<Option<BarState>>,
    mut hud: ResMut<CrewHud>,
) {
    let view = &items.client.view;
    let carrying = grab.held.map_or(0, |h| h.hands);
    let state: BarState = (view.selected, (0..item_core::BAR_SLOTS).map(|i| view.slot(i).map(|s| (s.item.kind.to_string(), s.count))).collect(), carrying, ui.open);
    let pressed = std::mem::take(&mut ui.touched);
    if seen.as_ref() != Some(&state) || ui.open || pressed {
        idle.touch();
    }
    *seen = Some(state);
    idle.step(time.delta_secs_f64());
    let contrast = idle.contrast(&look.0.hud.bar);
    if hud.contrast != contrast {
        hud.contrast = contrast;
    }
    let held = items.in_hand(carrying);
    let usable = held.is_some_and(|s| items.defs.of(s).is_some_and(|d| d.use_effect.is_some()));
    let want = hints(held.is_some(), usable, carrying > 0);
    if hud.hints != want {
        hud.hints = want;
    }
    let name = held.map(|s| items.name(s)).unwrap_or_default();
    if hud.held_name != name {
        hud.held_name = name;
    }
}

// ---------- window: look helpers ----------

pub(crate) fn col(c: Rgb) -> Color {
    Color::srgb(c.0[0], c.0[1], c.0[2])
}

pub(crate) fn col_a(c: Rgb, a: f64) -> Color {
    Color::srgba(c.0[0], c.0[1], c.0[2], a.clamp(0.0, 1.0) as f32)
}

/// The status colour of a vital: normal in steel (or `normal`), warn and bad as the tokens.
fn vital_colour(look: &display_core::Theme, normal: Rgb, t: Tone) -> Rgb {
    match t {
        Tone::Warn => look.colours.warn,
        Tone::Bad => look.colours.bad,
        _ => normal,
    }
}

/// The HUD's fonts, from the display design system (the same files the screens use).
#[derive(Resource, Clone)]
pub struct HudFonts {
    pub display: Handle<Font>,
    pub bold: Handle<Font>,
    pub mono: Handle<Font>,
}

/// A HUD text: the font, a size, a colour and the dark shadow that keeps it legible on bright ground.
pub(crate) fn hud_text(font: &Handle<Font>, s: &str, size: f64, colour: Color, look: &HudLook) -> impl Bundle {
    (
        Text::new(s),
        TextFont { font: bevy::text::FontSource::Handle(font.clone()), font_size: FontSize::Px(size as f32), ..default() },
        TextColor(colour),
        TextShadow { offset: Vec2::splat(look.text.shadow_px as f32), color: Color::srgba(0.0, 0.0, 0.0, look.text.shadow_alpha as f32) },
        TextLayout::no_wrap(),
    )
}

/// A dashed frame's container; `Dash` parts carry the same id.
#[derive(Component)]
pub(crate) struct DashFrame(pub u32);
#[derive(Component)]
pub(crate) struct Dash(pub u32);

/// Dashes along the edges of a `w` x `h` box, `thick` px thick, hidden until `DashFrame(id)` is
/// shown. They sit over the box's border (offset by `border`).
pub(crate) fn spawn_dashes(p: &mut ChildSpawnerCommands, id: u32, (w, h): (f32, f32), border: f32, thick: f32, look: &HudLook, colour: Color) {
    let (dash, step) = (look.prompt.dash as f32, (look.prompt.dash + look.prompt.dash_gap) as f32);
    p.spawn((DashFrame(id), Node { position_type: PositionType::Absolute, left: px(-border), top: px(-border), width: px(w), height: px(h), display: Display::None, ..default() })).with_children(|f| {
        let mut part = |x: f32, y: f32, pw: f32, ph: f32| {
            f.spawn((Dash(id), Node { position_type: PositionType::Absolute, left: px(x), top: px(y), width: px(pw), height: px(ph), ..default() }, BackgroundColor(colour)));
        };
        let mut x = 0.0;
        while x < w {
            let len = dash.min(w - x);
            part(x, 0.0, len, thick);
            part(x, h - thick, len, thick);
            x += step;
        }
        let mut y = step;
        while y < h - thick {
            let len = dash.min(h - thick - y);
            part(0.0, y, thick, len);
            part(w - thick, y, thick, len);
            y += step;
        }
    });
}

/// Shows a dashed frame and sets the colour of its dashes (only touched when it changes).
pub(crate) fn set_dashes<F: bevy::ecs::query::QueryFilter, G: bevy::ecs::query::QueryFilter>(
    id: u32,
    on: bool,
    colour: Color,
    frames: &mut Query<(&DashFrame, &mut Node), F>,
    dashes: &mut Query<(&Dash, &mut BackgroundColor), G>,
) {
    let want = if on { Display::Flex } else { Display::None };
    for (f, mut n) in frames {
        if f.0 == id && n.display != want {
            n.display = want;
        }
    }
    if on {
        for (d, mut bg) in dashes {
            if d.0 == id && bg.0 != colour {
                bg.0 = colour;
            }
        }
    }
}

// ---------- window: the elements ----------

pub fn window_plugin(app: &mut App) {
    use crate::phases::Frame;
    app.add_systems(Startup, spawn_hud);
    app.add_systems(
        Update,
        (update_compass_ui, update_vitals_ui, update_prompt_ui, update_hints_ui, update_reticle, update_credits_ui, update_readout_row).in_set(Frame::Hud),
    );
}

#[derive(Component)]
struct CompassHeading;
/// One mark of the strip: the wrapper holding the tick and its label.
#[derive(Component)]
struct CompassTick(usize);
#[derive(Component)]
struct CompassBar(usize);
#[derive(Component)]
struct CompassLabel(usize);

#[derive(Component)]
struct VitalRow(VitalKind);
#[derive(Component)]
struct VitalFill(VitalKind);

#[derive(Component)]
struct PromptRoot;
#[derive(Component)]
struct PromptKey;
#[derive(Component)]
struct PromptKeyText;
#[derive(Component)]
struct PromptText;
#[derive(Component)]
struct PromptUnderline;
/// The id of the prompt key's dashed frame (the bar's slots use their own index).
const PROMPT_DASHES: u32 = 100;

#[derive(Component)]
struct HintRow(usize);
#[derive(Component)]
struct HintKey(usize);
#[derive(Component)]
struct HintKeyText(usize);
#[derive(Component)]
struct HintLabel(usize);

#[derive(Component)]
struct ReticleRoot;
#[derive(Component)]
struct ReticleRing;

#[derive(Component)]
struct CreditsRoot;
#[derive(Component)]
struct CreditsValue;
#[derive(Component)]
struct CreditsDelta;

/// Marks used on the strip: label wrapper width (px).
const TICK_BOX: f32 = 44.0;

fn spawn_hud(mut commands: Commands, look: Res<DisplayLook>, assets: Res<AssetServer>) {
    let theme = &look.0;
    let (k, h) = (&theme.colours, &theme.hud);
    let fonts = HudFonts { display: assets.load(&theme.fonts.display), bold: assets.load(&theme.fonts.display_bold), mono: assets.load(&theme.fonts.mono) };
    commands.insert_resource(fonts.clone());
    let m = h.margin as f32;
    let overlay = || Node { position_type: PositionType::Absolute, ..default() };

    // Compass: the heading number, a small pointer, the strip.
    let c = &h.compass;
    let strip_h = (c.major_tick_px + c.label_size + 6.0) as f32;
    commands
        .spawn(Node { top: px(m * 0.75), left: percent(50.0), margin: UiRect::left(px(-(c.width as f32) * 0.5)), width: px(c.width as f32), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..overlay() })
        .with_children(|root| {
            root.spawn((CompassHeading, hud_text(&fonts.mono, "000°", c.heading_size, col(k.fg_title), h)));
            root.spawn((Node { width: px(7), height: px(7), margin: UiRect::vertical(px(4)), ..default() }, UiTransform { rotation: Rot2::degrees(45.0), ..UiTransform::IDENTITY }, BackgroundColor(col(k.fg_title))));
            root.spawn(Node { width: px(c.width as f32), height: px(strip_h), ..default() }).with_children(|strip| {
                let pool = (c.span_deg / c.tick_every_deg).ceil() as usize + 2;
                for i in 0..pool {
                    strip
                        .spawn((CompassTick(i), Node { top: px(0), width: px(TICK_BOX), height: px(strip_h), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, display: Display::None, ..overlay() }))
                        .with_children(|t| {
                            t.spawn((CompassBar(i), Node { width: px(1), height: px(c.tick_px as f32), ..default() }, BackgroundColor(col(k.hud))));
                            t.spawn((CompassLabel(i), hud_text(&fonts.mono, "", c.label_size, col(k.fg_1), h)));
                        });
                }
            });
        });

    // Vitals: the health bar, oxygen and burn only while they apply.
    let v = &h.vitals;
    commands.spawn(Node { left: px(m), bottom: px(m), flex_direction: FlexDirection::Column, row_gap: px(v.gap as f32), ..overlay() }).with_children(|root| {
        for (kind, thick, label) in [(VitalKind::Burn, v.thin_h, "BURN"), (VitalKind::Oxygen, v.thin_h, "O2"), (VitalKind::Health, v.bar_h, "")] {
            let show = kind == VitalKind::Health;
            root.spawn((VitalRow(kind), Node { align_items: AlignItems::Center, column_gap: px(8), display: if show { Display::Flex } else { Display::None }, ..default() })).with_children(|row| {
                row.spawn((Node { width: px(v.bar_w as f32), height: px(thick as f32), border: UiRect::all(px(1)), ..default() }, BackgroundColor(col_a(k.bg_0, 0.6)), BorderColor::all(col_a(k.line_2, 0.7)))).with_children(|track| {
                    track.spawn((VitalFill(kind), Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(col(k.hud))));
                });
                if !label.is_empty() {
                    row.spawn(hud_text(&fonts.display, label, v.label_size, col(k.fg_2), h));
                }
            });
        }
    });

    // The prompt: a key, the text, an amber line under it when blocked. Placed each frame.
    let p = &h.prompt;
    let key = p.key_px as f32;
    commands.spawn((PromptRoot, Node { left: px(0), width: px(600), bottom: px(0), justify_content: JustifyContent::Center, display: Display::None, ..overlay() })).with_children(|root| {
        root.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Stretch, row_gap: px(3), ..default() }).with_children(|col_| {
            col_.spawn(Node { align_items: AlignItems::Center, column_gap: px(8), ..default() }).with_children(|row| {
                row.spawn((
                    PromptKey,
                    Node { width: px(key), height: px(key), border: UiRect::all(px(1)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border_radius: BorderRadius::all(px(4)), ..default() },
                    BackgroundColor(col_a(k.bg_0, 0.7)),
                    BorderColor::all(col(k.hud)),
                ))
                .with_children(|cap| {
                    cap.spawn((PromptKeyText, hud_text(&fonts.mono, "F", p.text_size - 2.0, col(k.fg_title), h)));
                    spawn_dashes(cap, PROMPT_DASHES, (key, key), 1.0, 1.5, h, col(k.warn));
                });
                row.spawn((PromptText, hud_text(&fonts.display, "", p.text_size, col(k.fg_1), h)));
            });
            col_.spawn((PromptUnderline, Node { height: px(p.line_px as f32), display: Display::None, ..default() }, BackgroundColor(col(k.warn))));
        });
    });

    // Key hints, bottom right: the keys of the held item.
    let hi = &h.hints;
    commands.spawn(Node { right: px(m), bottom: px(m), column_gap: px(hi.gap as f32), align_items: AlignItems::Center, ..overlay() }).with_children(|root| {
        for i in 0..3 {
            root.spawn((HintRow(i), Node { align_items: AlignItems::Center, column_gap: px(6), display: Display::None, ..default() })).with_children(|row| {
                row.spawn((
                    HintKey(i),
                    Node { min_width: px(hi.key_px as f32), height: px(hi.key_px as f32), padding: UiRect::horizontal(px(6)), border: UiRect::all(px(1)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border_radius: BorderRadius::all(px(4)), ..default() },
                    BackgroundColor(col_a(k.bg_0, 0.7)),
                    BorderColor::all(col(k.hud)),
                ))
                .with_children(|cap| {
                    cap.spawn((HintKeyText(i), hud_text(&fonts.mono, "", hi.size - 1.0, col(k.fg_title), h)));
                });
                row.spawn((HintLabel(i), hud_text(&fonts.display, "", hi.size, col(k.fg_1), h)));
            });
        }
    });

    // Reticle: a small dot, a ring around it on anything F can use.
    let r = &h.reticle;
    commands.spawn((ReticleRoot, Node { left: px(0), top: px(0), width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..overlay() })).with_children(|root| {
        root.spawn((
            ReticleRing,
            Node { width: px(r.ring_px as f32), height: px(r.ring_px as f32), border: UiRect::all(px(r.ring_stroke_px as f32)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border_radius: BorderRadius::MAX, ..default() },
            BorderColor::all(Color::NONE),
        ))
        .with_children(|ring| {
            ring.spawn((Node { width: px(r.dot_px as f32), height: px(r.dot_px as f32), border: UiRect::all(px(1)), border_radius: BorderRadius::MAX, ..default() }, BackgroundColor(col_a(k.fg_title, 0.9)), BorderColor::all(Color::srgba(0.0, 0.0, 0.0, 0.5))));
        });
    });

    // Credits, top right, only for a moment after a change.
    let cr = &h.credits;
    commands.spawn((CreditsRoot, Node { right: px(m), top: px(m), flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, display: Display::None, ..overlay() })).with_children(|root| {
        root.spawn((CreditsValue, hud_text(&fonts.mono, "", cr.size, col(k.fg_1), h)));
        root.spawn((CreditsDelta, hud_text(&fonts.mono, "", cr.delta_size, col(k.good), h)));
    });
}

fn set_text(t: &mut Text, s: &str) {
    if t.0 != s {
        t.0 = s.to_string();
    }
}

fn set_display(n: &mut Node, d: Display) {
    if n.display != d {
        n.display = d;
    }
}

#[allow(clippy::type_complexity)]
fn update_compass_ui(
    hud: Res<CrewHud>,
    look: Res<DisplayLook>,
    fonts: Option<Res<HudFonts>>,
    mut heading: Query<(&mut Text, &mut TextColor), (With<CompassHeading>, Without<CompassLabel>)>,
    mut ticks: Query<(&CompassTick, &mut Node), (Without<CompassBar>, Without<CompassLabel>)>,
    mut bars: Query<(&CompassBar, &mut Node, &mut BackgroundColor), Without<CompassTick>>,
    mut labels: Query<(&CompassLabel, &mut Text, &mut TextColor, &mut TextFont), Without<CompassHeading>>,
) {
    let Some(fonts) = fonts else { return };
    let (theme, c) = (&look.0, &look.0.hud.compass);
    if let Ok((mut t, mut tc)) = heading.single_mut() {
        set_text(&mut t, &heading_text(hud.heading));
        tc.0 = col(theme.colours.fg_title);
    }
    let marks = compass_ticks(hud.heading, hud.reference, c);
    for (t, mut n) in &mut ticks {
        match marks.get(t.0) {
            Some(m) => {
                set_display(&mut n, Display::Flex);
                n.left = px((c.width * (0.5 + m.x)) as f32 - TICK_BOX * 0.5);
            }
            None => set_display(&mut n, Display::None),
        }
    }
    for (b, mut n, mut bg) in &mut bars {
        let Some(m) = marks.get(b.0) else { continue };
        n.height = px(if m.label.is_some() { c.major_tick_px } else { c.tick_px } as f32);
        n.width = px(if m.bold { 2.0 } else { 1.0 });
        bg.0 = col_a(if m.bold { theme.colours.fg_title } else { theme.colours.hud }, m.alpha);
    }
    for (l, mut t, mut tc, mut f) in &mut labels {
        let Some(m) = marks.get(l.0) else { continue };
        set_text(&mut t, m.label.as_deref().unwrap_or(""));
        tc.0 = col_a(if m.bold { theme.colours.fg_title } else { theme.colours.fg_1 }, m.alpha);
        let font = bevy::text::FontSource::Handle(if m.bold { fonts.bold.clone() } else { fonts.mono.clone() });
        if f.font != font {
            f.font = font;
        }
    }
}

fn update_vitals_ui(
    hud: Res<CrewHud>,
    look: Res<DisplayLook>,
    mut rows: Query<(&VitalRow, &mut Node), Without<VitalFill>>,
    mut fills: Query<(&VitalFill, &mut Node, &mut BackgroundColor), Without<VitalRow>>,
) {
    let theme = &look.0;
    for (r, mut n) in &mut rows {
        set_display(&mut n, if hud.vitals.iter().any(|l| l.kind == r.0) { Display::Flex } else { Display::None });
    }
    for (f, mut n, mut bg) in &mut fills {
        let Some(l) = hud.vitals.iter().find(|l| l.kind == f.0) else { continue };
        let w = percent(l.frac as f32 * 100.0);
        if n.width != w {
            n.width = w;
        }
        let normal = if l.kind == VitalKind::Oxygen { theme.colours.accent } else { theme.colours.hud };
        bg.0 = col(vital_colour(theme, normal, l.tone));
    }
}

/// Splits "[F] pick up Wrench" into the key and the rest.
fn split_prompt(p: &str) -> (&str, &str) {
    match p.strip_prefix('[').and_then(|r| r.split_once("] ")) {
        Some((k, rest)) => (k, rest),
        None => ("", p),
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_prompt_ui(
    inter: Res<Interaction>,
    look: Res<DisplayLook>,
    window: Res<HudWindow>,
    (active, ui): (Res<crate::displays::ActiveDisplay>, Res<InventoryUi>),
    origin: Res<RenderOrigin>,
    cam: Query<(&Camera, &WorldPose), With<MainCamera>>,
    mut root: Query<&mut Node, (With<PromptRoot>, Without<PromptUnderline>, Without<DashFrame>)>,
    mut key: Query<(&mut BorderColor, &mut BackgroundColor), (With<PromptKey>, Without<PromptText>)>,
    mut key_text: Query<(&mut Text, &mut TextColor), (With<PromptKeyText>, Without<PromptText>)>,
    mut text: Query<(&mut Text, &mut TextColor), (With<PromptText>, Without<PromptKeyText>)>,
    mut line: Query<&mut Node, (With<PromptUnderline>, Without<PromptRoot>, Without<DashFrame>)>,
    mut frames: Query<(&DashFrame, &mut Node), (Without<PromptRoot>, Without<PromptUnderline>)>,
    mut dashes: Query<(&Dash, &mut BackgroundColor), (Without<PromptKey>, Without<PromptText>)>,
) {
    let (theme, h) = (&look.0, &look.0.hud);
    let k = &theme.colours;
    let (Ok(mut root), Ok(mut line)) = (root.single_mut(), line.single_mut()) else { return };
    // The open inventory has the cursor: no prompt then.
    if inter.prompt.is_empty() || ui.open {
        set_display(&mut root, Display::None);
        return;
    }
    set_display(&mut root, Display::Flex);
    let (key_s, rest) = split_prompt(&inter.prompt);
    let blocked = inter.kind == PromptKind::Blocked;
    let (edge, ink) = match inter.kind {
        PromptKind::Use => (k.hud, k.fg_1),
        PromptKind::Collect => (k.good, k.fg_1),
        PromptKind::Leave => (k.fg_2, k.fg_2),
        PromptKind::Blocked => (k.warn, k.warn),
    };
    if let Ok((mut b, mut bg)) = key.single_mut() {
        // A blocked key has no solid outline: it is drawn dashed.
        let want = BorderColor::all(if blocked { Color::NONE } else { col(edge) });
        if *b != want {
            *b = want;
        }
        bg.0 = col_a(k.bg_0, 0.7);
    }
    if let Ok((mut t, mut c)) = key_text.single_mut() {
        set_text(&mut t, key_s);
        c.0 = col(if blocked { k.warn } else { k.fg_title });
    }
    if let Ok((mut t, mut c)) = text.single_mut() {
        set_text(&mut t, rest);
        c.0 = col(ink);
    }
    set_display(&mut line, if blocked { Display::Flex } else { Display::None });
    set_dashes(PROMPT_DASHES, blocked, col(k.warn), &mut frames, &mut dashes);
    // 16 px above the target; a target without a place on screen (or none) uses the spot below the
    // reticle, and at a screen in use the prompt drops to the bottom out of its way.
    let screen = inter.at.and_then(|p| {
        let (camera, wp) = cam.single().ok()?;
        let gt = GlobalTransform::from(Transform::from_translation((wp.pos - origin.origin).as_vec3()).with_rotation(wp.rot.as_quat()));
        let v = camera.world_to_viewport(&gt, (p - origin.origin).as_vec3()).ok()?;
        Some(DVec2::new(v.x as f64, v.y as f64))
    });
    let w = DVec2::new(window.0.x as f64, window.0.y as f64);
    let mut anchor = prompt_anchor(screen, w, h);
    if active.using().is_some() {
        anchor.y = w.y * 0.9;
    }
    root.left = px(anchor.x as f32 - 300.0);
    root.bottom = px((w.y - anchor.y) as f32);
}

#[allow(clippy::type_complexity)]
fn update_hints_ui(
    hud: Res<CrewHud>,
    look: Res<DisplayLook>,
    bindings: Res<Bindings>,
    mut rows: Query<(&HintRow, &mut Node), (Without<HintKey>, Without<HintLabel>)>,
    mut keys: Query<(&HintKey, &mut BorderColor)>,
    mut key_texts: Query<(&HintKeyText, &mut Text, &mut TextColor), Without<HintLabel>>,
    mut labels: Query<(&HintLabel, &mut Text, &mut TextColor), Without<HintKeyText>>,
) {
    let k = &look.0.colours;
    let hint = |i: usize| hud.hints.get(i);
    for (r, mut n) in &mut rows {
        set_display(&mut n, if hint(r.0).is_some() { Display::Flex } else { Display::None });
    }
    let key_of = |a: HintAction| match a {
        HintAction::Use => "LMB".to_string(),
        HintAction::Drop => key_label(&bindings, Tap::Drop),
        HintAction::Throw => key_label(&bindings, Tap::Throw),
    };
    for (i, mut b) in &mut keys {
        if let Some(h) = hint(i.0) {
            *b = BorderColor::all(col_a(k.hud, if h.dim { 0.4 } else { 1.0 }));
        }
    }
    for (i, mut t, mut c) in &mut key_texts {
        if let Some(h) = hint(i.0) {
            set_text(&mut t, &key_of(h.action));
            c.0 = col_a(k.fg_title, if h.dim { 0.5 } else { 1.0 });
        }
    }
    for (i, mut t, mut c) in &mut labels {
        if let Some(h) = hint(i.0) {
            set_text(&mut t, match h.action {
                HintAction::Use => "use",
                HintAction::Drop => "drop",
                HintAction::Throw => "throw",
            });
            c.0 = col_a(k.fg_1, if h.dim { 0.5 } else { 1.0 });
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_reticle(
    inter: Res<Interaction>,
    controls: Res<Controls>,
    look: Res<DisplayLook>,
    players: Query<&Player>,
    mut root: Query<&mut Node, With<ReticleRoot>>,
    mut ring: Query<&mut BorderColor, With<ReticleRing>>,
) {
    let seated = players.single().is_ok_and(|p| p.seated);
    if let Ok(mut n) = root.single_mut() {
        set_display(&mut n, if seated || controls.free_cursor { Display::None } else { Display::Flex });
    }
    if let Ok(mut b) = ring.single_mut() {
        let usable = inter.target.is_some() && inter.kind != PromptKind::Blocked;
        let want = BorderColor::all(if usable { col_a(look.0.colours.fg_1, 0.85) } else { Color::NONE });
        if *b != want {
            *b = want;
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_credits_ui(
    hud: Res<CrewHud>,
    look: Res<DisplayLook>,
    mut root: Query<&mut Node, With<CreditsRoot>>,
    mut value: Query<(&mut Text, &mut TextColor, &mut TextShadow), (With<CreditsValue>, Without<CreditsDelta>)>,
    mut delta: Query<(&mut Text, &mut TextColor, &mut TextShadow), (With<CreditsDelta>, Without<CreditsValue>)>,
) {
    let (theme, k) = (&look.0, &look.0.colours);
    let Ok(mut n) = root.single_mut() else { return };
    let Some(c) = &hud.credits else {
        set_display(&mut n, Display::None);
        return;
    };
    set_display(&mut n, Display::Flex);
    let shadow = Color::srgba(0.0, 0.0, 0.0, (theme.hud.text.shadow_alpha * c.alpha) as f32);
    if let Ok((mut t, mut tc, mut sh)) = value.single_mut() {
        set_text(&mut t, &credits_text(c.value));
        tc.0 = col_a(k.fg_1, c.alpha);
        sh.color = shadow;
    }
    if let Ok((mut t, mut tc, mut sh)) = delta.single_mut() {
        set_text(&mut t, &delta_text(c.delta));
        tc.0 = col_a(if c.delta < 0 { k.bad } else { k.good }, c.alpha);
        sh.color = shadow;
    }
}

/// The old top-left readout (mode, speed, altitude, boost): gone from the HUD, shown while seated
/// (the flight HUD still needs it) and in the F3 debug view.
fn update_readout_row(view: Res<ViewState>, players: Query<&Player>, mut row: Query<&mut Node, With<ReadoutRow>>) {
    let show = view.debug_hud || players.single().is_ok_and(|p| p.seated);
    if let Ok(mut n) = row.single_mut() {
        set_display(&mut n, if show { Display::Flex } else { Display::None });
    }
}
