//! The sniffer over a UDP loopback (#333, #335): a client on the dev area finds a fizz pocket, taps it
//! (`TapAsk`), and both processes agree on the pocket (`PocketNews`) and on the filled jar. Every wait has a time limit.
use bevy::prelude::*;
use exo_app::controls::Controls;
use exo_app::dev_area::sniffer::{PocketHost, FIZZ_JAR, SNIFFER};
use exo_app::dev_area::DevArea;
use exo_app::items::Items;
use exo_app::menu::{start_session_with, Session};
use exo_app::walker::Player;
use exo_app::{build_app, Options};
use item_core::Request;
use std::time::{Duration, Instant};

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
                let (h, c) = (self.host.world().resource::<Items>().dig.pockets.clone(), self.client.world().resource::<Items>().dig.pockets.clone());
                println!("host news {h:?}\nclient news {c:?}\nhost tappers {:?}", self.host.world().resource::<PocketHost>().tappers);
                panic!("timed out after {secs} s: {what}");
            }
            self.step();
        }
        println!("{what} (waited {:.2} s)", t0.elapsed().as_secs_f64());
    }
}

fn bar_slot(app: &App, kind: &str) -> Option<usize> {
    let v = &app.world().resource::<Items>().client.view;
    (0..8).find(|i| v.slot(*i).is_some_and(|s| s.item.kind.as_str() == kind))
}

fn select(app: &mut App, slot: usize) {
    let now = app.world().resource::<Time>().elapsed_secs_f64();
    let _ = app.world_mut().resource_mut::<Items>().request(now, Request::Select { slot });
}

#[test]
fn a_client_taps_a_pocket_and_fills_a_jar() {
    let port = 20300 + (std::process::id() % 300) as u16;
    let o = Options { headless: true, start: Some("dev_area".into()), ..Default::default() };
    let (mut host, mut client) = (build_app(&o), build_app(&o));
    for app in [&mut host, &mut client] {
        app.finish();
        app.cleanup();
        app.update();
    }
    let mut p = Pair { host, client };
    p.until(30.0, "both have the dev area", |p| [&p.host, &p.client].iter().all(|a| a.world().resource::<DevArea>().frame.is_some()));
    start_session_with(p.host.world_mut(), &Session::Host { port }, &[]).expect("host");
    start_session_with(p.client.world_mut(), &Session::Join { address: format!("127.0.0.1:{port}"), slot: 2 }, &[]).expect("join");
    p.until(30.0, "the client is connected", |p| p.client.world().resource::<exo_app::net_live::Net>().connected());
    {
        // The host hands player 2 a sniffer and two empty jars.
        let now = p.host.world().resource::<Time>().elapsed_secs_f64();
        let w = p.host.world_mut();
        for (id, n) in [(SNIFFER, 1), ("jar", 2)] {
            let stack = w.resource::<Items>().defs.get(id).unwrap().stack_of(n);
            assert!(w.resource_mut::<Items>().give(now, 2, stack).is_ok());
        }
    }
    p.until(30.0, "the client has the sniffer and the jars in its bar", |p| bar_slot(&p.client, SNIFFER).is_some() && bar_slot(&p.client, "jar").is_some());
    p.until(30.0, "the client got the pocket news", |p| p.client.world().resource::<Items>().dig.pockets.is_some());
    let (at, jars) = {
        let host = p.host.world().resource::<PocketHost>();
        let pk = &host.field.pockets()[0];
        (pk.at, pk.jars_left)
    };
    assert_eq!(p.client.world().resource::<Items>().dig.pockets.as_ref().unwrap().pockets[0].at, at, "both know where the first pocket is");

    // The client stands on the pocket with the sniffer in the hand and holds the primary button.
    {
        let planet = p.client.world().resource::<exo_app::env::PlanetRes>().clone();
        let on = p.client.world().resource::<DevArea>().ground(&planet, at, 0.3).unwrap().0;
        let w = p.client.world_mut();
        let mut pl = w.query::<&mut Player>().single_mut(w).unwrap();
        pl.w.pos = on;
        pl.w.vel = bevy::math::DVec3::ZERO;
    }
    let slot = bar_slot(&p.client, SNIFFER).unwrap();
    select(&mut p.client, slot);
    p.until(30.0, "the host's book has the sniffer selected for player 2", |p| {
        let inv = &p.host.world().resource::<Items>().host.inventories[&2];
        inv.slot(inv.selected).is_some_and(|s| s.item.kind.as_str() == SNIFFER)
    });
    p.client.world_mut().resource_mut::<Controls>().primary_held = true;
    p.until(30.0, "the host's pocket is open", |p| matches!(p.host.world().resource::<PocketHost>().field.pockets()[0].state, dig_core::pocket::PocketState::Open { .. }));
    p.client.world_mut().resource_mut::<Controls>().primary_held = false;
    p.until(30.0, "the client's news shows the vent open too", |p| p.client.world().resource::<Items>().dig.pockets.as_ref().is_some_and(|n| n.pockets[0].open_ds > 0));

    // F at the vent with an empty jar: the client asks for the fill.
    let jar = bar_slot(&p.client, "jar").unwrap();
    select(&mut p.client, jar);
    let now = p.client.world().resource::<Time>().elapsed_secs_f64();
    p.client.world_mut().resource_mut::<Items>().request(now, Request::Load { slot: jar, into: 0 }).expect("the client's view lets the fill go");
    p.until(30.0, "the host handed player 2 a fizz jar", |p| p.host.world().resource::<Items>().host.inventories[&2].count(FIZZ_JAR) == 1);
    p.until(30.0, "the client's bar shows the fizz jar", |p| bar_slot(&p.client, FIZZ_JAR).is_some());
    let host_jars = p.host.world().resource::<Items>().host.inventories[&2].count("jar");
    let client_jars = {
        let v = &p.client.world().resource::<Items>().client.view;
        (0..8).filter_map(|i| v.slot(i)).filter(|s| s.item.kind.as_str() == "jar").map(|s| s.count).sum::<u32>()
    };
    assert_eq!((host_jars, client_jars), (1, 1), "one empty jar was used up, on both");
    assert_eq!(p.host.world().resource::<PocketHost>().filled, 1);
    p.until(30.0, "the pocket news agree: one jar less in the pocket", |p| {
        let (h, c) = (p.host.world().resource::<Items>().dig.pockets.clone(), p.client.world().resource::<Items>().dig.pockets.clone());
        h.is_some() && h == c && h.unwrap().pockets[0].jars == (jars - 1) as u8
    });
}
