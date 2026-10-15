//! The gun's screen GN-01 (FIRE, the CLEAR JAM hold), its muzzle flash and the flash of an enemy's
//! hit on the hull. A press asks the host.
use crate::dev_area::*;
use crate::displays::{ActiveDisplay, DisplayAction, DisplayPanel, DisplayPointer, Panels};
use crate::dev_area::screens::*;
use crate::dev_area::ship::ReactorConfig;
use crate::items::Items;
use display_core::{button, canvas, column, font, micro, readout, row, space, Ink, Panel, Shape, Tier, Tone};
use item_core::wire::{BotStation, BreakerGroup, EnemyOutcome, ReactorAsk, ReactorNews};

use crate::dev_area::crew::{bot_tagged, manned};
use crate::dev_area::layout::{GUNNERY_AT, VENT_LIFT};
use crate::env::PlanetRes;

pub fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, (clear_jam_hold, read_clicks, write_panels).chain().in_set(crate::phases::Fx::Effects));
    app.add_systems(Update, (gun_flash, enemy_hit_flash));
}

pub const GUNNERY_ID: PanelId = PanelId::new("gunnery_panel");

pub const GUNNERY_NAME: &str = "GUNNERY";

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

/// The press on GN-01: FIRE asks the host (the CLEAR JAM hold is `clear_jam_hold`).
pub fn read_clicks(mut clicks: MessageReader<DisplayAction>, mut items: ResMut<Items>) {
    for c in clicks.read() {
        if c.panel == GUNNERY_ID && c.action == "fire" {
            items.ask_reactor(ReactorAsk::Fire);
        }
    }
}

/// GN-01 from the snapshot, every tick.
pub fn write_panels(items: Res<Items>, cfg: Res<ReactorConfig>, mut panels: ResMut<Panels>) {
    let r = items.reactor.as_ref();
    show(&mut panels, r, GUNNERY_ID, bot_tagged(gunnery_panel(r, cfg.0.weapons.capacity_shots), manned(&items, BotStation::Gunnery)));
}
