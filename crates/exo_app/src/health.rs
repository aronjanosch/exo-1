//! Health (#295): players have health, hazards hurt them, items heal them, death respawns them.
//! `health_core` holds the rules; this is the glue.
//!
//! - The host (slot 1; in one process the local player) owns every player's `Health`. Each fixed
//!   tick it adds the hazards under each player (the dev area's pads, `dev_area::hazards`), steps
//!   the body with the air at its place, and counts down a dead player's respawn.
//! - A use (LMB with a bandage or deglow in hand) is a request like any item request
//!   (`Request::Use`): the client aims at another player within 2 m (or itself), the host takes
//!   one from the stack and applies the treatment (`apply_uses`). A treatment with `use_s` takes
//!   that long with LMB held (`UseHold`, the reticle's ring); letting go, switching slot or the
//!   aim moving to another target cancels it.
//! - The host sends each client its own vitals when they change noticeably (`vitals_out`, over
//!   the reliable channel). Every process writes its own vitals into the HUD's `crew_hud::Vitals`
//!   (`own_sync`); while down, the controls are frozen and a centred line counts the respawn.
//! - Unconscious (vitality at zero or below, #300): the controls are frozen and no item is used,
//!   the screen fades to near black (`Faint`, about a second) and a centred line says so; awake
//!   again (vitality above zero, a treatment or the air) the screen fades back. Others see the
//!   player lying down (`public_status`).
//! - A respawn puts the walker on the dev area's pad with clean health. A client moves its own
//!   walker when the host's vitals say it is up again.
use crate::dev_area::hazards::{GLOW_BARREL, HOT_PLATE, NO_AIR};
use crate::dev_area::engine_room::EngineAir;
use crate::dev_area::fire::FireHeat;
use crate::dev_area::{area_of, DevArea, Zone, PAD_MAIN};
use crate::controls::{Actions, Controls};
use crate::env::PlanetRes;
use crate::items::{Items, Role, LOCAL};
use crate::net_live::{Net, RemoteWalker};
use crate::origin::WorldPose;
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::{DVec2, DVec3};
use bevy::prelude::*;
use health_core::{Env, Health, HealthEvent, HealthTuning, Life, Mobility};
use item_core::wire::Vitals;
use item_core::{PlayerId, Request};
use std::collections::BTreeMap;
use walker_core::Frame;

/// Skill of every treater for now (TODO(initiator): skills per crew role).
pub const TREAT_SKILL: f64 = 50.0;
/// A player's capsule: feet to head (m) and its radius.
const CAPSULE_HEIGHT: f64 = 1.8;
const CAPSULE_RADIUS: f64 = 0.4;

pub fn plugin(app: &mut App) {
    let files = crate::content_files("gameplay");
    let tuning = HealthTuning::load(&files).unwrap_or_else(|e| panic!("health: {e:?}"));
    app.insert_resource(HealthState::new(tuning)).init_resource::<OwnVitals>().init_resource::<UseHold>().init_resource::<Faint>().insert_resource(OwnMobility(Mobility { walk: 1.0, sprint: true }));
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

    /// The player left: their body goes.
    pub fn forget(&mut self, id: PlayerId) {
        self.bodies.remove(&id);
        self.sent.remove(&id);
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

/// A use in progress: LMB held on a treatment that takes time (`use_s`), for the reticle's ring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hold {
    pub slot: usize,
    pub target: PlayerId,
    pub elapsed: f64,
    pub need: f64,
}

/// The local use in progress, if any (#295).
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct UseHold(pub Option<Hold>);

impl UseHold {
    /// How far the hold has got, 0..1; None when nothing is held.
    pub fn progress(&self) -> Option<f64> {
        self.0.map(|h| (h.elapsed / h.need).clamp(0.0, 1.0))
    }
}

/// How far the screen has faded to black for an unconscious player, 0..1 (the window draws it at
/// `FAINT_ALPHA` of black). The fade takes `FADE_S` each way.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct Faint {
    pub level: f64,
}

/// The fade's time, each way (s), and the black it reaches (alpha: almost black).
pub const FADE_S: f64 = 1.0;
pub const FAINT_ALPHA: f32 = 0.95;

/// How the local player walks now (the nausea's share, sprint or not): the walker reads it. The
/// host's own body, or what the host sent a client.
#[derive(Resource, Clone, Copy, Debug)]
pub struct OwnMobility(pub Mobility);

/// What the player's body shows (the wire's vitals): shares of the maximum before a headache, the
/// burn's level, the buildup and the fire, the nausea and the walk.
pub fn vitals_of(t: &HealthTuning, b: &Body) -> Vitals {
    let h = &b.health;
    let share = |id: &str| t.affliction(id).map_or(0.0, |k| (h.strength(id) / k.max).clamp(0.0, 1.0));
    let m = h.mobility(t);
    Vitals {
        health: (h.vitality() / h.max_vitality()).clamp(0.0, 1.0),
        max_share: h.max_share(t),
        // The bar shows the air left: full when the air is good, falling as the low grows.
        oxygen: (h.strength("oxygen_low") > 0.0).then(|| 1.0 - share("oxygen_low")),
        burn_level: h.burn_level(t),
        buildup: h.buildup(),
        burning: h.burning(),
        radiation: share("radiation"),
        sick: h.nausea(t) > 0.0,
        walk: m.walk,
        sprint: m.sprint,
        down: b.respawn_in,
        unconscious: h.state() == Life::Unconscious,
        burn_cost: h.cost(t, "burn"),
        oxygen_cost: h.cost(t, "oxygen_low"),
        headache_cost: h.cost(t, "headache"),
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
        || (a.max_share - b.max_share).abs() > 0.005
        || (a.radiation - b.radiation).abs() > 0.005
        || (a.buildup - b.buildup).abs() > 0.02
        || moved(a.oxygen, b.oxygen, 0.01)
        || a.burn_level != b.burn_level
        || a.burning != b.burning
        || a.sick != b.sick
        || (a.walk - b.walk).abs() > 1e-9
        || a.sprint != b.sprint
        || moved(a.down.map(f64::floor), b.down.map(f64::floor), 0.0)
        || a.unconscious != b.unconscious
        || (a.burn_cost - b.burn_cost).abs() > 0.5
        || (a.oxygen_cost - b.oxygen_cost).abs() > 0.5
        || (a.headache_cost - b.headache_cost).abs() > 0.5
}

/// A player's place in the area (metres), if on foot on the planet and the dev area is there.
pub(crate) fn area_pos(area: Option<&DevArea>, planet: &PlanetRes, p: Option<DVec3>) -> Option<[f64; 2]> {
    let (f, p) = (area?.frame?, p?);
    Some(area_of(&f, planet, p))
}

/// Every player who is on foot in the world (the local one and the peers): where they stand.
pub(crate) fn walker_points(players: &Query<&Player>, remotes: &Query<(&RemoteWalker, &WorldPose)>) -> BTreeMap<PlayerId, DVec3> {
    let mut at: BTreeMap<PlayerId, DVec3> = BTreeMap::new();
    if let Ok(pl) = players.single()
        && pl.ship.is_none()
    {
        at.insert(LOCAL, pl.w.pos);
    }
    for (rw, wp) in remotes {
        at.insert(rw.owner, wp.pos);
    }
    at
}

/// Host: the hazards and the air under every body, one step, the respawn countdown. The fire's
/// burn on a body (`FireHeat`, set by the fire step before this one) is its rate per second.
#[allow(clippy::too_many_arguments)]
fn health_step(time: Res<Time>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, air: Option<Res<EngineAir>>, items: Res<Items>, mut st: ResMut<HealthState>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, net: Option<Res<Net>>, heat: Option<Res<FireHeat>>) {
    if items.role == Role::Client {
        return;
    }
    let dt = time.delta_secs_f64();
    let at = walker_points(&players, &remotes);
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
        let here = area_pos(area.as_deref(), &planet, at.get(id).copied());
        let inside = |z: &Zone| here.is_some_and(|p| z.contains(p));
        // The air is the room's (the engine room's deck); the no-air booth takes it all away. The
        // hot plate and the glow feed the body's own rules, per tick or into the buildup.
        // Without the dev area's engine room there is no deck: the air is full.
        let room_air = match (here, air.as_deref()) {
            (Some(p), Some(a)) => a.oxygen_at(p),
            _ => 1.0,
        };
        let fire = heat.as_ref().and_then(|h| h.0.get(id)).copied().unwrap_or(0.0);
        let env = Env { oxygen: if inside(&NO_AIR) { 0.0 } else { room_air }, hot: inside(&HOT_PLATE), glow: inside(&GLOW_BARREL), fire };
        for ev in b.health.step(t, dt, env) {
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

/// The first hit of the aim on another player's capsule, within reach: the player's slot. A
/// player who is down or unconscious lies: the aim meets a capsule along the ground, from the
/// feet on the figure's side (`lying`), where the upright one would miss it.
pub(crate) fn aim_target(eye: DVec3, dir: DVec3, planet: &PlanetRes, remotes: &Query<(&RemoteWalker, &WorldPose)>, lying: &dyn Fn(PlayerId) -> bool, reach: f64) -> Option<PlayerId> {
    let arr = |v: DVec3| [v.x, v.y, v.z];
    let mut best: Option<(f64, PlayerId)> = None;
    for (rw, wp) in remotes {
        let up = (wp.pos - planet.centre).normalize();
        let hit = if lying(rw.owner) {
            let side = (wp.rot * DVec3::X).normalize();
            let feet = wp.pos + up * CAPSULE_RADIUS;
            health_core::capsule_hit(arr(eye), arr(dir), reach, arr(feet), arr(side), CAPSULE_HEIGHT, CAPSULE_RADIUS)
        } else {
            health_core::capsule_hit(arr(eye), arr(dir), reach, arr(wp.pos), arr(up), CAPSULE_HEIGHT, CAPSULE_RADIUS)
        };
        if let Some(t) = hit
            && best.is_none_or(|(bt, _)| t < bt)
        {
            best = Some((t, rw.owner));
        }
    }
    best.map(|(_, id)| id)
}

/// Client and host: LMB with a treatment in the hand asks for its use on the player aimed at, or
/// on oneself. A treatment with `use_s` needs LMB held that long; letting go, switching slot or
/// the aim moving to another target cancels it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn use_input(time: Res<Time>, planet: Res<PlanetRes>, mut controls: ResMut<Controls>, mut items: ResMut<Items>, st: Res<HealthState>, own: Res<OwnVitals>, active: Res<crate::displays::ActiveDisplay>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, mut hold: ResMut<UseHold>) {
    let Ok(pl) = players.single() else {
        hold.0 = None;
        return;
    };
    if own.last.is_some_and(|v| v.unconscious) {
        // Unconscious: no use, and a click made now does not fire when awake.
        controls.click = false;
        hold.0 = None;
        return;
    }
    if controls.free_cursor || active.using().is_some() || pl.seated || pl.fly {
        hold.0 = None;
        return;
    }
    let slot = items.client.view.selected;
    let use_s = items.client.view.slot(slot).cloned().and_then(|stack| {
        let d = items.defs.of(&stack)?;
        let effect = d.use_effect.clone()?;
        st.tuning.treatment(&effect)?;
        Some(d.use_s)
    });
    let Some(use_s) = use_s else {
        hold.0 = None;
        return;
    };
    let target = if pl.ship.is_none() {
        let eye = pl.w.pos + pl.up * EYE_HEIGHT;
        let lying = |id: PlayerId| items.public.iter().any(|(p, s)| *p == id && (s.down || s.unconscious));
        aim_target(eye, pl.world_look(Frame::IDENTITY), &planet, &remotes, &lying, st.tuning.use_reach).unwrap_or(LOCAL)
    } else {
        LOCAL
    };
    let now = time.elapsed_secs_f64();
    match hold.0 {
        None => {
            if !controls.click {
                return;
            }
            controls.click = false;
            match use_s {
                Some(need) if need > 0.0 => hold.0 = Some(Hold { slot, target, elapsed: 0.0, need }),
                _ => {
                    let _ = items.request(now, Request::Use { slot, target });
                }
            }
        }
        Some(h) => {
            if !controls.primary_held || h.slot != slot || h.target != target {
                hold.0 = None;
                return;
            }
            let elapsed = h.elapsed + time.delta_secs_f64();
            if elapsed >= h.need {
                hold.0 = None;
                let _ = items.request(now, Request::Use { slot: h.slot, target: h.target });
            } else {
                hold.0 = Some(Hold { elapsed, ..h });
            }
        }
    }
}

/// Every process: its own vitals into the HUD; frozen controls and a walk to the pad when down or
/// back up; the fade when unconscious.
#[allow(clippy::too_many_arguments)]
fn own_sync(items: Res<Items>, st: Res<HealthState>, planet: Res<PlanetRes>, area: Option<Res<DevArea>>, time: Res<Time>, mut own: ResMut<OwnVitals>, mut faint: ResMut<Faint>, mut mobility: ResMut<OwnMobility>, mut hud: ResMut<crate::crew_hud::Vitals>, mut actions: ResMut<Actions>, mut players: Query<&mut Player>) {
    let v = match items.role {
        Role::Client => items.vitals_in,
        _ => st.bodies.get(&LOCAL).map(|b| vitals_of(&st.tuning, b)),
    };
    let Some(v) = v else { return };
    let want = if v.unconscious { 1.0 } else { 0.0 };
    let step = time.delta_secs_f64() / FADE_S;
    faint.level = if faint.level < want { (faint.level + step).min(want) } else { (faint.level - step).max(want) };
    let prev = own.last;
    let was_down = prev.is_some_and(|l| l.down.is_some());
    if was_down && v.down.is_none()
        && let Ok(mut pl) = players.single_mut()
    {
        if let Some(area) = area.as_deref() {
            to_pad(&mut pl, &planet, area);
        }
    }
    mobility.0 = Mobility { walk: v.walk, sprint: v.sprint };
    // The HUD is written when the vitals change, so a scenario's own readings stay until then.
    if prev != Some(v) {
        *hud = crate::crew_hud::Vitals {
            health: v.health,
            max_share: v.max_share,
            oxygen: v.oxygen,
            burn_level: v.burn_level,
            buildup: v.buildup,
            burning: v.burning,
            radiation: v.radiation,
            sick: v.sick,
            burn_cost: v.burn_cost,
            oxygen_cost: v.oxygen_cost,
            headache_cost: v.headache_cost,
        };
    }
    own.last = Some(v);
    if v.down.is_some() || v.unconscious {
        // Down or unconscious: no walking, no turning, no looking, no jump.
        actions.move_dir = DVec3::ZERO;
        actions.look = Vec2::ZERO;
        actions.turn = DVec2::ZERO;
        actions.roll = 0.0;
        actions.jump = false;
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

/// The full-screen black of the fade (under the line).
#[derive(Component)]
struct FaintRoot;

#[derive(Component)]
struct DownText;

/// Window: the centred line, once the HUD's fonts are there.
fn spawn_down_line(mut commands: Commands, fonts: Option<Res<crate::crew_hud::HudFonts>>, look: Res<crate::displays::DisplayLook>, existing: Query<(), With<DownRoot>>) {
    let Some(fonts) = fonts else { return };
    if !existing.is_empty() {
        return;
    }
    let size = 34.0;
    let full = Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: px(0), bottom: px(0), ..default() };
    commands.spawn((FaintRoot, GlobalZIndex(0), full, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0))));
    commands
        .spawn((DownRoot, GlobalZIndex(1), Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: percent(40.0), justify_content: JustifyContent::Center, display: Display::None, ..default() }))
        .with_children(|root| {
            root.spawn((DownText, crate::crew_hud::hud_caps(&fonts.display, "", size, crate::crew_hud::col(look.0.colours.fg_title), &look.0.hud, 0.1)));
        });
}

/// Window: the line while down (its count rounded up) or unconscious; the fade's black.
fn update_down_line(own: Res<OwnVitals>, faint: Res<Faint>, mut roots: Query<&mut Node, With<DownRoot>>, mut texts: Query<&mut Text, With<DownText>>, mut fades: Query<&mut BackgroundColor, With<FaintRoot>>) {
    let down = own.last.and_then(|l| l.down);
    let unconscious = own.last.is_some_and(|l| l.unconscious);
    for mut n in &mut roots {
        n.display = if down.is_some() || unconscious { Display::Flex } else { Display::None };
    }
    let s = match down {
        Some(left) => format!("DOWN — back in {} s", left.ceil().max(0.0) as u32),
        None => "UNCONSCIOUS".to_string(),
    };
    for mut t in &mut texts {
        if t.0 != s && (down.is_some() || unconscious) {
            t.0 = s.clone();
        }
    }
    let black = Color::srgba(0.0, 0.0, 0.0, FAINT_ALPHA * faint.level as f32);
    for mut bg in &mut fades {
        if bg.0 != black {
            bg.0 = black;
        }
    }
}
