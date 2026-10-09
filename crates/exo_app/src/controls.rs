//! Input as state the simulation reads, so scripts and the keyboard drive the same path.
//! Scripted runs never read the keyboard and never grab the mouse.
//!
//! Two layers: `Controls` is the raw input (held keys, tapped keys, mouse pixels) that the keyboard
//! and the scenario scripts write. Once per fixed step `resolve_actions` turns it into `Actions`
//! through the `Bindings` (`content/tuning/bindings.json`); the ship, walker, warp and view read
//! only `Actions`. A gamepad or joystick (#29) is a third source next to keyboard and scripts:
//! its axes and buttons land in `Controls` too, and the bindings map them like keys.
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use flight_core::Curve;
use std::collections::{HashMap, HashSet};

pub fn plugin(app: &mut App) {
    app.init_resource::<Controls>().init_resource::<Actions>().init_resource::<Bindings>();
    app.add_systems(FixedUpdate, resolve_actions.in_set(crate::phases::Fx::Input));
    app.add_systems(FixedLast, drop_taps);
}

/// Window only: the keyboard and mouse into `Controls`.
pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, read_input.in_set(crate::phases::Frame::Input));
}

pub const BINDINGS: &str = include_str!("../../../content/tuning/bindings.json");

/// Raw input, written by the keyboard and mouse (`read_input`) or by a scenario script.
#[derive(Resource, Default)]
pub struct Controls {
    pub held: HashSet<KeyCode>,
    /// Keys pressed since the last fixed step (only keys bound to a tap action).
    pub taps: Vec<KeyCode>,
    /// Mouse movement in pixels since the last fixed step.
    pub mouse: Vec2,
    /// First gamepad: held buttons, buttons pressed since the last fixed step, axes -1..1.
    pub pad_held: HashSet<GamepadButton>,
    pub pad_taps: Vec<GamepadButton>,
    pub pad_axes: HashMap<GamepadAxis, f32>,
    pub scripted: bool,
    /// The game does not have the mouse: a menu is open or the cursor is free. The virtual stick
    /// centres (#110 point 2).
    pub released: bool,
}

/// A bindable input: a key, or a gamepad button (`"Pad:South"` in the file).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    Key(KeyCode),
    Pad(GamepadButton),
}

/// Analog axes: -1..1, from a positive and a negative key and an optional pad axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    MoveX,
    MoveY,
    MoveZ,
    Roll,
    /// Stick pitch and yaw (`Actions::turn`); no keys by default.
    TurnPitch,
    TurnYaw,
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
    /// The one verb (#82): sit, stand up, pick up or set down a crate, whatever is targeted.
    Interact,
    /// Throw the held crate (#83).
    Throw,
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
    /// Dev switch: boost capacitor or the old speed stage (F6, #90).
    BoostMode,
    /// Landing mode of the flight model (spike 13). TODO(initiator): the key (K for now).
    LandingMode,
    /// A/B switch: the axis model's G-safety turn cap on or off (F8, #118).
    TurnCap,
    /// Run the queued banners and toasts (the arrival ritual) fast (#165).
    SkipNotices,
}

impl Axis {
    pub const ALL: [Axis; 6] = [Axis::MoveX, Axis::MoveY, Axis::MoveZ, Axis::Roll, Axis::TurnPitch, Axis::TurnYaw];
    pub fn name(self) -> &'static str {
        match self {
            Axis::MoveX => "move_x",
            Axis::MoveY => "move_y",
            Axis::MoveZ => "move_z",
            Axis::Roll => "roll",
            Axis::TurnPitch => "turn_pitch",
            Axis::TurnYaw => "turn_yaw",
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
    pub const ALL: [Tap; 15] = [Tap::Interact, Tap::Throw, Tap::HoverAssist, Tap::HorizonFollow, Tap::Lag, Tap::DebugFly, Tap::OrbitCamera, Tap::WarpTarget, Tap::Warp, Tap::Decoupled, Tap::DebugHud, Tap::BoostMode, Tap::LandingMode, Tap::TurnCap, Tap::SkipNotices];
    pub fn name(self) -> &'static str {
        match self {
            Tap::Interact => "interact",
            Tap::Throw => "throw",
            Tap::HoverAssist => "hover_assist",
            Tap::HorizonFollow => "horizon_follow",
            Tap::Lag => "lag",
            Tap::DebugFly => "debug_fly",
            Tap::OrbitCamera => "orbit_camera",
            Tap::WarpTarget => "warp_target",
            Tap::Warp => "warp",
            Tap::Decoupled => "decoupled",
            Tap::DebugHud => "debug_hud",
            Tap::BoostMode => "boost_mode",
            Tap::LandingMode => "landing_mode",
            Tap::TurnCap => "turn_cap",
            Tap::SkipNotices => "skip_notices",
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
    /// Pitch and yaw deflection -1..1 (x pitch, nose up positive; y yaw, left positive), from the
    /// pad's `turn_pitch` and `turn_yaw` axes. The ship adds the virtual-joystick mouse to it.
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
    pub positive: Vec<Input>,
    pub negative: Vec<Input>,
    /// Gamepad or joystick axis, and whether it is inverted (`"-LeftStickY"` in the file).
    /// HOTAS devices often report odd axes; `"Axis3"` names a raw axis.
    pub pad: Option<(GamepadAxis, bool)>,
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

/// Gamepad look on foot: full stick turns the walker's view this fast (rad/s; degrees in the file).
#[derive(Clone, Debug, PartialEq)]
pub struct PadBindings {
    pub look_rate: f64,
}

/// Which key feeds which action (`content/tuning/bindings.json`). A key may serve several actions
/// (Space is jump on foot and up in the ship or suit).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Bindings {
    pub axes: Vec<(Axis, AxisBinding)>,
    pub buttons: Vec<(Button, Vec<Input>)>,
    pub taps: Vec<(Tap, Vec<Input>)>,
    pub mouse: MouseBindings,
    pub pad: PadBindings,
}

impl Bindings {
    pub fn axis(&self, a: Axis) -> &AxisBinding {
        &self.axes.iter().find(|(x, _)| *x == a).expect("every axis is bound").1
    }
    fn button_keys(&self, b: Button) -> &[Input] {
        &self.buttons.iter().find(|(x, _)| *x == b).expect("every button is bound").1
    }
    /// Every key bound to a tap action: the keyboard reports these as taps.
    pub fn tap_keys(&self) -> impl Iterator<Item = KeyCode> + '_ {
        self.taps.iter().flat_map(|(_, ks)| ks.iter()).filter_map(|i| if let Input::Key(k) = i { Some(*k) } else { None })
    }
    /// Every pad button bound to a tap action.
    pub fn tap_pad_buttons(&self) -> impl Iterator<Item = GamepadButton> + '_ {
        self.taps.iter().flat_map(|(_, ks)| ks.iter()).filter_map(|i| if let Input::Pad(b) = i { Some(*b) } else { None })
    }

    pub fn from_json(s: &str) -> Result<Bindings, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| format!("bindings.json: {e}"))?;
        let obj = v.as_object().ok_or("bindings.json: not an object")?;
        let err = |action: &str, why: String| format!("bindings.json: action `{action}`: {why}");
        let known: Vec<&str> = Axis::ALL.iter().map(|a| a.name()).chain(Button::ALL.iter().map(|b| b.name())).chain(Tap::ALL.iter().map(|t| t.name())).chain(["mouse", "pad", "_comment", "seat"]).collect();
        if let Some(k) = obj.keys().find(|k| !known.contains(&k.as_str())) {
            return Err(err(k, "unknown action".into()));
        }
        let get = |name: &str| obj.get(name).ok_or_else(|| err(name, "missing".into()));
        let inputs = |name: &str, v: &serde_json::Value| -> Result<Vec<Input>, String> {
            let list = v.as_array().ok_or_else(|| err(name, "expected a list of key names".into()))?;
            list.iter()
                .map(|k| {
                    let n = k.as_str().ok_or_else(|| err(name, format!("key {k} is not a string")))?;
                    input_from_name(n).ok_or_else(|| err(name, format!("unknown key name `{n}`")))
                })
                .collect()
        };
        let keys = |name: &str, v: &serde_json::Value| -> Result<Vec<Input>, String> {
            let list = inputs(name, v)?;
            if list.is_empty() {
                return Err(err(name, "no key".into()));
            }
            Ok(list)
        };
        let mut axes = Vec::new();
        for a in Axis::ALL {
            let n = a.name();
            let o = get(n)?.as_object().ok_or_else(|| err(n, "expected { positive, negative }".into()))?;
            if let Some(k) = o.keys().find(|k| !["positive", "negative", "pad", "deadzone", "curve"].contains(&k.as_str())) {
                return Err(err(n, format!("unknown field `{k}`")));
            }
            let side = |s: &str| inputs(n, o.get(s).ok_or_else(|| err(n, format!("missing `{s}`")))?);
            let pad = match o.get("pad") {
                None => None,
                Some(p) => {
                    let name = p.as_str().ok_or_else(|| err(n, format!("pad {p} is not a string")))?;
                    let (inv, axis) = match name.strip_prefix('-') {
                        Some(a) => (true, a),
                        None => (false, name),
                    };
                    Some((pad_axis_from_name(axis).ok_or_else(|| err(n, format!("unknown pad axis `{axis}`")))?, inv))
                }
            };
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
            let (positive, negative) = (side("positive")?, side("negative")?);
            if positive.is_empty() && negative.is_empty() && pad.is_none() {
                return Err(err(n, "no key".into()));
            }
            axes.push((a, AxisBinding { positive, negative, pad, deadzone, curve }));
        }
        let buttons = Button::ALL.iter().map(|&b| Ok((b, keys(b.name(), get(b.name())?)?))).collect::<Result<_, String>>()?;
        // Files from before #82 call the one verb `seat` and have no `throw`: they keep working.
        let tap_keys = |t: Tap| -> Result<Vec<Input>, String> {
            match (t, obj.get(t.name())) {
                (_, Some(v)) => keys(t.name(), v),
                (Tap::Interact, None) if obj.contains_key("seat") => keys("seat", &obj["seat"]),
                (Tap::Throw, None) => Ok(vec![Input::Key(KeyCode::KeyR)]),
                // Files from before the boost switch (#90) have no `boost_mode`.
                (Tap::BoostMode, None) => Ok(vec![Input::Key(KeyCode::F6)]),
                // Files from before spike 13 have no `landing_mode`.
                (Tap::LandingMode, None) => Ok(vec![Input::Key(KeyCode::KeyK)]),
                // ... and no `turn_cap` (#118).
                (Tap::TurnCap, None) => Ok(vec![Input::Key(KeyCode::F8)]),
                // ... and no `skip_notices` (#165).
                (Tap::SkipNotices, None) => Ok(vec![Input::Key(KeyCode::Enter)]),
                _ => Err(err(t.name(), "missing".into())),
            }
        };
        let taps = Tap::ALL.iter().map(|&t| Ok((t, tap_keys(t)?))).collect::<Result<_, String>>()?;
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
        let p = get("pad")?.as_object().ok_or_else(|| err("pad", "expected an object".into()))?;
        if let Some(k) = p.keys().find(|k| k.as_str() != "look_rate_deg") {
            return Err(err("pad", format!("unknown field `{k}`")));
        }
        let look_rate = p.get("look_rate_deg").and_then(|v| v.as_f64()).filter(|v| v.is_finite() && *v > 0.0).ok_or_else(|| err("pad", "`look_rate_deg` must be a positive number".into()))?;
        Ok(Bindings { axes, buttons, taps, mouse, pad: PadBindings { look_rate: look_rate.to_radians() } })
    }
}

/// The name of an input as the file writes it.
pub fn input_name(i: Input) -> String {
    match i {
        Input::Key(k) => format!("{k:?}"),
        Input::Pad(GamepadButton::Other(n)) => format!("Pad:Button{n}"),
        Input::Pad(b) => format!("Pad:{b:?}"),
    }
}

fn pad_axis_name(a: GamepadAxis) -> String {
    match a {
        GamepadAxis::Other(n) => format!("Axis{n}"),
        a => format!("{a:?}"),
    }
}

fn curve_json(c: &Curve) -> serde_json::Value {
    let interp = match c.interp {
        flight_core::Interp::Smooth => "smooth",
        flight_core::Interp::Linear => "linear",
    };
    serde_json::json!({ "interp": interp, "points": c.points.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>() })
}

impl Bindings {
    /// The file form (`from_json` reads it back): rebinding in the settings writes it.
    pub fn to_json(&self) -> String {
        use serde_json::{json, Map, Value};
        let names = |l: &[Input]| Value::from(l.iter().map(|i| input_name(*i)).collect::<Vec<_>>());
        let mut o = Map::new();
        o.insert("_comment".into(), "Written by the settings menu; the shipped default is content/tuning/bindings.json.".into());
        for (a, ab) in &self.axes {
            let mut e = Map::new();
            e.insert("positive".into(), names(&ab.positive));
            e.insert("negative".into(), names(&ab.negative));
            if let Some((ax, inv)) = ab.pad {
                e.insert("pad".into(), format!("{}{}", if inv { "-" } else { "" }, pad_axis_name(ax)).into());
            }
            if ab.deadzone > 0.0 {
                e.insert("deadzone".into(), ab.deadzone.into());
            }
            if let Some(c) = &ab.curve {
                e.insert("curve".into(), curve_json(c));
            }
            o.insert(a.name().into(), Value::Object(e));
        }
        for (b, l) in &self.buttons {
            o.insert(b.name().into(), names(l));
        }
        for (t, l) in &self.taps {
            o.insert(t.name().into(), names(l));
        }
        let m = &self.mouse;
        o.insert(
            "mouse".into(),
            json!({
                "ship_mode": match m.ship_mode { ShipMouse::Direct => "direct", ShipMouse::Vjoy => "vjoy" },
                "ship_sensitivity": m.ship_sensitivity,
                "walker_sensitivity": m.walker_sensitivity,
                "vjoy_max_angle_deg": m.vjoy_max_angle.to_degrees(),
                "vjoy_deadzone_deg": m.vjoy_deadzone.to_degrees(),
                "vjoy_curve": curve_json(&m.vjoy_curve),
            }),
        );
        o.insert("pad".into(), json!({ "look_rate_deg": self.pad.look_rate.to_degrees() }));
        serde_json::to_string_pretty(&Value::Object(o)).unwrap()
    }

    /// The inputs of one bindable slot (the settings menu's rows).
    pub fn slot_mut(&mut self, slot: Slot) -> &mut Vec<Input> {
        match slot {
            Slot::Positive(a) => &mut self.axes.iter_mut().find(|(x, _)| *x == a).unwrap().1.positive,
            Slot::Negative(a) => &mut self.axes.iter_mut().find(|(x, _)| *x == a).unwrap().1.negative,
            Slot::Button(b) => &mut self.buttons.iter_mut().find(|(x, _)| *x == b).unwrap().1,
            Slot::Tap(t) => &mut self.taps.iter_mut().find(|(x, _)| *x == t).unwrap().1,
        }
    }

    /// Rebind: the slot's first key becomes `k` (pad buttons stay).
    pub fn rebind(&mut self, slot: Slot, k: KeyCode) {
        let l = self.slot_mut(slot);
        match l.iter().position(|i| matches!(i, Input::Key(_))) {
            Some(i) => l[i] = Input::Key(k),
            None => l.insert(0, Input::Key(k)),
        }
    }
}

/// A bindable list of inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Positive(Axis),
    Negative(Axis),
    Button(Button),
    Tap(Tap),
}

impl Slot {
    /// Every keyboard-bindable slot, in menu order (the stick-only turn axes have no keys).
    pub fn all() -> Vec<Slot> {
        let mut v = Vec::new();
        for a in [Axis::MoveZ, Axis::MoveX, Axis::MoveY, Axis::Roll] {
            v.push(Slot::Negative(a));
            v.push(Slot::Positive(a));
        }
        v.extend(Button::ALL.iter().map(|b| Slot::Button(*b)));
        v.extend(Tap::ALL.iter().map(|t| Slot::Tap(*t)));
        v
    }

    pub fn label(self) -> String {
        match self {
            Slot::Positive(Axis::MoveZ) => "back".into(),
            Slot::Negative(Axis::MoveZ) => "forward".into(),
            Slot::Positive(Axis::MoveX) => "right".into(),
            Slot::Negative(Axis::MoveX) => "left".into(),
            Slot::Positive(Axis::MoveY) => "up".into(),
            Slot::Negative(Axis::MoveY) => "down".into(),
            Slot::Positive(Axis::Roll) => "roll left".into(),
            Slot::Negative(Axis::Roll) => "roll right".into(),
            Slot::Positive(a) => format!("{} +", a.name()),
            Slot::Negative(a) => format!("{} -", a.name()),
            Slot::Button(b) => b.name().replace('_', " "),
            Slot::Tap(t) => t.name().replace('_', " "),
        }
    }
}

impl Default for Bindings {
    fn default() -> Self {
        Bindings::from_json(BINDINGS).unwrap_or_else(|e| panic!("{e}"))
    }
}

/// Raw input to actions, a pure function. Keys are digital (no dead zone or curve); a pad axis
/// goes through its binding's dead zone and curve and adds to the keys, clamped to -1..1.
pub fn resolve(b: &Bindings, c: &Controls) -> Actions {
    let down = |i: &Input| match i {
        Input::Key(k) => c.held.contains(k),
        Input::Pad(p) => c.pad_held.contains(p),
    };
    let tapped = |i: &Input| match i {
        Input::Key(k) => c.taps.contains(k),
        Input::Pad(p) => c.pad_taps.contains(p),
    };
    let any = |ks: &[Input]| ks.iter().any(down);
    let axis = |a: Axis| {
        let ab = b.axis(a);
        let keys = any(&ab.positive) as i32 as f64 - any(&ab.negative) as i32 as f64;
        let pad = ab.pad.and_then(|(ax, inv)| c.pad_axes.get(&ax).map(|v| ab.shape(*v as f64) * if inv { -1.0 } else { 1.0 })).unwrap_or(0.0);
        (keys + pad).clamp(-1.0, 1.0)
    };
    let button = |x: Button| any(b.button_keys(x));
    Actions {
        move_dir: DVec3::new(axis(Axis::MoveX), axis(Axis::MoveY), axis(Axis::MoveZ)),
        roll: axis(Axis::Roll),
        turn: DVec2::new(axis(Axis::TurnPitch), axis(Axis::TurnYaw)),
        look: c.mouse,
        boost: button(Button::Boost),
        brake: button(Button::Brake),
        run: button(Button::Run),
        jump: button(Button::Jump),
        warp_exit: button(Button::WarpExit),
        taps: b.taps.iter().filter(|(_, ks)| ks.iter().any(tapped)).map(|(t, _)| *t).collect(),
    }
}

/// Fixed step, after the scenario script and before every reader.
pub fn resolve_actions(mut c: ResMut<Controls>, b: Res<Bindings>, mut a: ResMut<Actions>) {
    let carry = a.look;
    *a = resolve(&b, &c);
    a.look += carry;
    c.taps.clear();
    c.pad_taps.clear();
    c.mouse = Vec2::ZERO;
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
    pads: Query<&Gamepad>,
    menu: Option<Res<crate::menu::Menu>>,
) {
    if c.scripted {
        return;
    }
    // A menu takes the keyboard and mouse: the game sees nothing held.
    c.released = true;
    if menu.is_some_and(|m| m.open()) {
        c.held.clear();
        c.taps.clear();
        c.pad_held.clear();
        c.pad_axes.clear();
        c.pad_taps.clear();
        c.mouse = Vec2::ZERO;
        return;
    }
    // The first gamepad or joystick: every bound axis and button.
    c.pad_axes.clear();
    c.pad_held.clear();
    if let Some(pad) = pads.iter().next() {
        for (_, ab) in &b.axes {
            if let Some((ax, _)) = ab.pad {
                c.pad_axes.insert(ax, pad.get(ax).unwrap_or(0.0));
            }
        }
        c.pad_held = pad.get_pressed().copied().collect();
        for p in b.tap_pad_buttons() {
            if pad.just_pressed(p) && !c.pad_taps.contains(&p) {
                c.pad_taps.push(p);
            }
        }
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
        c.released = false;
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

macro_rules! pad_table {
    ($($b:ident),* $(,)?) => {
        /// Gamepad buttons, `"Pad:<name>"` in the file (Bevy's `GamepadButton` names).
        pub const PAD_BUTTON_NAMES: &[(&str, GamepadButton)] = &[$((stringify!($b), GamepadButton::$b)),*];
    };
}

pad_table!(
    South, East, North, West, C, Z, LeftTrigger, LeftTrigger2, RightTrigger, RightTrigger2, Select, Start, Mode, LeftThumb, RightThumb,
    DPadUp, DPadDown, DPadLeft, DPadRight,
);

/// A key name or `"Pad:<button>"`; raw pad buttons as `"Pad:Button<n>"`.
pub fn input_from_name(n: &str) -> Option<Input> {
    match n.strip_prefix("Pad:") {
        Some(p) => PAD_BUTTON_NAMES
            .iter()
            .find(|(name, _)| *name == p)
            .map(|(_, b)| *b)
            .or_else(|| p.strip_prefix("Button")?.parse().ok().map(GamepadButton::Other))
            .map(Input::Pad),
        None => key_from_name(n).map(Input::Key),
    }
}

/// Bevy's `GamepadAxis` names; raw axes (HOTAS) as `"Axis<n>"`.
pub fn pad_axis_from_name(n: &str) -> Option<GamepadAxis> {
    Some(match n {
        "LeftStickX" => GamepadAxis::LeftStickX,
        "LeftStickY" => GamepadAxis::LeftStickY,
        "LeftZ" => GamepadAxis::LeftZ,
        "RightStickX" => GamepadAxis::RightStickX,
        "RightStickY" => GamepadAxis::RightStickY,
        "RightZ" => GamepadAxis::RightZ,
        _ => GamepadAxis::Other(n.strip_prefix("Axis")?.parse().ok()?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use KeyCode::*;

    fn raw(ks: &[KeyCode], taps: &[KeyCode]) -> Controls {
        Controls { held: ks.iter().copied().collect(), taps: taps.to_vec(), ..default() }
    }
    fn act(ks: &[KeyCode]) -> Actions {
        resolve(&Bindings::default(), &raw(ks, &[]))
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
        let mut a = resolve(&Bindings::default(), &raw(&[], &[KeyG, KeyF, KeyJ]));
        assert!(a.take_tap(Tap::Lag));
        assert!(a.take_tap(Tap::Interact));
        assert!(a.take_tap(Tap::Warp));
        assert!(!a.take_tap(Tap::Lag), "a tap is consumed once");
        assert!(!a.take_tap(Tap::HoverAssist));
    }

    /// #82: a player's file from before the one verb (`seat`, no `throw`) still loads.
    #[test]
    fn old_seat_binding_still_loads() {
        let old = BINDINGS.replace("\"interact\"", "\"seat\"").replace("  \"throw\": [\"KeyR\", \"Pad:RightThumb\"],\n", "");
        assert!(!old.contains("interact") && !old.contains("throw"));
        let b = Bindings::from_json(&old).unwrap();
        let mut a = resolve(&b, &raw(&[], &[KeyF, KeyR]));
        assert!(a.take_tap(Tap::Interact) && a.take_tap(Tap::Throw));
    }

    /// #90: a player's file from before the boost switch still loads, with F6.
    #[test]
    fn old_file_without_boost_mode_gets_f6() {
        let old = BINDINGS.replace("  \"boost_mode\": [\"F6\"],\n", "");
        assert!(!old.contains("boost_mode"));
        let b = Bindings::from_json(&old).unwrap();
        let mut a = resolve(&b, &raw(&[], &[F6]));
        assert!(a.take_tap(Tap::BoostMode));
    }

    /// Spike 13: a player's file from before the landing mode and the turn cap still loads, with
    /// K and F8.
    #[test]
    fn old_file_without_landing_mode_gets_k() {
        let old = BINDINGS.replace("  \"landing_mode\": [\"KeyK\"],\n", "");
        assert!(!old.contains("\"landing_mode\""));
        let mut a = resolve(&Bindings::from_json(&old).unwrap(), &raw(&[], &[KeyK]));
        assert!(a.take_tap(Tap::LandingMode));
        let old = BINDINGS.replace("  \"turn_cap\": [\"F8\"],\n", "");
        assert!(!old.contains("\"turn_cap\""));
        let mut a = resolve(&Bindings::from_json(&old).unwrap(), &raw(&[], &[F8]));
        assert!(a.take_tap(Tap::TurnCap));
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
            assert_eq!(act(&wasd), resolve(&b, &raw(&arrow, &[])));
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
    fn pad_axes_and_buttons() {
        let b = Bindings::default();
        let mut c = Controls::default();
        c.pad_axes.insert(GamepadAxis::LeftStickY, 1.0);
        c.pad_axes.insert(GamepadAxis::RightStickX, 0.6);
        c.pad_held.insert(GamepadButton::LeftThumb);
        c.pad_taps.push(GamepadButton::North);
        let mut a = resolve(&b, &c);
        assert_eq!(a.move_dir.z, -1.0, "stick forward is W");
        let yaw = b.axis(Axis::TurnYaw);
        assert!((a.turn.y + yaw.shape(0.6f32 as f64)).abs() < 1e-12 && a.turn.y < 0.0, "stick right yaws right through dead zone and curve: {}", a.turn.y);
        assert!(a.boost && a.take_tap(Tap::Interact));
        // Keys and stick add up, clamped.
        c.held.insert(KeyW);
        assert_eq!(resolve(&b, &c).move_dir.z, -1.0);
        c.pad_axes.insert(GamepadAxis::RightStickX, 0.05);
        assert_eq!(resolve(&b, &c).turn.y, 0.0, "inside the stick's dead zone");
    }

    #[test]
    fn hotas_raw_axes_and_buttons() {
        assert_eq!(pad_axis_from_name("Axis3"), Some(GamepadAxis::Other(3)));
        assert_eq!(input_from_name("Pad:Button7"), Some(Input::Pad(GamepadButton::Other(7))));
        assert_eq!(input_from_name("Pad:Sout"), None);
        let b = Bindings::from_json(&BINDINGS.replacen("\"pad\": \"-RightStickX\"", "\"pad\": \"Axis5\"", 1)).unwrap();
        assert_eq!(b.axis(Axis::TurnYaw).pad, Some((GamepadAxis::Other(5), false)));
        rejects("\"pad\": \"-RightStickX\"", "\"pad\": \"Wheel\"", "`turn_yaw`: unknown pad axis `Wheel`");
    }

    #[test]
    fn bindings_file_round_trip() {
        let b = Bindings::default();
        let back = Bindings::from_json(&b.to_json()).unwrap();
        assert_eq!(back.axes, b.axes);
        assert_eq!(back.buttons, b.buttons);
        assert_eq!(back.taps, b.taps);
        assert!((back.mouse.vjoy_max_angle - b.mouse.vjoy_max_angle).abs() < 1e-12 && (back.pad.look_rate - b.pad.look_rate).abs() < 1e-12);
    }

    #[test]
    fn rebind_replaces_the_first_key_and_keeps_pad_buttons() {
        let mut b = Bindings::default();
        b.rebind(Slot::Button(Button::Boost), KeyT);
        assert_eq!(b.slot_mut(Slot::Button(Button::Boost))[0], Input::Key(KeyT));
        assert!(b.slot_mut(Slot::Button(Button::Boost)).contains(&Input::Pad(GamepadButton::LeftThumb)));
        assert!(act(&[KeyT]).boost == false && resolve(&b, &raw(&[KeyT], &[])).boost);
        assert!(Slot::all().iter().all(|s| !s.label().is_empty()));
    }

    #[test]
    fn key_names_round_trip() {
        for (n, k) in KEY_NAMES {
            assert_eq!(format!("{k:?}"), *n);
        }
    }
}
