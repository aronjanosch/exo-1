//! #30: the menu's Host and Join start a session; two players find each other on loopback.
//! Headless, no window: the menu's buttons call `start_session`, which is what this drives.
use exo_app::menu::{start_session, Session};
use exo_app::net_live::RemoteWalker;
use exo_app::walker::Player;
use exo_app::{build_app, Options};

#[test]
fn host_and_join_through_the_menu_path_see_each_other() {
    let port = 18000 + (std::process::id() % 1000) as u16;
    let o = Options { headless: true, ..Default::default() };
    let mut host = build_app(&o);
    let mut client = build_app(&o);
    for app in [&mut host, &mut client] {
        app.finish();
        app.cleanup();
        app.update();
    }
    start_session(host.world_mut(), &Session::Host { port }).expect("host");
    assert!(start_session(client.world_mut(), &Session::Join { address: "not an address".into(), slot: 2 }).is_err(), "a bad address is an error, not a panic");
    start_session(client.world_mut(), &Session::Join { address: format!("127.0.0.1:{port}"), slot: 2 }).expect("join");
    // The joining player starts at slot 2's spawn, 20 m from slot 1's.
    let pos = |app: &mut bevy::app::App| app.world_mut().query::<&Player>().single(app.world()).unwrap().w.pos;
    let (h, c) = (pos(&mut host), pos(&mut client));
    assert!((h.distance(c) - 20.0).abs() < 1.0, "slot spawns {:.2} m apart", h.distance(c));
    let t0 = std::time::Instant::now();
    let sees = |app: &mut bevy::app::App| app.world_mut().query::<&RemoteWalker>().iter(app.world()).count();
    while (sees(&mut host) == 0 || sees(&mut client) == 0) && t0.elapsed().as_secs() < 20 {
        host.update();
        client.update();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!((sees(&mut host), sees(&mut client)), (1, 1), "each sees the other's walker");
}
