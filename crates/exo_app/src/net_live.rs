//! Live network: plain UDP (std::net, no dependency), host relay, client authority (spike 10).
//! Every process simulates its own ship and walker; snapshots (net_core) go to the host, the host
//! relays them to the other clients; everybody interpolates the others from a buffer and drives a
//! kinematic proxy ship (hull colliders) and a walker marker from it.
use crate::env::PlanetRes;
use crate::origin::{BodyInterp, RenderOrigin, WorldPose};
use crate::ship::{add_hull, RemoteShip, Ship};
use crate::walker::Player;
use avian3d::prelude::*;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use net_core::buffer::{Buffer, Mode};
use net_core::clock::ClockSync;
use net_core::snapshot::{FrameKind, Snapshot};
use net_core::to_world;
use net_core::wire::{self, Packet, UDP_IP_OVERHEAD};
use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::Instant;

const TICK_S: f64 = 1.0 / 60.0;
/// A remote owner without snapshots for this long is removed (spike 4: 2 s).
const EXPIRY: f64 = 2.0;

#[derive(Clone, Debug)]
pub struct NetConfig {
    pub host: bool,
    pub connect: Option<SocketAddr>,
    pub bind: String,
    pub port: u16,
    pub slot: u32,
    /// Snapshots per second.
    pub rate: f64,
    /// Playout buffer in seconds.
    pub buffer: f64,
    /// Display-only extrapolation during an underrun, ms (default 100; 0 = hold, spike 4 rule).
    pub extrapolate_ms: f64,
    /// Fly the scripted network bot (scenario `net`).
    pub bot: bool,
    pub headless: bool,
}

impl NetConfig {
    /// From `--net-host` or `--net-connect=ip:port` plus the options below; None without either.
    pub fn parse(args: &[(String, String)]) -> Option<NetConfig> {
        let get = |k: &str| args.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
        let host = get("--net-host").is_some();
        let connect = get("--net-connect").map(|v| v.parse::<SocketAddr>().expect("--net-connect=ip:port"));
        if !host && connect.is_none() {
            return None;
        }
        let num = |k: &str, d: f64| get(k).map(|v| v.parse::<f64>().unwrap_or_else(|_| panic!("{k}"))).unwrap_or(d);
        let slot = num("--slot", if host { 1.0 } else { 2.0 }) as u32;
        assert!((1..=8).contains(&slot), "--slot must be 1..8");
        Some(NetConfig {
            host,
            connect,
            bind: get("--bind").unwrap_or("0.0.0.0").to_string(),
            port: num("--port", 17441.0) as u16,
            slot,
            rate: num("--rate", 30.0).clamp(1.0, 60.0),
            buffer: num("--buffer", 150.0).clamp(0.0, 1000.0) / 1000.0,
            extrapolate_ms: num("--extrapolate", 100.0),
            bot: get("--bot").is_some(),
            headless: false,
        })
    }
}

struct Peer {
    addr: SocketAddr,
    /// Local time of the last datagram from it.
    last: f64,
}

/// Counters for the HUD line.
#[derive(Default)]
pub struct NetStats {
    pub wire_tx: u64,
    pub wire_rx: u64,
    pub invalid: u64,
    pub holds: u64,
    pub displayed: u64,
}

#[derive(Resource)]
pub struct Net {
    pub cfg: NetConfig,
    sock: UdpSocket,
    /// Datagrams from the receive thread: source, bytes, arrival time (local seconds).
    rx: std::sync::Mutex<std::sync::mpsc::Receiver<(SocketAddr, Vec<u8>, f64)>>,
    start: Instant,
    clock: ClockSync,
    ready: bool,
    host_addr: Option<SocketAddr>,
    peers: HashMap<u32, Peer>,
    hist: HashMap<u32, Buffer>,
    /// Local arrival time of the last snapshot per owner.
    seen: HashMap<u32, f64>,
    proxies: HashMap<u32, Entity>,
    walkers: HashMap<u32, Entity>,
    seq: u32,
    send_accum: f64,
    ping_accum: f64,
    hello_accum: f64,
    pub st: NetStats,
    /// Planet centres of the system: a snapshot is relative to its sender's planet, the buffers
    /// hold everything relative to planet 0 (at the origin) so a sender changing planet in a
    /// warp is interpolated straight through (spike 11).
    pub centres: Vec<bevy::math::DVec3>,
}

/// Marker of a remote player's walker (capsule in the view).
#[derive(Component)]
pub struct RemoteWalker {
    pub owner: u32,
}

impl Net {
    pub fn new(cfg: NetConfig) -> Net {
        let sock = if cfg.host {
            UdpSocket::bind(format!("{}:{}", cfg.bind, cfg.port)).unwrap_or_else(|e| panic!("bind {}:{}: {e}", cfg.bind, cfg.port))
        } else {
            UdpSocket::bind("0.0.0.0:0").expect("bind client socket")
        };
        let start = Instant::now();
        // A thread owns the receive side: it stamps the arrival time at once and, on the host,
        // answers pings itself, so clock sync does not wait for the next 60 Hz tick.
        let (tx, rx) = std::sync::mpsc::channel();
        let recv_sock = sock.try_clone().expect("clone socket");
        let is_host = cfg.host;
        std::thread::Builder::new()
            .name("net-recv".into())
            .spawn(move || {
                let mut buf = [0u8; 2048];
                while let Ok((n, from)) = recv_sock.recv_from(&mut buf) {
                    let arrival = start.elapsed().as_secs_f64();
                    if is_host {
                        if let Some(Packet::Ping { sent }) = Packet::decode(&buf[..n]) {
                            let pong = Packet::Pong { sent, server_time: start.elapsed().as_secs_f64() }.encode();
                            let _ = recv_sock.send_to(&pong, from);
                        }
                    }
                    if tx.send((from, buf[..n].to_vec(), arrival)).is_err() {
                        break;
                    }
                }
            })
            .expect("spawn receive thread");
        let host = cfg.host;
        let host_addr = cfg.connect;
        println!("NET START slot={} host={} port={}", cfg.slot, host, cfg.port);
        Net {
            cfg,
            sock,
            rx: std::sync::Mutex::new(rx),
            start,
            clock: if host { ClockSync::host() } else { ClockSync::default() },
            ready: host,
            host_addr,
            peers: HashMap::new(),
            hist: HashMap::new(),
            seen: HashMap::new(),
            proxies: HashMap::new(),
            walkers: HashMap::new(),
            seq: 0,
            send_accum: 0.0,
            ping_accum: 0.0,
            hello_accum: 1.0,
            st: NetStats::default(),
            centres: net_core::PLANET_CENTRES.to_vec(),
        }
    }

    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn send(&mut self, bytes: &[u8], to: SocketAddr) {
        if self.sock.send_to(bytes, to).is_ok() {
            self.st.wire_tx += (bytes.len() + UDP_IP_OVERHEAD) as u64;
        }
    }

    fn handle(&mut self, from: SocketAddr, data: &[u8], now: f64) {
        self.st.wire_rx += (data.len() + UDP_IP_OVERHEAD) as u64;
        let Some(packet) = Packet::decode(data) else {
            self.st.invalid += 1;
            return;
        };
        if self.cfg.host {
            self.handle_host(from, packet, data, now);
        } else if Some(from) == self.host_addr {
            match packet {
                Packet::Accepted { .. } => self.ready = true,
                Packet::Pong { sent, server_time } => self.clock.on_pong(sent, server_time, now),
                Packet::Snapshot(s) => self.ingest(s, now),
                _ => {}
            }
        } else {
            self.st.invalid += 1;
        }
    }

    fn handle_host(&mut self, from: SocketAddr, packet: Packet, raw: &[u8], now: f64) {
        match packet {
            Packet::Hello { slot, .. } => {
                let slot = slot as u32;
                if !(2..=8).contains(&slot) {
                    return;
                }
                let free = match self.peers.get(&slot) {
                    None => true,
                    Some(p) => p.addr == from || now - p.last > 1.0,
                };
                if free {
                    if self.peers.get(&slot).is_none_or(|p| p.addr != from) {
                        println!("NET JOIN slot={slot}");
                    }
                    self.peers.insert(slot, Peer { addr: from, last: now });
                    let ack = Packet::Accepted { slot: slot as u8 }.encode();
                    self.send(&ack, from);
                }
            }
            Packet::Ping { .. } => {
                // The receive thread has already answered; this only keeps the peer alive.
                if let Some(p) = self.peers.values_mut().find(|p| p.addr == from) {
                    p.last = now;
                }
            }
            Packet::Snapshot(s) => {
                let Some(slot) = self.peers.iter().find(|(_, p)| p.addr == from).map(|(k, _)| *k) else {
                    self.st.invalid += 1;
                    return;
                };
                if s.owner != slot {
                    self.st.invalid += 1;
                    return;
                }
                self.peers.get_mut(&slot).unwrap().last = now;
                // Relay the bytes unchanged to every other client.
                let others: Vec<SocketAddr> = self.peers.iter().filter(|(k, _)| **k != slot).map(|(_, p)| p.addr).collect();
                let out = wire::wrap_snapshot_bytes(&raw[2..]);
                for a in others {
                    self.send(&out, a);
                }
                self.ingest(s, now);
            }
            Packet::Bye { slot } => {
                if self.peers.get(&(slot as u32)).is_some_and(|p| p.addr == from) {
                    self.peers.remove(&(slot as u32));
                }
            }
            _ => {}
        }
    }

    /// A received snapshot goes into its owner's buffer. Snapshot times are shared (host) time.
    fn ingest(&mut self, s: Snapshot, arrival: f64) {
        if s.owner == self.cfg.slot {
            return;
        }
        self.seen.insert(s.owner, arrival);
        let mut s = s;
        s.to_frame_of(&self.centres, 0);
        self.hist.entry(s.owner).or_default().push(s);
    }

    /// One line for the HUD.
    pub fn hud_line(&self) -> String {
        let secs = self.now().max(1e-3);
        let state = if self.cfg.host {
            format!("host, {} clients", self.peers.len())
        } else if !self.ready {
            "joining...".to_string()
        } else {
            format!("client, rtt {:.0} ms", self.clock.best_rtt.unwrap_or(0.0) * 1000.0)
        };
        format!(
            "net slot {} {} | remote ships {} | holds {:.2} % | {:.1} kB/s in, {:.1} kB/s out | {} Hz + {:.0} ms buffer",
            self.cfg.slot,
            state,
            self.proxies.len(),
            100.0 * (self.st.holds as f64) / (self.st.displayed.max(1) as f64),
            self.st.wire_rx as f64 / secs / 1000.0,
            self.st.wire_tx as f64 / secs / 1000.0,
            self.cfg.rate,
            self.cfg.buffer * 1000.0,
        )
    }
}

pub fn spawn_proxy(commands: &mut Commands, owner: u32, pos: DVec3, rot: DQuat) -> Entity {
    let e = commands
        .spawn((
            RemoteShip { owner, lag: 1.0 },
            RigidBody::Kinematic,
            Position(pos),
            Rotation(rot),
            LinearVelocity::default(),
            AngularVelocity::default(),
            SleepingDisabled,
            Transform::default(),
            Visibility::default(),
            BodyInterp { prev: (pos, rot), curr: (pos, rot) },
        ))
        .id();
    add_hull(commands, e, crate::Layer::Remote);
    e
}

/// Receive, drop silent owners, and place every remote ship and walker at the shared playout time.
#[allow(clippy::too_many_arguments)]
pub fn net_pre(
    mut commands: Commands,
    mut net: ResMut<Net>,
    mut origin: ResMut<RenderOrigin>,
    mut players: Query<&mut Player>,
    own: Query<(&Position, &Rotation), (With<Ship>, Without<RemoteShip>)>,
    mut proxies: Query<(&mut Position, &mut Rotation, &mut LinearVelocity, &mut AngularVelocity, &mut RemoteShip), Without<Ship>>,
    mut walkers: Query<&mut WorldPose, With<RemoteWalker>>,
) {
    let net = &mut *net;
    let now = net.now();
    // Headless has no camera: the own ship is the view, so the render origin follows it.
    if net.cfg.headless {
        if let Ok((p, _)) = own.single() {
            origin.view = p.0;
        }
    }
    let incoming: Vec<_> = {
        let rx = net.rx.lock().unwrap();
        std::iter::from_fn(|| rx.try_recv().ok()).take(512).collect()
    };
    for (from, data, arrival) in incoming {
        net.handle(from, &data, arrival);
    }
    let server_now = net.clock.server_now(now);
    // Owners that went silent leave. A walker inside their ship is put back into the planet frame.
    let gone: Vec<u32> = net.seen.iter().filter(|(_, t)| now - **t > EXPIRY).map(|(o, _)| *o).collect();
    for o in gone {
        net.seen.remove(&o);
        net.hist.remove(&o);
        if let Some(e) = net.proxies.remove(&o) {
            if let Ok(mut pl) = players.single_mut() {
                if pl.ship == Some(e) {
                    if let Ok((p, r, v, ..)) = proxies.get(e) {
                        let f = walker_core::Frame { origin: p.0, rot: r.0 };
                        pl.w.change_frame(&f, &walker_core::Frame::IDENTITY, v.0);
                        pl.ship = None;
                    }
                }
            }
            commands.entity(e).despawn();
        }
        if let Some(e) = net.walkers.remove(&o) {
            commands.entity(e).despawn();
        }
        println!("NET LEAVE owner={o}");
    }
    // Playout: everybody is shown at the same shared timestamp, one buffer behind now.
    let target = server_now - net.cfg.buffer;
    let samples: HashMap<u32, net_core::buffer::Sample> =
        net.hist.iter().filter_map(|(o, b)| b.sample_extrapolated(target, net.cfg.extrapolate_ms / 1000.0).map(|s| (*o, s))).collect();
    for (&owner, sample) in &samples {
        let s = &sample.s;
        net.st.displayed += 1;
        if sample.mode == Mode::Hold {
            net.st.holds += 1;
        }
        let pos = to_world(s.planet, s.p);
        let ahead = net.hist[&owner].sample(target + TICK_S).map(|x| x.s.q).unwrap_or(s.q);
        let spin = {
            let d = ahead * s.q.inverse();
            let (axis, angle) = d.to_axis_angle();
            let angle = if angle > std::f64::consts::PI { angle - std::f64::consts::TAU } else { angle };
            axis * angle / TICK_S
        };
        match net.proxies.get(&owner).copied() {
            Some(e) => {
                if let Ok((mut p, mut r, mut v, mut w, mut rs)) = proxies.get_mut(e) {
                    p.0 = pos;
                    r.0 = s.q;
                    v.0 = s.v;
                    w.0 = if spin.is_finite() { spin } else { DVec3::ZERO };
                    rs.lag = s.lag;
                }
            }
            None => {
                let e = spawn_proxy(&mut commands, owner, pos, s.q);
                net.proxies.insert(owner, e);
            }
        }
    }
    // Walkers: pose in the frame they live in, composed with the ship sampled at the same time.
    for (&owner, sample) in &samples {
        let s = &sample.s;
        let (pos, rot) = match s.frame {
            FrameKind::Planet => (to_world(s.planet, s.wp), s.wq),
            FrameKind::Ship => {
                let parent = if s.frame_id == net.cfg.slot {
                    own.single().ok().map(|(p, r)| (p.0, r.0))
                } else {
                    samples.get(&s.frame_id).map(|x| (to_world(x.s.planet, x.s.p), x.s.q))
                };
                match parent {
                    Some((pp, pr)) => (pp + pr * s.wp, pr * s.wq),
                    None => continue,
                }
            }
        };
        match net.walkers.get(&owner).copied() {
            Some(e) => {
                if let Ok(mut wp) = walkers.get_mut(e) {
                    wp.pos = pos;
                    wp.rot = rot;
                }
            }
            None => {
                let e = commands.spawn((RemoteWalker { owner }, WorldPose { pos, rot }, Transform::default(), Visibility::default())).id();
                net.walkers.insert(owner, e);
            }
        }
    }
}

/// Join and clock sync (clients), then send the own snapshot at the configured rate.
pub fn net_post(
    mut net: ResMut<Net>,
    planet: Res<PlanetRes>,
    ships: Query<(&Position, &Rotation, &LinearVelocity, &Ship), Without<RemoteShip>>,
    remote_ships: Query<&RemoteShip>,
    players: Query<&Player>,
) {
    let net = &mut *net;
    let now = net.now();
    let (Ok((p, r, v, ship)), Ok(pl)) = (ships.single(), players.single()) else { return };
    if !net.cfg.host {
        let host = net.host_addr.unwrap();
        if !net.ready {
            net.hello_accum += TICK_S;
            if net.hello_accum >= 0.25 {
                net.hello_accum = 0.0;
                let hello = Packet::Hello { slot: net.cfg.slot as u8, planet: planet.id as u8 }.encode();
                net.send(&hello, host);
            }
        } else {
            net.ping_accum += TICK_S;
            if net.ping_accum >= 0.5 || !net.clock.ready {
                net.ping_accum = 0.0;
                let ping = Packet::Ping { sent: now }.encode();
                net.send(&ping, host);
            }
        }
    }
    net.send_accum += TICK_S;
    if net.send_accum >= 1.0 / net.cfg.rate && net.ready && net.clock.ready {
        net.send_accum %= 1.0 / net.cfg.rate;
        net.seq += 1;
        // The cabin the walker is in: own ship, or the owner of the remote ship it boarded.
        let frame_owner = pl.ship.and_then(|e| remote_ships.get(e).ok()).map(|r| r.owner).unwrap_or(net.cfg.slot);
        let mut s = crate::net::build_snapshot(net.cfg.slot, frame_owner, planet.id as u32, net.clock.server_now(now), net.seq, &planet, (p, r, v), pl);
        s.lag = ship.lag.level;
        let bytes = wire::encode_snapshot(&s);
        let to: Vec<SocketAddr> = if net.cfg.host { net.peers.values().map(|p| p.addr).collect() } else { vec![net.host_addr.unwrap()] };
        for a in to {
            net.send(&bytes, a);
        }
    }
}
