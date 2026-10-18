//! Game over (#254): when every player is dead at once, the host shows GAME OVER on every client
//! for `SCREEN_S` seconds, then reloads the last save. With no save, the dev area starts cold
//! again (a fresh engine room: Reset, as the dev menu does).
//! - Barotrauma ends the round when no character is alive: `BarotraumaClient/ClientSource/GameSession/
//!   GameModes/MultiPlayerCampaign.cs:306` ("crewdead" when every `CrewManager` character `IsDead`),
//!   and the server's end-of-round timer for a down crew, `BarotraumaServer/ServerSource/Networking/GameServer.cs:520-557`.
//!   Our reload replaces the campaign's end; the decision is `health_core::crew_wiped`.
//! - Not ported: the campaign's "respawn with new characters" path. TODO(initiator): a wipe with a
//!   save older than the last one, or a choice of save.
use bevy::prelude::*;
use gameplay_core::save::Envelope;

use crate::crew_hud::{col, hud_caps, HudFonts};
use crate::displays::DisplayLook;
use crate::health::HealthState;
use crate::items::{Items, Role};
use crate::repair::RepairSessions;
use crate::savefile::{read_slot, SaveDir, SLOT};
use item_core::wire::ReactorAsk;

/// How long GAME OVER stays up (s). TODO(initiator): the length.
pub const SCREEN_S: f64 = 5.0;

/// Present while GAME OVER is up. `left` counts down (s) on the host and on every client.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct GameOver {
    pub left: f64,
}

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, game_over_step.after(crate::health::health_step));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, update_screen.in_set(crate::phases::Frame::Hud));
    app.add_systems(Startup, spawn_screen);
}

/// Host: a wiped crew starts the screen and tells the clients. Client: the host's word starts it.
/// Then the countdown; at its end the host loads the save (`finish`).
fn game_over_step(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    if world.resource::<Items>().role == Role::Client {
        let told = std::mem::take(&mut world.resource_mut::<Items>().game_over_seen);
        if told && !world.contains_resource::<GameOver>() {
            world.insert_resource(GameOver { left: SCREEN_S });
        }
        count_down(world, dt, false);
        return;
    }
    if world.contains_resource::<GameOver>() {
        count_down(world, dt, true);
        return;
    }
    let wiped = world.get_resource::<HealthState>().is_some_and(|st| health_core::crew_wiped(st.bodies.values().map(|b| b.health.state())));
    if wiped {
        world.insert_resource(GameOver { left: SCREEN_S });
        world.resource_mut::<Items>().send_game_over();
        info!("game over: the crew is dead");
    }
}

fn count_down(world: &mut World, dt: f64, host: bool) {
    let Some(mut g) = world.get_resource_mut::<GameOver>() else { return };
    g.left -= dt;
    if g.left > 0.0 {
        return;
    }
    world.remove_resource::<GameOver>();
    if host {
        finish(world);
    }
}

/// The save comes back (its gameplay, jobs, crates and engine room). No save: the dev area starts
/// cold again. Either way the bodies start at full health with the start skills.
fn finish(world: &mut World) {
    let dir = world.resource::<SaveDir>().0.clone();
    let loaded = match read_slot(&dir, SLOT) {
        Ok(Some(text)) => Envelope::from_json(&text).map_err(|e| e.to_string()).and_then(|env| crate::savefile::load(world, &env).map_err(|e| e.to_string())),
        Ok(None) => Err("no save yet".to_string()),
        Err(e) => Err(e.to_string()),
    };
    match loaded {
        Ok(()) => info!("game over: the last save is back"),
        Err(e) => {
            println!("game over: no save loaded ({e}), the dev area starts fresh");
            let mut items = world.resource_mut::<Items>();
            items.ask_reactor(ReactorAsk::Reset);
        }
    }
    world.resource_mut::<HealthState>().new_crew();
    if let Some(mut s) = world.get_resource_mut::<RepairSessions>() {
        s.sessions.clear();
    }
}

#[derive(Component)]
struct OverRoot;

fn spawn_screen(mut commands: Commands, fonts: Option<Res<HudFonts>>, look: Res<DisplayLook>) {
    let Some(fonts) = fonts else { return };
    commands
        .spawn((OverRoot, GlobalZIndex(5), Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: px(0), bottom: px(0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, display: Display::None, ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.97))))
        .with_children(|root| {
            root.spawn(hud_caps(&fonts.display, "GAME OVER", 72.0, col(look.0.colours.fg_title), &look.0.hud, 0.1));
        });
}

fn update_screen(over: Option<Res<GameOver>>, mut roots: Query<&mut Node, With<OverRoot>>) {
    let shown = if over.is_some() { Display::Flex } else { Display::None };
    for mut n in &mut roots {
        if n.display != shown {
            n.display = shown;
        }
    }
}
