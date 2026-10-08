//! Input as state the simulation reads, so scripts and the keyboard drive the same path
//! (Godot's SpikeInput pattern). Scripted runs never read the keyboard and never grab the mouse.
//!
//! Two layers: `Controls` is the raw input (held keys, tapped keys, mouse pixels) that the keyboard
//! and the scenario scripts write. Once per fixed step `resolve_actions` turns it into `Actions`
//! through the `Bindings` (`content/tuning/bindings.json`); the ship, walker, warp and view read
//! only `Actions`.
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use flight_core::Curve;
use std::collections::HashSet;

pub const BINDINGS: &str = include_str!("../../../content/tuning/bindings.json");

/// Raw input, written by the keyboard and mouse (`read_input`) or by a scenario script.
#[derive(Resource, Default)]
pub struct Controls {
    pub held: HashSet<KeyCode>,
    /// Keys pressed since the last fixed step (only keys bound to a tap action).
    pub taps: Vec<KeyCode>,
    /// Mouse movement in pixels since the last fixed step.
    pub mouse: Vec2,
    pub scripted: bool,
}

/// Analog axes: -1..1, from a positive and a negative key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    MoveX,
    MoveY,
    MoveZ,
    Roll,
}

/// Held buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Boost,
    Brake,
    Run,
    Jump,
    /// Hold to drop out of the quantum drive (emergency exit).
    WarpExit,
}

/// One-shot actions. A tap lives until the end of the next fixed step; unconsumed it is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tap {
    Seat,
    HoverAssist,
    HorizonFollow,
    Lag,
    DebugFly,
    OrbitCamera,
    WarpTarget,
    Warp,
    /// Coupled or decoupled flight (#26).
    Decoupled,
    /// The debug lines of the HUD (F3).
    DebugHud,
}

impl Axis {
    pub const ALL: [Axis; 4] = [Axis::MoveX, Axis::MoveY, Axis::MoveZ, Axis::Roll];
    pub fn name(self) -> &'static str {
        match self {
            Axis::MoveX => "move_x",
            Axis::MoveY => "move_y",
            Axis::MoveZ => "move_z",
            Axis::Roll => "roll",
        }
    }
}

impl Button {
    pub const ALL: [Button; 5] = [Button::Boost, Button::Brake, Button::Run, Button::Jump, Button::WarpExit];
    pub fn name(self) -> &'static str {
        match self {
            Button::Boost => "boost",
            Button::Brake => "brake",
            Button::Run => "run",
            Button::Jump => "jump",
            Button::WarpExit => "warp_exit",
        }
    }
}

impl Tap {
    pub const ALL: [Tap; 10] = [Tap::Seat, Tap::HoverAssist, Tap::HorizonFollow, Tap::Lag, Tap::DebugFly, Tap::OrbitCamera, Tap::WarpTarget, Tap::Warp, Tap::Decoupled, Tap::DebugHud];
    pub fn name(self) -> &'static str {
        match self {
            Tap::Seat => "seat",
            Tap::HoverAssist => "hover_assist",
            Tap::HorizonFollow => "horizon_follow",
            Tap::Lag => "lag",
            Tap::DebugFly => "debug_fly",
            Tap::OrbitCamera => "orbit_camera",
            Tap::WarpTarget => "warp_target",
            Tap::Warp => "warp",
            Tap::Decoupled => "decoupled",
            Tap::DebugHud => "debug_hud",
        }
    }
}

/// What the simulation reads, once per fixed step.
#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct Actions {
    /// -1..1 per axis, x right, y up, z back (W gives z = -1). Ship, suit and walker (x and z).
    pub move_dir: DVec3,
    /// Q positive.
    pub roll: f64,
    /// Pitch and yaw deflection -1..1. Empty from keyboard and mouse for now; the virtual-joystick
    /// mouse and the pad fill it.
    pub turn: DVec2,
    /// Mouse movement in pixels. Taken by whoever uses it; left over it carries to the next step
    /// (as before the action layer).
    pub look: Vec2,
    pub boost: bool,
    pub brake: bool,
    pub run: bool,
    pub jump: bool,
    pub warp_exit: bool,
    taps: Vec<Tap>,
}

impl Actions {
    pub fn take_tap(&mut self, t: Tap) -> bool {
        if let Some(i) = self.taps.iter().position(|&x| x == t) {
            self.taps.remove(i);
            true
        } else {
            false
        }
    }
    pub fn taps(&self) -> &[Tap] {
        &self.taps
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisBinding {
    pub positive: Vec<KeyCode>,
    pub negative: Vec<KeyCode>,
    /// Analog sources only (keyboard is digital): inner dead zone 0..1, then the response curve.
    pub deadzone: f64,
    pub curve: Option<Curve>,
}

impl AxisBinding {
    /// Shapes an analog value -1..1: dead zone (rescaled so the output starts at 0), then the
    /// curve on the magnitude, sign kept.
    pub fn shape(&self, v: f64) -> f64 {
        let m = v.abs().min(1.0);
        if m <= self.deadzone {
            return 0.0;
        }
        let m = (m - self.deadzone) / (1.0 - self.deadzone);
        let m = self.curve.as_ref().map_or(m, |c| c.eval(m));
        m.copysign(v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShipMouse {
    /// Pixels times sensitivity, capped at the ship's turn rate.
    Direct,
    /// Virtual joystick: the mouse moves an offset (pixels times sensitivity, an angle); past the
    /// dead zone it is the deflection, full at the max angle. The HUD shows it.
    Vjoy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MouseBindings {
    pub ship_mode: ShipMouse,
    /// Radians per pixel: the ship's turn (direct) or the stick's offset (vjoy).
    pub ship_sensitivity: f64,
    pub walker_sensitivity: f64,
    /// Virtual joystick, radians (degrees in the file).
    pub vjoy_max_angle: f64,
    pub vjoy_deadzone: f64,
    pub vjoy_curve: Curve,
}

/// Which key feeds which action (`content/tuning/bindings.json`). A key may serve several actions
/// (Space is jump on foot and up in the ship or suit).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Bindings {
    pub axes: Vec<(Axis, AxisBinding)>,
    pub buttons: Vec<(Button, Vec<KeyCode>)>,
    pub taps: Vec<(Tap, Vec<KeyCode>)>,
    pub mouse: MouseBindings,
}

impl Bindings {
    pub fn axis(&self, a: Axis) -> &AxisBinding {
        &self.axes.iter().find(|(x, _)| *x == a).expect("every axis is bound").1
    }
    fn button_keys(&self, b: Button) -> &[KeyCode] {
        &self.buttons.iter().find(|(x, _)| *x == b).expect("every button is bound").1
    }
    /// Every key bound to a tap action: the keyboard reports these as taps.
    pub fn tap_keys(&self) -> impl Iterator<Item = KeyCode> + '_ {
        self.taps.iter().flat_map(|(_, ks)| ks.iter().copied())
    }

    pub fn from_json(s: &str) -> Result<Bindings, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("bindings.json: {e}"))?;
        let obj = v.as_object().ok_or("bindings.json: not an object")?;
        let err = |action: &str, why: String| format!("bindings.json: action `{action}`: {why}");
        let known: Vec<&str> = Axis::ALL.iter().map(|a| a.name()).chain(Button::ALL.iter().map(|b| b.name())).chain(Tap::ALL.iter().map(|t| t.name())).chain(["mouse", "_comment"]).collect();
        if let Some(k) = obj.keys().find(|k| !known.contains(&k.as_str())) {
            return Err(err(k, "unknown action".into()));
        }
        let get = |name: &str| obj.get(name).ok_or_else(|| err(name, "missing".into()));
        let keys = |name: &str, v: &serde_json::Value| -> Result<Vec<KeyCode>, String> {
            let list = v.as_array().ok_or_else(|| err(name, "expected a list of key names".into()))?;
            if list.is_empty() {
                return Err(err(name, "no key".into()));
            }
            list.iter()
                .map(|k| {
                    let n = k.as_str().ok_or_else(|| err(name, format!("key {k} is not a string")))?;
                    key_from_name(n).ok_or_else(|| err(name, format!("unknown key name `{n}`")))
                })
                .collect()
        };
        let mut axes = Vec::new();
        for a in Axis::ALL {
            let n = a.name();
            let o = get(n)?.as_object().ok_or_else(|| err(n, "expected { positive, negative }".into()))?;
            if let Some(k) = o.keys().find(|k| !["positive", "negative", "deadzone", "curve"].contains(&k.as_str())) {
                return Err(err(n, format!("unknown field `{k}`")));
            }
            let side = |s: &str| keys(n, o.get(s).ok_or_else(|| err(n, format!("missing `{s}`")))?);
            let deadzone = match o.get("deadzone") {
                None => 0.0,
                Some(d) => d.as_f64().filter(|d| (0.0..1.0).contains(d)).ok_or_else(|| err(n, format!("deadzone {d} outside 0..1")))?,
            };
            let curve = match o.get("curve") {
                None => None,
                Some(c) => {
                    let c: Curve = serde_json::from_value(c.clone()).map_err(|e| err(n, format!("curve: {e}")))?;
                    c.validate().map_err(|e| err(n, e))?;
                    Some(c)
                }
            };
            axes.push((a, AxisBinding { positive: side("positive")?, negative: side("negative")?, deadzone, curve }));
        }
        let buttons = Button::ALL.iter().map(|&b| Ok((b, keys(b.name(), get(b.name())?)?))).collect::<Result<_, String>>()?;
        let taps = Tap::ALL.iter().map(|&t| Ok((t, keys(t.name(), get(t.name())?)?))).collect::<Result<_, String>>()?;
        let m = get("mouse")?.as_object().ok_or_else(|| err("mouse", "expected an object".into()))?;
        if let Some(k) = m.keys().find(|k| !["ship_mode", "ship_sensitivity", "walker_sensitivity", "vjoy_max_angle_deg", "vjoy_deadzone_deg", "vjoy_curve"].contains(&k.as_str())) {
            return Err(err("mouse", format!("unknown field `{k}`")));
        }
        let num = |f: &str| {
            m.get(f).and_then(|v| v.as_f64()).filter(|v| v.is_finite() && *v > 0.0).ok_or_else(|| err("mouse", format!("`{f}` must be a positive number")))
        };
        let ship_mode = match m.get("ship_mode").and_then(|v| v.as_str()) {
            Some("direct") => ShipMouse::Direct,
            Some("vjoy") => ShipMouse::Vjoy,
            other => return Err(err("mouse", format!("ship_mode {other:?}: \"direct\" or \"vjoy\""))),
        };
        let (max, dz) = (num("vjoy_max_angle_deg")?, m.get("vjoy_deadzone_deg").and_then(|v| v.as_f64()).unwrap_or(f64::NAN));
        if !(0.0..max).contains(&dz) {
            return Err(err("mouse", format!("vjoy_deadzone_deg must be 0 or more and below vjoy_max_angle_deg ({max})")));
        }
        let curve: Curve = serde_json::from_value(m.get("vjoy_curve").cloned().ok_or_else(|| err("mouse", "missing `vjoy_curve`".into()))?)
            .map_err(|e| err("mouse", format!("vjoy_curve: {e}")))?;
        curve.validate().map_err(|e| err("mouse", format!("vjoy_curve: {e}")))?;
        let mouse = MouseBindings {
            ship_mode,
            ship_sensitivity: num("ship_sensitivity")?,
            walker_sensitivity: num("walker_sensitivity")?,
            vjoy_max_angle: max.to_radians(),
            vjoy_deadzone: dz.to_radians(),
            vjoy_curve: curve,
        };
        Ok(Bindings { axes, buttons, taps, mouse })
    }
}

impl Default for Bindings {
    fn default() -> Self {
        Bindings::from_json(BINDINGS).unwrap_or_else(|e| panic!("{e}"))
    }
}

/// Raw input to actions, a pure function. Keyboard axes are digital (no dead zone or curve).
pub fn resolve(b: &Bindings, held: &HashSet<KeyCode>, taps: &[KeyCode], mouse: Vec2) -> Actions {
    let any = |ks: &[KeyCode]| ks.iter().any(|k| held.contains(k));
    let axis = |a: Axis| {
        let ab = b.axis(a);
        any(&ab.positive) as i32 as f64 - any(&ab.negative) as i32 as f64
    };
    let button = |x: Button| any(b.button_keys(x));
    Actions {
        move_dir: DVec3::new(axis(Axis::MoveX), axis(Axis::MoveY), axis(Axis::MoveZ)),
        roll: axis(Axis::Roll),
        turn: DVec2::ZERO,
        look: mouse,
        boost: button(Button::Boost),
        brake: button(Button::Brake),
        run: button(Button::Run),
        jump: button(Button::Jump),
        warp_exit: button(Button::WarpExit),
        taps: b.taps.iter().filter(|(_, ks)| ks.iter().any(|k| taps.contains(k))).map(|(t, _)| *t).collect(),
    }
}

/// Fixed step, after the scenario script and before every reader.
pub fn resolve_actions(mut c: ResMut<Controls>, b: Res<Bindings>, mut a: ResMut<Actions>) {
    let taps = std::mem::take(&mut c.taps);
    let mouse = std::mem::take(&mut c.mouse);
    let carry = a.look;
    *a = resolve(&b, &c.held, &taps, mouse);
    a.look += carry;
}

/// FixedLast: taps nobody consumed this step are dropped (a tap in the wrong context does not
/// fire later).
pub fn drop_taps(mut a: ResMut<Actions>) {
    a.taps.clear();
}

pub fn read_input(
    mut c: ResMut<Controls>,
    b: Res<Bindings>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    mut cursor: Query<&mut CursorOptions>,
) {
    if c.scripted {
        return;
    }
    c.held = keys.get_pressed().copied().collect();
    for k in b.tap_keys() {
        if keys.just_pressed(k) && !c.taps.contains(&k) {
            c.taps.push(k);
        }
    }
    let Ok(mut cur) = cursor.single_mut() else { return };
    if mouse_buttons.just_pressed(MouseButton::Left) {
        cur.grab_mode = CursorGrabMode::Locked;
        cur.visible = false;
    }
    if keys.just_pressed(KeyCode::Escape) {
        cur.grab_mode = CursorGrabMode::None;
        cur.visible = true;
    }
    if cur.grab_mode != CursorGrabMode::None {
        c.mouse += motion.delta;
    }
}

macro_rules! key_table {
    ($($k:ident),* $(,)?) => {
        /// Our own name table for the keys we bind: Bevy's `KeyCode` names as strings.
        pub const KEY_NAMES: &[(&str, KeyCode)] = &[$((stringify!($k), KeyCode::$k)),*];
    };
}

key_table!(
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW,
    KeyX, KeyY, KeyZ, Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9, Space, ShiftLeft, ShiftRight,
    ControlLeft, ControlRight, AltLeft, AltRight, Tab, Enter, Backspace, CapsLock, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, PageUp,
    PageDown, Home, End, Insert, Delete, Minus, Equal, BracketLeft, BracketRight, Semicolon, Quote, Backquote, Backslash, Comma, Period,
    Slash, Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, NumpadAdd, NumpadSubtract,
    NumpadMultiply, NumpadDivide, NumpadEnter, NumpadDecimal, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
);

pub fn key_from_name(n: &str) -> Option<KeyCode> {
    KEY_NAMES.iter().find(|(name, _)| *name == n).map(|(_, k)| *k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use KeyCode::*;

    fn held(ks: &[KeyCode]) -> HashSet<KeyCode> {
        ks.iter().copied().collect()
    }
    fn act(ks: &[KeyCode]) -> Actions {
        resolve(&Bindings::default(), &held(ks), &[], Vec2::ZERO)
    }

    #[test]
    fn w_and_d() {
        assert_eq!(act(&[KeyW, KeyD]).move_dir, DVec3::new(1.0, 0.0, -1.0));
    }

    #[test]
    fn space_and_ctrl_cancel() {
        assert_eq!(act(&[Space, ControlLeft]).move_dir, DVec3::ZERO);
        assert_eq!(act(&[Space]).move_dir.y, 1.0);
        assert!(act(&[Space]).jump);
    }

    #[test]
    fn q_and_e() {
        assert_eq!(act(&[KeyQ]).roll, 1.0);
        assert_eq!(act(&[KeyE]).roll, -1.0);
        assert_eq!(act(&[KeyQ, KeyE]).roll, 0.0);
    }

    #[test]
    fn shift_is_boost_and_run() {
        let a = act(&[ShiftLeft, KeyX]);
        assert!(a.boost && a.run && a.brake && !a.jump);
    }

    #[test]
    fn taps_map_to_actions() {
        let mut a = resolve(&Bindings::default(), &HashSet::new(), &[KeyG, KeyF, KeyJ], Vec2::ZERO);
        assert!(a.take_tap(Tap::Lag));
        assert!(a.take_tap(Tap::Seat));
        assert!(a.take_tap(Tap::Warp));
        assert!(!a.take_tap(Tap::Lag), "a tap is consumed once");
        assert!(!a.take_tap(Tap::HoverAssist));
    }

    /// G was missing from the keyboard's tap list (only scenarios could inject it).
    #[test]
    fn every_tap_action_is_a_keyboard_tap() {
        let b = Bindings::default();
        let tap_keys: Vec<KeyCode> = b.tap_keys().collect();
        assert!(tap_keys.contains(&KeyG));
        assert!(!tap_keys.contains(&KeyB), "B is bound to nothing");
    }

    #[test]
    fn stale_tap_is_dropped() {
        let mut app = App::new();
        app.init_resource::<Controls>().init_resource::<Bindings>().init_resource::<Actions>();
        app.add_systems(Update, (resolve_actions, drop_taps).chain());
        app.world_mut().resource_mut::<Controls>().taps.push(KeyH);
        app.update();
        app.update();
        assert!(!app.world_mut().resource_mut::<Actions>().take_tap(Tap::HoverAssist), "an unconsumed tap must not fire a step later");
        // Through the same path a consumer in the same step sees it.
        app.world_mut().resource_mut::<Controls>().taps.push(KeyG);
        app.world_mut().run_system_once(resolve_actions).unwrap();
        assert!(app.world_mut().resource_mut::<Actions>().take_tap(Tap::Lag));
    }

    #[test]
    fn mouse_carries_until_taken() {
        let mut app = App::new();
        app.init_resource::<Controls>().init_resource::<Bindings>().init_resource::<Actions>();
        app.add_systems(Update, resolve_actions);
        app.world_mut().resource_mut::<Controls>().mouse = Vec2::new(3.0, 0.0);
        app.update();
        app.world_mut().resource_mut::<Controls>().mouse = Vec2::new(2.0, 1.0);
        app.update();
        assert_eq!(app.world().resource::<Actions>().look, Vec2::new(5.0, 1.0));
    }

    #[test]
    fn arrows_instead_of_wasd() {
        let arrows = BINDINGS
            .replacen("\"positive\": [\"KeyD\"], \"negative\": [\"KeyA\"]", "\"positive\": [\"ArrowRight\"], \"negative\": [\"ArrowLeft\"]", 1)
            .replacen("\"positive\": [\"KeyS\"], \"negative\": [\"KeyW\"]", "\"positive\": [\"ArrowDown\"], \"negative\": [\"ArrowUp\"]", 1);
        let b = Bindings::from_json(&arrows).unwrap();
        assert_ne!(b, Bindings::default());
        for (wasd, arrow) in [(vec![KeyW, KeyD], vec![ArrowUp, ArrowRight]), (vec![KeyS, KeyA, Space], vec![ArrowDown, ArrowLeft, Space])] {
            assert_eq!(act(&wasd), resolve(&b, &held(&arrow), &[], Vec2::ZERO));
        }
    }

    fn rejects(from: &str, to: &str, needle: &str) {
        assert!(BINDINGS.contains(from), "fixture {from:?} not in bindings.json");
        let e = Bindings::from_json(&BINDINGS.replacen(from, to, 1)).unwrap_err();
        assert!(e.contains(needle), "{e:?} should contain {needle:?}");
    }

    #[test]
    fn validation_names_the_action() {
        rejects("\"boost\":", "\"bost\": [\"KeyB\"], \"boost\":", "`bost`: unknown action");
        rejects("\"lag\": [\"KeyG\"]", "\"lag\": [\"KeyGG\"]", "`lag`: unknown key name `KeyGG`");
        rejects("\"lag\": [\"KeyG\"]", "\"lag\": []", "`lag`: no key");
        rejects("\"roll\": {", "\"roll\": { \"deadzone\": 1.5,", "`roll`: deadzone");
        rejects("\"roll\": {", "\"roll\": { \"curve\": { \"interp\": \"linear\", \"points\": [[1, 0], [0, 1]] },", "`roll`: curve x must be strictly ascending");
        rejects("\"lag\": [\"KeyG\"],", "", "`lag`: missing");
        rejects("\"vjoy\"", "\"mouse-aim\"", "`mouse`: ship_mode");
        rejects("\"vjoy_deadzone_deg\": 1.5", "\"vjoy_deadzone_deg\": 20", "`mouse`: vjoy_deadzone_deg");
    }

    #[test]
    fn analog_shape() {
        let mut ab = Bindings::default().axis(Axis::MoveX).clone();
        ab.deadzone = 0.2;
        assert_eq!(ab.shape(0.1), 0.0);
        assert!((ab.shape(0.6) - 0.5).abs() < 1e-12);
        assert!((ab.shape(-1.0) + 1.0).abs() < 1e-12);
    }

    #[test]
    fn key_names_round_trip() {
        for (n, k) in KEY_NAMES {
            assert_eq!(format!("{k:?}"), *n);
        }
    }
}
