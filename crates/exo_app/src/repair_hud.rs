use crate::crew_hud::{col, col_a, dark};
use crate::displays::DisplayLook;
use crate::repair::{RepairConfig, RepairHurt, RepairPlay};
use bevy::math::Rot2;
use bevy::prelude::*;
use bevy::ui::{AngularColorStop, BackgroundGradient, BorderGradient, ColorStop, ConicGradient, RadialGradient, RadialGradientShape, UiPosition};
use device_core::minigame::{minigame_on, Machine, Press};

/// Zones the ring can show at once (the maximum of the rules; a spare slot stays hidden).
const ZONE_SLOTS: usize = 4;
/// The ring's radius (px, on the 1600 x 900 frame, scaled with the HUD like the rest), its stroke,
/// the zone band's width and the progress arc's stroke (the arc sits on the ring's line).
const RADIUS: f32 = 110.0;
const STROKE: f32 = 2.0;
const ZONE_BAND: f32 = 14.0;
const ARC_STROKE: f32 = 4.0;
/// The pointer's tick: its length (px) inside the ring, and its width.
const TICK_LEN: f32 = 26.0;
const TICK_W: f32 = 4.0;
/// The three hit pips of a broken machine: their size and the gap (px), beside the ring.
const PIPS: usize = 3;
const PIP: f32 = 7.0;
const PIP_GAP: f32 = 12.0;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum RepairPart {
    Root,
    Dial,
    Progress,
    Base,
    Zone(usize),
    Mask,
    Pointer,
    Tick,
    Flash,
    Pip(usize),
}

pub fn window_plugin(app: &mut App) {
    use crate::phases::Frame;
    app.add_systems(Update, (spawn_ring, update_ring).chain().in_set(Frame::Hud));
    app.add_systems(Update, (spawn_hurt, update_hurt).chain().in_set(Frame::Hud));
}

#[derive(Component)]
struct HurtEdge;

/// The flash's colour by the machine: white-blue for the shock, orange for the hot machine, none
/// for the knock.
pub fn hurt_colour(m: Machine) -> Option<Color> {
    match m {
        Machine::Electrical => Some(Color::srgb(0.7, 0.9, 1.0)),
        Machine::Hot => Some(Color::srgb(1.0, 0.45, 0.1)),
        Machine::Mechanical => None,
    }
}

/// The flash's alpha at an age: 0.7 at the start, 0 at `flash_s`.
pub fn hurt_alpha(age: f64, flash_s: f64) -> f32 {
    if flash_s <= 0.0 {
        return 0.0;
    }
    (0.7 * (1.0 - age / flash_s)).clamp(0.0, 0.7) as f32
}

fn spawn_hurt(mut commands: Commands, existing: Query<(), With<HurtEdge>>) {
    if existing.is_empty() {
        let full = Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: px(0), bottom: px(0), ..default() };
        commands.spawn((HurtEdge, GlobalZIndex(-2), full, BackgroundGradient::default()));
    }
}

fn update_hurt(time: Res<Time>, hurt: Res<RepairHurt>, cfg: Res<RepairConfig>, mut edges: Query<&mut BackgroundGradient, With<HurtEdge>>) {
    let now = time.elapsed_secs_f64();
    let (colour, flash) = match (hurt.machine.and_then(hurt_colour), hurt.at) {
        (Some(c), Some(at)) => (c, hurt_alpha(now - at, cfg.0.hurt_flash_s)),
        _ => (Color::NONE, 0.0),
    };
    for mut gradient in &mut edges {
        *gradient = BackgroundGradient::from(RadialGradient::new(
            UiPosition::CENTER,
            RadialGradientShape::FarthestCorner,
            vec![ColorStop::new(colour.with_alpha(0.0), percent(55.0)), ColorStop::new(colour.with_alpha(flash), percent(100.0))],
        ));
    }
}

fn disc(r: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: px(-r), top: px(-r), width: px(2.0 * r), height: px(2.0 * r), border_radius: BorderRadius::MAX, display: Display::None, ..default() }
}

fn conic(colour: Color, start: f32, end: f32) -> BackgroundGradient {
    BackgroundGradient::from(ConicGradient::new(UiPosition::CENTER, arc_stops(colour, start, end)))
}

/// The stops of a conic from `start` to `end` (radians from the top, clockwise): the colour there,
/// clear after `end` to the full turn.
fn arc_stops(colour: Color, start: f32, end: f32) -> Vec<AngularColorStop> {
    let clear = colour.with_alpha(0.0);
    vec![AngularColorStop::new(colour, start), AngularColorStop::new(colour, end), AngularColorStop::new(clear, end), AngularColorStop::new(clear, std::f32::consts::TAU)]
}

/// The hold's progress as the arc's end (radians from the top, clockwise): the share of a full turn.
pub fn arc_end(share: f64) -> f32 {
    (share.clamp(0.0, 1.0) as f32) * std::f32::consts::TAU
}

fn part_of(r: &item_core::wire::ReactorNews, d: item_core::wire::RepairDevice) -> item_core::wire::PartNews {
    use item_core::wire::RepairDevice::*;
    match d {
        Reactor => r.reactor,
        CoolantPump => r.pump,
        JunctionBox => r.junction,
        OxygenGenerator => r.oxygen_generator,
        HullBreach => item_core::wire::PartNews { share: 1.0 - r.breach, working: true },
    }
}

fn spawn_ring(mut commands: Commands, look: Res<DisplayLook>, mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;
    let k = &look.0.colours;
    let fg = col(k.fg_title);
    commands.spawn((RepairPart::Root, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: percent(100.0), height: percent(100.0), display: Display::None, ..default() })).with_children(|root| {
        root.spawn((RepairPart::Dial, Node { position_type: PositionType::Absolute, left: percent(50.0), top: percent(50.0), width: px(0), height: px(0), ..default() })).with_children(|dial| {
            // The progress arc: a border of the disc on the ring's radius, painted by a conic gradient
            // that is clear past the progress, so only a stroke along the circle shows (no fill).
            dial.spawn((RepairPart::Progress, Node { border: UiRect::all(px(ARC_STROKE)), ..disc(RADIUS) }, BorderGradient::default()));
            dial.spawn((RepairPart::Base, Node { border: UiRect::all(px(STROKE)), ..disc(RADIUS) }, BorderColor::all(col_a(k.fg_title, 0.35))));
            for i in 0..ZONE_SLOTS {
                dial.spawn((RepairPart::Zone(i), disc(RADIUS), BackgroundGradient::default()));
            }
            dial.spawn((RepairPart::Mask, disc(RADIUS - ZONE_BAND), BackgroundColor(dark(0.25))));
            dial.spawn((RepairPart::Pointer, Node { position_type: PositionType::Absolute, left: px(0), top: px(0), width: px(0), height: px(0), display: Display::None, ..default() }, UiTransform::default())).with_children(|p| {
                p.spawn((RepairPart::Tick, Node { position_type: PositionType::Absolute, left: px(-TICK_W * 0.5), top: px(-RADIUS), width: px(TICK_W), height: px(TICK_LEN), ..default() }, BackgroundColor(fg)));
            });
            dial.spawn((RepairPart::Flash, Node { border: UiRect::all(px(STROKE + 1.0)), ..disc(RADIUS + 1.0) }, BorderColor::all(col(k.good))));
            for i in 0..PIPS {
                dial.spawn((RepairPart::Pip(i), Node { position_type: PositionType::Absolute, left: px(RADIUS + PIP_GAP), top: px(-RADIUS + i as f32 * (PIP + PIP_GAP / 2.0)), width: px(PIP), height: px(PIP), border_radius: BorderRadius::MAX, display: Display::None, ..default() }, BackgroundColor(col_a(k.fg_title, 0.25))));
            }
        });
    });
}

#[allow(clippy::type_complexity)]
fn update_ring(
    play: Res<RepairPlay>,
    cfg: Res<crate::repair::RepairConfig>,
    look: Res<DisplayLook>,
    mut nodes: Query<(&RepairPart, &mut Node)>,
    mut turns: Query<(&RepairPart, &mut UiTransform)>,
    mut grads: Query<(&RepairPart, &mut BackgroundGradient)>,
    mut borders: Query<(&RepairPart, &mut BorderColor)>,
    mut arcs: Query<(&RepairPart, &mut BorderGradient)>,
    items: Res<crate::items::Items>,
    mut pips: Query<(&RepairPart, &mut BackgroundColor)>,
) {
    let k = &look.0.colours;
    let session = play.0.as_ref();
    let show = |on: bool| if on { Display::Flex } else { Display::None };
    let share = session.and_then(|p| items.reactor.as_ref().map(|r| part_of(r, p.device).share));
    // The minigame's parts show only while it runs (half condition or less); the progress arc
    // shows through any session; the pips only for a broken machine under the minigame.
    // The breach has no ring: its hold is the wrench on the wall, as the plain hold has none (r38).
    let game = session.is_some_and(|p| p.device != item_core::wire::RepairDevice::HullBreach) && minigame_on(share.unwrap_or(1.0), &cfg.0);
    let broken = session.is_some_and(|p| p.gated);
    let hits = session.map_or(0, |p| p.session.hits);
    for (part, mut n) in &mut nodes {
        match *part {
            RepairPart::Root => n.display = show(session.is_some()),
            RepairPart::Progress => n.display = show(share.is_some_and(|s| s > 0.0)),
            RepairPart::Zone(i) => n.display = show(game && session.is_some_and(|p| p.session.ring.zones.get(i).is_some_and(|z| z.width(&cfg.0) > 0.0))),
            RepairPart::Pointer => n.display = show(game),
            RepairPart::Flash => n.display = show(session.is_some_and(|p| p.flash.is_some())),
            RepairPart::Pip(_) => n.display = show(game && broken),
            _ => {}
        }
    }
    for (part, mut bg) in &mut pips {
        if let RepairPart::Pip(i) = *part {
            *bg = BackgroundColor(if hits as usize > i { col(k.fg_title) } else { col_a(k.fg_title, 0.25) });
        }
    }
    for (part, mut t) in &mut turns {
        if let (RepairPart::Pointer, Some(p)) = (*part, session) {
            t.rotation = Rot2::degrees(p.session.ring.pointer_deg as f32);
        }
        if let (RepairPart::Zone(i), Some(p)) = (*part, session)
            && let Some(z) = p.session.ring.zones.get(i)
        {
            t.rotation = Rot2::degrees(z.arc(&cfg.0).0 as f32);
        }
    }
    for (part, mut g) in &mut grads {
        match (*part, session) {
            (RepairPart::Zone(i), Some(p)) => {
                if let Some(z) = p.session.ring.zones.get(i) {
                    let w = z.width(&cfg.0).to_radians() as f32;
                    *g = conic(col(k.fg_title), 0.0, w);
                }
            }
            _ => {}
        }
    }
    for (part, mut g) in &mut arcs {
        if let (RepairPart::Progress, Some(_)) = (*part, session) {
            let p = share.unwrap_or(0.0).clamp(0.0, 1.0);
            let [r, g_, b] = crate::effects::progress_rgb(p);
            *g = BorderGradient::from(ConicGradient::new(UiPosition::CENTER, arc_stops(Color::srgb(r, g_, b), 0.0, arc_end(p))));
        }
    }
    for (part, mut b) in &mut borders {
        if let (RepairPart::Flash, Some(p)) = (*part, session)
            && let Some((press, age)) = p.flash
        {
            let colour = match press {
                Press::Hit => k.good,
                Press::Miss => k.bad,
            };
            let fade = (1.0 - age / cfg.0.flash_s.max(1e-6)).clamp(0.0, 1.0);
            *b = BorderColor::all(col_a(colour, fade));
        }
    }
}
