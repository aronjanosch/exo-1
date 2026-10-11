//! The round-6 HUD in the window: the goal line, heat stars, the radio box, the phone, the scan
//! prompt and the kettle's needle. Everything here only copies `SmuggleHud` (filled by the fixed
//! step, headless too) into `bevy_ui` nodes. TODO(initiator): the look of all of it.
use super::SmuggleHud;
use bevy::prelude::*;

#[derive(Component)]
struct GoalText;
#[derive(Component)]
struct HeatText;
#[derive(Component)]
struct ClearText;
#[derive(Component)]
struct RadioText;
#[derive(Component)]
struct PhoneText;
#[derive(Component)]
struct ScanText;
#[derive(Component)]
struct BrewBar;
#[derive(Component)]
struct BrewNeedle;
#[derive(Component)]
struct BrewSweet;

const INK: Color = Color::srgb(0.95, 0.95, 0.85);
const PANEL: Color = Color::srgba(0.04, 0.05, 0.09, 0.78);

fn spawn_hud(mut commands: Commands) {
    // Goal line and heat: top left, under nothing else (the job line is top right).
    commands
        .spawn(Node { position_type: PositionType::Absolute, left: px(16), top: px(96), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() })
        .with_children(|c| {
            c.spawn((GoalText, Text::new(""), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(Color::srgb(1.0, 0.85, 0.45))));
            c.spawn((HeatText, Text::new(""), TextFont { font_size: FontSize::Px(20.0), ..default() }, TextColor(Color::srgb(1.0, 0.35, 0.3))));
            c.spawn((ClearText, Text::new(""), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(Color::srgb(0.55, 0.9, 1.0))));
        });
    // Radio box: bottom left, the tower and the buyers talk here.
    commands.spawn((
        RadioText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(Color::srgb(0.6, 1.0, 0.75)),
        BackgroundColor(PANEL),
        Node { position_type: PositionType::Absolute, left: px(16), bottom: px(16), max_width: px(520), padding: UiRect::all(px(10)), display: Display::None, ..default() },
    ));
    // The phone: right side, middle.
    commands.spawn((
        PhoneText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(INK),
        BackgroundColor(Color::srgba(0.08, 0.06, 0.14, 0.92)),
        Node { position_type: PositionType::Absolute, right: px(16), top: percent(18), width: px(430), padding: UiRect::all(px(14)), display: Display::None, ..default() },
    ));
    // The scan prompt: centre, above the middle.
    commands.spawn((
        ScanText,
        Text::new(""),
        TextFont { font_size: FontSize::Px(20.0), ..default() },
        TextColor(Color::srgb(1.0, 0.9, 0.5)),
        BackgroundColor(Color::srgba(0.25, 0.02, 0.02, 0.85)),
        Node { position_type: PositionType::Absolute, left: percent(30), top: percent(22), width: percent(40), padding: UiRect::all(px(14)), display: Display::None, ..default() },
    ));
    // The kettle's needle: a bar with the sweet zone and the needle, bottom centre.
    commands
        .spawn((BrewBar, Node { position_type: PositionType::Absolute, left: percent(35), bottom: px(120), width: percent(30), height: px(22), display: Display::None, ..default() }, BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.85))))
        .with_children(|c| {
            c.spawn((BrewSweet, Node { position_type: PositionType::Absolute, left: percent(40), width: percent(20), height: percent(100), ..default() }, BackgroundColor(Color::srgba(0.3, 0.9, 0.4, 0.8))));
            c.spawn((BrewNeedle, Node { position_type: PositionType::Absolute, left: percent(0), width: px(4), height: percent(140), top: px(-4), ..default() }, BackgroundColor(Color::srgb(1.0, 1.0, 1.0))));
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

#[allow(clippy::type_complexity)]
fn draw_hud(
    hud: Res<SmuggleHud>,
    mut goal: Query<&mut Text, (With<GoalText>, Without<HeatText>, Without<RadioText>, Without<PhoneText>, Without<ScanText>)>,
    mut heat: Query<&mut Text, (With<HeatText>, Without<GoalText>, Without<RadioText>, Without<PhoneText>, Without<ScanText>)>,
    mut radio: Query<(&mut Text, &mut Node), (With<RadioText>, Without<PhoneText>, Without<ScanText>)>,
    mut phone: Query<(&mut Text, &mut Node), (With<PhoneText>, Without<RadioText>, Without<ScanText>)>,
    mut scan: Query<(&mut Text, &mut Node), (With<ScanText>, Without<RadioText>, Without<PhoneText>)>,
) {
    if let Ok(mut t) = goal.single_mut() {
        if t.0 != hud.goal_line {
            t.0 = hud.goal_line.clone();
        }
    }
    if let Ok(mut t) = heat.single_mut() {
        let s = if hud.heat_stars == 0 && hud.heat_points < 1.0 { String::new() } else { format!("HEAT [{}{}]", "*".repeat(hud.heat_stars as usize), "-".repeat(5 - hud.heat_stars.min(5) as usize)) };
        if t.0 != s {
            t.0 = s;
        }
    }
    if let Ok((mut t, mut n)) = radio.single_mut() {
        set_text(&mut t, &mut n, &hud.radio_text());
    }
    if let Ok((mut t, mut n)) = phone.single_mut() {
        set_text(&mut t, &mut n, &hud.phone_text());
    }
    if let Ok((mut t, mut n)) = scan.single_mut() {
        set_text(&mut t, &mut n, &hud.scan);
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

fn draw_clearance(hud: Res<SmuggleHud>, mut q: Query<&mut Text, With<ClearText>>) {
    if let Ok(mut t) = q.single_mut()
        && t.0 != hud.clearance
    {
        t.0 = hud.clearance.clone();
    }
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Startup, spawn_hud);
    app.add_systems(Update, (draw_hud, draw_brew, draw_clearance).in_set(crate::phases::Frame::Hud));
}
