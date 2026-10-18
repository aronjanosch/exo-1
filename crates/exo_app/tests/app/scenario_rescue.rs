//! Headless: the rescue hold is sent directly (no aim or ring without a window).
use bevy::math::DVec3;
use bevy::prelude::*;
use exo_app::cargo::Crate;
use exo_app::health::HealthState;
use exo_app::items::{Items, LooseItem, LOCAL};
use exo_app::menu::{start_session_with, Session};
use exo_app::net_live::RemoteWalker;
use exo_app::public_status::RemoteStatus;
use exo_app::walker::Player;
use exo_app::{build_app, Options};
use health_core::{Env, Life};
use item_core::wire::{Doing, PublicStatus};
use item_core::{FrameRef, Placement, Reject, Request};
use std::time::{Duration, Instant};

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
}

fn now(app: &App) -> f64 {
    app.world().resource::<Time<Fixed>>().elapsed_secs_f64()
}

fn items(app: &App) -> &Items {
    app.world().resource::<Items>()
}

fn request(app: &mut App, req: Request) -> Result<u32, Reject> {
    let t = now(app);
    app.world_mut().resource_mut::<Items>().request(t, req)
}

fn status(app: &App, id: u32) -> PublicStatus {
    items(app).public.iter().find(|(p, _)| *p == id).map(|(_, s)| *s).unwrap_or_default()
}

fn figure(app: &mut App, owner: u32) -> Option<PublicStatus> {
    let w = app.world_mut();
    let mut q = w.query::<(&RemoteWalker, &RemoteStatus)>();
    q.iter(w).find(|(rw, _)| rw.owner == owner).map(|(_, s)| s.0)
}

fn oxygen_low(app: &App) -> f64 {
    app.world().resource::<HealthState>().bodies.get(&LOCAL).map_or(0.0, |b| b.health.strength("oxygen_low"))
}

fn lies(app: &App) -> bool {
    app.world().resource::<HealthState>().bodies.get(&LOCAL).is_some_and(|b| b.health.state() == Life::Unconscious)
}

fn faint_in_bad_air(app: &mut App) {
    let mut st = app.world_mut().resource_mut::<HealthState>();
    let st = &mut *st;
    let b = st.bodies.get_mut(&LOCAL).expect("the host's body");
    for _ in 0..2_000 {
        if b.health.state() == Life::Unconscious {
            return;
        }
        b.health.step(&st.tuning, 0.1, Env { oxygen: 0.0, hot: false, glow: false, fire: 0.0, spicy_per_s: 0.0, vacuum: false, suited: false });
    }
    panic!("the host did not faint in bad air");
}

fn loose(app: &mut App) -> Vec<(Option<u64>, DVec3)> {
    let w = app.world_mut();
    let mut q = w.query::<(&LooseItem, &Crate)>();
    q.iter(w).map(|(li, c)| (li.id, c.body.pos)).collect()
}

/// A placement 1.5 m in front of the walker, above the ground.
fn in_front(app: &mut App) -> Placement {
    let w = app.world_mut();
    let p = w.query::<&Player>().single(w).unwrap();
    let (pos, up) = (p.w.pos, p.up);
    let fwd = up.any_orthonormal_vector();
    let planet = w.resource::<exo_app::env::PlanetRes>().id.0 as u32;
    Placement { frame: FrameRef::Planet(planet), pos: (pos + fwd * 1.5 + up * 1.2).to_array(), vel: [0.0; 3], forward: fwd.to_array() }
}

#[test]
fn a_client_revives_the_host_from_bad_air_and_the_oxy_canister_wakes_it_again() {
    let port = 22000 + (std::process::id() % 1000) as u16;
    let o = Options { headless: true, ..Default::default() };
    let mut host = build_app(&o);
    let mut client = build_app(&o);
    for app in [&mut host, &mut client] {
        app.finish();
        app.cleanup();
        app.update();
    }
    let start = Instant::now();
    start_session_with(host.world_mut(), &Session::Host { port }, &[]).expect("host");
    start_session_with(client.world_mut(), &Session::Join { address: format!("127.0.0.1:{port}"), slot: 2 }, &[]).expect("join");
    let mut p = Pair { host, client, start };
    p.until(30.0, "connected", |p| {
        p.host.world_mut().query::<&RemoteWalker>().iter(p.host.world()).count() == 1
            && p.client.world_mut().query::<&RemoteWalker>().iter(p.client.world()).count() == 1
    });
    p.until(5.0, "the host has a body for its player", |p| p.host.world().resource::<HealthState>().bodies.contains_key(&LOCAL));

    faint_in_bad_air(&mut p.host);
    assert!(lies(&p.host), "the host lies unconscious");
    let low = oxygen_low(&p.host);
    p.until(15.0, "the client's figure of the host is unconscious", |p| figure(&mut p.client, LOCAL).is_some_and(|s| s.unconscious));

    {
        let mut it = p.client.world_mut().resource_mut::<Items>();
        it.ask_rescue(Some(LOCAL));
    }
    p.until(15.0, "the host lists the client as reviving it", |p| status(&p.host, 2).doing == Doing::Reviving(LOCAL));
    p.until(15.0, "the client's own status says reviving the host", |p| status(&p.client, 2).doing == Doing::Reviving(LOCAL));
    p.until(10.0, "the host's figure of the client says REVIVING", |p| figure(&mut p.host, 2).map(|s| s.doing) == Some(Doing::Reviving(LOCAL)));
    assert_eq!(figure(&mut p.client, LOCAL).map(|s| s.unconscious), Some(true), "the client's figure of the host still lies");
    p.until(10.0, "the pump lowers the host's oxygen low", |p| oxygen_low(&p.host) < low - 0.5);

    // A revive roll wakes the host (about one a second at skill 50; the revive waits 2 s).
    p.until(90.0, "the host wakes up after the revive roll", |p| !lies(&p.host));
    assert!(oxygen_low(&p.host) <= 90.0 + 1e-6, "the revive leaves the oxygen low at 90 or less: {}", oxygen_low(&p.host));
    p.until(10.0, "the client sees the host awake", |p| figure(&mut p.client, LOCAL).is_some_and(|s| !s.unconscious));
    p.client.world_mut().resource_mut::<Items>().ask_rescue(None);
    p.until(10.0, "the client's status is nothing again", |p| status(&p.host, 2).doing == Doing::Nothing);

    faint_in_bad_air(&mut p.host);
    p.until(15.0, "the client sees the host unconscious again", |p| figure(&mut p.client, LOCAL).is_some_and(|s| s.unconscious));
    let at = in_front(&mut p.client);
    {
        let t = now(&p.host);
        let mut it = p.host.world_mut().resource_mut::<Items>();
        let stack = it.defs.get("oxy").expect("the oxy item").stack_of(1);
        it.spawn(t, stack, at);
    }
    p.until(10.0, "the client sees the canister", |p| loose(&mut p.client).iter().any(|(id, _)| id.is_some()));
    let id = loose(&mut p.client).iter().find_map(|(id, _)| *id).expect("id");
    request(&mut p.client, Request::Collect { item: id }).expect("collect");
    p.until(10.0, "the host confirmed the canister", |p| items(&p.client).client.confirmed.count("oxy") == 1);
    assert_eq!(items(&p.client).client.view.slot(0).map(|s| s.item.kind.as_str()), Some("oxy"));
    let before = oxygen_low(&p.host);
    request(&mut p.client, Request::Use { slot: 0, target: LOCAL }).expect("use");
    p.until(10.0, "the host's oxygen low drops by the canister", |p| oxygen_low(&p.host) < before - 30.0);
    assert!(!lies(&p.host), "the canister wakes the host at once (oxygen low {})", oxygen_low(&p.host));
    p.until(10.0, "the client sees the host awake after the canister", |p| figure(&mut p.client, LOCAL).is_some_and(|s| !s.unconscious));
    println!("[{:5.1} s] done", p.start.elapsed().as_secs_f64());
}
