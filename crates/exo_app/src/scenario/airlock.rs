//! The airlock (#309): the walker stands in the chamber and presses PUSH on the inside panel, as a crew
//! would. The cycle in (the outer door shuts, the air fills, the inner door opens), then the cycle out
//! (the inner door shuts, the air goes back into the ship, the outer door opens). Every fixed tick
//! `airlock_audit` checks that the doors are never open together, that none is open while a cycle
//! runs, and that the ship's air stays finite and never grows during a cycle. The steps only poll for
//! their conditions, each with a limit in simulated seconds.
use super::*;
use super::reactor::{click_until, use_display};
use crate::dev_area::hull::{panels::{AIRLOCK_IN_ID, AIRLOCK_IN_NAME}, ShipAir};
use crate::dev_area::layout::AIRLOCK;
use crate::items::Items;
use item_core::wire::{AirlockCycle, AirlockNews};

/// The rules, counted over the whole run: each tick with a violation adds one.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct AirlockAudit {
    pub samples: u64,
    pub doors_together: u32,
    pub open_while_cycling: u32,
    pub air_bad: u32,
    /// The most ship air seen (m³): a cycle may only move air, so this stays near the start value.
    pub air_max: f64,
    pub air_start: f64,
}

fn lock(w: &World) -> Option<AirlockNews> {
    w.resource::<Items>().reactor.map(|r| r.airlock)
}

pub fn airlock_audit(air: Option<Res<ShipAir>>, mut audit: ResMut<AirlockAudit>) {
    let Some(air) = air else { return };
    audit.samples += 1;
    let n = air.airlock_news();
    if n.inner_open && n.outer_open {
        audit.doors_together += 1;
    }
    if n.cycling != AirlockCycle::Idle && (n.inner_open || n.outer_open) {
        audit.open_while_cycling += 1;
    }
    // The ship's rooms only: the outside is a source of air, not part of the count.
    let total: f64 = [air.engine, air.corridor, air.equipment, air.airlock, air.bridge].iter().map(|&r| air.deck.room(r).oxygen).sum();
    if !total.is_finite() {
        audit.air_bad += 1;
    }
    if audit.air_start == 0.0 {
        audit.air_start = total;
    }
    audit.air_max = audit.air_max.max(total);
}

/// A step that waits for `done`, a limit in simulated seconds.
fn wait_for(what: &'static str, limit: f64, done: fn(&World) -> bool) -> Step {
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

pub(super) fn airlock_steps(s: &mut Vec<Step>) {
    // The chamber's panel is on its north wall; the walker stands in the middle of the chamber.
    use_display(s, AIRLOCK_IN_NAME, AIRLOCK_IN_ID, [AIRLOCK.centre[0], AIRLOCK.centre[1]]);
    s.push(Box::new(|w, c| {
        begin(w, c, "airlock: the start state is outside, the outer door open and no cycle");
        let n = lock(w);
        check(c, n.is_some_and(|n| !n.inside && n.cycling == AirlockCycle::Idle), format!("airlock: at rest outside ({n:?})"));
        end(w, c, String::new());
        true
    }));

    // In: one push runs it all.
    s.push(click_until(AIRLOCK_IN_ID, "push", None, "airlock: PUSH starts the cycle in", |w| lock(w).is_some_and(|n| n.cycling == AirlockCycle::Inner)));
    s.push(Box::new(|w, c| {
        begin(w, c, "airlock: during the cycle the outer door is shut and the inner one still locked");
        let n = lock(w).unwrap_or_default();
        check(c, !n.outer_open && !n.inner_open, format!("airlock: both doors shut during the cycle ({n:?})"));
        end(w, c, String::new());
        true
    }));
    s.push(wait_for("airlock: the cycle in ends with the inner door open (10 s)", 10.0, |w| lock(w).is_some_and(|n| n.cycling == AirlockCycle::Idle && n.inner_open)));
    s.push(Box::new(|w, c| {
        begin(w, c, "airlock: at rest inside, the outer door locked");
        let n = lock(w).unwrap_or_default();
        check(c, n.inside && n.inner_open && !n.outer_open, format!("airlock: inside, inner open, outer shut ({n:?})"));
        end(w, c, String::new());
        true
    }));

    // Out: the same push from the other side.
    s.push(click_until(AIRLOCK_IN_ID, "push", None, "airlock: PUSH starts the cycle out", |w| lock(w).is_some_and(|n| n.cycling == AirlockCycle::Outer)));
    s.push(wait_for("airlock: the cycle out ends with the outer door open (10 s)", 10.0, |w| lock(w).is_some_and(|n| n.cycling == AirlockCycle::Idle && n.outer_open)));
    s.push(Box::new(|w, c| {
        begin(w, c, "airlock: the rules over the run");
        let n = lock(w).unwrap_or_default();
        check(c, !n.inside && !n.inner_open && n.outer_open, format!("airlock: outside, outer open, inner shut ({n:?})"));
        let a = *w.resource::<AirlockAudit>();
        check(c, a.samples > 0, format!("airlock: the rules were sampled ({} ticks)", a.samples));
        check(c, a.doors_together == 0, format!("airlock: the doors were never open together ({} ticks with both)", a.doors_together));
        check(c, a.open_while_cycling == 0, format!("airlock: no door open during a cycle ({} ticks)", a.open_while_cycling));
        check(c, a.air_bad == 0, format!("airlock: the ship's air stayed finite ({} bad ticks)", a.air_bad));
        check(c, a.air_max <= a.air_start * 1.001 + 0.01, format!("airlock: the cycles made no air (start {:.2}, max {:.2} m3)", a.air_start, a.air_max));
        end(w, c, format!("{} ticks sampled", a.samples));
        true
    }));
}
