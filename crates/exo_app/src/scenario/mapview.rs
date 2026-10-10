//! #166, the map: the pin list from the game's state (places, the tracked job's next stop, the
//! player), M opens a north-up picture of the ground and closes it again, and with a window the
//! picture is shot (`<out>/shot-*-map.png`). Headless it checks the pins and the picture's data.
use super::deliver::{gp, goods_of, show_offer, walker_on_pad};
use super::*;
use crate::map::{current_pins, pin_px, MapView, MAP_PX};
use jobs_core::map::{PinKind, Stop};

fn pins(w: &mut World) -> Vec<jobs_core::map::Pin> {
    let pos = player_world(w);
    let planet = planet(w);
    current_pins(gp(w), &planet, Some(pos))
}

fn target(w: &mut World) -> Option<(Stop, String)> {
    let p = pins(w);
    p.into_iter().find_map(|p| if let PinKind::Target(s) = p.kind { Some((s, gp(w).text(&p.label))) } else { None })
}

pub fn map_steps(s: &mut Vec<Step>, out_dir: &std::path::Path, windowed: bool) {
    let dir = out_dir.to_path_buf();
    s.push(settle());
    s.push(Box::new(|w, c| {
        crate::scenario::cargo::clear_crates(w);
        begin(w, c, "map: pins without a job");
        walker_on_pad(w, "drip_rock");
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let p = pins(w);
        let places: Vec<String> = p.iter().filter(|p| p.kind == PinKind::Place).map(|p| gp(w).text(&p.label)).collect();
        check(c, places.len() == 7, format!("map: every place with a pad is pinned ({} places: {places:?})", places.len()));
        check(c, p.iter().filter(|p| p.kind == PinKind::Player { own: true }).count() == 1, "map: the player is pinned".into());
        check(c, target(w).is_none() && gp(w).tracked.is_none(), "map: no job, no target".into());
        // The picture shows 1 km around the player: Bent Spoon, 1.9 km away, is off it, the rest on it.
        let planet = planet(w);
        let pos = player_world(w);
        let centre = planet.up(pos);
        let off: Vec<String> = p.iter().filter(|p| pin_px(&planet, centre, p, 640.0).is_none()).map(|p| gp(w).text(&p.label)).collect();
        check(c, off == vec!["Bent Spoon".to_string()], format!("map: every pin but far Bent Spoon lies on the picture (off it: {off:?})"));
        end(w, c, String::new());
        true
    }));
    // Take a courier job, track it.
    s.push(Box::new(|w, c| {
        begin(w, c, "map: the tracked job's pin");
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(0.5));
    s.push(show_offer("courier_lint_trap"));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let t = target(w);
        check(c, gp(w).tracked.is_some() && t == Some((Stop::Pickup, "Drip Rock".into())), format!("map: the accepted job is tracked, its pin at the pickup ({t:?})"));
        let e = goods_of(w)[0].0;
        let at = crate::scenario::cargo::crate_world_pos(w, e);
        crate::scenario::cargo::look_at(w, at);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, _| {
        tap(w, KeyCode::KeyF);
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        let t = target(w);
        check(c, t == Some((Stop::Dropoff, "Lint Trap".into())), format!("map: the crate carried off, the pin moves to the dropoff ({t:?})"));
        let line = gp(w).readout.clone();
        check(c, line.contains("deliver to: Lint Trap"), format!("map: the job line's pointer follows the same stop ('{}')", line.replace('\n', " | ")));
        // T with one active job keeps it.
        tap(w, KeyCode::KeyT);
        true
    }));
    s.push(wait(0.3));
    s.push(Box::new(|w, c| {
        check(c, gp(w).tracked.is_some(), "map: T keeps the one job tracked".into());
        end(w, c, String::new());
        true
    }));
    // M opens the map, a picture of the ground around the player is made, M closes it.
    s.push(Box::new(|w, c| {
        begin(w, c, "map: M opens and closes it");
        check(c, !w.resource::<MapView>().open, "map: closed at first".into());
        tap(w, KeyCode::KeyM);
        true
    }));
    // The picture is made on a worker thread: wait for it.
    s.push(Box::new(|w, c| {
        let done = !w.resource::<MapView>().rgb.is_empty();
        assert!(done || c.t < 20.0, "the map picture never came");
        done
    }));
    s.push(Box::new(move |w, c| {
        let (open, len, ms) = {
            let m = w.resource::<MapView>();
            (m.open, m.rgb.len(), m.made_ms)
        };
        check(c, open && len == MAP_PX * MAP_PX * 3, format!("map: M opens it and the picture is made ({} px across, {ms:.0} ms)", MAP_PX));
        // The picture is not one flat colour: land, water or shading differ.
        let m = w.resource::<MapView>();
        let distinct = m.rgb.chunks(3).map(|p| (p[0] / 16, p[1] / 16, p[2] / 16)).collect::<std::collections::BTreeSet<_>>().len();
        check(c, distinct > 4, format!("map: the picture shows the ground ({distinct} distinct tones)"));
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(move |w, c| {
        shot(w, c, &dir, windowed, "map");
        true
    }));
    s.push(wait(1.5));
    s.push(Box::new(|w, c| {
        tap(w, KeyCode::KeyM);
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, c| {
        check(c, !w.resource::<MapView>().open, "map: M closes it again".into());
        end(w, c, String::new());
        true
    }));
}
