//!   (TODO(initiator): confirm).
//! TODO(initiator): the glow of the critical text and the scanlines' look are not drawn yet.
use super::*;
use crate::displays::{ActiveDisplay, DisplayAction, DisplayPanel, DisplayPointer, Panels};
use crate::dev_area::bridge::captain_screen;
use crate::dev_area::{layout::{GUNNERY_AT, VENT_LIFT}};
use crate::dev_area::crew::bot_tagged;
use crate::dev_area::reactor::{lamp_blink_on, ReactorConfig};
use crate::dev_area::engineering::startup_screen;
use crate::dev_area::hull::status_monitor;
use crate::items::Items;
use display_core::{button, canvas, column, ctl, font, label, micro, readout, row, slider, space, toggle, Ink, Item, Panel, Rect, Shape, Tier, TextFace, Tone, Widget};
use item_core::wire::{AirlockCycle, BotStation, BreakerGroup, EnemyOutcome, FuelKind, FuelTube, JumpStage, Lamp, PumpMode, ReactorAsk, RoomAsk, RoomNews, ReactorFault, ReactorNews, FUEL_TUBES};

const LAMPS: [Lamp; Lamp::COUNT] = [Lamp::Hot, Lamp::CritHeat, Lamp::Cold, Lamp::LowTemp, Lamp::HighOut, Lamp::CritOut, Lamp::LowOut, Lamp::FuelLow, Lamp::NoFuel, Lamp::Meltdown, Lamp::Scram];
/// A lamp's cell (px) and the gap between the cells.
const LAMP_W: f64 = 96.0;
const LAMP_H: f64 = 20.0;
const LAMP_GAP: f64 = 3.0;

pub const READOUT_ID: PanelId = PanelId::new("reactor_readout");
pub const READOUT_NAME: &str = "REACTOR";
pub const PUMP_ID: PanelId = PanelId::new("coolant_pump");
pub const PUMP_NAME: &str = "COOLANT PUMP";
pub const JUNCTION_ID: PanelId = PanelId::new("junction_box");
pub const JUNCTION_NAME: &str = "JUNCTION BOX";
pub const AUX_ID: PanelId = PanelId::new("aux_panel");
pub const BATTERY_ID: PanelId = PanelId::new("battery_panel");
pub const BATTERY_NAME: &str = "BATTERY";
pub const SHIELD_ID: PanelId = PanelId::new("shield_panel");
pub const SHIELD_NAME: &str = "SHIELD";
pub const HELM_ID: PanelId = PanelId::new("helm_panel");
pub const HELM_NAME: &str = "HELM";
pub const GUNNERY_ID: PanelId = PanelId::new("gunnery_panel");
pub const GUNNERY_NAME: &str = "GUNNERY";
pub const JUMP_ID: PanelId = PanelId::new("jump_drive");
pub const JUMP_NAME: &str = "JUMP DRIVE";
pub const AUX_NAME: &str = "AUX";
pub const DOOR_ID: PanelId = PanelId::new("engine_door_screen");
pub const DOOR_NAME: &str = "REACTOR ROOM";
/// The airlock's two panels (r40): the inner door's (corridor side) and the outer door's (inside the airlock).
pub const AIRLOCK_INNER_ID: PanelId = PanelId::new("airlock_inner");
pub const AIRLOCK_INNER_NAME: &str = "INNER DOOR";
pub const AIRLOCK_OUTER_ID: PanelId = PanelId::new("airlock_outer");
pub const AIRLOCK_OUTER_NAME: &str = "OUTER DOOR";
pub const FAULTS_ID: PanelId = PanelId::new("reactor_faults");
pub const FAULTS_NAME: &str = "Dev faults";
/// The vents' panels (r38, r44): index 0 the engine room's, 1 the corridor's, 2 the bridge's (`ReactorAsk::SetVent`).
pub const VENT_ENGINE_ID: PanelId = PanelId::new("vent_engine");
pub const VENT_ENGINE_NAME: &str = "VENT ENGINE";
pub const VENT_CORRIDOR_ID: PanelId = PanelId::new("vent_corridor");
pub const VENT_CORRIDOR_NAME: &str = "VENT CORRIDOR";
pub const VENT_BRIDGE_ID: PanelId = PanelId::new("vent_bridge");
pub const VENT_BRIDGE_NAME: &str = "VENT BRIDGE";
/// The bridge door's panel (r44, BD-01): its toggle, beside the bridge door in the corridor.
pub const BRIDGE_DOOR_ID: PanelId = PanelId::new("bridge_door_screen");
pub const BRIDGE_DOOR_NAME: &str = "BRIDGE DOOR";
pub const STATUS_ID: PanelId = PanelId::new("reactor_status");
pub const STATUS_NAME: &str = "RX STATUS";
/// The dev faults screen's mount point (area metres, the engine room's north wall, the room side).
pub const FAULTS_AT: [f64; 2] = [-17.5, -11.04];

pub const HELP_ACTION: &str = "help";

/// Whether this player has the readout's help view open. Local UI state: it is not sent, and each
/// process draws its own panels. Closed when the player leaves the display (`write_panels`).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactorHelp(pub bool);

impl ReactorHelp {
    pub fn toggle(&mut self) {
        self.0 = !self.0;
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<ReactorHelp>();
    app.add_systems(FixedUpdate, (clear_jam_hold, read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
    app.add_systems(Update, (gun_flash, enemy_hit_flash));
}

/// GN-01's CLEAR JAM hold (r41): the pointer held on the button (`Pointer::pressed`, set on the press,
/// cleared on the release) sends `ClearJam(true)`, and the release `ClearJam(false)`. Sent on each
/// change only. A hold on another panel, or with no display in use, is a release.
pub fn clear_jam_hold(active: Res<ActiveDisplay>, pointer: Res<DisplayPointer>, displays: Query<&DisplayPanel>, mut items: ResMut<Items>, mut holding: Local<bool>) {
    let on = active.using().and_then(|e| displays.get(e).ok()).is_some_and(|d| d.id == GUNNERY_ID) && pointer.0.pressed.as_deref() == Some("clear_jam");
    if on != *holding {
        *holding = on;
        items.ask_reactor(ReactorAsk::ClearJam(on));
    }
}

/// A shot the GN-01 saw (r40): the shot count went down and the reload started. A reset (the count
/// falls with no reload) is no shot. The audio (`audio_slice`) and the flash both use it.
pub fn shot_fired(prev: Option<u8>, r: &ReactorNews) -> bool {
    prev.is_some_and(|p| r.weapon_shots < p) && r.weapon_reload_s > 0.0
}

/// The muzzle flash (r40): a short point light at the GN-01, fading out over `GUN_FLASH_S`. Ours,
/// no camera; the strength is a start value (TODO(initiator)).
pub const GUN_FLASH_S: f32 = 0.12;
pub const GUN_FLASH_PEAK: f32 = 40_000.0;

#[derive(Component)]
struct GunFlash {
    age: f32,
}

fn gun_flash(mut commands: Commands, time: Res<Time>, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, mut prev: Local<Option<u8>>, mut flashes: Query<(Entity, &mut GunFlash, &mut PointLight)>) {
    let dt = time.delta_secs();
    for (e, mut f, mut light) in &mut flashes {
        f.age += dt;
        if f.age >= GUN_FLASH_S {
            commands.entity(e).despawn();
        } else {
            light.intensity = GUN_FLASH_PEAK * (1.0 - f.age / GUN_FLASH_S);
        }
    }
    let Some(news) = items.reactor else {
        *prev = None;
        return;
    };
    let fired = shot_fired(*prev, &news);
    *prev = Some(news.weapon_shots);
    if !fired {
        return;
    }
    let Some(area) = area else { return };
    if let Some((at, _)) = area.ground(&planet, GUNNERY_AT, VENT_LIFT) {
        commands.spawn((PointLight { color: Color::srgb(1.0, 0.85, 0.5), intensity: GUN_FLASH_PEAK, range: 3.0, shadow_maps_enabled: false, ..default() }, Transform::default(), WorldPos(at), GunFlash { age: 0.0 }));
    }
}

/// An enemy shot the GN-01's news shows (r41): the shot count went up in an encounter that runs. A new
/// encounter starts its count at 0 and so is no shot. The audio (`audio_slice`) and the flash use it.
pub fn enemy_shot(prev: Option<u32>, r: &ReactorNews) -> bool {
    match (prev, r.enemy) {
        (Some(p), Some(e)) => e.shots > p,
        _ => false,
    }
}

/// An enemy shot's hit (r41): a light flash at the wall section it hit (`reactor_core::hull::SECTION_CENTRES`),
/// the muzzle flash's light and fade (`GunFlash`), no camera.
fn enemy_hit_flash(mut commands: Commands, items: Res<Items>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, mut prev: Local<Option<u32>>) {
    let Some(news) = items.reactor else {
        *prev = None;
        return;
    };
    let shot = enemy_shot(*prev, &news);
    *prev = news.enemy.map(|e| e.shots);
    if !shot {
        return;
    }
    let (Some(area), Some(e)) = (area, news.enemy) else { return };
    let centre = reactor_core::hull::SECTION_CENTRES[usize::from(e.section).min(reactor_core::hull::SECTIONS - 1)];
    if let Some((at, _)) = area.ground(&planet, centre, VENT_LIFT) {
        commands.spawn((PointLight { color: Color::srgb(0.6, 0.8, 1.0), intensity: GUN_FLASH_PEAK, range: 3.0, shadow_maps_enabled: false, ..default() }, Transform::default(), WorldPos(at), GunFlash { age: 0.0 }));
    }
}

// ---------- the gauge's geometry ----------

/// The arc's start (bottom left) and its sweep (270 degrees, clockwise on screen).
const ARC_START: f64 = 135.0;
const ARC_SWEEP: f64 = 270.0;

/// The angle of a value 0..100 on the arc.
fn angle(v: f64) -> f64 {
    ARC_START + ARC_SWEEP * v.clamp(0.0, 100.0) / 100.0
}

/// The temperature zones of the sim's bands: `Good` is the optimal band, `Low` and `High` the
/// rest of the allowed band, `Crit` above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Cold,
    Low,
    Good,
    High,
    Crit,
}

/// The bands `(optimal, allowed)` the screens show: the news' (the sim's, from the operator's
/// skill). Without news the skill-50 bands, so the screens stay readable.
pub fn bands(r: Option<&ReactorNews>) -> ([f64; 2], [f64; 2]) {
    match r {
        Some(r) if r.optimal[1] > r.optimal[0] => (r.optimal, r.allowed),
        _ => (SKILL_50_OPTIMAL, SKILL_50_ALLOWED),
    }
}

/// The sim's bands at skill 50 (`reactor_core::Bands::new`).
const SKILL_50_OPTIMAL: [f64; 2] = [35.0, 65.0];
const SKILL_50_ALLOWED: [f64; 2] = [20.0, 80.0];

pub fn zone(t: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Zone {
    if t > allowed[1] {
        Zone::Crit
    } else if t > optimal[1] {
        Zone::High
    } else if t >= optimal[0] {
        Zone::Good
    } else if t >= allowed[0] {
        Zone::Low
    } else {
        Zone::Cold
    }
}

pub fn zone_tone(z: Zone) -> Tone {
    match z {
        Zone::Cold => Tone::Dim,
        Zone::Low | Zone::High => Tone::Warn,
        Zone::Good => Tone::Good,
        Zone::Crit => Tone::Bad,
    }
}

fn arc(cx: f64, cy: f64, r: f64, from: f64, to: f64, stroke: f64, ink: Ink, dash: Option<(f64, f64)>) -> Shape {
    Shape::Arc { cx, cy, r, from, to, stroke, ink, dash }
}

fn temp_gauge(temp: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Item {
    let (cx, cy, r, sw) = (108.0, 70.0, 60.0, 14.0);
    let z = zone(temp, optimal, allowed);
    let t = temp.clamp(0.0, 100.0);
    let mut s = vec![
        arc(cx, cy, r, angle(0.0), angle(allowed[0]), sw, Ink::Line2, None),
        arc(cx, cy, r, angle(allowed[0]), angle(optimal[0]), sw, Ink::Warn, Some((9.0, 4.0))),
        arc(cx, cy, r, angle(optimal[0]), angle(optimal[1]), sw, Ink::Good, None),
        arc(cx, cy, r, angle(optimal[1]), angle(allowed[1]), sw, Ink::Warn, Some((9.0, 4.0))),
        arc(cx, cy, r, angle(allowed[1]), angle(100.0), sw, Ink::Bad, Some((3.0, 2.0))),
    ];
    let a = angle(t).to_radians();
    s.push(Shape::Line { a: [cx, cy], b: [cx + 46.0 * a.cos(), cy + 46.0 * a.sin()], width: 5.0, ink: Ink::FgTitle, dash: None });
    s.push(Shape::Circle { cx, cy, r: 8.0, fill: Some(Ink::Bg0), stroke: Some((Ink::FgTitle, 3.0)) });
    s.push(text(0.0, 74.0, 216.0, 28.0, &format!("{temp:.0} °C"), 28.0, TextFace::Mono, zone_ink(z), true));
    s.push(text(0.0, 124.0, 216.0, 20.0, "CORE TEMP", 16.0, TextFace::Display, Ink::HudDim, true));
    canvas(216.0, 152.0, s, None).fixed(216.0)
}

fn zone_ink(z: Zone) -> Ink {
    match z {
        Zone::Cold => Ink::Fg2,
        Zone::Low | Zone::High => Ink::Warn,
        Zone::Good => Ink::Good,
        Zone::Crit => Ink::Bad,
    }
}

fn power_gauge(r: Option<&ReactorNews>, max_kw: f64) -> Item {
    let (cx, cy, rr, sw) = (108.0, 70.0, 60.0, 14.0);
    let (out, load) = r.map_or((0.0, 0.0), |r| (r.output_kw, r.load_kw));
    let frac = (out / max_kw * 100.0).clamp(0.0, 100.0);
    let ink = power_ink(out, load);
    let mut s = vec![arc(cx, cy, rr, angle(0.0), angle(100.0), sw, Ink::Bg3, None)];
    if frac > 0.0 {
        s.push(arc(cx, cy, rr, angle(0.0), angle(frac), sw, ink, None));
    }
    let n = (load / max_kw * 100.0).clamp(0.0, 100.0);
    let (a, p0, p1) = (angle(n).to_radians(), 50.0, 66.0);
    s.push(Shape::Line { a: [cx + p0 * a.cos(), cy + p0 * a.sin()], b: [cx + p1 * a.cos(), cy + p1 * a.sin()], width: 4.0, ink: Ink::FgTitle, dash: None });
    s.push(text(0.0, 62.0, 216.0, 32.0, &format!("{out:.0}"), 28.0, TextFace::Mono, ink, true));
    s.push(text(0.0, 98.0, 216.0, 18.0, &format!("LOAD {load:.0}"), 16.0, TextFace::Mono, Ink::Fg2, true));
    s.push(text(0.0, 124.0, 216.0, 20.0, "POWER kW", 16.0, TextFace::Display, Ink::HudDim, true));
    canvas(216.0, 152.0, s, None).fixed(216.0)
}

pub fn power_ink(out: f64, load: f64) -> Ink {
    if out <= 0.0 {
        return Ink::Fg3;
    }
    if load <= 0.0 {
        return Ink::Good;
    }
    let short = (load - out) / load;
    if short <= 0.05 {
        Ink::Good
    } else if short <= 0.40 {
        Ink::Warn
    } else {
        Ink::Bad
    }
}

fn fuel(r: Option<&ReactorNews>) -> Item {
    let tubes = r.map_or([FuelTube::default(); FUEL_TUBES], |r| r.fuel);
    let mut s = Vec::new();
    for (i, t) in tubes.iter().enumerate() {
        let x = 12.0 + 40.0 * i as f64;
        let (top, h) = (10.0, 100.0);
        let fill = t.rod.is_some().then_some(Ink::Bg3);
        s.push(Shape::Rect { x, y: top, w: 26.0, h, fill, outline: Some((Ink::HudDim, 2.0)) });
        let fill_h = h * (t.life / 100.0).clamp(0.0, 1.0);
        let fy = top + h - fill_h;
        if let Some(kind) = t.rod.filter(|_| fill_h > 0.0) {
            if t.life <= 10.0 {
                s.push(Shape::Hatch { x: x + 2.0, y: fy, w: 22.0, h: fill_h, ink: Ink::Bad });
            } else if t.life <= 25.0 {
                s.push(Shape::Hatch { x: x + 2.0, y: fy, w: 22.0, h: fill_h, ink: Ink::Warn });
            } else {
                s.push(Shape::Rect { x: x + 2.0, y: fy, w: 22.0, h: fill_h, fill: Some(tube_ink(kind)), outline: None });
            }
        }
        s.push(text(x - 7.0, 116.0, 40.0, 16.0, &format!("R{}", i + 1), 16.0, TextFace::Mono, Ink::Fg2, true));
    }
    s.push(text(0.0, 136.0, 176.0, 16.0, "FUEL", 16.0, TextFace::Display, Ink::HudDim, true));
    canvas(176.0, 152.0, s, None).flex(1.0)
}

/// A rod kind's colour on the tube (the palette has no magenta: spicy takes the danger red,
/// TODO(initiator): a colour of its own in `display_core`).
fn tube_ink(kind: FuelKind) -> Ink {
    match kind {
        FuelKind::Standard => Ink::Good,
        FuelKind::Hot => Ink::Warn,
        FuelKind::Spicy => Ink::Bad,
    }
}

fn text(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, face: TextFace, ink: Ink, centred: bool) -> Shape {
    Shape::Text { x, y, w, h, text: s.to_string(), size, face, ink, centred }
}

pub fn reactor_status(r: Option<&ReactorNews>) -> (&'static str, Tone) {
    match r {
        Some(r) if r.ejected => ("NO CORE", Tone::Dim),
        Some(r) if r.power_on => match zone(r.temp, r.optimal, r.allowed) {
            Zone::Crit => ("CRITICAL", Tone::Bad),
            Zone::High => ("HOT", Tone::Warn),
            _ => ("STABLE", Tone::Good),
        },
        _ => ("OFF", Tone::Dim),
    }
}

fn hazard(w: f64) -> Item {
    canvas(w, 14.0, vec![Shape::Rect { x: 0.0, y: 0.0, w, h: 14.0, fill: Some(Ink::Bg0), outline: None }, Shape::Hatch { x: 0.0, y: 0.0, w, h: 14.0, ink: Ink::Bad }], None).fixed(14.0)
}

/// until a repair brings it back (`Reactor::melted`).
fn meltdown_view() -> Panel {
    let word = canvas(624.0, 200.0, vec![text(0.0, 60.0, 624.0, 72.0, "MELTDOWN", 56.0, TextFace::DisplayBold, Ink::Bad, true)], None).fixed(200.0);
    Panel::screen(Tier::M, READOUT_NAME, "RX-01", column(8.0, vec![hazard(624.0), word, hazard(624.0)])).status("MELTDOWN", Tone::Bad).with_padding(8.0)
}

/// The slider track's width under the readout (the bottom-left column: 608 - 16 - 200), and the band
/// strip's height. The strip sits right under its slider, as wide as the track, so the 0..100 scale matches.
pub const STRIP_W: f64 = 392.0;
pub const STRIP_H: f64 = 6.0;

/// The wobble of the fission needle (Barotrauma's jitter, with a sine sum for its Perlin noise): within
/// 5 of the allowed top, the needle moves by up to half of the set point's distance over the top (at
/// most 10 / 2). Deterministic in `t` (the display time, seconds).
pub fn fission_wobble(r: &ReactorNews, t: f64) -> f64 {
    let top = r.fission_allowed[1];
    if r.fission <= top - 5.0 {
        return 0.0;
    }
    let amount = (r.set_fission - top).clamp(0.0, 10.0);
    amount * 0.5 * ((t * 9.0).sin() + (t * 13.7).sin()) * 0.5
}

/// the actual value as a marker. A band with `hi <= lo` (no news) is not drawn.
fn band_strip(optimal: [f64; 2], allowed: [f64; 2], actual: Option<f64>) -> Item {
    let at = |v: f64| v.clamp(0.0, 100.0) / 100.0 * STRIP_W;
    let span = |b: [f64; 2]| (at(b[0]), at(b[1]) - at(b[0]));
    let mut shapes = Vec::new();
    if allowed[1] > allowed[0] {
        let (x, w) = span(allowed);
        shapes.push(Shape::Rect { x, y: 0.0, w, h: 4.0, fill: Some(Ink::Warn), outline: None });
    }
    if optimal[1] > optimal[0] {
        let (x, w) = span(optimal);
        shapes.push(Shape::Rect { x, y: 0.0, w, h: 4.0, fill: Some(Ink::Good), outline: None });
    }
    if let Some(v) = actual {
        // Kept inside the strip's box at the two ends (a marker at 0 or 100 would hang over by a pixel).
        shapes.push(Shape::Rect { x: (at(v) - 1.0).clamp(0.0, STRIP_W - 2.0), y: 0.0, w: 2.0, h: STRIP_H, fill: Some(Ink::FgTitle), outline: None });
    }
    canvas(STRIP_W, STRIP_H, shapes, None).fixed(STRIP_H)
}

/// The arrow is ASCII: the mono font has no U+2192.
fn set_and_actual(set: f64, actual: f64, manual: bool) -> Item {
    row(
        6.0,
        vec![
            readout(format!("{set:.0} %")).text_size(font::LABEL).tone(if manual { Tone::Normal } else { Tone::Dim }).flex(1.0),
            label(format!("-> {actual:.0}")).mono().text_size(font::SMALL).tone(Tone::Dim).flex(1.0),
        ],
    )
}

/// The readout panel; `max_kw` is the scale of the power gauge (the reactor's `max_output_kw`).
pub fn readout_panel(r: Option<&ReactorNews>, max_kw: f64) -> Panel {
    readout_panel_at(r, max_kw, 0.0)
}

pub fn readout_panel_at(r: Option<&ReactorNews>, max_kw: f64, t: f64) -> Panel {
    if r.is_some_and(|r| r.melted) {
        return meltdown_view();
    }
    let on = r.is_some_and(|r| r.power_on);
    let manual = r.is_some_and(|r| r.manual);
    let (fis, tur) = r.map_or((0.0, 0.0), |r| (r.set_fission, r.set_turbine));
    let (fis_act, tur_act) = r.map_or((0.0, 0.0), |r| (r.fission, r.turbine));
    let (word, tone) = reactor_status(r);
    let zero = [0.0; 2];
    let (fis_opt, fis_all, tur_opt, tur_all) = r.map_or((zero, zero, zero, zero), |r| (r.fission_optimal, r.fission_allowed, r.turbine_optimal, r.turbine_allowed));
    let fis_needle = r.map(|r| (r.fission + fission_wobble(r, t)).clamp(0.0, 100.0));
    // Critical: the stripe is as wide as the bottom-left column (624 content - 16 gap - 200 switches)
    // and the spacing tightens by 18 px so the stripe fits (TODO(initiator): the readout's height budget).
    let critical = crate::effects::is_critical(r);
    // The two strips take 12 px of the column's 184: the gaps give it back (10 -> 7, and 6 -> 3 when critical).
    let (filler, gap) = if critical { (hazard(408.0), 3.0) } else { (space(), 7.0) };
    let track = |s: Item, strip: Item| column(0.0, vec![s, strip]).fixed(ctl::MD + STRIP_H);
    let bottom_left = column(
        gap,
        vec![
            row(8.0, vec![micro("FISSION RATE").flex(1.0), set_and_actual(fis, fis_act, manual)]).fixed(24.0),
            track(slider(fis / 100.0, "", "fis_set").enabled(manual), band_strip(fis_opt, fis_all, fis_needle)),
            row(8.0, vec![micro("TURBINE OUTPUT").flex(1.0), set_and_actual(tur, tur_act, manual)]).fixed(24.0),
            track(slider(tur / 100.0, "", "tur_set").enabled(manual), band_strip(tur_opt, tur_all, r.map(|r| r.turbine))),
            filler,
        ],
    );
    let switches = column(
        8.0,
        vec![
            toggle(manual, "", "mode").labels("AUTO", "MAN").fixed(ctl::MD),
            toggle(on, "", "power").labels("OFF", "ON").fixed(ctl::MD),
            button("?", HELP_ACTION).fixed(ctl::LG),
        ],
    );
    let (optimal, allowed) = bands(r);
    let gauges = row(8.0, vec![temp_gauge(r.map_or(0.0, |r| r.temp), optimal, allowed), power_gauge(r, max_kw), fuel(r)]).fixed(152.0);
    let body = column(if critical { 4.0 } else { 8.0 }, vec![gauges, row(16.0, vec![bottom_left.flex(1.0), switches.fixed(200.0)]).flex(1.0)]);
    Panel::screen(Tier::M, READOUT_NAME, "RX-01", body).status(word, tone).with_padding(8.0)
}

// ---------- the help view ----------

const CALLOUT_W: f64 = 256.0;
const CALLOUT_H: f64 = 44.0;
const CALLOUT_SIZE: f64 = 14.0;
const LEFT: f64 = 0.0;
const RIGHT: f64 = 352.0;

/// What a callout points at: a leaf of the readout by its place in the layout (the three gauges come
/// first), or a control by its action.
#[derive(Clone, Copy)]
enum Target {
    Leaf(usize),
    Action(&'static str),
}

const CALLOUTS: &[(f64, f64, [&str; 2], Target)] = &[
    (LEFT, 4.0, ["keep the needle in the green", "red too long: core goes bang"], Target::Leaf(0)),
    (RIGHT, 4.0, ["what you make", "vs. what the ship wants"], Target::Leaf(1)),
    (RIGHT, 104.0, ["rods; pull one in a pinch", ""], Target::Leaf(2)),
    (LEFT, 152.0, ["how hard the atoms argue", ""], Target::Action("fis_set")),
    (LEFT, 252.0, ["how much steam makes power", "and how much makes cooling"], Target::Action("tur_set")),
    (RIGHT, 152.0, ["AUTO: the ship has the dial", "MAN: your hand, your fault"], Target::Action("mode")),
    (RIGHT, 200.0, ["the big on switch", "nothing turns without it"], Target::Action("power")),
];
const CLOSE_AT: (f64, f64) = (LEFT, 104.0);

fn dim_ink(i: Ink) -> Ink {
    match i {
        Ink::Hud | Ink::HudDim => Ink::HudDim,
        Ink::Good | Ink::GoodDim => Ink::GoodDim,
        Ink::Warn | Ink::WarnDim => Ink::WarnDim,
        Ink::Bad | Ink::BadDim => Ink::BadDim,
        Ink::FgTitle | Ink::Fg1 | Ink::Fg2 | Ink::Fg3 => Ink::Fg3,
        Ink::Line1 | Ink::Line2 => Ink::Line2,
        Ink::Bg0 | Ink::Bg1 | Ink::Bg3 => i,
    }
}

/// A shape moved by (dx, dy) and dimmed. A text box is cut at `right` (the content's width): the fuel's
/// captions are drawn past its 160 px box, and the help view must keep them inside its picture.
fn shifted_dim(s: &Shape, dx: f64, dy: f64, right: f64) -> Shape {
    let mut t = s.clone();
    match &mut t {
        Shape::Arc { cx, cy, ink, .. } => {
            *cx += dx;
            *cy += dy;
            *ink = dim_ink(*ink);
        }
        Shape::Line { a, b, ink, .. } => {
            a[0] += dx;
            a[1] += dy;
            b[0] += dx;
            b[1] += dy;
            *ink = dim_ink(*ink);
        }
        Shape::Rect { x, y, fill, outline, .. } => {
            *x += dx;
            *y += dy;
            *fill = fill.map(dim_ink);
            *outline = outline.map(|(i, w)| (dim_ink(i), w));
        }
        Shape::Hatch { x, y, ink, .. } => {
            *x += dx;
            *y += dy;
            *ink = dim_ink(*ink);
        }
        Shape::Text { x, y, w, ink, .. } => {
            *x += dx;
            *y += dy;
            *w = w.min(right - *x).max(0.0);
            *ink = dim_ink(*ink);
        }
        Shape::Circle { cx, cy, fill, stroke, .. } => {
            *cx += dx;
            *cy += dy;
            *fill = fill.map(dim_ink);
            *stroke = stroke.map(|(i, w)| (dim_ink(i), w));
        }
    }
    t
}

fn dimmed_leaf(w: &Widget, r: Rect, o: DVec2, right: f64, out: &mut Vec<Shape>) {
    let (x, y, bw, bh) = (r.min.x - o.x, r.min.y - o.y, r.size.x, r.size.y);
    let frame = Shape::Rect { x, y, w: bw, h: bh, fill: None, outline: Some((Ink::Line2, 2.0)) };
    match w {
        Widget::Canvas { shapes, .. } => out.extend(shapes.iter().map(|s| shifted_dim(s, x, y, right))),
        Widget::Label { text, size, mono, .. } => out.push(text_shape(x, y, bw, bh, text, *size, *mono, false)),
        Widget::Button { label, .. } => {
            out.push(frame);
            out.push(text_shape(x, y, bw, bh, label, font::LABEL.min(bh * 0.55), false, true));
        }
        Widget::Toggle { labels, .. } => {
            out.push(frame);
            out.push(text_shape(x, y, bw / 2.0, bh, &labels[0], font::SMALL, false, true));
            out.push(text_shape(x + bw / 2.0, y, bw / 2.0, bh, &labels[1], font::SMALL, false, true));
        }
        Widget::Slider { value, .. } => {
            out.push(frame);
            // The marker's stroke is 3 px wide: at the ends it stays 2 px inside the box.
            let at = x + (bw * value.clamp(0.0, 1.0)).clamp(2.0, bw - 2.0);
            out.push(Shape::Line { a: [at, y], b: [at, y + bh], width: 3.0, ink: Ink::Fg3, dash: None });
        }
        _ => {}
    }
}

fn text_shape(x: f64, y: f64, w: f64, h: f64, s: &str, size: f64, mono: bool, centred: bool) -> Shape {
    let face = if mono { TextFace::Mono } else { TextFace::Display };
    Shape::Text { x, y, w, h, text: s.to_string(), size, face, ink: Ink::Fg3, centred }
}

/// One callout: the pointer line from the box's centre to the target's centre (drawn first, so the box
/// covers its start), then the box with its two lines.
fn callout_shapes(x: f64, y: f64, lines: &[&str; 2], target: Rect, o: DVec2, out: &mut Vec<Shape>) {
    let from = DVec2::new(x + CALLOUT_W / 2.0, y + CALLOUT_H / 2.0);
    let to = target.centre() - o;
    out.push(Shape::Line { a: [from.x, from.y], b: [to.x, to.y], width: 2.0, ink: Ink::Hud, dash: None });
    out.push(Shape::Circle { cx: to.x, cy: to.y, r: 4.0, fill: Some(Ink::Hud), stroke: None });
    out.push(Shape::Rect { x, y, w: CALLOUT_W, h: CALLOUT_H, fill: Some(Ink::Bg0), outline: Some((Ink::Hud, 2.0)) });
    for (i, line) in lines.iter().enumerate() {
        let ly = y + 5.0 + i as f64 * 18.0;
        out.push(Shape::Text { x: x + 8.0, y: ly, w: CALLOUT_W - 16.0, h: 18.0, text: line.to_string(), size: CALLOUT_SIZE, face: TextFace::Display, ink: Ink::FgTitle, centred: false });
    }
}

/// a CLOSE box. The whole picture is the close button (`HELP_ACTION`), so a click anywhere closes it.
pub fn readout_help_panel(r: Option<&ReactorNews>, max_kw: f64) -> Panel {
    let base = readout_panel(r, max_kw);
    if r.is_some_and(|r| r.melted) {
        return base;
    }
    let c = base.content();
    let o = c.min;
    let leaves = base.layout();
    let mut shapes = Vec::new();
    for w in &leaves {
        dimmed_leaf(w.widget, w.rect, o, c.size.x, &mut shapes);
    }
    for &(x, y, lines, target) in CALLOUTS {
        let rect = match target {
            Target::Leaf(i) => leaves[i].rect,
            Target::Action(a) => base.find(a).map(|p| p.rect).unwrap_or(c),
        };
        callout_shapes(x, y, &lines, rect, o, &mut shapes);
    }
    let (cx, cy) = CLOSE_AT;
    shapes.push(Shape::Rect { x: cx, y: cy, w: CALLOUT_W, h: CALLOUT_H, fill: Some(Ink::Bg0), outline: Some((Ink::Warn, 2.0)) });
    shapes.push(Shape::Text { x: cx, y: cy, w: CALLOUT_W, h: CALLOUT_H, text: "CLOSE".into(), size: font::LABEL, face: TextFace::DisplayBold, ink: Ink::Warn, centred: true });
    let mut panel = base;
    panel.root = canvas(c.size.x, c.size.y, shapes, Some(HELP_ACTION)).flex(1.0);
    panel
}

pub fn no_power(title: &str, tag: &str, tier: Tier) -> Panel {
    let size = tier.canvas();
    let (w, h) = (size.x - 40.0, size.y - 80.0);
    // A ring above the word (drawn, not a glyph: the display font has no ○).
    let ring = Shape::Circle { cx: w * 0.5, cy: h * 0.5 - 36.0, r: 14.0, fill: None, stroke: Some((Ink::Fg3, 3.0)) };
    let shapes = vec![Shape::Rect { x: 0.0, y: 0.0, w, h, fill: Some(Ink::Bg0), outline: None }, ring, text(0.0, 12.0, w, h - 12.0, "NO POWER", 28.0, TextFace::DisplayBold, Ink::Fg3, true)];
    Panel::screen(tier, title, tag, canvas(w, h, shapes, None).flex(1.0))
}

fn powered(r: Option<&ReactorNews>) -> bool {
    r.is_some_and(|r| r.powered)
}

/// The pump's status word and its tone: OFF, LOW or HIGH by the mode, DRY with an empty tank (the
/// column is 60 px: "NO COOLANT" does not fit it).
pub fn pump_status(mode: PumpMode, coolant: f64) -> (&'static str, Tone) {
    match mode {
        PumpMode::Off => ("OFF", Tone::Dim),
        _ if coolant <= 0.0 => ("DRY", Tone::Bad),
        PumpMode::Low => ("LOW", Tone::Normal),
        PumpMode::High => ("HIGH", Tone::Good),
    }
}

fn pump_modes(mode: PumpMode, r: &ReactorNews) -> Item {
    let mode_button = |label: &str, action: &str, m: PumpMode| button(label, action).tone(if mode == m { Tone::Good } else { Tone::Normal });
    // E-COOL is the fourth cell: the panel has no room for a row of its own (see `ecool_button`).
    row(6.0, vec![mode_button("OFF", "pump_off", PumpMode::Off).flex(1.0), mode_button("LOW", "pump_low", PumpMode::Low).flex(1.0), mode_button("HIGH", "pump_high", PumpMode::High).flex(1.0), ecool_button(r).flex(1.4)]).fixed(ctl::MD)
}

/// when a press would be taken, the cooldown as m:ss greyed while it runs (TODO(initiator): the
/// label "E-COOL 0:42" does not fit a cell of this row), COOL greyed while the extra cooling runs.
pub fn ecool_button(r: &ReactorNews) -> Item {
    if r.ecool_active {
        return button("COOL", "ecool").tone(Tone::Good).enabled(false);
    }
    if r.ecool_ready_in_s > 0.0 {
        let s = r.ecool_ready_in_s.ceil() as u32;
        return button(format!("{}:{:02}", s / 60, s % 60), "ecool").enabled(false);
    }
    button("E-COOL", "ecool").enabled(r.ecool_ok)
}

fn pump_picture(flow_on: bool, angle: f64) -> Item {
    let fluid = if flow_on { Ink::Fg1 } else { Ink::Fg3 };
    let (cx, cy) = (76.0, 60.0);
    let mut s = vec![
        Shape::Rect { x: 0.0, y: 44.0, w: 44.0, h: 16.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
        Shape::Line { a: [1.5, 52.0], b: [44.0, 52.0], width: 3.0, ink: fluid, dash: Some((6.0, 4.0)) },
        Shape::Rect { x: 70.0, y: 0.0, w: 14.0, h: 22.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
        Shape::Rect { x: 70.0, y: 0.0, w: 82.0, h: 14.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
        Shape::Line { a: [76.0, 20.0], b: [76.0, 7.0], width: 3.0, ink: fluid, dash: Some((6.0, 4.0)) },
        Shape::Line { a: [76.0, 7.0], b: [150.5, 7.0], width: 3.0, ink: fluid, dash: Some((6.0, 4.0)) },
        Shape::Circle { cx, cy, r: 40.0, fill: Some(Ink::Bg1), stroke: Some((Ink::Hud, 3.0)) },
    ];
    for k in 0..6 {
        let a = (k as f64 * 60.0 + angle).to_radians();
        s.push(Shape::Line { a: [cx, cy], b: [cx + 30.0 * a.cos(), cy + 30.0 * a.sin()], width: 6.0, ink: Ink::Hud, dash: None });
    }
    s.push(Shape::Circle { cx, cy, r: 9.0, fill: Some(Ink::Hud), stroke: None });
    canvas(152.0, 109.0, s, None).fixed(152.0)
}

pub fn pump_panel(r: Option<&ReactorNews>) -> Panel {
    pump_panel_at(r, 0.0, true)
}

/// `blink_on` is the lamps' blink (`lamp_blink_on`): the LEAK word shows on its lit half.
pub fn pump_panel_at(r: Option<&ReactorNews>, angle: f64, blink_on: bool) -> Panel {
    if !powered(r) {
        return no_power(PUMP_NAME, "CP-01", Tier::S);
    }
    let r = r.copied().unwrap_or_default();
    let (word, tone) = pump_status(r.pump_mode, r.coolant);
    let body = column(
        8.0,
        vec![
            row(
                10.0,
                vec![
                    pump_picture(r.pump_on && r.flow > 0.0, angle),
                    column(4.0, vec![micro("FLOW").fixed(14.0), readout(format!("{:.0}", r.flow * 100.0)).text_size(font::LABEL).tone(tone).fixed(46.0), readout(word).text_size(font::LABEL).tone(tone)]).flex(1.0),
                    coolant_tube(&r, blink_on).fixed(60.0),
                ],
            )
            .fixed(109.0),
            pump_modes(r.pump_mode, &r),
            space(),
        ],
    );
    Panel::screen(Tier::S, PUMP_NAME, "CP-01", body).with_padding(6.0)
}

/// fill) and EMPTY at none (the word in bad). The sim's level and line (`ReactorNews::coolant`).
/// A leak blinks LEAK in the word's place (not over EMPTY).
fn coolant_tube(r: &ReactorNews, blink_on: bool) -> Item {
    let (x, top, w, h) = (14.0, 16.0, 32.0, 70.0);
    let mut s = vec![text(0.0, 0.0, 60.0, 14.0, "COOL", 14.0, TextFace::Display, Ink::HudDim, true), Shape::Rect { x, y: top, w, h, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    let fill_h = h * (r.coolant / 100.0).clamp(0.0, 1.0);
    if fill_h > 0.0 {
        let fy = top + h - fill_h;
        if r.coolant_low {
            s.push(Shape::Hatch { x: x + 2.0, y: fy, w: w - 4.0, h: fill_h, ink: Ink::Warn });
        } else {
            s.push(Shape::Rect { x: x + 2.0, y: fy, w: w - 4.0, h: fill_h, fill: Some(Ink::Good), outline: None });
        }
    }
    let (word, ink) = if r.coolant <= 0.0 {
        ("EMPTY", Ink::Bad)
    } else if r.coolant_leak > 0.0 && blink_on {
        ("LEAK", Ink::Warn)
    } else if r.coolant_low {
        ("LOW", Ink::Warn)
    } else {
        ("", Ink::Fg2)
    };
    if !word.is_empty() {
        s.push(text(0.0, 90.0, 60.0, 16.0, word, 16.0, TextFace::DisplayBold, ink, true));
    }
    canvas(60.0, 109.0, s, None)
}

/// A lever's track and handle, px. The room's levers use `LEVER`; the junction box packs six of
/// them on one screen (r39: two rows of three in the same 260 px), so its track is `LEVER_PACKED`.
/// TODO(initiator): the packed look, not seen in play yet.
#[derive(Clone, Copy)]
struct Lever {
    track: f64,
    handle: f64,
}

const LEVER: Lever = Lever { track: 124.0, handle: 34.0 };
const LEVER_PACKED: Lever = Lever { track: 24.0, handle: 20.0 };

fn lever_in(w: f64, geo: Lever, lines: &[&str], on: bool, action: &str, name_ink: Ink) -> Item {
    // The names are set in lines of 17 px, bottom-aligned on the slot's top (y 36): a name too wide
    // for the lever is split over two lines rather than cut.
    let cx = w / 2.0;
    let mut s: Vec<Shape> = lines.iter().enumerate().map(|(i, l)| text(0.0, 36.0 - 17.0 * (lines.len() - i) as f64, w, 17.0, l, 16.0, TextFace::Display, name_ink, true)).collect();
    s.push(Shape::Rect { x: cx - 11.0, y: 36.0, w: 22.0, h: geo.track, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) });
    // The handle sits at the track's top when on, its bottom when off.
    let hy = if on { 36.0 } else { 36.0 + geo.track - geo.handle };
    let ink = if on { Ink::Hud } else { Ink::Warn };
    s.push(Shape::Rect { x: cx - 24.0, y: hy, w: 48.0, h: geo.handle, fill: Some(ink), outline: Some((Ink::FgTitle, 2.0)) });
    s.push(text(cx - 24.0, hy, 48.0, geo.handle, if on { "ON" } else { "OFF" }, 16.0, TextFace::DisplayBold, Ink::Bg0, true));
    canvas(w, 44.0 + geo.track, s, Some(action)).flex(1.0)
}

pub const TALL_CANVAS: f64 = 320.0;

pub fn tall(mut p: Panel) -> Panel {
    p.size.y = TALL_CANVAS;
    p
}

/// A group's bar under its lever (94 wide, 18 high): the fill is `delivered / demand` (0 with no demand)
fn group_bar(delivered: f64, demand: f64, limit: f64) -> Item {
    let (w, top, h) = (94.0, 6.0, 12.0);
    let frac = if demand > 0.0 { (delivered / demand).clamp(0.0, 1.0) } else { 0.0 };
    let mut s = vec![Shape::Rect { x: 0.0, y: top, w, h, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if frac > 0.0 {
        s.push(Shape::Rect { x: 2.0, y: top + 2.0, w: (w - 4.0) * frac, h: h - 4.0, fill: Some(Ink::Hud), outline: None });
    }
    let cx = (limit.clamp(0.0, 1.0) * w).clamp(5.0, w - 5.0);
    // The sides start 1 px down: a 2 px stroke at the canvas's top edge would stick out of it.
    s.push(Shape::Line { a: [cx - 4.0, 1.0], b: [cx, top], width: 2.0, ink: Ink::FgTitle, dash: None });
    s.push(Shape::Line { a: [cx + 4.0, 1.0], b: [cx, top], width: 2.0, ink: Ink::FgTitle, dash: None });
    canvas(w, 18.0, s, None).fixed(18.0)
}

fn total_bar(total: f64, max_kw: f64) -> Item {
    let (w, bar_x, bar_w) = (292.0, 60.0, 140.0);
    let frac = if max_kw > 0.0 { (total / max_kw).clamp(0.0, 1.0) } else { 0.0 };
    let mut s = vec![
        text(0.0, 0.0, 56.0, 18.0, "TOTAL", 16.0, TextFace::Display, Ink::HudDim, false),
        Shape::Rect { x: bar_x, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) },
    ];
    if frac > 0.0 {
        s.push(Shape::Rect { x: bar_x + 2.0, y: 5.0, w: (bar_w - 4.0) * frac, h: 8.0, fill: Some(Ink::Hud), outline: None });
    }
    s.push(text(210.0, 0.0, 82.0, 18.0, &format!("{total:.0} kW"), 16.0, TextFace::Mono, Ink::Fg2, false));
    canvas(w, 18.0, s, None).fixed(18.0)
}

fn junction_lever(r: &ReactorNews, g: BreakerGroup, names: &[&str], action: &str, limit: &str) -> Item {
    let i = g.index();
    let on = r.breakers[i];
    let name_ink = if on { Ink::Fg1 } else { Ink::Fg3 };
    column(
        3.0,
        vec![
            lever_in(94.0, LEVER_PACKED, names, on, action, name_ink).fixed(44.0 + LEVER_PACKED.track),
            group_bar(r.delivered_kw[i], r.demand_kw[i], r.limits[i]),
            slider(r.limits[i], "", limit).fixed(20.0),
        ],
    )
    .fixed(94.0)
}

pub fn junction_panel(r: Option<&ReactorNews>, max_kw: f64) -> Panel {
    if !powered(r) {
        return tall(no_power(JUNCTION_NAME, "JB-01", Tier::S));
    }
    let r = r.copied().unwrap_or_default();
    // Two rows of three (r39): the room's three groups, then the r40 groups. Each row is 112 px.
    let row_a = row(
        5.0,
        vec![
            junction_lever(&r, BreakerGroup::Engines, &["ENGINES"], "br_engines", "lim_engines"),
            junction_lever(&r, BreakerGroup::LifeSupport, &["LIFE", "SUPPORT"], "br_life", "lim_life"),
            junction_lever(&r, BreakerGroup::Lights, &["LIGHTS"], "br_lights", "lim_lights"),
        ],
    )
    .fixed(112.0);
    let row_b = row(
        5.0,
        vec![
            junction_lever(&r, BreakerGroup::Shields, &["SHIELDS"], "br_shields", "lim_shields"),
            junction_lever(&r, BreakerGroup::Weapons, &["WEAPONS"], "br_weapons", "lim_weapons"),
            junction_lever(&r, BreakerGroup::JumpDrive, &["JUMP", "DRIVE"], "br_jump", "lim_jump"),
        ],
    )
    .fixed(112.0);
    let levers = column(4.0, vec![row_a, row_b]).fixed(228.0);
    let total = r.delivered_kw.iter().sum::<f64>();
    let body = column(6.0, vec![levers, total_bar(total, max_kw)]);
    tall(Panel::screen(Tier::S, JUNCTION_NAME, "JB-01", body).with_padding(6.0))
}

/// BT-01 (r38, Barotrauma PowerContainer): the battery behind the reactor: its charge, the kW it
/// gives the grid now, and the recharge slider (the share of its max charge speed taken from the surplus).
pub fn battery_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    let bar_w = 200.0;
    let pct = format!("{:.0} %", r.battery_charge.clamp(0.0, 1.0) * 100.0);
    let mut s = vec![Shape::Rect { x: 0.0, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if r.battery_charge > 0.0 {
        s.push(Shape::Rect { x: 2.0, y: 5.0, w: (bar_w - 4.0) * r.battery_charge.clamp(0.0, 1.0), h: 8.0, fill: Some(Ink::Hud), outline: None });
    }
    s.push(text(210.0, 0.0, 82.0, 18.0, &pct, 16.0, TextFace::Mono, Ink::Fg2, false));
    let charge = column(4.0, vec![micro("CHARGE").fixed(14.0), canvas(292.0, 18.0, s, None).fixed(18.0)]);
    let out = readout(format!("OUTPUT {:.0} kW", r.battery_kw)).text_size(font::SMALL).tone(Tone::Normal);
    let rate = column(4.0, vec![micro("RECHARGE").fixed(14.0), slider(r.battery_recharge, "", "bat_rate").fixed(40.0)]);
    let body = column(0.0, vec![space().flex(1.0), column(10.0, vec![charge, out.fixed(20.0), rate]).fixed(120.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, BATTERY_NAME, "BT-01", body)).with_padding(6.0)
}

/// SH-01 (r40, PULSAR's shield generator): the shield's charge, the quantum shield's word (UP while it
/// blocks boarding; DOWN after a drop below its line, for the offline time) and the regen in % a second
/// (0 inside the recovery delay).
pub fn shield_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    let bar_w = 200.0;
    let charge_frac = r.shield.clamp(0.0, 1.0);
    let mut s = vec![Shape::Rect { x: 0.0, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if charge_frac > 0.0 {
        s.push(Shape::Rect { x: 2.0, y: 5.0, w: (bar_w - 4.0) * charge_frac, h: 8.0, fill: Some(Ink::Hud), outline: None });
    }
    s.push(text(210.0, 0.0, 82.0, 18.0, &format!("{:.0} %", charge_frac * 100.0), 16.0, TextFace::Mono, Ink::Fg2, false));
    let charge = column(4.0, vec![micro("SHIELD").fixed(14.0), canvas(292.0, 18.0, s, None).fixed(18.0)]);
    let (word, tone) = if r.quantum_shield { ("QT SHLD UP", Tone::Good) } else { ("QT SHLD DOWN", Tone::Bad) };
    let quantum = readout(word.to_string()).text_size(font::SMALL).tone(tone);
    let regen = readout(format!("REGEN {:.1} %/s", r.shield_regen)).text_size(font::SMALL).tone(Tone::Normal);
    let body = column(0.0, vec![space().flex(1.0), column(10.0, vec![charge, quantum.fixed(20.0), regen.fixed(20.0)]).fixed(120.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, SHIELD_NAME, "SH-01", body)).with_padding(6.0)
}

/// HM-01 (r38, the helm): the throttle slider (-100..100 %, the engines' target) and what the engines
/// make of it: the thrust (%) and the kW they draw of their demand. A short grid shows here as a
/// thrust below the throttle and a draw below the demand.
pub fn helm_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    let throttle = column(4.0, vec![micro("THROTTLE").fixed(14.0), slider((r.throttle + 100.0) / 200.0, "", "throttle").fixed(40.0)]);
    let thrust = readout(format!("THRUST {:.0} %", r.engine_thrust)).text_size(font::SMALL).tone(Tone::Normal);
    let draw = readout(format!("ENGINE {:.0} / {:.0} kW", r.delivered_kw[0], r.demand_kw[0])).text_size(font::SMALL).tone(Tone::Normal);
    let body = column(0.0, vec![space().flex(1.0), column(10.0, vec![throttle, thrust.fixed(20.0), draw.fixed(20.0)]).fixed(120.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, HELM_NAME, "HM-01", body)).with_padding(6.0)
}

/// The GN-01 word: RELOADING while the gun reloads, READY with a shot in the capacitor, CHARGING
/// while it is empty and the WEAPONS group gets some power, NO POWER when it gets none.
pub fn gun_word(r: &ReactorNews) -> (&'static str, Tone) {
    if r.weapon_jammed {
        ("JAMMED", Tone::Bad)
    } else if r.weapon_overheated {
        ("OVERHEAT", Tone::Bad)
    } else if r.weapon_reload_s > 0.0 {
        ("RELOADING", Tone::Warn)
    } else if r.weapon_shots > 0 {
        ("READY", Tone::Good)
    } else if r.delivered_kw[BreakerGroup::Weapons.index()] > 0.0 {
        ("CHARGING", Tone::Warn)
    } else {
        ("NO POWER", Tone::Bad)
    }
}

/// An encounter runs (r41): the enemy's outcome is still None.
pub fn ambush_running(r: &ReactorNews) -> bool {
    r.enemy.is_some_and(|e| e.outcome == EnemyOutcome::None)
}

/// GN-01 (r40, the gunnery): the capacitor's shots as pips, its charge, the gun's heat (r41), the
/// word and FIRE. FIRE stands in for the tactical officer until there are enemies and aiming. A
/// jammed gun shows CLEAR JAM in FIRE's place: held, it clears the jam (`clear_jam_hold`). `capacity`
/// is the pips the capacitor holds (`WeaponsTuning::capacity_shots`).
pub fn gunnery_panel(r: Option<&ReactorNews>, capacity: u8) -> Panel {
    let r = r.copied().unwrap_or_default();
    let (word, tone) = gun_word(&r);
    let (pip_w, pip_gap) = (30.0, 8.0);
    let pips: Vec<Shape> = (0..capacity)
        .map(|i| Shape::Rect { x: f64::from(i) * (pip_w + pip_gap), y: 0.0, w: pip_w, h: 20.0, fill: Some(if i < r.weapon_shots { Ink::Hud } else { Ink::Bg3 }), outline: Some((Ink::HudDim, 2.0)) })
        .collect();
    let pip_row = canvas(f64::from(capacity) * (pip_w + pip_gap) - pip_gap, 20.0, pips, None).fixed(20.0);
    let bar_w = 200.0;
    let charge = r.weapon_charge.clamp(0.0, 1.0);
    // A 0..1 bar with its percent, as the charge and the enemy's shield and hull read.
    let bar_row = |frac: f64| {
        let f = frac.clamp(0.0, 1.0);
        let mut s = vec![Shape::Rect { x: 0.0, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
        if f > 0.0 {
            s.push(Shape::Rect { x: 2.0, y: 5.0, w: (bar_w - 4.0) * f, h: 8.0, fill: Some(Ink::Hud), outline: None });
        }
        row(8.0, vec![canvas(bar_w, 18.0, s, None).fixed(bar_w), readout(format!("{:.0} %", f * 100.0)).text_size(font::SMALL).tone(Tone::Normal).flex(1.0)]).fixed(18.0)
    };
    let bar = bar_row(charge);
    // The heat bar fills to the overheat line (1); the lock lets go below `unlock_at`, which the word shows.
    let heat = r.weapon_heat.clamp(0.0, 1.0);
    let mut hs = vec![Shape::Rect { x: 0.0, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if heat > 0.0 {
        hs.push(Shape::Rect { x: 2.0, y: 5.0, w: (bar_w - 4.0) * heat, h: 8.0, fill: Some(if heat >= 1.0 { Ink::Bad } else { Ink::Warn }), outline: None });
    }
    let heat_bar = row(8.0, vec![canvas(bar_w, 18.0, hs, None).fixed(bar_w), readout(format!("{:.0} %", heat * 100.0)).text_size(font::SMALL).tone(Tone::Normal).flex(1.0)]).fixed(18.0);
    let word = readout(word).text_size(font::SMALL).tone(tone).fixed(20.0);
    let action = if r.weapon_jammed {
        button("CLEAR JAM", "clear_jam").fixed(40.0)
    } else {
        button("FIRE", "fire").enabled(r.weapon_shots > 0 && r.weapon_reload_s <= 0.0 && !r.weapon_overheated).fixed(40.0)
    };
    let mut rows = vec![micro("SHOTS").fixed(14.0), pip_row, micro("CHARGE").fixed(14.0), bar, micro("HEAT").fixed(14.0), heat_bar, word, action];
    // The enemy ship of the ambush (r41): its shield and hull while the encounter runs, its end at the last.
    if let Some(e) = r.enemy {
        let (end, end_tone) = match e.outcome {
            EnemyOutcome::None => ("ENGAGED", Tone::Bad),
            EnemyOutcome::Destroyed => ("DESTROYED", Tone::Good),
            EnemyOutcome::Escaped => ("ESCAPED", Tone::Warn),
        };
        rows.push(micro("ENEMY SHIELD").fixed(14.0));
        rows.push(bar_row(e.shield));
        rows.push(micro("ENEMY HULL").fixed(14.0));
        rows.push(bar_row(e.hull));
        rows.push(readout(end).text_size(font::SMALL).tone(end_tone).fixed(20.0));
    }
    rows.push(space());
    let body = column(6.0, rows);
    Panel::screen(Tier::S, GUNNERY_NAME, "GN-01", body).with_padding(8.0)
}

/// JD-01 (r40, #252, PULSAR's warp drive screen): the stage, the charge, the EM SPIKE warning while
/// it charges, ARM (starts the charge, from cold only) and JUMP (live at READY only). After a jump the
/// stage reads JUMPED while the drive runs down to cold.
pub fn jump_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    let (word, tone) = match r.jump_stage {
        JumpStage::ColdStart => ("COLD START", Tone::Dim),
        JumpStage::Prepping => ("PREPPING", Tone::Normal),
        JumpStage::Paused => ("PAUSED", Tone::Warn),
        JumpStage::Ready => ("READY", Tone::Normal),
        JumpStage::Active => ("JUMPED", Tone::Bad),
    };
    let stage = readout(word).text_size(font::SMALL).tone(tone);
    let bar_w = 200.0;
    let pct = format!("{:.0} %", r.jump_charge.clamp(0.0, 1.0) * 100.0);
    let mut s = vec![Shape::Rect { x: 0.0, y: 3.0, w: bar_w, h: 12.0, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }];
    if r.jump_charge > 0.0 {
        s.push(Shape::Rect { x: 2.0, y: 5.0, w: (bar_w - 4.0) * r.jump_charge.clamp(0.0, 1.0), h: 8.0, fill: Some(Ink::Hud), outline: None });
    }
    s.push(text(210.0, 0.0, 82.0, 18.0, &pct, 16.0, TextFace::Mono, Ink::Fg2, false));
    let charge = column(4.0, vec![micro("CHARGE").fixed(14.0), canvas(292.0, 18.0, s, None).fixed(18.0)]);
    let spike = if r.jump_stage == JumpStage::Prepping { readout("EM SPIKE").text_size(font::SMALL).tone(Tone::Bad) } else { readout("").text_size(font::SMALL).tone(Tone::Dim) };
    let arm = button("ARM", "jump_arm").enabled(r.jump_stage == JumpStage::ColdStart);
    let go = button("JUMP", "jump_go").tone(Tone::Bad).enabled(r.jump_stage == JumpStage::Ready);
    let presses = row(8.0, vec![arm.flex(1.0), go.flex(1.0)]).fixed(28.0);
    let body = column(0.0, vec![space().flex(1.0), column(10.0, vec![stage.fixed(20.0), charge, spike.fixed(20.0), presses]).fixed(160.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, JUMP_NAME, "JD-01", body)).with_padding(6.0)
}

/// power: the aux reactor has its own supply. TODO(initiator): confirm.
pub fn aux_panel(r: Option<&ReactorNews>) -> Panel {
    let r = r.copied().unwrap_or_default();
    if !r.aux_on {
        let (w, h) = (292.0, 264.0);
        let shapes = vec![text(0.0, h * 0.5 - 10.0, w, 20.0, "AUX OFFLINE", 22.0, TextFace::DisplayBold, Ink::Fg3, true)];
        return tall(Panel::screen(Tier::S, AUX_NAME, "AX-01", canvas(w, h, shapes, None).flex(1.0)));
    }
    let name = Ink::Fg1;
    let switches = row(
        0.0,
        vec![lever_in(76.0, LEVER, &["LIFE", "SUPPORT"], r.aux_life_support, "aux_life", name), lever_in(76.0, LEVER, &["LIGHTS"], r.aux_lights, "aux_lights", name)],
    )
    .fixed(152.0);
    let boost = column(4.0, vec![micro("AUX OUTPUT").fixed(14.0), readout(format!("BOOST +{:.0} kW", r.aux_boost_kw)).text_size(font::SMALL).tone(Tone::Normal)]);
    let body = column(0.0, vec![space().flex(1.0), row(10.0, vec![switches, boost.flex(1.0)]).fixed(168.0), space().flex(1.0)]);
    tall(Panel::screen(Tier::S, AUX_NAME, "AX-01", body)).with_padding(6.0)
}

/// A vent's grille panel (r38): one OPEN/SHUT toggle. A shut vent takes no air; the generator's
/// flow goes to the other one. Press asks the host (`ReactorAsk::SetVent`); the host keeps the state.
pub fn vent_panel(r: Option<&ReactorNews>, i: usize) -> Panel {
    let open = r.map_or(false, |r| r.vents[i]);
    let (name, code) = match i {
        0 => (VENT_ENGINE_NAME, "VT-01"),
        1 => (VENT_CORRIDOR_NAME, "VT-02"),
        _ => (VENT_BRIDGE_NAME, "VT-03"),
    };
    let word = if open { "OPEN" } else { "SHUT" };
    let body = column(6.0, vec![micro("VENT").fixed(14.0), toggle(open, "", "vent").labels("SHUT", "OPEN").fixed(48.0), readout(word).text_size(font::SMALL).tone(if open { Tone::Good } else { Tone::Warn })]);
    Panel::screen(Tier::S, name, code, body).with_padding(8.0)
}

pub fn oxygen_tone(o: f64) -> Tone {
    let p = o * 100.0;
    if p <= 40.0 {
        Tone::Bad
    } else if p <= 70.0 {
        Tone::Warn
    } else {
        Tone::Good
    }
}

fn band(temp: f64, optimal: [f64; 2], allowed: [f64; 2]) -> Item {
    let x = |v: f64| v * 2.0;
    let seg = |a: f64, b: f64, ink: Option<Ink>, fill: Ink| {
        let mut v = vec![Shape::Rect { x: x(a), y: 7.0, w: x(b) - x(a), h: 8.0, fill: Some(fill), outline: None }];
        if let Some(i) = ink {
            v.push(Shape::Hatch { x: x(a), y: 7.0, w: x(b) - x(a), h: 8.0, ink: i });
        }
        v
    };
    let mut s = Vec::new();
    s.extend(seg(0.0, allowed[0], None, Ink::Line2));
    s.extend(seg(allowed[0], optimal[0], Some(Ink::Warn), Ink::Bg3));
    s.extend(seg(optimal[0], optimal[1], None, Ink::GoodDim));
    s.extend(seg(optimal[1], allowed[1], Some(Ink::Warn), Ink::Bg3));
    s.extend(seg(allowed[1], 100.0, Some(Ink::Bad), Ink::Bg3));
    let m = temp.clamp(0.0, 100.0);
    s.push(Shape::Rect { x: x(m) - 2.0, y: 0.0, w: 4.0, h: 22.0, fill: Some(Ink::FgTitle), outline: None });
    canvas(200.0, 22.0, s, None).fixed(200.0)
}

/// The bridge door's panel (r44, BD-01): the door's state and the toggle, as the vent panels show their valve.
/// Press asks the host (`RoomAsk::ToggleBridgeDoor`); the room's news carries the state.
pub fn bridge_door_panel(room: Option<&RoomNews>) -> Panel {
    let open = room.is_some_and(|r| r.bridge_door);
    let word = if open { "OPEN" } else { "SHUT" };
    let body = column(6.0, vec![micro("BRIDGE DOOR").fixed(14.0), toggle(open, "", "bridge_door").labels("SHUT", "OPEN").fixed(48.0), readout(word).text_size(font::SMALL).tone(if open { Tone::Good } else { Tone::Warn })]);
    Panel::screen(Tier::S, BRIDGE_DOOR_NAME, "BD-01", body).with_padding(8.0)
}

pub fn door_panel(r: Option<&ReactorNews>) -> Panel {
    if !powered(r) {
        return no_power(DOOR_NAME, "ER-01", Tier::S);
    }
    let r = r.copied().unwrap_or_default();
    let (optimal, allowed) = bands(Some(&r));
    let z = zone_tone(zone(r.temp, optimal, allowed));
    let (word, word_tone) = reactor_status(Some(&r));
    // The captain's threat (#301): a red EMERGENCY line while the ship is under threat.
    let emergency = captain_screen::emergency(display_core::captain::captain_view(&r).threat).map(|w| readout(format!("EMERGENCY {w}")).text_size(font::SMALL).tone(Tone::Bad).fixed(18.0));
    let temp = row(6.0, vec![readout(format!("{:.0}", r.temp)).text_size(72.0).tone(z), readout("°C").text_size(28.0).tone(z)]).fixed(70.0);
    let bandrow = row(8.0, vec![band(r.temp, optimal, allowed), readout(word).text_size(font::SMALL).tone(word_tone).flex(1.0)]).fixed(18.0);
    let air = row(
        8.0,
        vec![
            column(0.0, vec![micro("O₂").fixed(14.0), readout(format!("{:.0}%", r.oxygen * 100.0)).text_size(font::VALUE).tone(oxygen_tone(r.oxygen))]).flex(1.0),
            column(0.0, vec![micro("LIGHT").fixed(14.0), toggle(r.breakers[BreakerGroup::Lights.index()], "", "br_lights").labels("OFF", "ON").fixed(48.0)]).fixed(112.0),
        ],
    )
    .fixed(62.0);
    let mut body = vec![micro("REACTOR").fixed(14.0), temp, bandrow, air];
    body.extend(emergency);
    body.push(space());
    Panel::screen(Tier::S, DOOR_NAME, "ER-01", column(4.0, body)).with_padding(8.0)
}

/// An airlock panel (r40): the doors' state or the cycle's progress, and IN (the inner door), OUT (the
/// outer door), CYCLE (the pump alone). Both panels show the same; the press goes to the host.
pub fn airlock_panel(r: Option<&ReactorNews>, name: &'static str, code: &'static str) -> Panel {
    let a = r.map(|r| r.airlock).unwrap_or_default();
    let word = match a.cycling {
        AirlockCycle::Idle if a.inner_open => "INNER OPEN".to_string(),
        AirlockCycle::Idle if a.outer_open => "OUTER OPEN".to_string(),
        AirlockCycle::Idle => "DOORS SHUT".to_string(),
        _ => format!("CYCLING {:.0} %", a.progress * 100.0),
    };
    let presses = row(4.0, vec![button("IN", "in").flex(1.0), button("OUT", "out").flex(1.0), button("CYCLE", "cycle").flex(1.0)]).fixed(36.0);
    let body = column(6.0, vec![micro("AIRLOCK").fixed(14.0), readout(word).text_size(font::SMALL).tone(Tone::Normal).fixed(20.0), presses]);
    Panel::screen(Tier::S, name, code, body).with_padding(8.0)
}

/// The master is off (r42, #268): the screen goes dark under its plate, its status says OFF. Every
/// screen but SP-01 goes through this in `write_panels`.
pub fn off(p: Panel) -> Panel {
    let (w, h) = (p.size.x - 40.0, p.size.y - 80.0);
    let shapes = vec![Shape::Rect { x: 0.0, y: 0.0, w, h, fill: Some(Ink::Bg0), outline: None }, text(0.0, h * 0.5 - 10.0, w, 20.0, "OFF", 22.0, TextFace::DisplayBold, Ink::Fg3, true)];
    let plate = p.plate.map(|pl| display_core::Plate { status: Some(("OFF".to_string(), Tone::Dim)), ..pl });
    Panel { plate, root: canvas(w, h, shapes, None).flex(1.0), ..p }
}

pub fn faults_panel(r: Option<&ReactorNews>) -> Panel {
    let aux = if r.is_some_and(|r| r.aux_on) { "AUX ON" } else { "AUX OFF" };
    let director = if r.is_some_and(|r| r.director) { "DIRECTOR ON" } else { "DIRECTOR OFF" };
    let vacuum = if r.is_some_and(|r| r.vacuum) { "VACUUM ON" } else { "VACUUM OFF" };
    let ambush = if r.is_some_and(ambush_running) { "AMBUSH ON" } else { "AMBUSH OFF" };
    let bots = r.map_or(0, |r| r.bot_count);
    let col = |items: Vec<_>| column(4.0, items);
    Panel::screen(
        Tier::S,
        FAULTS_NAME,
        "DEV-02",
        row(
            6.0,
            vec![
                col(vec![
                    button("Pump break", "pump_break").fixed(24.0),
                    button("Load drop", "load_drop").fixed(24.0),
                    button("Surge", "surge").fixed(24.0),
                    button("Hull breach", "hull_breach").fixed(24.0),
                    button("Hit light", "hit_light").fixed(24.0),
                    button("Hit heavy", "hit_heavy").fixed(24.0),
                    button("Drain tank", "drain_coolant").fixed(24.0),
                    button("O2 gen break", "oxygen_break").fixed(24.0),
                ]),
                col(vec![
                    button("Start fire", "fire").fixed(24.0),
                    button("Fire mode", "fire_model").fixed(24.0),
                    button("Warm start", "warm_start").fixed(24.0),
                    button(aux, "aux").fixed(24.0),
                    button("Reset RX", "reset").fixed(24.0),
                    button(director, "director").fixed(24.0),
                    button("Shield hit", "shield_hit").fixed(24.0),
                    button("Jump full", "jump_full").fixed(24.0),
                ]),
                col(vec![button(ambush, "ambush").fixed(24.0), button(vacuum, "vacuum").fixed(28.0), button("COLD SHIP", "cold_ship").fixed(24.0), button(format!("BOTS {bots}"), "bots").fixed(24.0)]),
            ],
        ),
    )
}

fn lamp_word(l: Lamp) -> (&'static str, Ink) {
    match l {
        Lamp::Hot => ("HOT", Ink::Warn),
        Lamp::CritHeat => ("CRIT HEAT", Ink::Bad),
        Lamp::Cold => ("COLD", Ink::Warn),
        Lamp::LowTemp => ("LOW TEMP", Ink::Warn),
        Lamp::HighOut => ("HIGH OUT", Ink::Warn),
        Lamp::CritOut => ("CRIT OUT", Ink::Bad),
        Lamp::LowOut => ("LOW OUT", Ink::Warn),
        Lamp::FuelLow => ("FUEL LOW", Ink::Warn),
        Lamp::NoFuel => ("NO FUEL", Ink::Bad),
        Lamp::Meltdown => ("MELTDOWN", Ink::Bad),
        Lamp::Scram => ("SCRAM", Ink::Warn),
    }
}

fn lamp_cell(l: Lamp, lit: bool) -> Item {
    let (word, ink) = lamp_word(l);
    let shapes = if lit {
        vec![Shape::Rect { x: 0.0, y: 0.0, w: LAMP_W, h: LAMP_H, fill: Some(ink), outline: None }, text(0.0, 0.0, LAMP_W, LAMP_H, word, 14.0, TextFace::Display, Ink::Bg0, true)]
    } else {
        vec![Shape::Rect { x: 0.0, y: 0.0, w: LAMP_W, h: LAMP_H, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 2.0)) }, text(0.0, 0.0, LAMP_W, LAMP_H, word, 14.0, TextFace::Display, Ink::Fg3, true)]
    };
    canvas(LAMP_W, LAMP_H, shapes, None).flex(1.0)
}

/// The graph's cell (px) on the M panel (640 x 400): the width less the well and padding, and the
/// height the lamps, boost, presses, countdown and the stability bar leave (TODO(initiator): the status
/// board's budget: the bar took 25 px of it).
const GRAPH_W: f64 = 600.0;
const GRAPH_H: f64 = 125.0;

/// The graph's shapes (Barotrauma's `DrawGraph`, `Reactor.cs:543-575`): output in the good tone,
/// load in the neutral one, both on one scale, the larger of half the room's maximum and the
/// highest load shown (Barotrauma uses 10000 kW with a 5000 kW reactor). Newest at the right.
/// No curves (`graph_points` 0) draws the frame only.
pub fn graph_shapes(r: Option<&ReactorNews>, max_kw: f64, w: f64, h: f64) -> Vec<Shape> {
    let mut s = vec![Shape::Rect { x: 0.0, y: 0.0, w, h, fill: Some(Ink::Bg0), outline: Some((Ink::HudDim, 1.0)) }];
    let Some(r) = r else { return s };
    let n = (r.graph_points as usize).min(item_core::wire::GRAPH_POINTS);
    if n < 2 {
        return s;
    }
    let top = r.graph_load[..n].iter().copied().max().unwrap_or(0) as f64;
    let scale = (max_kw * 0.5).max(top).max(1.0);
    // Index 0 is the newest sample, at the right edge; the oldest is at the left.
    let point = |curve: &[u16; item_core::wire::GRAPH_POINTS], i: usize| -> [f64; 2] {
        let x = w * (1.0 - i as f64 / (n - 1) as f64);
        let y = h - (curve[i] as f64 / scale).clamp(0.0, 1.0) * (h - 2.0) - 1.0;
        [x, y]
    };
    for (curve, ink) in [(&r.graph_output, Ink::Good), (&r.graph_load, Ink::Fg2)] {
        for i in 0..n - 1 {
            s.push(Shape::Line { a: point(curve, i + 1), b: point(curve, i), width: 2.0, ink, dash: None });
        }
    }
    s
}

fn graph_item(r: Option<&ReactorNews>, max_kw: f64) -> Item {
    canvas(GRAPH_W, GRAPH_H, graph_shapes(r, max_kw, GRAPH_W, GRAPH_H), None).fixed(GRAPH_H)
}

const BAR_H: f64 = 8.0;

/// The stability bar's fill (0..1, the percent) and tone: good above 75 %, warn from 40 to 75 %, bad
/// below 40 % (PULSAR's core stability). Pure, for the tests.
pub fn stability_bar(pct: u8) -> (f64, Tone) {
    let tone = if pct > 75 {
        Tone::Good
    } else if pct >= 40 {
        Tone::Warn
    } else {
        Tone::Bad
    };
    (f64::from(pct.min(100)) / 100.0, tone)
}

/// status board's strip. No news yet: the percent is a dash and the bar is empty.
fn stability_item(r: Option<&ReactorNews>) -> Item {
    let (fill, tone, word) = match r {
        Some(r) => {
            let (fill, tone) = stability_bar(r.stability);
            (fill, tone, format!("{}%", r.stability.min(100)))
        }
        None => (0.0, Tone::Dim, "--".to_string()),
    };
    let ink = match tone {
        Tone::Good => Ink::Good,
        Tone::Warn => Ink::Warn,
        _ => Ink::Bad,
    };
    let head = row(8.0, vec![micro("STABILITY").flex(1.0), readout(word).text_size(font::SMALL).tone(tone)]).fixed(16.0);
    let bar = vec![
        Shape::Rect { x: 0.0, y: 0.0, w: GRAPH_W, h: BAR_H, fill: Some(Ink::Bg3), outline: Some((Ink::HudDim, 1.0)) },
        Shape::Rect { x: 0.0, y: 0.0, w: GRAPH_W * fill, h: BAR_H, fill: Some(ink), outline: None },
    ];
    column(3.0, vec![head, canvas(GRAPH_W, BAR_H, bar, None).fixed(BAR_H)]).fixed(16.0 + 3.0 + BAR_H)
}

pub fn clock(s: f64) -> String {
    let s = s.max(0.0).ceil() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn status_panel(r: Option<&ReactorNews>, max_kw: f64, blink_on: bool) -> Panel {
    // No NO POWER here: like the readout, the panel has its own start-up supply (the module doc), so
    // EJECT works with the machines dark. The boost is refused without power, so its buttons are off.
    let r = r.copied().unwrap_or_default();
    let lit = |l: Lamp| r.lamps[l.index()] && (l == Lamp::Scram || blink_on);
    let mut grid = Vec::new();
    for chunk in LAMPS.chunks(3) {
        let mut cells: Vec<Item> = chunk.iter().map(|&l| lamp_cell(l, lit(l))).collect();
        while cells.len() < 3 {
            cells.push(space());
        }
        grid.push(row(LAMP_GAP, cells).fixed(LAMP_H));
    }
    let lamps = column(LAMP_GAP, grid).fixed(4.0 * LAMP_H + 3.0 * LAMP_GAP);
    let boost = row(8.0, vec![micro("BOOST").flex(1.0), readout(format!("{:+.0}", r.boost)).text_size(font::SMALL).tone(Tone::Normal), cover_word(&r)]).fixed(16.0);
    let countdown = match r.meltdown_eta_s {
        Some(s) => readout(format!("IMPLOSION {}", clock(s))).text_size(font::LABEL).tone(Tone::Bad).fixed(22.0),
        None => space().fixed(22.0),
    };
    let presses = row(6.0, vec![button("UP", "boost_up").enabled(r.boost_ok).flex(1.0), button("DOWN", "boost_down").enabled(r.boost_ok).flex(1.0), eject_button(&r).flex(1.0)]).fixed(ctl::SM);
    let stability = stability_item(Some(&r));
    let graph = graph_item(Some(&r), max_kw);
    Panel::screen(Tier::M, STATUS_NAME, "RX-02", column(5.0, vec![lamps, boost, presses, countdown, stability, graph])).with_padding(6.0)
}

pub fn eject_button(r: &ReactorNews) -> Item {
    let press = if r.eject_armed { button("EJECT", "eject").tone(Tone::Bad) } else { button("ARM EJECT", "arm_eject") };
    press.enabled(!r.ejected)
}

pub fn cover_word(r: &ReactorNews) -> Item {
    let (word, tone) = if r.ejected {
        ("NO CORE", Tone::Dim)
    } else if r.eject_armed {
        ("COVER OPEN", Tone::Bad)
    } else {
        ("COVER SHUT", Tone::Dim)
    };
    readout(word).text_size(font::SMALL).tone(tone)
}

pub fn limit_group(action: &str) -> Option<BreakerGroup> {
    match action {
        "lim_engines" => Some(BreakerGroup::Engines),
        "lim_life" => Some(BreakerGroup::LifeSupport),
        "lim_lights" => Some(BreakerGroup::Lights),
        "lim_shields" => Some(BreakerGroup::Shields),
        "lim_weapons" => Some(BreakerGroup::Weapons),
        "lim_jump" => Some(BreakerGroup::JumpDrive),
        _ => None,
    }
}

pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>, mut help: ResMut<ReactorHelp>) {
    for c in clicks.read() {
        let snap = items.reactor;
        let new_state = |current: bool| c.value.map(|v| v >= 0.5).unwrap_or(!current);
        let ask = if c.panel == startup_screen::ID {
            startup_screen::ask(c, snap.as_ref())
        } else if c.panel == READOUT_ID {
            match c.action.as_str() {
                HELP_ACTION => {
                    help.toggle();
                    None
                }
                "mode" => snap.map(|r| ReactorAsk::SetManual(new_state(r.manual))),
                "power" => snap.map(|r| ReactorAsk::SetPower(new_state(r.power_on))),
                "fis_set" => c.value.map(|v| ReactorAsk::SetFission(v as f64 * 100.0)),
                "tur_set" => c.value.map(|v| ReactorAsk::SetTurbine(v as f64 * 100.0)),
                _ => None,
            }
        } else if c.panel == PUMP_ID {
            match c.action.as_str() {
                "pump_off" => Some(ReactorAsk::SetPumpMode(PumpMode::Off)),
                "pump_low" => Some(ReactorAsk::SetPumpMode(PumpMode::Low)),
                "pump_high" => Some(ReactorAsk::SetPumpMode(PumpMode::High)),
                "ecool" => Some(ReactorAsk::EmergencyCool),
                _ => None,
            }
        } else if c.panel == JUNCTION_ID && let Some(g) = limit_group(&c.action) {
            c.value.map(|v| ReactorAsk::SetLimit(g, v as f64))
        } else if c.panel == JUNCTION_ID || c.panel == DOOR_ID {
            let group = match c.action.as_str() {
                "br_engines" => Some(BreakerGroup::Engines),
                "br_life" => Some(BreakerGroup::LifeSupport),
                "br_lights" => Some(BreakerGroup::Lights),
                "br_shields" => Some(BreakerGroup::Shields),
                "br_weapons" => Some(BreakerGroup::Weapons),
                "br_jump" => Some(BreakerGroup::JumpDrive),
                _ => None,
            };
            group.and_then(|g| snap.map(|r| ReactorAsk::SetBreaker(g, new_state(r.breakers[g.index()]))))
        } else if c.panel == AUX_ID {
            match c.action.as_str() {
                "aux_life" => snap.map(|r| ReactorAsk::SetAuxSwitch(BreakerGroup::LifeSupport, new_state(r.aux_life_support))),
                "aux_lights" => snap.map(|r| ReactorAsk::SetAuxSwitch(BreakerGroup::Lights, new_state(r.aux_lights))),
                _ => None,
            }
        } else if c.panel == BATTERY_ID {
            match c.action.as_str() {
                "bat_rate" => c.value.map(|v| ReactorAsk::SetBatteryRecharge(v as f64)),
                _ => None,
            }
        } else if c.panel == HELM_ID {
            match c.action.as_str() {
                // The slider's 0..1 is -100..100 %.
                "throttle" => c.value.map(|v| ReactorAsk::SetThrottle(v as f64 * 200.0 - 100.0)),
                _ => None,
            }
        } else if c.panel == GUNNERY_ID {
            match c.action.as_str() {
                "fire" => Some(ReactorAsk::Fire),
                _ => None,
            }
        } else if c.panel == JUMP_ID {
            match c.action.as_str() {
                "jump_arm" => Some(ReactorAsk::JumpArm),
                "jump_go" => Some(ReactorAsk::JumpGo),
                _ => None,
            }
        } else if c.panel == VENT_ENGINE_ID || c.panel == VENT_CORRIDOR_ID || c.panel == VENT_BRIDGE_ID {
            let i = if c.panel == VENT_ENGINE_ID {
                0
            } else if c.panel == VENT_CORRIDOR_ID {
                1
            } else {
                2
            };
            snap.map(|r| ReactorAsk::SetVent(i, new_state(r.vents[i as usize])))
        } else if c.panel == BRIDGE_DOOR_ID {
            // The bridge door's panel (r44): the press flips the room's door, as CP-01's does.
            let open = new_state(items.room.bridge_door);
            items.ask_room(RoomAsk::SetBridgeDoor(open));
            None
        } else if c.panel == STATUS_ID {
            match c.action.as_str() {
                "boost_up" => Some(ReactorAsk::Boost(true)),
                "boost_down" => Some(ReactorAsk::Boost(false)),
                "arm_eject" => Some(ReactorAsk::ArmEject),
                "eject" => Some(ReactorAsk::Eject),
                _ => None,
            }
        } else if c.panel == status_monitor::ID {
            match c.action.as_str() {
                status_monitor::SILENCE_ACTION => Some(ReactorAsk::SilenceAlarms),
                _ => None,
            }
        } else if c.panel == FAULTS_ID {
            match c.action.as_str() {
                "pump_break" => Some(ReactorAsk::Trigger(ReactorFault::PumpBreak)),
                "oxygen_break" => Some(ReactorAsk::Trigger(ReactorFault::OxygenGeneratorBreak)),
                "load_drop" => Some(ReactorAsk::Trigger(ReactorFault::LoadDrop)),
                "surge" => Some(ReactorAsk::Trigger(ReactorFault::Surge)),
                "hull_breach" => Some(ReactorAsk::Trigger(ReactorFault::HullBreach)),
                "shield_hit" => Some(ReactorAsk::Trigger(ReactorFault::ShieldHit)),
                "hit_light" => Some(ReactorAsk::Trigger(ReactorFault::HitLight)),
                "hit_heavy" => Some(ReactorAsk::Trigger(ReactorFault::HitHeavy)),
                "fire" => Some(ReactorAsk::Trigger(ReactorFault::Fire)),
                "reset" => Some(ReactorAsk::Reset),
                "drain_coolant" => Some(ReactorAsk::Trigger(ReactorFault::DrainCoolant)),
                "warm_start" => Some(ReactorAsk::Trigger(ReactorFault::WarmStart)),
                "cold_ship" => Some(ReactorAsk::Trigger(ReactorFault::ColdShip)),
                "aux" => Some(ReactorAsk::Trigger(ReactorFault::Aux)),
                "director" => snap.map(|r| ReactorAsk::SetDirector(!r.director)),
                "jump_full" => Some(ReactorAsk::Trigger(ReactorFault::JumpFull)),
                "vacuum" => snap.map(|r| ReactorAsk::SetVacuum(!r.vacuum)),
                "ambush" => snap.map(|r| ReactorAsk::Ambush(!ambush_running(&r))),
                // The dev BOTS count, 0..3 in turn (r42, #253).
                "bots" => snap.map(|r| ReactorAsk::SetBots((r.bot_count + 1) % (reactor_core::bots::MAX_BOTS + 1))),
                _ => None,
            }
        } else if c.panel == AIRLOCK_INNER_ID || c.panel == AIRLOCK_OUTER_ID {
            crate::dev_area::hull::airlock::panel_ask(&c.action)
        } else if c.panel == captain_screen::ID {
            // CP-01 (r41): the engine door is the room's (`RoomAsk::SetDoor`), the rest the reactor's.
            match c.action.as_str() {
                captain_screen::DOOR_ACTION => {
                    let open = !items.room.door_open;
                    items.ask_room(RoomAsk::SetDoor(open));
                    None
                }
                // The bridge door (r44): CP-01's third door.
                captain_screen::BRIDGE_DOOR_ACTION => {
                    let open = !items.room.bridge_door;
                    items.ask_room(RoomAsk::SetBridgeDoor(open));
                    None
                }
                captain_screen::SEAL_ACTION => Some(ReactorAsk::SealAirlock),
                captain_screen::SILENCE_ACTION => Some(ReactorAsk::SilenceAlarms),
                other => captain_screen::parse_bot_action(other).map(|(s, level)| ReactorAsk::SetBotPriority(s, level)),
            }
        } else {
            None
        };
        if let Some(a) = ask {
            items.ask_reactor(a);
        }
    }
}

/// The impeller's angle is kept in steps of 6 degrees, so the panel is only redrawn when it shows.
const IMPELLER_STEP: f64 = 6.0;

/// Every panel from the state, every tick (`Panels::set` skips an unchanged one). The impeller
pub fn write_panels(items: Res<Items>, cfg: Res<ReactorConfig>, time: Res<Time>, active: Res<ActiveDisplay>, mut help: ResMut<ReactorHelp>, mut spin: Local<f64>, mut panels: ResMut<Panels>) {
    if active.using().is_none() {
        help.0 = false;
    }
    let speed = items.reactor.map_or(0.0, |r| r.pump_speed);
    *spin = crate::effects::impeller_angle(*spin, speed, time.delta_secs_f64());
    let angle = (*spin / IMPELLER_STEP).round() * IMPELLER_STEP;
    let max_kw = cfg.0.reactor.max_output_kw;
    // SP-01 is on whatever the master; every other screen goes through `set`, which shows OFF while the master is off.
    panels.set(startup_screen::ID, startup_screen::panel(items.reactor.as_ref()));
    let live = items.reactor.map_or(true, |r| r.startup.live());
    let mut set = |id: PanelId, p: Panel| panels.set(id, if live { p } else { off(p) });
    let readout = if help.0 { readout_help_panel(items.reactor.as_ref(), max_kw) } else { readout_panel_at(items.reactor.as_ref(), max_kw, time.elapsed_secs_f64()) };
    let manned = |s: BotStation| items.reactor.is_some_and(|r| r.bots[s.index()].manned);
    set(READOUT_ID, bot_tagged(readout, manned(BotStation::Reactor)));
    set(PUMP_ID, bot_tagged(pump_panel_at(items.reactor.as_ref(), angle, lamp_blink_on(time.elapsed_secs_f64())), manned(BotStation::Reactor)));
    set(JUNCTION_ID, bot_tagged(junction_panel(items.reactor.as_ref(), max_kw), manned(BotStation::Power)));
    set(AUX_ID, aux_panel(items.reactor.as_ref()));
    set(BATTERY_ID, battery_panel(items.reactor.as_ref()));
    set(SHIELD_ID, shield_panel(items.reactor.as_ref()));
    set(HELM_ID, bot_tagged(helm_panel(items.reactor.as_ref()), manned(BotStation::Helm)));
    set(GUNNERY_ID, bot_tagged(gunnery_panel(items.reactor.as_ref(), cfg.0.weapons.capacity_shots), manned(BotStation::Gunnery)));
    set(JUMP_ID, jump_panel(items.reactor.as_ref()));
    set(DOOR_ID, door_panel(items.reactor.as_ref()));
    set(VENT_ENGINE_ID, vent_panel(items.reactor.as_ref(), 0));
    set(VENT_CORRIDOR_ID, vent_panel(items.reactor.as_ref(), 1));
    set(VENT_BRIDGE_ID, vent_panel(items.reactor.as_ref(), 2));
    set(BRIDGE_DOOR_ID, bridge_door_panel(Some(&items.room)));
    set(AIRLOCK_INNER_ID, airlock_panel(items.reactor.as_ref(), AIRLOCK_INNER_NAME, "AL-01"));
    set(AIRLOCK_OUTER_ID, airlock_panel(items.reactor.as_ref(), AIRLOCK_OUTER_NAME, "AL-02"));
    set(captain_screen::ID, captain_screen::captain_panel(items.reactor.as_ref(), items.room.door_open, items.room.bridge_door));
    set(FAULTS_ID, faults_panel(items.reactor.as_ref()));
    set(STATUS_ID, status_panel(items.reactor.as_ref(), cfg.0.reactor.max_output_kw, lamp_blink_on(time.elapsed_secs_f64())));
    set(status_monitor::ID, status_monitor::monitor_panel(items.reactor.as_ref()));
}

#[cfg(test)]
mod tests {
    use item_core::wire::{FuelKind, FuelTube};

    fn tube(life: f64) -> FuelTube {
        FuelTube { rod: Some(FuelKind::Standard), life }
    }

    use super::*;

    fn shipped_panel(r: Option<&ReactorNews>) -> Panel {
        let shipped = reactor_core::Tuning::shipped();
        readout_panel(r, shipped.reactor.max_output_kw)
    }

    fn header(p: &Panel) -> Option<(String, Tone)> {
        p.plate.as_ref()?.status.clone()
    }

    fn news() -> ReactorNews {
        ReactorNews { powered: true, power_on: true, pump_on: true, pump_mode: PumpMode::High, pump_speed: 1.0, flow: 1.0, breakers: [true; 6], optimal: [35.0, 65.0], allowed: [20.0, 80.0], temp: 54.0, output_kw: 500.0, load_kw: 500.0, fuel: [tube(80.0), tube(12.0), Default::default(), Default::default()], ..Default::default() }
    }

    fn status_news() -> ReactorNews {
        ReactorNews { lamps: [false; Lamp::COUNT], boost_ok: true, ..news() }
    }

    #[test]
    fn the_status_board_has_its_own_supply_so_eject_works_with_the_machines_dark() {
        let off = status_panel(Some(&ReactorNews { powered: false, boost_ok: false, ..status_news() }), 2000.0, true);
        for a in ["boost_up", "boost_down"] {
            assert!(matches!(off.find(a).unwrap().widget, display_core::Widget::Button { enabled: false, .. }), "{a}");
        }
        assert!(off.overflowing().is_empty());
        assert!(matches!(off.find("arm_eject").expect("ARM EJECT with no power").widget, display_core::Widget::Button { enabled: true, .. }));
        let dark = status_panel(Some(&ReactorNews { powered: false, eject_armed: true, ..status_news() }), 2000.0, true);
        assert!(dark.find("eject").is_some(), "EJECT with the cover open and no power");
        assert!(status_panel(None, 2000.0, true).find("arm_eject").is_some(), "no snapshot yet: the button still works");
    }

    #[test]
    fn the_manual_sliders_are_only_on_when_manual() {
        let auto = shipped_panel(Some(&news()));
        assert!(matches!(auto.find("fis_set").unwrap().widget, display_core::Widget::Slider { enabled: false, .. }));
        let man = shipped_panel(Some(&ReactorNews { manual: true, ..news() }));
        assert!(matches!(man.find("fis_set").unwrap().widget, display_core::Widget::Slider { enabled: true, .. }));
        assert!(man.find("mode").is_some() && man.find("reset").is_none());
        assert!(faults_panel(None).find("reset").is_some() && faults_panel(None).find("pump_break").is_some() && faults_panel(None).find("oxygen_break").is_some() && faults_panel(None).find("director").is_some());
        assert!(faults_panel(None).find("vacuum").is_some(), "the dev VACUUM switch is on the faults screen");
        assert!(airlock_panel(None, AIRLOCK_INNER_NAME, "AL-01").find("cycle").is_some(), "the airlock panels have IN, OUT and CYCLE");
    }

    #[test]
    fn the_critical_readout_keeps_its_controls_and_says_critical() {
        let hot = ReactorNews { temp: 85.0, ..news() };
        assert!(crate::effects::is_critical(Some(&hot)));
        let crit = shipped_panel(Some(&hot));
        for a in ["power", "mode", "fis_set", "tur_set"] {
            assert!(crit.find(a).is_some(), "{a} while critical");
        }
        assert_eq!(header(&crit), Some(("CRITICAL".to_string(), Tone::Bad)));
        assert!(crit.overflowing().is_empty());
        let off = ReactorNews { power_on: false, ..hot };
        assert!(!crate::effects::is_critical(Some(&off)));
        let normal = shipped_panel(Some(&off));
        assert!(normal.find("fis_set").is_some(), "powered off: the normal view");
        assert_eq!(reactor_status(Some(&off)), ("OFF", Tone::Dim));
        assert_eq!(header(&normal), Some(("OFF".to_string(), Tone::Dim)));
    }

    #[test]
    fn critical_is_the_same_at_every_temperature_on_the_readout_and_the_lamp() {
        for (allowed, temp) in [([20.0, 80.0], 85.0), ([10.0, 90.0], 85.0), ([10.0, 90.0], 95.0)] {
            let r = ReactorNews { allowed, temp, ..news() };
            let p = shipped_panel(Some(&r));
            let critical = crate::effects::is_critical(Some(&r));
            assert_eq!(header(&p).is_some_and(|h| h.0 == "CRITICAL"), critical, "allowed {allowed:?}, temp {temp}");
            assert_eq!(reactor_status(Some(&r)).0 == "CRITICAL", critical, "word at {temp} with {allowed:?}");
            assert!(p.find("fis_set").is_some() && p.find("power").is_some(), "the controls stay at {temp}");
        }
    }

    #[test]
    fn a_melted_core_says_meltdown_without_a_button() {
        let p = shipped_panel(Some(&ReactorNews { melted: true, ..news() }));
        assert!(p.find("power").is_none() && p.find("fis_set").is_none());
        assert_eq!(header(&p), Some(("MELTDOWN".to_string(), Tone::Bad)));
        assert!(p.overflowing().is_empty());
    }
}
