//! Two app instances in one process over a UDP loopback with simulated link delay and loss.
use bevy::math::DVec3;
use bevy::prelude::*;
use exo_app::cargo::Crate;
use exo_app::controls::Controls;
use exo_app::interact::Interaction;
use exo_app::health::HealthState;
use exo_app::public_status::RemoteStatus;
use exo_app::repair::{HostSession, RepairSessions};
use health_core::Env;
use item_core::wire::{Doing, PublicStatus, RepairDevice, RepairTool};
use exo_app::items::{Items, LooseItem, Role};
use exo_app::menu::{start_session_with, Session};
use exo_app::net_live::Net;
use exo_app::walker::Player;
use exo_app::{build_app, Options};
use item_core::{FrameRef, Placement, Reject, Request};
use std::time::{Duration, Instant};

fn sim_args() -> Vec<(String, String)> {
    ["--sim-delay=150", "--sim-jitter=10", "--sim-loss=2"].iter().map(|a| {
        let (k, v) = a.split_once('=').unwrap();
        (k.to_string(), v.to_string())
    }).collect()
}

struct Pair {
    host: App,
    client: App,
    start: Instant,
}

impl Pair {
    fn step(&mut self) {
        self.host.update();
        self.client.update();
        std::thread::sleep(Duration::from_millis(2));
    }

    fn until(&mut self, secs: f64, what: &str, mut done: impl FnMut(&mut Pair) -> bool) -> f64 {
        let t0 = Instant::now();
        while !done(self) {
            assert!(t0.elapsed().as_secs_f64() < secs, "timed out after {secs} s: {what}");
            self.step();
        }
        let took = t0.elapsed().as_secs_f64();
        println!("[{:5.1} s] {what} (waited {took:.2} s)", self.start.elapsed().as_secs_f64());
        took
    }

    fn run(&mut self, secs: f64) {
        let t0 = Instant::now();
        while t0.elapsed().as_secs_f64() < secs {
            self.step();
        }
    }
}

fn now(app: &App) -> f64 {
    app.world().resource::<Time<Fixed>>().elapsed_secs_f64()
}

fn request(app: &mut App, req: Request) -> Result<u32, Reject> {
    let t = now(app);
    app.world_mut().resource_mut::<Items>().request(t, req)
}

fn items(app: &App) -> &Items {
    app.world().resource::<Items>()
}

fn count(app: &App, def: &str) -> u32 {
    items(app).client.view.count(def)
}

fn confirmed(app: &App, def: &str) -> u32 {
    items(app).client.confirmed.count(def)
}

/// A loose item as a process has it: host id, frame-local position, the ship it is in, asleep.
#[derive(Clone, Copy)]
struct L {
    id: Option<u64>,
    pos: DVec3,
    ship: Option<Entity>,
    asleep: bool,
}

fn loose(app: &mut App) -> Vec<L> {
    let w = app.world_mut();
    let mut q = w.query::<(&LooseItem, &Crate)>();
    q.iter(w).map(|(li, c)| L { id: li.id, pos: c.body.pos, ship: c.ship, asleep: c.body.asleep }).collect()
}

fn walker(app: &mut App) -> (DVec3, DVec3) {
    let planet = app.world().resource::<exo_app::env::PlanetRes>().clone();
    let w = app.world_mut();
    let p = w.query::<&Player>().single(w).unwrap();
    (p.w.pos, planet.up(p.w.pos))
}

fn planet_id(app: &App) -> u32 {
    app.world().resource::<exo_app::env::PlanetRes>().id.0 as u32
}

/// A placement 1.5 m in front of the walker, above the ground, moving at `vel`.
fn in_front(app: &mut App, vel: impl Fn(DVec3, DVec3) -> DVec3) -> Placement {
    let (pos, up) = walker(app);
    let fwd = up.any_orthonormal_vector();
    Placement { frame: FrameRef::Planet(planet_id(app)), pos: (pos + fwd * 1.5 + up * 1.2).to_array(), vel: vel(fwd, up).to_array(), forward: fwd.to_array() }
}

fn spawn(app: &mut App, def: &str, at: Placement) {
    let t = now(app);
    let mut it = app.world_mut().resource_mut::<Items>();
    let stack = it.defs.get(def).expect("item").stack_of(1);
    it.spawn(t, stack, at);
}

#[test]
fn two_players_share_items_over_150_ms_and_2_percent_loss() {
    let port = 19000 + (std::process::id() % 1000) as u16;
    let o = Options { headless: true, ..Default::default() };
    let mut host = build_app(&o);
    let mut client = build_app(&o);
    for app in [&mut host, &mut client] {
        app.finish();
        app.cleanup();
        app.update();
    }
    let start = Instant::now();
    let at0 = in_front(&mut host, |_, _| DVec3::ZERO);
    spawn(&mut host, "wrench", at0);
    host.update();
    start_session_with(host.world_mut(), &Session::Host { port }, &sim_args()).expect("host");
    start_session_with(client.world_mut(), &Session::Join { address: format!("127.0.0.1:{port}"), slot: 2 }, &sim_args()).expect("join");
    let mut p = Pair { host, client, start };

    p.until(30.0, "connected, both see each other", |p| {
        p.host.world_mut().query::<&exo_app::net_live::RemoteWalker>().iter(p.host.world()).count() == 1
            && p.client.world_mut().query::<&exo_app::net_live::RemoteWalker>().iter(p.client.world()).count() == 1
    });
    assert_eq!((items(&p.host).role, items(&p.client).role), (Role::Host, Role::Client));
    assert_eq!(items(&p.client).me(), 2);
    p.until(10.0, "the late joiner has the wrench lying there", |p| loose(&mut p.client).len() == 1);
    let w0 = loose(&mut p.host)[0].id.expect("id");
    assert_eq!(loose(&mut p.client)[0].id, Some(w0), "the same item id on both");
    // It settled on the host and the client shows it at the same spot. Wait for that, not for a
    // fixed time: the physics runs on sim time, which follows the frames, so under load a fixed
    // two seconds of real time can end before the wrench lies still.
    p.until(30.0, "the late joiner's wrench came to rest at the host's spot", |p| {
        let (h, c) = (loose(&mut p.host)[0], loose(&mut p.client)[0]);
        h.asleep && h.pos.distance(c.pos) < 0.01
    });

// The client's request needs 150 ms to cross, so the host's is first and wins.
    let seq_b = request(&mut p.client, Request::Collect { item: w0 }).expect("predicted");
    assert_eq!(count(&p.client, "wrench"), 1, "the client shows it at once");
    assert_eq!(confirmed(&p.client, "wrench"), 0);
    request(&mut p.host, Request::Collect { item: w0 }).expect("host predicts too");
    p.until(10.0, "the host has the wrench", |p| confirmed(&p.host, "wrench") == 1);
    p.until(10.0, "the client's prediction rolled back", |p| count(&p.client, "wrench") == 0 && items(&p.client).client.pending.is_empty());
    assert_eq!(items(&p.client).client.rejected, vec![(seq_b, Reject::Gone)], "the second request was told it was gone");
    assert_eq!(confirmed(&p.client, "wrench"), 0);
    assert!(loose(&mut p.client).is_empty() && loose(&mut p.host).is_empty(), "the wrench is gone from both worlds");
    p.until(10.0, "the client knows the host holds the wrench", |p| items(&p.client).client.held_by(1).is_some_and(|s| s.item.kind.as_str() == "wrench"));

    let at = in_front(&mut p.host, |_, _| DVec3::ZERO);
    request(&mut p.host, Request::Drop { slot: 0, at }).expect("drop");
    assert_eq!(count(&p.host, "wrench"), 0);
    p.until(10.0, "the client sees it lie there", |p| loose(&mut p.client).len() == 1 && loose(&mut p.host).len() == 1);
    let dropped = loose(&mut p.client)[0].id.expect("id");
    p.until(15.0, "it came to rest on the host and the client shows it there", |p| {
        let (h, c) = (loose(&mut p.host)[0], loose(&mut p.client)[0]);
        h.asleep && h.pos.distance(c.pos) < 0.01 && items(&p.client).client.loose.get(&dropped).is_some_and(|l| DVec3::from_array(l.at.pos).distance(h.pos) < 0.01)
    });
    request(&mut p.client, Request::Collect { item: dropped }).expect("predicted");
    p.until(10.0, "the client has it", |p| confirmed(&p.client, "wrench") == 1);
    p.until(10.0, "it is gone from the host", |p| loose(&mut p.host).is_empty() && loose(&mut p.client).is_empty());
    assert_eq!(confirmed(&p.host, "wrench"), 0, "the host's bar is empty again");
    assert!(items(&p.client).client.rejected.len() == 1, "no new refusals");

    let slot = items(&p.client).client.view.selected;
    let at = in_front(&mut p.client, |fwd, up| fwd * 7.0 + up * 3.0);
    request(&mut p.client, Request::Throw { slot, at }).expect("throw");
    assert_eq!(count(&p.client, "wrench"), 0, "out of the bar at once");
    p.until(10.0, "the host has the thrown item", |p| loose(&mut p.host).len() == 1);
    p.until(20.0, "it lies still on the host", |p| {
        let h = loose(&mut p.host)[0];
        h.asleep
    });
    p.until(10.0, "the client shows it at the host's spot", |p| {
        let (h, c) = (loose(&mut p.host)[0], loose(&mut p.client)[0]);
        h.pos.distance(c.pos) < 0.01
    });
    let thrown = loose(&mut p.client)[0].id.unwrap();
    request(&mut p.host, Request::Collect { item: thrown }).expect("host collects");
    p.until(10.0, "gone from both again", |p| loose(&mut p.host).is_empty() && loose(&mut p.client).is_empty());
    assert_eq!(confirmed(&p.host, "wrench"), 1);

    let in_cabin = Placement { frame: FrameRef::Ship(1), pos: [-1.3, exo_app::cargo::FLOOR_Y + 0.05, 0.8], vel: [0.0; 3], forward: [0.0, 0.0, -1.0] };
    spawn(&mut p.host, "bandage", in_cabin);
    p.until(10.0, "the client has the item from the cabin", |p| loose(&mut p.client).len() == 1);
    p.until(15.0, "it lies still in the host's cabin and the client shows it at the same place of the ship", |p| {
        let (h, c) = (loose(&mut p.host)[0], loose(&mut p.client)[0]);
        h.ship.is_some() && h.asleep && c.ship.is_some() && h.pos.distance(c.pos) < 0.01
    });
    let proxy = p.client.world().resource::<Net>().proxy_of(1).expect("the host's ship is a proxy on the client");
    assert_eq!(loose(&mut p.client)[0].ship, Some(proxy), "the client's item rides the proxy of the host's ship");
    p.run(1.0);
    let sent = p.host.world().resource::<Net>().st.bodies_tx;
    p.run(1.5);
    assert_eq!(p.host.world().resource::<Net>().st.bodies_tx, sent, "no poses while everything lies still");

    let proxy = p.client.world().resource::<Net>().proxy_of(1).unwrap();
    {
        let w = p.client.world_mut();
        let mut pl = w.query::<&mut Player>().single_mut(w).unwrap();
        pl.ship = Some(proxy);
        pl.w.pos = DVec3::new(-1.3, 0.32, 1.5);
        pl.w.forward = DVec3::NEG_Z;
        pl.cabin_up = DVec3::Y;
        pl.pitch = -1.1;
    }
    let target = |p: &mut Pair| p.client.world().resource::<Interaction>().prompt.clone();
    p.until(10.0, "the client is offered the bandage in the host's cabin", |p| target(p).contains("pick up Bandage"));
    p.client.world_mut().resource_mut::<Controls>().taps.push(KeyCode::KeyF);
    p.until(10.0, "the host confirmed the client's bandage", |p| confirmed(&p.client, "bandage") == 1);
    p.until(10.0, "the bandage is gone from the cabin on both", |p| loose(&mut p.host).is_empty() && loose(&mut p.client).is_empty());
    assert_eq!(items(&p.host).host.inventory(2).count("bandage"), 1, "the host has it in player 2's bar");

    let slot = items(&p.client).client.view.selected;
    let seq = request(&mut p.client, Request::Drop { slot, at: Placement { frame: FrameRef::Ship(2), pos: [0.0, 1.0, 0.0], vel: [0.0; 3], forward: [0.0, 0.0, -1.0] } }).expect("predicted");
    assert_eq!(count(&p.client, "bandage"), 0);
    p.until(10.0, "the refused drop rolled back", |p| count(&p.client, "bandage") == 1);
    assert_eq!(items(&p.client).client.rejected.last(), Some(&(seq, Reject::Invalid)));
    let at = in_front(&mut p.host, |_, _| DVec3::ZERO);
    spawn(&mut p.host, "extinguisher", at);
    p.until(10.0, "both see the extinguisher", |p| loose(&mut p.client).len() == 1 && loose(&mut p.host).len() == 1);
    let e1 = loose(&mut p.client)[0].id.expect("id");
    request(&mut p.client, Request::Collect { item: e1 }).expect("predicted");
    // The host's own request comes once its world has the client's (the item is gone there). A
    // fixed delay would race the client's send, which follows the frames and so the load.
    p.until(10.0, "the host took the extinguisher for the client's request", |p| loose(&mut p.host).is_empty());
    assert_eq!(request(&mut p.host, Request::Collect { item: e1 }), Err(Reject::Gone), "the client was first at the host");
    p.until(10.0, "the client's extinguisher is confirmed", |p| confirmed(&p.client, "extinguisher") == 1);
    assert_eq!(confirmed(&p.host, "extinguisher"), 0);
    assert_eq!(items(&p.host).host.inventory(2).count("extinguisher"), 1);
    println!("[{:5.1} s] done; host rx {} B, tx {} B", p.start.elapsed().as_secs_f64(), p.host.world().resource::<Net>().st.wire_rx, p.host.world().resource::<Net>().st.wire_tx);
}

#[test]
fn a_bandage_used_by_a_client_on_the_host_heals_the_host_and_the_stack_shrinks() {
    let port = 20000 + (std::process::id() % 1000) as u16;
    let o = Options { headless: true, ..Default::default() };
    let mut host = build_app(&o);
    let mut client = build_app(&o);
    for app in [&mut host, &mut client] {
        app.finish();
        app.cleanup();
        app.update();
    }
    let start = Instant::now();
    start_session_with(host.world_mut(), &Session::Host { port }, &sim_args()).expect("host");
    start_session_with(client.world_mut(), &Session::Join { address: format!("127.0.0.1:{port}"), slot: 2 }, &sim_args()).expect("join");
    let mut p = Pair { host, client, start };
    p.until(30.0, "connected", |p| {
        p.host.world_mut().query::<&exo_app::net_live::RemoteWalker>().iter(p.host.world()).count() == 1
            && p.client.world_mut().query::<&exo_app::net_live::RemoteWalker>().iter(p.client.world()).count() == 1
    });
    p.until(5.0, "the host has a body for its player", |p| p.host.world().resource::<HealthState>().bodies.contains_key(&1));
    {
        let st = p.host.world_mut().resource_mut::<HealthState>().into_inner();
        st.bodies.get_mut(&1).unwrap().health.add(&st.tuning, "burn", 60.0);
    }
    let burn = |p: &Pair| p.host.world().resource::<HealthState>().bodies.get(&1).map_or(0.0, |b| b.health.strength("burn"));
    assert!(burn(&p) >= 59.0, "the host burns: {}", burn(&p));
    let at = in_front(&mut p.client, |_, _| DVec3::ZERO);
    spawn(&mut p.host, "bandage", at);
    p.until(10.0, "the client sees the bandage", |p| loose(&mut p.client).len() == 1);
    let b1 = loose(&mut p.client)[0].id.expect("id");
    request(&mut p.client, Request::Collect { item: b1 }).expect("predicted");
    p.until(10.0, "the host confirmed the client's bandage", |p| confirmed(&p.client, "bandage") == 1);
    assert_eq!(items(&p.client).client.view.slot(0).map(|s| s.item.kind.as_str()), Some("bandage"));
    let burn_before = burn(&p);
    request(&mut p.client, Request::Use { slot: 0, target: 1 }).expect("predicted");
    assert_eq!(count(&p.client, "bandage"), 0, "shown at once: the client's stack is empty");
    p.until(10.0, "the host applied the bandage", |p| burn(p) < burn_before);
    p.until(10.0, "the host confirmed the use", |p| confirmed(&p.client, "bandage") == 0 && items(&p.client).client.pending.is_empty());
    assert_eq!(items(&p.host).host.inventory(2).count("bandage"), 0, "the host's record of the client's stack");
    p.until(10.0, "the client has its vitals from the host", |p| items(&p.client).vitals_in.is_some());
    let v = items(&p.client).vitals_in.expect("vitals");
    assert!(v.burn_level == 0 && !v.burning && v.down.is_none(), "the client's own vitals are calm: {v:?}");
    println!("[{:5.1} s] done; the host's burn {:.1}", p.start.elapsed().as_secs_f64(), burn(&p));
}

fn status(app: &App, id: u32) -> PublicStatus {
    items(app).public.iter().find(|(p, _)| *p == id).map(|(_, s)| *s).unwrap_or_default()
}

fn figure(app: &mut App, owner: u32) -> Option<PublicStatus> {
    let w = app.world_mut();
    let mut q = w.query::<(&exo_app::net_live::RemoteWalker, &RemoteStatus)>();
    q.iter(w).find(|(rw, _)| rw.owner == owner).map(|(_, s)| s.0)
}

fn ignite(app: &mut App) {
    let mut st = app.world_mut().resource_mut::<HealthState>();
    let st = &mut *st;
    let b = st.bodies.get_mut(&exo_app::items::LOCAL).expect("the host's body");
    for _ in 0..10_000 {
        if b.health.burning() {
            return;
        }
        b.health.step(&st.tuning, 0.1, Env { oxygen: 1.0, hot: true, glow: false, fire: 0.0, rod_per_s: 0.0 });
    }
    panic!("the host's body did not catch fire");
}

fn set_down(app: &mut App, down: bool) {
    let mut st = app.world_mut().resource_mut::<HealthState>();
    st.bodies.get_mut(&exo_app::items::LOCAL).expect("the host's body").respawn_in = down.then_some(5.0);
}

#[test]
fn the_client_sees_the_host_burning_down_and_repairing() {
    let port = 21000 + (std::process::id() % 1000) as u16;
    let o = Options { headless: true, ..Default::default() };
    let mut host = build_app(&o);
    let mut client = build_app(&o);
    for app in [&mut host, &mut client] {
        app.finish();
        app.cleanup();
        app.update();
    }
    start_session_with(host.world_mut(), &Session::Host { port }, &sim_args()).expect("host");
    start_session_with(client.world_mut(), &Session::Join { address: format!("127.0.0.1:{port}"), slot: 2 }, &sim_args()).expect("join");
    let start = Instant::now();
    let mut p = Pair { host, client, start };
    p.until(30.0, "connected, the client has the host's figure", |p| {
        p.client.world_mut().query::<&exo_app::net_live::RemoteWalker>().iter(p.client.world()).count() == 1
    });

    ignite(&mut p.host);
    p.until(15.0, "the client's figure of the host burns", |p| figure(&mut p.client, LOCAL_HOST).is_some_and(|s| s.burning));

    set_down(&mut p.host, true);
    p.until(15.0, "the client's figure of the host is down", |p| figure(&mut p.client, LOCAL_HOST).is_some_and(|s| s.down));
    assert!(status(&p.client, LOCAL_HOST).down, "the public status says down too");
    set_down(&mut p.host, false);

    p.host.world_mut().resource_mut::<RepairSessions>().sessions.push(HostSession { player: LOCAL_HOST, device: RepairDevice::Reactor, tool: RepairTool::Wrench, hits: 0, gated: false });
    p.until(15.0, "the client's figure of the host repairs the reactor", |p| figure(&mut p.client, LOCAL_HOST).is_some_and(|s| s.doing == Doing::Repairing(RepairDevice::Reactor)));
    assert_eq!(figure(&mut p.client, LOCAL_HOST), Some(status(&p.client, LOCAL_HOST)), "the figure carries the public status");
    assert!(!status(&p.client, LOCAL_HOST).down, "up again after the down");

    p.host.world_mut().resource_mut::<RepairSessions>().sessions.clear();
    {
        let mut st = p.host.world_mut().resource_mut::<HealthState>();
        let st = &mut *st;
        st.bodies.get_mut(&LOCAL_HOST).expect("the host's body").health.add(&st.tuning, "burn", 110.0);
    }
    p.until(15.0, "the client's figure of the host is unconscious", |p| figure(&mut p.client, LOCAL_HOST).is_some_and(|s| s.unconscious));
    let s = status(&p.client, LOCAL_HOST);
    assert!(s.unconscious && !s.down && s.doing == Doing::Nothing, "the public status says unconscious, not down: {s:?}");
    // Treatments on the host until its vitality is above zero (each takes 18.75 of burn at full skill).
    for _ in 0..10 {
        let mut st = p.host.world_mut().resource_mut::<HealthState>();
        let st = &mut *st;
        let b = st.bodies.get_mut(&LOCAL_HOST).expect("the host's body");
        b.health.treat(&st.tuning, "bandage", 100.0);
        if b.health.vitality() > 0.0 {
            break;
        }
    }
    p.until(15.0, "the client's figure of the host is awake again", |p| figure(&mut p.client, LOCAL_HOST).is_some_and(|s| !s.unconscious));
}

const LOCAL_HOST: u32 = exo_app::items::LOCAL;

#[test]
fn the_client_looking_at_the_burning_host_gets_the_medic_panel_and_the_bandage_highlight() {
    let port = 20500 + (std::process::id() % 1000) as u16;
    let o = Options { headless: true, ..Default::default() };
    let mut host = build_app(&o);
    let mut client = build_app(&o);
    for app in [&mut host, &mut client] {
        app.finish();
        app.cleanup();
        app.update();
    }
    let start = Instant::now();
    start_session_with(host.world_mut(), &Session::Host { port }, &sim_args()).expect("host");
    start_session_with(client.world_mut(), &Session::Join { address: format!("127.0.0.1:{port}"), slot: 2 }, &sim_args()).expect("join");
    let mut p = Pair { host, client, start };
    p.until(30.0, "connected", |p| {
        p.host.world_mut().query::<&exo_app::net_live::RemoteWalker>().iter(p.host.world()).count() == 1
            && p.client.world_mut().query::<&exo_app::net_live::RemoteWalker>().iter(p.client.world()).count() == 1
    });
    p.until(5.0, "the host has a body for its player", |p| p.host.world().resource::<HealthState>().bodies.contains_key(&LOCAL_HOST));
    {
        let st = p.host.world_mut().resource_mut::<HealthState>().into_inner();
        st.bodies.get_mut(&LOCAL_HOST).unwrap().health.add(&st.tuning, "burn", 60.0);
    }
    let at = in_front(&mut p.client, |_, _| DVec3::ZERO);
    spawn(&mut p.host, "bandage", at);
    p.until(10.0, "the client sees the bandage", |p| loose(&mut p.client).len() == 1);
    let b1 = loose(&mut p.client)[0].id.expect("id");
    request(&mut p.client, Request::Collect { item: b1 }).expect("predicted");
    p.until(10.0, "the host confirmed the client's bandage", |p| confirmed(&p.client, "bandage") == 1);
    assert_eq!(items(&p.client).client.view.slot(0).map(|s| s.item.kind.as_str()), Some("bandage"));

    let (hp, hup) = walker(&mut p.host);
    let tangent = hup.any_orthonormal_vector();
    {
        let w = p.client.world_mut();
        let mut pl = w.query::<&mut Player>().single_mut(w).expect("the client's player");
        pl.w.pos = hp + tangent * 2.0;
        pl.w.forward = -tangent;
        pl.pitch = 0.0;
    }
    let medic = |p: &Pair| p.client.world().resource::<exo_app::health_view::MedicView>().clone();
    p.until(15.0, "the client's medic view is about the host", |p| medic(p).target == Some(LOCAL_HOST) && medic(p).view.is_some());
    let m = medic(&p);
    let view = m.view.expect("the panel");
    assert_eq!(view.name, "PILOT 1");
    assert!(view.lines.iter().any(|l| l.label.contains("BURN")), "the panel names the burn: {:?}", view.lines);
    assert_eq!(view.hint.as_deref(), Some("BANDAGE ▶ BURN"), "the bandage suits the burn");
    assert_eq!(m.suitable, vec![0], "only the bandage's slot is highlighted");
    println!("[{:5.1} s] done; panel {:?}", p.start.elapsed().as_secs_f64(), view.lines);
}
