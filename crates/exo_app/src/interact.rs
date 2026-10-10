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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Seat,
    StandUp,
    /// Pick up a crate with the hands or pull it with the grab tool.
    Crate(Entity, Reach),
    /// Set the held crate down (let go).
    Drop(Entity),
    /// Take the job offered at this pad for the crew.
    Job(jobs_core::JobId),
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

fn verb(t: Target, size: &str) -> String {
    match t {
        Target::Seat => "sit".into(),
        Target::StandUp => "stand up".into(),
        Target::Crate(_, Reach::Hands) => format!("pick up the {size} crate"),
        Target::Crate(_, Reach::Tool) => format!("pull the {size} crate (grab tool)"),
        Target::Drop(_) => format!("set the {size} crate down"),
        Target::Job(_) => "take the job: ".into(),
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
    let target = if pl.seated {
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
            _ if at_seat => Some(Target::Seat),
            Some((_, e, r)) => Some(Target::Crate(e, r)),
            None => None,
        }
    };
    // On a pad with an offered job, and nothing else to do: take the job.
    let target = target.or_else(|| (pl.ship.is_none() && !pl.seated && !pl.fly && grab.held.is_none()).then(|| gp.offer_at(pl.world_pos(Frame::IDENTITY))).flatten().map(Target::Job));
    inter.target = target;
    inter.prompt = match target {
        None => String::new(),
        Some(t) => {
            let size = match t {
                Target::Crate(e, _) | Target::Drop(e) => size_of(e),
                _ => String::new(),
            };
            let mut p = format!("[{}] {}", key_label(&bindings, Tap::Interact), verb(t, &size));
            if let Target::Job(j) = t {
                p.push_str(&gp.offer_label(j));
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
    inter.used.push(t);
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
        Target::Job(j) => gp.push_job(crate::gameplay::HOST, jobs_core::JobEvent::OfferAccepted { job: j }),
    }
}
