use crate::controls::{Actions, Tap};
use crate::daynight::{DayClock, DayNightRes};
use crate::env::{from_v3, PlanetRes};
use crate::warp::SystemRes;
use crate::gameplay::{Gameplay, HOST};
use crate::ship::Ship;
use avian3d::prelude::{AngularVelocity, LinearVelocity, Position, Rotation};
use bevy::math::{DMat3, DQuat, DVec3};
use bevy::prelude::*;
use planet_core::recipe::SiteCategory;
use gameplay_core::{TrackId, WorldEvent};

pub fn plugin(app: &mut App) {
    app.init_resource::<DevMenu>().init_resource::<Wear>();
    app.add_systems(FixedUpdate, (update_dev, jump_to_settlement, toggle_noclip, toggle_god, toggle_wear).chain().in_set(crate::phases::Fx::Effects));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Startup, spawn_dev);
    app.add_systems(Update, draw_dev.in_set(crate::phases::Frame::Hud));
}

pub const COMMANDS: [&str; 11] = ["Grant every licence", "+1000 credits", "Refill the boost", "Always day", "Always night", "Jump to day (cycle runs)", "Jump to night (cycle runs)", "Ship to the next settlement", "Noclip on/off", "God mode on/off", "Wear on/off"];

const KEYS: [Tap; 11] = [Tap::Dev1, Tap::Dev2, Tap::Dev3, Tap::Dev4, Tap::Dev5, Tap::Dev6, Tap::Dev7, Tap::Dev8, Tap::Dev9, Tap::Dev0, Tap::DevMinus];
/// The keys' names in the menu, in the order of `KEYS`.
const KEY_NAMES: [&str; 11] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-"];

/// Normal wear of the ship's machines (`reactor_core::EngineRoom::set_wear`): off by default, so the
/// ship stands ready while nobody plays with it; the dev menu turns it on to test the repairs.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Wear(pub bool);

/// Money one press of the money command adds. TODO(initiator): the amount.
pub const MONEY: i64 = 1000;
/// Local hours the day and night commands go to. TODO(initiator): the hours.
pub const DAY_HOUR: f64 = 12.0;
pub const NIGHT_HOUR: f64 = 0.0;
/// The settlement jump puts the ship this high above the settlement's ground (m), stopped and
/// level. Settlements closer than `JUMP_SKIP_M` (along the ground) are skipped, so pressing again
/// goes on to another one. TODO(initiator): values.
pub const JUMP_ABOVE_M: f64 = 150.0;
pub const JUMP_SKIP_M: f64 = 1000.0;

#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct DevMenu {
    pub open: bool,
    pub last: Option<String>,
    pub jump: bool,
    pub noclip: bool,
    pub god: bool,
    pub wear: bool,
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
    positions: Query<&Position, With<Ship>>,
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
            if i == 7 {
                menu.jump = true;
                continue;
            }
            if i == 8 {
                menu.noclip = true;
                continue;
            }
            if i == 9 {
                menu.god = true;
                continue;
            }
            if i == 10 {
                menu.wear = true;
                continue;
            }
            let now = clock.t;
            let at_hour = |h: f64| ps.as_ref().zip(up).and_then(|(ps, up)| ps.time_for_hour(up, to_star, h, now));
            menu.last = Some(run(i, &mut gp, &mut ships, &mut clock, at_hour));
        }
    }
}

pub fn toggle_noclip(mut menu: ResMut<DevMenu>, mut players: Query<&mut crate::walker::Player>) {
    if !std::mem::take(&mut menu.noclip) {
        return;
    }
    for mut pl in &mut players {
        pl.noclip = !pl.noclip;
        pl.w.halt();
        menu.last = Some(if pl.noclip { "noclip on".into() } else { "noclip off".into() });
    }
}

/// Flips `GodMode` (`health_step` keeps every body at full health while it is there).
pub fn toggle_god(mut menu: ResMut<DevMenu>, god: Option<Res<crate::health::GodMode>>, mut commands: Commands) {
    if !std::mem::take(&mut menu.god) {
        return;
    }
    if god.is_some() {
        commands.remove_resource::<crate::health::GodMode>();
        menu.last = Some("god mode off".into());
    } else {
        commands.insert_resource(crate::health::GodMode);
        menu.last = Some("god mode on".into());
    }
}

pub fn jump_to_settlement(mut menu: ResMut<DevMenu>, planet: Res<PlanetRes>, mut ships: Query<(&mut Position, &mut Rotation, &mut LinearVelocity, &mut AngularVelocity), With<Ship>>) {
    if !menu.jump {
        return;
    }
    menu.jump = false;
    let Ok((mut pos, mut rot, mut lin, mut ang)) = ships.single_mut() else { return };
    let here = (pos.0 - planet.centre).normalize_or_zero();
    let along = |up: DVec3| up.dot(here).clamp(-1.0, 1.0).acos() * planet.radius;
    let next = planet.pgen.sites.iter().filter(|s| s.category == SiteCategory::Settlement).map(|s| (s, from_v3(s.dir))).filter(|(_, up)| along(*up) > JUMP_SKIP_M).min_by(|a, b| along(a.1).total_cmp(&along(b.1)));
    let Some((site, up)) = next else {
        menu.last = Some("no settlement on this planet".into());
        return;
    };
    pos.0 = planet.centre + up * (planet.radius + site.ground_m + JUMP_ABOVE_M);
    rot.0 = level(up);
    lin.0 = DVec3::ZERO;
    ang.0 = DVec3::ZERO;
    menu.last = Some(format!("{}: {:.1} km away, now {JUMP_ABOVE_M:.0} m above its pad", site.id, along(up) / 1000.0));
}

/// A pose with `up` as the ship's up and its nose (-Z) towards local north (east at a pole).
fn level(up: DVec3) -> DQuat {
    let east = DVec3::Y.cross(up).try_normalize().unwrap_or(DVec3::X);
    let nose = up.cross(east);
    DQuat::from_mat3(&DMat3::from_cols(up.cross(-nose), up, -nose))
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

/// Flips `Wear`.
pub fn toggle_wear(mut menu: ResMut<DevMenu>, mut wear: ResMut<Wear>) {
    if !std::mem::take(&mut menu.wear) {
        return;
    }
    wear.0 = !wear.0;
    menu.last = Some(if wear.0 { "wear on".into() } else { "wear off".into() });
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
        for (k, c) in KEY_NAMES.iter().zip(COMMANDS) {
            s.push_str(&format!("\n{k}   {c}"));
        }
        if let Some(l) = &menu.last {
            s.push_str(&format!("\n\n{l}"));
        }
        t.0 = s;
    }
}
