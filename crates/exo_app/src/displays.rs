//! Display panels (#285): screens in the world the walker uses with the interact key, like a
//! station's console. Looking at one shows "[F] use <name>"; F frees the mouse cursor and shows
//! the panel, a click on it acts, F or Escape leaves. The walker stands still meanwhile.
//!
//! What a panel shows is a `display_core::Panel` that its owner writes into `Panels` each tick;
//! clicks come back as `DisplayAction` messages. A role adds a panel with two systems and no
//! other file: one writes `Panels::set(id, panel)` from its state, one reads `DisplayAction`
//! for its id. The panel's place in the world is a `DisplayPanel` entity (the dev area spawns
//! them from its registry).
//!
//! Two ways to draw it (F6, a variant to compare by feel): `Overlay` puts the panel on the
//! screen, centred, 70 % of the window's height, the pointer is the cursor; `World` draws it on
//! the display's quad (a camera renders the panel's UI into a texture) and the pointer is where
//! the ray through the cursor meets the quad. Pointer and click go through `Controls`, so scripted
//! runs drive both without a mouse.
use crate::controls::{Actions, Controls, Tap};
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::{DQuat, DVec2, DVec3};
use bevy::prelude::*;
use display_core::{Fit, Panel, PanelId, Pointer};
use std::collections::HashMap;
use walker_core::Frame;

/// Reach of the interact key on a display: eye to the panel (m). TODO(initiator): by feel.
pub const USE_RANGE: f64 = 2.5;
/// Overlay: the panel's height as a share of the window's. TODO(initiator): by feel.
pub const OVERLAY_SHARE: f64 = 0.7;

/// A display in the world: what it is called (the prompt), which panel it shows, its quad.
#[derive(Component, Clone, Debug)]
pub struct DisplayPanel {
    pub id: PanelId,
    pub name: String,
    /// Width and height of the quad (m).
    pub size_m: DVec2,
    /// The quad's centre, world space.
    pub centre: DVec3,
    /// Local x right and y up on the screen, +z out of its front (towards the user).
    pub rot: DQuat,
}

/// The panels by id, written by their owners each tick.
#[derive(Resource, Default, Debug)]
pub struct Panels(pub HashMap<PanelId, Panel>);

impl Panels {
    /// Sets a panel; an unchanged one is not touched (no change detection, no redraw).
    pub fn set(&mut self, id: PanelId, panel: Panel) {
        if self.0.get(&id) != Some(&panel) {
            self.0.insert(id, panel);
        }
    }
    pub fn get(&self, id: &PanelId) -> Option<&Panel> {
        self.0.get(id)
    }
}

/// A click on a panel's widget: the panel, the widget's action id, a toggle's new state (1 or 0)
/// or a slider's value (0..1).
#[derive(Message, Clone, Debug, PartialEq)]
pub struct DisplayAction {
    pub panel: PanelId,
    pub action: String,
    pub value: Option<f32>,
}

/// Which display the player uses.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveDisplay {
    #[default]
    Idle,
    Using(Entity),
}

impl ActiveDisplay {
    pub fn using(&self) -> Option<Entity> {
        match self {
            ActiveDisplay::Using(e) => Some(*e),
            ActiveDisplay::Idle => None,
        }
    }
}

/// The pointer on the display in use (hover and press for drawing; reset on enter and leave).
#[derive(Resource, Default, Debug)]
pub struct DisplayPointer(pub Pointer);

/// How panels are drawn (F6).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayMode {
    #[default]
    Overlay,
    World,
}

/// The screen as the pointer sees it: the window's size (logical pixels) and, in world mode, the
/// camera's ray through the pointer (world space), written each frame by the window. Headless the
/// window is the default 1600 x 900 and there is no ray.
#[derive(Resource, Debug)]
pub struct DisplayView {
    pub window: Vec2,
    pub ray: Option<(DVec3, DVec3)>,
}

impl Default for DisplayView {
    fn default() -> Self {
        DisplayView { window: Vec2::new(1600.0, 900.0), ray: None }
    }
}

impl DisplayView {
    /// Where a panel sits on the screen in overlay mode.
    pub fn fit(&self, panel: &Panel) -> Fit {
        Fit::centred(DVec2::new(self.window.x as f64, self.window.y as f64), panel.size, OVERLAY_SHARE)
    }
}

pub fn plugin(app: &mut App) {
    use crate::phases::Fx;
    app.init_resource::<Panels>().init_resource::<ActiveDisplay>().init_resource::<DisplayPointer>().init_resource::<DisplayMode>().init_resource::<DisplayView>();
    app.add_message::<DisplayAction>();
    app.add_systems(FixedUpdate, switch_mode.after(crate::controls::resolve_actions).in_set(Fx::Input));
    app.add_systems(FixedUpdate, use_display.after(crate::interact::interaction).before(crate::walker::walker_step).in_set(Fx::Walker));
}

/// F6: overlay or world.
fn switch_mode(mut actions: ResMut<Actions>, mut mode: ResMut<DisplayMode>) {
    if actions.take_tap(Tap::DisplayMode) {
        *mode = match *mode {
            DisplayMode::Overlay => DisplayMode::World,
            DisplayMode::World => DisplayMode::Overlay,
        };
        info!("display mode: {:?}", *mode);
    }
}

/// The eye and the view direction of a walker on foot (outside a cabin), or None.
pub fn eye_on_foot(pl: &Player) -> Option<(DVec3, DVec3)> {
    (pl.ship.is_none() && !pl.seated && !pl.fly && pl.body.is_none()).then(|| (pl.w.pos + pl.world_up(Frame::IDENTITY) * EYE_HEIGHT, pl.world_look(Frame::IDENTITY)))
}

/// Each tick while a display is in use: Escape leaves, the walker stands still (no walking, no
/// turning), and the pointer acts on the panel. Leaves by itself when the walker is no longer
/// on foot in reach or the display is gone.
#[allow(clippy::too_many_arguments)]
pub fn use_display(
    mut active: ResMut<ActiveDisplay>,
    mut pointer: ResMut<DisplayPointer>,
    mut actions: ResMut<Actions>,
    mut controls: ResMut<Controls>,
    mode: Res<DisplayMode>,
    view: Res<DisplayView>,
    panels: Res<Panels>,
    displays: Query<&DisplayPanel>,
    players: Query<&Player>,
    mut out: MessageWriter<DisplayAction>,
    mut last: Local<Option<Entity>>,
) {
    let click = std::mem::take(&mut controls.click);
    if let Some(e) = active.using() {
        let in_reach = displays.get(e).ok().zip(players.single().ok().and_then(eye_on_foot)).is_some_and(|(d, (eye, _))| eye.distance(d.centre) < USE_RANGE * 1.5);
        if actions.take_tap(Tap::DisplayLeave) || !in_reach {
            *active = ActiveDisplay::Idle;
        }
    }
    if *last != active.using() {
        *last = active.using();
        pointer.0 = Pointer::default();
    }
    controls.free_cursor = active.using().is_some();
    let Some(e) = active.using() else { return };
    // The walker stands: no walking, jumping or turning while the hands are on the panel.
    actions.move_dir = DVec3::ZERO;
    actions.run = false;
    actions.jump = false;
    actions.look = Vec2::ZERO;
    actions.turn = DVec2::ZERO;
    let Ok(d) = displays.get(e) else { return };
    let Some(panel) = panels.get(&d.id) else { return };
    let at = match *mode {
        DisplayMode::Overlay => controls.pointer.map(|p| view.fit(panel).to_panel(DVec2::new(p.x as f64, p.y as f64))),
        DisplayMode::World => view.ray.and_then(|(o, dir)| display_core::ray_to_panel(o, dir, d.centre, d.rot, d.size_m)).map(|h| panel.from_uv(h.uv)),
    };
    for c in pointer.0.step(panel, at, controls.pointer_down || click) {
        out.write(DisplayAction { panel: d.id.clone(), action: c.action, value: c.value.map(|v| v as f32) });
    }
}

/// The window position (logical pixels) of a panel point in overlay mode, for scripts: where to put
/// `Controls::pointer` to point at it.
pub fn overlay_point(w: &World, panel: &PanelId, at: DVec2) -> Option<Vec2> {
    let p = w.resource::<Panels>().get(panel)?;
    let s = w.resource::<DisplayView>().fit(p).to_screen(at);
    Some(Vec2::new(s.x as f32, s.y as f32))
}

/// The window position of a widget's centre (or of `t` across a slider, 0..1) in overlay mode.
pub fn overlay_point_of(w: &World, panel: &PanelId, action: &str, t: Option<f64>) -> Option<Vec2> {
    let p = w.resource::<Panels>().get(panel)?;
    let r = p.find(action)?.rect;
    let at = match t {
        Some(t) => r.at(DVec2::new(t, 0.5)),
        None => r.centre(),
    };
    overlay_point(w, panel, at)
}

// ---------- window: drawing ----------

/// Window only: the screen size and pointer ray, the overlay, and the quads in the world.
pub fn window_plugin(app: &mut App) {
    use crate::phases::Frame;
    app.add_systems(Startup, spawn_overlay);
    app.add_systems(Update, (update_view, add_quads, draw_overlay, draw_world).chain().in_set(Frame::Hud));
}

mod draw;
pub use draw::*;
