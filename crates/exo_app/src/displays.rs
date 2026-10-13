//! Display panels (#285): screens in the world, operated where they hang (decision "Screens you
//! touch", 2026-10-13, as in Star Citizen). The walker stands at a machine; looking at its screen
//! shows "[F] use <name>". F frees the mouse cursor, and the cursor is then the pointer on the
//! screen in the world: the ray from the camera through the cursor meets the display's quad, and
//! that point is where the panel is touched. A click acts, a held slider follows the cursor (also
//! beside the screen), F or Escape leaves. No window opens over the view. The walker stands still
//! and the world runs on; damage does not pull the walker off (leaving is only F, Escape, or
//! walking out of reach). One player per screen.
//!
//! What a panel shows is a `display_core::Panel` that its owner writes into `Panels` each tick;
//! clicks come back as `DisplayAction` messages. A role adds a panel with two systems and no
//! other file: one writes `Panels::set(id, panel)` from its state, one reads `DisplayAction`
//! for its id. The panel's place in the world is a `DisplayPanel` entity (the dev area spawns
//! them from its registry).
//!
//! Every screen near the camera shows its panel live, rendered into the quad's texture, so others
//! see a slider move; the one in use redraws every frame and carries a small cursor mark at the
//! pointer. The size of a tier (S, M, L) is the panel's canvas and the texture's resolution; how
//! large it looks is the quad's size in metres. The pointer comes through `DisplayView::ray`
//! (written from the camera and `Controls::pointer` each frame in a window), so scripts drive it
//! with `aim_at` (a world point, e.g. `world_point_of` a widget) without a mouse; clicks go through
//! `Controls`.
//!
use crate::controls::{Actions, Controls, Tap};
use crate::origin::WorldPose;
use crate::view::MainCamera;
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::{DQuat, DVec2, DVec3};
use bevy::prelude::*;
use display_core::{Panel, PanelId, Pointer};
use std::collections::HashMap;
use walker_core::Frame;

/// Reach of the interact key on a display: eye to the panel (m). TODO(initiator): by feel.
pub const USE_RANGE: f64 = 2.5;
/// The look of the panels, embedded; dev builds reload it from `content/ui/` while running.
pub const DISPLAYS_JSON: &str = include_str!("../../../content/ui/displays.json");

/// The displays' theme (`content/ui/displays.json`).
#[derive(Resource, Clone, Debug)]
pub struct DisplayLook(pub display_core::Theme);

impl Default for DisplayLook {
    fn default() -> Self {
        DisplayLook(display_core::Theme::from_json(DISPLAYS_JSON).unwrap_or_else(|e| panic!("{e}")))
    }
}

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

/// The screen as the pointer sees it: the window's size (logical pixels) and the camera's ray
/// through the cursor (world space), written each frame by the window. Headless the window is the
/// default 1600 x 900 and the ray is only what a script sets (`aim_at`).
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

pub fn plugin(app: &mut App) {
    use crate::phases::Fx;
    app.init_resource::<Panels>().init_resource::<DisplayLook>().init_resource::<ActiveDisplay>().init_resource::<DisplayPointer>().init_resource::<DisplayView>();
    app.add_message::<DisplayAction>();
    app.add_systems(FixedUpdate, use_display.after(crate::interact::interaction).before(crate::walker::walker_step).in_set(Fx::Walker));
}

/// The eye and the view direction of a walker on foot (outside a cabin), or None.
pub fn eye_on_foot(pl: &Player) -> Option<(DVec3, DVec3)> {
    (pl.ship.is_none() && !pl.seated && !pl.fly && pl.body.is_none()).then(|| (pl.w.pos + pl.world_up(Frame::IDENTITY) * EYE_HEIGHT, pl.world_look(Frame::IDENTITY)))
}

/// Each tick while a display is in use: F or Escape leaves, the walker stands still (no walking, no
/// turning), and the pointer acts on the panel. Leaves by itself when the walker is no longer
/// on foot in reach or the display is gone.
#[allow(clippy::too_many_arguments)]
pub fn use_display(
    mut active: ResMut<ActiveDisplay>,
    mut pointer: ResMut<DisplayPointer>,
    mut actions: ResMut<Actions>,
    mut controls: ResMut<Controls>,
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
    // The cursor's ray against the quad; off the screen it hits nothing. A held button keeps
    // the pointer on the quad's plane, so a slider follows the cursor beside the screen too.
    let held = pointer.0.pressed.is_some();
    let hit = view.ray.and_then(|(o, dir)| if held { display_core::ray_to_panel_plane(o, dir, d.centre, d.rot, d.size_m) } else { display_core::ray_to_panel(o, dir, d.centre, d.rot, d.size_m) });
    let at = hit.map(|h| panel.from_uv(h.uv));
    for c in pointer.0.step(panel, at, controls.pointer_down || click) {
        out.write(DisplayAction { panel: d.id.clone(), action: c.action, value: c.value.map(|v| v as f32) });
    }
}

/// The world point of a panel point (panel pixels) on a display's quad, for scripts.
pub fn world_point(w: &World, display: &DisplayPanel, at: DVec2) -> Option<DVec3> {
    let p = w.resource::<Panels>().get(&display.id)?;
    Some(display_core::panel_point(display.centre, display.rot, display.size_m, p.to_uv(at)))
}

/// The world point of a widget's centre (or of `t` across a slider, 0..1) on a display's quad, for
/// scripts.
pub fn world_point_of(w: &World, display: &DisplayPanel, action: &str, t: Option<f64>) -> Option<DVec3> {
    let r = w.resource::<Panels>().get(&display.id)?.find(action)?.rect;
    world_point(w, display, t.map_or(r.centre(), |t| r.at(DVec2::new(t, 0.5))))
}

/// For scripts: the cursor goes to a world point. The ray is set from the camera (window) or the
/// walker's eye (headless) to the point at once; with a window camera `Controls::pointer` gets the
/// point's window position too, so the next frame's ray is the same one.
pub fn aim_at(w: &mut World, target: DVec3) {
    let cam = w.query_filtered::<(&WorldPose, &Projection), With<MainCamera>>().iter(w).find_map(|(p, pr)| if let Projection::Perspective(pp) = pr { Some((p.pos, p.rot, pp.fov as f64)) } else { None });
    let win = w.resource::<DisplayView>().window;
    let eye = match cam {
        Some((pos, rot, fov)) => {
            let at = display_core::window_point(pos, rot, fov, DVec2::new(win.x as f64, win.y as f64), target);
            w.resource_mut::<Controls>().pointer = at.map(|a| Vec2::new(a.x as f32, a.y as f32));
            Some(pos)
        }
        None => w.query::<&Player>().iter(w).find_map(eye_on_foot).map(|(e, _)| e),
    };
    w.resource_mut::<DisplayView>().ray = eye.map(|e| (e, (target - e).normalize()));
}

/// For scripts: the cursor points at nothing (the corner of the window, no ray).
pub fn aim_off(w: &mut World) {
    w.resource_mut::<Controls>().pointer = Some(Vec2::new(4.0, 4.0));
    w.resource_mut::<DisplayView>().ray = None;
}

// ---------- window: drawing ----------

/// Window only: the screen size and pointer ray, the fonts, and the screens in the world.
pub fn window_plugin(app: &mut App) {
    use crate::phases::Frame;
    app.add_systems(Update, (reload_look, load_fonts, update_view, add_quads, draw_world).chain().in_set(Frame::Hud));
}

/// Dev builds: `content/ui/displays.json` is read every 250 ms of wall time; a changed file that
/// parses replaces the look at once, one that does not is reported and the old look stays.
fn reload_look(mut look: ResMut<DisplayLook>, mut state: Local<Option<(String, std::time::Instant)>>) {
    if !cfg!(debug_assertions) {
        return;
    }
    let now = std::time::Instant::now();
    let (loaded, next) = state.get_or_insert_with(|| (DISPLAYS_JSON.to_string(), now));
    if now < *next {
        return;
    }
    *next = now + std::time::Duration::from_millis(250);
    let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/ui/displays.json"));
    let Ok(text) = std::fs::read_to_string(path) else { return };
    if text == *loaded {
        return;
    }
    *loaded = text.clone();
    match display_core::Theme::from_json(&text) {
        Ok(t) => {
            look.0 = t;
            println!("displays: look reloaded");
        }
        Err(e) => println!("displays: reload refused, the old look stays: {e}"),
    }
}

mod draw;
pub use draw::*;
