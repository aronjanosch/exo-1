//! Spike 10 live network: plain UDP (std::net, no dependency), host relay, client authority.
//! Every process simulates its own ship and walker; snapshots (net_core) go to the host, the host
//! relays them to the other clients; everybody interpolates the others from a buffer and drives a
//! kinematic proxy ship (hull colliders) and a walker marker from it.
use crate::env::PlanetRes;
use crate::origin::{BodyInterp, RenderOrigin, WorldPose, WorldPos};
use crate::ship::{add_hull, RemoteShip, Ship};
use crate::walker::Player;
use crate::{controls::Controls, PhysicsTiming};
use avian3d::prelude::*;
use bevy::app::AppExit;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use net_core::buffer::{Buffer, Mode};
use net_core::clock::ClockSync;
use net_core::link::Link;
use net_core::metrics::{mean, percentile};
use net_core::snapshot::{FrameKind, Snapshot};
use net_core::wire::{self, Packet, UDP_IP_OVERHEAD};
use net_core::{to_world, PLANET_CENTRES};
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
    pub planet: u32,
    pub rate: f64,
    /// Extra playout buffer in seconds (on top of the artificial one-way delay).
    pub buffer: f64,
    pub delay_ms: f64,
    pub jitter_ms: f64,
    pub loss: f64,
    pub seconds: f64,
    pub tag: String,
    pub out: std::path::PathBuf,
    /// Test hook (headless only): at this many seconds move the render origin by an offset.
    pub force_shift: Option<(f64, DVec3)>,
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
            planet: num("--planet", 0.0) as u32,
            rate: num("--rate", 30.0).clamp(1.0, 60.0),
            buffer: num("--buffer", 150.0).clamp(0.0, 1000.0) / 1000.0,
            delay_ms: num("--delay", 0.0),
            jitter_ms: num("--jitter", 0.0),
            loss: num("--loss", 0.0),
            seconds: num("--seconds", 0.0),
            tag: get("--tag").unwrap_or("run").to_string(),
            out: get("--net-out").unwrap_or("results/net").into(),
            force_shift: get("--force-shift").map(|v| {
                let c: Vec<f64> = v.split(',').map(|x| x.parse().expect("--force-shift=t,x,y,z")).collect();
                (c[0], DVec3::new(c[1], c[2], c[3]))
            }),
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

#[derive(Default)]
pub struct NetStats {
    pub ticks: u64,
    pub payload_tx: u64,
    pub payload_rx: u64,
    pub wire_tx: u64,
    pub wire_rx: u64,
    pub dgram_tx: u64,
    pub dgram_rx: u64,
    pub invalid: u64,
    pub holds: u64,
    pub displayed: u64,
    pub max_remotes: usize,
    pub pre_ms: Vec<f64>,
    pub post_ms: Vec<f64>,
    pub phys_ms: Vec<f64>,
    pub cpu_ticks0: Option<u64>,
    pub err_near_max: f64,
    pub err_far_max: f64,
    pub jump_max: f64,
    pub shift_frames: u32,
    pub forced_shift: bool,
    pub input_start: Option<u64>,
    pub input_response_ticks: Option<u64>,
    pub lost_host: bool,
    pub joined_owners: Vec<u32>,
}

#[derive(Resource)]
pub struct Net {
    pub cfg: NetConfig,
    sock: UdpSocket,
    start: Instant,
    /// Wall clock at `start` (UNIX seconds), to check the clock sync against the true offset.
    start_epoch: f64,
    clock: ClockSync,
    ready: bool,
    host_addr: Option<SocketAddr>,
    peers: HashMap<u32, Peer>,
    link: Link<Snapshot>,
    hist: HashMap<u32, Buffer>,
    seen: HashMap<u32, f64>,
    pending_holds: HashMap<u32, u64>,
    proxies: HashMap<u32, Entity>,
    walkers: HashMap<u32, Entity>,
    seq: u32,
    send_accum: f64,
    ping_accum: f64,
    hello_accum: f64,
    last_host_packet: f64,
    last_origin: DVec3,
    pub st: NetStats,
}

/// Marker of a remote player's walker (capsule in the view).
#[derive(Component)]
pub struct RemoteWalker {
    pub owner: u32,
}

impl Net {
    pub fn new(cfg: NetConfig, origin: DVec3) -> Net {
        let sock = if cfg.host {
            UdpSocket::bind(format!("{}:{}", cfg.bind, cfg.port)).unwrap_or_else(|e| panic!("bind {}:{}: {e}", cfg.bind, cfg.port))
        } else {
            UdpSocket::bind("0.0.0.0:0").expect("bind client socket")
        };
        sock.set_nonblocking(true).expect("nonblocking");
        let link = Link::new(cfg.delay_ms, cfg.jitter_ms, cfg.loss, 4000 + cfg.slot as u64);
        let host = cfg.host;
        let host_addr = cfg.connect;
        println!("NET START slot={} host={} planet={} port={}", cfg.slot, host, cfg.planet, cfg.port);
        Net {
            cfg,
            sock,
            start: Instant::now(),
            start_epoch: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0),
            clock: if host { ClockSync::host() } else { ClockSync::default() },
            ready: host,
            host_addr,
            peers: HashMap::new(),
            link,
            hist: HashMap::new(),
            seen: HashMap::new(),
            pending_holds: HashMap::new(),
            proxies: HashMap::new(),
            walkers: HashMap::new(),
            seq: 0,
            send_accum: 0.0,
            ping_accum: 0.0,
            hello_accum: 1.0,
            last_host_packet: 0.0,
            last_origin: origin,
            st: NetStats::default(),
        }
    }

    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn send(&mut self, bytes: &[u8], to: SocketAddr, snapshot: bool) {
        if self.sock.send_to(bytes, to).is_ok() {
            self.st.dgram_tx += 1;
            self.st.wire_tx += (bytes.len() + UDP_IP_OVERHEAD) as u64;
            if snapshot {
                self.st.payload_tx += net_core::snapshot::SIZE as u64;
            }
        }
    }

    fn handle(&mut self, from: SocketAddr, data: &[u8], now: f64) {
        self.st.dgram_rx += 1;
        self.st.wire_rx += (data.len() + UDP_IP_OVERHEAD) as u64;
        let Some(packet) = Packet::decode(data) else {
            self.st.invalid += 1;
            return;
        };
        if self.cfg.host {
            self.handle_host(from, packet, data, now);
        } else if Some(from) == self.host_addr {
            self.last_host_packet = now;
            match packet {
                Packet::Accepted { .. } => self.ready = true,
                Packet::Pong { sent, server_time } => self.clock.on_pong(sent, server_time, now),
                Packet::Snapshot(s) => self.ingest(s),
                _ => {}
            }
        } else {
            self.st.invalid += 1;
        }
    }

    fn handle_host(&mut self, from: SocketAddr, packet: Packet, raw: &[u8], now: f64) {
        match packet {
            Packet::Hello { slot, planet } => {
                let slot = slot as u32;
                if !(2..=8).contains(&slot) || planet as usize >= PLANET_CENTRES.len() {
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
                    self.send(&ack, from, false);
                }
            }
            Packet::Ping { sent } => {
                if let Some(p) = self.peers.values_mut().find(|p| p.addr == from) {
                    p.last = now;
                }
                let pong = Packet::Pong { sent, server_time: self.clock.server_now(now) }.encode();
                self.send(&pong, from, false);
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
                    self.send(&out, a, true);
                }
                self.ingest(s);
            }
            Packet::Bye { slot } => {
                if self.peers.get(&(slot as u32)).is_some_and(|p| p.addr == from) {
                    self.peers.remove(&(slot as u32));
                }
            }
            _ => {}
        }
    }

    /// A received snapshot goes through the fault-injection link (receiver side, after the real
    /// transport) and then into the owner's buffer.
    fn ingest(&mut self, s: Snapshot) {
        if s.owner == self.cfg.slot {
            return;
        }
        self.st.payload_rx += net_core::snapshot::SIZE as u64;
        let t = self.clock.server_now(self.now());
        self.link.enqueue(t, s);
    }
}

fn read_cpu_ticks() -> u64 {
    // utime + stime of this process in clock ticks (USER_HZ is 100 on Linux). 0 elsewhere.
    std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|s| {
            let rest = s.rsplit_once(')')?.1.to_string();
            let f: Vec<&str> = rest.split_whitespace().collect();
            Some(f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?)
        })
        .unwrap_or(0)
}

fn spawn_proxy(commands: &mut Commands, owner: u32, pos: DVec3, rot: DQuat) -> Entity {
    let e = commands
        .spawn((
            RemoteShip { owner },
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
    add_hull(commands, e);
    e
}

#[allow(clippy::too_many_arguments)]
pub fn net_pre(
    mut commands: Commands,
    mut net: ResMut<Net>,
    mut origin: ResMut<RenderOrigin>,
    mut players: Query<&mut Player>,
    own: Query<(&Position, &Rotation), (With<Ship>, Without<RemoteShip>)>,
    mut proxies: Query<(&mut Position, &mut Rotation, &mut LinearVelocity, &mut AngularVelocity), (With<RemoteShip>, Without<Ship>)>,
    mut walkers: Query<&mut WorldPose, With<RemoteWalker>>,
    terrain: Query<(&WorldPos, &mut Transform)>,
) {
    let begin = Instant::now();
    let net = &mut *net;
    if net.st.cpu_ticks0.is_none() {
        net.st.cpu_ticks0 = Some(read_cpu_ticks());
    }
    let now = net.now();
    // Headless has no camera: the own ship is the view, so the render origin follows it.
    if net.cfg.headless {
        if let Ok((p, _)) = own.single() {
            origin.view = p.0;
        }
    }
    if let Some((t, d)) = net.cfg.force_shift {
        if now >= t && !net.st.forced_shift {
            net.st.forced_shift = true;
            origin.origin += d;
            origin.shifts += 1;
            let o = origin.origin;
            for (wp, mut tr) in terrain {
                tr.translation = (wp.0 - o).as_vec3();
            }
        }
    }
    // Receive.
    let mut buf = [0u8; 2048];
    for _ in 0..512 {
        match net.sock.recv_from(&mut buf) {
            Ok((n, from)) => net.handle(from, &buf[..n], now),
            Err(_) => break,
        }
    }
    let server_now = net.clock.server_now(now);
    // Fault-injected link into the buffers.
    for s in net.link.ready(server_now) {
        net.seen.insert(s.owner, server_now);
        if !net.hist.contains_key(&s.owner) {
            net.hist.insert(s.owner, Buffer::new());
            if !net.st.joined_owners.contains(&s.owner) {
                net.st.joined_owners.push(s.owner);
            }
        }
        net.hist.get_mut(&s.owner).unwrap().push(s);
    }
    // Owners that went silent leave. A walker inside their ship is put back into the planet frame.
    let gone: Vec<u32> = net.seen.iter().filter(|(_, t)| server_now - **t > EXPIRY).map(|(o, _)| *o).collect();
    for o in gone {
        net.seen.remove(&o);
        net.pending_holds.remove(&o);
        net.hist.remove(&o);
        if let Some(e) = net.proxies.remove(&o) {
            if let Ok(mut pl) = players.single_mut() {
                if pl.ship == Some(e) {
                    if let Ok((p, r, v, _)) = proxies.get(e) {
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
    // Playout: everybody is shown at the same shared timestamp, delay and buffer behind now.
    let target = server_now - net.link.delay - net.cfg.buffer;
    let samples: HashMap<u32, net_core::buffer::Sample> = net.hist.iter().filter_map(|(o, b)| b.sample(target).map(|s| (*o, s))).collect();
    net.st.max_remotes = net.st.max_remotes.max(samples.len());
    for (&owner, sample) in &samples {
        let s = &sample.s;
        // A hold counts as an underrun only once the stream resumes; a stream that ended
        // (owner left, run over) leaves its held ticks out.
        net.st.displayed += 1;
        if sample.mode == Mode::Hold {
            *net.pending_holds.entry(owner).or_default() += 1;
        } else if let Some(n) = net.pending_holds.remove(&owner) {
            net.st.holds += n;
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
                if let Ok((mut p, mut r, mut v, mut w)) = proxies.get_mut(e) {
                    p.0 = pos;
                    r.0 = s.q;
                    v.0 = s.v;
                    w.0 = if spin.is_finite() { spin } else { DVec3::ZERO };
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
    net.st.ticks += 1;
    net.st.pre_ms.push(begin.elapsed().as_secs_f64() * 1000.0);
}

#[allow(clippy::too_many_arguments)]
pub fn net_post(
    mut net: ResMut<Net>,
    planet: Res<PlanetRes>,
    controls: Res<Controls>,
    mut phys: ResMut<PhysicsTiming>,
    ships: Query<(&Position, &Rotation, &LinearVelocity, &Ship), Without<RemoteShip>>,
    players: Query<&Player>,
) {
    let begin = Instant::now();
    let net = &mut *net;
    let now = net.now();
    let tick = net.st.ticks;
    net.st.phys_ms.push(std::mem::take(&mut phys.frame_ms));
    let (Ok((p, r, v, ship)), Ok(pl)) = (ships.single(), players.single()) else { return };
    // Own input response, in physics ticks: first piloted tick with the climb key until motion.
    if net.st.input_start.is_none() && ship.piloted && controls.pressed(KeyCode::Space) {
        net.st.input_start = Some(tick);
    }
    if let (Some(s0), None) = (net.st.input_start, net.st.input_response_ticks) {
        if v.0.length() > 0.01 {
            net.st.input_response_ticks = Some(tick - s0 + 1);
        }
    }
    if !net.cfg.host {
        if net.cfg.connect.is_some() && net.last_host_packet > 0.0 && now - net.last_host_packet > 3.0 && net.ready {
            net.st.lost_host = true;
        }
        if !net.ready {
            net.hello_accum += TICK_S;
            if net.hello_accum >= 0.25 {
                net.hello_accum = 0.0;
                let hello = Packet::Hello { slot: net.cfg.slot as u8, planet: net.cfg.planet as u8 }.encode();
                net.send(&hello, net.host_addr.unwrap(), false);
            }
        } else {
            net.ping_accum += TICK_S;
            if net.ping_accum >= 0.5 || !net.clock.ready {
                net.ping_accum = 0.0;
                let ping = Packet::Ping { sent: now }.encode();
                net.send(&ping, net.host_addr.unwrap(), false);
            }
        }
    }
    net.send_accum += TICK_S;
    if net.send_accum >= 1.0 / net.cfg.rate && net.ready && net.clock.ready {
        net.send_accum %= 1.0 / net.cfg.rate;
        net.seq += 1;
        let s = crate::net::build_snapshot(net.cfg.slot, net.cfg.planet, net.clock.server_now(now), net.seq, &planet, (p, r, v), pl);
        let bytes = wire::encode_snapshot(&s);
        if net.cfg.host {
            let addrs: Vec<SocketAddr> = net.peers.values().map(|p| p.addr).collect();
            for a in addrs {
                net.send(&bytes, a, true);
            }
        } else {
            let a = net.host_addr.unwrap();
            net.send(&bytes, a, true);
        }
    }
    net.st.post_ms.push(begin.elapsed().as_secs_f64() * 1000.0);
}

/// How far the f32 transform the GPU sees is from the exact f64 pose, and the jump of that
/// reconstruction when the render origin moves.
pub fn net_measure(mut net: ResMut<Net>, origin: Res<RenderOrigin>, fixed: Res<Time<Fixed>>, q: Query<(&BodyInterp, &Transform), With<RemoteShip>>) {
    let f = fixed.overstep_fraction_f64();
    let moved = origin.origin != net.last_origin;
    let old = net.last_origin;
    net.last_origin = origin.origin;
    if moved {
        net.st.shift_frames += 1;
    }
    for (i, t) in &q {
        let (pos, _) = i.at(f);
        let recon = origin.origin + t.translation.as_dvec3();
        let err = recon.distance(pos) * 1000.0;
        let dist = (pos - origin.origin).length();
        if dist > 100_000.0 {
            net.st.err_far_max = net.st.err_far_max.max(err);
        } else {
            net.st.err_near_max = net.st.err_near_max.max(err);
        }
        if moved {
            // The same exact pose drawn relative to the old and to the new origin.
            let before = old + (pos - old).as_vec3().as_dvec3();
            let after = origin.origin + (pos - origin.origin).as_vec3().as_dvec3();
            net.st.jump_max = net.st.jump_max.max(before.distance(after) * 1000.0);
        }
    }
}

fn stats_json(v: &[f64]) -> (f64, f64, f64) {
    let mut s = v.to_vec();
    (mean(v), percentile(&mut s, 0.95), percentile(&mut s, 1.0))
}

pub fn net_finish(mut net: ResMut<Net>, origin: Res<RenderOrigin>, mut exit: MessageWriter<AppExit>) {
    let now = net.now();
    let net = &mut *net;
    let over = net.cfg.seconds > 0.0 && now >= net.cfg.seconds;
    if !over && !net.st.lost_host {
        return;
    }
    if !net.cfg.host {
        if let Some(a) = net.host_addr {
            let bye = Packet::Bye { slot: net.cfg.slot as u8 }.encode();
            net.send(&bye, a, false);
        }
    }
    let st = &net.st;
    let ticks = st.ticks.max(1) as f64;
    let cpu_ticks = read_cpu_ticks().saturating_sub(st.cpu_ticks0.unwrap_or(0)) as f64;
    let (pre_mean, pre_p95, pre_max) = stats_json(&st.pre_ms);
    let (post_mean, post_p95, _) = stats_json(&st.post_ms);
    let (phys_mean, phys_p95, phys_max) = stats_json(&st.phys_ms);
    let secs = (now - 0.0).max(1e-6);
    let json = format!(
        "{{\n  \"tag\": \"{}\", \"slot\": {}, \"host\": {}, \"planet\": {}, \"seconds\": {:.2}, \"ticks\": {},\n  \"rate\": {}, \"buffer_ms\": {:.0}, \"delay_ms\": {:.0}, \"jitter_ms\": {:.0}, \"loss_percent\": {:.1},\n  \"remote_count\": {}, \"max_remotes\": {}, \"joined_owners\": {:?},\n  \"payload_tx_bytes\": {}, \"payload_rx_bytes\": {}, \"wire_tx_bytes\": {}, \"wire_rx_bytes\": {}, \"dgram_tx\": {}, \"dgram_rx\": {},\n  \"payload_tx_kB_s\": {:.3}, \"payload_rx_kB_s\": {:.3}, \"wire_tx_kB_s\": {:.3}, \"wire_rx_kB_s\": {:.3},\n  \"injected_dropped\": {}, \"invalid\": {}, \"hold_percent\": {:.4}, \"displayed\": {},\n  \"net_pre_ms_mean\": {:.4}, \"net_pre_ms_p95\": {:.4}, \"net_pre_ms_max\": {:.4}, \"net_post_ms_mean\": {:.4}, \"net_post_ms_p95\": {:.4},\n  \"physics_step_ms_mean\": {:.4}, \"physics_step_ms_p95\": {:.4}, \"physics_step_ms_max\": {:.4},\n  \"process_cpu_ms_per_tick\": {:.4}, \"process_cpu_percent_of_one_core\": {:.2},\n  \"clock_rtt_ms\": {:.3}, \"clock_offset_s\": {:.6}, \"start_epoch_s\": {:.6}, \"input_response_ticks\": {}, \"shifts\": {}, \"shift_frames_seen\": {}, \"forced_shift\": {},\n  \"render_error_near_max_mm\": {:.4}, \"render_error_far_max_mm\": {:.4}, \"render_shift_jump_max_mm\": {:.4}, \"lost_host\": {}\n}}\n",
        net.cfg.tag, net.cfg.slot, net.cfg.host, net.cfg.planet, now, st.ticks, net.cfg.rate, net.cfg.buffer * 1000.0, net.link.delay * 1000.0, net.cfg.jitter_ms, net.link.loss * 100.0,
        net.hist.len(), st.max_remotes, st.joined_owners, st.payload_tx, st.payload_rx, st.wire_tx, st.wire_rx, st.dgram_tx, st.dgram_rx,
        st.payload_tx as f64 / secs / 1000.0, st.payload_rx as f64 / secs / 1000.0, st.wire_tx as f64 / secs / 1000.0, st.wire_rx as f64 / secs / 1000.0,
        net.link.dropped, st.invalid, 100.0 * st.holds as f64 / st.displayed.max(1) as f64, st.displayed,
        pre_mean, pre_p95, pre_max, post_mean, post_p95, phys_mean, phys_p95, phys_max,
        cpu_ticks * 10.0 / ticks, cpu_ticks * 10.0 / 1000.0 / secs * 100.0,
        net.clock.best_rtt.unwrap_or(0.0) * 1000.0, net.clock.offset, net.start_epoch, st.input_response_ticks.map(|t| t.to_string()).unwrap_or("null".into()), origin.shifts, st.shift_frames, st.forced_shift,
        st.err_near_max, st.err_far_max, st.jump_max, st.lost_host,
    );
    let _ = std::fs::create_dir_all(&net.cfg.out);
    let path = net.cfg.out.join(format!("{}-slot{}.json", net.cfg.tag, net.cfg.slot));
    std::fs::write(&path, &json).expect("write net result");
    println!("NET RESULT {}", path.display());
    let ok = !st.lost_host && st.invalid == 0 && (net.cfg.host || !st.joined_owners.is_empty());
    exit.write(if ok { AppExit::Success } else { AppExit::from_code(1) });
}
