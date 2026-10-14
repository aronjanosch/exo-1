//! Items over the network (C2 of the interaction model; concept repo `decisions/crew-ship.md`,
//! "Interaction model", and `design/interaction.md`, "Who owns what in co-op").
//!
//! - The host (slot 1) owns every loose item and every inventory: it runs `item_core::Host` and
//!   simulates the loose items as bodies (the cargo code), in the host's ship or on the planet.
//!   Each client simulates only its own walker and shows its requests at once; the host confirms
//!   or rejects them.
//! - Requests (client to host) and events (host to clients) travel over the reliable, ordered
//!   channel of `net_core::reliable`, as `item_core::wire` messages. The host takes the player
//!   from the connection, not from the message. A client that joins gets the state as events.
//! - A loose item sends its pose (`net_core::loose`, unreliable, at the snapshot rate) only while
//!   it moves; when it comes to rest the host sends one `Event::Rest` with the final pose, and
//!   nothing more until it moves again. Poses are in the item's frame: the planet (world
//!   coordinates) or the host's ship (ship-local, so items in the cabin stay where they lie).
//! - A client does not simulate loose items. Its item entities are pictures (`LooseView`):
//!   interpolated at the shared playout time like the other players' ships, riding the host's ship
//!   proxy for items in the cabin. Until the host's first pose arrives a dropped item shows where
//!   it was let go (TODO(initiator): a client-side flight of a throw before the host's answer).
//! - The held item of another player shows in the hand of its figure (placeholder box).
use crate::cargo::Crate;
use crate::env::PlanetRes;
use crate::dev_area::fire::Sprays;
use crate::health::HealthState;
use crate::items::{Items, LooseItem, NetOut, Role};
use crate::net_live::Net;
use crate::repair::RepairSessions;
use crate::ship::Ship;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use item_core::wire::{self, ToClient, ToHost};
use item_core::{Event, FrameRef, LooseId, Placement};
use net_core::loose::{BodyFrame, BodyState, Pose, Track};
use std::collections::{HashMap, HashSet};

pub fn plugin(app: &mut App) {
    use crate::phases::Fx;
    app.init_resource::<RemoteBodies>();
    app.add_systems(FixedUpdate, items_net_in.run_if(resource_exists::<Net>).after(crate::net_live::net_pre).in_set(Fx::Input));
    app.add_systems(FixedUpdate, loose_views.run_if(resource_exists::<Net>).in_set(Fx::Cargo));
    app.add_systems(
        FixedLast,
        items_net_out.run_if(resource_exists::<Net>).after(crate::avian_crates::avian_post).before(crate::net_live::net_channels),
    );
}

/// Window only: the other players' held items, and no loose item before it has a place.
pub fn window_plugin(app: &mut App) {
    use crate::phases::Frame;
    app.add_systems(Update, (remote_held, hide_unplaced.after(crate::items::hide_items)).in_set(Frame::World));
}

/// A client's picture of a loose item (the entity also carries the `Crate` the rest of the game
/// reads, which `crate_step` leaves alone).
#[derive(Component, Debug, Clone)]
pub struct LooseView {
    /// Where the host first said it was, shown until poses arrive.
    pub spawn: Placement,
    /// A frame to put it in was found at the last update (the proxy of a ship may be missing for a moment).
    pub placed: bool,
}

impl LooseView {
    pub fn new(spawn: Placement) -> LooseView {
        LooseView { spawn, placed: false }
    }
}

/// The poses of moving loose items a client has received, by the host's item id. They arrive
/// before or after the item's `Spawned` event, so they live apart from the entities.
#[derive(Resource, Default)]
pub struct RemoteBodies {
    pub tracks: HashMap<LooseId, Track>,
}

pub fn body_frame(f: FrameRef) -> BodyFrame {
    match f {
        FrameRef::Planet(n) => BodyFrame::Planet(n),
        FrameRef::Ship(n) => BodyFrame::Ship(n),
    }
}

impl RemoteBodies {
    /// What an event of the host says about a body's track.
    fn on_event(&mut self, ev: &Event) {
        match ev {
            // The resting pose is the last pose of the track (and the only one of an item that
            // was never streamed, a dropped item the datagrams missed).
            Event::Rest { item, at, rot, t } => {
                let q = DQuat::from_xyzw(rot[0], rot[1], rot[2], rot[3]);
                let q = if q.is_finite() && q.length_squared() > 0.5 { q.normalize() } else { DQuat::IDENTITY };
                self.tracks.entry(*item).or_default().push(Pose { t: *t, frame: body_frame(at.frame), pos: DVec3::from_array(at.pos), vel: DVec3::ZERO, rot: q });
            }
            Event::Removed { item } => {
                self.tracks.remove(item);
            }
            _ => {}
        }
    }
}

/// Role setup on the first tick of a session, then the messages that arrived this tick.
#[allow(clippy::too_many_arguments)]
pub fn items_net_in(mut commands: Commands, time: Res<Time>, mut net: ResMut<Net>, mut items: ResMut<Items>, mut bodies: ResMut<RemoteBodies>, loose: Query<Entity, With<LooseItem>>, mut repairs: Option<ResMut<RepairSessions>>, mut sprays: Option<ResMut<Sprays>>, mut health: Option<ResMut<HealthState>>) {
    let now = time.elapsed_secs_f64();
    if items.role == Role::Local {
        items.role = if net.cfg.host { Role::Host } else { Role::Client };
        if !net.cfg.host {
            // This process is a client: the host's events bring the world. What the one-process
            // game had here (level content) is the host's to put there.
            let defs = items.defs.clone();
            items.client = item_core::Client::new(net.cfg.slot, defs.clone());
            items.host = item_core::Host::new(defs);
            for e in &loose {
                commands.entity(e).despawn();
            }
            bodies.tracks.clear();
        }
        println!("ITEMS role={:?} slot={}", items.role, net.cfg.slot);
    }
    match items.role {
        Role::Host => {
            // A player who left: their repair session, spray and body go (the host owns them).
            for slot in net.take_left_peers() {
                if let Some(r) = repairs.as_mut() {
                    r.forget(slot);
                }
                if let Some(s) = sprays.as_mut() {
                    s.forget(slot);
                }
                if let Some(h) = health.as_mut() {
                    h.forget(slot);
                }
            }
            for slot in net.take_new_peers() {
                items.send_state(slot);
                let r = items.room;
                items.net_out.push(NetOut::Room { to: Some(slot), r });
                if let Some(r) = items.reactor {
                    items.net_out.push(NetOut::Reactor { to: Some(slot), r });
                }
                if let Some(f) = items.fire.clone() {
                    items.net_out.push(NetOut::Fire { to: Some(slot), f });
                }
                let s = items.public.clone();
                items.net_out.push(NetOut::Status { to: Some(slot), s });
            }
            for (slot, msg) in net.take_messages() {
                let ok = match wire::decode::<ToHost>(&msg) {
                    Some(ToHost::Action(a)) => items.from_peer(now, slot, a),
                    Some(ToHost::Room(a)) => {
                        items.ask_room(a);
                        true
                    }
                    Some(ToHost::Reactor(a)) => {
                        items.ask_reactor(a);
                        true
                    }
                    Some(ToHost::Spray(a)) => {
                        items.spray_asks.push((slot, a));
                        true
                    }
                    Some(ToHost::Repair(a)) => {
                        items.repair_asks.push((slot, a));
                        true
                    }
                    Some(ToHost::Treat(t)) => {
                        items.treat_asks.push((slot, t));
                        true
                    }
                    Some(ToHost::Rescue(t)) => {
                        items.rescue_asks.push((slot, t));
                        true
                    }
                    None => false,
                };
                if !ok {
                    net.st.invalid += 1;
                }
            }
        }
        Role::Client => {
            for (_, msg) in net.take_messages() {
                match wire::decode::<ToClient>(&msg) {
                    Some(ToClient::Event(ev)) => {
                        bodies.on_event(&ev);
                        items.hear(now, ev);
                    }
                    Some(ToClient::Vitals(v)) => items.vitals_in = Some(v),
                    Some(ToClient::Room(r)) => items.room = r,
                    Some(ToClient::Reactor(r)) => items.reactor = Some(r),
                    Some(ToClient::Fire(f)) => items.fire = Some(f),
                    Some(ToClient::RepairDone) | Some(ToClient::RepairRefused) => items.repair_done = true,
                    Some(ToClient::Status(s)) => items.public = s,
                    None => net.st.invalid += 1,
                }
            }
            for d in net.take_bodies() {
                for s in &d.bodies {
                    bodies.tracks.entry(s.id).or_default().push(Pose::of(d.t, s));
                }
            }
        }
        Role::Local => {}
    }
}

/// Client: every loose item is shown where the host's poses put it at the playout time. Items in
/// a ship are in that ship's frame: the own ship, or the proxy of the host's.
#[allow(clippy::too_many_arguments)]
pub fn loose_views(
    net: Res<Net>,
    items: Res<Items>,
    planet: Res<PlanetRes>,
    bodies: Res<RemoteBodies>,
    ships: Query<Entity, With<Ship>>,
    mut q: Query<(&LooseItem, &mut LooseView, &mut Crate)>,
) {
    if items.role != Role::Client || !net.connected() {
        return;
    }
    let Some(own) = ships.iter().next() else { return };
    let (t, extrapolate, me) = (net.playout_time(), net.extrapolation(), items.me());
    for (li, mut view, mut c) in &mut q {
        let pose = li.id.and_then(|id| bodies.tracks.get(&id)).and_then(|tr| tr.sample_extrapolated(t, extrapolate)).map(|(p, _)| p);
        let (frame, pos, vel, rot) = match pose {
            Some(p) => (p.frame, p.pos, p.vel, p.rot),
            None => {
                let at = &view.spawn;
                let pos = DVec3::from_array(at.pos);
                let up = match at.frame {
                    FrameRef::Planet(_) => planet.up(pos),
                    FrameRef::Ship(_) => DVec3::Y,
                };
                let fwd = DVec3::from_array(at.forward);
                let fwd = (fwd - up * fwd.dot(up)).try_normalize().unwrap_or(up.any_orthonormal_vector());
                (body_frame(at.frame), pos, DVec3::ZERO, DQuat::from_mat3(&bevy::math::DMat3::from_cols(fwd.cross(up), up, -fwd)))
            }
        };
        let ship = match frame {
            BodyFrame::Planet(_) => Some(None),
            BodyFrame::Ship(n) if n == me => Some(Some(own)),
            BodyFrame::Ship(n) => net.proxy_of(n).map(Some),
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
        c.body.asleep = true;
    }
}

/// Host: drains what `Items` has for the network, and streams the poses of the loose items that
/// move (at the snapshot rate); an item that stopped gets its `Rest` event, then nothing.
#[allow(clippy::too_many_arguments)]
pub fn items_net_out(
    time: Res<Time>,
    mut net: ResMut<Net>,
    mut items: ResMut<Items>,
    planet: Res<PlanetRes>,
    loose: Query<(&LooseItem, &Crate), Without<LooseView>>,
    mut moving: Local<HashSet<LooseId>>,
    mut accum: Local<f64>,
) {
    if items.role == Role::Host && net.connected() {
        *accum += time.delta_secs_f64();
        let period = 1.0 / net.cfg.rate;
        if *accum >= period {
            *accum %= period;
            let (t, now) = (net.server_now(), time.elapsed_secs_f64());
            let mut states: Vec<BodyState> = Vec::new();
            let mut seen: HashSet<LooseId> = HashSet::new();
            for (li, c) in &loose {
                let Some(id) = li.id else { continue };
                seen.insert(id);
                let (frame, fref) = match c.ship {
                    Some(_) => (BodyFrame::Ship(crate::items::LOCAL), FrameRef::Ship(crate::items::LOCAL)),
                    None => (BodyFrame::Planet(planet.id.0 as u32), FrameRef::Planet(planet.id.0 as u32)),
                };
                let rot = c.body.rot();
                let at = |vel: DVec3| Placement { frame: fref, pos: c.body.pos.to_array(), vel: vel.to_array(), forward: c.body.forward.to_array() };
                if !c.body.asleep {
                    states.push(BodyState { id, frame, pos: c.body.pos, vel: c.body.vel, rot });
                    items.host.track(id, at(c.body.vel));
                    moving.insert(id);
                } else if moving.remove(&id) {
                    items.rest(now, id, at(DVec3::ZERO), [rot.x, rot.y, rot.z, rot.w], t);
                }
            }
            moving.retain(|id| seen.contains(id));
            net.send_bodies(t, states);
        }
    }
    for out in std::mem::take(&mut items.net_out) {
        match out {
            NetOut::Action(a) => {
                net.send_reliable(None, &wire::encode(&ToHost::Action(a)));
            }
            NetOut::Event { to, ev } => {
                net.send_reliable(to, &wire::encode(&ToClient::Event(ev)));
            }
            NetOut::Vitals { to, v } => {
                net.send_reliable(Some(to), &wire::encode(&ToClient::Vitals(v)));
            }
            NetOut::Room { to, r } => {
                net.send_reliable(to, &wire::encode(&ToClient::Room(r)));
            }
            NetOut::Ask(a) => {
                net.send_reliable(None, &wire::encode(&ToHost::Room(a)));
            }
            NetOut::Reactor { to, r } => {
                net.send_reliable(to, &wire::encode(&ToClient::Reactor(r)));
            }
            NetOut::ReactorAsk(a) => {
                net.send_reliable(None, &wire::encode(&ToHost::Reactor(a)));
            }
            NetOut::Fire { to, f } => {
                net.send_reliable(to, &wire::encode(&ToClient::Fire(f)));
            }
            NetOut::SprayAsk(a) => {
                net.send_reliable(None, &wire::encode(&ToHost::Spray(a)));
            }
            NetOut::RepairAsk(a) => {
                net.send_reliable(None, &wire::encode(&ToHost::Repair(a)));
            }
            NetOut::RepairDone { to } => {
                net.send_reliable(Some(to), &wire::encode(&ToClient::RepairDone));
            }
            NetOut::Status { to, s } => {
                net.send_reliable(to, &wire::encode(&ToClient::Status(s)));
            }
            NetOut::Treat(t) => {
                net.send_reliable(None, &wire::encode(&ToHost::Treat(t)));
            }
            NetOut::Rescue(t) => {
                net.send_reliable(None, &wire::encode(&ToHost::Rescue(t)));
            }
            NetOut::RepairRefused { to } => {
                net.send_reliable(Some(to), &wire::encode(&ToClient::RepairRefused));
            }
        }
    }
}

/// Where the item sits in a figure's hand (figure-local, metres; -z is forward).
const HAND: Vec3 = Vec3::new(0.34, 1.0, -0.4);

/// The item a remote player holds, a placeholder box in the hand of its figure.
#[derive(Component)]
struct HeldOnRemote(Option<String>);

#[allow(clippy::too_many_arguments)]
fn remote_held(
    mut commands: Commands,
    items: Res<Items>,
    walkers: Query<(Entity, &crate::net_live::RemoteWalker, Option<&Children>)>,
    mut held: Query<(Entity, &mut HeldOnRemote, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (e, rw, children) in &walkers {
        let want = items.client.held_by(rw.owner).map(|s| s.item.kind.to_string());
        let mine = children.and_then(|c| c.iter().find(|ch| held.contains(*ch)));
        let Some(h) = mine else {
            commands.entity(e).with_children(|c| {
                c.spawn((HeldOnRemote(None), Transform::from_translation(HAND), Visibility::Hidden));
            });
            continue;
        };
        let Ok((he, mut m, mut vis)) = held.get_mut(h) else { continue };
        if m.0 != want {
            m.0 = want.clone();
            if let Some(d) = want.as_deref().and_then(|id| items.defs.get(id)) {
                let [x, y, z] = d.shape.extents.map(|v| v as f32);
                let [r, g, b] = d.shape.colour;
                commands.entity(he).insert((Mesh3d(meshes.add(Cuboid::new(x, y, z))), MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.6, ..default() }))));
            }
        }
        let want_vis = if m.0.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want_vis {
            *vis = want_vis;
        }
    }
}

/// A picture of a loose item whose frame is not there yet stays hidden.
fn hide_unplaced(mut q: Query<(&LooseView, &mut Visibility)>) {
    for (v, mut vis) in &mut q {
        if !v.placed && *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
    }
}
