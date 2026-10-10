//! Main menu and pause menu (#30): Host, Join (address, slot), Settings, Quit; Escape in game
//! pauses (the world keeps running: the others are still flying). Players start here; the
//! `--net-host` / `--net-connect` flags stay for scenarios and bots and skip the menu, as does every
//! scripted or headless run. Simple look, few elements.
use crate::controls::{input_name, Bindings, Slot};
use crate::net_live::{Net, NetConfig};
use crate::settings::{Settings, SettingsDir, FOV_RANGE, MOUSE_RANGE};
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
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
    Edit(FirstPersonSetting),
    CameraShake(f64),
    Volume(f64),
    Sound,
    Rebind(Slot),
}

#[derive(Component)]
struct MenuRoot;
#[derive(Component)]
struct MenuButton(Action);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FirstPersonSetting {
    Mouse,
    Fov,
}

impl FirstPersonSetting {
    fn range(self) -> (f64, f64) {
        match self { Self::Mouse => MOUSE_RANGE, Self::Fov => FOV_RANGE }
    }

    fn value(self, settings: &Settings) -> f64 {
        match self { Self::Mouse => settings.mouse_sensitivity, Self::Fov => settings.fov_deg }
    }

    fn display(self, settings: &Settings) -> String {
        match self { Self::Mouse => format!("{:.2}", self.value(settings)), Self::Fov => format!("{:.1}", self.value(settings)) }
    }

    fn set(self, settings: &mut Settings, value: f64) {
        let (lo, hi) = self.range();
        let precision = if self == Self::Mouse { 100.0 } else { 10.0 };
        let value = (value.clamp(lo, hi) * precision).round() / precision;
        match self { Self::Mouse => settings.mouse_sensitivity = value, Self::Fov => settings.fov_deg = value }
    }
}

#[derive(Debug)]
struct NumberEdit {
    setting: FirstPersonSetting,
    text: String,
    caret: usize,
    selected: bool,
}

impl NumberEdit {
    fn new(setting: FirstPersonSetting, settings: &Settings) -> Self {
        let text = setting.display(settings);
        Self { setting, caret: text.len(), text, selected: true }
    }

    fn insert(&mut self, text: &str) {
        let text: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
        if text.is_empty() { return; }
        if self.selected { self.text.clear(); self.caret = 0; self.selected = false; }
        if self.text.len() + text.len() <= 16 {
            self.text.insert_str(self.caret, &text);
            self.caret += text.len();
        }
    }

    fn value(&self) -> Option<f64> {
        let value: f64 = self.text.replace(',', ".").parse().ok()?;
        let (lo, hi) = self.setting.range();
        (value.is_finite() && (lo..=hi).contains(&value)).then_some(value)
    }

    fn display(&self) -> String {
        if self.selected { format!("[{}]", self.text) } else { format!("{}|{}", &self.text[..self.caret], &self.text[self.caret..]) }
    }
}

#[derive(Resource, Default)]
struct SettingsInput {
    editing: Option<NumberEdit>,
    dragging: Option<FirstPersonSetting>,
}

#[derive(Component)]
struct SettingsSlider(FirstPersonSetting);
#[derive(Component)]
struct SliderThumb(FirstPersonSetting);
#[derive(Component)]
enum SettingsText {
    Number(FirstPersonSetting),
    HorizontalFov,
    Shake,
    Volume,
    Sound,
}

fn persist(settings: &Settings, dir: &SettingsDir, menu: &mut Menu, scripted: bool) {
    if scripted { return; }
    let message = crate::settings::save_settings(&dir.0, settings).err().map(|e| format!("could not save settings: {e}"));
    if menu.message != message { menu.message = message; }
}

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
        crate::ship::spawn_ship(&mut commands, &planet, &tuning, bevy::math::DVec3::Y, offset);
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
    .with_children(|b| label(b, text, if matches!(action, Action::Rebind(_)) { 14.0 } else { 18.0 }));
}

/// A row: name, value, then -/+ buttons.
fn stepper(c: &mut ChildSpawnerCommands, name: &str, value: String, down: Action, up: Action) {
    c.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|r| {
        r.spawn(Node { width: px(200), ..default() }).with_children(|n| label(n, name, 16.0));
        r.spawn(Node { width: px(70), ..default() }).with_children(|n| {
            let mut text = n.spawn((Text::new(value), TextFont { font_size: FontSize::Px(16.0), ..default() }));
            match down {
                Action::CameraShake(_) => { text.insert(SettingsText::Shake); }
                Action::Volume(_) => { text.insert(SettingsText::Volume); }
                _ => {}
            }
        });
        button(r, "-", down, 40.0);
        button(r, "+", up, 40.0);
    });
}

fn slider(c: &mut ChildSpawnerCommands, name: &str, setting: FirstPersonSetting, settings: &Settings) {
    c.spawn(Node { column_gap: px(12), align_items: AlignItems::Center, ..default() }).with_children(|r| {
        r.spawn(Node { width: px(180), ..default() }).with_children(|n| label(n, name, 16.0));
        r.spawn((
            Button, SettingsSlider(setting), RelativeCursorPosition::default(),
            Node { width: px(280), height: px(28), ..default() },
            BackgroundColor(Color::srgba(0.15, 0.2, 0.35, 0.9)),
        )).with_children(|track| {
            track.spawn((SliderThumb(setting), Node { position_type: PositionType::Absolute, width: px(4), height: percent(100), ..default() }, BackgroundColor(Color::srgb(0.6, 0.8, 1.0))));
        });
        r.spawn((
            Button, MenuButton(Action::Edit(setting)),
            Node { width: px(110), padding: UiRect::all(px(6)), justify_content: JustifyContent::Center, border: UiRect::all(px(1)), ..default() },
            BorderColor::all(Color::srgba(0.8, 0.9, 1.0, 0.5)), BackgroundColor(Color::srgba(0.15, 0.2, 0.35, 0.9)),
        )).with_children(|b| {
            b.spawn((SettingsText::Number(setting), Text::new(setting.display(settings)), TextFont { font_size: FontSize::Px(18.0), ..default() }));
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn rebuild(mut commands: Commands, menu: Res<Menu>, settings: Res<Settings>, bindings: Res<Bindings>, root: Query<Entity, With<MenuRoot>>, mut vis: Query<&mut Visibility, With<MenuRoot>>) {
    if !menu.is_changed() && !bindings.is_changed() {
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
                label(c, "First-person view (on foot, also inside a ship)", 16.0);
                slider(c, "Mouse sensitivity", FirstPersonSetting::Mouse, &settings);
                label(c, "0.10 - 10.00   |   m_yaw / m_pitch: 0.022 degrees per count", 14.0);
                slider(c, "Vertical FOV", FirstPersonSetting::Fov, &settings);
                c.spawn((SettingsText::HorizontalFov, Text::new(""), TextFont { font_size: FontSize::Px(14.0), ..default() }));
                label(c, "40.0 - 90.0 degrees   |   Click a number to type; Enter confirms, Escape cancels", 14.0);
                stepper(c, "Camera shake", format!("{:.0} %", settings.camera_shake * 100.0), Action::CameraShake(-0.1), Action::CameraShake(0.1));
                stepper(c, "Volume", format!("{:.0} %", settings.volume * 100.0), Action::Volume(-0.1), Action::Volume(0.1));
                c.spawn((Button, MenuButton(Action::Sound), Node { width: px(260), padding: UiRect::all(px(6)), justify_content: JustifyContent::Center, ..default() }, BackgroundColor(Color::srgba(0.15, 0.2, 0.35, 0.9)))).with_children(|b| {
                    b.spawn((SettingsText::Sound, Text::new(""), TextFont { font_size: FontSize::Px(18.0), ..default() }));
                });
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
    let save = |w: &mut World| {
        let scripted = w.get_resource::<crate::controls::Controls>().is_some_and(|c| c.scripted);
        w.resource_scope(|w, mut menu: Mut<Menu>| persist(w.resource::<Settings>(), w.resource::<SettingsDir>(), &mut menu, scripted));
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
        Action::Edit(setting) => {
            let editing = NumberEdit::new(setting, w.resource::<Settings>());
            w.resource_mut::<SettingsInput>().editing = Some(editing);
            w.resource_mut::<Menu>().rebinding = None;
        }
        Action::CameraShake(d) => {
            let mut s = w.resource_mut::<Settings>();
            s.camera_shake = ((s.camera_shake + d) * 10.0).round().clamp(0.0, 10.0) / 10.0;
            save(w);
        }
        Action::Volume(d) => {
            let mut s = w.resource_mut::<Settings>();
            s.volume = ((s.volume + d) * 10.0).round().clamp(0.0, 10.0) / 10.0;
            save(w);
        }
        Action::Sound => {
            let mut s = w.resource_mut::<Settings>();
            s.sound = !s.sound;
            save(w);
        }
        Action::Rebind(slot) => {
            w.resource_mut::<SettingsInput>().editing = None;
            w.resource_mut::<Menu>().rebinding = Some(slot);
        }
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
    mut input: ResMut<SettingsInput>,
    mut settings: ResMut<Settings>,
    controls: Option<Res<crate::controls::Controls>>,
) {
    let text: Vec<KeyboardInput> = typed.read().cloned().collect();
    if let Some(edit) = &mut input.editing {
        edit_number(edit, &text, &keys);
        if keys.just_pressed(KeyCode::Escape) {
            input.editing = None;
            menu.message = None;
        } else if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
            if let Some(value) = edit.value() {
                edit.setting.set(&mut settings, value);
                persist(&settings, &dir, &mut menu, controls.is_some_and(|c| c.scripted));
                input.editing = None;
            } else {
                let (lo, hi) = edit.setting.range();
                menu.message = Some(format!("Enter a number from {lo} to {hi}"));
            }
        }
        return;
    }
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

fn edit_number(edit: &mut NumberEdit, events: &[KeyboardInput], keys: &ButtonInput<KeyCode>) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    for ev in events.iter().filter(|e| e.state.is_pressed()) {
        match &ev.logical_key {
            Key::Character(c) if ctrl && c.eq_ignore_ascii_case("a") => edit.selected = true,
            Key::Character(c) if !ctrl => edit.insert(c),
            Key::Backspace | Key::Delete => {
                if edit.selected {
                    edit.text.clear(); edit.caret = 0; edit.selected = false;
                } else if ev.logical_key == Key::Backspace && edit.caret > 0 {
                    edit.caret -= 1; edit.text.remove(edit.caret);
                } else if ev.logical_key == Key::Delete && edit.caret < edit.text.len() {
                    edit.text.remove(edit.caret);
                }
            }
            Key::ArrowLeft => { edit.caret = if edit.selected { 0 } else { edit.caret.saturating_sub(1) }; edit.selected = false; }
            Key::ArrowRight => { edit.caret = if edit.selected { edit.text.len() } else { (edit.caret + 1).min(edit.text.len()) }; edit.selected = false; }
            Key::Home => { edit.caret = 0; edit.selected = false; }
            Key::End => { edit.caret = edit.text.len(); edit.selected = false; }
            _ => {}
        }
    }
}

/// Keep the UI tree alive during dragging: relative cursor positions are computed by Bevy.
fn sliders(
    mut input: ResMut<SettingsInput>, mut settings: ResMut<Settings>, mut menu: ResMut<Menu>,
    mouse: Res<ButtonInput<MouseButton>>, dir: Res<SettingsDir>,
    q: Query<(&Interaction, &SettingsSlider, &RelativeCursorPosition)>,
    controls: Option<Res<crate::controls::Controls>>,
) {
    let scripted = controls.is_some_and(|c| c.scripted);
    if !matches!(menu.screen, Screen::Settings(_)) {
        input.editing = None;
        if input.dragging.take().is_some() { persist(&settings, &dir, &mut menu, scripted); }
        return;
    }
    if mouse.just_pressed(MouseButton::Left) {
        for (interaction, slider, _) in &q {
            if *interaction == Interaction::Pressed {
                input.dragging = Some(slider.0);
                input.editing = None;
                if menu.rebinding.is_some() { menu.rebinding = None; }
            }
        }
    }
    if let Some(setting) = input.dragging {
        for (_, slider, cursor) in &q {
            if slider.0 == setting && let Some(pos) = cursor.normalized {
                let (lo, hi) = setting.range();
                let value = lo + (pos.x as f64 + 0.5).clamp(0.0, 1.0) * (hi - lo);
                if (value - setting.value(&settings)).abs() > 1e-9 { setting.set(&mut settings, value); }
            }
        }
        if !mouse.pressed(MouseButton::Left) {
            input.dragging = None;
            persist(&settings, &dir, &mut menu, scripted);
        }
    }
}

fn refresh_settings(
    settings: Res<Settings>, input: Res<SettingsInput>,
    mut texts: Query<(&SettingsText, &mut Text)>, mut thumbs: Query<(&SliderThumb, &mut Node)>,
) {
    for (kind, mut text) in &mut texts {
        let value = match kind {
            SettingsText::Number(setting) => input.editing.as_ref().filter(|e| e.setting == *setting).map_or_else(|| setting.display(&settings), NumberEdit::display),
            SettingsText::HorizontalFov => format!("Horizontal FOV: 4:3 {:.1} deg   |   16:9 {:.1} deg   |   21:9 {:.1} deg", crate::settings::horizontal_fov(settings.fov_deg, 4.0 / 3.0), crate::settings::horizontal_fov(settings.fov_deg, 16.0 / 9.0), crate::settings::horizontal_fov(settings.fov_deg, 21.0 / 9.0)),
            SettingsText::Shake => format!("{:.0} %", settings.camera_shake * 100.0),
            SettingsText::Volume => format!("{:.0} %", settings.volume * 100.0),
            SettingsText::Sound => if settings.sound { "Sound: on" } else { "Sound: off" }.into(),
        };
        if text.0 != value { text.0 = value; }
    }
    for (thumb, mut node) in &mut thumbs {
        let (lo, hi) = thumb.0.range();
        node.left = px(((thumb.0.value(&settings) - lo) / (hi - lo) * 276.0) as f32);
    }
}

/// Windowed player runs: the menus.
pub fn plugin(app: &mut App) {
    app.init_resource::<Menu>().init_resource::<SettingsInput>();
    app.add_systems(Startup, setup);
    // Menus consume Escape/clicks before gameplay reads this frame's input.
    app.add_systems(RunFixedMainLoop, (keys, clicks, sliders, rebuild, refresh_settings).chain().before(crate::controls::read_input).in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_app(name: &str) -> App {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/menu-tests").join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = App::new();
        app.init_resource::<Settings>().init_resource::<Bindings>()
            .init_resource::<ButtonInput<KeyCode>>().init_resource::<ButtonInput<MouseButton>>()
            .add_message::<KeyboardInput>().insert_resource(SettingsDir(dir));
        plugin(&mut app);
        app.world_mut().resource_mut::<Menu>().screen = Screen::Settings(Back::Main);
        app.update();
        app
    }

    fn type_text(app: &mut App, text: &str) {
        app.world_mut().write_message(KeyboardInput { key_code: KeyCode::Digit0, logical_key: Key::Character(text.into()), state: bevy::input::ButtonState::Pressed, text: Some(text.into()), repeat: false, window: Entity::PLACEHOLDER });
        app.update();
    }

    fn click_number(app: &mut App, setting: FirstPersonSetting) {
        let mut q = app.world_mut().query::<(&MenuButton, &mut Interaction)>();
        for (b, mut i) in q.iter_mut(app.world_mut()) {
            if b.0 == Action::Edit(setting) { *i = Interaction::Pressed; }
        }
        app.update();
    }

    fn press_key(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
    }

    #[test]
    fn number_click_selects_and_enter_applies_and_saves() {
        let mut app = settings_app("number");
        click_number(&mut app, FirstPersonSetting::Mouse);
        assert!(app.world().resource::<SettingsInput>().editing.as_ref().unwrap().selected);
        type_text(&mut app, "2,35");
        assert_eq!(app.world().resource::<Settings>().mouse_sensitivity, 1.0, "unconfirmed text does not affect the game");
        press_key(&mut app, KeyCode::Enter);
        assert_eq!(app.world().resource::<Settings>().mouse_sensitivity, 2.35);
        assert!(app.world().resource::<SettingsInput>().editing.is_none());
        let dir = &app.world().resource::<SettingsDir>().0;
        assert_eq!(crate::settings::load(dir).0.mouse_sensitivity, 2.35);
        click_number(&mut app, FirstPersonSetting::Fov);
        type_text(&mut app, "83.7");
        press_key(&mut app, KeyCode::NumpadEnter);
        assert_eq!(app.world().resource::<Settings>().fov_deg, 83.7);
        let mut q = app.world_mut().query::<(&SettingsText, &Text)>();
        let hfov = q.iter(app.world()).find(|(kind, _)| matches!(kind, SettingsText::HorizontalFov)).unwrap().1;
        assert!(hfov.0.contains(&format!("16:9 {:.1} deg", crate::settings::horizontal_fov(83.7, 16.0 / 9.0))));
    }

    #[test]
    fn invalid_numbers_are_refused_and_escape_only_cancels_edit() {
        let mut app = settings_app("cancel");
        click_number(&mut app, FirstPersonSetting::Fov);
        type_text(&mut app, "91");
        press_key(&mut app, KeyCode::Enter);
        assert_eq!(app.world().resource::<Settings>().fov_deg, 75.0);
        assert!(app.world().resource::<Menu>().message.is_some());
        assert!(app.world().resource::<SettingsInput>().editing.is_some());
        press_key(&mut app, KeyCode::Escape);
        assert!(app.world().resource::<SettingsInput>().editing.is_none());
        assert_eq!(app.world().resource::<Menu>().screen, Screen::Settings(Back::Main));
        assert!(!app.world().resource::<SettingsDir>().0.join("settings.json").exists());
        for text in ["", ".", "1..2", "NaN", "inf", "0.09", "10.01"] {
            let edit = NumberEdit { setting: FirstPersonSetting::Mouse, text: text.into(), caret: text.len(), selected: false };
            assert!(edit.value().is_none(), "{text:?} must be refused");
        }
    }

    #[test]
    fn slider_drags_live_without_rebuilding_and_saves_on_release() {
        let mut app = settings_app("slider");
        let mut q = app.world_mut().query::<(Entity, &SettingsSlider)>();
        let e = q.iter(app.world()).find(|(_, s)| s.0 == FirstPersonSetting::Fov).unwrap().0;
        app.world_mut().entity_mut(e).insert((Interaction::Pressed, RelativeCursorPosition { normalized: Some(Vec2::new(0.0, 0.0)), cursor_over: true }));
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<Settings>().fov_deg, 65.0);
        assert!(app.world().get_entity(e).is_ok(), "drag keeps the slider entity and cursor coordinates alive");
        assert!(!app.world().resource::<SettingsDir>().0.join("settings.json").exists());
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().clear();
        app.world_mut().get_mut::<RelativeCursorPosition>(e).unwrap().normalized = Some(Vec2::new(0.8, 0.0));
        app.update();
        assert_eq!(app.world().resource::<Settings>().fov_deg, 90.0, "drag beyond the track clamps at the endpoint");
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Left);
        app.update();
        assert!(app.world().resource::<SettingsInput>().dragging.is_none());
        assert_eq!(crate::settings::load(&app.world().resource::<SettingsDir>().0).0.fov_deg, 90.0);
    }

    #[test]
    fn the_sound_switch_toggles_and_is_saved() {
        let dir = std::env::temp_dir().join(format!("exo-menu-sound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut w = World::new();
        w.insert_resource(Settings::default());
        w.insert_resource(SettingsDir(dir.clone()));
        w.init_resource::<Menu>();
        act(&mut w, Action::Sound);
        assert!(!w.resource::<Settings>().sound);
        assert!(!crate::settings::load(&dir).0.sound, "saved off");
        act(&mut w, Action::Sound);
        assert!(w.resource::<Settings>().sound);
        assert!(crate::settings::load(&dir).0.sound, "saved on");
        assert_eq!(w.resource::<Settings>().volume, 0.5, "the volume stays");
    }
}
