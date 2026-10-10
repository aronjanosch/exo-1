//! F10: the dev menu, a few cheats for playtests (dev builds only, `debug_assertions`). While it
//! is open, the digit keys run its commands. A new command is one line in `COMMANDS` and one arm
//! in `run`. The fixed step fills `DevMenu` (headless too, so scenarios check it); the window draws it.
use crate::controls::{Actions, Tap};
use crate::daynight::{DayClock, DayNightRes};
use crate::env::PlanetRes;
use crate::warp::SystemRes;
use crate::gameplay::{Gameplay, HOST};
use crate::ship::Ship;
use bevy::prelude::*;
use gameplay_core::{TrackId, WorldEvent};

pub fn plugin(app: &mut App) {
    app.init_resource::<DevMenu>();
    app.add_systems(FixedUpdate, update_dev.in_set(crate::phases::Fx::Effects));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Startup, spawn_dev);
    app.add_systems(Update, draw_dev.in_set(crate::phases::Frame::Hud));
}

/// The commands, in key order (1, 2, 3, ...).
pub const COMMANDS: [&str; 7] = ["Grant every licence", "+1000 credits", "Refill the boost", "Always day", "Always night", "Jump to day (cycle runs)", "Jump to night (cycle runs)"];

/// The digit taps, in the order of `COMMANDS`.
const KEYS: [Tap; 7] = [Tap::Dev1, Tap::Dev2, Tap::Dev3, Tap::Dev4, Tap::Dev5, Tap::Dev6, Tap::Dev7];

/// Money one press of the money command adds. TODO(initiator): the amount.
pub const MONEY: i64 = 1000;
/// Local hours the day and night commands go to. TODO(initiator): the hours.
pub const DAY_HOUR: f64 = 12.0;
pub const NIGHT_HOUR: f64 = 0.0;

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct DevMenu {
    pub open: bool,
    /// What the last command did, shown under the list.
    pub last: Option<String>,
}

/// Every licence the data has, earned by the host's client (the ones it has stay as they are).
pub fn grant_licences(gp: &mut Gameplay) -> usize {
    let tracks: Vec<TrackId> = gp.jobs_content.licences.values().map(|l| l.record.track.clone()).collect();
    let mut n = 0;
    for track in tracks {
        if !gp.progress.value(&gp.kernel, &track, Some(HOST)).is_some_and(|v| v >= 1) {
            gp.push_world(HOST, WorldEvent::TrackChanged { track, delta: 1, player: Some(HOST) });
            n += 1;
        }
    }
    n
}

/// Runs command `i`. `at_hour` gives the clock time of a local hour where the player is (`None`
/// in space or at a pole).
fn run(i: usize, gp: &mut Gameplay, ships: &mut Query<&mut Ship>, clock: &mut DayClock, at_hour: impl Fn(f64) -> Option<f64>) -> String {
    match i {
        0 => format!("licences granted: {}", grant_licences(gp)),
        1 => {
            gp.push_world(HOST, WorldEvent::TrackChanged { track: TrackId::new(gameplay_core::content::WALLET), delta: MONEY, player: None });
            format!("+{MONEY} credits")
        }
        2 => {
            for mut s in ships.iter_mut() {
                s.sc.drive.boost.charge = 1.0;
            }
            "boost full".into()
        }
        _ => {
            // 3 always day, 4 always night (the clock stands), 5 and 6 jump and the cycle runs.
            let (hour, stand) = match i {
                3 => (DAY_HOUR, true),
                4 => (NIGHT_HOUR, true),
                5 => (DAY_HOUR, false),
                _ => (NIGHT_HOUR, false),
            };
            let Some(t) = at_hour(hour) else { return "no day and night here".into() };
            clock.t = t;
            clock.rate = if stand { 0.0 } else { 1.0 };
            let what = if hour == DAY_HOUR { "day" } else { "night" };
            if stand { format!("always {what}") } else { format!("{what}, the cycle runs") }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_dev(
    mut actions: ResMut<Actions>,
    mut menu: ResMut<DevMenu>,
    mut gp: ResMut<Gameplay>,
    mut ships: Query<&mut Ship>,
    mut clock: ResMut<DayClock>,
    dn: Res<DayNightRes>,
    sys: Res<SystemRes>,
    planet: Res<PlanetRes>,
    players: Query<&crate::walker::Player>,
    positions: Query<&avian3d::prelude::Position, With<Ship>>,
) {
    // Taken in every build, so the keys never reach anything else; acted on in dev builds only.
    let flip = actions.take_tap(Tap::DevMenu);
    let pressed: Vec<usize> = KEYS.iter().enumerate().filter(|(_, t)| actions.take_tap(**t)).map(|(i, _)| i).collect();
    if !cfg!(debug_assertions) {
        return;
    }
    if flip {
        menu.open = !menu.open;
    }
    if menu.open {
        let ps = dn.0.planet(&sys.0.planet(planet.id).recipe).ok().map(|(p, _)| p.clone());
        let up = crate::daynight::viewer(&players, &positions).and_then(|p| (p - planet.centre).try_normalize());
        let to_star = daynight_core::to_star(sys.0.star.position(), planet.centre);
        for i in pressed {
            let now = clock.t;
            let at_hour = |h: f64| ps.as_ref().zip(up).and_then(|(ps, up)| ps.time_for_hour(up, to_star, h, now));
            menu.last = Some(run(i, &mut gp, &mut ships, &mut clock, at_hour));
        }
    }
}

#[derive(Component)]
struct DevRoot;
#[derive(Component)]
struct DevText;

fn spawn_dev(mut commands: Commands) {
    commands
        .spawn((
            DevRoot,
            Node { position_type: PositionType::Absolute, left: px(16), top: px(120), padding: UiRect::all(px(12)), display: Display::None, ..default() },
            BackgroundColor(Color::srgba(0.18, 0.04, 0.12, 0.85)),
        ))
        .with_children(|c| {
            c.spawn((DevText, Text::new(""), TextFont { font_size: FontSize::Px(15.0), ..default() }, TextColor(Color::srgb(1.0, 0.86, 0.94))));
        });
}

fn draw_dev(menu: Res<DevMenu>, mut root: Query<&mut Node, With<DevRoot>>, mut text: Query<&mut Text, With<DevText>>) {
    if !menu.is_changed() {
        return;
    }
    if let Ok(mut n) = root.single_mut() {
        n.display = if menu.open { Display::Flex } else { Display::None };
    }
    if let Ok(mut t) = text.single_mut() {
        let mut s = String::from("DEV   (F10 closes)\n");
        for (i, c) in COMMANDS.iter().enumerate() {
            s.push_str(&format!("\n{}   {c}", i + 1));
        }
        if let Some(l) = &menu.last {
            s.push_str(&format!("\n\n{l}"));
        }
        t.0 = s;
    }
}
