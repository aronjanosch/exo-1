//! Cargo crates over a UDP loopback (#332): two app instances in one process, the second one joins
//! (late) a host that already has crates and an ore tub; the client carries the tub, both ask for
//! the same crate at once, and a client that leaves lets go. Every wait has a time limit.
use bevy::math::DVec3;
use bevy::prelude::*;
use exo_app::cargo::{crate_bundle, Crate, Crates};
use exo_app::controls::Controls;
use exo_app::crate_net::{CrateId, CrateNet, RemoteHeld};
use exo_app::grab::Grab;
use exo_app::items::Items;
use exo_app::menu::{start_session_with, Session};
use exo_app::ore_tub::{self, Tub};
use exo_app::walker::{Player, EYE_HEIGHT};
use exo_app::{build_app, Options};
use grab_core::Reach;
use std::time::{Duration, Instant};
use walker_core::Frame;

fn args(sim: bool) -> Vec<(String, String)> {
    let a: &[&str] = if sim { &["--sim-delay=150", "--sim-jitter=10", "--sim-loss=2"] } else { &[] };
    a.iter().map(|a| {
        let (k, v) = a.split_once('=').unwrap();
        (k.to_string(), v.to_string())
    }).collect()
}

struct Pair {
    host: App,
    client: App,
}

impl Pair {
    fn step(&mut self) {
        self.host.update();
        self.client.update();
        std::thread::sleep(Duration::from_millis(2));
    }

    fn until(&mut self, secs: f64, what: &str, mut done: impl FnMut(&mut Pair) -> bool) {
        let t0 = Instant::now();
        while !done(self) {
            if t0.elapsed().as_secs_f64() >= secs {
                let (hc, cc) = (crates(&mut self.host), crates(&mut self.client));
                let hb: Vec<_> = (1..5).map(|i| host_holder(self, i)).collect();
                let cn = self.client.world().resource::<CrateNet>();
                let tr: Vec<_> = self.client.world().resource::<exo_app::items_net::RemoteBodies>().tracks.iter().filter(|(k, _)| **k & (1 << 63) != 0).map(|(k, t)| (k & 0xffff, t.history.iter().map(|p| (p.t, p.pos.z)).collect::<Vec<_>>())).collect();
                let hbs: Vec<_> = hc.iter().chain(&cc).map(|c| c.handback).collect();
                println!("HANDBACKS {hbs:?}");
                println!("TRACKS {tr:?} playout {:?}", self.client.world().resource::<exo_app::net_live::Net>().playout_time());
                println!("STATE host {hc:?}\nclient {cc:?}\nmine {:?} refused {:?} client book {:?} host book {hb:?}\nheld host {} client {} breaks client {}", cn.mine, cn.refused, (1..5).map(|i| cn.book.holder(i)).collect::<Vec<_>>(), held_crate(&self.host).is_some(), held_crate(&self.client).is_some(), self.client.world().resource::<Grab>().breaks);
                panic!("timed out after {secs} s: {what}");
            }
            self.step();
        }
        println!("{what} (waited {:.2} s)", t0.elapsed().as_secs_f64());
    }
}

/// A crate as a process has it.
#[derive(Clone, Copy, Debug)]
struct C {
    id: u32,
    pos: DVec3,
    asleep: bool,
    tub: Option<u32>,
    size: usize,
    remote_held: bool,
    view: bool,
    handback: Option<Option<f64>>,
}

fn crates(app: &mut App) -> Vec<C> {
    let w = app.world_mut();
    let mut q = w.query::<(&CrateId, &Crate, Option<&Tub>, Has<RemoteHeld>, Has<exo_app::items_net::LooseView>, Option<&exo_app::crate_net::Handback>)>();
    let mut v: Vec<C> = q.iter(w).map(|(i, c, t, rh, view, hb)| C { handback: hb.map(|h| h.until), id: i.0, pos: c.body.pos, asleep: c.body.asleep, tub: t.map(|t| t.rack), size: c.size, remote_held: rh, view }).collect();
    v.sort_by_key(|c| c.id);
    v
}

fn crate_of(app: &mut App, id: u32) -> Option<C> {
    crates(app).into_iter().find(|c| c.id == id)
}

fn walker(app: &mut App) -> (DVec3, DVec3, DVec3) {
    let w = app.world_mut();
    let p = w.query::<&Player>().single(w).unwrap();
    let up = p.world_up(Frame::IDENTITY);
    (p.w.pos, up, p.w.forward)
}

fn sim_now(app: &App) -> f64 {
    app.world().resource::<Time<Fixed>>().elapsed_secs_f64()
}

/// Puts a crate (of `size`) on the ground `ahead` metres in front of the walker and `right` to its side.
fn put_crate(app: &mut App, anchor: (DVec3, DVec3, DVec3), size: &str, ahead: f64, right: f64, tub: Option<u32>) {
    let (pos, up, fwd) = anchor;
    let planet = app.world().resource::<exo_app::env::PlanetRes>().clone();
    let table = app.world().resource::<Crates>().0.clone();
    let side = fwd.cross(up);
    let flat = pos + fwd * ahead + side * right;
    let up = planet.up(flat);
    let at = planet.centre + up * (planet.surface(up) + table.get(size).unwrap().extents[1] * 0.5 + 0.02);
    let mut e = app.world_mut().spawn(crate_bundle(&table, size, None, at, fwd));
    if let Some(rack) = tub {
        e.insert(Tub { rack });
    }
}

fn host_holder(p: &Pair, id: u32) -> Option<u32> {
    p.host.world().resource::<CrateNet>().book.holder(id)
}

fn held_crate(app: &App) -> Option<Entity> {
    app.world().resource::<Grab>().held.map(|h| h.crate_e)
}

/// The same as pressing F in front of the crate: starts a hold at once.
fn take(app: &mut App, id: u32) {
    let w = app.world_mut();
    let cfg = w.resource::<exo_app::tuning::Tuning>().grab;
    let table = w.resource::<Crates>().0.clone();
    let (eye, look) = {
        let p = w.query::<&Player>().single(w).unwrap();
        (p.world_pos(Frame::IDENTITY) + p.world_up(Frame::IDENTITY) * EYE_HEIGHT, p.world_look(Frame::IDENTITY))
    };
    w.resource_scope(|w, mut grab: Mut<Grab>| {
        let mut q = w.query::<(Entity, &CrateId, &Crate)>();
        let (e, _, c) = q.iter(w).find(|(_, i, _)| i.0 == id).expect("crate");
        grab.start(&cfg, e, c, &table.sizes[c.size], Reach::Hands, eye, look, &Frame::IDENTITY);
    });
}

fn load_of(app: &App, rack: u32) -> Option<item_core::container::Container> {
    let it = app.world().resource::<Items>();
    it.host.racks.get(&rack).or_else(|| it.client.racks.get(&rack)).cloned()
}

fn session(sim: bool, leave: bool) -> (Pair, Vec<u32>) {
    let port = 19400 + (std::process::id() % 300) as u16 + if sim { 300 } else { 0 } + if leave { 600 } else { 0 };
    let o = Options { headless: true, ..Default::default() };
    let mut host = build_app(&o);
    let mut client = build_app(&o);
    for app in [&mut host, &mut client] {
        app.finish();
        app.cleanup();
        app.update();
        // The default start puts a crate in the cabin; these tests bring their own.
        let w = app.world_mut();
        let all: Vec<Entity> = w.query_filtered::<Entity, With<Crate>>().iter(w).collect();
        for e in all {
            w.despawn(e);
        }
    }
    // The host has two crates and a tub with ore; one crate was moved before anyone joined.
    let anchor = walker(&mut host);
    put_crate(&mut host, anchor, "small", 3.0, 1.0, None);
    put_crate(&mut host, anchor, "medium", 4.0, -1.5, None);
    put_crate(&mut host, anchor, "tub", 1.3, 0.0, Some(ore_tub::TUB_A));
    {
        let t = sim_now(&host);
        let w = host.world_mut();
        w.resource_scope(|_, mut items: Mut<Items>| {
            let mut c = ore_tub::contents();
            let ore = items.defs.get("grumblite").unwrap().stack_of(4);
            assert!(matches!(c.try_insert(&[], ore), Ok(None)));
            items.add_container(t, ore_tub::TUB_A, c);
        });
    }
    let mut pair = Pair { host, client };
    // Let them settle, then move the small crate 4 m away and let it settle there (before the join).
    pair.until_host(10.0, "the host's crates lie still", |h| crates(h).len() == 3 && crates(h).iter().all(|c| c.asleep));
    let ids: Vec<u32> = crates(&mut pair.host).iter().map(|c| c.id).collect();
    let small = crates(&mut pair.host).iter().find(|c| c.size == 0).unwrap().id;
    {
        let (pos, up, fwd) = anchor;
        let planet = pair.host.world().resource::<exo_app::env::PlanetRes>().clone();
        let to = pos + fwd * 6.0 + fwd.cross(up) * 3.0;
        let u = planet.up(to);
        let at = planet.centre + u * (planet.surface(u) + 0.27);
        let w = pair.host.world_mut();
        let mut q = w.query::<(&CrateId, &mut Crate)>();
        let (_, mut c) = q.iter_mut(w).find(|(i, _)| i.0 == small).unwrap();
        c.body.pos = at;
        c.body.wake();
    }
    pair.until_host(10.0, "the moved crate lies still at its new place", |h| crate_of(h, small).is_some_and(|c| c.asleep && c.asleep));
    start_session_with(pair.host.world_mut(), &Session::Host { port }, &args(sim)).expect("host");
    start_session_with(pair.client.world_mut(), &Session::Join { address: format!("127.0.0.1:{port}"), slot: 2 }, &args(sim)).expect("join");
    (pair, ids)
}

impl Pair {
    fn run_secs(&mut self, secs: f64) {
        let t0 = Instant::now();
        while t0.elapsed().as_secs_f64() < secs {
            self.step();
        }
    }

    fn until_host(&mut self, secs: f64, what: &str, mut done: impl FnMut(&mut App) -> bool) {
        let t0 = Instant::now();
        while !done(&mut self.host) {
            assert!(t0.elapsed().as_secs_f64() < secs, "timed out after {secs} s: {what}");
            self.host.update();
            self.client.update();
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

fn the_wire_carries_crates_and_tubs(sim: bool) {
    let (mut p, ids) = session(sim, false);
    let _ = &ids;
    let kind = |p: &mut Pair, size: &str, tub: bool| {
        let row = p.host.world().resource::<Crates>().0.sizes.iter().position(|s| s.name == size).unwrap();
        crates(&mut p.host).into_iter().find(|c| c.size == row && c.tub.is_some() == tub).unwrap().id
    };
    let (small, medium, tub) = (kind(&mut p, "small", false), kind(&mut p, "medium", false), kind(&mut p, "tub", true));
    let walk_secs = if sim { 60.0 } else { 30.0 };

    p.until(walk_secs, "connected, and the late joiner has all three crates", |p| crates(&mut p.client).len() == 3);
    let (h, c) = (crates(&mut p.host), crates(&mut p.client));
    assert_eq!(h.iter().map(|x| (x.id, x.tub)).collect::<Vec<_>>(), c.iter().map(|x| (x.id, x.tub)).collect::<Vec<_>>(), "the same ids and the same tub on both");
    p.until(walk_secs, "the client shows every crate where the host has it", |p| {
        let (h, c) = (crates(&mut p.host), crates(&mut p.client));
        h.iter().zip(&c).all(|(h, c)| h.pos.distance(c.pos) < 0.02) && c.iter().all(|c| c.view)
    });
    let moved = crate_of(&mut p.client, small).unwrap().pos;
    assert!(moved.distance(walker(&mut p.client).0) > 5.0, "the crate the host moved before the join is shown at its new place");
    p.until(walk_secs, "the client has the tub's load", |p| load_of(&p.client, ore_tub::TUB_A).is_some() && load_of(&p.client, ore_tub::TUB_A) == load_of(&p.host, ore_tub::TUB_A));
    p.until(walk_secs, "the client's tub spots hold the tub by its rack", |p| p.client.world().resource::<ore_tub::TubSpots>().standing(ore_tub::TUB_A).is_some());
    let load_before = load_of(&p.host, ore_tub::TUB_A).unwrap();
    assert_eq!(ore_tub::used_places(&load_before), 2, "four ore, stacks of three: two places");

    // The client carries the tub 3 m and sets it down: its walker (spawned elsewhere) stands beside the host's.
    {
        let (hw, up, fwd) = walker(&mut p.host);
        let w = p.client.world_mut();
        let mut pl = w.query::<&mut Player>().single_mut(w).unwrap();
        pl.w.pos = hw + fwd.cross(up) * 1.0;
        pl.w.forward = fwd;
        pl.w.vel = DVec3::ZERO;
    }
    p.run_secs(0.3);
    let start = walker(&mut p.client).0;
    take(&mut p.client, tub);
    p.until(walk_secs, "the host's book gives the tub to player 2", |p| host_holder(p, tub) == Some(2));
    p.until(walk_secs, "the host shows the tub as held by a client", |p| crate_of(&mut p.host, tub).is_some_and(|c| c.remote_held));
    assert!(p.client.world().resource::<CrateNet>().mine.is_some_and(|m| m.confirmed || m.id == tub));
    p.client.world_mut().resource_mut::<Controls>().held.insert(KeyCode::KeyW);
    p.until(walk_secs, "the client walked 3 m with the tub", |p| walker(&mut p.client).0.distance(start) > 3.0);
    p.client.world_mut().resource_mut::<Controls>().held.remove(&KeyCode::KeyW);
    p.client.world_mut().resource_mut::<Grab>().release();
    p.until(walk_secs, "the host's book is free again", |p| host_holder(p, tub).is_none());
    p.until(walk_secs, "the tub lies still on the host", |p| crate_of(&mut p.host, tub).is_some_and(|c| c.asleep && !c.remote_held && !c.view));
    p.until(walk_secs, "the client shows the tub at the host's place", |p| {
        let (h, c) = (crate_of(&mut p.host, tub).unwrap(), crate_of(&mut p.client, tub).unwrap());
        c.view && h.pos.distance(c.pos) < 0.05
    });
    let at = crate_of(&mut p.host, tub).unwrap().pos;
    let client_walker = walker(&mut p.client).0;
    // The tub went where the client carried it: well away from where it stood, and near the walker.
    let stood = start - walker(&mut p.client).1.cross(walker(&mut p.client).2).normalize() * 1.0 + walker(&mut p.client).2 * 1.3;
    assert!(at.distance(stood) > 2.0, "the tub moved {:.2} m", at.distance(stood));
    assert!(at.distance(client_walker) < 3.0, "the tub is near the client's walker ({:.2} m)", at.distance(client_walker));
    assert_eq!(load_of(&p.host, ore_tub::TUB_A), Some(load_before.clone()), "the load is unchanged on the host");
    assert_eq!(load_of(&p.client, ore_tub::TUB_A), Some(load_before), "and on the client");

    // Both ask for the same crate at once: exactly one holds it. Both walkers stand at the medium crate.
    {
        let at = crate_of(&mut p.host, medium).unwrap().pos;
        let (_, up, fwd) = walker(&mut p.host);
        let side = fwd.cross(up);
        for (app, off) in [(&mut p.host, side * -0.8), (&mut p.client, side * 0.8)] {
            let w = app.world_mut();
            let mut pl = w.query::<&mut Player>().single_mut(w).unwrap();
            pl.w.pos = at - fwd * 1.6 - up * 0.2 + off;
            pl.w.forward = fwd;
            pl.w.vel = DVec3::ZERO;
        }
    }
    p.run_secs(0.5);
    take(&mut p.host, medium);
    take(&mut p.client, medium);
    let settled = |p: &mut Pair| {
        let (hh, ch) = (held_crate(&p.host).is_some(), held_crate(&p.client).is_some());
        match host_holder(p, medium) {
            Some(1) => hh && !ch,
            Some(2) => ch && !hh,
            _ => false,
        }
    };
    p.until(walk_secs, "the book and both processes agree on one holder", settled);
    p.run_secs(1.0);
    assert!(settled(&mut p), "and it stays that way");
    let host_won = host_holder(&p, medium) == Some(1);
    let winner = if host_won { &mut p.host } else { &mut p.client };
    winner.world_mut().resource_mut::<Grab>().release();
    p.until(walk_secs, "the crate is free in the book and lies still on the host", |p| host_holder(p, medium).is_none() && crate_of(&mut p.host, medium).is_some_and(|c| c.asleep && !c.remote_held));
    p.until(walk_secs, "both agree where it is", |p| {
        let (h, c) = (crate_of(&mut p.host, medium).unwrap(), crate_of(&mut p.client, medium).unwrap());
        h.pos.distance(c.pos) < 0.05
    });
    // The client holds the small crate first (the book says so); the host's later ask is refused.
    take(&mut p.client, small);
    p.until(walk_secs, "the book gives the small crate to player 2", |p| host_holder(p, small) == Some(2));
    take(&mut p.host, small);
    p.until(walk_secs, "the host's own late ask was refused", |p| held_crate(&p.host).is_none());
    assert_eq!(host_holder(&p, small), Some(2), "the client still holds it");
    assert!(held_crate(&p.client).is_some());
    p.client.world_mut().resource_mut::<Grab>().release();
    p.until(walk_secs, "free again", |p| host_holder(p, small).is_none());
    assert!(p.host.world().resource::<CrateNet>().book.ids().count() == 3, "no crate lost or doubled");
}

/// One at a time: a starved process loses its connection (the host drops a peer silent for a second), and these tests are heavy.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn crates_and_tubs_over_loopback() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    the_wire_carries_crates_and_tubs(false);
}

#[test]
fn crates_and_tubs_over_150_ms_and_2_percent_loss() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    the_wire_carries_crates_and_tubs(true);
}

#[test]
fn a_holder_who_leaves_lets_the_crate_go() {
    let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let (mut p, _) = session(false, true);
    let small = p.host.world_mut().query::<(&CrateId, &Crate)>().iter(p.host.world()).find(|(_, c)| c.size == 0).unwrap().0 .0;
    p.until(30.0, "the client has the crates", |p| crates(&mut p.client).len() == 3 && crate_of(&mut p.client, small).is_some_and(|c| c.view));
    take(&mut p.client, small);
    p.until(30.0, "the client holds the small crate", |p| host_holder(p, small) == Some(2));
    p.until(30.0, "the host shows it held", |p| crate_of(&mut p.host, small).is_some_and(|c| c.remote_held));
    // The connection is gone as the host notices it.
    p.host.world_mut().resource_mut::<CrateNet>().left_peers.push(2);
    p.until(30.0, "the crate is free and the host's physics has it again", |p| host_holder(p, small).is_none() && crate_of(&mut p.host, small).is_some_and(|c| !c.remote_held && !c.view));
}
