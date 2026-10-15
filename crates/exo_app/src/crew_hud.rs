//! The simulation half (`plugin`, headless too) fills `CrewHud` every fixed step from the world, so
//! scenarios check what the player would read; the window half only draws it. The rules are in
use crate::controls::{Bindings, Controls, Tap};
use crate::displays::DisplayLook;
use crate::env::PlanetRes;
use crate::gameplay::Gameplay;
use crate::health::UseHold;
use crate::grab::Grab;
use crate::interact::{key_cap, Interaction};
use crate::items::{HudWindow, InventoryUi, Items};
use crate::origin::{RenderOrigin, WorldPose};
use crate::ship::Ship;
use crate::view::{MainCamera, ReadoutRow, ViewState};
use crate::walker::Player;
use avian3d::prelude::{Position, Rotation};
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use display_core::hud::{
    compass_ticks, credits_number, delta_text, heading_deg, heading_text, display_hints, hint_label, hints, north_at, prompt_anchor, reference, vital_lines, BarIdle, CreditsFlash, CreditsView, Hint, HintAction, HudLook,
    display_prompt_top, health_segments, PromptKind, Reference, TickLen, VitalKind, VitalLine, VitalSymbol, VitalsIn,
};
use display_core::hud::Costs;
use display_core::{Rgb, Tone};

/// while it does not apply, the burn is level 0 without burn, radiation is 0 without.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Vitals {
    /// Vitality's share of the maximum before a headache.
    pub health: f64,
    /// The maximum, as a share of that (a headache lowers it); 1 without.
    pub max_share: f64,
    /// 0..1 while the air runs out or the suit has a tank; `None` when breathing is no issue.
    pub oxygen: Option<f64>,
    /// The burn's level 1 to 3; 0 without burn (or only building up).
    pub burn_level: u8,
    /// The burning buildup, 0..1.
    pub buildup: f64,
    pub burning: bool,
    /// 0..1, how much radiation the body has; the line shows only above 0.
    pub radiation: f64,
    /// Radiation makes nausea (the RAD label says SICK).
    pub sick: bool,
    /// What each injury costs the health, in vitality points: the burn, the oxygen low,
    /// and the headache's part of the maximum.
    pub burn_cost: f64,
    pub oxygen_cost: f64,
    pub headache_cost: f64,
}

impl Default for Vitals {
    fn default() -> Self {
        Vitals { health: 1.0, max_share: 1.0, oxygen: None, burn_level: 0, buildup: 0.0, burning: false, radiation: 0.0, sick: false, burn_cost: 0.0, oxygen_cost: 0.0, headache_cost: 0.0 }
    }
}

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
    let input = VitalsIn {
        health: vitals.health,
        max_share: vitals.max_share,
        oxygen: vitals.oxygen,
        burn_level: vitals.burn_level,
        buildup: vitals.buildup,
        burning: vitals.burning,
        radiation: vitals.radiation,
        sick: vitals.sick,
        costs: Costs { burn: vitals.burn_cost, oxygen: vitals.oxygen_cost, headache: vitals.headache_cost },
    };
    let lines = vital_lines(&input, &look.0.hud.vitals);
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

type BarState = (usize, Vec<Option<(String, u32)>>, u8, bool);

fn update_bar(
    time: Res<Time>,
    look: Res<DisplayLook>,
    items: Res<Items>,
    grab: Res<Grab>,
    active: Res<crate::displays::ActiveDisplay>,
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
    // A display in use takes the hints: the one key that leaves it.
    let want = if active.using().is_some() { display_hints() } else { hints(held.is_some(), usable, carrying > 0) };
    if hud.hints != want {
        hud.hints = want;
    }
    let name = held.map(|s| items.name(s)).unwrap_or_default();
    if hud.held_name != name {
        hud.held_name = name;
    }
}

pub(crate) fn col(c: Rgb) -> Color {
    Color::srgb(c.0[0], c.0[1], c.0[2])
}

pub(crate) fn col_a(c: Rgb, a: f64) -> Color {
    Color::srgba(c.0[0], c.0[1], c.0[2], a.clamp(0.0, 1.0) as f32)
}

pub(crate) fn dark(a: f64) -> Color {
    Color::srgba(0.0, 0.0, 0.0, a.clamp(0.0, 1.0) as f32)
}

fn label_colour(theme: &display_core::Theme, l: &VitalLine) -> Rgb {
    if l.calm() {
        theme.colours.fg_title
    } else {
        tone_colour(theme, l.tone)
    }
}

/// The colour of a vital's tone: calm is the plain text colour (not green), warn and bad the tokens.
fn tone_colour(theme: &display_core::Theme, t: Tone) -> Rgb {
    match t {
        Tone::Warn => theme.colours.warn,
        Tone::Bad => theme.colours.bad,
        _ => theme.colours.fg_1,
    }
}

#[derive(Resource, Clone)]
pub struct HudFonts {
    pub display: Handle<Font>,
    pub semibold: Handle<Font>,
    pub bold: Handle<Font>,
    pub mono: Handle<Font>,
}

impl HudFonts {
    pub fn load(theme: &display_core::Theme, assets: &AssetServer) -> HudFonts {
        HudFonts { display: assets.load(&theme.fonts.display), semibold: assets.load(&theme.fonts.display_semibold), bold: assets.load(&theme.fonts.display_bold), mono: assets.load(&theme.fonts.mono) }
    }
}

pub(crate) fn hud_text(font: &Handle<Font>, s: &str, size: f64, colour: Color, look: &HudLook) -> impl Bundle + use<> {
    (
        Text::new(s),
        TextFont { font: bevy::text::FontSource::Handle(font.clone()), font_size: FontSize::Px(size as f32), ..default() },
        TextColor(colour),
        TextShadow { offset: Vec2::splat(look.text.shadow_px as f32), color: Color::srgba(0.0, 0.0, 0.0, look.text.shadow_alpha as f32) },
        TextLayout::no_wrap(),
    )
}

/// A HUD text in capitals with letter spacing (`em` of the size, the design uses .1).
pub(crate) fn hud_caps(font: &Handle<Font>, s: &str, size: f64, colour: Color, look: &HudLook, em: f64) -> impl Bundle + use<> {
    (hud_text(font, &s.to_uppercase(), size, colour, look), bevy::text::LetterSpacing::Rem(em as f32))
}

/// file changes, so every size in `displays.json` follows a hot reload.
#[derive(Component)]
pub(crate) struct HudPart;

pub(crate) fn abs() -> Node {
    Node { position_type: PositionType::Absolute, ..default() }
}

/// Changes a node only when a field differs (a touched `Node` makes the layout run again).
pub(crate) fn edit(n: &mut Mut<Node>, f: impl FnOnce(&mut Node)) {
    let cur = n.bypass_change_detection();
    let mut next = cur.clone();
    f(&mut next);
    if *cur != next {
        **n = next;
    }
}

pub(crate) fn set_text_colour(c: &mut Mut<TextColor>, v: Color) {
    if c.0 != v {
        c.0 = v;
    }
}

pub(crate) fn set_bg(c: &mut Mut<BackgroundColor>, v: Color) {
    if c.0 != v {
        c.0 = v;
    }
}

pub(crate) fn set_border(c: &mut Mut<BorderColor>, v: Color) {
    let want = BorderColor::all(v);
    if **c != want {
        **c = want;
    }
}

pub(crate) fn set_font(f: &mut Mut<TextFont>, font: &Handle<Font>, size: f64) {
    let (font, size) = (bevy::text::FontSource::Handle(font.clone()), FontSize::Px(size as f32));
    if f.font != font || f.font_size != size {
        f.font = font;
        f.font_size = size;
    }
}

pub(crate) fn set_text(t: &mut Mut<Text>, s: &str) {
    if t.0 != s {
        t.0 = s.to_string();
    }
}

pub(crate) fn set_display(n: &mut Mut<Node>, d: Display) {
    if n.display != d {
        n.display = d;
    }
}

#[derive(Component)]
pub(crate) struct DashFrame(pub u32);
#[derive(Component)]
pub(crate) struct Dash(pub u32);

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

/// A triangle pointing down from one pixel row each (the UI has no triangle).
fn spawn_triangle(p: &mut ChildSpawnerCommands, (left, top): (f32, f32), (w, h): (f32, f32), colour: Color) {
    let rows = h.round().max(1.0) as i32;
    for i in 0..rows {
        let rw = (w * (1.0 - i as f32 / rows as f32)).max(1.0);
        p.spawn((Node { position_type: PositionType::Absolute, left: px(left + (w - rw) * 0.5), top: px(top + i as f32), width: px(rw), height: px(1), ..default() }, BackgroundColor(colour)));
    }
}

fn fade_across(colour: Color, fade: f64) -> BackgroundGradient {
    let clear = colour.with_alpha(0.0);
    let f = (fade * 100.0) as f32;
    BackgroundGradient::from(LinearGradient::to_right(vec![ColorStop::new(clear, percent(0.0)), ColorStop::new(colour, percent(f)), ColorStop::new(colour, percent(100.0 - f)), ColorStop::new(clear, percent(100.0))]))
}

pub fn window_plugin(app: &mut App) {
    use crate::phases::Frame;
    app.add_systems(
        Update,
        (
            build_hud,
            (update_scrims, update_compass_ui, update_vitals_ui, update_prompt_ui, update_hints_ui, update_reticle, update_credits_ui, update_readout_row),
        )
            .chain()
            .in_set(Frame::Hud),
    );
}

pub(crate) fn build_hud(mut commands: Commands, look: Res<DisplayLook>, assets: Res<AssetServer>, parts: Query<Entity, With<HudPart>>, mut built: Local<bool>) {
    if *built && !look.is_changed() {
        return;
    }
    *built = true;
    for e in &parts {
        commands.entity(e).despawn();
    }
    let fonts = HudFonts::load(&look.0, &assets);
    commands.insert_resource(fonts.clone());
    spawn_hud(&mut commands, &look.0, &fonts);
    crate::items::spawn_bar(&mut commands, &look.0, &fonts);
}

#[derive(Component)]
enum ScrimPart {
    Bottom,
    Tab,
}

/// Marks used on the strip: label wrapper width (px).
const TICK_BOX: f32 = 44.0;

#[derive(Component, Clone, Copy)]
enum CompassPart {
    Heading,
    Tick(usize),
    Bar(usize),
    LabelBox(usize),
    Label(usize),
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum VitalPart {
    Row(VitalKind),
    Head(VitalKind),
    Glyph(VitalKind),
    Name(VitalKind),
    Cost(VitalKind),
    Value(VitalKind),
    Track(VitalKind),
    Fill(VitalKind),
    Segment(VitalKind, Cause),
    Cap(VitalKind),
    Ring,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cause {
    Burn,
    Oxygen,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum PromptPart {
    Root,
    Key,
    KeyText,
    Verb,
    Name,
    Sub,
    Note,
}
/// The id of the prompt key's dashed frame (the bar's slots use their own index).
const PROMPT_DASHES: u32 = 100;

#[derive(Component, Clone, Copy)]
enum HintPart {
    Row(usize),
    Key(usize),
    KeyText(usize),
    Label(usize),
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum ReticlePart {
    Root,
    Dot,
    Ring,
    RingInner,
    Hold,
    HoldInner,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum CreditsPart {
    Root,
    Value,
    Unit,
    Delta,
}

/// A 16 px symbol drawn from plain shapes (placeholder): a heart, a bubble, a trefoil or a flame.
fn spawn_symbol(p: &mut ChildSpawnerCommands, sym: VitalSymbol, colour: Color) {
    p.spawn(Node { width: px(16), height: px(16), flex_shrink: 0.0, ..default() }).with_children(|b| {
        let shape = |b: &mut ChildSpawnerCommands, (x, y, w, h): (f32, f32, f32, f32), round: bool, hollow: bool, rot: f32| {
            let mut node = Node { left: px(x), top: px(y), width: px(w), height: px(h), border_radius: if round { BorderRadius::MAX } else { BorderRadius::ZERO }, ..abs() };
            let (fill, edge) = if hollow {
                node.border = UiRect::all(px(2));
                (Color::NONE, BorderColor::all(colour))
            } else {
                (colour, BorderColor::all(Color::NONE))
            };
            b.spawn((node, BackgroundColor(fill), edge, UiTransform::from_rotation(Rot2::degrees(rot))));
        };
        match sym {
            VitalSymbol::Heart => {
                shape(b, (1.0, 2.0, 8.0, 8.0), true, false, 0.0);
                shape(b, (7.0, 2.0, 8.0, 8.0), true, false, 0.0);
                shape(b, (4.0, 6.0, 8.0, 8.0), false, false, 45.0);
            }
            VitalSymbol::Bubble => {
                shape(b, (1.0, 1.0, 14.0, 14.0), true, true, 0.0);
                shape(b, (4.0, 4.0, 4.0, 4.0), true, false, 0.0);
            }
            VitalSymbol::Trefoil => {
                shape(b, (5.0, 0.0, 6.0, 6.0), true, false, 0.0);
                shape(b, (0.0, 9.0, 6.0, 6.0), true, false, 0.0);
                shape(b, (10.0, 9.0, 6.0, 6.0), true, false, 0.0);
                shape(b, (6.0, 6.0, 4.0, 4.0), true, false, 0.0);
            }
            VitalSymbol::Flame => shape(b, (3.5, 3.5, 9.0, 9.0), false, false, 45.0),
        }
    });
}

fn ring_fill(colour: Color, frac: f64) -> BackgroundGradient {
    let end = frac.clamp(0.0, 1.0) as f32 * std::f32::consts::TAU;
    let clear = colour.with_alpha(0.0);
    let stops = vec![AngularColorStop::new(colour, 0.0), AngularColorStop::new(colour, end), AngularColorStop::new(clear, end), AngularColorStop::new(clear, std::f32::consts::TAU)];
    BackgroundGradient::from(ConicGradient::new(UiPosition::CENTER, stops))
}

fn spawn_hud(commands: &mut Commands, theme: &display_core::Theme, fonts: &HudFonts) {
    let (k, h) = (&theme.colours, &theme.hud);
    let m = h.margin as f32;
    let edge = h.edge as f32;
    let soft = |a: f64| col_a(h.soft, a);

    // The scrims behind the lower HUD, below everything else.
    commands.spawn((HudPart, ScrimPart::Bottom, GlobalZIndex(-1), Node { left: px(0), right: px(0), bottom: px(0), height: px(h.scrim.height as f32), ..abs() }, BackgroundGradient::default()));
    commands.spawn((HudPart, ScrimPart::Tab, GlobalZIndex(-1), Node { width: px(h.bar.scrim_w as f32), height: px(h.bar.scrim_h as f32), display: Display::None, ..abs() }, BackgroundGradient::default()));

    let c = &h.compass;
    commands
        .spawn((HudPart, Node { top: px(c.top as f32), left: percent(50.0), margin: UiRect::left(px(-(c.width as f32) * 0.5)), width: px(c.width as f32), height: px(c.height as f32), ..abs() }))
        .with_children(|root| {
            root.spawn(Node { left: px(0), top: px(0), width: percent(100.0), justify_content: JustifyContent::Center, ..abs() }).with_children(|b| {
                b.spawn((CompassPart::Heading, hud_text(&fonts.mono, "000°", c.heading_size, col(k.fg_title), h)));
            });
            let (pw, ph) = (c.pointer_w as f32, c.pointer_h as f32);
            let left = (c.width as f32 - pw) * 0.5;
            spawn_triangle(root, (left - 1.0, c.pointer_y as f32), (pw + 2.0, ph + 1.0), dark(0.7));
            spawn_triangle(root, (left, c.pointer_y as f32), (pw, ph), col(k.fg_title));
            let outline = c.line_px as f32 + 2.0;
            root.spawn((Node { left: px(0), top: px(c.line_y as f32 - 1.0), width: percent(100.0), height: px(outline), ..abs() }, fade_across(dark(0.55), c.fade)));
            root.spawn((Node { left: px(0), top: px(c.line_y as f32), width: percent(100.0), height: px(c.line_px as f32), ..abs() }, fade_across(soft(c.line_alpha), c.fade)));
            let pool = (c.span_deg / c.tick_every_deg).ceil() as usize + 2;
            for i in 0..pool {
                root.spawn((CompassPart::Tick(i), Node { top: px(0), width: px(TICK_BOX), height: percent(100.0), display: Display::None, ..abs() })).with_children(|t| {
                    t.spawn((CompassPart::Bar(i), Node { left: px((TICK_BOX - c.tick_w as f32) * 0.5), top: px(c.tick_y as f32), width: px(c.tick_w as f32), height: px(c.tick_px as f32), ..abs() }, BackgroundColor(soft(1.0))));
                    t.spawn((CompassPart::LabelBox(i), Node { left: px(0), top: px(c.label_top as f32), width: percent(100.0), justify_content: JustifyContent::Center, ..abs() })).with_children(|l| {
                        l.spawn((CompassPart::Label(i), hud_text(&fonts.mono, "", c.label_size, soft(1.0), h)));
                    });
                });
            }
        });

    // health bars; each only while it applies, the health always.
    let v = &h.vitals;
    commands.spawn((HudPart, Node { left: px(edge), bottom: px(edge), width: px(v.width as f32), flex_direction: FlexDirection::Column, row_gap: px(v.gap as f32), ..abs() })).with_children(|root| {
        // The burn line sits on a dark plate: its warm text would vanish on the burning edge.
        root.spawn((VitalPart::Row(VitalKind::Burn), Node { display: Display::None, align_items: AlignItems::Center, column_gap: px(6), padding: UiRect::axes(px(6), px(3)), ..default() }, BackgroundColor(dark(0.6)))).with_children(|row| {
            spawn_symbol(row, VitalSymbol::Flame, col(k.warn));
            row.spawn((VitalPart::Name(VitalKind::Burn), hud_caps(&fonts.semibold, "", v.label_size, col(k.warn), h, 0.1)));
            row.spawn((
                VitalPart::Ring,
                Node { width: px(16), height: px(16), border: UiRect::all(px(2)), border_radius: BorderRadius::MAX, margin: UiRect::left(px(6)), ..default() },
                BorderColor::all(col(k.fg_title)),
                BackgroundGradient::default(),
            ));
        });
        for kind in [VitalKind::Radiation, VitalKind::Oxygen, VitalKind::Health] {
            root.spawn((VitalPart::Row(kind), Node { flex_direction: FlexDirection::Column, row_gap: px(v.label_gap as f32), display: Display::None, ..default() })).with_children(|row| {
                row.spawn((VitalPart::Head(kind), Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Baseline, display: Display::None, ..default() })).with_children(|head| {
                    head.spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() }).with_children(|label| {
                        spawn_symbol(label, kind.symbol(), col(k.fg_title));
                        label.spawn((VitalPart::Glyph(kind), hud_text(&fonts.semibold, "", v.label_size, col(k.warn), h)));
                        label.spawn((VitalPart::Name(kind), hud_caps(&fonts.semibold, "", v.label_size, col(k.fg_title), h, 0.1)));
                    });
                    head.spawn(Node { column_gap: px(8), align_items: AlignItems::Baseline, ..default() }).with_children(|right| {
                        right.spawn((VitalPart::Cost(kind), hud_text(&fonts.mono, "", v.value_size, col(k.fg_1), h)));
                        right.spawn((VitalPart::Value(kind), hud_text(&fonts.mono, "", v.value_size, col(k.fg_1), h)));
                    });
                });
                row.spawn((
                    VitalPart::Track(kind),
                    Node { width: percent(100.0), height: px(v.track_h as f32), ..default() },
                    BackgroundColor(soft(v.track_alpha)),
                    Outline::new(px(1), px(0), dark(0.6)),
                    BoxShadow::default(),
                ))
                .with_children(|track| {
                    track.spawn((VitalPart::Fill(kind), Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(col(k.fg_1))));
                    for cause in [Cause::Burn, Cause::Oxygen] {
                        track.spawn((VitalPart::Segment(kind, cause), Node { display: Display::None, height: percent(100.0), ..abs() }, BackgroundColor(col(if cause == Cause::Burn { k.warn } else { k.accent }))));
                    }
                    track.spawn((VitalPart::Cap(kind), Node { display: Display::None, height: percent(100.0), ..abs() }, BackgroundColor(col_a(k.fg_1, 0.3))));
                });
            });
        }
    });

    let p = &h.prompt;
    let key = p.key_px as f32;
    commands.spawn((HudPart, PromptPart::Root, Node { left: px(0), width: px(600), bottom: px(0), justify_content: JustifyContent::Center, display: Display::None, ..abs() })).with_children(|root| {
        root.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(p.note_gap as f32), ..default() }).with_children(|col_| {
            col_.spawn(Node { align_items: AlignItems::Center, column_gap: px(p.key_gap as f32), ..default() }).with_children(|row| {
                row.spawn((
                    PromptPart::Key,
                    Node { width: px(key), height: px(key), border: UiRect::all(px(p.key_border as f32)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                    BackgroundColor(col_a(k.bg_0, p.key_bg_alpha)),
                    BorderColor::all(col(k.fg_title)),
                ))
                .with_children(|cap| {
                    cap.spawn((PromptPart::KeyText, hud_text(&fonts.mono, "F", p.key_size, col(k.fg_title), h)));
                    spawn_dashes(cap, PROMPT_DASHES, (key, key), p.key_border as f32, p.key_border as f32, h, soft(p.blocked_alpha));
                });
                row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(p.line_gap as f32), ..default() }).with_children(|text| {
                    text.spawn((PromptPart::Verb, hud_caps(&fonts.display, "", p.verb_size, soft(p.verb_alpha), h, 0.1)));
                    text.spawn(Node { align_items: AlignItems::Baseline, column_gap: px(10), ..default() }).with_children(|name| {
                        name.spawn((PromptPart::Name, hud_caps(&fonts.bold, "", p.name_size, col(k.fg_title), h, 0.1)));
                        name.spawn((PromptPart::Sub, hud_text(&fonts.mono, "", p.sub_size, soft(0.8), h)));
                    });
                });
            });
            col_.spawn((PromptPart::Note, Node { display: Display::None, ..default() }, hud_caps(&fonts.display, "", p.note_size, col(k.warn), h, 0.1)));
        });
    });

    let hi = &h.hints;
    commands.spawn((HudPart, Node { right: px(edge), bottom: px(edge), flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, row_gap: px(hi.gap as f32), ..abs() })).with_children(|root| {
        for i in 0..3 {
            root.spawn((HintPart::Row(i), Node { align_items: AlignItems::Center, column_gap: px(hi.key_gap as f32), display: Display::None, ..default() })).with_children(|row| {
                row.spawn((HintPart::Label(i), hud_caps(&fonts.display, "", hi.size, col(k.fg_1), h, hi.spacing)));
                row.spawn((
                    HintPart::Key(i),
                    Node { min_width: px(hi.key_min_w as f32), height: px(hi.key_h as f32), padding: UiRect::horizontal(px(7)), border: UiRect::all(px(hi.key_border as f32)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                    BackgroundColor(col_a(k.bg_0, hi.key_bg_alpha)),
                    BorderColor::all(soft(hi.key_alpha)),
                ))
                .with_children(|cap| {
                    cap.spawn((HintPart::KeyText(i), hud_text(&fonts.mono, "", hi.key_size, col(k.fg_title), h)));
                });
            });
        }
    });

    let r = &h.reticle;
    let (ring, dot) = (r.ring_px as f32, r.dot_px as f32);
    commands.spawn((HudPart, ReticlePart::Root, Node { left: px(0), top: px(0), width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..abs() })).with_children(|root| {
        root.spawn((ReticlePart::Dot, Node { width: px(dot), height: px(dot), border_radius: BorderRadius::MAX, ..default() }, BackgroundColor(col(k.fg_title)), Outline::new(px(r.outline_px as f32), px(0), dark(r.outline_alpha))));
        root.spawn((
            ReticlePart::Hold,
            Node { width: px(ring), height: px(ring), border_radius: BorderRadius::MAX, display: Display::None, ..abs() },
            BackgroundGradient::default(),
        ))
        .with_children(|hold| {
            let inset = r.ring_stroke_px as f32;
            hold.spawn((ReticlePart::HoldInner, Node { position_type: PositionType::Absolute, left: px(inset), top: px(inset), width: px(ring - 2.0 * inset), height: px(ring - 2.0 * inset), border_radius: BorderRadius::MAX, ..default() }, BackgroundColor(dark(0.45))));
        });
        root.spawn((
            ReticlePart::Ring,
            Node { width: px(ring), height: px(ring), border: UiRect::all(px(r.ring_stroke_px as f32)), border_radius: BorderRadius::MAX, display: Display::None, ..abs() },
            BorderColor::all(col(k.fg_title)),
            Outline::new(px(r.outline_px as f32), px(0), dark(r.outline_alpha)),
        ))
        .with_children(|ring| {
            ring.spawn((ReticlePart::RingInner, Node { width: percent(100.0), height: percent(100.0), border: UiRect::all(px(r.outline_px as f32)), border_radius: BorderRadius::MAX, ..default() }, BorderColor::all(dark(r.outline_alpha))));
        });
    });

    let cr = &h.credits;
    commands.spawn((HudPart, CreditsPart::Root, Node { right: px(m), top: px(m), flex_direction: FlexDirection::Column, align_items: AlignItems::FlexEnd, display: Display::None, ..abs() })).with_children(|root| {
        root.spawn(Node { align_items: AlignItems::Baseline, column_gap: px(6), ..default() }).with_children(|row| {
            row.spawn((CreditsPart::Value, hud_text(&fonts.mono, "", cr.size, col(k.fg_title), h)));
            row.spawn((CreditsPart::Unit, hud_caps(&fonts.display, "cr", cr.unit_size, col_a(k.fg_title, cr.unit_alpha), h, 0.1)));
        });
        root.spawn((CreditsPart::Delta, hud_text(&fonts.mono, "", cr.delta_size, col(k.good), h)));
    });
}

#[allow(clippy::type_complexity)]
fn update_scrims(
    ui: Res<InventoryUi>,
    look: Res<DisplayLook>,
    window: Res<HudWindow>,
    mut nodes: Query<(&ScrimPart, &mut Node)>,
    mut gradients: Query<(&ScrimPart, &mut BackgroundGradient)>,
    mut seen: Local<Option<(bool, Vec2)>>,
) {
    let h = &look.0.hud;
    let state = (ui.open, window.0);
    let redraw = *seen != Some(state);
    *seen = Some(state);
    let (s, b) = (&h.scrim, &h.bar);
    let ink = col(s.colour);
    for (part, mut n) in &mut nodes {
        match part {
            ScrimPart::Bottom => edit(&mut n, |n| n.height = px(if ui.open { s.open_height } else { s.height } as f32)),
            ScrimPart::Tab => {
                let (first, size) = crate::items::slot_rect(0, true, window.0, &h.bar);
                let (top, bottom) = (first.y, window.0.y - h.bar.open_bottom as f32);
                let (cx, cy) = (window.0.x * 0.5, (top + bottom) * 0.5);
                let _ = size;
                edit(&mut n, |n| {
                    n.display = if ui.open { Display::Flex } else { Display::None };
                    n.left = px(cx - b.scrim_w as f32 * 0.5);
                    n.top = px(cy - b.scrim_h as f32 * 0.5);
                });
            }
        }
    }
    if !redraw && !look.is_changed() {
        return;
    }
    for (part, mut g) in &mut gradients {
        *g = match part {
            ScrimPart::Bottom => {
                let a = if ui.open { s.open_alpha } else { s.alpha };
                BackgroundGradient::from(LinearGradient::to_top(vec![ColorStop::new(ink.with_alpha(a as f32), percent(0.0)), ColorStop::new(ink.with_alpha(0.0), percent(100.0))]))
            }
            ScrimPart::Tab => BackgroundGradient::from(RadialGradient::new(
                UiPosition::CENTER,
                RadialGradientShape::FarthestCorner,
                vec![
                    ColorStop::new(ink.with_alpha(b.scrim_alpha as f32), percent(0.0)),
                    ColorStop::new(ink.with_alpha(b.scrim_mid_alpha as f32), percent((b.scrim_mid_at * 100.0) as f32)),
                    ColorStop::new(ink.with_alpha(0.0), percent((b.scrim_end_at * 100.0) as f32)),
                ],
            )),
        };
    }
}

#[allow(clippy::type_complexity)]
fn update_compass_ui(
    hud: Res<CrewHud>,
    look: Res<DisplayLook>,
    fonts: Option<Res<HudFonts>>,
    mut nodes: Query<(&CompassPart, &mut Node)>,
    mut fills: Query<(&CompassPart, &mut BackgroundColor)>,
    mut texts: Query<(&CompassPart, &mut Text, &mut TextColor, &mut TextFont)>,
) {
    let Some(fonts) = fonts else { return };
    let (theme, c) = (&look.0, &look.0.hud.compass);
    let soft = |a: f64| col_a(theme.hud.soft, a);
    let marks = compass_ticks(hud.heading, hud.reference, c);
    for (part, mut n) in &mut nodes {
        match *part {
            CompassPart::Tick(i) => match marks.get(i) {
                Some(m) => edit(&mut n, |n| {
                    n.display = Display::Flex;
                    n.left = px((c.width * (0.5 + m.x)) as f32 - TICK_BOX * 0.5);
                }),
                None => set_display(&mut n, Display::None),
            },
            CompassPart::Bar(i) => {
                let Some(m) = marks.get(i) else { continue };
                let len = match m.len {
                    TickLen::Major => c.tick_major_px,
                    TickLen::Mid => c.tick_mid_px,
                    TickLen::Minor => c.tick_px,
                };
                edit(&mut n, |n| n.height = px(len as f32));
            }
            CompassPart::LabelBox(i) => {
                let Some(m) = marks.get(i) else { continue };
                edit(&mut n, |n| n.top = px(if m.bold { c.cardinal_top } else { c.label_top } as f32));
            }
            _ => {}
        }
    }
    for (part, mut bg) in &mut fills {
        if let CompassPart::Bar(i) = *part
            && let Some(m) = marks.get(i)
        {
            let want = if m.bold { col_a(theme.colours.fg_title, m.alpha) } else { soft(c.tick_alpha * m.alpha) };
            set_bg(&mut bg, want);
        }
    }
    for (part, mut t, mut tc, mut f) in &mut texts {
        match *part {
            CompassPart::Heading => set_text(&mut t, &heading_text(hud.heading)),
            CompassPart::Label(i) => {
                let Some(m) = marks.get(i) else { continue };
                set_text(&mut t, m.label.as_deref().unwrap_or(""));
                let (colour, font, size) = if m.bold {
                    (col_a(theme.colours.fg_title, m.alpha), &fonts.bold, c.cardinal_size)
                } else {
                    (soft(c.label_alpha * m.alpha), &fonts.mono, c.label_size)
                };
                set_text_colour(&mut tc, colour);
                set_font(&mut f, font, size);
            }
            _ => {}
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_vitals_ui(
    hud: Res<CrewHud>,
    look: Res<DisplayLook>,
    mut nodes: Query<(&VitalPart, &mut Node)>,
    mut fills: Query<(&VitalPart, &mut BackgroundColor)>,
    mut glows: Query<(&VitalPart, &mut BoxShadow)>,
    mut rings: Query<(&VitalPart, &mut BackgroundGradient)>,
    mut texts: Query<(&VitalPart, &mut Text, &mut TextColor)>,
) {
    let (theme, v) = (&look.0, &look.0.hud.vitals);
    let line = |k: VitalKind| hud.vitals.iter().find(|l| l.kind == k);
    for (part, mut n) in &mut nodes {
        match *part {
            VitalPart::Row(k) => set_display(&mut n, if line(k).is_some() { Display::Flex } else { Display::None }),
            VitalPart::Head(k) => set_display(&mut n, if line(k).is_some() { Display::Flex } else { Display::None }),
            VitalPart::Fill(k) => {
                if let Some(l) = line(k) {
                    edit(&mut n, |n| n.width = percent(l.frac as f32 * 100.0));
                }
            }
            // The health bar's causes: from the cap inward, burn then oxygen (`health_segments`).
            VitalPart::Segment(VitalKind::Health, cause) => match line(VitalKind::Health).map(health_segments) {
                Some(seg) => {
                    let (from, to) = if cause == Cause::Burn { seg.burn } else { seg.oxygen };
                    edit(&mut n, |n| {
                        n.display = if to > from { Display::Flex } else { Display::None };
                        n.left = percent(from as f32 * 100.0);
                        n.width = percent(((to - from) as f32 * 100.0).max(0.0));
                    });
                }
                None => set_display(&mut n, Display::None),
            },
            VitalPart::Cap(k) => match line(k) {
                Some(l) if l.cap < 1.0 => edit(&mut n, |n| {
                    n.display = Display::Flex;
                    n.left = percent(l.cap as f32 * 100.0);
                    n.width = percent((1.0 - l.cap) as f32 * 100.0);
                }),
                _ => set_display(&mut n, Display::None),
            },
            _ => {}
        }
    }
    for (part, mut bg) in &mut fills {
        if let VitalPart::Fill(k) = *part
            && let Some(l) = line(k)
        {
            set_bg(&mut bg, col(tone_colour(theme, l.tone)));
        }
    }
    for (part, mut g) in &mut rings {
        if let VitalPart::Ring = *part
            && let Some(l) = line(VitalKind::Burn)
        {
            *g = ring_fill(col(tone_colour(theme, l.tone)), l.frac);
        }
    }
    for (part, mut shadow) in &mut glows {
        let VitalPart::Track(k) = *part else { continue };
        let want = match line(k) {
            Some(l) if l.tone == Tone::Bad => vec![ShadowStyle { color: col_a(theme.colours.bad, v.glow_alpha), x_offset: px(0), y_offset: px(0), spread_radius: px(0), blur_radius: px(v.glow_px as f32) }],
            _ => Vec::new(),
        };
        if shadow.0 != want {
            shadow.0 = want;
        }
    }
    for (part, mut t, mut tc) in &mut texts {
        match *part {
            VitalPart::Name(VitalKind::Burn) => {
                if let Some(l) = line(VitalKind::Burn) {
                    set_text(&mut t, &l.burn_text());
                    set_text_colour(&mut tc, col(tone_colour(theme, l.tone)));
                }
            }
            VitalPart::Glyph(k) => {
                if let Some(l) = line(k) {
                    set_text(&mut t, l.glyph());
                    set_text_colour(&mut tc, col(tone_colour(theme, l.tone)));
                }
            }
            VitalPart::Name(k) => {
                if let Some(l) = line(k) {
                    set_text(&mut t, &l.label());
                    set_text_colour(&mut tc, col(label_colour(theme, l)));
                }
            }
            VitalPart::Value(k) => {
                if let Some(l) = line(k) {
                    set_text(&mut t, &l.value());
                    set_text_colour(&mut tc, col(tone_colour(theme, l.tone)));
                }
            }
            VitalPart::Cost(k) => {
                if let Some(l) = line(k) {
                    set_text(&mut t, &l.cost_text());
                    set_text_colour(&mut tc, col(tone_colour(theme, l.tone)));
                }
            }
            _ => {}
        }
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
    mut nodes: Query<(&PromptPart, &mut Node)>,
    mut borders: Query<(&PromptPart, &mut BorderColor)>,
    mut texts: Query<(&PromptPart, &mut Text, &mut TextColor)>,
    mut frames: Query<(&DashFrame, &mut Node), Without<PromptPart>>,
    mut dashes: Query<(&Dash, &mut BackgroundColor), Without<PromptPart>>,
) {
    let (theme, h) = (&look.0, &look.0.hud);
    let (k, p) = (&theme.colours, &h.prompt);
    let soft = |a: f64| col_a(h.soft, a);
    // A display in use has the cursor free: no prompt then.
    let show = !inter.prompt.is_empty() && !ui.open && active.using().is_none();
    let project = |at: DVec3| {
        let (camera, wp) = cam.single().ok()?;
        let gt = GlobalTransform::from(Transform::from_translation((wp.pos - origin.origin).as_vec3()).with_rotation(wp.rot.as_quat()));
        let v = camera.world_to_viewport(&gt, (at - origin.origin).as_vec3()).ok()?;
        Some(DVec2::new(v.x as f64, v.y as f64))
    };
    let screen = inter.at.and_then(project);
    let w = DVec2::new(window.0.x as f64, window.0.y as f64);
    let anchor = prompt_anchor(screen, w, h);
    // A display's prompt goes under its bottom edge (never over the display); the others hang above their point.
    let below = inter.edge.map(|e| [project(e[0]).unwrap_or(DVec2::NAN), project(e[1]).unwrap_or(DVec2::NAN)]).map(|e| display_prompt_top(Some(e), w, h));
    let (v, blocked) = (&inter.view, inter.kind == PromptKind::Blocked);
    let name_a = if blocked { p.blocked_alpha + 0.15 } else { 1.0 };
    for (part, mut n) in &mut nodes {
        match *part {
            PromptPart::Root => edit(&mut n, |n| {
                n.display = if show { Display::Flex } else { Display::None };
                n.left = px(anchor.x as f32 - 300.0);
                match below {
                    Some(top) => {
                        n.top = px(top as f32);
                        n.bottom = Val::Auto;
                    }
                    None => {
                        n.top = Val::Auto;
                        n.bottom = px((w.y - anchor.y) as f32);
                    }
                }
            }),
            PromptPart::Note => set_display(&mut n, if v.note.is_empty() { Display::None } else { Display::Flex }),
            _ => {}
        }
    }
    if !show {
        return;
    }
    for (part, mut b) in &mut borders {
        if *part == PromptPart::Key {
            set_border(&mut b, if blocked { Color::NONE } else { col(k.fg_title) });
        }
    }
    let tone_ink = |t: Tone, a: f64| match t {
        Tone::Warn => col_a(k.warn, a),
        Tone::Bad => col_a(k.bad, a),
        _ => soft(0.8 * a),
    };
    for (part, mut t, mut tc) in &mut texts {
        match *part {
            PromptPart::KeyText => {
                set_text(&mut t, inter.prompt.strip_prefix('[').and_then(|r| r.split_once(']')).map_or("F", |(key, _)| key));
                set_text_colour(&mut tc, col_a(k.fg_title, name_a));
            }
            PromptPart::Verb => {
                set_text(&mut t, &v.verb.to_uppercase());
                set_text_colour(&mut tc, soft(p.verb_alpha));
            }
            PromptPart::Name => {
                set_text(&mut t, &v.name.to_uppercase());
                set_text_colour(&mut tc, col_a(k.fg_title, name_a));
            }
            PromptPart::Sub => {
                set_text(&mut t, &v.sub);
                set_text_colour(&mut tc, tone_ink(v.sub_tone, 1.0));
            }
            PromptPart::Note => {
                set_text(&mut t, &v.note.to_uppercase());
                set_text_colour(&mut tc, match v.note_tone {
                    Tone::Warn | Tone::Bad => tone_ink(v.note_tone, 1.0),
                    _ => soft(p.verb_alpha),
                });
            }
            _ => {}
        }
    }
    set_dashes(PROMPT_DASHES, blocked, soft(p.blocked_alpha), &mut frames, &mut dashes);
}

#[allow(clippy::type_complexity)]
fn leave_keys(bound: &str) -> String {
    if bound == "ESC" { bound.to_string() } else { format!("{bound} / ESC") }
}

fn update_hints_ui(
    hud: Res<CrewHud>,
    look: Res<DisplayLook>,
    bindings: Res<Bindings>,
    ui: Res<InventoryUi>,
    mut nodes: Query<(&HintPart, &mut Node)>,
    mut keys: Query<(&HintPart, &mut BorderColor)>,
    mut texts: Query<(&HintPart, &mut Text, &mut TextColor)>,
) {
    let (k, hi) = (&look.0.colours, &look.0.hud.hints);
    let soft = |a: f64| col_a(look.0.hud.soft, a);
    let hint = |i: usize| hud.hints.get(i).filter(|_| !ui.open);
    for (part, mut n) in &mut nodes {
        if let HintPart::Row(i) = *part {
            set_display(&mut n, if hint(i).is_some() { Display::Flex } else { Display::None });
        }
    }
    for (part, mut b) in &mut keys {
        if let HintPart::Key(i) = *part
            && let Some(h) = hint(i)
        {
            set_border(&mut b, soft(hi.key_alpha * if h.dim { 0.5 } else { 1.0 }));
        }
    }
    for (part, mut t, mut c) in &mut texts {
        match *part {
            HintPart::KeyText(i) => {
                if let Some(h) = hint(i) {
                    set_text(&mut t, &match h.action {
                        HintAction::Use => "LMB".to_string(),
                        HintAction::Drop => key_cap(&bindings, Tap::Drop),
                        HintAction::Throw => key_cap(&bindings, Tap::Throw),
                        HintAction::Leave => leave_keys(&key_cap(&bindings, Tap::DisplayLeave)),
                    });
                    set_text_colour(&mut c, col_a(k.fg_title, if h.dim { 0.5 } else { 1.0 }));
                }
            }
            HintPart::Label(i) => {
                if let Some(h) = hint(i) {
                    set_text(&mut t, hint_label(h.action));
                    set_text_colour(&mut c, col_a(k.fg_1, if h.dim { 0.5 } else { 1.0 }));
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_reticle(
    inter: Res<Interaction>,
    controls: Res<Controls>,
    hold: Res<UseHold>,
    rescue: Res<crate::rescue::RescueHold>,
    refill: Res<crate::dev_area::coolant::CoolantHold>,
    look: Res<DisplayLook>,
    players: Query<&Player>,
    mut nodes: Query<(&ReticlePart, &mut Node)>,
    mut grads: Query<(&ReticlePart, &mut BackgroundGradient)>,
) {
    let seated = players.single().is_ok_and(|p| p.seated);
    let usable = inter.target.is_some() && inter.kind != PromptKind::Blocked;
    let progress = hold.progress().or(rescue.progress()).or(refill.progress()).filter(|p| *p > 0.0);
    for (part, mut n) in &mut nodes {
        match *part {
            ReticlePart::Root => set_display(&mut n, if seated || controls.free_cursor { Display::None } else { Display::Flex }),
            ReticlePart::Dot => set_display(&mut n, if usable || progress.is_some() { Display::None } else { Display::Flex }),
            ReticlePart::Ring => set_display(&mut n, if usable && progress.is_none() { Display::Flex } else { Display::None }),
            ReticlePart::Hold => set_display(&mut n, if progress.is_some() { Display::Flex } else { Display::None }),
            ReticlePart::RingInner | ReticlePart::HoldInner => {}
        }
    }
    for (part, mut g) in &mut grads {
        if let (ReticlePart::Hold, Some(p)) = (*part, progress) {
            *g = ring_fill(col(look.0.colours.fg_title), p);
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_credits_ui(
    hud: Res<CrewHud>,
    look: Res<DisplayLook>,
    mut nodes: Query<(&CreditsPart, &mut Node)>,
    mut texts: Query<(&CreditsPart, &mut Text, &mut TextColor, &mut TextShadow)>,
) {
    let (theme, k) = (&look.0, &look.0.colours);
    let Some(c) = &hud.credits else {
        for (part, mut n) in &mut nodes {
            if *part == CreditsPart::Root {
                set_display(&mut n, Display::None);
            }
        }
        return;
    };
    for (part, mut n) in &mut nodes {
        if *part == CreditsPart::Root {
            set_display(&mut n, Display::Flex);
        }
    }
    let shadow = dark(theme.hud.text.shadow_alpha * c.alpha);
    for (part, mut t, mut tc, mut sh) in &mut texts {
        let (want, colour) = match *part {
            CreditsPart::Value => (credits_number(c.value), col_a(k.fg_title, c.alpha)),
            CreditsPart::Unit => ("CR".to_string(), col_a(k.fg_title, theme.hud.credits.unit_alpha * c.alpha)),
            CreditsPart::Delta => (delta_text(c.delta), col_a(if c.delta < 0 { k.bad } else { k.good }, c.alpha)),
            CreditsPart::Root => continue,
        };
        set_text(&mut t, &want);
        set_text_colour(&mut tc, colour);
        if sh.color != shadow {
            sh.color = shadow;
        }
    }
}

fn update_readout_row(view: Res<ViewState>, players: Query<&Player>, mut row: Query<&mut Node, With<ReadoutRow>>) {
    let show = view.debug_hud || players.single().is_ok_and(|p| p.seated);
    if let Ok(mut n) = row.single_mut() {
        set_display(&mut n, if show { Display::Flex } else { Display::None });
    }
}
