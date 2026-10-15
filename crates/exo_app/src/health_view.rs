//! The medic view: look at another player within 3 m and see their injuries.
//!
//! Research (Barotrauma, `~/Work/exo-research/barotrauma/source/`, read for the rules only):
//! - `BarotraumaClient/ClientSource/Characters/Health/CharacterHealth.cs`, `DrawHealthWindow`
//!   (about line 1826): the health window of any character shows the limbs, tinted by the
//!   afflictions that show (negative orange to red, positive orange to lime).
//! - Same file, `afflictionIconList` (about line 345): the afflictions as icons; an icon shows
//!   while `Affliction.ShouldShowIcon` says so (`BarotraumaShared/.../Afflictions/Affliction.cs`,
//!   line 544): strength at or above `ShowIconThreshold` for yourself, `ShowIconToOthersThreshold`
//!   for others (`AfflictionPrefab.cs`, lines 755 and 760; both default 0.05, per affliction in XML).
//! - Same file, "Suitable treatments" (`SuitableTreatments` text about line 356, filled about line
//!   1430): `GetSuitableTreatments(ignoreHiddenAfflictions: true, checkTreatmentSuggestionThreshold:
//!   true, checkTreatmentThreshold: false)`. The list is the treatments in `TreatmentSuitabilities`
//!   of the shown afflictions, times strength (`BarotraumaShared/.../Health/CharacterHealth.cs`,
//!   line 1239); negative ones are not listed, and an item the viewer carries counts times ten
//!   (prefer what you have).
//! - The affliction XML files are not in this checkout (no `ShowIconThreshold` outside the .cs).
//!
//! What we take: the injuries others see are the ones a treatment can reduce (burn, radiation, oxygen
//! low); the suggestion is an item in our action bar that suits one of them, shown as an outline on
//! its slot and named in the panel (`BANDAGE ▶ BURN`). Not taken: the limb pictures and the
//! thresholds (every injury above zero shows, the health sends them in 5 % steps).
//!
//! The host sends every player's injuries with the public status (`public_status`), so a client
//! reads them from `Items::public`. This module finds the player aimed at (`health::aim_target`,
//! reach `MEDIC_REACH`), holds `MedicView` for the window to draw (`window_plugin`) and says "TREATED
//! BY" to the patient. The panel stays on a player while a treatment is held on them.
use crate::controls::Controls;
use crate::crew_hud::{col, col_a, dark, hud_text, HudFonts};
use crate::displays::{ActiveDisplay, DisplayLook};
use crate::env::PlanetRes;
use crate::health::{aim_target, HealthState, OwnVitals, UseHold};
use crate::items::{slot_rect, HudWindow, InventoryUi, Items};
use crate::net_live::RemoteWalker;
use crate::origin::WorldPose;
use crate::phases::{Frame, Fx};
use crate::walker::{Player, EYE_HEIGHT};
use bevy::prelude::*;
use display_core::target::{suit_hint, target_view, Injury, TargetIn, TargetLine, TargetView};
use display_core::{theme::Colours, Tone};
use item_core::wire::Doing;
use item_core::{PlayerId, BAR_SLOTS};
use walker_core::Frame as WalkFrame;

/// How far a player can look at another to see their injuries (m).
pub const MEDIC_REACH: f64 = 3.0;
/// The most injury lines the panel has rows for (burn, radiation, oxygen, down or unconscious).
const ROWS: usize = 4;

/// The name a player has in the HUD and over the figure (the name tags say the same).
pub fn pilot_name(id: PlayerId) -> String {
    format!("Pilot {id}")
}

/// The affliction a treatment reduces for each injury (`health.json`).
fn affliction_of(i: Injury) -> &'static str {
    match i {
        Injury::Burn => "burn",
        Injury::Radiation => "radiation",
        Injury::Oxygen => "oxygen_low",
    }
}

/// What the medic view shows (written every fixed step; headless too).
#[derive(Resource, Default, Clone, Debug, PartialEq)]
pub struct MedicView {
    /// The player the panel is about: the one aimed at, or the one a treatment is held on.
    pub target: Option<PlayerId>,
    pub view: Option<TargetView>,
    /// Action-bar slots whose treatment suits an injury of the target.
    pub suitable: Vec<usize>,
    /// Someone is treating this player (their name), for the patient's own HUD.
    pub treated_by: Option<String>,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<MedicView>();
    app.add_systems(FixedUpdate, look_out.in_set(Fx::Effects));
}

/// Window only: the panel, the outlines on suitable slots, the patient's line.
pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (build_panel, update_panel).chain().in_set(Frame::Hud));
}

/// The player the medic view is about, and what it shows of them. Only on foot: a player in a ship
/// cabin is not aimed at (the cabin's frame is not the world's).
#[allow(clippy::too_many_arguments)]
fn look_out(
    items: Res<Items>,
    st: Res<HealthState>,
    planet: Res<PlanetRes>,
    controls: Res<Controls>,
    active: Res<ActiveDisplay>,
    own: Res<OwnVitals>,
    hold: Res<UseHold>,
    players: Query<&Player>,
    remotes: Query<(&RemoteWalker, &WorldPose)>,
    mut view: ResMut<MedicView>,
) {
    let me = items.me();
    let status = |id: PlayerId| items.public.iter().find(|(p, _)| *p == id).map(|(_, s)| *s);
    let mut next = MedicView::default();
    next.treated_by = items.public.iter().find(|(who, s)| *who != me && s.doing == Doing::Treating(me)).map(|(who, _)| pilot_name(*who));
    let aimed = players
        .single()
        .ok()
        .filter(|pl| pl.ship.is_none() && !controls.free_cursor && !pl.seated && !pl.fly && active.using().is_none() && !own.last.is_some_and(|v| v.unconscious))
        .and_then(|pl| {
            let eye = pl.w.pos + pl.up * EYE_HEIGHT;
            let lying = |id: PlayerId| status(id).is_some_and(|s| s.down || s.unconscious);
            aim_target(eye, pl.world_look(WalkFrame::IDENTITY), &planet, &remotes, &lying, MEDIC_REACH)
        });
    // A treatment held on someone keeps the panel on them, aimed at or not.
    let target = hold.0.map(|h| h.target).filter(|t| *t != me).or(aimed).filter(|t| *t != me);
    if let Some(t) = target
        && let Some(s) = status(t)
    {
        let hurt = s.hurt;
        let tin = TargetIn {
            name: pilot_name(t),
            health: hurt.health as f64 / 100.0,
            down: s.down,
            unconscious: s.unconscious,
            burning: s.burning,
            burn_level: hurt.burn_level,
            radiation: hurt.radiation as f64 / 100.0,
            oxygen_low: hurt.oxygen_low as f64 / 100.0,
        };
        let injuries = tin.injuries();
        let mut hint = None;
        for i in 0..BAR_SLOTS {
            let Some(stack) = items.client.view.slot(i) else { continue };
            let Some(effect) = items.defs.of(stack).and_then(|d| d.use_effect.clone()) else { continue };
            let Some(inj) = injuries.iter().copied().find(|inj| st.tuning.suits(&effect, affliction_of(*inj))) else { continue };
            next.suitable.push(i);
            hint.get_or_insert_with(|| suit_hint(&items.name(stack), inj));
        }
        next.target = Some(t);
        next.view = Some(target_view(&tin, hint));
    }
    if *view != next {
        *view = next;
    }
}

/// A node of the panel, by its part (the rows and the outlines are numbered).
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum PanelPart {
    Root,
    Name,
    HealthText,
    Track,
    Fill,
    Row(usize),
    Label(usize),
    Value(usize),
    Hint,
    Outline(usize),
    TreatedBy,
}

/// Builds the panel and the outlines once the fonts and the look exist.
fn build_panel(mut commands: Commands, fonts: Option<Res<HudFonts>>, look: Option<Res<DisplayLook>>, existing: Query<(), With<PanelPart>>, mut built: Local<bool>) {
    let (Some(fonts), Some(look)) = (fonts, look) else { return };
    if *built || !existing.is_empty() {
        *built = true;
        return;
    }
    *built = true;
    let (k, h) = (&look.0.colours, &look.0.hud);
    let t = &h.target;
    let soft = |a: f64| col_a(h.soft, a);
    commands.spawn((PanelPart::Root, Node { position_type: PositionType::Absolute, display: Display::None, flex_direction: FlexDirection::Column, row_gap: px(4), width: px(t.width as f32), padding: UiRect::all(px(t.pad as f32)), ..default() }, BackgroundColor(dark(0.6)), GlobalZIndex(20))).with_children(|p| {
        p.spawn(Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, ..default() }).with_children(|r| {
            r.spawn((PanelPart::Name, hud_text(&fonts.bold, "", t.name_size, col(k.fg_title), h)));
            r.spawn((PanelPart::HealthText, hud_text(&fonts.mono, "", t.line_size, soft(0.85), h)));
        });
        p.spawn((PanelPart::Track, Node { width: percent(100.0), height: px(t.bar_h as f32), ..default() }, BackgroundColor(soft(h.vitals.track_alpha))))
            .with_children(|b| {
                b.spawn((PanelPart::Fill, Node { width: percent(0.0), height: percent(100.0), ..default() }, BackgroundColor(col(k.good))));
            });
        for i in 0..ROWS {
            p.spawn((PanelPart::Row(i), Node { flex_direction: FlexDirection::Row, justify_content: JustifyContent::SpaceBetween, display: Display::None, ..default() })).with_children(|r| {
                r.spawn((PanelPart::Label(i), hud_text(&fonts.mono, "", t.line_size, col(k.fg_1), h)));
                r.spawn((PanelPart::Value(i), hud_text(&fonts.mono, "", t.line_size, col(k.fg_1), h)));
            });
        }
        p.spawn((PanelPart::Hint, hud_text(&fonts.semibold, "", t.hint_size, col(k.accent), h), Node { display: Display::None, ..default() }));
    });
    for i in 0..BAR_SLOTS {
        commands.spawn((PanelPart::Outline(i), Node { position_type: PositionType::Absolute, display: Display::None, border: UiRect::all(px(t.outline_px as f32)), ..default() }, BorderColor::all(col_a(k.accent, t.outline_alpha)), GlobalZIndex(19)));
    }
    commands.spawn((PanelPart::TreatedBy, hud_text(&fonts.semibold, "", t.name_size, col(k.accent), h), Node { position_type: PositionType::Absolute, display: Display::None, ..default() }, GlobalZIndex(20)));
}

fn tone_colour(k: &Colours, tone: Tone) -> Color {
    match tone {
        Tone::Warn => col(k.warn),
        Tone::Bad => col(k.bad),
        Tone::Good => col(k.good),
        _ => col(k.fg_1),
    }
}

/// Every frame: the panel's place and text, the rows, the health bar, the outlines, the patient's line.
#[allow(clippy::type_complexity)]
fn update_panel(
    view: Res<MedicView>,
    look: Res<DisplayLook>,
    window: Res<HudWindow>,
    ui: Res<InventoryUi>,
    mut q: Query<(&PanelPart, &mut Node, Option<&mut Text>, Option<&mut TextColor>, Option<&mut BackgroundColor>)>,
) {
    let (k, h) = (&look.0.colours, &look.0.hud);
    let t = &h.target;
    let at = display_core::target::panel_at((window.0.x as f64, window.0.y as f64), t);
    let shown = view.view.as_ref();
    let lines: &[TargetLine] = shown.map_or(&[], |v| v.lines.as_slice());
    for (part, mut node, text, colour, bg) in &mut q {
        let show = |node: &mut Node, on: bool| {
            let d = if on { Display::Flex } else { Display::None };
            if node.display != d {
                node.display = d;
            }
        };
        match *part {
            PanelPart::Root => {
                show(&mut node, shown.is_some());
                if node.left != px(at.0 as f32) {
                    node.left = px(at.0 as f32);
                    node.top = px(at.1 as f32);
                }
            }
            PanelPart::Name => set(text, shown.map_or("", |v| v.name.as_str())),
            PanelPart::HealthText => set(text, shown.map_or("", |v| v.health_text.as_str())),
            PanelPart::Track => {}
            PanelPart::Fill => {
                let frac = shown.map_or(0.0, |v| v.health);
                let w = percent((frac * 100.0) as f32);
                if node.width != w {
                    node.width = w;
                }
                if let Some(mut bg) = bg {
                    let tone = if frac < h.vitals.bad_below { Tone::Bad } else if frac < h.vitals.warn_below { Tone::Warn } else { Tone::Good };
                    bg.0 = tone_colour(k, tone);
                }
            }
            PanelPart::Row(i) => show(&mut node, i < lines.len()),
            PanelPart::Label(i) => {
                let line = lines.get(i);
                set(text, line.map_or("", |l| l.label.as_str()));
                if let (Some(mut c), Some(l)) = (colour, line) {
                    c.0 = tone_colour(k, l.tone);
                }
            }
            PanelPart::Value(i) => set(text, lines.get(i).map_or("", |l| l.value.as_str())),
            PanelPart::Hint => {
                let hint = shown.and_then(|v| v.hint.as_deref());
                show(&mut node, hint.is_some());
                set(text, hint.unwrap_or(""));
            }
            PanelPart::Outline(i) => {
                let on = shown.is_some() && view.suitable.contains(&i);
                show(&mut node, on);
                if on {
                    let (pos, size) = slot_rect(i, ui.open, window.0, &h.bar);
                    if node.left != px(pos.x) {
                        node.left = px(pos.x - t.outline_px as f32);
                        node.top = px(pos.y - t.outline_px as f32);
                        node.width = px(size + 2.0 * t.outline_px as f32);
                        node.height = px(size + 2.0 * t.outline_px as f32);
                    }
                }
            }
            PanelPart::TreatedBy => {
                let by = view.treated_by.as_deref();
                show(&mut node, by.is_some());
                if by.is_some() {
                    let top = (h.compass.top + h.compass.height + 4.0) as f32;
                    if node.top != px(top) {
                        node.top = px(top);
                        node.left = percent(50.0);
                        node.margin = UiRect::left(px(-120.0));
                    }
                }
                set(text, &by.map_or(String::new(), |n| format!("TREATED BY {}", n.to_uppercase())));
            }
        }
    }
}

/// Changes the text only when it differs (a touched `Text` lays out again).
fn set(text: Option<Mut<Text>>, s: &str) {
    if let Some(mut t) = text
        && t.0 != s
    {
        t.0 = s.to_string();
    }
}
