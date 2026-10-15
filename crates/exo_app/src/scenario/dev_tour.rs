//! A picture tour of the dev area for the initiator's look (window run with `--out=<dir>`): the
//! walker's start, each room of the mini ship, the area from above, each role zone and the pad. It
//! checks only that the walker starts inside the ship; the pictures are for the eye.
use super::*;
use crate::dev_area::layout::{self, Place, AIRLOCK, BRIDGE, CORRIDOR, GUNNERY_AT, HELM_AT, LAB_ROOM, MEDBAY_ROOM, REACTOR_AT, ROOM, TURRET_ROOM};
use crate::dev_area::{area_of, DevArea, PADS, ROLE_ZONES};
use bevy::math::DVec3;

/// A point at area metres, `height` above the ground.
fn at(w: &mut World, p: [f64; 2], height: f64) -> DVec3 {
    let pl = planet(w);
    w.resource::<DevArea>().ground(&pl, p, height).expect("the dev area is there").0
}

/// Puts the walker on foot at `stand`, looking at `look` (area metres, 1.2 m above the ground).
fn stand(stand: [f64; 2], look: [f64; 2]) -> Step {
    Box::new(move |w, _| {
        with_player(w, |p| {
            p.fly = false;
            p.noclip = false;
        });
        let feet = at(w, stand, 0.0);
        place_walker(w, feet);
        let target = at(w, look, 1.2);
        face_towards(w, target);
        true
    })
}

/// Hovers the walker `height` m above `over`, looking at `look` on the ground.
fn hover(over: [f64; 2], height: f64, look: [f64; 2]) -> Step {
    Box::new(move |w, _| {
        let eye = at(w, over, height);
        let target = at(w, look, 0.0);
        with_player(w, |p| {
            p.ship = None;
            p.fly = true;
            p.noclip = true;
            p.w.pos = eye;
            p.w.halt();
        });
        face_towards(w, target);
        let pl = planet(w);
        let up = pl.up(eye);
        let d = target - eye;
        let v = d.dot(up);
        let h = (d - up * v).length();
        with_player(w, |p| p.pitch = v.atan2(h));
        true
    })
}

pub(super) fn dev_tour_steps(s: &mut Vec<Step>, shot_step: &dyn Fn(&'static str) -> Step) {
    s.push(Box::new(|w, c| {
        begin(w, c, "dev tour: the walker starts in the mini ship");
        let pl = planet(w);
        let f = w.resource::<DevArea>().frame.expect("dev area frame");
        let p = player_world(w);
        let xy = area_of(&f, &pl, p);
        check(c, layout::place_of(xy) != Place::Outside, format!("dev tour: the walker starts at {:.1} m east, {:.1} m north, in {:?}", xy[0], xy[1], layout::place_of(xy)));
        end(w, c, "start".into());
        true
    }));
    s.push(Box::new(|w, _| {
        let target = at(w, [CORRIDOR.centre[0] + CORRIDOR.half[0], CORRIDOR.centre[1]], 1.2);
        face_towards(w, target);
        true
    }));
    s.push(wait(3.0));
    s.push(shot_step("tour-01-start"));
    s.push(wait(3.0));
    let rooms: [(&'static str, [f64; 2], [f64; 2]); 6] = [
        ("tour-02-engine-room", [ROOM.centre[0] + 2.0, ROOM.centre[1] + 3.0], REACTOR_AT),
        ("tour-03-bridge", [BRIDGE.centre[0] + 2.5, BRIDGE.centre[1] - 2.0], HELM_AT),
        ("tour-04-turret-room", [TURRET_ROOM.centre[0], TURRET_ROOM.centre[1] - 2.2], GUNNERY_AT),
        ("tour-05-lab", [LAB_ROOM.centre[0], LAB_ROOM.centre[1] - 2.2], [LAB_ROOM.centre[0], LAB_ROOM.centre[1] + 3.0]),
        ("tour-06-medbay", [MEDBAY_ROOM.centre[0], MEDBAY_ROOM.centre[1] + 2.4], [MEDBAY_ROOM.centre[0], MEDBAY_ROOM.centre[1] - 3.0]),
        ("tour-07-airlock", [1.5, CORRIDOR.centre[1]], [AIRLOCK.centre[0] + 3.0, AIRLOCK.centre[1]]),
    ];
    for (tag, from, look) in rooms {
        s.push(stand(from, look));
        s.push(wait(3.0));
        s.push(shot_step(tag));
        s.push(wait(3.0));
    }
    s.push(hover([0.0, -30.0], 24.0, [0.0, 0.0]));
    s.push(wait(3.0));
    s.push(shot_step("tour-08-overview"));
    s.push(wait(3.0));
    s.push(hover([-7.0, -2.0], 22.0, [-7.0, -12.0]));
    s.push(wait(3.0));
    s.push(shot_step("tour-09-ship-from-above"));
    s.push(wait(3.0));
    s.push(hover([0.0, -4.0], 16.0, [0.0, 11.0]));
    s.push(wait(3.0));
    s.push(shot_step("tour-10-zones-from-above"));
    s.push(wait(3.0));
    const ZONE_TAGS: [&str; 8] = ["tour-11-zone-engineering", "tour-12-zone-bridge", "tour-13-zone-turrets", "tour-14-zone-lab", "tour-15-zone-medbay", "tour-16-zone-storage", "tour-17-zone-hazards", "tour-18-zone-dev"];
    for (z, tag) in ROLE_ZONES.iter().zip(ZONE_TAGS) {
        s.push(stand([z.centre[0], z.centre[1] - z.half[1] - 1.5], z.centre));
        s.push(wait(3.0));
        s.push(shot_step(tag));
        s.push(wait(3.0));
    }
    s.push(stand([PADS.centre[0] - 9.0, PADS.centre[1] + 2.0], PADS.centre));
    s.push(wait(3.0));
    s.push(shot_step("tour-19-pad"));
    s.push(wait(3.0));
}
