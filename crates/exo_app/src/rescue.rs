//! Rescue: CPR on an unconscious player. `health_core` holds the rules (`Health::cpr`);
//! this is the glue: the hold, the wire, the host's pumping and the freeze.
//!
//! ## What Barotrauma does (checked in `~/Work/exo-research/barotrauma/`)
//!
//! - Who and how: the client's CPR button (`Barotrauma/BarotraumaClient/ClientSource/Characters/
//!   Health/CharacterHealth.cs:300-320`) sets the rescuer's animation to `AnimController.Animation.CPR`
//!   (`BarotraumaShared/SharedSource/Characters/Animation/AnimController.cs:168`) on the selected
//!   character, and only when that character is unconscious or stunned. The target is chosen by
//!   the aim (`Character.cs` `SelectCharacter`, `Character.cs:3110`). There is no skill gate to
//!   start it.
//! - The rescuer is locked: `HumanoidAnimController.UpdateCPR` (`.../Animation/HumanoidAnimController.cs:1187`)
//!   crouches the rescuer, disables head and torso, pulls the patient's torso, and `Character.cs:2287`
//!   keeps walking out of the CPR animation. Ours: the walk and the jump are frozen while the hold lasts.
//! - Per second (`deltaTime`, `HumanoidAnimController.cs:1244-1267`): the patient's oxygen +0.5 when
//!   its air is good (`OxygenAvailable >= InsufficientOxygenThreshold`, 30). Below -10 oxygen it
//!   also stabilises: + clamp(skill x 0.02, 0.5, 2.0) per second. Stabilised means held, not woken.
//! - Per pump (one second, `cprPumpTimer`, `HumanoidAnimController.cs:1275-1310`): from two seconds
//!   of CPR (`cprAnimTimer > 2`), a revive roll with chance clamp((skill x 0.007)^2, 0.05, 0.5)
//!   (`CPRSettings` in `content/Afflictions.xml:8-56`). Success: oxygen = max(oxygen + 10, 10).
//!   Failure effect: below skill 50, internal damage of (50 - skill) x 0.05 on the torso each pump.
//!   At skill 50 the failure costs nothing.
//! - Unconscious oxygen falls 1 per second whatever the air (`CharacterHealth.cs:1027-1032`, death
//!   in about 100 s); awake, it falls 5 per second in bad air and rises 10 in good air (`:1040-1050`).
//!   Our `oxygen_low` runs the other way: `unconscious_per_tick` 1.0 is Barotrauma's rule.
//! - Oxygen-low treatments: Liquid Oxygenite (`content/Items/Medical/medical.xml`, about line 593,
//!   "one dose halts the progression", 2.5 per effect over 60 s) and Deusizine (about line 489,
//!   `SuitableTreatment oxygenlow 100`, 2.5 per effect). Ours: the O2 canister (`content/items/oxy.json`),
//!   an instant 40 off `oxygen_low`, the dose is TODO(initiator).
//!
//! ## Ours
//!
//! - Hands empty (or holding something that is not used): LMB on a lying unconscious player in reach
//!   (the same aim as the treatments). Held for `cpr.start_s` the ring fills, then the pump runs while
//!   LMB stays held and the aim stays on that player. Letting go or aiming elsewhere ends it.
//! - Host authority: the client sends `ToHost::Rescue` (as the treatment hold does); the host keeps
//!   `Rescuers` and pumps the patient each fixed tick (`rescue_step`, `Health::cpr`) only while the
//!   rescuer stands up and the patient lies unconscious. The air that counts is the patient's own
//!   (`HealthState::air`).
//! - Others see the rescuer as `Doing::Reviving(patient)`, labelled REVIVING (`public_status.rs`).
//! - TODO(initiator): the start time, the rate and the revive numbers are in `health.json` (`cpr`);
//!   a skill per crew role (now `TREAT_SKILL`); whether a rescuer can be rescued while pumping.

use crate::controls::Controls;
use crate::displays::ActiveDisplay;
use crate::health::{aim_target, HealthState, OwnVitals, TREAT_SKILL, UseHold};
use crate::items::{Items, Role};
use crate::net_live::RemoteWalker;
use crate::origin::WorldPose;
use crate::walker::{Player, EYE_HEIGHT};
use bevy::math::DVec3;
use bevy::prelude::*;
use health_core::{HealthEvent, Life};
use crate::controls::Actions;
use crate::health::Body;
use item_core::PlayerId;
use std::collections::BTreeMap;
use walker_core::Frame;

pub fn plugin(app: &mut App) {
    app.init_resource::<RescueHold>().init_resource::<Rescuers>();
    app.add_systems(FixedUpdate, (rescue_input, rescue_out).chain().after(crate::health::use_input).before(crate::displays::use_display));
}

/// The CPR hold on the local player: the patient, the time held so far and whether the pump runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kneel {
    pub target: PlayerId,
    pub elapsed: f64,
    /// Held for `start_s` (the ring's time).
    pub need: f64,
    /// The pump runs (the ring is full).
    pub started: bool,
}

/// The local CPR hold, if any.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct RescueHold(pub Option<Kneel>);

impl RescueHold {
    /// The ring while the hold fills, 0..1; None once the pump runs or nothing is held.
    pub fn progress(&self) -> Option<f64> {
        self.0.filter(|k| !k.started).map(|k| (k.elapsed / k.need).clamp(0.0, 1.0))
    }
}

/// Host: who pumps whom (rescuer to patient), as the clients asked.
#[derive(Resource, Default, Debug)]
pub struct Rescuers {
    pub by_rescuer: BTreeMap<PlayerId, PlayerId>,
}

/// Client and host: LMB with the hands free on a lying unconscious player in reach starts the
/// ring; held through it, the pump runs. Movement is frozen by `freeze` meanwhile.
#[allow(clippy::too_many_arguments)]
fn rescue_input(time: Res<Time>, planet: Res<crate::env::PlanetRes>, mut controls: ResMut<Controls>, items: Res<Items>, st: Res<HealthState>, own: Res<OwnVitals>, active: Res<ActiveDisplay>, players: Query<&Player>, remotes: Query<(&RemoteWalker, &WorldPose)>, use_hold: Res<UseHold>, mut hold: ResMut<RescueHold>) {
    let Ok(pl) = players.single() else {
        hold.0 = None;
        return;
    };
    let busy = own.last.is_some_and(|v| v.unconscious || v.down.is_some()) || controls.free_cursor || active.using().is_some() || pl.seated || pl.fly || pl.ship.is_some() || use_hold.0.is_some();
    if busy {
        hold.0 = None;
        return;
    }
    // Hands free: nothing in the selected slot, or something that is not used.
    let slot = items.client.view.selected;
    let usable = items.client.view.slot(slot).and_then(|s| items.defs.of(s)).is_some_and(|d| d.use_effect.is_some());
    if usable {
        hold.0 = None;
        return;
    }
    let eye = pl.w.pos + pl.up * EYE_HEIGHT;
    let lying = |id: PlayerId| items.public.iter().any(|(p, s)| *p == id && (s.down || s.unconscious));
    let aimed = aim_target(eye, pl.world_look(Frame::IDENTITY), &planet, &remotes, &lying, st.tuning.use_reach);
    let want = aimed.filter(|t| items.public.iter().any(|(p, s)| p == t && s.unconscious && !s.down));
    let need = st.tuning.cpr.start_s;
    match hold.0 {
        None => {
            let Some(target) = want else { return };
            if !controls.click {
                return;
            }
            controls.click = false;
            hold.0 = Some(Kneel { target, elapsed: 0.0, need, started: need <= 0.0 });
        }
        Some(k) => {
            if !controls.primary_held || want != Some(k.target) {
                hold.0 = None;
                return;
            }
            let elapsed = (k.elapsed + time.delta_secs_f64()).min(k.need);
            hold.0 = Some(Kneel { elapsed, started: k.started || elapsed >= k.need, ..k });
        }
    }
}

/// Client and host: the pump's request (the patient, or none) goes out when the pump starts or stops.
fn rescue_out(mut items: ResMut<Items>, hold: Res<RescueHold>, mut sent: Local<Option<PlayerId>>) {
    let now = hold.0.filter(|k| k.started).map(|k| k.target);
    if now != *sent {
        *sent = now;
        items.ask_rescue(now);
    }
}

/// Every process: while a CPR hold lasts the walk and the jump are frozen (the rescuer kneels).
pub(crate) fn freeze(hold: Res<RescueHold>, mut actions: ResMut<Actions>) {
    if hold.0.is_some() {
        actions.move_dir = DVec3::ZERO;
        actions.jump = false;
    }
}

/// Host: the requests in, then every body's CPR for this step: the patient is pumped while a
/// rescuer stands up and the patient lies unconscious; the others' CPR counters reset.
pub(crate) fn rescue_step(time: Res<Time>, mut items: ResMut<Items>, mut st: ResMut<HealthState>, mut rs: ResMut<Rescuers>) {
    if items.role == Role::Client {
        return;
    }
    for (who, patient) in std::mem::take(&mut items.rescue_asks) {
        match patient {
            Some(p) => {
                rs.by_rescuer.insert(who, p);
            }
            None => {
                rs.by_rescuer.remove(&who);
            }
        }
    }
    let st = &mut *st;
    let t = &st.tuning;
    let bodies: &mut BTreeMap<PlayerId, Body> = &mut st.bodies;
    let standing = |id: &PlayerId| bodies.get(id).is_some_and(|b| b.respawn_in.is_none() && b.health.state() == Life::Conscious);
    let lying = |id: &PlayerId| bodies.get(id).is_some_and(|b| b.health.state() == Life::Unconscious);
    rs.by_rescuer.retain(|r, p| standing(r) && lying(p));
    let patients: Vec<PlayerId> = rs.by_rescuer.values().copied().collect();
    let dt = time.delta_secs_f64();
    let ids: Vec<PlayerId> = bodies.keys().copied().collect();
    for id in ids {
        let pumped = patients.contains(&id);
        let air_ok = st.air.get(&id).is_some_and(|o| *o >= t.oxygen.low_below);
        let Some(b) = bodies.get_mut(&id) else { continue };
        for ev in b.health.cpr(t, dt, pumped, TREAT_SKILL, air_ok) {
            if matches!(ev, HealthEvent::Died { .. }) {
                b.respawn_in = Some(t.respawn_s);
            }
        }
    }
}
