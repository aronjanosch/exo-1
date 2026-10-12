//! Interaction (#82): one verb (`Tap::Interact`, F) and one prompt. Each step the target is
//! chosen from what the walker looks at: the crate nearest the centre of the view cone, or the
//! seat when the walker stands at it. The HUD shows `Interaction::prompt`; the tap does what it
//! says. Targets: seat, crates, and on a pad the job offered there (#133).
use crate::cargo::{crate_world, Crate, Crates};
use crate::controls::{Actions, Bindings, Input, Tap};
use crate::grab::Grab;
use crate::ship::{Ship, SEAT_POS};
use crate::walker::{cabin_frame, CabinFloor, Player, EYE_HEIGHT};
use avian3d::prelude::*;
use bevy::math::DVec3;
use bevy::prelude::*;
use grab_core::{in_cone, Reach};
use walker_core::Frame;

pub fn plugin(app: &mut App) {
    app.init_resource::<Interaction>();
    app.add_systems(FixedUpdate, interaction.before(crate::walker::walker_step).in_set(crate::phases::Fx::Walker));
}

/// The walker sits down when its feet are this close to the seat (m), as before #82.
pub const SEAT_RANGE: f64 = 1.8;

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Seat,
    /// The seat, but the player has no flight licence: the prompt says where to get one (#169).
    SeatLocked,
    StandUp,
    /// Pick up a crate with the hands or pull it with the grab tool.
    Crate(Entity, Reach),
    /// Set the held crate down (let go).
    Drop(Entity),
    /// Talk to the giver whose counter is at this pad: opens the briefing (#167).
    Counter(jobs_core::GiverId),
    /// Take the job the open counter shows, for the crew.
    Accept(jobs_core::JobId),
}

#[derive(Resource, Default, Debug)]
pub struct Interaction {
    pub target: Option<Target>,
    /// What the HUD shows, e.g. "[F] sit". Empty when there is nothing to do.
    pub prompt: String,
    /// Every target the tap was used on (scenario checks).
    pub used: Vec<Target>,
}

/// Short label of the first keyboard key bound to a tap ("F").
pub fn key_label(bindings: &Bindings, t: Tap) -> String {
    bindings
        .taps
        .iter()
        .filter(|(x, _)| *x == t)
        .flat_map(|(_, ks)| ks.iter())
        .find_map(|i| if let Input::Key(k) = i { Some(format!("{k:?}")) } else { None })
        .map(|k| k.trim_start_matches("Key").trim_start_matches("Digit").to_string())
        .unwrap_or_else(|| "?".into())
}

fn verb(t: &Target, size: &str) -> String {
    match t {
        Target::Seat => "sit".into(),
        Target::SeatLocked => "sit".into(),
        Target::StandUp => "stand up".into(),
        Target::Crate(_, Reach::Hands) => format!("pick up the {size} crate"),
        Target::Crate(_, Reach::Tool) => format!("pull the {size} crate (grab tool)"),
        Target::Drop(_) => format!("set the {size} crate down"),
        Target::Counter(_) => "talk to ".into(),
        Target::Accept(_) => "take the job: ".into(),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn interaction(
    mut commands: Commands,
    mut actions: ResMut<Actions>,
    bindings: Res<Bindings>,
    tuning: Res<crate::tuning::Tuning>,
    table: Res<Crates>,
    mut inter: ResMut<Interaction>,
    mut grab: ResMut<Grab>,
    mut players: Query<&mut Player>,
    mut ships: Query<(Entity, &mut Ship, &Position, &Rotation)>,
    floors: Query<(&ChildOf, &Position, &Rotation, &ColliderTransform), With<CabinFloor>>,
    crates: Query<(Entity, &Crate)>,
    mut gp: ResMut<crate::gameplay::Gameplay>,
) {
    let Ok(mut pl) = players.single_mut() else { return };
    let Some((ship_e, sp, sr)) = ships.iter().next().map(|(e, _, p, r)| (e, *p, *r)) else { return };
    let frame = cabin_frame(ship_e, (&sp, &sr), &floors);
    let cfg = &tuning.grab;
    // A held crate that is gone (despawned) is no longer held.
    if grab.held.is_some_and(|h| crates.get(h.crate_e).is_err()) {
        grab.held = None;
    }
    let size_of = |e: Entity| crates.get(e).map(|(_, c)| table.0.sizes[c.size].name.clone()).unwrap_or_default();

    // In another player's cabin nothing is offered yet (#86).
    let own_frame = match pl.ship {
        None => true,
        Some(e) => e == ship_e,
    };
    // The player's place on foot, for counters: not in a cabin, not in the air.
    let on_foot = pl.ship.is_none() && !pl.seated && !pl.fly;
    let here = pl.world_pos(Frame::IDENTITY);
    // Walking away from the counter closes it.
    if gp.panel.as_ref().is_some_and(|p| !on_foot || gp.counter_at(here).as_ref() != Some(&p.giver)) {
        gp.close_counter();
    }
    if on_foot && gp.panel.is_some() {
        if actions.take_tap(Tap::Decline) {
            gp.close_counter();
        } else if actions.take_tap(Tap::NextOffer) {
            if let Some(p) = gp.panel.as_mut() {
                p.page += 1;
            }
        }
    }
    let panel_offer = if on_foot && grab.held.is_none() { gp.panel_offer() } else { None };
    let target = if let Some(job) = panel_offer {
        Some(Target::Accept(job))
    } else if pl.seated {
        Some(Target::StandUp)
    } else if let Some(h) = grab.held {
        Some(Target::Drop(h.crate_e))
    } else if !own_frame || pl.fly {
        None
    } else {
        let f = if pl.ship.is_some() { frame } else { Frame::IDENTITY };
        let eye = pl.world_pos(f) + pl.world_up(f) * EYE_HEIGHT;
        let look = pl.world_look(f);
        let half = cfg.cone_half_angle_deg.to_radians();
        let mut best: Option<(f64, Entity, Reach)> = None;
        for (e, c) in &crates {
            let (centre, _) = crate_world(c, &frame);
            let Some(dist) = in_cone(eye, look, centre, half, cfg.tool_max_range) else { continue };
            let angle = (centre - eye).angle_between(look);
            let reach = if dist - c.body.half.max_element() <= cfg.hand_range { Reach::Hands } else { Reach::Tool };
            if best.is_none_or(|(a, ..)| angle < a) {
                best = Some((angle, e, reach));
            }
        }
        let at_seat = pl.ship == Some(ship_e) && pl.w.pos.distance(SEAT_POS) < SEAT_RANGE;
        match best {
            // A crate in reach of the hands wins over the seat; the tool's reach does not.
            Some((_, e, Reach::Hands)) => Some(Target::Crate(e, Reach::Hands)),
            _ if at_seat => Some(if gp.may_pilot(crate::gameplay::HOST) { Target::Seat } else { Target::SeatLocked }),
            Some((_, e, r)) => Some(Target::Crate(e, r)),
            None => None,
        }
    };
    // At a giver's counter with something to offer, and nothing else to do: talk to the giver.
    let target = target.or_else(|| {
        let giver = (on_foot && grab.held.is_none()).then(|| gp.counter_at(here)).flatten()?;
        (!gp.offers_of(&giver).is_empty()).then_some(Target::Counter(giver))
    });
    inter.target = target.clone();
    inter.prompt = match &target {
        None => String::new(),
        Some(t) => {
            let size = match t {
                Target::Crate(e, _) | Target::Drop(e) => size_of(*e),
                _ => String::new(),
            };
            let mut p = format!("[{}] {}", key_label(&bindings, Tap::Interact), verb(t, &size));
            match t {
                Target::SeatLocked => p.push_str(&format!(" ({})", gp.licence_hint())),
                Target::Counter(g) => p.push_str(&gp.jobs_content.givers.get(g).map(|g| gp.text(&g.record.name)).unwrap_or_default()),
                Target::Accept(j) => {
                    p.push_str(&gp.offer_label(*j));
                    p.push_str(&format!("   [{}] next  [{}] decline", key_label(&bindings, Tap::NextOffer), key_label(&bindings, Tap::Decline)));
                }
                _ => {}
            }
            if matches!(t, Target::Drop(_)) {
                p.push_str(&format!("  [{}] throw", key_label(&bindings, Tap::Throw)));
            }
            p
        }
    };

    if !actions.take_tap(Tap::Interact) {
        return;
    }
    let Some(t) = target else { return };
    inter.used.push(t.clone());
    let (_, mut ship, ..) = ships.get_mut(ship_e).unwrap();
    match t {
        Target::StandUp => {
            pl.seated = false;
            ship.piloted = false; // hover assist now holds the ship
            pl.w.pos = DVec3::new(0.0, 0.32, SEAT_POS.z + 1.0);
            pl.w.halt();
        }
        Target::Seat => {
            pl.seated = true;
            ship.piloted = true;
            if ship.parked {
                ship.parked = false;
                commands.entity(ship_e).insert(RigidBody::Dynamic);
            }
            pl.w.pos = SEAT_POS - DVec3::new(0.0, 0.3, 0.0);
            pl.w.halt();
        }
        Target::Crate(e, reach) => {
            if let Ok((_, c)) = crates.get(e) {
                let f = if pl.ship.is_some() { frame } else { Frame::IDENTITY };
                let eye = pl.world_pos(f) + pl.world_up(f) * EYE_HEIGHT;
                let fwd = f.rot * pl.w.forward;
                grab.start(cfg, e, c, &table.0.sizes[c.size], reach, eye, fwd, &frame);
            }
        }
        Target::Drop(_) => grab.release(),
        Target::SeatLocked => gp.notify(gameplay_core::notice::Notice::new(gameplay_core::notice::NoticeKind::Warning, "notice.licence.needed")),
        Target::Counter(g) => gp.open_counter(g),
        Target::Accept(j) => {
            gp.push_job(crate::gameplay::HOST, jobs_core::JobEvent::OfferAccepted { job: j });
            gp.close_counter();
        }
    }
}
