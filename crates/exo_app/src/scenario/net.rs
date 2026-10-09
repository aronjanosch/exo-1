//! Network scenarios: the foreign ship driven through the snapshot path, the net bot's flight, and a
//! remote proxy at warp speed beside the walker.
use super::*;

/// `foreign` scenario: a remote ship (kinematic proxy) is driven through the real
/// snapshot path (encode, decode, buffer, 150 ms playout) along a known path, so the walker in its
/// cabin can be checked against exact truth.
#[derive(Resource)]
pub struct ForeignDriver {
    pub proxy: Entity,
    pub buf: Buffer,
    pub tick: u64,
    /// Planet-relative start of the flight; the ship moves along -z at 350 m/s and yaws 0.003 rad/tick.
    pub p0: DVec3,
    /// When set the ship stands still at this pose (walker beside a parked foreign ship).
    pub parked: Option<(DVec3, bevy::math::DQuat)>,
    pub max_pos_err: f64,
    pub max_rot_err_deg: f64,
    /// #16: the received velocity is replaced by this one (a ship at warp speed held in place).
    pub hold_vel: Option<DVec3>,
    /// #16: the ship warps along its nose (wins over `parked`).
    pub warp: Option<ForeignWarp>,
}

/// A remote ship's warp for the `foreign` scenario: still at `p0` until `t0`, then the drive's
/// two acceleration stages up to its top speed, then cruise (`content/system/system.json`).
#[derive(Clone, Copy)]
pub struct ForeignWarp {
    pub t0: f64,
    pub p0: DVec3,
    pub q: DQuat,
    pub accel_one: f64,
    pub switch_speed: f64,
    pub accel_two: f64,
    pub top_speed: f64,
}

impl ForeignWarp {
    /// Distance along the nose and speed at time `t`.
    fn at(&self, t: f64) -> (f64, f64) {
        let tau = (t - self.t0).max(0.0);
        let (a1, vs, a2, vt) = (self.accel_one, self.switch_speed, self.accel_two, self.top_speed);
        let (t1, t2) = (vs / a1, (vt - vs) / a2);
        let (s1, s2) = (0.5 * a1 * t1 * t1, vs * t2 + 0.5 * a2 * t2 * t2);
        if tau < t1 {
            (0.5 * a1 * tau * tau, a1 * tau)
        } else if tau < t1 + t2 {
            let u = tau - t1;
            (s1 + vs * u + 0.5 * a2 * u * u, vs + a2 * u)
        } else {
            (s1 + s2 + vt * (tau - t1 - t2), vt)
        }
    }

    /// Seconds from `t0` to the top speed.
    fn ramp_time(&self) -> f64 {
        self.switch_speed / self.accel_one + (self.top_speed - self.switch_speed) / self.accel_two
    }
}

const FOREIGN_SPEED: f64 = 350.0;
const FOREIGN_YAW_RATE: f64 = 0.003 * 60.0;

fn foreign_truth(d: &ForeignDriver, t: f64) -> (DVec3, bevy::math::DQuat, DVec3) {
    if let Some(wp) = d.warp {
        let nose = wp.q * DVec3::NEG_Z;
        let (s, v) = wp.at(t);
        return (wp.p0 + nose * s, wp.q, nose * v);
    }
    if let Some((p, q)) = d.parked {
        return (p, q, DVec3::ZERO);
    }
    (d.p0 + DVec3::new(0.0, 0.0, -FOREIGN_SPEED * t), bevy::math::DQuat::from_rotation_y(FOREIGN_YAW_RATE * t), DVec3::new(0.0, 0.0, -FOREIGN_SPEED))
}

pub fn foreign_drive(
    mut d: ResMut<ForeignDriver>,
    mut origin: ResMut<RenderOrigin>,
    mut q: Query<(&mut Position, &mut Rotation, &mut LinearVelocity, &mut AngularVelocity, &mut RemoteShip)>,
) {
    d.tick += 1;
    let t = d.tick as f64 * DT;
    if d.tick % 2 == 0 {
        let (p, qn, v) = foreign_truth(&d, t);
        let mut s = Snapshot::new(2, t, p, v, qn);
        s.seq = d.tick as u32;
        // Parked means landed: its cabin gravity is off.
        s.lag = if d.parked.is_some() { 0.0 } else { 1.0 };
        if d.warp.is_some() {
            // The owner sits in its own ship, as a real pilot sends it (the default walker pose is
            // planet-relative and would be out of the walker range at warp speed).
            s.frame = net_core::snapshot::FrameKind::Ship;
            s.frame_id = 2;
            s.wp = SEAT_POS;
            s.wv = DVec3::ZERO;
        }
        // Through the wire format, like a received packet.
        let s = Snapshot::decode(&s.encode()).expect("own snapshot decodes");
        d.buf.push(s);
    }
    let target = t - 0.15;
    let Some(sample) = d.buf.sample(target) else { return };
    let Ok((mut p, mut r, mut v, mut w, mut rs)) = q.get_mut(d.proxy) else { return };
    rs.lag = sample.s.lag;
    p.0 = sample.s.p;
    r.0 = sample.s.q;
    v.0 = d.hold_vel.unwrap_or(sample.s.v);
    w.0 = if d.parked.is_some() || d.warp.is_some() { DVec3::ZERO } else { DVec3::new(0.0, FOREIGN_YAW_RATE, 0.0) };
    origin.view = sample.s.p;
    if sample.mode == net_core::buffer::Mode::Interpolate && target > 0.5 {
        let (tp, tq, _) = foreign_truth(&d, target);
        d.max_pos_err = d.max_pos_err.max(sample.s.p.distance(tp));
        d.max_rot_err_deg = d.max_rot_err_deg.max(sample.s.q.angle_between(tq).to_degrees());
    }
}

/// One flight of the network bot: take off, cruise, turn, brake, descend, land, idle.
pub(super) fn net_cycle() -> Vec<Step> {
    vec![
        hold_until("takeoff", &[KeyCode::Space, KeyCode::ShiftLeft], 40.0, |w| above_ground(w) > 80.0),
        hold_until("cruise", &[KeyCode::KeyW, KeyCode::ShiftLeft], 8.0, |_| false),
        Box::new(|w, c| {
            if c.t == 0.0 {
                begin(w, c, "turn");
                keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], true);
            }
            // About 0.5 rad/s of yaw (the controller turns 0.002 rad per pixel).
            w.resource_mut::<Controls>().mouse.x += 4.2;
            if c.t >= 4.0 {
                keys(w, &[KeyCode::KeyW, KeyCode::ShiftLeft], false);
                end(w, c, "turned".into());
                return true;
            }
            false
        }),
        hold_until("brake", &[KeyCode::KeyX], 15.0, |w| ship_vel(w).length() < 1.0),
        hold_until("descend", &[KeyCode::ControlLeft, KeyCode::ShiftLeft], 90.0, |w| above_ground(w) < 25.0),
        hold_until("land", &[KeyCode::ControlLeft], 60.0, {
            let mut t = 0.0;
            move |w| {
                t += 1.0 / 60.0;
                t > 3.0 && ship_vel(w).length() < 0.05
            }
        }),
        wait(3.0),
    ]
}

pub(super) fn foreign_steps(s: &mut Vec<Step>) {
    // 1. A proxy ship 20 km above the planet, flying through the snapshot path.
    s.push(Box::new(|w, _| {
        let pl = planet(w);
        let p0 = DVec3::new(0.0, pl.radius + 20_000.0, 0.0);
        let mut commands = w.commands();
        let proxy = crate::net_live::spawn_proxy(&mut commands, 2, p0, bevy::math::DQuat::IDENTITY);
        w.flush();
        w.insert_resource(ForeignDriver { proxy, buf: Buffer::new(), tick: 0, p0, parked: None, max_pos_err: 0.0, max_rot_err_deg: 0.0, hold_vel: None, warp: None });
        true
    }));
    s.push(wait(1.0));
    // 2. Put the walker into the foreign cabin (test setup, not a boarding mechanic).
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "board the foreign ship (350 m/s, yawing)");
            let proxy = w.resource::<ForeignDriver>().proxy;
            with_player(w, |p| {
                p.ship = Some(proxy);
                p.w.pos = DVec3::new(0.0, 0.31, 1.0);
                p.w.halt();
                p.fly = false;
            });
            return false;
        }
        if c.t < 0.5 {
            return false;
        }
        let proxy = w.resource::<ForeignDriver>().proxy;
        let inside = with_player(w, |p| p.ship == Some(proxy));
        check(c, inside, "walker is in the cabin of the remote ship".into());
        true
    }));
    // 3. Stand 6 s (360 ticks) at 350 m/s: drift, deck contact, height.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "stand in the foreign cabin, 6 s");
            let p = with_player(w, |p| p.w.pos);
            c.p.insert("start", p);
            c.v.insert("drift", 0.0);
            c.v.insert("ymin", f64::MAX);
            c.v.insert("ticks", 0.0);
            c.v.insert("floor", 0.0);
            c.v.insert("left", 0.0);
            c.v.insert("shifts0", w.resource::<RenderOrigin>().shifts as f64);
        }
        let proxy = w.resource::<ForeignDriver>().proxy;
        let (pos, grounded, inside) = with_player(w, |p| (p.w.pos, p.w.grounded, p.ship == Some(proxy)));
        let start = c.p["start"];
        *c.v.get_mut("drift").unwrap() = c.v["drift"].max(((pos.x - start.x).powi(2) + (pos.z - start.z).powi(2)).sqrt());
        *c.v.get_mut("ymin").unwrap() = c.v["ymin"].min(pos.y);
        *c.v.get_mut("ticks").unwrap() += 1.0;
        *c.v.get_mut("floor").unwrap() += grounded as u32 as f64;
        if !inside {
            c.v.insert("left", 1.0);
        }
        if c.v["ticks"] >= 360.0 {
            let d = w.resource::<ForeignDriver>();
            let (pe, re) = (d.max_pos_err, d.max_rot_err_deg);
            let shifts = w.resource::<RenderOrigin>().shifts as f64 - c.v["shifts0"];
            let note = format!(
                "lateral standing drift {:.4} m, deck contact {}/360 ticks, lowest feet {:.3} m, left cabin {}, {shifts:.0} render-origin shifts, proxy vs exact path: max {:.4} mm, {:.4} deg",
                c.v["drift"], c.v["floor"], c.v["ymin"], c.v["left"] > 0.0, pe * 1000.0, re
            );
            end(w, c, note);
            check(c, c.v["drift"] < 0.05 && c.v["ymin"] > 0.28 && c.v["floor"] > 300.0 && c.v["left"] == 0.0,
                format!("walker on interpolated 350 m/s foreign ship: drift {:.4} m, deck contact {}/360", c.v["drift"], c.v["floor"]));
            return true;
        }
        false
    }));
    // 3b. The snapshot of the passenger names the OWNER OF THE FOREIGN SHIP as its frame (it used
    //     to say "my own ship", so the pilot composed it into the wrong ship and it vanished).
    s.push(Box::new(|w, c| {
        let pl = planet(w);
        let proxy = w.resource::<ForeignDriver>().proxy;
        let owner = w.get::<RemoteShip>(proxy).unwrap().owner;
        let ship = w.query_filtered::<(&Position, &Rotation, &LinearVelocity), With<Ship>>().single(w).map(|(a, b, c)| (*a, *b, *c)).unwrap();
        let mut q = w.query::<&Player>();
        let player = q.single(w).unwrap();
        let s = crate::net::build_snapshot(1, owner, 0, 2.0, 5, &pl, (&ship.0, &ship.1, &ship.2), player);
        let dec = Snapshot::decode(&s.encode()).unwrap();
        check(c, dec.frame == net_core::snapshot::FrameKind::Ship && dec.frame_id == 2 && dec.wp.distance(player.w.pos) < 1e-6,
            "passenger snapshot names the foreign ship's owner and the local pose".into());
        true
    }));
    // 4. Walk sideways in the local frame for 12 ticks (about 1 m).
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "walk inside the foreign cabin");
            c.p.insert("start", with_player(w, |p| p.w.pos));
            c.v.insert("n", 0.0);
        }
        keys(w, &[KeyCode::KeyD], true);
        *c.v.get_mut("n").unwrap() += 1.0;
        if c.v["n"] >= 12.0 {
            keys(w, &[KeyCode::KeyD], false);
            let dx = with_player(w, |p| p.w.pos.x) - c.p["start"].x;
            end(w, c, format!("moved {dx:.3} m along the cabin x axis"));
            check(c, dx > 0.5 && dx < 1.5, "walking inside the foreign cabin uses the local frame".into());
            return true;
        }
        false
    }));
    // 5. Leave: back to the planet frame at the world pose, and park the remote ship on the ground
    //    next to the spawn.
    s.push(Box::new(|w, c| {
        begin(w, c, "foreign ship parked on the ground, walker beside it");
        let pl = planet(w);
        let dir = (DVec3::Y * pl.radius + DVec3::new(30.0, 0.0, 0.0)).normalize();
        let rot = crate::ship::basis_for_up(dir);
        let mut ground = f64::MIN;
        for cc in [DVec3::ZERO, DVec3::new(2., 0., 4.), DVec3::new(-2., 0., 4.), DVec3::new(2., 0., -4.), DVec3::new(-2., 0., -4.)] {
            let d = (dir * pl.radius + rot * cc).normalize();
            ground = ground.max(pl.surface(d) - pl.radius);
        }
        let pos = pl.centre + dir * (pl.radius + ground + 0.05);
        w.resource_mut::<ForeignDriver>().parked = Some((pos, rot));
        w.resource_mut::<RenderOrigin>().origin = pos.round();
        place_walker(w, pos + rot * DVec3::new(7.0, 0.0, 0.0));
        c.v.insert("n", 0.0);
        c.v.insert("floor", 0.0);
        true
    }));
    s.push(wait(1.5));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            c.p.insert("start", with_player(w, |p| p.w.pos));
            c.v.insert("n", 0.0);
            c.v.insert("floor", 0.0);
        }
        keys(w, &[KeyCode::KeyD], true);
        let grounded = with_player(w, |p| p.w.grounded);
        *c.v.get_mut("n").unwrap() += 1.0;
        *c.v.get_mut("floor").unwrap() += grounded as u32 as f64;
        if c.v["n"] >= 24.0 {
            keys(w, &[KeyCode::KeyD], false);
            let moved = with_player(w, |p| p.w.pos).distance(c.p["start"]);
            end(w, c, format!("moved {moved:.2} m, grounded {}/24 ticks", c.v["floor"]));
            check(c, moved > 1.0 && c.v["floor"] > 15.0, format!("planet-frame walker moves beside the parked foreign ship; floor {}/24", c.v["floor"]));
            // The wire round trip of that pose.
            let pl = planet(w);
            let snap = with_player(w, |p| (p.w.pos, p.w.forward, p.ship));
            let ship = w.query_filtered::<(&Position, &Rotation, &LinearVelocity), With<Ship>>().single(w).map(|(a, b, c)| (*a, *b, *c)).unwrap();
            let mut q = w.query::<&Player>();
            let player = q.single(w).unwrap();
            let s = crate::net::build_snapshot(1, 1, 0, 2.0, 5, &pl, (&ship.0, &ship.1, &ship.2), player);
            let dec = Snapshot::decode(&s.encode()).unwrap();
            check(c, dec.frame == net_core::snapshot::FrameKind::Planet && dec.wp.distance(snap.0 - pl.centre) < 1e-6 && snap.2.is_none(),
                "outside walker snapshot uses the shared planet frame".into());
            return true;
        }
        false
    }));
    // 6. Issue #11: the parked (landed) foreign ship stands 8 deg tilted and sends its cabin
    //    gravity off; a walker in its cabin stands about the planet's up, not the tilted floor's.
    s.push(Box::new(|w, _| {
        let mut d = w.resource_mut::<ForeignDriver>();
        let (pos, rot) = d.parked.unwrap();
        d.parked = Some((pos + rot * DVec3::new(0.0, 0.6, 0.0), rot * bevy::math::DQuat::from_rotation_x(8f64.to_radians())));
        true
    }));
    s.push(wait(1.0));
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "in the cabin of the tilted, landed foreign ship");
            let proxy = w.resource::<ForeignDriver>().proxy;
            let (p, r) = (w.get::<Position>(proxy).unwrap().0, w.get::<Rotation>(proxy).unwrap().0);
            let f = Frame { origin: p, rot: r };
            with_player(w, |pl| {
                pl.ship = Some(proxy);
                pl.w.pos = DVec3::new(0.0, 0.4, -1.0);
                pl.w.halt();
                pl.w.forward = DVec3::NEG_Z;
                pl.cabin_up = DVec3::Y;
                pl.view_up = f.rot * DVec3::Y;
            });
        }
        if c.t >= 1.0 {
            let proxy = w.resource::<ForeignDriver>().proxy;
            let (p, r) = (w.get::<Position>(proxy).unwrap().0, w.get::<Rotation>(proxy).unwrap().0);
            let lag = w.get::<RemoteShip>(proxy).unwrap().lag;
            let pl = planet(w);
            let up = with_player(w, |pl| r * pl.cabin_up);
            let off = up.angle_between(pl.up(p)).to_degrees();
            let tilt = (r * DVec3::Y).angle_between(pl.up(p)).to_degrees();
            end(w, c, format!("received cabin gravity {:.0} %, up {off:.3} deg from the planet's, ship tilt {tilt:.1} deg", lag * 100.0));
            check(c, lag == 0.0 && off < 0.5 && tilt > 7.0, format!("landed foreign ship: its cabin gravity is off over the wire, walker stands about the planet's up ({off:.3} deg)"));
            return true;
        }
        false
    }));
    foreign_warp_extra_steps(s);
}

/// The same speed as `foreign_warp`: the drive's top speed (1e6 m/s).
const FOREIGN_WARP_SPEED: f64 = 1.0e6;

/// #16 without a network: a remote ship at warp speed next to the walking walker, then warping
/// with the walker in its cabin. Through the real snapshot path like the rest of `foreign`.
fn foreign_warp_extra_steps(s: &mut Vec<Step>) {
    // 7. The landed ship carries 1e6 m/s along its nose while it stands (a 17 km swept AABB and
    //    that relative velocity in the walker's moving-collider sweeps); the walker walks away
    //    from it outside.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "walk 5 s beside the landed foreign ship carrying 1e6 m/s");
            let proxy = w.resource::<ForeignDriver>().proxy;
            let (pp, pr) = (w.get::<Position>(proxy).unwrap().0, w.get::<Rotation>(proxy).unwrap().0);
            w.resource_mut::<ForeignDriver>().hold_vel = Some(pr * DVec3::NEG_Z * FOREIGN_WARP_SPEED);
            place_walker(w, pp + pr * DVec3::new(7.0, 0.0, 0.0));
            c.v.insert("ready", 0.0);
            return false;
        }
        // Land first (placed 0.1 m above the ground).
        if c.v["ready"] == 0.0 {
            if c.t < 1.0 {
                return false;
            }
            let proxy = w.resource::<ForeignDriver>().proxy;
            let pp = w.get::<Position>(proxy).unwrap().0;
            let p = player_world(w);
            face_towards(w, p + (p - pp));
            c.v.insert("ready", c.t);
            c.p.insert("start", p);
            c.p.insert("last", p);
            c.v.insert("depen0", w.resource::<WalkStats>().depenetrations as f64);
            c.v.insert("g", 0.0);
            c.v.insert("n", 0.0);
            c.v.insert("jump", 0.0);
            keys(w, &[KeyCode::KeyW], true);
            return false;
        }
        let p = player_world(w);
        *c.v.get_mut("jump").unwrap() = c.v["jump"].max(p.distance(c.p["last"]));
        c.p.insert("last", p);
        *c.v.get_mut("g").unwrap() += with_player(w, |pl| pl.w.grounded) as u32 as f64;
        *c.v.get_mut("n").unwrap() += 1.0;
        if c.t - c.v["ready"] >= 5.0 {
            keys(w, &[KeyCode::KeyW], false);
            let d = p.distance(c.p["start"]);
            let ws = w.resource::<WalkStats>().clone();
            let (dep, resc) = (ws.depenetrations as f64 - c.v["depen0"], ws.rescues - c.rescues0);
            let (g, n, jump) = (c.v["g"], c.v["n"], c.v["jump"]);
            let proxy = w.resource::<ForeignDriver>().proxy;
            let pv = w.get::<LinearVelocity>(proxy).unwrap().0.length();
            end(w, c, format!("walked {d:.2} m, grounded {g}/{n} ticks, largest step {:.3} m, {dep} depenetrations, foreign ship at {:.0} km/s", jump, pv / 1000.0));
            // Walk speed 5 m/s for 5 s, less the step-off (as in `foreign_warp`).
            check(c, d > 22.0 && d < 26.0 && g / n > 0.95 && resc == 0 && jump < 0.2 && pv > 0.99 * FOREIGN_WARP_SPEED && p.is_finite(),
                format!("foreign ship at {:.0} km/s next to the walker: walked {d:.2} m in 5 s (free walk 24.5), grounded {:.1} %, {resc} rescues, {dep} depenetrations, largest step {jump:.3} m", pv / 1000.0, 100.0 * g / n));
            w.resource_mut::<ForeignDriver>().hold_vel = None;
            return true;
        }
        false
    }));
    // 8. The ship lifts to 3 km (no terrain on its course), the walker boards (test setup), the
    //    ship ramps up like the quantum drive to 1e6 m/s and cruises 3 s; at the end the walker
    //    walks sideways in the cabin.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "in the cabin of the foreign ship warping to 1e6 m/s");
            let pl = planet(w);
            let (pos, _) = w.resource::<ForeignDriver>().parked.unwrap();
            let up = pl.up(pos);
            let p0 = pl.centre + up * (pl.surface(up) + 3000.0);
            let q = crate::ship::basis_for_up(up);
            let cfg = w.resource::<SystemRes>().0.drive.clone();
            let mut d = w.resource_mut::<ForeignDriver>();
            let t0 = d.tick as f64 * DT + 1.5;
            // In flight: the snapshots send the cabin gravity on again.
            d.parked = None;
            d.warp = Some(ForeignWarp { t0, p0, q, accel_one: cfg.accel_stage_one, switch_speed: cfg.stage_switch_speed, accel_two: cfg.accel_stage_two, top_speed: FOREIGN_WARP_SPEED });
            for k in ["boarded", "n", "g", "out", "drift", "ymin", "jump", "vmax", "walk_n"] {
                c.v.remove(k);
            }
            return false;
        }
        let d = w.resource::<ForeignDriver>();
        let (proxy, wp, now) = (d.proxy, d.warp.unwrap(), d.tick as f64 * DT);
        // Board once the ship holds still at 3 km (after the 150 ms playout).
        if !c.v.contains_key("boarded") {
            if c.t < 1.0 {
                return false;
            }
            with_player(w, |p| {
                p.ship = Some(proxy);
                p.w.pos = DVec3::new(0.0, 0.4, -1.0);
                p.w.halt();
                p.w.forward = DVec3::NEG_Z;
                p.cabin_up = DVec3::Y;
            });
            c.v.insert("boarded", 1.0);
            return false;
        }
        if now < wp.t0 + 0.3 {
            // Settle on the deck while it stands.
            c.p.insert("start", with_player(w, |p| p.w.pos));
            c.p.insert("last", c.p["start"]);
            c.v.insert("depen0", w.resource::<WalkStats>().depenetrations as f64);
            c.v.insert("rescue0", w.resource::<WalkStats>().rescues as f64);
            for k in ["n", "g", "out", "drift", "jump", "vmax"] {
                c.v.insert(k, 0.0);
            }
            c.v.insert("ymin", f64::MAX);
            return false;
        }
        let (pos, grounded, inside) = with_player(w, |p| (p.w.pos, p.w.grounded, p.ship == Some(proxy)));
        let pv = w.get::<LinearVelocity>(proxy).unwrap().0.length();
        let walking = c.v.contains_key("walk_n");
        let start = c.p["start"];
        if !walking {
            *c.v.get_mut("drift").unwrap() = c.v["drift"].max(((pos.x - start.x).powi(2) + (pos.z - start.z).powi(2)).sqrt());
            *c.v.get_mut("ymin").unwrap() = c.v["ymin"].min(pos.y);
            *c.v.get_mut("jump").unwrap() = c.v["jump"].max(pos.distance(c.p["last"]));
        }
        c.p.insert("last", pos);
        *c.v.get_mut("vmax").unwrap() = c.v["vmax"].max(pv);
        *c.v.get_mut("n").unwrap() += 1.0;
        *c.v.get_mut("g").unwrap() += grounded as u32 as f64;
        *c.v.get_mut("out").unwrap() += !inside as u32 as f64;
        let cruise_end = wp.t0 + wp.ramp_time() + 3.0;
        if now >= cruise_end && !walking {
            c.v.insert("walk_n", 0.0);
            c.p.insert("walk_start", pos);
            keys(w, &[KeyCode::KeyD], true);
            return false;
        }
        if walking {
            *c.v.get_mut("walk_n").unwrap() += 1.0;
            if c.v["walk_n"] < 12.0 {
                return false;
            }
            keys(w, &[KeyCode::KeyD], false);
            let dx = pos.x - c.p["walk_start"].x;
            let ws = w.resource::<WalkStats>().clone();
            let dep = ws.depenetrations as f64 - c.v["depen0"];
            let resc = ws.rescues as f64 - c.v["rescue0"];
            let (n, g, out, drift, ymin, jump, vmax) = (c.v["n"], c.v["g"], c.v["out"], c.v["drift"], c.v["ymin"], c.v["jump"], c.v["vmax"]);
            let (dist, _) = wp.at(now);
            end(w, c, format!(
                "ship top speed {:.0} km/s, {:.0} km flown, ramp {:.1} s + 3 s cruise; deck contact {g}/{n} ticks, left cabin {out} ticks, standing drift {:.2} mm, lowest feet {ymin:.3} m, largest standing step {:.2} mm, {dep} depenetrations, {resc} rescues; walked {dx:.3} m sideways at top speed",
                vmax / 1000.0, dist / 1000.0, wp.ramp_time(), drift * 1000.0, jump * 1000.0));
            check(c, vmax > 0.99 * FOREIGN_WARP_SPEED && out == 0.0 && g / n > 0.99 && drift < 0.05 && ymin > 0.28 && jump < 0.01 && resc == 0.0 && dep == 0.0 && pos.is_finite(),
                format!("walker in the cabin of a foreign ship warping to {:.0} km/s: deck contact {:.1} %, drift {:.2} mm, largest step {:.2} mm, {dep} depenetrations, {resc} rescues", vmax / 1000.0, 100.0 * g / n, drift * 1000.0, jump * 1000.0));
            check(c, dx > 0.5 && dx < 1.5, format!("walking inside the foreign cabin at {:.0} km/s uses the local frame ({dx:.3} m)", vmax / 1000.0));
            return true;
        }
        false
    }));
}

/// #16: a remote proxy held 40 m beside the walker with the velocity of a ship in a warp
/// (1e6 m/s): its collider AABB grows to kilometres and the walker's moving-collider sweep sees
/// 16.7 km of relative motion per tick. The walk must be the same as without it.
pub(super) fn foreign_warp_steps(s: &mut Vec<Step>) {
    const WARP_SPEED: f64 = 1.0e6;
    s.push(Box::new(|w, _| {
        // Away from the own parked ship (15 m ahead of the spawn).
        let p = player_world(w);
        let own = ship_frame_of(w).origin;
        face_towards(w, p + (p - own));
        let pl = planet(w);
        let feet = player_world(w);
        let up = pl.up(feet);
        let (fwd, right) = with_player(w, |p| (p.w.forward, p.w.forward.cross(p.up)));
        let pos = feet + right * 40.0 + up * 3.0;
        let mut commands = w.commands();
        let proxy = crate::net_live::spawn_proxy(&mut commands, 2, pos, walker_core::look_rot(fwd, up));
        w.flush();
        w.insert_resource(WarpingProxy { proxy, pos, vel: fwd * WARP_SPEED });
        true
    }));
    s.push(Box::new(|w, c| {
        hold_proxy(w);
        if c.t == 0.0 {
            begin(w, c, "walk 5 s beside a remote ship at 1e6 m/s");
            c.p.insert("start", player_world(w));
            keys(w, &[KeyCode::KeyW], true);
        }
        if c.t >= 5.0 {
            keys(w, &[KeyCode::KeyW], false);
            let d = player_world(w).distance(c.p["start"]);
            let (v, steps) = (with_player(w, |p| p.w.vel.length()), w.resource::<WalkStats>().steps);
            end(w, c, format!("walked {d:.2} m, speed {v:.2} m/s, {steps} steps"));
            // Walk speed 5 m/s for 5 s, less the step-off.
            check(c, d > 22.0 && d < 26.0 && d.is_finite(), format!("foreign warp: walked {d:.2} m in 5 s next to a ship at 1e6 m/s (free walk 24.5)"));
            return true;
        }
        false
    }));
    s.push(Box::new(|w, c| {
        let ok = with_player(w, |p| p.w.pos.is_finite() && p.ship.is_none());
        check(c, ok, "foreign warp: walker stays outside, finite".into());
        true
    }));
}

#[derive(Resource)]
struct WarpingProxy {
    proxy: Entity,
    pos: DVec3,
    vel: DVec3,
}

/// Back to its place each tick, with the warp's velocity (the physics step moves it 16.7 km).
fn hold_proxy(w: &mut World) {
    let Some(wp) = w.get_resource::<WarpingProxy>() else { return };
    let (e, pos, vel) = (wp.proxy, wp.pos, wp.vel);
    if let Some(mut p) = w.get_mut::<Position>(e) {
        p.0 = pos;
    }
    if let Some(mut v) = w.get_mut::<LinearVelocity>(e) {
        v.0 = vel;
    }
}
