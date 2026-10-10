//! F1: the keys that apply right now, as a panel (on foot, in the suit, in the ship). The keys come from the live `Bindings`, so a rebind shows at once. The fixed
//! step fills `HelpPanel` (headless too, so scenarios check it); the window draws it.
//! TODO(initiator): the words, the order and the look; `KEYS.md` is the full list.
use crate::controls::{Actions, Axis, Bindings, Button, Slot, Tap};
use crate::walker::Player;
use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;

pub fn plugin(app: &mut App) {
    app.init_resource::<HelpPanel>();
    app.add_systems(FixedUpdate, update_help.in_set(crate::phases::Fx::Effects));
}

pub fn window_plugin(app: &mut App) {
    app.add_systems(Startup, spawn_help);
    app.add_systems(Update, draw_help.in_set(crate::phases::Frame::Hud));
}

/// What the player is doing, for the keys that apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelpContext {
    /// Walking, also in a cabin.
    Foot,
    /// Outside a ship in space.
    Suit,
    /// Seated in the ship.
    Ship,
}

impl HelpContext {
    pub fn title(self) -> &'static str {
        match self {
            HelpContext::Foot => "On foot",
            HelpContext::Suit => "Suit",
            HelpContext::Ship => "Ship",
        }
    }
}

/// The panel's content: shown or not, the context's title and one line per action (keys, what
/// it does).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct HelpPanel {
    pub shown: bool,
    pub title: String,
    pub lines: Vec<(String, &'static str)>,
}

/// One help line: the slots whose keys are shown together, and what they do.
type Entry = (&'static [Slot], &'static str);

const MOVE: [Slot; 4] = [Slot::Negative(Axis::MoveZ), Slot::Negative(Axis::MoveX), Slot::Positive(Axis::MoveZ), Slot::Positive(Axis::MoveX)];
const UP_DOWN: [Slot; 2] = [Slot::Positive(Axis::MoveY), Slot::Negative(Axis::MoveY)];
const ROLL: [Slot; 2] = [Slot::Positive(Axis::Roll), Slot::Negative(Axis::Roll)];

const FOOT: &[Entry] = &[
    (&MOVE, "Walk"),
    (&[Slot::Button(Button::Run)], "Run"),
    (&[Slot::Button(Button::Jump)], "Jump"),
    (&[Slot::Tap(Tap::Interact)], "Interact: sit, crates, counters"),
    (&[Slot::Tap(Tap::Throw)], "Throw the held crate"),
    (&[Slot::Tap(Tap::Lag)], "Cabin gravity (landed ship)"),
    (&[Slot::Tap(Tap::Map)], "Map"),
    (&[Slot::Tap(Tap::TrackJob)], "Track a job"),
    (&[Slot::Tap(Tap::AbandonJob)], "Drop the tracked job (twice)"),
];

const SUIT: &[Entry] = &[
    (&MOVE, "Move"),
    (&UP_DOWN, "Up / down"),
    (&ROLL, "Roll"),
    (&[Slot::Button(Button::Boost)], "Boost"),
    (&[Slot::Button(Button::Brake)], "Brake to rest"),
    (&[Slot::Tap(Tap::Interact)], "Interact"),
];

const SHIP: &[Entry] = &[
    (&MOVE, "Thrust: forward, strafe"),
    (&UP_DOWN, "Thrust up / down"),
    (&ROLL, "Roll"),
    (&[Slot::Button(Button::Boost)], "Boost"),
    (&[Slot::Button(Button::Brake)], "Brake"),
    (&[Slot::Tap(Tap::Decoupled)], "Coupled / decoupled"),
    (&[Slot::Tap(Tap::LandingMode)], "Landing mode"),
];

const SHIP_SC: &[Entry] = &[
    (&[Slot::Tap(Tap::HoverAssist)], "Gravity compensation"),
    (&[Slot::Tap(Tap::TurnCap)], "G-safe"),
    (&[Slot::Tap(Tap::MasterMode)], "Master mode SCM / NAV"),
    (&[Slot::Tap(Tap::Comstab)], "Comstab"),
    (&[Slot::Tap(Tap::ProximityAssist)], "Proximity assist"),
    (&[Slot::Tap(Tap::WindComp)], "Wind compensation"),
    (&[Slot::Tap(Tap::LimiterUp), Slot::Tap(Tap::LimiterDown)], "Speed limiter up / down"),
];

const SHIP_END: &[Entry] = &[
    (&[Slot::Tap(Tap::WarpTarget)], "Quantum target"),
    (&[Slot::Tap(Tap::Warp)], "Quantum jump (hold: exit)"),
    (&[Slot::Tap(Tap::Interact)], "Stand up"),
];

const ALWAYS: &[Entry] = &[(&[Slot::Tap(Tap::Help)], "This help"), (&[Slot::Tap(Tap::DebugHud)], "Debug lines")];

/// The entries for a context, in order.
pub fn entries(ctx: HelpContext) -> Vec<Entry> {
    let mut v: Vec<Entry> = Vec::new();
    match ctx {
        HelpContext::Foot => v.extend_from_slice(FOOT),
        HelpContext::Suit => v.extend_from_slice(SUIT),
        HelpContext::Ship => {
            v.extend_from_slice(SHIP);
            v.extend_from_slice(SHIP_SC);
            v.extend_from_slice(SHIP_END);
        }
    }
    v.extend_from_slice(ALWAYS);
    v
}

/// A key as players read it: `W`, `Shift`, `Ctrl`, `Page Up`, `F1`.
pub fn key_label(k: KeyCode) -> String {
    let n = format!("{k:?}");
    let n = n.strip_prefix("Key").or_else(|| n.strip_prefix("Digit")).unwrap_or(&n).to_string();
    match n.as_str() {
        "ShiftLeft" | "ShiftRight" => "Shift".into(),
        "ControlLeft" | "ControlRight" => "Ctrl".into(),
        "AltLeft" | "AltRight" => "Alt".into(),
        "PageUp" => "Page Up".into(),
        "PageDown" => "Page Down".into(),
        "Escape" => "Esc".into(),
        _ => n,
    }
}

/// The lines for a context with the keys of `b`: every slot's first keyboard key, joined; a slot
/// without one shows `-`.
pub fn lines(ctx: HelpContext, b: &Bindings) -> Vec<(String, &'static str)> {
    entries(ctx)
        .into_iter()
        .map(|(slots, what)| {
            let keys: Vec<String> = slots.iter().map(|s| b.slot_keys(*s).first().map_or_else(|| "-".to_string(), |k| key_label(*k))).collect();
            (keys.join(" "), what)
        })
        .collect()
}

/// F1 flips the panel; the context follows the player every step.
pub fn update_help(mut actions: ResMut<Actions>, bindings: Res<Bindings>, mut help: ResMut<HelpPanel>, players: Query<&Player>) {
    let flip = actions.take_tap(Tap::Help);
    let Ok(pl) = players.single() else { return };
    let ctx = if pl.seated {
        HelpContext::Ship
    } else if pl.body.is_some() {
        HelpContext::Suit
    } else {
        HelpContext::Foot
    };
    let new = HelpPanel { shown: help.shown != flip, title: ctx.title().into(), lines: lines(ctx, &bindings) };
    if *help != new {
        *help = new;
    }
}

#[derive(Component)]
struct HelpRoot;
#[derive(Component)]
struct HelpText;

/// TODO(initiator): place, size and colours.
fn spawn_help(mut commands: Commands) {
    commands
        .spawn((
            HelpRoot,
            Node { position_type: PositionType::Absolute, right: px(16), top: px(120), padding: UiRect::all(px(12)), display: Display::None, ..default() },
            BackgroundColor(Color::srgba(0.04, 0.06, 0.08, 0.82)),
        ))
        .with_children(|c| {
            c.spawn((HelpText, Text::new(""), TextFont { font_size: FontSize::Px(15.0), ..default() }, TextColor(Color::srgb(0.92, 0.95, 0.97))));
        });
}

fn draw_help(help: Res<HelpPanel>, mut root: Query<&mut Node, With<HelpRoot>>, mut text: Query<&mut Text, With<HelpText>>) {
    if !help.is_changed() {
        return;
    }
    if let Ok(mut n) = root.single_mut() {
        n.display = if help.shown { Display::Flex } else { Display::None };
    }
    if let Ok(mut t) = text.single_mut() {
        let width = help.lines.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
        let mut s = format!("{}   (F1 closes)\n", help.title);
        for (k, what) in &help.lines {
            s.push_str(&format!("\n{k:<width$}   {what}"));
        }
        t.0 = s;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_read_as_players_say_them() {
        assert_eq!(key_label(KeyCode::KeyW), "W");
        assert_eq!(key_label(KeyCode::ShiftLeft), "Shift");
        assert_eq!(key_label(KeyCode::ControlLeft), "Ctrl");
        assert_eq!(key_label(KeyCode::PageUp), "Page Up");
        assert_eq!(key_label(KeyCode::F1), "F1");
        assert_eq!(key_label(KeyCode::Digit3), "3");
    }

    #[test]
    fn each_context_shows_its_own_keys() {
        let b = Bindings::default();
        let foot = lines(HelpContext::Foot, &b);
        assert_eq!(foot[0], ("W A S D".to_string(), "Walk"));
        assert!(foot.iter().any(|(k, w)| k == "Shift" && *w == "Run"));
        assert!(!foot.iter().any(|(_, w)| *w == "Boost"), "on foot there is no boost");
        let sc = lines(HelpContext::Ship, &b);
        assert!(sc.iter().any(|(k, w)| k == "H" && *w == "Gravity compensation"));
        assert!(sc.iter().any(|(k, w)| k == "Page Up Page Down" && w.starts_with("Speed limiter")));
        for ctx in [HelpContext::Foot, HelpContext::Suit, HelpContext::Ship] {
            assert!(lines(ctx, &b).iter().any(|(k, w)| k == "F1" && *w == "This help"), "{ctx:?}");
        }
    }

    #[test]
    fn a_rebind_shows_in_the_help() {
        let mut b = Bindings::default();
        b.rebind(Slot::Button(Button::Run), KeyCode::KeyZ);
        assert!(lines(HelpContext::Foot, &b).iter().any(|(k, w)| k == "Z" && *w == "Run"));
    }
}
