//! Cargo crates on the network (#332), the same model as loose items: the host owns them.
//!
//! - Every crate on the host gets a stable `CrateId` (`grab_core::holders` hands them out, never
//!   twice). The host tells the clients: `Spawn` (kind, tub rack, pose, holder), `Despawn`, a
//!   pose datagram stream while a crate moves (ids tagged with `CRATE_KEY`, at the snapshot rate)
//!   and one `Rest` when it stops. A late joiner gets a `Spawn` for each crate.
//! - A client's copy is a picture (`LooseView`, no physics here): `crate_views` poses it from the
//!   track of the host's stream, in the frame the host named (a planet, or a ship's cabin).
//! - Carrying: whoever holds a crate drives it. The host's own hold runs as before; a client asks
//!   (`CrateAsk::Take`), starts carrying at once (its copy becomes a simulated body again, hold
//!   force and all) and streams the pose to the host. The host's book (`Holders`) lets one
//!   player hold at a time, first ask wins; a refusal rolls the client back. On the host a crate
//!   a client holds is a view too (`RemoteHeld`), posed from the client's stream and relayed to the
//!   others. Letting go sends the last pose and velocity (`Release`); the host's physics takes
//!   over, and the client keeps the crate where it let go until the host's word (`Held` with no
//!   holder, at host time `t`) is past the playout time (`Handback`).
//! - A holder who leaves frees the crate.
//!
//! Not covered: crates in a client's own cabin (the host cannot simulate another ship's frame:
//! a crate set down there stays where it was set), and a client holding a crate in another
//! player's cabin (the grab does not work there anyway).
use crate::avian_crates::BodyParts;
use crate::cargo::{crate_bundle, Crate, Crates};
use crate::env::PlanetRes;
use crate::grab::Grab;
use crate::items::{Items, LooseItem, Role};
use crate::items_net::{body_frame, LooseView, RemoteBodies};
use crate::net_live::Net;
use crate::ore_tub::Tub;
use crate::ship::{RemoteShip, Ship};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use grab_core::holders::Holders;
use item_core::cargo_wire::{key, unkey, CrateAsk, CrateNetId, CrateNews, CratePose};
use item_core::wire::{self, ToClient, ToHost};
use item_core::{FrameRef, Placement, PlayerId};
use net_core::loose::{BodyState, Pose};
use std::collections::{HashMap, HashSet};

/// The host's slot.
const HOST: PlayerId = crate::items::LOCAL;
/// A client gives up waiting for the host's word after a let-go (s).
const HANDBACK_TIMEOUT: f64 = 3.0;

pub fn plugin(app: &mut App) {
    use crate::phases::Fx;
    app.init_resource::<CrateNet>();
    app.add_systems(FixedUpdate, crate_ids.in_set(Fx::Input));
    app.add_systems(FixedUpdate, crate_net_in.run_if(resource_exists::<Net>).after(crate::items_net::items_net_in).in_set(Fx::Input));
    app.add_systems(FixedUpdate, crate_holds.run_if(resource_exists::<Net>).before(crate::grab::grab_step).in_set(Fx::Cargo));
    app.add_systems(FixedUpdate, crate_views.run_if(resource_exists::<Net>).in_set(Fx::Cargo));
    app.add_systems(FixedLast, crate_net_out.run_if(resource_exists::<Net>).after(crate::avian_crates::avian_post).before(crate::net_live::net_channels));
}

/// A cargo crate's id, given by the host.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CrateId(pub CrateNetId);

/// Host: a client holds this crate. It is not simulated here; its pose is the client's stream.
#[derive(Component, Clone, Copy, Debug)]
pub struct RemoteHeld(pub PlayerId);

/// Client: let go of this crate; it stays where it was let go until the host's word is past.
#[derive(Component, Clone, Copy, Debug)]
pub struct Handback {
    /// Host time of the host's word (`Held` with no holder), once it came.
    pub until: Option<f64>,
    pub since: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Mine {
    pub id: CrateNetId,
    pub entity: Entity,
    /// The host said yes.
    pub confirmed: bool,
}

#[derive(Resource, Default)]
pub struct CrateNet {
    /// Host: the book. Client: what the host last said about each crate.
    pub book: Holders,
    /// Asks from the clients (host) and news from the host (client), put here by `items_net_in`.
    pub asks: Vec<(PlayerId, CrateAsk)>,
    pub news: Vec<CrateNews>,
    pub new_peers: Vec<PlayerId>,
    pub left_peers: Vec<PlayerId>,
    /// Client: the crate this client carries (or has asked for).
    pub mine: Option<Mine>,
    /// Client: the crates the host refused (for tests and the log).
    pub refused: Vec<CrateNetId>,
    announced: HashSet<CrateNetId>,
    moving: HashSet<CrateNetId>,
    client_ready: bool,
}

fn rot_of(q: [f64; 4]) -> DQuat {
    let q = DQuat::from_xyzw(q[0], q[1], q[2], q[3]);
    if q.is_finite() && q.length_squared() > 0.5 { q.normalize() } else { DQuat::IDENTITY }
}

fn pose_of(c: &Crate, frame: FrameRef) -> CratePose {
    let r = c.body.rot();
    CratePose { frame, pos: c.body.pos.to_array(), vel: c.body.vel.to_array(), rot: [r.x, r.y, r.z, r.w] }
}

fn placement_of(p: &CratePose) -> Placement {
    let fwd = rot_of(p.rot) * DVec3::NEG_Z;
    Placement { frame: p.frame, pos: p.pos, vel: p.vel, forward: fwd.to_array() }
}

/// The frame a crate of this process is posed in on the wire.
fn frame_of(c: &Crate, own: u32, planet: &PlanetRes, remotes: &Query<&RemoteShip>) -> FrameRef {
    match c.ship {
        Some(e) => FrameRef::Ship(remotes.get(e).map_or(own, |r| r.owner)),
        None => FrameRef::Planet(planet.id.0 as u32),
    }
}

/// The host numbers every crate that appears (a client's copies come with the host's number).
pub fn crate_ids(mut commands: Commands, mut cn: ResMut<CrateNet>, items: Res<Items>, new: Query<Entity, (Added<Crate>, Without<CrateId>, Without<LooseItem>)>) {
    if items.role == Role::Client {
        return;
    }
    for e in &new {
        commands.entity(e).insert(CrateId(cn.book.register()));
    }
}

fn send(net: &mut Net, to: Option<PlayerId>, n: CrateNews) {
    net.send_reliable(to, &wire::encode(&ToClient::Crate(n)));
}

/// Takes a crate back from a client's hold on the host: resumes its physics at `at` on the
/// planet, or leaves it where it is when it is posed in a client's cabin (the host cannot
/// simulate that). Returns the pose it now has.
fn host_let_go(commands: &mut Commands, e: Entity, c: &mut Crate, at: CratePose, planet: &PlanetRes, now: f64, bodies: &mut RemoteBodies, id: CrateNetId, t: f64) -> CratePose {
    commands.entity(e).remove::<RemoteHeld>();
    match at.frame {
        FrameRef::Planet(n) if n == planet.id.0 as u32 => {
            let rot = rot_of(at.rot);
            c.ship = None;
            c.planet = None;
            c.body.pos = DVec3::from_array(at.pos);
            c.body.vel = DVec3::from_array(at.vel);
            c.body.forward = rot * DVec3::NEG_Z;
            c.body.up = rot * DVec3::Y;
            c.body.wake();
            c.touched = Some(now);
            commands.entity(e).remove::<LooseView>();
            at
        }
        _ => {
            c.body.vel = DVec3::ZERO;
            c.body.asleep = true;
            let rot = c.body.rot();
            let at = CratePose { vel: [0.0; 3], rot: [rot.x, rot.y, rot.z, rot.w], ..at };
            bodies.tracks.entry(key(id)).or_default().push(Pose { t, frame: crate::items_net::body_frame(at.frame), pos: DVec3::from_array(at.pos), vel: DVec3::ZERO, rot });
            at
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn crate_net_in(
    mut commands: Commands,
    time: Res<Time>,
    mut net: ResMut<Net>,
    mut cn: ResMut<CrateNet>,
    items: Res<Items>,
    table: Res<Crates>,
    planet: Res<PlanetRes>,
    mut bodies: ResMut<RemoteBodies>,
    mut grab: ResMut<Grab>,
    mut crates: Query<(Entity, &CrateId, &mut Crate, Option<&Handback>), Without<LooseItem>>,
    remotes: Query<&RemoteShip>,
) {
    let now = time.elapsed_secs_f64();
    let cn = &mut *cn;
    match items.role {
        Role::Host => {
            for slot in std::mem::take(&mut cn.left_peers) {
                let t = net.server_now();
                for id in cn.book.player_left(slot) {
                    let Some((e, _, mut c, _)) = crates.iter_mut().find(|(_, i, ..)| i.0 == id) else { continue };
                    // Where it was, at rest. A crate in the leaver's cabin goes with the ship.
                    let frame = frame_of(&c, HOST, &planet, &remotes);
                    let at = CratePose { vel: [0.0; 3], ..pose_of(&c, frame) };
                    if matches!(at.frame, FrameRef::Ship(n) if n != HOST) {
                        commands.entity(e).despawn();
                        continue;
                    }
                    let at = host_let_go(&mut commands, e, &mut c, at, &planet, now, &mut bodies, id, t);
                    send(&mut net, None, CrateNews::Held { id, by: None, at, t });
                }
            }
            for (slot, d) in net.take_peer_bodies() {
                let server = net.server_now();
                // Only the holder's poses count; the stamp is the client's estimate of host time, trusted within reason.
                let t = if (d.t - server).abs() < 2.0 { d.t } else { server };
                for s in &d.bodies {
                    if let Some(id) = unkey(s.id)
                        && cn.book.holder(id) == Some(slot)
                    {
                        bodies.tracks.entry(s.id).or_default().push(Pose { t, ..Pose::of(t, s) });
                    }
                }
            }
            for (slot, ask) in std::mem::take(&mut cn.asks) {
                match ask {
                    CrateAsk::Take { id } => {
                        let found = crates.iter_mut().find(|(_, i, ..)| i.0 == id);
                        let ok = found.is_some() && cn.book.ask(id, slot).is_ok();
                        let Some((e, _, mut c, _)) = found.filter(|_| ok) else {
                            send(&mut net, Some(slot), CrateNews::Refused { id });
                            continue;
                        };
                        let frame = frame_of(&c, HOST, &planet, &remotes);
                        let at = pose_of(&c, frame);
                        c.body.asleep = false;
                        c.touched = Some(now);
                        commands.entity(e).remove::<BodyParts>().insert((RemoteHeld(slot), LooseView::new(placement_of(&at))));
                        let t = net.server_now();
                        send(&mut net, None, CrateNews::Held { id, by: Some(slot), at, t });
                    }
                    CrateAsk::Release { id, at } => {
                        if cn.book.holder(id) != Some(slot) {
                            continue;
                        }
                        cn.book.release(id, slot);
                        let Some((e, _, mut c, _)) = crates.iter_mut().find(|(_, i, ..)| i.0 == id) else { continue };
                        let t = net.server_now();
                        let own = frame_of(&c, HOST, &planet, &remotes);
                        // A pose that cannot be, or one in the host's own cabin (no client is there), is not taken: the crate stays where the stream left it.
                        let at = if at.sane() && !matches!(at.frame, FrameRef::Ship(n) if n == HOST) { at } else { CratePose { vel: [0.0; 3], ..pose_of(&c, own) } };
                        let at = host_let_go(&mut commands, e, &mut c, at, &planet, now, &mut bodies, id, t);
                        send(&mut net, None, CrateNews::Held { id, by: None, at, t });
                    }
                }
            }
        }
        Role::Client => {
            if !cn.client_ready {
                // The crates this process made itself (level content) are the host's to put there.
                cn.client_ready = true;
                for (e, ..) in &crates {
                    commands.entity(e).despawn();
                }
                cn.book = Holders::default();
                cn.announced.clear();
            }
            let me = items.me();
            let mut have: HashMap<CrateNetId, Entity> = crates.iter().map(|(e, i, ..)| (i.0, e)).collect();
            for n in std::mem::take(&mut cn.news) {
                match n {
                    CrateNews::Spawn { id, size, rack, at, by } => {
                        cn.book.learn(id, by);
                        if have.contains_key(&id) || table.0.get(&size).is_none() {
                            continue; // told twice (a rejoin), or a crate kind this build does not know
                        }
                        let p = placement_of(&at);
                        let mut e = commands.spawn((crate_bundle(&table.0, &size, None, DVec3::from_array(at.pos), DVec3::from_array(p.forward)), CrateId(id), LooseView::new(p)));
                        if let Some(rack) = rack {
                            e.insert(Tub { rack });
                        }
                        have.insert(id, e.id());
                    }
                    CrateNews::Despawn { id } => {
                        cn.book.forget(id);
                        bodies.tracks.remove(&key(id));
                        if let Some(e) = have.remove(&id) {
                            commands.entity(e).despawn();
                        }
                        if cn.mine.is_some_and(|m| m.id == id) {
                            cn.mine = None;
                            grab.release();
                        }
                    }
                    CrateNews::Rest { id, at, t } => {
                        bodies.tracks.entry(key(id)).or_default().push(Pose { t, frame: body_frame(at.frame), pos: DVec3::from_array(at.pos), vel: DVec3::ZERO, rot: rot_of(at.rot) });
                    }
                    CrateNews::Held { id, by, at, t } => {
                        cn.book.learn(id, by);
                        match by {
                            Some(p) if p == me => {
                                if let Some(m) = cn.mine.as_mut().filter(|m| m.id == id) {
                                    m.confirmed = true;
                                }
                            }
                            Some(_) => {
                                // Someone else has it: if this client believed it had it, it did not.
                                if cn.mine.is_some_and(|m| m.id == id) {
                                    rollback(&mut commands, &mut cn.mine, &mut grab, &crates, me, &planet, &remotes);
                                }
                            }
                            None => {
                                bodies.tracks.entry(key(id)).or_default().push(Pose { t, frame: body_frame(at.frame), pos: DVec3::from_array(at.pos), vel: DVec3::from_array(at.vel), rot: rot_of(at.rot) });
                                if let Some(e) = have.get(&id)
                                    && let Ok((_, _, _, Some(h))) = crates.get(*e)
                                {
                                    commands.entity(*e).insert(Handback { until: Some(t), since: h.since });
                                }
                            }
                        }
                    }
                    CrateNews::Refused { id } => {
                        cn.refused.push(id);
                        if cn.mine.is_some_and(|m| m.id == id) {
                            rollback(&mut commands, &mut cn.mine, &mut grab, &crates, me, &planet, &remotes);
                        }
                    }
                }
            }
            // The handback is over once the host's word is past the playout time (or it never came).
            if net.connected() {
                let playout = net.playout_time();
                for (e, _, _, h) in &crates {
                    if let Some(h) = h
                        && (h.until.is_some_and(|t| playout >= t) || now - h.since > HANDBACK_TIMEOUT)
                    {
                        commands.entity(e).remove::<Handback>();
                    }
                }
            }
        }
        Role::Local => {
            cn.asks.clear();
            cn.news.clear();
        }
    }
}

/// The host did not give the crate: the client lets go and shows the host's picture again.
#[allow(clippy::type_complexity)]
fn rollback(commands: &mut Commands, mine: &mut Option<Mine>, grab: &mut Grab, crates: &Query<(Entity, &CrateId, &mut Crate, Option<&Handback>), Without<LooseItem>>, me: u32, planet: &PlanetRes, remotes: &Query<&RemoteShip>) {
    let Some(m) = mine.take() else { return };
    if grab.held.is_some_and(|h| h.crate_e == m.entity) {
        grab.release();
    }
    if let Ok((_, _, c, _)) = crates.get(m.entity) {
        let at = pose_of(c, frame_of(c, me, planet, remotes));
        commands.entity(m.entity).remove::<BodyParts>().insert(LooseView::new(placement_of(&at)));
    }
}

/// Keeps the hold of this process and the book in step. Host: the own hold goes into the book (a
/// refusal ends it, as a client's does); client: a hold starts an ask, a let-go a release.
#[allow(clippy::too_many_arguments)]
pub fn crate_holds(
    mut commands: Commands,
    time: Res<Time>,
    mut net: ResMut<Net>,
    mut cn: ResMut<CrateNet>,
    items: Res<Items>,
    planet: Res<PlanetRes>,
    mut bodies: ResMut<RemoteBodies>,
    mut grab: ResMut<Grab>,
    mut crates: Query<(Entity, &CrateId, &mut Crate), Without<LooseItem>>,
    remotes: Query<&RemoteShip>,
) {
    let now = time.elapsed_secs_f64();
    let cn = &mut *cn;
    let held = grab.held.and_then(|h| crates.get(h.crate_e).ok().map(|(e, i, _)| (e, i.0)));
    match items.role {
        Role::Host => {
            let in_book = cn.book.held_by(HOST);
            if held.map(|h| h.1) == in_book {
                return;
            }
            let t = net.server_now();
            if let Some(id) = in_book {
                cn.book.release(id, HOST);
                if let Some((_, _, c)) = crates.iter().find(|(_, i, _)| i.0 == id) {
                    let at = pose_of(c, frame_of(c, HOST, &planet, &remotes));
                    send(&mut net, None, CrateNews::Held { id, by: None, at, t });
                }
            }
            if let Some((e, id)) = held {
                match cn.book.ask(id, HOST) {
                    Ok(()) => {
                        let (_, _, c) = crates.get(e).unwrap();
                        let at = pose_of(c, frame_of(c, HOST, &planet, &remotes));
                        send(&mut net, None, CrateNews::Held { id, by: Some(HOST), at, t });
                    }
                    Err(_) => grab.release(),
                }
            }
        }
        Role::Client if net.connected() => {
            let me = items.me();
            match (cn.mine, held) {
                (None, Some((e, id))) => {
                    if cn.book.holder(id).is_some() {
                        grab.release(); // somebody has it already
                        return;
                    }
                    let Ok((_, _, mut c)) = crates.get_mut(e) else { return };
                    // Where the host's picture has it now, as the oldest pose of its track: a rollback shows it again (a crate that never moved has no other).
                    let at = pose_of(&c, frame_of(&c, me, &planet, &remotes));
                    bodies.tracks.entry(key(id)).or_default().push(Pose { t: 0.0, frame: body_frame(at.frame), pos: DVec3::from_array(at.pos), vel: DVec3::ZERO, rot: rot_of(at.rot) });
                    c.body.wake();
                    c.body.vel = DVec3::ZERO;
                    c.touched = Some(now);
                    // The copy is a body again: this process carries it.
                    commands.entity(e).remove::<(LooseView, Handback)>();
                    cn.mine = Some(Mine { id, entity: e, confirmed: false });
                    net.send_reliable(None, &wire::encode(&ToHost::Crate(CrateAsk::Take { id })));
                }
                (Some(m), h) if h.map(|h| h.1) != Some(m.id) => {
                    // Let go (set down, thrown, the hold broke): the last pose goes to the host.
                    cn.mine = None;
                    let Ok((_, _, c)) = crates.get(m.entity) else { return };
                    let at = pose_of(c, frame_of(c, me, &planet, &remotes));
                    net.send_reliable(None, &wire::encode(&ToHost::Crate(CrateAsk::Release { id: m.id, at })));
                    // The physics body goes with the hold: from here the host's word poses it.
                    commands.entity(m.entity).remove::<BodyParts>().insert((LooseView::new(placement_of(&at)), Handback { until: None, since: now }));
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// Poses the crates that are pictures here from the tracks of the stream: on a client every
/// crate it does not carry, on the host the ones a client carries.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn crate_views(
    net: Res<Net>,
    items: Res<Items>,
    bodies: Res<RemoteBodies>,
    ships: Query<Entity, (With<Ship>, Without<RemoteShip>)>,
    mut q: Query<(&CrateId, &mut LooseView, &mut Crate, &mut Visibility, Option<&RemoteHeld>, Option<&Handback>), Without<LooseItem>>,
) {
    if items.role == Role::Local || !net.connected() {
        return;
    }
    let Some(own) = ships.iter().next() else { return };
    let (t, extrapolate, me) = (net.playout_time(), net.extrapolation(), items.me());
    for (id, mut view, mut c, mut vis, held, handback) in &mut q {
        if handback.is_some() {
            continue; // stays where it was let go until the host's word is past
        }
        let pose = bodies.tracks.get(&key(id.0)).and_then(|tr| tr.sample_extrapolated(t, extrapolate)).map(|(p, _)| p);
        let (frame, pos, vel, rot) = match pose {
            Some(p) => (p.frame, p.pos, p.vel, p.rot),
            None => {
                let at = view.spawn;
                (body_frame(at.frame), DVec3::from_array(at.pos), DVec3::ZERO, DQuat::from_mat3(&bevy::math::DMat3::from_cols(DVec3::from_array(at.forward).cross(DVec3::Y).normalize_or(DVec3::X), DVec3::Y, -DVec3::from_array(at.forward).normalize_or(DVec3::NEG_Z))))
            }
        };
        let ship = match frame {
            net_core::loose::BodyFrame::Planet(_) => Some(None),
            net_core::loose::BodyFrame::Ship(n) if n == me => Some(Some(own)),
            net_core::loose::BodyFrame::Ship(n) => net.proxy_of(n).map(Some),
        };
        let Some(ship) = ship else {
            view.placed = false;
            continue;
        };
        view.placed = true;
        c.ship = ship;
        c.body.pos = pos;
        c.body.vel = vel;
        c.body.forward = rot * DVec3::NEG_Z;
        c.body.up = rot * DVec3::Y;
        // A crate a client carries is awake on the host, so the host streams it on to the others.
        c.body.asleep = held.is_none();
        let want = if view.placed { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn crate_net_out(
    time: Res<Time>,
    mut net: ResMut<Net>,
    mut cn: ResMut<CrateNet>,
    items: Res<Items>,
    table: Res<Crates>,
    planet: Res<PlanetRes>,
    crates: Query<(&CrateId, &Crate, Option<&Tub>), Without<LooseItem>>,
    remotes: Query<&RemoteShip>,
    mut accum: Local<f64>,
) {
    if !net.connected() || items.role == Role::Local {
        return;
    }
    let cn = &mut *cn;
    *accum += time.delta_secs_f64();
    let period = 1.0 / net.cfg.rate;
    let due = *accum >= period;
    if due {
        *accum %= period;
    }
    let t = net.server_now();
    match items.role {
        Role::Host => {
            let own = HOST;
            let spawn = |id: CrateNetId, c: &Crate, tub: Option<&Tub>, by: Option<PlayerId>| CrateNews::Spawn {
                id,
                size: table.0.sizes[c.size].name.clone(),
                rack: tub.map(|t| t.rack),
                at: pose_of(c, frame_of(c, own, &planet, &remotes)),
                by,
            };
            // A client that joined gets every crate where it is.
            for slot in std::mem::take(&mut cn.new_peers) {
                for (id, c, tub) in &crates {
                    send(&mut net, Some(slot), spawn(id.0, c, tub, cn.book.holder(id.0)));
                }
            }
            let mut seen: HashSet<CrateNetId> = HashSet::new();
            for (id, c, tub) in &crates {
                seen.insert(id.0);
                if cn.announced.insert(id.0) {
                    send(&mut net, None, spawn(id.0, c, tub, cn.book.holder(id.0)));
                }
            }
            let gone: Vec<CrateNetId> = cn.announced.iter().copied().filter(|id| !seen.contains(id)).collect();
            for id in gone {
                cn.announced.remove(&id);
                cn.moving.remove(&id);
                cn.book.forget(id);
                send(&mut net, None, CrateNews::Despawn { id });
            }
            if due {
                let mut states: Vec<BodyState> = Vec::new();
                for (id, c, _) in &crates {
                    let at = pose_of(c, frame_of(c, own, &planet, &remotes));
                    if !c.body.asleep {
                        states.push(BodyState { id: key(id.0), frame: body_frame(at.frame), pos: c.body.pos, vel: c.body.vel, rot: c.body.rot() });
                        cn.moving.insert(id.0);
                    } else if cn.moving.remove(&id.0) {
                        send(&mut net, None, CrateNews::Rest { id: id.0, at: CratePose { vel: [0.0; 3], ..at }, t });
                    }
                }
                net.send_bodies(t, states);
            }
        }
        Role::Client => {
            if due && let Some(m) = cn.mine {
                if let Some((_, c, _)) = crates.iter().find(|(id, ..)| id.0 == m.id) {
                    let at = pose_of(c, frame_of(c, items.me(), &planet, &remotes));
                    net.send_bodies_to_host(t, vec![BodyState { id: key(m.id), frame: body_frame(at.frame), pos: c.body.pos, vel: c.body.vel, rot: c.body.rot() }]);
                }
            }
        }
        Role::Local => {}
    }
}
