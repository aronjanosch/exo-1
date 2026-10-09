//! Main menu and pause menu (#30): Host, Join (address, slot), Settings, Quit; Escape in game
//! pauses (the world keeps running: the others are still flying). Players start here; the
//! `--net-host` / `--net-connect` flags stay for scenarios and bots and skip the menu, as does every
//! scripted or headless run. Simple look, few elements.
use crate::controls::{input_name, Bindings, Slot};
use crate::net_live::{Net, NetConfig};
use crate::settings::{Settings, SettingsDir, FOV_RANGE, MOUSE_RANGE};
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Main,
    Join,
    /// Settings, and where Back leads.
    Settings(Back),
    Paused,
    /// In game, no menu.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Back {
    Main,
    Paused,
}

#[derive(Resource, Debug)]
pub struct Menu {
    pub screen: Screen,
    pub address: String,
    pub slot: u32,
    /// Waiting for a key for this slot.
    pub rebinding: Option<Slot>,
    pub message: Option<String>,
}

impl Default for Menu {
    fn default() -> Self {
        Menu { screen: Screen::Main, address: "127.0.0.1:17441".into(), slot: 2, rebinding: None, message: None }
    }
}

impl Menu {
    pub fn open(&self) -> bool {
        self.screen != Screen::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    Host,
    JoinScreen,
    Join,
    Slot(i32),
    Settings,
    Back,
    Resume,
    Quit,
    Mouse(f64),
    Fov(f64),
    Volume(f64),
    Rebind(Slot),
}

#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct MenuButton(Action);

/// How a session starts: host on slot 1, or join an address on a slot.
#[derive(Clone, Debug)]
pub enum Session {
    Host { port: u16 },
    Join { address: String, slot: u32 },
}

/// Starts the network from a menu choice: the `Net` resource, and the own walker and ship moved to
/// the slot's spawn (players of one session start 20 m apart). Shared with the `menu` scenario.
pub fn start_session(w: &mut World, s: &Session) -> Result<(), String> {
    let mut args: Vec<(String, String)> = Vec::new();
    match s {
        Session::Host { port } => {
            args.push(("--net-host".into(), String::new()));
            args.push(("--port".into(), port.to_string()));
        }
        Session::Join { address, slot } => {
            let addr: std::net::SocketAddr = address.trim().parse().map_err(|_| format!("`{address}` is not ip:port"))?;
            args.push(("--net-connect".into(), addr.to_string()));
            args.push(("--slot".into(), slot.to_string()));
        }
    }
    let cfg = NetConfig::parse(&args).ok_or("no session")?;
    let net = Net::try_new(cfg.clone(), &w.resource::<crate::warp::SystemRes>().0)?;
    w.insert_resource(net);
    let offset = (cfg.slot as f64 - 1.0) * 20.0;
    if (w.resource::<crate::SpawnOffset>().0 - offset).abs() > 1e-9 {
        w.insert_resource(crate::SpawnOffset(offset));
        let old: Vec<Entity> = w.query_filtered::<Entity, Or<(With<crate::walker::Player>, With<crate::ship::Ship>)>>().iter(w).collect();
        for e in old {
            w.entity_mut(e).despawn();
        }
        let planet = w.resource::<crate::env::PlanetRes>().clone();
        let tuning = w.resource::<crate::tuning::Tuning>().clone();
        let mut commands = w.commands();
        crate::walker::spawn_player(&mut commands, &planet, &tuning.walker, offset);
        crate::ship::spawn_ship(&mut commands, &planet, &tuning.ship, bevy::math::DVec3::Y, offset);
        w.flush();
    }
    println!("menu: {s:?} started, slot {}", cfg.slot);
    Ok(())
}

fn setup(mut commands: Commands) {
    commands.spawn((
        MenuRoot,
        Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, flex_direction: FlexDirection::Column, justify_content: JustifyContent::Center, align_items: AlignItems::Center, row_gap: px(10), ..default() },
        BackgroundColor(Color::srgba(0.02, 0.03, 0.08, 0.72)),
        GlobalZIndex(10),
    ));
}

fn label(c: &mut ChildSpawnerCommands, text: impl Into<String>, size: f32) {
    c.spawn((Text::new(text), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(Color::srgb(0.9, 0.93, 1.0))));
}

fn button(c: &mut ChildSpawnerCommands, text: impl Into<String>, action: Action, width: f32) {
    c.spawn((
        Button,
        MenuButton(action),
        Node { width: px(width), padding: UiRect::axes(px(12), px(6)), justify_content: JustifyContent::Center, border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(4)), ..default() },
        BorderColor::all(Color::srgba(0.8, 0.9, 1.0, 0.5)),
        BackgroundColor(Color::srgba(0.15, 0.2, 0.35, 0.9)),
    ))
    .with_children(|b| label(b, text, 18.0));
}

/// A row: name, value, then -/+ buttons.
fn stepper(c: &mut ChildSpawnerCommands, name: &str, value: String, down: Action, up: Action) {
    c.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|r| {
        r.spawn(Node { width: px(200), ..default() }).with_children(|n| label(n, name, 16.0));
        r.spawn(Node { width: px(70), ..default() }).with_children(|n| label(n, value, 16.0));
        button(r, "-", down, 40.0);
        button(r, "+", up, 40.0);
    });
}

#[allow(clippy::too_many_arguments)]
fn rebuild(mut commands: Commands, menu: Res<Menu>, settings: Res<Settings>, bindings: Res<Bindings>, root: Query<Entity, With<MenuRoot>>, mut vis: Query<&mut Visibility, With<MenuRoot>>) {
    if !menu.is_changed() && !settings.is_changed() && !bindings.is_changed() {
        return;
    }
    let Ok(root) = root.single() else { return };
    if let Ok(mut v) = vis.single_mut() {
        *v = if menu.open() { Visibility::Inherited } else { Visibility::Hidden };
    }
    commands.entity(root).despawn_children();
    if !menu.open() {
        return;
    }
    commands.entity(root).with_children(|c| {
        match menu.screen {
            Screen::Main => {
                label(c, "EXO-1", 40.0);
                button(c, "Host", Action::Host, 260.0);
                button(c, "Join", Action::JoinScreen, 260.0);
                button(c, "Settings", Action::Settings, 260.0);
                button(c, "Quit", Action::Quit, 260.0);
            }
            Screen::Join => {
                label(c, "Join", 32.0);
                label(c, format!("Address (type): {}_", menu.address), 18.0);
                stepper(c, "Slot", menu.slot.to_string(), Action::Slot(-1), Action::Slot(1));
                button(c, "Join", Action::Join, 260.0);
                button(c, "Back", Action::Back, 260.0);
            }
            Screen::Paused => {
                label(c, "Paused", 32.0);
                button(c, "Resume", Action::Resume, 260.0);
                button(c, "Settings", Action::Settings, 260.0);
                button(c, "Quit", Action::Quit, 260.0);
            }
            Screen::Settings(_) => {
                label(c, "Settings", 32.0);
                stepper(c, "Mouse sensitivity", format!("{:.2}", settings.mouse_sensitivity), Action::Mouse(-0.1), Action::Mouse(0.1));
                stepper(c, "Field of view", format!("{:.0}", settings.fov_deg), Action::Fov(-5.0), Action::Fov(5.0));
                stepper(c, "Volume", format!("{:.0} %", settings.volume * 100.0), Action::Volume(-0.1), Action::Volume(0.1));
                label(c, if menu.rebinding.is_some() { "Press a key (Escape cancels)" } else { "Keys: click one to rebind" }, 16.0);
                c.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, width: px(900), column_gap: px(6), row_gap: px(4), justify_content: JustifyContent::Center, ..default() }).with_children(|g| {
                    let mut b = bindings.clone();
                    for slot in Slot::all() {
                        let keys: Vec<String> = b.slot_mut(slot).iter().filter(|i| matches!(i, crate::controls::Input::Key(_))).map(|i| input_name(*i)).collect();
                        let text = if menu.rebinding == Some(slot) { format!("{}: ...", slot.label()) } else { format!("{}: {}", slot.label(), keys.first().cloned().unwrap_or_else(|| "-".into())) };
                        button(g, text, Action::Rebind(slot), 210.0);
                    }
                });
                button(c, "Back", Action::Back, 260.0);
            }
            Screen::None => {}
        }
        if let Some(m) = &menu.message {
            label(c, m.clone(), 16.0);
        }
    });
}

fn clicks(mut commands: Commands, q: Query<(&Interaction, &MenuButton), Changed<Interaction>>) {
    for (i, b) in &q {
        if *i == Interaction::Pressed {
            let a = b.0;
            commands.queue(move |w: &mut World| act(w, a));
        }
    }
}

fn act(w: &mut World, a: Action) {
    let dir = w.resource::<SettingsDir>().0.clone();
    let save = |w: &mut World| {
        if let Err(e) = crate::settings::save_settings(&dir, w.resource::<Settings>()) {
            w.resource_mut::<Menu>().message = Some(format!("could not save settings: {e}"));
        }
    };
    let screen = w.resource::<Menu>().screen;
    match a {
        Action::Host | Action::Join => {
            let session = if a == Action::Host {
                Session::Host { port: 17441 }
            } else {
                let m = w.resource::<Menu>();
                Session::Join { address: m.address.clone(), slot: m.slot }
            };
            match start_session(w, &session) {
                Ok(()) => {
                    let mut m = w.resource_mut::<Menu>();
                    m.screen = Screen::None;
                    m.message = None;
                }
                Err(e) => w.resource_mut::<Menu>().message = Some(e),
            }
        }
        Action::JoinScreen => w.resource_mut::<Menu>().screen = Screen::Join,
        Action::Slot(d) => {
            let mut m = w.resource_mut::<Menu>();
            m.slot = (m.slot as i32 + d).clamp(2, 8) as u32;
        }
        Action::Settings => w.resource_mut::<Menu>().screen = Screen::Settings(if screen == Screen::Paused { Back::Paused } else { Back::Main }),
        Action::Back => {
            let mut m = w.resource_mut::<Menu>();
            m.rebinding = None;
            m.screen = match screen {
                Screen::Settings(Back::Paused) => Screen::Paused,
                _ => Screen::Main,
            };
        }
        Action::Resume => w.resource_mut::<Menu>().screen = Screen::None,
        Action::Quit => {
            // From the pause menu back to the main menu would need the session torn down; quit.
            w.write_message(AppExit::Success);
        }
        Action::Mouse(d) => {
            let mut s = w.resource_mut::<Settings>();
            s.mouse_sensitivity = ((s.mouse_sensitivity + d) * 100.0).round().clamp(MOUSE_RANGE.0 * 100.0, MOUSE_RANGE.1 * 100.0) / 100.0;
            save(w);
        }
        Action::Fov(d) => {
            let mut s = w.resource_mut::<Settings>();
            s.fov_deg = (s.fov_deg + d).clamp(FOV_RANGE.0, FOV_RANGE.1);
            save(w);
        }
        Action::Volume(d) => {
            let mut s = w.resource_mut::<Settings>();
            s.volume = ((s.volume + d) * 10.0).round().clamp(0.0, 10.0) / 10.0;
            save(w);
        }
        Action::Rebind(slot) => w.resource_mut::<Menu>().rebinding = Some(slot),
    }
}

/// Escape pauses and goes back; typing into the address; the key for a rebind.
#[allow(clippy::too_many_arguments)]
fn keys(
    mut menu: ResMut<Menu>,
    keys: Res<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    mut bindings: ResMut<Bindings>,
    dir: Res<SettingsDir>,
    mut cursor: Query<&mut CursorOptions>,
) {
    let text: Vec<KeyboardInput> = typed.read().cloned().collect();
    if let Some(slot) = menu.rebinding {
        if keys.just_pressed(KeyCode::Escape) {
            menu.rebinding = None;
            return;
        }
        if let Some(k) = keys.get_just_pressed().copied().find(|k| crate::controls::key_from_name(&format!("{k:?}")).is_some()) {
            bindings.rebind(slot, k);
            menu.rebinding = None;
            menu.message = crate::settings::save_bindings(&dir.0, &bindings).err().map(|e| format!("could not save bindings: {e}"));
        }
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        menu.screen = match menu.screen {
            Screen::None => Screen::Paused,
            Screen::Paused => Screen::None,
            Screen::Settings(Back::Paused) => Screen::Paused,
            Screen::Join | Screen::Settings(Back::Main) => Screen::Main,
            Screen::Main => Screen::Main,
        };
    }
    if menu.screen == Screen::Join {
        for ev in text.iter().filter(|e| e.state.is_pressed()) {
            match &ev.logical_key {
                Key::Backspace => {
                    menu.address.pop();
                }
                Key::Character(c) if menu.address.len() < 40 => {
                    for ch in c.chars().filter(|ch| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == ':') {
                        menu.address.push(ch);
                    }
                }
                _ => {}
            }
        }
    }
    // A menu needs the pointer.
    if menu.open()
        && let Ok(mut cur) = cursor.single_mut()
        && cur.grab_mode != CursorGrabMode::None
    {
        cur.grab_mode = CursorGrabMode::None;
        cur.visible = true;
    }
}

/// Windowed player runs: the menus.
pub fn plugin(app: &mut App) {
    app.init_resource::<Menu>();
    app.add_systems(Startup, setup);
    app.add_systems(Update, (keys, clicks, rebuild).chain().before(crate::controls::read_input).in_set(crate::phases::Frame::Input));
}
