//! The ship ambush (r42, #174 and #255 in one): an enemy comes out of the dark, shoots the shield
//! and the hull; the crew's walker fires GN-01 until it is DESTROYED, or ARMs and JUMPs out on JD-01
//! when the shots run out of time. Driven through `Controls` and the displays, as a crew would.
//! The enemy and its outcome: `dev_area/reactor.rs` (`enemy_outcome`, `ReactorAsk::Ambush`).
//! AMBUSH on the faults screen: `dev_area/reactor_screen.rs` (`ambush`). GN-01 FIRE and CLEAR JAM
//! hold: `gunnery_panel`, `clear_jam_hold` (the hold is `Controls::pointer_down` on the widget, the
//! same press path as a click). JD-01 ARM and JUMP: `jump_panel`.
//!
//! The rules are checked every fixed tick by `ambush_audit` (FixedLast, after the host's steps): the
//! two airlock doors are never open together, the shield and each hull section stay in 0..1, and no
//! number in the reactor news is NaN. The steps only poll for their conditions, each with a limit in
//! simulated seconds (the fixed step), so a busy machine runs the same ticks as a quiet one.
use super::*;
use super::reactor::{click_until, use_display};
use crate::dev_area::{hull::ShipAir, layout::{GUNNERY_AT, JUMP_AT}};
use crate::dev_area::ship::ReactorSim;
use crate::dev_area::reactor_screen::{ambush_running, FAULTS_ID, FAULTS_NAME, GUNNERY_ID, GUNNERY_NAME, JUMP_ID, JUMP_NAME};
use crate::displays::{aim_at, world_point_of, DisplayPanel};
use crate::items::Items;
use bevy::math::DVec3;
use display_core::PanelId;
use item_core::wire::{EnemyOutcome, JumpStage, ReactorNews};

/// The rules, counted over the whole run: each tick with a violation adds one.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct AmbushAudit {
    pub samples: u64,
    pub doors_together: u32,
    pub shield_out: u32,
    pub hull_out: u32,
    pub nan: u32,
}

fn news(w: &World) -> Option<ReactorNews> {
    w.resource::<Items>().reactor
}

/// The reactor news' numbers that are not finite, by name (the enemy's too).
fn nan_fields(r: &ReactorNews) -> Vec<&'static str> {
    let mut bad = Vec::new();
    if !r.temp.is_finite() {
        bad.push("temperature");
    }
    if !r.output_kw.is_finite() {
        bad.push("output");
    }
    if !r.shield.is_finite() {
        bad.push("shield");
    }
    if r.breaches.iter().any(|b| !b.is_finite()) {
        bad.push("breaches");
    }
    if !r.oxygen.is_finite() {
        bad.push("air");
    }
    if let Some(e) = r.enemy {
        if !e.shield.is_finite() || !e.hull.is_finite() {
            bad.push("enemy");
        }
    }
    bad
}

pub fn ambush_audit(air: Option<Res<ShipAir>>, sim: Option<Res<ReactorSim>>, items: Option<Res<Items>>, mut audit: ResMut<AmbushAudit>) {
    audit.samples += 1;
    if let Some(air) = air {
        let n = air.airlock_news();
        if n.inner_open && n.outer_open {
            audit.doors_together += 1;
        }
    }
    if let Some(sim) = sim {
        if !(0.0..=1.0).contains(&sim.room.shield().fraction()) {
            audit.shield_out += 1;
        }
        if sim.room.hull().sections.iter().any(|s| !(0.0..=1.0).contains(&s.damage())) {
            audit.hull_out += 1;
        }
    }
    if items.and_then(|i| i.reactor).is_some_and(|r| !nan_fields(&r).is_empty()) {
        audit.nan += 1;
    }
}

/// The widget's point on a display, if the display is there.
fn widget_at(w: &mut World, id: &PanelId, action: &str) -> Option<DVec3> {
    let d = w.query::<&DisplayPanel>().iter(w).find(|d| &d.id == id).cloned()?;
    world_point_of(w, &d, action, None)
}

/// Aims the pointer at a widget of an open display. False when the widget is not there.
fn aim_widget(w: &mut World, id: &PanelId, action: &str) -> bool {
    match widget_at(w, id, action) {
        Some(at) => {
            aim_at(w, at);
            true
        }
        None => false,
    }
}

/// Polls for a condition, a limit in seconds of the script's clock.
fn poll(what: &'static str, limit: f64, done: fn(&World) -> bool) -> Step {
    Box::new(move |w, c| {
        if c.t == 0.0 {
            begin(w, c, what);
            return false;
        }
        if done(w) {
            end(w, c, format!("{:.1} s", c.t));
            return true;
        }
        if c.t >= limit {
            check(c, false, format!("{what}: not within {limit} s"));
            return true;
        }
        false
    })
}

/// A step that does nothing once the enemy is destroyed (the jump is the fallback to the gun).
fn unless_destroyed(inner: Step) -> Step {
    let mut inner = inner;
    Box::new(move |w, c| {
        if c.v.get("skip").is_some_and(|&k| k > 0.0) {
            return true;
        }
        inner(w, c)
    })
}

/// The gun phase: FIRE at GN-01 every half second while a shot is ready, CLEAR JAM held while the
/// gun is jammed. Ends DESTROYED (sets `skip`, so the jump steps pass) or at 240 s (the jump runs).
fn fire_until_end() -> Step {
    Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "ship ambush: FIRE at GN-01 until the enemy is DESTROYED (240 s)");
            c.v.insert("skip", 0.0);
            c.v.insert("hold", 0.0);
            c.v.insert("next_fire", 0.0);
            return false;
        }
        let r = news(w);
        let outcome = r.and_then(|r| r.enemy).map(|e| e.outcome);
        let release = |w: &mut World, c: &mut Ctx| {
            if c.v.get("hold").is_some_and(|&h| h > 0.0) {
                w.resource_mut::<Controls>().pointer_down = false;
                c.v.insert("hold", 0.0);
            }
        };
        if outcome == Some(EnemyOutcome::Destroyed) {
            release(w, c);
            c.v.insert("skip", 1.0);
            end(w, c, format!("DESTROYED at {:.1} s", c.t));
            return true;
        }
        if c.t >= 240.0 {
            release(w, c);
            end(w, c, format!("not destroyed in 240 s ({outcome:?}): the jump runs"));
            return true;
        }
        let Some(r) = r else { return false };
        if r.weapon_jammed {
            // The press path: the pointer is down on CLEAR JAM, the host counts the hold.
            if aim_widget(w, &GUNNERY_ID, "clear_jam") {
                w.resource_mut::<Controls>().pointer_down = true;
                c.v.insert("hold", 1.0);
            }
        } else {
            release(w, c);
            let ready = r.weapon_shots > 0 && r.weapon_reload_s <= 0.0 && !r.weapon_overheated;
            if ready && c.t >= c.v.get("next_fire").copied().unwrap_or(0.0) && aim_widget(w, &GUNNERY_ID, "fire") {
                w.resource_mut::<Controls>().click = true;
                c.v.insert("next_fire", c.t + 0.5);
            }
        }
        false
    })
}

pub(super) fn ship_ambush_steps(s: &mut Vec<Step>) {
    // The warm reactor on the dev area: its snapshot is there and the reactor is on, not melted.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "ship ambush: the warm reactor is on");
            return false;
        }
        if news(w).is_none() && c.t < 10.0 {
            return false;
        }
        let ok = news(w).is_some_and(|r| r.power_on && !r.melted);
        check(c, ok, format!("ship ambush: the warm reactor is on and not melted ({:?})", news(w).map(|r| (r.power_on, r.melted))));
        end(w, c, format!("{:.1} s", c.t));
        true
    }));

    // AMBUSH on, at the dev faults screen: a new encounter.
    use_display(s, FAULTS_NAME, FAULTS_ID, [-17.5, -12.2]);
    s.push(click_until(FAULTS_ID, "ambush", None, "ship ambush: AMBUSH on starts an encounter", |w| news(w).is_some_and(|r| ambush_running(&r))));

    // The enemy fires: at least two shots within 60 s. Then no NaN, and the shield or a section took damage.
    s.push(Box::new(|w, c| {
        if c.t == 0.0 {
            begin(w, c, "ship ambush: the enemy fires at least twice (60 s)");
            return false;
        }
        let shots = news(w).and_then(|r| r.enemy).map_or(0, |e| e.shots);
        if shots < 2 {
            if c.t < 60.0 {
                return false;
            }
            check(c, false, format!("ship ambush: the enemy fired {shots} times within 60 s"));
            return true;
        }
        let r = news(w).expect("the enemy has shot, so the snapshot is there");
        let bad = nan_fields(&r);
        check(c, bad.is_empty(), format!("ship ambush: no NaN in the reactor news ({bad:?})"));
        let hit = r.shield < 1.0 || r.breaches.iter().any(|&b| b > 0.0);
        check(c, hit, format!("ship ambush: the shield ({:.3}) or a hull section took damage (breaches {:?})", r.shield, r.breaches));
        end(w, c, format!("{shots} shots, shield {:.3}", r.shield));
        true
    }));

    // GN-01: FIRE until DESTROYED (240 s); CLEAR JAM is held when the gun jams.
    // GN-01 is on the bridge's south wall facing north (r44): the walker stands 1 m north of it.
    use_display(s, GUNNERY_NAME, GUNNERY_ID, [GUNNERY_AT[0], GUNNERY_AT[1] + 1.0]);
    s.push(fire_until_end());

    // The fallback, only when the enemy is not destroyed: JD-01, ARM, the charge, JUMP.
    let mut jd = Vec::new();
    // JD-01 is on the bridge's north wall facing south (r44): the walker stands 1.4 m south of it.
    use_display(&mut jd, JUMP_NAME, JUMP_ID, [JUMP_AT[0], JUMP_AT[1] - 1.4]);
    s.extend(jd.into_iter().map(unless_destroyed));
    s.push(unless_destroyed(click_until(JUMP_ID, "jump_arm", None, "ship ambush: ARM the jump drive", |w| news(w).is_some_and(|r| r.jump_stage != JumpStage::ColdStart))));
    s.push(unless_destroyed(poll("ship ambush: the jump charge is READY (240 s)", 240.0, |w| news(w).is_some_and(|r| r.jump_stage == JumpStage::Ready))));
    s.push(unless_destroyed(click_until(JUMP_ID, "jump_go", None, "ship ambush: JUMP at READY", |w| news(w).is_some_and(|r| r.jump_stage == JumpStage::Active))));
    s.push(unless_destroyed(poll("ship ambush: the enemy escapes the jump", 10.0, |w| news(w).and_then(|r| r.enemy).is_some_and(|e| e.outcome == EnemyOutcome::Escaped))));

    // The end: the outcome (DESTROYED, or ESCAPED after the jump), the rule counts of the whole run,
    // and the snapshot against the room (the snapshot may lag the room, never lead it).
    s.push(Box::new(|w, c| {
        begin(w, c, "ship ambush: the outcome, the rules over the run, the snapshot");
        let skipped = c.v.get("skip").is_some_and(|&k| k > 0.0);
        let want = if skipped { EnemyOutcome::Destroyed } else { EnemyOutcome::Escaped };
        let got = news(w).and_then(|r| r.enemy).map(|e| e.outcome);
        check(c, got == Some(want), format!("ship ambush: the outcome is {want:?} (it is {got:?})"));
        let a = *w.resource::<AmbushAudit>();
        check(c, a.samples > 0, format!("ship ambush: the rules were sampled ({} ticks)", a.samples));
        check(c, a.doors_together == 0, format!("ship ambush: the airlock doors were never open together ({} ticks with both)", a.doors_together));
        check(c, a.shield_out == 0, format!("ship ambush: the shield stayed in 0..1 ({} ticks out)", a.shield_out));
        check(c, a.hull_out == 0, format!("ship ambush: every hull section stayed in 0..1 ({} ticks out)", a.hull_out));
        check(c, a.nan == 0, format!("ship ambush: no NaN in the news over the run ({} ticks)", a.nan));
        let sim = w.resource::<ReactorSim>();
        let room = sim.room.encounter().map(|e| (e.shots(), e.outcome()));
        let snap = news(w).and_then(|r| r.enemy);
        let synced = match (snap, room) {
            (Some(s), Some((shots, out))) => s.shots <= shots && (s.outcome == EnemyOutcome::None || s.outcome == crate::dev_area::ship::enemy_outcome(out)),
            _ => true,
        };
        check(c, synced, format!("ship ambush: the snapshot is the room's, or behind it (snapshot {snap:?}, room {room:?})"));
        end(w, c, format!("{} ticks sampled", a.samples));
        true
    }));
}
