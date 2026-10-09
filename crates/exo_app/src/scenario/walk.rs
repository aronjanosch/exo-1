//! Long walks over the planet (spike 8 T5).
use super::*;

/// Long walks (spike 8 T5): 1.8 m/s for 300 s from four starts; steep slopes may stop the walker.
pub(super) fn t5_starts() -> Vec<(&'static str, DVec3, DVec3)> {
    // The stamps are placed by the planet's budget (#69): starts come from its look spots.
    let sys = warp_core::System::from_json(crate::warp::SYSTEM).expect("system.json");
    let home = PlanetRes::load(PlanetId(0), sys.planet(PlanetId(0)));
    let pg = &home.pgen;
    let r = home.radius;
    let back = |s: planet_core::Spot, m: f64| crate::env::from_v3(planet_core::look::walk(s.dir, s.facing, m, r));
    let mut out = vec![("spawn, heading east", DVec3::Y, DVec3::X)];
    if let Some(s) = pg.spot("basin") {
        out.push(("basin shore, heading to the centre", crate::env::from_v3(s.dir), crate::env::from_v3(s.facing)));
    }
    // The rim spot stands on top facing down: start 300 m below it, heading up the step.
    if let Some(s) = pg.spot("rim") {
        let start = back(s, 300.0);
        out.push(("escarpment foot, heading up the step", start, (crate::env::from_v3(s.dir) - start).normalize()));
    }
    // The plateau spot is near its edge facing out: start 400 m outside, heading in.
    if let Some(s) = pg.spot("plateau") {
        let start = back(s, 400.0);
        out.push(("plateau approach, heading to the centre", start, (crate::env::from_v3(s.dir) - start).normalize()));
    }
    out
}

pub(super) fn t5_walk(name: &'static str, dir: DVec3, heading: DVec3, secs: f64) -> Vec<Step> {
    vec![
        Box::new(move |w, _| {
            let pl = planet(w);
            place_walker(w, pl.centre + dir * pl.surface(dir));
            with_player(w, |p| {
                p.w.forward = heading;
                p.w.cfg.walk_speed = 1.8;
            });
            w.resource_mut::<Ring>().force_update();
            true
        }),
        settle(),
        Box::new(move |w, c| {
            if c.t == 0.0 {
                begin(w, c, name);
                c.p.insert("last", player_world(w));
                c.v.insert("path", 0.0);
                c.v.insert("hmin", f64::MAX);
                c.v.insert("hmax", f64::MIN);
                keys(w, &[KeyCode::KeyW], true);
            }
            let p = player_world(w);
            *c.v.get_mut("path").unwrap() += p.distance(c.p["last"]);
            c.p.insert("last", p);
            let pl = planet(w);
            let h = (p - pl.centre).length() - pl.radius - pl.sea;
            *c.v.get_mut("hmin").unwrap() = c.v["hmin"].min(h);
            *c.v.get_mut("hmax").unwrap() = c.v["hmax"].max(h);
            let here = pl.pgen.sample(crate::env::to_v3(pl.up(p)));
            let slope = here.slope_deg;
            // Biome rows walked through (#68), as a bit set.
            let seen = c.v.entry("biomes").or_insert(0.0);
            *seen = (*seen as u64 | 1u64 << here.biome.clamp(0, 63)) as f64;
            let e = c.v.entry("slope").or_insert(0.0);
            *e = e.max(slope);
            let climb = c.v.entry("climb").or_insert(0.0);
            if slope > 50.0 { *climb += 1.0; }
            if c.t >= secs {
                keys(w, &[KeyCode::KeyW], false);
                let rows = (c.v["biomes"] as u64).count_ones();
                let note = format!("path {:.0} m, height above sea {:+.1}..{:+.1} m, steepest ground under the walker {:.1} deg, ticks on ground steeper than 50 deg {}, biome rows {rows}", c.v["path"], c.v["hmin"], c.v["hmax"], c.v["slope"], c.v["climb"]);
                c.v.remove("slope");
                c.v.remove("climb");
                c.v.remove("biomes");
                end(w, c, note);
                check(c, w.resource::<WalkStats>().rescues == c.rescues0, format!("{name}: no fall-through"));
                check(c, rows >= 2, format!("{name}: crosses {rows} biome rows (at least 2)"));
                return true;
            }
            false
        }),
    ]
}
