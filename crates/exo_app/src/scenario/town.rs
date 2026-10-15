//! The town, headless: buildings' colliders, characters, and the new towns' pads.
use super::jobs_helpers::pad;
use super::*;
use crate::town::Town;

fn tower(w: &World) -> (DVec3, DVec3) {
    let town = w.resource::<Town>();
    let plan = town.plan.as_ref().expect("plan");
    let frame = town.frame.expect("city frame");
    let p = plan.placements.iter().find(|p| p.model == "company_hq").expect("the company tower is in the plan");
    frame.ground(&planet(w), p.at[0], p.at[1], 0.0)
}

pub fn town_steps(s: &mut Vec<Step>) {
    s.push(settle());
    s.push(Box::new(|w, c| {
        begin(w, c, "town: the city on its place");
        let town = w.resource::<Town>();
        let framed = town.frame.is_some();
        check(c, framed, "town: the place 'city' has a frame on the planet".to_string());
        let pl = planet(w);
        let (a, b) = (pad(w, "drip_rock").centre, pad(w, "bent_spoon").centre);
        c.v.insert("bent_km", (a - b).length() / 1000.0);
        let (centre, up) = tower(w);
        let east = up.any_orthonormal_vector();
        let start = centre - up.cross(east).normalize() * 22.0;
        let dir = pl.up(start);
        place_walker(w, pl.centre + dir * (pl.surface(dir) + 1.0));
        face_towards(w, centre);
        c.v.insert("start_d", (start - centre).length());
        true
    }));
    s.push(wait(0.5));
    s.push(Box::new(|w, _| {
        keys(w, &[KeyCode::KeyW], true);
        true
    }));
    s.push(wait(8.0));
    s.push(Box::new(|w, c| {
        keys(w, &[KeyCode::KeyW], false);
        let (centre, up) = tower(w);
        let p = player_world(w);
        let flat = (p - centre) - up * (p - centre).dot(up);
        let d = flat.length();
        // The tower's box is 20.4 m wide and 17.6 m deep: a walker outside it is at least 8 m from the centre.
        check(c, d > 8.0, format!("town: walking 8 s at the company tower stops at its wall ({d:.1} m from its centre, started {:.1} m away)", c.v["start_d"]));
        end(w, c, "city stands, walls hold".into());
        true
    }));
}
