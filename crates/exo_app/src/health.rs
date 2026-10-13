//! Health (#295): players have health, hazards hurt them, items heal them, death respawns them.
//! `health_core` holds the rules; this is the glue.
//!
//! - The host (slot 1; in one process the local player) owns every player's `Health`. Each fixed
//!   tick it adds the hazards under each player (the dev area's pads, `dev_area::hazards`), steps
//!   the body with the air at its place, and counts down a dead player's respawn.
//! - A use (LMB with a bandage or deglow in hand) is a request like any item request
//!   (`Request::Use`): the client aims at another player within 2 m (or itself), the host takes
//!   one from the stack and applies the treatment (`apply_uses`).
//! - The host sends each client its own vitals when they change noticeably (`vitals_out`, over
//!   the reliable channel). Every process writes its own vitals into the HUD's `crew_hud::Vitals`
//!   (`own_sync`); while down, the controls are frozen and a centred line counts the respawn.
//! - A respawn puts the walker on the dev area's pad with clean health. A client moves its own
//!   walker when the host's vitals say it is up again.
use crate::dev_area::hazards::{BURN_PER_S, GLOW_BARREL, HOT_PLATE, NO_AIR, RADIATION_PER_S};
use crate::dev_area::{area_of, DevArea, Zone, PAD_MAIN};
use crate::controls::{Actions, Controls};
use crate::env::PlanetRes;
use crate::items::{Items, Role, LOCAL};
use crate::net_live::{Net, RemoteWalker};
use crate::origin::WorldPose;
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use health_core::{Env, Health, HealthEvent, HealthTuning, Life};
use item_core::wire::Vitals;
use item_core::{PlayerId, Request};
use std::collections::BTreeMap;
use walker_core::Frame;

/// Skill of every treater for now (TODO(initiator): skills per crew role).
pub const TREAT_SKILL: f64 = 50.0;
/// How far the aim reaches for another player (m).
pub const USE_REACH: f64 = 2.0;
/// A player's capsule: feet to head (m) and its radius.
const CAPSULE_HEIGHT: f64 = 1.8;
const CAPSULE_RADIUS: f64 = 0.4;

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let tuning = HealthTuning::load(&files).unwrap_or_else(|e| panic!("health: {e:?}"));
    app.insert_resource(HealthState::new(tuning)).init_resource::<OwnVitals>();
    // The use takes the click before the displays do (they take every click), once the script's
    // input is in.
    app.add_systems(FixedUpdate, use_input.after(crate::scenario::run_script).before(crate::displays::use_display));
    app.add_systems(
        FixedUpdate,
        (apply_uses, health_step, vitals_out, own_sync).chain().after(crate::items::deliver).before(crate::walker::walker_step).in_set(crate::phases::Fx::Walker),
    );
}

/// Window only: the centred down line.
pub fn window_plugin(app: &mut App) {
    app.add_systems(Update, (spawn_down_line, update_down_line).chain().in_set(crate::phases::Frame::Hud));
}

/// One player's body on the host.
#[derive(Clone, Debug)]
pub struct Body {
    pub health: Health,
    /// Seconds until the respawn, while dead.
    pub respawn_in: Option<f64>,
}

/// Host: every player's body. Also what the host last sent each client.
#[derive(Resource)]
pub struct HealthState {
    pub tuning: HealthTuning,
    pub bodies: BTreeMap<PlayerId, Body>,
    sent: BTreeMap<PlayerId, Vitals>,
}

impl HealthState {
    pub fn new(tuning: HealthTuning) -> HealthState {
        HealthState { tuning, bodies: BTreeMap::new(), sent: BTreeMap::new() }
    }

    fn body(&mut self, id: PlayerId) -> &mut Body {
        let t = &self.tuning;
        self.bodies.entry(id).or_insert_with(|| Body { health: Health::new(t, 0x51 + id as u64), respawn_in: None })
    }
}

/// This process's own vitals as the HUD and the controls read them.
#[derive(Resource, Default)]
pub struct OwnVitals {
    pub last: Option<Vitals>,
}

/// What the player's body shows: shares of the maximum, the burn's level as the HUD reads it.
pub fn vitals_of(t: &HealthTuning, b: &Body) -> Vitals {
    let share = |id: &str| t.affliction(id).map_or(0.0, |k| (b.health.strength(id) / k.max).clamp(0.0, 1.0));
    let burn = b.health.strength("burn");
    let burn_level = t.affliction("burn").map(|k| k.band_index(burn) + 1);
    let oxygen = b.health.strength("oxygen_low") > 0.0;
    Vitals {
        health: (b.health.vitality() / b.health.max_vitality()).clamp(0.0, 1.0),
        // The bar shows the air left: full when the air is good, falling as the low grows.
        oxygen: oxygen.then(|| 1.0 - share("oxygen_low")),
        // The HUD's three burn levels are the thirds of its share: a level's middle.
        burn: (burn > 0.0).then(|| (burn_level.unwrap_or(1) as f64 - 0.5) / 3.0),
        radiation: share("radiation"),
        down: b.respawn_in,
    }
}

/// Whether a vitals change is worth a message: a few per cent, a band, a second of the countdown.
pub fn noticeably(a: &Vitals, b: &Vitals) -> bool {
    fn moved(a: Option<f64>, b: Option<f64>, eps: f64) -> bool {
        match (a, b) {
            (Some(x), Some(y)) => (x - y).abs() > eps,
            (None, None) => false,
            _ => true,
        }
    }
    (a.health - b.health).abs() > 0.005
        || (a.radiation - b.radiation).abs() > 0.005
        || moved(a.oxygen, b.oxygen, 0.01)
        || moved(a.burn, b.burn, 1e-9)
        || moved(a.down.map(f64::floor), b.down.map(f64::floor), 0.0)
}

/// A player's place in the area (metres), if on foot on the planet.
fn area_pos(area: &DevArea, planet: &PlanetRes, p: Option<DVec3>) -> Option<[f64; 2]> {
    let (f, p) = (area.frame?, p?);
    Some(area_of(&f, planet, p))
}

/// Host: the hazards and the air under every body, one step, the respawn countdown.
#[allow(clippy::too_many_arguments)]
fn health_step(time: Res<Time>, planet: Res<PlanetRes>, area: Res<DevArea>, items: Res<Items>, mut st: ResMut<HealthState>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, net: Option<Res<Net>>) {
    if items.role == Role::Client {
        return;
    }
    let dt = time.delta_secs_f64();
    let mut at: BTreeMap<PlayerId, DVec3> = BTreeMap::new();
    if let Ok(pl) = players.single()
        && pl.ship.is_none()
    {
        at.insert(LOCAL, pl.w.pos);
    }
    for (rw, wp) in &remotes {
        at.insert(rw.owner, wp.pos);
    }
    st.body(LOCAL);
    for slot in net.map(|n| n.peer_slots()).unwrap_or_default() {
        st.body(slot);
    }
    let st = &mut *st;
    let t = &st.tuning;
    for (id, b) in st.bodies.iter_mut() {
        if b.health.state() == Life::Dead {
            if let Some(left) = b.respawn_in.as_mut() {
                *left -= dt;
                if *left <= 0.0 {
                    b.health.respawn(t);
                    b.respawn_in = None;
                }
            }
            continue;
        }
        let here = area_pos(&area, &planet, at.get(id).copied());
        let inside = |z: &Zone| here.is_some_and(|p| z.contains(p));
        if inside(&HOT_PLATE) {
            b.health.add(t, "burn", BURN_PER_S * dt);
        }
        if inside(&GLOW_BARREL) {
            b.health.add(t, "radiation", RADIATION_PER_S * dt);
        }
        let oxygen = if inside(&NO_AIR) { 0.0 } else { 1.0 };
        for ev in b.health.step(t, dt, Env { oxygen }) {
            if matches!(ev, HealthEvent::Died { .. }) {
                b.respawn_in = Some(t.respawn_s);
            }
        }
    }
}

/// Host: a use the host applied (`Items::take_uses`) treats its target.
fn apply_uses(mut items: ResMut<Items>, mut st: ResMut<HealthState>) {
    if items.role == Role::Client {
        return;
    }
    let st = &mut *st;
    for (_user, target, effect) in items.take_uses() {
        if let Some(b) = st.bodies.get_mut(&target)
            && st.tuning.treatment(&effect).is_some()
        {
            b.health.treat(&st.tuning, &effect, TREAT_SKILL);
        }
    }
}

/// Host: each client gets its own vitals when they change noticeably.
fn vitals_out(mut items: ResMut<Items>, mut st: ResMut<HealthState>) {
    if items.role != Role::Host {
        return;
    }
    let st = &mut *st;
    let now: Vec<(PlayerId, Vitals)> = st.bodies.iter().filter(|(id, _)| **id != LOCAL).map(|(id, b)| (*id, vitals_of(&st.tuning, b))).collect();
    for (id, v) in now {
        let changed = st.sent.get(&id).is_none_or(|old| noticeably(old, &v));
        if changed {
            items.send_vitals(id, v);
            st.sent.insert(id, v);
        }
    }
}

/// The first hit of the aim on another player's capsule, within reach: the player's slot.
fn aim_target(eye: DVec3, dir: DVec3, planet: &PlanetRes, remotes: &Query<(&RemoteWalker, &WorldPose)>) -> Option<PlayerId> {
    let arr = |v: DVec3| [v.x, v.y, v.z];
    let mut best: Option<(f64, PlayerId)> = None;
    for (rw, wp) in remotes {
        let up = (wp.pos - planet.centre).normalize();
        if let Some(t) = health_core::capsule_hit(arr(eye), arr(dir), USE_REACH, arr(wp.pos), arr(up), CAPSULE_HEIGHT, CAPSULE_RADIUS)
            && best.is_none_or(|(bt, _)| t < bt)
        {
            best = Some((t, rw.owner));
        }
    }
    best.map(|(_, id)| id)
}

/// Client and host: LMB with a treatment in the hand asks for its use on the player aimed at, or
/// on oneself.
#[allow(clippy::too_many_arguments)]
fn use_input(time: Res<Time>, planet: Res<PlanetRes>, mut controls: ResMut<Controls>, mut items: ResMut<Items>, st: Res<HealthState>, active: Res<crate::displays::ActiveDisplay>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>) {
    if !controls.click || controls.free_cursor || active.using().is_some() {
        return;
    }
    let Ok(pl) = players.single() else { return };
    if pl.seated || pl.fly {
        return;
    }
    let slot = items.client.view.selected;
    let Some(stack) = items.client.view.slot(slot).cloned() else { return };
    let Some(effect) = items.defs.of(&stack).and_then(|d| d.use_effect.clone()) else { return };
    if st.tuning.treatment(&effect).is_none() {
        return;
    }
    controls.click = false;
    let target = if pl.ship.is_none() {
        let eye = pl.w.pos + pl.up * EYE_HEIGHT;
        aim_target(eye, pl.world_look(Frame::IDENTITY), &planet, &remotes).unwrap_or(LOCAL)
    } else {
        LOCAL
    };
    let _ = items.request(time.elapsed_secs_f64(), Request::Use { slot, target });
}

/// Every process: its own vitals into the HUD; frozen controls and a walk to the pad when down or
/// back up.
#[allow(clippy::too_many_arguments)]
fn own_sync(items: Res<Items>, st: Res<HealthState>, planet: Res<PlanetRes>, area: Res<DevArea>, mut own: ResMut<OwnVitals>, mut hud: ResMut<crate::crew_hud::Vitals>, mut actions: ResMut<Actions>, mut players: Query<&mut Player>) {
    let v = match items.role {
        Role::Client => items.vitals_in,
        _ => st.bodies.get(&LOCAL).map(|b| vitals_of(&st.tuning, b)),
    };
    let Some(v) = v else { return };
    let prev = own.last;
    let was_down = prev.is_some_and(|l| l.down.is_some());
    if was_down && v.down.is_none()
        && let Ok(mut pl) = players.single_mut()
    {
        to_pad(&mut pl, &planet, &area);
    }
    // The HUD is written when the vitals change, so a scenario's own readings stay until then.
    if prev != Some(v) {
        hud.health = v.health;
        hud.oxygen = v.oxygen;
        hud.burn = v.burn;
        hud.radiation = v.radiation;
    }
    own.last = Some(v);
    if v.down.is_some() {
        // Down: no walking, no turning, no looking.
        actions.move_dir = DVec3::ZERO;
        actions.look = Vec2::ZERO;
        actions.turn = DVec2::ZERO;
        actions.roll = 0.0;
    }
}

/// Puts the walker on the dev area's pad, on foot, with its feet on the ground.
fn to_pad(pl: &mut Player, planet: &PlanetRes, area: &DevArea) {
    let Some((feet, up)) = area.ground(planet, PAD_MAIN, 2.0) else { return };
    pl.ship = None;
    pl.seated = false;
    pl.fly = false;
    pl.body = None;
    pl.up = up;
    pl.view_up = up;
    pl.cabin_up = DVec3::Y;
    pl.w.pos = feet;
    pl.w.vel = DVec3::ZERO;
    pl.w.move_vel = DVec3::ZERO;
    pl.w.align(up, 0.0);
}

#[derive(Component)]
struct DownRoot;

#[derive(Component)]
struct DownText;

/// Window: the centred line, once the HUD's fonts are there.
fn spawn_down_line(mut commands: Commands, fonts: Option<Res<crate::crew_hud::HudFonts>>, look: Res<crate::displays::DisplayLook>, existing: Query<(), With<DownRoot>>) {
    let Some(fonts) = fonts else { return };
    if !existing.is_empty() {
        return;
    }
    let size = 34.0;
    commands
        .spawn((DownRoot, Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: percent(40.0), justify_content: JustifyContent::Center, display: Display::None, ..default() }))
        .with_children(|root| {
            root.spawn((DownText, crate::crew_hud::hud_caps(&fonts.display, "", size, crate::crew_hud::col(look.0.colours.fg_title), &look.0.hud, 0.1)));
        });
}

/// Window: the line while down, its count rounded up.
fn update_down_line(own: Res<OwnVitals>, mut roots: Query<&mut Node, With<DownRoot>>, mut texts: Query<&mut Text, With<DownText>>) {
    let down = own.last.and_then(|l| l.down);
    for mut n in &mut roots {
        n.display = if down.is_some() { Display::Flex } else { Display::None };
    }
    if let Some(left) = down {
        for mut t in &mut texts {
            let s = format!("DOWN — back in {} s", left.ceil().max(0.0) as u32);
            if t.0 != s {
                t.0 = s;
            }
        }
    }
}
