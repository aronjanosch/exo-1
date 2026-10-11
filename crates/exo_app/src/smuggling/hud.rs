//! The round-6 HUD in the window: the goal line, heat stars, the tower's line with its countdown
//! bar, the radio box, the phone, the scan prompt and the kettle's needle. Everything here only
//! copies `SmuggleHud` (filled by the fixed step, headless too) into `bevy_ui` nodes, in the
//! look of `ui_theme`; the punches (a new heat star, a changed goal) come from `feedback_view`.
//! TODO(initiator): the look of all of it.
use super::{PhoneTab, SmuggleHud};
use crate::feedback_view::Punches;
use crate::ui_theme as th;
use bevy::prelude::*;

#[derive(Component)]
struct GoalText;
#[derive(Component)]
struct HeatRow;
#[derive(Component)]
struct HeatStar(u8);
#[derive(Component)]
struct ClearText;
#[derive(Component)]
struct ClearBar;
#[derive(Component)]
struct ClearFill;
#[derive(Component)]
struct RadioText;
#[derive(Component)]
struct Phone;
#[derive(Component)]
struct PhoneTabs;
#[derive(Component)]
struct PhoneRows;
#[derive(Component)]
struct PhoneHint;
#[derive(Component)]
struct ScanText;
#[derive(Component)]
struct BrewBar;
#[derive(Component)]
struct BrewNeedle;
#[derive(Component)]
struct BrewSweet;

/// Seconds per character of a new radio line (typewriter). TODO(initiator).
const TYPE_S: f64 = 0.018;
/// Heat stars shown.
const STARS: u8 = 5;

fn font(size: f32) -> TextFont {
    TextFont { font_size: FontSize::Px(size), ..default() }
}

fn shadow() -> TextShadow {
    TextShadow { offset: Vec2::splat(1.5), color: th::SHADOW }
}

fn spawn_hud(mut commands: Commands) {
    // Goal, heat and the tower's line: top left, under the flight readouts.
    commands
        .spawn(Node { position_type: PositionType::Absolute, left: px(th::MARGIN), top: px(96), flex_direction: FlexDirection::Column, row_gap: px(6), ..default() })
        .with_children(|c| {
            c.spawn((GoalText, Text::new(""), font(th::BODY), TextColor(th::GOLD), shadow(), UiTransform::IDENTITY));
            c.spawn((HeatRow, Node { flex_direction: FlexDirection::Row, column_gap: px(4), align_items: AlignItems::Center, display: Display::None, ..default() }, UiTransform::IDENTITY)).with_children(|r| {
                r.spawn((Text::new("HEAT"), font(th::SMALL), TextColor(th::BAD), shadow(), Node { margin: UiRect::right(px(4)), ..default() }));
                for i in 0..STARS {
                    r.spawn((HeatStar(i), Node { width: px(14), height: px(14), border: UiRect::all(px(2)), ..default() }, BorderColor::all(th::BAD), BackgroundColor(Color::NONE), UiTransform::IDENTITY));
                }
            });
            c.spawn((ClearText, Text::new(""), font(th::BODY), TextColor(th::INFO), shadow()));
            c.spawn((ClearBar, Node { width: px(240), height: px(6), display: Display::None, ..default() }, BackgroundColor(th::PANEL))).with_children(|b| {
                b.spawn((ClearFill, Node { width: percent(100), height: percent(100), ..default() }, BackgroundColor(th::INFO)));
            });
        });
    // Radio box: bottom left, the tower, the patrol and the buyers talk here.
    commands.spawn((
        RadioText,
        Text::new(""),
        font(th::BODY),
        TextColor(th::GOOD),
        BackgroundColor(th::PANEL),
        Node { position_type: PositionType::Absolute, left: px(th::MARGIN), bottom: px(th::MARGIN), max_width: px(520), padding: UiRect::all(px(10)), display: Display::None, ..default() },
    ));
    // The phone: right side, tabs as tags, one node per row so the selection is a bar.
    commands
        .spawn((
            Phone,
            BackgroundColor(Color::srgba(0.07, 0.06, 0.12, 0.94)),
            BorderColor::all(th::faded(th::GOLD, 0.5)),
            Node {
                position_type: PositionType::Absolute,
                right: px(th::MARGIN),
                top: percent(16),
                width: px(380),
                max_height: percent(66),
                padding: UiRect::all(px(12)),
                border: UiRect::all(px(1)),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                overflow: Overflow::clip(),
                display: Display::None,
                ..default()
            },
            UiTransform::IDENTITY,
        ))
        .with_children(|p| {
            p.spawn((PhoneTabs, Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), ..default() }));
            p.spawn((PhoneRows, Node { flex_direction: FlexDirection::Column, row_gap: px(2), ..default() }));
            p.spawn((PhoneHint, Text::new(""), font(th::SMALL - 2.0), TextColor(th::DIM)));
        });
    // The scan prompt: centre, below the stamps and the prompt line, clear of the phone.
    commands.spawn((
        ScanText,
        Text::new(""),
        font(th::LARGE),
        TextColor(th::GOLD),
        BackgroundColor(th::PANEL_ALERT),
        BorderColor::all(th::BAD),
        Node { position_type: PositionType::Absolute, left: percent(31), top: percent(62), width: percent(36), padding: UiRect::all(px(12)), border: UiRect::all(px(2)), display: Display::None, ..default() },
    ));
    // The kettle's needle: a bar with the sweet zone and the needle, bottom centre.
    commands
        .spawn((BrewBar, Node { position_type: PositionType::Absolute, left: percent(35), bottom: px(120), width: percent(30), height: px(22), display: Display::None, ..default() }, BackgroundColor(th::PANEL)))
        .with_children(|c| {
            c.spawn((BrewSweet, Node { position_type: PositionType::Absolute, left: percent(40), width: percent(20), height: percent(100), ..default() }, BackgroundColor(th::faded(th::GOOD, 0.8))));
            c.spawn((BrewNeedle, Node { position_type: PositionType::Absolute, left: percent(0), width: px(4), height: percent(140), top: px(-4), ..default() }, BackgroundColor(Color::WHITE)));
        });
}

fn set_text(t: &mut Text, n: &mut Node, s: &str) {
    let d = if s.is_empty() { Display::None } else { Display::Flex };
    if n.display != d {
        n.display = d;
    }
    if t.0 != s {
        t.0 = s.to_string();
    }
}

/// The radio lines, the newest typed in letter by letter.
fn radio_typed(hud: &SmuggleHud) -> String {
    let recent: Vec<&super::RadioLine> = hud.radio.iter().rev().take_while(|l| hud.clock - l.at < super::RADIO_S).take(3).collect();
    let n = recent.len();
    recent
        .iter()
        .rev()
        .enumerate()
        .map(|(i, l)| {
            let full = format!("[{}] {}", l.who, l.text);
            if i + 1 == n {
                let shown = ((hud.clock - l.at) / TYPE_S).max(0.0) as usize;
                full.chars().take(shown.max(l.who.chars().count() + 2)).collect()
            } else {
                full
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_hud(
    time: Res<Time>,
    hud: Res<SmuggleHud>,
    punches: Res<Punches>,
    mut goal: Query<(&mut Text, &mut UiTransform, &mut TextColor), (With<GoalText>, Without<RadioText>, Without<ScanText>, Without<ClearText>)>,
    mut heat_row: Query<(&mut Node, &mut UiTransform), (With<HeatRow>, Without<GoalText>, Without<HeatStar>)>,
    mut stars: Query<(&HeatStar, &mut BackgroundColor, &mut UiTransform), (Without<HeatRow>, Without<GoalText>)>,
    mut radio: Query<(&mut Text, &mut Node), (With<RadioText>, Without<ScanText>, Without<HeatRow>, Without<ClearText>, Without<GoalText>)>,
    mut scan: Query<(&mut Text, &mut Node), (With<ScanText>, Without<RadioText>, Without<HeatRow>, Without<ClearText>, Without<GoalText>)>,
) {
    let now = time.elapsed_secs_f64();
    if let Ok((mut t, mut tr, mut c)) = goal.single_mut() {
        if t.0 != hud.goal_line {
            t.0 = hud.goal_line.clone();
        }
        // A changed goal pulses: bigger and white, settling back to gold.
        let k = punches.goal.map_or(1.0, |b| ((now - b) / 0.5) as f32).clamp(0.0, 1.0);
        tr.scale = Vec2::splat(1.0 + 0.25 * (1.0 - th::ease_out(k)));
        c.0 = th::GOLD.mix(&Color::WHITE, 1.0 - k);
    }
    let shown = hud.heat_stars > 0 || hud.heat_points >= 1.0;
    if let Ok((mut n, mut tr)) = heat_row.single_mut() {
        let d = if shown { Display::Flex } else { Display::None };
        if n.display != d {
            n.display = d;
        }
        let k = punches.heat.map_or(1.0, |(b, _)| ((now - b) / 0.4) as f32).clamp(0.0, 1.0);
        tr.translation = Val2::px(6.0 * (1.0 - k) * (k * 40.0).sin(), 0.0);
    }
    let (punch_k, rose) = punches.heat.map_or((1.0, false), |(b, up)| ((((now - b) / 0.35) as f32).clamp(0.0, 1.0), up));
    for (s, mut bg, mut tr) in &mut stars {
        let lit = s.0 < hud.heat_stars;
        bg.0 = if lit { th::BAD } else { Color::NONE };
        // The newest star (the highest lit one, or the one just gone) punches.
        let newest = if rose { s.0 + 1 == hud.heat_stars } else { s.0 == hud.heat_stars };
        tr.scale = Vec2::splat(if newest { 1.0 + 0.8 * (1.0 - th::ease_out_back(punch_k)).max(0.0) } else { 1.0 });
    }
    if let Ok((mut t, mut n)) = radio.single_mut() {
        set_text(&mut t, &mut n, &radio_typed(&hud));
    }
    if let Ok((mut t, mut n)) = scan.single_mut() {
        set_text(&mut t, &mut n, &hud.scan);
    }
}

#[allow(clippy::type_complexity)]
fn draw_clearance(hud: Res<SmuggleHud>, mut text: Query<&mut Text, With<ClearText>>, mut bar: Query<&mut Node, (With<ClearBar>, Without<ClearFill>)>, mut fill: Query<(&mut Node, &mut BackgroundColor), (With<ClearFill>, Without<ClearBar>)>) {
    if let Ok(mut t) = text.single_mut()
        && t.0 != hud.clearance
    {
        t.0 = hud.clearance.clone();
    }
    if let Ok(mut n) = bar.single_mut() {
        let d = if hud.bar.is_some() { Display::Flex } else { Display::None };
        if n.display != d {
            n.display = d;
        }
    }
    if let (Some(b), Ok((mut n, mut c))) = (hud.bar, fill.single_mut()) {
        n.width = percent((b.left * 100.0) as f32);
        c.0 = th::tone(b.tone);
    }
}

/// The phone: tabs as tags, rows as lines, the selected one on a bar. Rebuilt when it changes.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_phone(
    mut commands: Commands,
    time: Res<Time>,
    hud: Res<SmuggleHud>,
    mut last: Local<Option<super::PhoneView>>,
    mut opened: Local<f64>,
    mut phone: Query<(&mut Node, &mut UiTransform), With<Phone>>,
    tabs: Query<Entity, With<PhoneTabs>>,
    rows: Query<Entity, With<PhoneRows>>,
    mut hint: Query<&mut Text, With<PhoneHint>>,
) {
    let now = time.elapsed_secs_f64();
    let p = &hud.phone;
    let Ok((mut n, mut tr)) = phone.single_mut() else { return };
    let d = if p.open { Display::Flex } else { Display::None };
    if n.display != d {
        n.display = d;
        if p.open {
            *opened = now;
        }
    }
    // Slides in from the right.
    let k = (((now - *opened) / 0.14) as f32).clamp(0.0, 1.0);
    tr.translation = Val2::px(60.0 * (1.0 - th::ease_out(k)), 0.0);
    if last.as_ref() == Some(p) || !p.open {
        if !p.open {
            *last = None;
        }
        return;
    }
    *last = Some(p.clone());
    if let Ok(e) = tabs.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|c| {
            for t in PhoneTab::ALL {
                let on = t == p.tab;
                c.spawn((Node { padding: UiRect::axes(px(6), px(2)), ..default() }, BackgroundColor(if on { th::GOLD } else { th::faded(th::INK, 0.08) }))).with_children(|tag| {
                    tag.spawn((Text::new(t.title()), font(th::SMALL), TextColor(if on { Color::BLACK } else { th::DIM })));
                });
            }
        });
    }
    if let Ok(e) = rows.single() {
        commands.entity(e).despawn_children();
        commands.entity(e).with_children(|c| {
            if p.rows.is_empty() {
                c.spawn((Text::new("(nothing here yet)"), font(th::BODY), TextColor(th::DIM)));
            }
            for (i, r) in p.rows.iter().enumerate() {
                let on = i == p.selected;
                c.spawn((Node { padding: UiRect::axes(px(6), px(3)), border: UiRect::left(px(3)), ..default() }, BorderColor::all(if on { th::GOLD } else { Color::NONE }), BackgroundColor(if on { th::faded(th::GOLD, 0.16) } else { Color::NONE })))
                    .with_children(|row| {
                        row.spawn((Text::new(r.clone()), font(th::BODY), TextColor(if on { th::INK } else { th::faded(th::INK, 0.75) })));
                    });
            }
        });
    }
    if let Ok(mut t) = hint.single_mut()
        && t.0 != p.hint
    {
        t.0 = p.hint.clone();
    }
}

#[allow(clippy::type_complexity)]
fn draw_brew(hud: Res<SmuggleHud>, mut bar: Query<&mut Node, (With<BrewBar>, Without<BrewNeedle>, Without<BrewSweet>)>, mut needle: Query<&mut Node, (With<BrewNeedle>, Without<BrewBar>, Without<BrewSweet>)>, mut sweet: Query<&mut Node, (With<BrewSweet>, Without<BrewBar>, Without<BrewNeedle>)>) {
    let Ok(mut b) = bar.single_mut() else { return };
    let d = if hud.brew.is_some() { Display::Flex } else { Display::None };
    if b.display != d {
        b.display = d;
    }
    let Some(br) = hud.brew else { return };
    if let Ok(mut n) = needle.single_mut() {
        n.left = percent((br.needle * 100.0) as f32);
    }
    if let Ok(mut s) = sweet.single_mut() {
        s.left = percent(((br.sweet - br.width) * 100.0) as f32);
        s.width = percent((br.width * 200.0) as f32);
    }
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Startup, spawn_hud);
    app.add_systems(Update, (draw_hud, draw_brew, draw_clearance, draw_phone).in_set(crate::phases::Frame::Hud));
    app.add_systems(Update, (super::glue::kettle_view, super::glue::kettle_fx).chain().in_set(crate::phases::Frame::World));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smuggling::RadioLine;

    #[test]
    fn the_newest_radio_line_types_in_and_older_ones_stand() {
        let mut hud = SmuggleHud { clock: 10.0, ..Default::default() };
        hud.radio.push(RadioLine { who: "Tower".into(), text: "old line".into(), at: 5.0 });
        hud.radio.push(RadioLine { who: "Tower".into(), text: "a new line that types".into(), at: 10.0 });
        let s = radio_typed(&hud);
        assert!(s.starts_with("[Tower] old line\n[Tower]"), "{s}");
        assert!(!s.contains("types"), "not typed yet: {s}");
        hud.clock = 12.0;
        assert!(radio_typed(&hud).ends_with("a new line that types"));
    }
}
