//! C2: the item path over the real wire format, without sockets: a host and clients joined by
//! `net_core`'s reliable channel (item requests and events) and body datagrams (poses of a moving
//! loose item), through `link` at the internet's numbers: 150 ms each way, jitter, 2 % loss.
//! Time is virtual, 60 ticks a second, so this runs in milliseconds.
use std::collections::BTreeMap;

use glam::{DQuat, DVec3};
use item_core::wire::{self, ToClient, ToHost};
use item_core::*;
use net_core::link::Link;
use net_core::loose::{BodyDatagram, BodyFrame, BodyState, Pose, Track};
use net_core::reliable::{Datagram, Reliable};

const WRENCH: &str = r#"{ "id": "wrench", "name": "Wrench", "take": "collect", "hands": 1, "stack": 1, "tags": ["tool"], "shape": { "extents": [0.06, 0.04, 0.3], "mass": 0.8, "colour": [0.6, 0.6, 0.65] } }"#;
const BANDAGE: &str = r#"{ "id": "bandage", "name": "Bandage", "take": "collect", "hands": 1, "stack": 4, "tags": ["medical"], "shape": { "extents": [0.1, 0.05, 0.1], "mass": 0.1, "colour": [0.9, 0.9, 0.85] } }"#;
const DT: f64 = 1.0 / 60.0;
/// What the game uses: 150 ms one way, a little jitter, 2 % loss.
const DELAY: f64 = 150.0;
const JITTER: f64 = 20.0;
const LOSS: f64 = 2.0;

fn defs() -> ItemDefs {
    ItemDefs::from_files([("wrench.json", WRENCH), ("bandage.json", BANDAGE)]).unwrap()
}

fn at(x: f64) -> Placement {
    Placement { frame: FrameRef::Planet(0), pos: [x, 0.0, 0.0], vel: [0.0; 3], forward: [0.0, 0.0, -1.0] }
}

/// One client's end: its item state, its side of the channel and its two links.
struct End {
    client: Client,
    chan: Reliable,
    up: Link<Vec<u8>>,
    down: Link<Vec<u8>>,
    bodies: Link<Vec<u8>>,
    tracks: BTreeMap<LooseId, Track>,
    /// The actions it sent, in order.
    sent: Vec<Action>,
}

/// Host, its channel per client, and the clients.
struct Net {
    now: f64,
    host: Host,
    host_chans: BTreeMap<PlayerId, Reliable>,
    ends: BTreeMap<PlayerId, End>,
    seed: u64,
    /// Body datagrams the host sent (counted per tick it sent any).
    streams: u32,
    stream_seq: u32,
}

impl Net {
    fn new(seed: u64) -> Net {
        Net { now: 0.0, host: Host::new(defs()), host_chans: BTreeMap::new(), ends: BTreeMap::new(), seed, streams: 0, stream_seq: 0 }
    }

    /// A client connects now; the host sends it the state as it is.
    fn join(&mut self, player: PlayerId) {
        let s = self.seed + player as u64 * 1000;
        let chan = Reliable::new();
        self.ends.insert(
            player,
            End {
                client: Client::new(player, defs()),
                chan,
                up: Link::new(DELAY, JITTER, LOSS, s + 1),
                down: Link::new(DELAY, JITTER, LOSS, s + 2),
                bodies: Link::new(DELAY, JITTER, LOSS, s + 3),
                tracks: BTreeMap::new(),
                sent: Vec::new(),
            },
        );
        let mut hc = Reliable::new();
        for ev in self.host.state() {
            assert!(hc.send(&wire::encode(&ToClient::Event(ev))));
        }
        self.host_chans.insert(player, hc);
    }

    /// The player asks; it shows at once and the action goes up.
    fn request(&mut self, player: PlayerId, req: Request) -> Result<Action, Reject> {
        let e = self.ends.get_mut(&player).unwrap();
        let a = e.client.request(req)?;
        assert!(e.chan.send(&wire::encode(&ToHost::Action(a.clone()))));
        e.sent.push(a.clone());
        Ok(a)
    }

    /// The host puts something into the world (level content); everyone hears.
    fn spawn(&mut self, stack: container::Stack, at: Placement) -> LooseId {
        let ev = self.host.spawn(stack, at, None);
        let Event::Spawned { item, .. } = &ev else { unreachable!() };
        let item = *item;
        self.broadcast(None, &ev);
        item
    }

    /// An event to every client, or only to `only` when it is that player's answer.
    fn broadcast(&mut self, only: Option<PlayerId>, ev: &Event) {
        let bytes = wire::encode(&ToClient::Event(ev.clone()));
        for (p, c) in &mut self.host_chans {
            if only.is_none_or(|o| o == *p) {
                assert!(c.send(&bytes));
            }
        }
    }

    fn step(&mut self) {
        self.now += DT;
        let now = self.now;
        // Client side: arrivals, messages, sends.
        for e in self.ends.values_mut() {
            for b in e.down.ready(now) {
                e.chan.on_datagram(now, &Datagram::decode(&b).unwrap());
            }
            while let Some(m) = e.chan.recv() {
                let Some(ToClient::Event(ev)) = wire::decode(&m) else { panic!("garbled event") };
                if let Event::Rest { item, at, t, .. } = &ev {
                    // The resting pose joins the track, like a streamed one.
                    e.tracks.entry(*item).or_default().push(Pose { t: *t, frame: BodyFrame::Planet(0), pos: DVec3::from_array(at.pos), vel: DVec3::ZERO, rot: DQuat::IDENTITY });
                }
                e.client.apply(&ev);
            }
            for b in e.bodies.ready(now) {
                let d = BodyDatagram::decode(&b).unwrap();
                for s in &d.bodies {
                    e.tracks.entry(s.id).or_default().push(Pose::of(d.t, s));
                }
            }
            for d in e.chan.poll(now) {
                e.up.enqueue(now, d.encode());
            }
        }
        // Host side: arrivals, requests, answers.
        let players: Vec<PlayerId> = self.ends.keys().copied().collect();
        for p in &players {
            let arrived: Vec<Vec<u8>> = self.ends.get_mut(p).unwrap().up.ready(now);
            let chan = self.host_chans.get_mut(p).unwrap();
            for b in arrived {
                chan.on_datagram(now, &Datagram::decode(&b).unwrap());
            }
            let mut actions = Vec::new();
            while let Some(m) = chan.recv() {
                let Some(ToHost::Action(a)) = wire::decode(&m) else { panic!("garbled action") };
                assert_eq!(a.player, *p, "the host takes the player from the channel, not the message");
                actions.push(a);
            }
            for a in actions {
                for ev in self.host.handle(&a) {
                    let only = matches!(ev, Event::Confirmed { .. } | Event::Rejected { .. }).then_some(a.player);
                    self.broadcast(only, &ev);
                }
            }
        }
        for (p, c) in &mut self.host_chans {
            for d in c.poll(now) {
                self.ends.get_mut(p).unwrap().down.enqueue(now, d.encode());
            }
        }
    }

    fn run(&mut self, seconds: f64) {
        for _ in 0..(seconds / DT).round() as usize {
            self.step();
        }
    }
}

fn wrench(n: &mut Net, x: f64) -> LooseId {
    let s = defs().get("wrench").unwrap().stack_of(1);
    n.spawn(s, at(x))
}

const SEEDS: [u64; 4] = [1, 22, 333, 4444];

#[test]
fn two_players_reach_for_the_same_item_first_at_the_host_wins() {
    for seed in SEEDS {
        let mut n = Net::new(seed);
        n.join(1);
        n.join(2);
        let w = wrench(&mut n, 1.0);
        n.run(1.0);
        assert!(n.ends[&1].client.loose.contains_key(&w) && n.ends[&2].client.loose.contains_key(&w), "seed {seed}: both see the item");
        // Player 2 reaches 5 ticks (83 ms, more than the links' jitter) before player 1: both are
        // 150 ms away, so 2 is first at the host.
        let b = n.request(2, Request::Collect { item: w }).unwrap();
        for _ in 0..5 {
            n.step();
        }
        let a = n.request(1, Request::Collect { item: w }).unwrap();
        assert_eq!((n.ends[&1].client.view.count("wrench"), n.ends[&2].client.view.count("wrench")), (1, 1), "seed {seed}: both show it at once");
        assert_eq!(n.ends[&1].client.confirmed.count("wrench"), 0, "nothing is confirmed yet");
        n.run(3.0);
        assert_eq!(n.ends[&2].client.confirmed.count("wrench"), 1, "seed {seed}: the first request wins");
        assert_eq!(n.ends[&2].client.view.count("wrench"), 1);
        assert_eq!(n.ends[&1].client.view.count("wrench"), 0, "seed {seed}: the other rolls back");
        assert_eq!(n.ends[&1].client.rejected, vec![(a.seq, Reject::Gone)]);
        assert!(n.ends[&2].client.rejected.is_empty() && b.seq == 1);
        assert!(n.ends[&1].client.pending.is_empty() && n.ends[&2].client.pending.is_empty());
        assert!(n.ends.values().all(|e| e.client.loose.is_empty()), "the item is gone for everyone");
        assert_eq!(n.host.inventory(2).count("wrench") + n.host.inventory(1).count("wrench"), 1);
        // Player 2's inventory is known to player 1 (the held item shows on the figure).
        assert_eq!(n.ends[&1].client.held_by(2).map(|s| s.item.kind.as_str()), Some("wrench"));
    }
}

#[test]
fn one_player_drops_and_the_other_picks_it_up() {
    for seed in SEEDS {
        let mut n = Net::new(seed);
        n.join(1);
        n.join(2);
        let w = wrench(&mut n, 1.0);
        n.run(1.0);
        n.request(1, Request::Collect { item: w }).unwrap();
        n.run(1.5);
        assert_eq!(n.ends[&1].client.confirmed.count("wrench"), 1);
        let drop = n.request(1, Request::Drop { slot: 0, at: at(3.0) }).unwrap();
        assert_eq!(n.ends[&1].client.view.count("wrench"), 0, "gone from the bar at once");
        assert!(n.ends[&1].client.pending_drop(drop.seq));
        n.run(1.5);
        let dropped: Vec<LooseId> = n.host.loose.keys().copied().collect();
        assert_eq!(dropped.len(), 1, "seed {seed}");
        let item = dropped[0];
        assert!(n.ends[&2].client.loose.contains_key(&item), "seed {seed}: the other player sees it lie there");
        n.request(2, Request::Collect { item }).unwrap();
        n.run(2.0);
        assert_eq!(n.ends[&2].client.confirmed.count("wrench"), 1, "seed {seed}");
        assert_eq!(n.ends[&1].client.view.count("wrench"), 0);
        assert!(n.host.loose.is_empty() && n.ends.values().all(|e| e.client.loose.is_empty()));
        assert!(n.ends.values().all(|e| e.client.rejected.is_empty()), "nothing was refused");
    }
}

#[test]
fn a_late_joiner_gets_the_state_as_it_is() {
    for seed in SEEDS {
        let mut n = Net::new(seed);
        n.join(1);
        let a = wrench(&mut n, 1.0);
        let _b = wrench(&mut n, 2.0);
        n.spawn(defs().get("bandage").unwrap().stack_of(3), at(3.0));
        n.run(1.0);
        n.request(1, Request::Collect { item: a }).unwrap();
        n.request(1, Request::Select { slot: 2 }).unwrap();
        n.run(2.0);
        // Player 2 comes in late: a wrench and the bandages lie there, player 1 carries a wrench.
        n.join(2);
        n.run(2.0);
        let late = &n.ends[&2].client;
        assert_eq!(late.loose, n.host.loose, "seed {seed}: the loose items as the host has them");
        assert_eq!(late.loose.len(), 2);
        assert_eq!(late.others.get(&1), Some(&n.host.inventory(1)), "and what player 1 carries");
        assert_eq!(late.held_by(1), None, "player 1 holds slot 2, which is empty");
        // The late joiner can act at once.
        let item = *late.loose.keys().next().unwrap();
        n.request(2, Request::Collect { item }).unwrap();
        n.run(2.0);
        assert!(n.host.inventory(2).bar.slot(0).is_some(), "seed {seed}: it took the item");
    }
}

#[test]
fn a_forged_action_is_refused_at_the_host() {
    let mut n = Net::new(5);
    n.join(1);
    let bad = Placement { vel: [1.0e6, 0.0, 0.0], ..at(0.0) };
    let w = wrench(&mut n, 1.0);
    n.run(1.0);
    n.request(1, Request::Collect { item: w }).unwrap();
    n.run(1.5);
    // Sent past the client's own checks: a throw at a million metres a second.
    let forged = Action { player: 1, seq: 50, req: Request::Throw { slot: 0, at: bad } };
    assert_eq!(n.host.handle(&forged), vec![Event::Rejected { player: 1, seq: 50, why: Reject::Invalid }]);
    let nan = Placement { pos: [f64::NAN, 0.0, 0.0], ..at(0.0) };
    let forged = Action { player: 1, seq: 51, req: Request::Drop { slot: 0, at: nan } };
    assert_eq!(n.host.handle(&forged), vec![Event::Rejected { player: 1, seq: 51, why: Reject::Invalid }]);
    assert_eq!(n.host.inventory(1).count("wrench"), 1, "the wrench stayed in the bar");
}

/// A thrown wrench: the host's physics moves it 30 times a second as datagrams, stops sending when
/// it lies still and says where it came to rest as an event. Both clients show it at the same
/// spot, and while it flew they showed it close to where the host had it.
#[test]
fn a_thrown_item_lands_at_the_same_spot_on_both() {
    for seed in SEEDS {
        let mut n = Net::new(seed);
        n.join(1);
        n.join(2);
        let w = wrench(&mut n, 1.0);
        n.run(1.0);
        n.request(1, Request::Collect { item: w }).unwrap();
        n.run(1.5);
        // Thrown at 6 m/s forward, 3 up, from 1.5 m up; the host simulates it.
        let throw_at = Placement { pos: [0.0, 1.5, 0.0], vel: [6.0, 3.0, 0.0], ..at(0.0) };
        n.request(1, Request::Throw { slot: 0, at: throw_at }).unwrap();
        // Run until the host has handled it.
        let item = loop {
            n.step();
            if let Some(id) = n.host.loose.keys().next().copied() {
                break id;
            }
            assert!(n.now < 10.0);
        };
        let (mut pos, mut vel) = (DVec3::new(0.0, 1.5, 0.0), DVec3::new(6.0, 3.0, 0.0));
        let mut resting = false;
        let mut stream_ticks_after_rest = 0;
        let mut worst: f64 = 0.0;
        let mut history: Vec<(f64, DVec3)> = Vec::new();
        let mut since = 0.0;
        let t_end = n.now + 4.0;
        while n.now < t_end {
            // The host's physics, 60 Hz: gravity, a bounce-less landing at y = 0.
            if !resting {
                vel.y -= 9.81 * DT;
                pos += vel * DT;
                if pos.y <= 0.0 {
                    pos.y = 0.0;
                    vel = DVec3::ZERO;
                    resting = true;
                    let rest = Placement { pos: pos.to_array(), vel: [0.0; 3], ..at(0.0) };
                    let ev = n.host.rest(item, rest, [0.0, 0.0, 0.0, 1.0], n.now).expect("still there");
                    n.broadcast(None, &ev);
                }
            }
            history.push((n.now, pos));
            since += DT;
            if since >= 1.0 / 30.0 {
                since = 0.0;
                if !resting {
                    n.stream_seq += 1;
                    let d = BodyDatagram { t: n.now, seq: n.stream_seq, bodies: vec![BodyState { id: item, frame: BodyFrame::Planet(0), pos, vel, rot: DQuat::IDENTITY }] };
                    n.streams += 1;
                    for e in n.ends.values_mut() {
                        e.bodies.enqueue(n.now, d.encode());
                    }
                } else {
                    stream_ticks_after_rest += 1; // nothing is sent
                }
            }
            n.step();
            // What a client shows: its track at "now" minus the host-to-client transit and the playout buffer.
            let shown_t = n.now - 0.15 - 0.15;
            for e in n.ends.values() {
                if let Some(tr) = e.tracks.get(&item) {
                    if shown_t > 0.0 && !resting {
                        let (p, _) = tr.sample_extrapolated(shown_t, 0.1).unwrap();
                        if let Some((_, truth)) = history.iter().rev().find(|(t, _)| *t <= shown_t) {
                            // Only once the client has had the first datagram for that time.
                            if tr.history.front().is_some_and(|f| f.t <= shown_t) {
                                worst = worst.max((p.pos - *truth).length());
                            }
                        }
                    }
                }
            }
        }
        assert!(resting && stream_ticks_after_rest > 10, "seed {seed}: it came to rest and the host stopped sending");
        assert!(n.streams > 3, "seed {seed}: it streamed while it flew ({})", n.streams);
        // Every client has the resting pose, exactly the host's.
        for (p, e) in &n.ends {
            let tr = &e.tracks[&item];
            let (shown, _) = tr.sample(n.now).unwrap();
            assert_eq!(shown.pos, pos, "seed {seed}: client {p} shows it where the host has it");
            assert_eq!(e.client.loose[&item].at.pos, pos.to_array(), "seed {seed}: and knows it as lying there");
        }
        assert_eq!(n.host.loose[&item].at.pos, pos.to_array());
        assert!(worst < 0.6, "seed {seed}: in flight the shown path stayed within {worst:.2} m of the host's");
    }
}
